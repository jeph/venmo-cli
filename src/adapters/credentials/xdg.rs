#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::fs::{self, DirBuilder, File};
use std::io::{self, Read, Write};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use directories::BaseDirs;
use rustix::fs::{
    AtFlags, FileType, Mode, OFlags, fchmod, fstat, fsync, open, openat, renameat, statat, unlinkat,
};
use rustix::process::geteuid;
use thiserror::Error;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::shared::{
    CredentialBackend, CredentialCapability, CredentialDeleteOutcome, CredentialDeleter,
    CredentialEnvelope, CredentialFailureKind, CredentialReader, CredentialStoreFailure,
    CredentialWriter, LoadedCredential,
};

use super::codec::{self, CredentialCodecError, CredentialCodecErrorKind};

const APPLICATION_DIRECTORY: &str = "venmo-cli";
const CREDENTIAL_FILE: &str = "credential.json";
const TEMP_FILE_PREFIX: &str = ".credential.json.tmp-";
const OWNER_DIRECTORY_MODE: u32 = 0o700;

fn owner_directory_mode() -> Mode {
    Mode::RUSR | Mode::WUSR | Mode::XUSR
}

fn owner_file_mode() -> Mode {
    Mode::RUSR | Mode::WUSR
}

pub(crate) struct XdgCredentialStore {
    directory: PathBuf,
}

impl XdgCredentialStore {
    pub(crate) fn production() -> Result<Self, XdgCredentialStoreError> {
        let base = BaseDirs::new().ok_or(XdgCredentialStoreError::StateDirectoryUnavailable)?;
        let state = base
            .state_dir()
            .ok_or(XdgCredentialStoreError::StateDirectoryUnavailable)?;
        Self::from_state_directory(state)
    }

    fn from_state_directory(state_directory: &Path) -> Result<Self, XdgCredentialStoreError> {
        if !state_directory.is_absolute() {
            return Err(XdgCredentialStoreError::StateDirectoryNotAbsolute);
        }
        Ok(Self {
            directory: state_directory.join(APPLICATION_DIRECTORY),
        })
    }

    #[cfg(test)]
    pub(crate) fn for_test_state_directory(
        state_directory: &Path,
    ) -> Result<Self, XdgCredentialStoreError> {
        Self::from_state_directory(state_directory)
    }

    pub(crate) fn credential_exists(&self) -> Result<bool, XdgCredentialStoreError> {
        let Some(directory) = self.open_directory(false)? else {
            return Ok(false);
        };
        self.entry_stat(&directory).map(|entry| entry.is_some())
    }

    fn open_directory(&self, create: bool) -> Result<Option<File>, XdgCredentialStoreError> {
        if create {
            let mut builder = DirBuilder::new();
            builder.recursive(true).mode(OWNER_DIRECTORY_MODE);
            builder
                .create(&self.directory)
                .map_err(|source| storage_error("create the XDG credential directory", source))?;
        } else {
            match fs::symlink_metadata(&self.directory) {
                Ok(_) => {}
                Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(source) => {
                    return Err(storage_error(
                        "inspect the XDG credential directory",
                        source,
                    ));
                }
            }
        }

        let descriptor = open(
            &self.directory,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|source| {
            storage_error(
                "open the XDG credential directory without following links",
                source.into(),
            )
        })?;
        let stat = fstat(&descriptor).map_err(|source| {
            storage_error("inspect the open XDG credential directory", source.into())
        })?;
        validate_directory_stat(&stat)?;
        Ok(Some(File::from(descriptor)))
    }

    fn entry_stat(
        &self,
        directory: &File,
    ) -> Result<Option<rustix::fs::Stat>, XdgCredentialStoreError> {
        match statat(directory, CREDENTIAL_FILE, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat) => {
                validate_credential_stat(&stat)?;
                Ok(Some(stat))
            }
            Err(source) if source == rustix::io::Errno::NOENT => Ok(None),
            Err(source) => Err(storage_error(
                "inspect the XDG credential file",
                source.into(),
            )),
        }
    }

    fn read(&self) -> Result<Option<LoadedCredential>, XdgCredentialStoreError> {
        let Some(directory) = self.open_directory(false)? else {
            return Ok(None);
        };
        if self.entry_stat(&directory)?.is_none() {
            return Ok(None);
        }

        let descriptor = openat(
            &directory,
            CREDENTIAL_FILE,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|source| {
            storage_error(
                "open the XDG credential file without following links",
                source.into(),
            )
        })?;
        let stat = fstat(&descriptor).map_err(|source| {
            storage_error("inspect the open XDG credential file", source.into())
        })?;
        validate_credential_stat(&stat)?;

        let file = File::from(descriptor);
        let mut raw = Zeroizing::new(String::new());
        let maximum = u64::try_from(codec::MAX_ENCODED_CREDENTIAL_BYTES)
            .map_err(|_| XdgCredentialStoreError::InternalLimit)?;
        file.take(maximum + 1)
            .read_to_string(&mut raw)
            .map_err(|source| {
                if source.kind() == io::ErrorKind::InvalidData {
                    XdgCredentialStoreError::InvalidEncoding
                } else {
                    storage_error("read the XDG credential file", source)
                }
            })?;
        if raw.len() > codec::MAX_ENCODED_CREDENTIAL_BYTES {
            return Err(XdgCredentialStoreError::TooLarge);
        }
        codec::decode(&raw)
            .map(Some)
            .map_err(|source| XdgCredentialStoreError::Codec { source })
    }

    fn save(&self, credential: &CredentialEnvelope) -> Result<(), XdgCredentialStoreError> {
        let encoded = codec::encode(credential)
            .map_err(|source| XdgCredentialStoreError::Codec { source })?;
        let directory = self
            .open_directory(true)?
            .ok_or(XdgCredentialStoreError::StateDirectoryUnavailable)?;
        let _ = self.entry_stat(&directory)?;
        let temporary_name = format!("{TEMP_FILE_PREFIX}{}", Uuid::new_v4());

        let result = (|| {
            let descriptor = openat(
                &directory,
                temporary_name.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                owner_file_mode(),
            )
            .map_err(|source| {
                storage_error("create a temporary XDG credential file", source.into())
            })?;
            fchmod(&descriptor, owner_file_mode()).map_err(|source| {
                storage_error(
                    "set owner-only permissions on a temporary XDG credential file",
                    source.into(),
                )
            })?;
            let stat = fstat(&descriptor).map_err(|source| {
                storage_error("inspect a temporary XDG credential file", source.into())
            })?;
            validate_credential_stat(&stat)?;

            let mut file = File::from(descriptor);
            file.write_all(encoded.as_str().as_bytes())
                .map_err(|source| storage_error("write a temporary XDG credential file", source))?;
            file.sync_all()
                .map_err(|source| storage_error("sync a temporary XDG credential file", source))?;
            drop(file);

            renameat(
                &directory,
                temporary_name.as_str(),
                &directory,
                CREDENTIAL_FILE,
            )
            .map_err(|source| {
                storage_error("atomically replace the XDG credential file", source.into())
            })?;
            fsync(&directory)
                .map_err(|source| storage_error("sync the XDG credential directory", source.into()))
        })();

        if let Err(original) = result {
            match unlinkat(&directory, temporary_name.as_str(), AtFlags::empty()) {
                Ok(()) => fsync(&directory).map_err(|source| {
                    storage_error(
                        "sync the XDG credential directory after temporary-file cleanup",
                        source.into(),
                    )
                })?,
                Err(source) if source == rustix::io::Errno::NOENT => {}
                Err(source) => {
                    return Err(storage_error(
                        "remove a temporary XDG credential file after a failed write",
                        source.into(),
                    ));
                }
            }
            return Err(original);
        }
        Ok(())
    }

    fn delete(&self) -> Result<CredentialDeleteOutcome, XdgCredentialStoreError> {
        let Some(directory) = self.open_directory(false)? else {
            return Ok(CredentialDeleteOutcome::Missing);
        };
        if self.entry_stat(&directory)?.is_none() {
            return Ok(CredentialDeleteOutcome::Missing);
        }
        unlinkat(&directory, CREDENTIAL_FILE, AtFlags::empty())
            .map_err(|source| storage_error("delete the XDG credential file", source.into()))?;
        fsync(&directory)
            .map_err(|source| storage_error("sync the XDG credential directory", source.into()))?;
        Ok(CredentialDeleteOutcome::Deleted)
    }
}

impl CredentialCapability for XdgCredentialStore {
    type Error = XdgCredentialStoreError;
}

impl CredentialReader for XdgCredentialStore {
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        self.read()
    }

    fn credential_backend(&self) -> CredentialBackend {
        CredentialBackend::Xdg
    }
}

impl CredentialWriter for XdgCredentialStore {
    fn save_credential(&self, credential: &CredentialEnvelope) -> Result<(), Self::Error> {
        self.save(credential)
    }
}

impl CredentialDeleter for XdgCredentialStore {
    fn delete_credential(&self) -> Result<CredentialDeleteOutcome, Self::Error> {
        self.delete()
    }
}

fn validate_directory_stat(stat: &rustix::fs::Stat) -> Result<(), XdgCredentialStoreError> {
    if FileType::from_raw_mode(stat.st_mode) != FileType::Directory {
        return Err(XdgCredentialStoreError::InsecureDirectory {
            reason: "it is not a directory",
        });
    }
    if stat.st_uid != geteuid().as_raw() {
        return Err(XdgCredentialStoreError::InsecureDirectory {
            reason: "it is not owned by the current user",
        });
    }
    if !(Mode::from_raw_mode(stat.st_mode) & !owner_directory_mode()).is_empty() {
        return Err(XdgCredentialStoreError::InsecureDirectory {
            reason: "group or other users have access",
        });
    }
    Ok(())
}

fn validate_credential_stat(stat: &rustix::fs::Stat) -> Result<(), XdgCredentialStoreError> {
    if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile {
        return Err(XdgCredentialStoreError::InsecureCredential {
            reason: "it is not a regular file",
        });
    }
    if stat.st_uid != geteuid().as_raw() {
        return Err(XdgCredentialStoreError::InsecureCredential {
            reason: "it is not owned by the current user",
        });
    }
    if stat.st_nlink != 1 {
        return Err(XdgCredentialStoreError::InsecureCredential {
            reason: "it has multiple hard links",
        });
    }
    if !(Mode::from_raw_mode(stat.st_mode) & !owner_file_mode()).is_empty() {
        return Err(XdgCredentialStoreError::InsecureCredential {
            reason: "group or other users have access",
        });
    }
    Ok(())
}

fn storage_error(operation: &'static str, source: io::Error) -> XdgCredentialStoreError {
    XdgCredentialStoreError::Storage { operation, source }
}

#[derive(Debug, Error)]
pub(crate) enum XdgCredentialStoreError {
    #[error("the XDG state directory could not be resolved")]
    StateDirectoryUnavailable,

    #[error("the XDG state directory must be an absolute path")]
    StateDirectoryNotAbsolute,

    #[error("the XDG credential directory is unsafe because {reason}")]
    InsecureDirectory { reason: &'static str },

    #[error("the XDG credential file is unsafe because {reason}")]
    InsecureCredential { reason: &'static str },

    #[error("the XDG credential file is not valid UTF-8")]
    InvalidEncoding,

    #[error("the XDG credential file exceeds the 131072-byte safety limit")]
    TooLarge,

    #[error("the credential size limit could not be represented")]
    InternalLimit,

    #[error("failed to {operation}")]
    Storage {
        operation: &'static str,
        #[source]
        source: io::Error,
    },

    #[error("the stored credential envelope is unusable")]
    Codec {
        #[source]
        source: CredentialCodecError,
    },
}

impl CredentialStoreFailure for XdgCredentialStoreError {
    fn kind(&self) -> CredentialFailureKind {
        match self {
            Self::StateDirectoryUnavailable | Self::StateDirectoryNotAbsolute => {
                CredentialFailureKind::Unavailable
            }
            Self::InsecureDirectory { .. } | Self::InsecureCredential { .. } => {
                CredentialFailureKind::Invalid
            }
            Self::InvalidEncoding => CredentialFailureKind::Corrupt,
            Self::TooLarge => CredentialFailureKind::TooLarge,
            Self::InternalLimit => CredentialFailureKind::Internal,
            Self::Storage { source, .. } if source.kind() == io::ErrorKind::PermissionDenied => {
                CredentialFailureKind::Inaccessible
            }
            Self::Storage { .. } => CredentialFailureKind::Platform,
            Self::Codec { source } => match source.kind() {
                CredentialCodecErrorKind::Corrupt => CredentialFailureKind::Corrupt,
                CredentialCodecErrorKind::Invalid => CredentialFailureKind::Invalid,
                CredentialCodecErrorKind::UnsupportedVersion => {
                    CredentialFailureKind::UnsupportedVersion
                }
                CredentialCodecErrorKind::TooLarge => CredentialFailureKind::TooLarge,
                CredentialCodecErrorKind::Internal => CredentialFailureKind::Internal,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::str::FromStr;

    use time::OffsetDateTime;

    use crate::shared::{AccessToken, CredentialFormat, DeviceId, UserId, Username};

    use super::*;

    type TestResult = Result<(), Box<dyn Error>>;

    struct TemporaryStateDirectory {
        path: PathBuf,
    }

    impl TemporaryStateDirectory {
        fn new() -> io::Result<Self> {
            let path = std::env::temp_dir().join(format!("venmo-cli-xdg-test-{}", Uuid::new_v4()));
            fs::create_dir(&path)?;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
            Ok(Self { path })
        }

        fn store(&self) -> Result<XdgCredentialStore, XdgCredentialStoreError> {
            XdgCredentialStore::for_test_state_directory(&self.path)
        }

        fn application_directory(&self) -> PathBuf {
            self.path.join(APPLICATION_DIRECTORY)
        }

        fn credential_path(&self) -> PathBuf {
            self.application_directory().join(CREDENTIAL_FILE)
        }
    }

    impl Drop for TemporaryStateDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn fixture(token: &str) -> Result<CredentialEnvelope, Box<dyn Error>> {
        Ok(CredentialEnvelope::new(
            AccessToken::from_str(token)?,
            DeviceId::from_str("xdg-test-device")?,
            UserId::from_str("123")?,
            Username::from_bare("xdg-user")?,
            Some("XDG User".to_owned()),
            OffsetDateTime::UNIX_EPOCH,
        ))
    }

    #[test]
    fn xdg_store_round_trips_atomically_with_owner_only_modes_and_idempotent_delete() -> TestResult
    {
        let state = TemporaryStateDirectory::new()?;
        let store = state.store()?;
        assert_eq!(store.credential_backend(), CredentialBackend::Xdg);
        assert!(store.read_credential()?.is_none());

        let first = fixture("first-xdg-token")?;
        store.save_credential(&first)?;
        let second = fixture("second-xdg-token")?;
        store.save_credential(&second)?;

        let loaded = store
            .read_credential()?
            .ok_or_else(|| io::Error::other("saved credential was missing"))?;
        assert_eq!(loaded.format, CredentialFormat::Version1);
        assert!(second.storage_equivalent(&loaded.envelope));
        assert_eq!(
            fs::metadata(state.application_directory())?
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(state.credential_path())?.permissions().mode() & 0o777,
            0o600
        );
        let entries =
            fs::read_dir(state.application_directory())?.collect::<Result<Vec<_>, _>>()?;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].file_name(), CREDENTIAL_FILE);

        assert_eq!(store.delete_credential()?, CredentialDeleteOutcome::Deleted);
        assert_eq!(store.delete_credential()?, CredentialDeleteOutcome::Missing);
        Ok(())
    }

    #[test]
    fn xdg_store_requires_an_absolute_state_directory() {
        assert!(matches!(
            XdgCredentialStore::for_test_state_directory(Path::new("relative-state")),
            Err(XdgCredentialStoreError::StateDirectoryNotAbsolute)
        ));
    }

    #[test]
    fn xdg_store_rejects_broad_directory_permissions() -> TestResult {
        let state = TemporaryStateDirectory::new()?;
        fs::create_dir(state.application_directory())?;
        fs::set_permissions(
            state.application_directory(),
            fs::Permissions::from_mode(0o755),
        )?;

        let error = state
            .store()?
            .read_credential()
            .err()
            .ok_or_else(|| io::Error::other("insecure directory was accepted"))?;
        assert!(matches!(
            error,
            XdgCredentialStoreError::InsecureDirectory { .. }
        ));
        Ok(())
    }

    #[test]
    fn xdg_store_rejects_symlinks_and_hard_links() -> TestResult {
        let state = TemporaryStateDirectory::new()?;
        let store = state.store()?;
        store.save_credential(&fixture("linked-xdg-token")?)?;

        let external = state.path.join("external-credential");
        fs::hard_link(state.credential_path(), &external)?;
        let hard_link_error = store
            .read_credential()
            .err()
            .ok_or_else(|| io::Error::other("hard-linked credential was accepted"))?;
        assert!(matches!(
            hard_link_error,
            XdgCredentialStoreError::InsecureCredential { .. }
        ));

        fs::remove_file(state.credential_path())?;
        symlink(&external, state.credential_path())?;
        let symlink_error = store
            .read_credential()
            .err()
            .ok_or_else(|| io::Error::other("symlinked credential was accepted"))?;
        assert!(matches!(
            symlink_error,
            XdgCredentialStoreError::InsecureCredential { .. }
        ));
        Ok(())
    }

    #[test]
    fn xdg_store_rejects_credential_permissions_visible_to_other_users() -> TestResult {
        let state = TemporaryStateDirectory::new()?;
        let store = state.store()?;
        store.save_credential(&fixture("broad-mode-xdg-token")?)?;
        fs::set_permissions(state.credential_path(), fs::Permissions::from_mode(0o644))?;

        let error = store
            .read_credential()
            .err()
            .ok_or_else(|| io::Error::other("broad credential permissions were accepted"))?;
        assert!(matches!(
            error,
            XdgCredentialStoreError::InsecureCredential { .. }
        ));
        Ok(())
    }

    #[test]
    fn xdg_store_bounds_file_reads_before_decoding() -> TestResult {
        let state = TemporaryStateDirectory::new()?;
        let store = state.store()?;
        store.save_credential(&fixture("bounded-xdg-token")?)?;
        fs::write(
            state.credential_path(),
            vec![b'x'; codec::MAX_ENCODED_CREDENTIAL_BYTES + 1],
        )?;
        fs::set_permissions(state.credential_path(), fs::Permissions::from_mode(0o600))?;

        let error = store
            .read_credential()
            .err()
            .ok_or_else(|| io::Error::other("oversized credential was accepted"))?;
        assert!(matches!(error, XdgCredentialStoreError::TooLarge));
        assert_eq!(error.kind(), CredentialFailureKind::TooLarge);
        Ok(())
    }
}

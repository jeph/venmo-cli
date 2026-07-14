use keyring::{Entry, Error as KeyringError};
use thiserror::Error;
use zeroize::Zeroizing;

use crate::shared::{
    CredentialCapability, CredentialDeleteOutcome, CredentialDeleter, CredentialEnvelope,
    CredentialFailureKind, CredentialReader, CredentialStoreFailure, CredentialWriter,
    LoadedCredential,
};

use super::codec::{self, CredentialCodecError, CredentialCodecErrorKind};

pub const VENMO_CREDENTIAL_SERVICE: &str = "venmo-cli";
pub const VENMO_CREDENTIAL_ACCOUNT: &str = "default";

pub(crate) trait CredentialEntry {
    fn get_password(&self) -> Result<String, KeyringError>;
    fn set_password(&self, password: &str) -> Result<(), KeyringError>;
    fn delete_credential(&self) -> Result<(), KeyringError>;
}

pub(crate) trait CredentialEntryFactory {
    type Entry<'entry>: CredentialEntry
    where
        Self: 'entry;

    fn entry(&self, service: &str, account: &str) -> Result<Self::Entry<'_>, KeyringError>;
}

struct KeyringEntryFactory;

impl CredentialEntryFactory for KeyringEntryFactory {
    type Entry<'entry> = Entry;

    fn entry(&self, service: &str, account: &str) -> Result<Self::Entry<'_>, KeyringError> {
        Entry::new(service, account)
    }
}

impl CredentialEntry for Entry {
    fn get_password(&self) -> Result<String, KeyringError> {
        Entry::get_password(self)
    }

    fn set_password(&self, password: &str) -> Result<(), KeyringError> {
        Entry::set_password(self, password)
    }

    fn delete_credential(&self) -> Result<(), KeyringError> {
        Entry::delete_credential(self)
    }
}

struct CredentialStoreAdapter<F> {
    service: String,
    account: String,
    entry_factory: F,
}

impl<F> CredentialStoreAdapter<F> {
    fn new(service: String, account: String, entry_factory: F) -> Self {
        Self {
            service,
            account,
            entry_factory,
        }
    }
}

impl<F> CredentialStoreAdapter<F>
where
    F: CredentialEntryFactory,
{
    fn entry(&self, operation: &'static str) -> Result<F::Entry<'_>, CredentialStoreError> {
        self.entry_factory
            .entry(&self.service, &self.account)
            .map_err(|source| classify_keyring_error(operation, source))
    }
}

pub struct NativeCredentialStore {
    adapter: CredentialStoreAdapter<KeyringEntryFactory>,
}

impl NativeCredentialStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            adapter: CredentialStoreAdapter::new(
                VENMO_CREDENTIAL_SERVICE.to_owned(),
                VENMO_CREDENTIAL_ACCOUNT.to_owned(),
                KeyringEntryFactory,
            ),
        }
    }

    #[cfg(test)]
    fn for_test_location(service: String, account: String) -> Self {
        Self {
            adapter: CredentialStoreAdapter::new(service, account, KeyringEntryFactory),
        }
    }
}

impl Default for NativeCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialCapability for NativeCredentialStore {
    type Error = CredentialStoreError;
}

impl CredentialReader for NativeCredentialStore {
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        self.adapter.read_credential()
    }
}

impl CredentialWriter for NativeCredentialStore {
    fn save_credential(&self, credential: &CredentialEnvelope) -> Result<(), Self::Error> {
        self.adapter.save_credential(credential)
    }
}

impl CredentialDeleter for NativeCredentialStore {
    fn delete_credential(&self) -> Result<CredentialDeleteOutcome, Self::Error> {
        self.adapter.delete_credential()
    }
}

impl<F> CredentialCapability for CredentialStoreAdapter<F>
where
    F: CredentialEntryFactory,
{
    type Error = CredentialStoreError;
}

impl<F> CredentialReader for CredentialStoreAdapter<F>
where
    F: CredentialEntryFactory,
{
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        let entry = self.entry("read")?;
        let raw = match entry.get_password() {
            Ok(raw) => Zeroizing::new(raw),
            Err(KeyringError::NoEntry) => return Ok(None),
            Err(source) => return Err(classify_keyring_error("read", source)),
        };
        codec::decode(&raw)
            .map(Some)
            .map_err(|source| CredentialStoreError::Codec { source })
    }
}

impl<F> CredentialWriter for CredentialStoreAdapter<F>
where
    F: CredentialEntryFactory,
{
    fn save_credential(&self, credential: &CredentialEnvelope) -> Result<(), Self::Error> {
        let encoded =
            codec::encode(credential).map_err(|source| CredentialStoreError::Codec { source })?;
        let entry = self.entry("save")?;
        entry
            .set_password(encoded.as_str())
            .map_err(|source| classify_keyring_error("save", source))
    }
}

impl<F> CredentialDeleter for CredentialStoreAdapter<F>
where
    F: CredentialEntryFactory,
{
    fn delete_credential(&self) -> Result<CredentialDeleteOutcome, Self::Error> {
        let entry = self.entry("delete")?;
        match entry.delete_credential() {
            Ok(()) => Ok(CredentialDeleteOutcome::Deleted),
            Err(KeyringError::NoEntry) => Ok(CredentialDeleteOutcome::Missing),
            Err(source) => Err(classify_keyring_error("delete", source)),
        }
    }
}

pub type CredentialStoreErrorKind = CredentialFailureKind;

#[derive(Debug, Error)]
pub enum CredentialStoreError {
    #[error("cannot {operation} credentials because the OS credential store is unavailable")]
    Unavailable {
        operation: &'static str,
        #[source]
        source: KeyringError,
    },

    #[error("cannot {operation} credentials because the OS credential entry is corrupt")]
    Corrupt { operation: &'static str },

    #[error("cannot {operation} credentials because a credential-store value is invalid")]
    Invalid {
        operation: &'static str,
        #[source]
        source: KeyringError,
    },

    #[error("cannot {operation} credentials because a credential-store limit was exceeded")]
    TooLarge {
        operation: &'static str,
        #[source]
        source: KeyringError,
    },

    #[error("cannot {operation} credentials because multiple OS credential entries matched")]
    Ambiguous { operation: &'static str },

    #[error("the OS credential store failed while attempting to {operation} credentials")]
    Platform {
        operation: &'static str,
        #[source]
        source: KeyringError,
    },

    #[error("the stored credential envelope is unusable")]
    Codec {
        #[source]
        source: CredentialCodecError,
    },
}

impl CredentialStoreError {
    #[must_use]
    pub const fn kind(&self) -> CredentialStoreErrorKind {
        match self {
            Self::Unavailable { .. } => CredentialStoreErrorKind::Unavailable,
            Self::Corrupt { .. } => CredentialStoreErrorKind::Corrupt,
            Self::Invalid { .. } => CredentialStoreErrorKind::Invalid,
            Self::TooLarge { .. } => CredentialStoreErrorKind::TooLarge,
            Self::Ambiguous { .. } => CredentialStoreErrorKind::Ambiguous,
            Self::Platform { .. } => CredentialStoreErrorKind::Platform,
            Self::Codec { source } => match source.kind() {
                CredentialCodecErrorKind::Corrupt => CredentialStoreErrorKind::Corrupt,
                CredentialCodecErrorKind::Invalid => CredentialStoreErrorKind::Invalid,
                CredentialCodecErrorKind::UnsupportedVersion => {
                    CredentialStoreErrorKind::UnsupportedVersion
                }
                CredentialCodecErrorKind::TooLarge => CredentialStoreErrorKind::TooLarge,
                CredentialCodecErrorKind::Internal => CredentialStoreErrorKind::Internal,
            },
        }
    }
}

impl CredentialStoreFailure for CredentialStoreError {
    fn kind(&self) -> CredentialFailureKind {
        Self::kind(self)
    }
}

fn classify_keyring_error(operation: &'static str, source: KeyringError) -> CredentialStoreError {
    match source {
        source @ (KeyringError::NoStorageAccess(_)
        | KeyringError::NoDefaultStore
        | KeyringError::NotSupportedByStore(_)) => {
            CredentialStoreError::Unavailable { operation, source }
        }
        KeyringError::BadEncoding(_) | KeyringError::BadDataFormat(_, _) => {
            CredentialStoreError::Corrupt { operation }
        }
        source @ KeyringError::Invalid(_, _) => CredentialStoreError::Invalid { operation, source },
        source @ KeyringError::TooLong(_, _) => {
            CredentialStoreError::TooLarge { operation, source }
        }
        KeyringError::Ambiguous(_) => CredentialStoreError::Ambiguous { operation },
        source => CredentialStoreError::Platform { operation, source },
    }
}

#[cfg(test)]
mod tests;

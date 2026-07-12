use keyring::{Entry, Error as KeyringError};
use thiserror::Error;
use zeroize::Zeroizing;

use crate::application::ports::{
    CredentialDeleteOutcome, CredentialFailureKind, CredentialStore, CredentialStoreFailure,
    LoadedCredential,
};

use super::codec::{self, CredentialCodecError, CredentialCodecErrorKind};

pub const VENMO_CREDENTIAL_SERVICE: &str = "venmo-cli";
pub const VENMO_CREDENTIAL_ACCOUNT: &str = "default";

pub struct NativeCredentialStore {
    service: String,
    account: String,
}

impl NativeCredentialStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            service: VENMO_CREDENTIAL_SERVICE.to_owned(),
            account: VENMO_CREDENTIAL_ACCOUNT.to_owned(),
        }
    }

    fn entry(&self, operation: &'static str) -> Result<Entry, CredentialStoreError> {
        Entry::new(&self.service, &self.account)
            .map_err(|source| classify_keyring_error(operation, source))
    }

    #[cfg(test)]
    fn for_test_location(service: String, account: String) -> Self {
        Self { service, account }
    }
}

impl Default for NativeCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialStore for NativeCredentialStore {
    type Error = CredentialStoreError;

    fn load(&self) -> Result<Option<LoadedCredential>, Self::Error> {
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

    fn save(&self, credential: &crate::domain::CredentialEnvelope) -> Result<(), Self::Error> {
        let encoded =
            codec::encode(credential).map_err(|source| CredentialStoreError::Codec { source })?;
        let entry = self.entry("save")?;
        entry
            .set_password(encoded.as_str())
            .map_err(|source| classify_keyring_error("save", source))
    }

    fn delete(&self) -> Result<CredentialDeleteOutcome, Self::Error> {
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
mod tests {
    use std::error::Error;
    use std::io;
    use std::str::FromStr;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::domain::{AccessToken, CredentialEnvelope, DeviceId, UserId, Username};

    use super::*;

    type TestResult = Result<(), Box<dyn Error>>;

    #[test]
    fn keyring_errors_map_by_typed_variant() {
        let unavailable = classify_keyring_error("read", KeyringError::NoDefaultStore);
        assert_eq!(unavailable.kind(), CredentialStoreErrorKind::Unavailable);

        let corrupt = classify_keyring_error("read", KeyringError::BadEncoding(vec![0xff]));
        assert_eq!(corrupt.kind(), CredentialStoreErrorKind::Corrupt);

        let ambiguous = classify_keyring_error("read", KeyringError::Ambiguous(Vec::new()));
        assert_eq!(ambiguous.kind(), CredentialStoreErrorKind::Ambiguous);
    }

    #[test]
    #[ignore = "manually exercises the native OS credential store"]
    fn native_keyring_round_trip_uses_an_isolated_entry() -> TestResult {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let service = format!("venmo-cli-native-smoke-{}-{nonce}", std::process::id());
        let store = NativeCredentialStore::for_test_location(service, "synthetic".to_owned());
        let credential = CredentialEnvelope::new(
            AccessToken::from_normalized_owned("synthetic-token".to_owned())?,
            DeviceId::from_owned("synthetic-device".to_owned())?,
            UserId::from_str("123")?,
            Username::from_bare("synthetic-user")?,
            Some("Synthetic User".to_owned()),
            time::OffsetDateTime::UNIX_EPOCH,
        );

        let exercise_result = exercise_native_round_trip(&store, &credential);
        let cleanup_result = store.delete();

        exercise_result?;
        match cleanup_result? {
            CredentialDeleteOutcome::Deleted => {}
            CredentialDeleteOutcome::Missing => {
                return Err(io::Error::other(
                    "native keyring test entry disappeared before cleanup",
                )
                .into());
            }
        }
        if store.load()?.is_some() {
            return Err(
                io::Error::other("native keyring test entry remained after deletion").into(),
            );
        }
        Ok(())
    }

    fn exercise_native_round_trip(
        store: &NativeCredentialStore,
        credential: &CredentialEnvelope,
    ) -> TestResult {
        store.save(credential)?;
        let Some(loaded) = store.load()? else {
            return Err(io::Error::other("native keyring test entry was not readable").into());
        };
        if loaded.format != crate::application::ports::CredentialFormat::Version1 {
            return Err(io::Error::other("native keyring test entry used the wrong format").into());
        }
        if loaded.envelope.access_token().expose_secret() != "synthetic-token" {
            return Err(io::Error::other("native keyring returned a different secret").into());
        }
        if loaded.envelope.user_id().as_str() != "123" {
            return Err(io::Error::other("native keyring returned different metadata").into());
        }
        Ok(())
    }
}

use std::error::Error;

use thiserror::Error;

use super::{CredentialEnvelope, OperationFailure};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialFormat {
    Version1,
    LegacyTypeScript,
}

#[derive(Debug)]
pub struct LoadedCredential {
    pub envelope: CredentialEnvelope,
    pub format: CredentialFormat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialDeleteOutcome {
    Deleted,
    Missing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialFailureKind {
    Unavailable,
    Corrupt,
    Invalid,
    UnsupportedVersion,
    TooLarge,
    Ambiguous,
    Platform,
    Internal,
}

impl CredentialFailureKind {
    #[must_use]
    pub const fn permits_validated_replacement(self) -> bool {
        matches!(
            self,
            Self::Corrupt | Self::Invalid | Self::UnsupportedVersion | Self::TooLarge
        )
    }
}

pub trait CredentialStoreFailure: Error + Send + Sync + 'static {
    fn kind(&self) -> CredentialFailureKind;
}

pub trait CredentialCapability {
    type Error: CredentialStoreFailure;
}

pub trait CredentialReader: CredentialCapability {
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error>;
}

pub trait CredentialWriter: CredentialCapability {
    fn save_credential(&self, credential: &CredentialEnvelope) -> Result<(), Self::Error>;
}

pub trait CredentialDeleter: CredentialCapability {
    fn delete_credential(&self) -> Result<CredentialDeleteOutcome, Self::Error>;
}

#[derive(Debug, Error)]
pub enum CredentialAccessError {
    #[error("no Venmo credential is stored; run `venmo auth login`")]
    Missing,

    #[error("failed to read the OS credential entry: {source}")]
    Read {
        #[source]
        source: OperationFailure,
    },
}

pub fn require_credential<R: CredentialReader>(
    credentials: &R,
) -> Result<LoadedCredential, CredentialAccessError> {
    credentials
        .read_credential()
        .map_err(|source| CredentialAccessError::Read {
            source: OperationFailure::new(source),
        })?
        .ok_or(CredentialAccessError::Missing)
}

#[cfg(test)]
mod tests {
    use std::error::Error as StdError;
    use std::fmt;
    use std::io;

    use super::*;

    struct MissingReader;

    impl CredentialCapability for MissingReader {
        type Error = SyntheticCredentialError;
    }

    impl CredentialReader for MissingReader {
        fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
            Ok(None)
        }
    }

    struct FailingReader;

    impl CredentialCapability for FailingReader {
        type Error = SyntheticCredentialError;
    }

    impl CredentialReader for FailingReader {
        fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
            Err(SyntheticCredentialError)
        }
    }

    struct WriterOnly;

    impl CredentialCapability for WriterOnly {
        type Error = SyntheticCredentialError;
    }

    impl CredentialWriter for WriterOnly {
        fn save_credential(&self, _credential: &CredentialEnvelope) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    struct DeleterOnly;

    impl CredentialCapability for DeleterOnly {
        type Error = SyntheticCredentialError;
    }

    impl CredentialDeleter for DeleterOnly {
        fn delete_credential(&self) -> Result<CredentialDeleteOutcome, Self::Error> {
            Ok(CredentialDeleteOutcome::Missing)
        }
    }

    struct SyntheticCredentialError;

    impl fmt::Debug for SyntheticCredentialError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("sensitive-credential-detail")
        }
    }

    impl fmt::Display for SyntheticCredentialError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("synthetic credential failure")
        }
    }

    impl Error for SyntheticCredentialError {}

    impl CredentialStoreFailure for SyntheticCredentialError {
        fn kind(&self) -> CredentialFailureKind {
            CredentialFailureKind::Internal
        }
    }

    #[test]
    fn credential_capabilities_are_independently_implementable() {
        fn accepts_reader(_reader: &impl CredentialReader) {}
        fn accepts_writer(_writer: &impl CredentialWriter) {}
        fn accepts_deleter(_deleter: &impl CredentialDeleter) {}

        accepts_reader(&MissingReader);
        accepts_writer(&WriterOnly);
        accepts_deleter(&DeleterOnly);
    }

    #[test]
    fn required_credentials_distinguish_missing_from_redacted_read_failure()
    -> Result<(), Box<dyn StdError>> {
        let Err(missing) = require_credential(&MissingReader) else {
            return Err(io::Error::other("credential should be missing").into());
        };
        assert!(matches!(missing, CredentialAccessError::Missing));
        assert_eq!(
            missing.to_string(),
            "no Venmo credential is stored; run `venmo auth login`"
        );

        let Err(read) = require_credential(&FailingReader) else {
            return Err(io::Error::other("credential read should fail").into());
        };
        assert!(matches!(read, CredentialAccessError::Read { .. }));
        assert_eq!(
            read.to_string(),
            "failed to read the OS credential entry: synthetic credential failure"
        );
        assert!(!format!("{read:?}").contains("sensitive-credential-detail"));
        Ok(())
    }
}

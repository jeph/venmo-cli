use thiserror::Error;

use crate::shared::{ApiOperationFailure, CredentialAccessError, ReadFailureKind};

#[derive(Debug, Error)]
pub enum ActivityError {
    #[error(transparent)]
    Credential(#[from] CredentialAccessError),

    #[error("failed to read Venmo activity: {source}")]
    Api {
        #[source]
        source: ApiOperationFailure,
    },

    #[error("the Venmo activity response violates its contract because {problem}")]
    ResponseContract { problem: &'static str },
}

impl ActivityError {
    #[must_use]
    pub const fn failure_kind(&self) -> ReadFailureKind {
        match self {
            Self::Credential(_) => ReadFailureKind::Credential,
            Self::Api { source } => ReadFailureKind::Api(source.kind()),
            Self::ResponseContract { .. } => ReadFailureKind::ResponseContract,
        }
    }
}

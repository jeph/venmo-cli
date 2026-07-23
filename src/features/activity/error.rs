use thiserror::Error;

use crate::features::people::lookup::UserLookupError;
use crate::shared::{ApiOperationFailure, ApplicationFailureKind, CredentialAccessError};

#[derive(Debug, Error)]
pub enum ActivityError {
    #[error(transparent)]
    Credential(#[from] CredentialAccessError),

    #[error("failed to resolve the activity subject: {source}")]
    UserLookup {
        #[source]
        source: UserLookupError,
    },

    #[error("the selected activity subject did not provide a profile type")]
    MissingProfileType,

    #[error("activity listing currently supports only personal Venmo profiles")]
    UnsupportedProfileType,

    #[error("failed to read Venmo activity: {source}")]
    Api {
        #[source]
        source: ApiOperationFailure,
    },

    #[error("the Venmo activity response violates its contract because {problem}")]
    ResponseContract { problem: &'static str },

    #[error("Venmo did not provide a comment collection for this activity")]
    CommentsUnavailable,

    #[error(
        "Venmo provided only a partial comment collection, so the CLI cannot safely paginate it"
    )]
    CommentsIncomplete,

    #[error("Venmo did not provide reaction data for this activity")]
    ReactionsUnavailable,
}

impl ActivityError {
    #[must_use]
    pub const fn failure_kind(&self) -> ApplicationFailureKind {
        match self {
            Self::Credential(_) => ApplicationFailureKind::Credential,
            Self::UserLookup { source } => source.application_failure_kind(),
            Self::MissingProfileType | Self::UnsupportedProfileType => {
                ApplicationFailureKind::Usage
            }
            Self::Api { source } => ApplicationFailureKind::Api(source.kind()),
            Self::ResponseContract { .. }
            | Self::CommentsUnavailable
            | Self::CommentsIncomplete
            | Self::ReactionsUnavailable => ApplicationFailureKind::ApiContract,
        }
    }
}

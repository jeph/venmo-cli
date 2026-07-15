use thiserror::Error;

use super::{User, UserLookupApi};
use crate::shared::{
    ApiOperationFailure, CredentialAccessError, CredentialReader, ReadFailureKind, UserId,
    require_credential,
};

#[derive(Debug, Eq, PartialEq)]
pub struct UserInfoResult {
    user: User,
}

impl UserInfoResult {
    #[must_use]
    pub(crate) const fn new(user: User) -> Self {
        Self { user }
    }

    #[must_use]
    pub const fn user(&self) -> &User {
        &self.user
    }
}

#[derive(Debug, Error)]
pub enum UserInfoError {
    #[error(transparent)]
    Credential(#[from] CredentialAccessError),

    #[error("failed to read Venmo user information: {source}")]
    Api {
        #[source]
        source: ApiOperationFailure,
    },

    #[error("the Venmo user response violates its contract because {problem}")]
    ResponseContract { problem: &'static str },
}

impl UserInfoError {
    #[must_use]
    pub const fn failure_kind(&self) -> ReadFailureKind {
        match self {
            Self::Credential(_) => ReadFailureKind::Credential,
            Self::Api { source } => ReadFailureKind::Api(source.kind()),
            Self::ResponseContract { .. } => ReadFailureKind::ResponseContract,
        }
    }
}

pub async fn info<R, A>(
    credentials: &R,
    api: &A,
    user_id: &UserId,
) -> Result<UserInfoResult, UserInfoError>
where
    R: CredentialReader,
    A: UserLookupApi,
{
    let loaded = require_credential(credentials)?;
    let user = api
        .user_by_id(
            loaded.envelope.access_token(),
            loaded.envelope.device_id(),
            user_id,
        )
        .await
        .map_err(|source| UserInfoError::Api {
            source: ApiOperationFailure::new(source),
        })?;
    if user.user_id() != user_id {
        return Err(UserInfoError::ResponseContract {
            problem: "the API returned a different user ID",
        });
    }
    Ok(UserInfoResult::new(user))
}

#[cfg(test)]
#[path = "info_tests.rs"]
mod tests;

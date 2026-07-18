use thiserror::Error;

use super::lookup::{self, UserLookupError};
use super::{User, UserLookupApi, UserSearchApi};
use crate::shared::{
    ApiOperationFailure, ApplicationFailureKind, CredentialAccessError, CredentialReader, Username,
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

    #[error(transparent)]
    Lookup {
        #[from]
        source: UserLookupError,
    },
}

impl UserInfoError {
    #[must_use]
    pub const fn failure_kind(&self) -> ApplicationFailureKind {
        match self {
            Self::Credential(_) => ApplicationFailureKind::Credential,
            Self::Api { source } => ApplicationFailureKind::Api(source.kind()),
            Self::ResponseContract { .. } => ApplicationFailureKind::ApiContract,
            Self::Lookup { source } => source.application_failure_kind(),
        }
    }
}

pub async fn info<R, A>(
    credentials: &R,
    api: &A,
    username: &Username,
) -> Result<UserInfoResult, UserInfoError>
where
    R: CredentialReader,
    A: UserLookupApi + UserSearchApi,
{
    let loaded = require_credential(credentials)?;
    let user = lookup::resolve_with_credential(&loaded.envelope, api, username)
        .await
        .map_err(map_lookup_error)?;
    Ok(UserInfoResult::new(user))
}

fn map_lookup_error(source: UserLookupError) -> UserInfoError {
    match source {
        UserLookupError::Api { source } => UserInfoError::Api { source },
        UserLookupError::LookupContract { problem } => UserInfoError::ResponseContract { problem },
        source => UserInfoError::Lookup { source },
    }
}

#[cfg(test)]
#[path = "info_tests.rs"]
mod tests;

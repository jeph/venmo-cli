use std::str::FromStr;

use thiserror::Error;

use super::users::{UserSearchError, UserSearchFailureKind, search_exhaustively_with_credential};
use super::{User, UserLookupApi, UserSearchApi, UserSearchQuery};
use crate::shared::{
    ApiFailureKind, ApiOperationFailure, ApplicationFailureKind, CredentialEnvelope, Username,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserLookupFailureKind {
    Credential,
    Api(ApiFailureKind),
    ApiContract,
    NotFound,
    Ambiguous,
    Internal,
}

#[derive(Debug, Error)]
pub enum UserLookupError {
    #[error("failed to look up the Venmo user: {source}")]
    Api {
        #[source]
        source: ApiOperationFailure,
    },

    #[error("cannot use the user lookup result because {problem}")]
    LookupContract { problem: &'static str },

    #[error("failed to complete the exact Venmo username search: {source}")]
    UsernameSearch {
        kind: UserLookupFailureKind,
        #[source]
        source: UserSearchError,
    },

    #[error("no Venmo user has that exact username")]
    UsernameNotFound,

    #[error("multiple Venmo users matched that exact username")]
    AmbiguousUsername,

    #[error("user resolution failed because {problem}")]
    Internal { problem: &'static str },
}

impl UserLookupError {
    #[must_use]
    pub const fn failure_kind(&self) -> UserLookupFailureKind {
        match self {
            Self::Api { source } => UserLookupFailureKind::Api(source.kind()),
            Self::LookupContract { .. } => UserLookupFailureKind::ApiContract,
            Self::UsernameSearch { kind, .. } => *kind,
            Self::UsernameNotFound => UserLookupFailureKind::NotFound,
            Self::AmbiguousUsername => UserLookupFailureKind::Ambiguous,
            Self::Internal { .. } => UserLookupFailureKind::Internal,
        }
    }

    #[must_use]
    pub const fn application_failure_kind(&self) -> ApplicationFailureKind {
        match self.failure_kind() {
            UserLookupFailureKind::Credential => ApplicationFailureKind::Credential,
            UserLookupFailureKind::Api(kind) => ApplicationFailureKind::Api(kind),
            UserLookupFailureKind::ApiContract => ApplicationFailureKind::ApiContract,
            UserLookupFailureKind::NotFound | UserLookupFailureKind::Ambiguous => {
                ApplicationFailureKind::Usage
            }
            UserLookupFailureKind::Internal => ApplicationFailureKind::Internal,
        }
    }
}

pub(crate) async fn resolve_with_credential<A>(
    credential: &CredentialEnvelope,
    api: &A,
    username: &Username,
) -> Result<User, UserLookupError>
where
    A: UserLookupApi + UserSearchApi,
{
    resolve_exact_username(credential, api, username).await
}

async fn resolve_exact_username<A>(
    credential: &CredentialEnvelope,
    api: &A,
    username: &Username,
) -> Result<User, UserLookupError>
where
    A: UserLookupApi + UserSearchApi,
{
    let query = UserSearchQuery::from_str(&username.to_string()).map_err(|_| {
        UserLookupError::Internal {
            problem: "a validated username could not form an exact-search query",
        }
    })?;
    let result = search_exhaustively_with_credential(credential, api, &query)
        .await
        .map_err(|source| UserLookupError::UsernameSearch {
            kind: map_search_failure(source.failure_kind()),
            source,
        })?;

    let mut matches = result.users().iter().filter(|user| {
        user.username()
            .is_some_and(|candidate| username_matches(candidate, username))
    });
    let search_match = match (matches.next(), matches.next()) {
        (Some(_), Some(_)) => return Err(UserLookupError::AmbiguousUsername),
        (Some(search_match), None) => search_match,
        (None, _) => return Err(UserLookupError::UsernameNotFound),
    };
    let expected_user_id = search_match.user_id().clone();
    let user = api
        .user_by_id(
            credential.access_token(),
            credential.device_id(),
            &expected_user_id,
        )
        .await
        .map_err(|source| UserLookupError::Api {
            source: ApiOperationFailure::new(source),
        })?;
    if user.user_id() != &expected_user_id {
        return Err(UserLookupError::LookupContract {
            problem: "the user detail response returned a different user ID",
        });
    }
    if !user
        .username()
        .is_some_and(|candidate| username_matches(candidate, username))
    {
        return Err(UserLookupError::LookupContract {
            problem: "the user detail response returned a different username",
        });
    }
    Ok(user)
}

fn username_matches(candidate: &Username, requested: &Username) -> bool {
    candidate.as_str().to_lowercase() == requested.as_str().to_lowercase()
}

const fn map_search_failure(kind: UserSearchFailureKind) -> UserLookupFailureKind {
    match kind {
        UserSearchFailureKind::Credential => UserLookupFailureKind::Credential,
        UserSearchFailureKind::Api(kind) => UserLookupFailureKind::Api(kind),
        UserSearchFailureKind::ResponseContract => UserLookupFailureKind::ApiContract,
        UserSearchFailureKind::Internal => UserLookupFailureKind::Internal,
    }
}

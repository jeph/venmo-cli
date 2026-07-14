use std::str::FromStr;

use thiserror::Error;

use super::users::{
    ExhaustiveSearchCompletion, UserSearchError, UserSearchFailureKind,
    search_exhaustively_with_credential,
};
use super::{
    RecipientInput, ResolvedRecipient, User, UserLookupApi, UserSearchApi, UserSearchQuery,
};
use crate::shared::{
    ApiFailureKind, ApiOperationFailure, ApplicationFailureKind, CredentialEnvelope, Username,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecipientResolutionFailureKind {
    Credential,
    Api(ApiFailureKind),
    ApiContract,
    NotFound,
    Ambiguous,
    IncompleteSearch,
    Internal,
}

#[derive(Debug, Error)]
pub enum RecipientResolutionError {
    #[error("failed to look up the Venmo recipient: {source}")]
    Api {
        #[source]
        source: ApiOperationFailure,
    },

    #[error("cannot use the recipient lookup result because {problem}")]
    LookupContract { problem: &'static str },

    #[error("failed to complete the exact Venmo username search: {source}")]
    UsernameSearch {
        kind: RecipientResolutionFailureKind,
        #[source]
        source: UserSearchError,
    },

    #[error("no Venmo user has that exact username")]
    UsernameNotFound,

    #[error("multiple Venmo users matched that exact username")]
    AmbiguousUsername,

    #[error(
        "the bounded username search was incomplete, so the recipient cannot be resolved safely; use a numeric Venmo user ID"
    )]
    IncompleteUsernameSearch,

    #[error("recipient resolution failed because {problem}")]
    Internal { problem: &'static str },
}

impl RecipientResolutionError {
    #[must_use]
    pub const fn failure_kind(&self) -> RecipientResolutionFailureKind {
        match self {
            Self::Api { source } => RecipientResolutionFailureKind::Api(source.kind()),
            Self::LookupContract { .. } => RecipientResolutionFailureKind::ApiContract,
            Self::UsernameSearch { kind, .. } => *kind,
            Self::UsernameNotFound => RecipientResolutionFailureKind::NotFound,
            Self::AmbiguousUsername => RecipientResolutionFailureKind::Ambiguous,
            Self::IncompleteUsernameSearch => RecipientResolutionFailureKind::IncompleteSearch,
            Self::Internal { .. } => RecipientResolutionFailureKind::Internal,
        }
    }

    #[must_use]
    pub const fn application_failure_kind(&self) -> ApplicationFailureKind {
        match self.failure_kind() {
            RecipientResolutionFailureKind::Credential => ApplicationFailureKind::Credential,
            RecipientResolutionFailureKind::Api(kind) => ApplicationFailureKind::Api(kind),
            RecipientResolutionFailureKind::ApiContract => ApplicationFailureKind::ApiContract,
            RecipientResolutionFailureKind::NotFound
            | RecipientResolutionFailureKind::Ambiguous
            | RecipientResolutionFailureKind::IncompleteSearch => ApplicationFailureKind::Usage,
            RecipientResolutionFailureKind::Internal => ApplicationFailureKind::Internal,
        }
    }
}

pub(crate) async fn resolve_with_credential<A>(
    credential: &CredentialEnvelope,
    api: &A,
    recipient: &RecipientInput,
) -> Result<ResolvedRecipient, RecipientResolutionError>
where
    A: UserLookupApi + UserSearchApi,
{
    let user = match recipient {
        RecipientInput::UserId(user_id) => {
            let user = api
                .user_by_id(credential.access_token(), credential.device_id(), user_id)
                .await
                .map_err(|source| RecipientResolutionError::Api {
                    source: ApiOperationFailure::new(source),
                })?;
            if user.user_id() != user_id {
                return Err(RecipientResolutionError::LookupContract {
                    problem: "the API returned a different user ID",
                });
            }
            user
        }
        RecipientInput::Username(username) => {
            resolve_exact_username(credential, api, username).await?
        }
    };
    Ok(ResolvedRecipient::new(user))
}

async fn resolve_exact_username<A>(
    credential: &CredentialEnvelope,
    api: &A,
    username: &Username,
) -> Result<User, RecipientResolutionError>
where
    A: UserLookupApi + UserSearchApi,
{
    let query = UserSearchQuery::from_str(&username.to_string()).map_err(|_| {
        RecipientResolutionError::Internal {
            problem: "a validated username could not form an exact-search query",
        }
    })?;
    let result = search_exhaustively_with_credential(credential, api, &query)
        .await
        .map_err(|source| RecipientResolutionError::UsernameSearch {
            kind: map_search_failure(source.failure_kind()),
            source,
        })?;

    let mut matches = result.users().iter().filter(|user| {
        user.username()
            .is_some_and(|candidate| username_matches(candidate, username))
    });
    let search_match = match (matches.next(), matches.next()) {
        (Some(_), Some(_)) => return Err(RecipientResolutionError::AmbiguousUsername),
        (Some(search_match), None) => search_match,
        (None, _) => {
            return if result.completion() == ExhaustiveSearchCompletion::Exhausted {
                Err(RecipientResolutionError::UsernameNotFound)
            } else {
                Err(RecipientResolutionError::IncompleteUsernameSearch)
            };
        }
    };
    let expected_user_id = search_match.user_id().clone();
    let user = api
        .user_by_id(
            credential.access_token(),
            credential.device_id(),
            &expected_user_id,
        )
        .await
        .map_err(|source| RecipientResolutionError::Api {
            source: ApiOperationFailure::new(source),
        })?;
    if user.user_id() != &expected_user_id {
        return Err(RecipientResolutionError::LookupContract {
            problem: "the recipient detail response returned a different user ID",
        });
    }
    if !user
        .username()
        .is_some_and(|candidate| username_matches(candidate, username))
    {
        return Err(RecipientResolutionError::LookupContract {
            problem: "the recipient detail response returned a different username",
        });
    }
    Ok(user)
}

fn username_matches(candidate: &Username, requested: &Username) -> bool {
    candidate.as_str().to_lowercase() == requested.as_str().to_lowercase()
}

const fn map_search_failure(kind: UserSearchFailureKind) -> RecipientResolutionFailureKind {
    match kind {
        UserSearchFailureKind::Credential => RecipientResolutionFailureKind::Credential,
        UserSearchFailureKind::Api(kind) => RecipientResolutionFailureKind::Api(kind),
        UserSearchFailureKind::ResponseContract => RecipientResolutionFailureKind::ApiContract,
        UserSearchFailureKind::Internal => RecipientResolutionFailureKind::Internal,
    }
}

#[cfg(test)]
#[path = "recipients_tests.rs"]
mod tests;

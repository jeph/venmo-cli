use thiserror::Error;

use super::{User, UserSearchApi, UserSearchPage, UserSearchPageRequest, UserSearchQuery};
use crate::shared::{
    ApiFailureKind, ApiOperationFailure, CredentialAccessError, CredentialEnvelope,
    CredentialReader, FirstSeen, FirstSeenResult, Limit, Offset, UserId, require_credential,
};

const EXHAUSTIVE_PAGE_SIZE: u32 = 50;
const MAX_EXHAUSTIVE_PAGE_REQUESTS: u8 = 4;
const MAX_EXHAUSTIVE_USER_SEARCH_RESULTS: u32 = 200;

#[derive(Debug, Eq, PartialEq)]
pub struct UserSearchResult {
    users: Vec<User>,
    next_offset: Option<Offset>,
}

impl UserSearchResult {
    #[must_use]
    pub(crate) fn new(users: Vec<User>, next_offset: Option<Offset>) -> Self {
        Self { users, next_offset }
    }

    #[must_use]
    pub fn users(&self) -> &[User] {
        &self.users
    }

    #[must_use]
    pub const fn next_offset(&self) -> Option<Offset> {
        self.next_offset
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ExhaustiveUserSearchResult {
    users: Vec<User>,
}

impl ExhaustiveUserSearchResult {
    pub(crate) fn users(&self) -> &[User] {
        &self.users
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserSearchFailureKind {
    Credential,
    Api(ApiFailureKind),
    ResponseContract,
    Internal,
}

#[derive(Debug, Error)]
pub enum UserSearchError {
    #[error(transparent)]
    Credential(#[from] CredentialAccessError),

    #[error("failed to search Venmo users: {source}")]
    Api {
        #[source]
        source: ApiOperationFailure,
    },

    #[error("the Venmo user-search response violates its contract because {problem}")]
    ResponseContract { problem: &'static str },

    #[error("user-search processing failed because {problem}")]
    Internal { problem: &'static str },
}

impl UserSearchError {
    #[must_use]
    pub const fn failure_kind(&self) -> UserSearchFailureKind {
        match self {
            Self::Credential(_) => UserSearchFailureKind::Credential,
            Self::Api { source } => UserSearchFailureKind::Api(source.kind()),
            Self::ResponseContract { .. } => UserSearchFailureKind::ResponseContract,
            Self::Internal { .. } => UserSearchFailureKind::Internal,
        }
    }
}

pub async fn search<R, A>(
    credentials: &R,
    api: &A,
    query: &UserSearchQuery,
    limit: Limit,
    offset: Offset,
) -> Result<UserSearchResult, UserSearchError>
where
    R: CredentialReader,
    A: UserSearchApi,
{
    let loaded = require_credential(credentials)?;
    let page = request_page(&loaded.envelope, api, query, limit, offset).await?;
    let (page_users, next_token) = page.into_parts();
    validate_page_len(page_users.len(), limit)?;
    validate_next_offset(offset, next_token)?;
    if page_users.is_empty() && next_token.is_some() {
        return Err(UserSearchError::ResponseContract {
            problem: "the API returned an empty page with a continuation offset",
        });
    }
    let users = buffer_public_page(page_users)?;
    Ok(UserSearchResult::new(users, next_token))
}

pub(crate) async fn search_exhaustively_with_credential<A>(
    credential: &CredentialEnvelope,
    api: &A,
    query: &UserSearchQuery,
) -> Result<ExhaustiveUserSearchResult, UserSearchError>
where
    A: UserSearchApi,
{
    let capacity = usize::try_from(MAX_EXHAUSTIVE_USER_SEARCH_RESULTS).map_err(|_| {
        UserSearchError::Internal {
            problem: "the exhaustive user-search bound did not fit in memory addressing",
        }
    })?;
    let mut users = Vec::with_capacity(capacity);
    let mut seen = FirstSeen::<UserId>::with_capacity(capacity);
    let mut current_offset = Offset::default();

    for _ in 0..MAX_EXHAUSTIVE_PAGE_REQUESTS {
        let collected = u32::try_from(users.len()).map_err(|_| UserSearchError::Internal {
            problem: "the collected user count exceeded its bounded representation",
        })?;
        let page_size = MAX_EXHAUSTIVE_USER_SEARCH_RESULTS
            .saturating_sub(collected)
            .min(EXHAUSTIVE_PAGE_SIZE);
        let page_size = Limit::try_from(page_size).map_err(|_| UserSearchError::Internal {
            problem: "the exhaustive user search attempted a zero-sized page",
        })?;
        let page = request_page(credential, api, query, page_size, current_offset).await?;
        let (page_users, next_token) = page.into_parts();
        validate_page_len(page_users.len(), page_size)?;
        validate_next_offset(current_offset, next_token)?;
        if page_users.is_empty() {
            if next_token.is_some() {
                return Err(UserSearchError::ResponseContract {
                    problem: "the API returned an empty page with a continuation offset",
                });
            }
            return Ok(ExhaustiveUserSearchResult { users });
        }

        let previous_len = users.len();
        merge_users(page_users, &mut users, &mut seen)?;
        if users.len() == previous_len && next_token.is_some() {
            return Err(UserSearchError::ResponseContract {
                problem: "the API returned a continuation page with no new users",
            });
        }
        let Some(next) = next_token else {
            return Ok(ExhaustiveUserSearchResult { users });
        };
        current_offset = next;
        if users.len() >= capacity {
            return Ok(ExhaustiveUserSearchResult { users });
        }
    }

    Ok(ExhaustiveUserSearchResult { users })
}

async fn request_page<A: UserSearchApi>(
    credential: &CredentialEnvelope,
    api: &A,
    query: &UserSearchQuery,
    page_size: Limit,
    offset: Offset,
) -> Result<UserSearchPage, UserSearchError> {
    api.search_users(
        credential.access_token(),
        credential.device_id(),
        query,
        UserSearchPageRequest::new(page_size, offset),
    )
    .await
    .map_err(|source| UserSearchError::Api {
        source: ApiOperationFailure::new(source),
    })
}

fn buffer_public_page(page_users: Vec<User>) -> Result<Vec<User>, UserSearchError> {
    let mut users = Vec::with_capacity(page_users.len());
    let mut seen = FirstSeen::with_capacity(page_users.len());
    merge_users(page_users, &mut users, &mut seen)?;
    Ok(users)
}

fn merge_users(
    page_users: Vec<User>,
    users: &mut Vec<User>,
    seen: &mut FirstSeen<UserId>,
) -> Result<(), UserSearchError> {
    for user in page_users {
        match seen.push(users, user.user_id().clone(), user) {
            FirstSeenResult::First | FirstSeenResult::Duplicate => {}
            FirstSeenResult::Conflicting => {
                return Err(UserSearchError::ResponseContract {
                    problem: "the API returned conflicting records for one user ID",
                });
            }
        }
    }
    Ok(())
}

fn validate_next_offset(
    current_offset: Offset,
    next: Option<Offset>,
) -> Result<(), UserSearchError> {
    if next.is_some_and(|next| next.get() <= current_offset.get()) {
        return Err(UserSearchError::ResponseContract {
            problem: "the API returned a repeated or non-progressing continuation offset",
        });
    }
    Ok(())
}

fn validate_page_len(actual: usize, requested: Limit) -> Result<(), UserSearchError> {
    if actual > requested.get() as usize {
        return Err(UserSearchError::ResponseContract {
            problem: "the API returned more users than requested",
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "users_tests.rs"]
mod tests;

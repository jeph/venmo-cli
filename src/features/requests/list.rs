use thiserror::Error;

use super::{
    PendingRequestsPageRequest, RequestDirectionFilter, RequestRecord, RequestsApi, RequestsBefore,
};
use crate::shared::{
    ApiOperationFailure, CredentialAccessError, CredentialReader, FirstSeen, FirstSeenResult,
    Limit, ReadFailureKind, require_credential,
};

#[derive(Debug, Eq, PartialEq)]
pub struct RequestsResult {
    requests: Vec<RequestRecord>,
    direction: RequestDirectionFilter,
    next_before: Option<RequestsBefore>,
}

impl RequestsResult {
    #[must_use]
    pub(crate) fn new(
        requests: Vec<RequestRecord>,
        direction: RequestDirectionFilter,
        next_before: Option<RequestsBefore>,
    ) -> Self {
        Self {
            requests,
            direction,
            next_before,
        }
    }

    #[must_use]
    pub fn requests(&self) -> &[RequestRecord] {
        &self.requests
    }

    #[must_use]
    pub const fn direction(&self) -> RequestDirectionFilter {
        self.direction
    }

    #[must_use]
    pub const fn next_before(&self) -> Option<&RequestsBefore> {
        self.next_before.as_ref()
    }
}

#[derive(Debug, Error)]
pub enum RequestsError {
    #[error(transparent)]
    Credential(#[from] CredentialAccessError),

    #[error("failed to list pending Venmo requests: {source}")]
    Api {
        #[source]
        source: ApiOperationFailure,
    },

    #[error("the Venmo pending-request response violates its contract because {problem}")]
    ResponseContract { problem: &'static str },
}

impl RequestsError {
    #[must_use]
    pub const fn failure_kind(&self) -> ReadFailureKind {
        match self {
            Self::Credential(_) => ReadFailureKind::Credential,
            Self::Api { source } => ReadFailureKind::Api(source.kind()),
            Self::ResponseContract { .. } => ReadFailureKind::ResponseContract,
        }
    }
}

pub async fn list<R, A>(
    credentials: &R,
    api: &A,
    direction: RequestDirectionFilter,
    limit: Limit,
    before: Option<&RequestsBefore>,
) -> Result<RequestsResult, RequestsError>
where
    R: CredentialReader,
    A: RequestsApi,
{
    let loaded = require_credential(credentials)?;
    let current = before.cloned();
    let page = api
        .pending_requests(
            loaded.envelope.access_token(),
            loaded.envelope.device_id(),
            loaded.envelope.user_id(),
            PendingRequestsPageRequest::new(limit, current.clone()),
        )
        .await
        .map_err(|source| RequestsError::Api {
            source: ApiOperationFailure::new(source),
        })?;
    let (page_requests, next_token) = page.into_parts();
    validate_page_len(page_requests.len(), limit)?;
    validate_next_token(current.as_ref(), next_token.as_ref())?;
    let requests = buffer_and_filter(page_requests, direction)?;
    Ok(RequestsResult::new(requests, direction, next_token))
}

fn buffer_and_filter(
    page: Vec<RequestRecord>,
    direction: RequestDirectionFilter,
) -> Result<Vec<RequestRecord>, RequestsError> {
    let mut requests = Vec::with_capacity(page.len());
    let mut seen = FirstSeen::with_capacity(page.len());
    for request in page {
        match seen.push(&mut requests, request.id().clone(), request) {
            FirstSeenResult::First | FirstSeenResult::Duplicate => {}
            FirstSeenResult::Conflicting => {
                return Err(RequestsError::ResponseContract {
                    problem: "the API returned conflicting records for one request ID",
                });
            }
        }
    }
    requests.retain(|request| direction.matches(request.direction()));
    Ok(requests)
}

fn validate_next_token(
    current: Option<&RequestsBefore>,
    next: Option<&RequestsBefore>,
) -> Result<(), RequestsError> {
    if current == next && next.is_some() {
        return Err(RequestsError::ResponseContract {
            problem: "the API returned a repeated or non-progressing before continuation",
        });
    }
    Ok(())
}

fn validate_page_len(actual: usize, requested: Limit) -> Result<(), RequestsError> {
    if actual > requested.get() as usize {
        return Err(RequestsError::ResponseContract {
            problem: "the API returned more pending requests than requested",
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;

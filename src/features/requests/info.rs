use thiserror::Error;

use super::{RequestAction, RequestId, RequestLookupApi, RequestRecord};
use crate::shared::{
    ApiOperationFailure, ApplicationFailureKind, CredentialAccessError, CredentialReader,
    require_credential,
};

#[derive(Debug, Eq, PartialEq)]
pub struct RequestInfoResult {
    request: RequestRecord,
}

impl RequestInfoResult {
    #[must_use]
    pub(crate) const fn new(request: RequestRecord) -> Self {
        Self { request }
    }

    #[must_use]
    pub const fn request(&self) -> &RequestRecord {
        &self.request
    }
}

#[derive(Debug, Error)]
pub enum RequestInfoError {
    #[error(transparent)]
    Credential(#[from] CredentialAccessError),

    #[error("failed to read Venmo request information: {source}")]
    Api {
        #[source]
        source: ApiOperationFailure,
    },

    #[error("the Venmo request response violates its contract because {problem}")]
    ResponseContract { problem: &'static str },

    #[error("the selected record is a payment, not a request")]
    NotRequest,

    #[error("the selected request is not pending or held")]
    NotOpen,
}

impl RequestInfoError {
    #[must_use]
    pub const fn failure_kind(&self) -> ApplicationFailureKind {
        match self {
            Self::Credential(_) => ApplicationFailureKind::Credential,
            Self::Api { source } => ApplicationFailureKind::Api(source.kind()),
            Self::ResponseContract { .. } => ApplicationFailureKind::ApiContract,
            Self::NotRequest | Self::NotOpen => ApplicationFailureKind::Usage,
        }
    }
}

pub async fn info<R, A>(
    credentials: &R,
    api: &A,
    request_id: &RequestId,
) -> Result<RequestInfoResult, RequestInfoError>
where
    R: CredentialReader,
    A: RequestLookupApi,
{
    let loaded = require_credential(credentials)?;
    let request = api
        .request_by_id(
            loaded.envelope.access_token(),
            loaded.envelope.device_id(),
            loaded.envelope.user_id(),
            request_id,
        )
        .await
        .map_err(|source| RequestInfoError::Api {
            source: ApiOperationFailure::new(source),
        })?;
    if request.id() != request_id {
        return Err(RequestInfoError::ResponseContract {
            problem: "the API returned a different request ID",
        });
    }
    if request.action() != RequestAction::Charge {
        return Err(RequestInfoError::NotRequest);
    }
    if !request.status().is_pending_or_held() {
        return Err(RequestInfoError::NotOpen);
    }
    Ok(RequestInfoResult::new(request))
}

#[cfg(test)]
#[path = "info_tests.rs"]
mod tests;

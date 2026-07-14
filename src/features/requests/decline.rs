use thiserror::Error;

use super::preflight;
use super::{
    DeclineRequestPlan, DeclinedRequest, RequestDeclineApi, RequestId, RequestLookupApi,
    RequestMutationPreflightError,
};
use crate::features::auth::CurrentAccountApi;
use crate::shared::{
    ApiFailure, ApiOperationFailure, ApplicationFailureKind, CredentialEnvelope, CredentialReader,
};

#[derive(Debug)]
pub struct PreparedDecline {
    credential: CredentialEnvelope,
    plan: DeclineRequestPlan,
}

impl PreparedDecline {
    #[must_use]
    pub(crate) const fn new(credential: CredentialEnvelope, plan: DeclineRequestPlan) -> Self {
        Self { credential, plan }
    }

    #[must_use]
    pub const fn plan(&self) -> &DeclineRequestPlan {
        &self.plan
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct DeclineResult {
    plan: DeclineRequestPlan,
    declined: DeclinedRequest,
}

impl DeclineResult {
    #[must_use]
    pub(crate) const fn new(plan: DeclineRequestPlan, declined: DeclinedRequest) -> Self {
        Self { plan, declined }
    }

    #[must_use]
    pub const fn plan(&self) -> &DeclineRequestPlan {
        &self.plan
    }

    #[must_use]
    pub const fn declined(&self) -> &DeclinedRequest {
        &self.declined
    }
}

#[derive(Debug, Error)]
pub enum DeclineError {
    #[error(transparent)]
    Preflight(#[from] RequestMutationPreflightError),
    #[error("failed to decline the Venmo request: {source}")]
    Decline {
        #[source]
        source: ApiOperationFailure,
    },
}

impl DeclineError {
    #[must_use]
    pub const fn failure_kind(&self) -> ApplicationFailureKind {
        match self {
            Self::Preflight(source) => source.failure_kind(),
            Self::Decline { source } => ApplicationFailureKind::Api(source.kind()),
        }
    }
}

pub(crate) async fn prepare<R, A>(
    credentials: &R,
    api: &A,
    request_id: &RequestId,
) -> Result<PreparedDecline, DeclineError>
where
    R: CredentialReader,
    A: CurrentAccountApi + RequestLookupApi,
    <A as CurrentAccountApi>::Error: ApiFailure,
{
    let (credential, account, request) = preflight::prepare(credentials, api, request_id)
        .await?
        .into_parts();
    Ok(PreparedDecline::new(
        credential,
        DeclineRequestPlan::new(account, request),
    ))
}

pub(crate) async fn execute<A>(
    api: &A,
    prepared: PreparedDecline,
) -> Result<DeclineResult, DeclineError>
where
    A: RequestDeclineApi,
{
    let declined = api
        .decline_request(
            prepared.credential.access_token(),
            prepared.credential.device_id(),
            &prepared.plan,
        )
        .await
        .map_err(|source| DeclineError::Decline {
            source: ApiOperationFailure::new(source),
        })?;
    Ok(DeclineResult::new(prepared.plan, declined))
}

#[cfg(test)]
#[path = "decline_tests.rs"]
mod tests;

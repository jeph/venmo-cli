use thiserror::Error;

use super::preflight;
use super::{
    DeclineRequestPlan, DeclinedRequest, RequestDeclineApi, RequestId, RequestLookupApi,
    RequestMutationPreflightError,
};
use crate::features::auth::{CurrentAccountApi, PromptError, prompt_failure_kind};
use crate::features::payments::DefaultNoConfirmation;
use crate::features::payments::confirmation::{self, DefaultNoConfirmationError};
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
    #[error(
        "request decline confirmation requires both stdin and stderr to be terminals; pass `--yes` to authorize non-interactively"
    )]
    ConfirmationRequired,
    #[error("request decline cancelled")]
    ConfirmationDeclined,
    #[error("request decline confirmation failed: {source}")]
    Confirmation {
        #[source]
        source: PromptError,
    },
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
            Self::ConfirmationRequired => ApplicationFailureKind::Usage,
            Self::ConfirmationDeclined => ApplicationFailureKind::Cancelled,
            Self::Confirmation { source } => prompt_failure_kind(source),
            Self::Decline { source } => ApplicationFailureKind::Api(source.kind()),
        }
    }
}

#[derive(Debug)]
pub(crate) struct AuthorizedDecline(PreparedDecline);

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

pub(crate) fn authorize<P>(
    prompt: &P,
    prepared: PreparedDecline,
    assume_yes: bool,
) -> Result<AuthorizedDecline, DeclineError>
where
    P: DefaultNoConfirmation,
{
    confirmation::authorize(
        prompt,
        assume_yes,
        "Decline this request without sending money?",
    )
    .map_err(|error| match error {
        DefaultNoConfirmationError::Required => DeclineError::ConfirmationRequired,
        DefaultNoConfirmationError::Declined => DeclineError::ConfirmationDeclined,
        DefaultNoConfirmationError::Prompt(source) => DeclineError::Confirmation { source },
    })?;
    Ok(AuthorizedDecline(prepared))
}

pub(crate) async fn execute<A>(
    api: &A,
    authorized: AuthorizedDecline,
) -> Result<DeclineResult, DeclineError>
where
    A: RequestDeclineApi,
{
    let AuthorizedDecline(prepared) = authorized;
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

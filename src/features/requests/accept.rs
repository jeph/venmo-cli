use thiserror::Error;

use super::preflight;
use super::validation::{
    RequestMutationValidationError, validate_complete_request, validate_private_audience,
};
use super::{
    AcceptRequestPlan, AcceptedRequest, RequestAcceptanceApi, RequestId, RequestLookupApi,
    RequestMutationPreflightError,
};
use crate::features::auth::{CurrentAccountApi, PromptError, prompt_failure_kind};
use crate::features::payments::confirmation::{self, DefaultNoConfirmationError};
use crate::features::payments::{
    DefaultNoConfirmation, FinancialValidationError, validate_recipient,
};
use crate::features::people::UserLookupApi;
use crate::features::wallet::BalanceApi;
use crate::shared::{
    ApiFailure, ApiOperationFailure, ApplicationFailureKind, CredentialEnvelope, CredentialReader,
};

#[derive(Debug)]
pub struct PreparedAccept {
    credential: CredentialEnvelope,
    plan: AcceptRequestPlan,
}

impl PreparedAccept {
    #[must_use]
    pub(crate) const fn new(credential: CredentialEnvelope, plan: AcceptRequestPlan) -> Self {
        Self { credential, plan }
    }

    #[must_use]
    pub const fn plan(&self) -> &AcceptRequestPlan {
        &self.plan
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct AcceptResult {
    plan: AcceptRequestPlan,
    accepted: AcceptedRequest,
}

impl AcceptResult {
    #[must_use]
    pub(crate) const fn new(plan: AcceptRequestPlan, accepted: AcceptedRequest) -> Self {
        Self { plan, accepted }
    }

    #[must_use]
    pub const fn plan(&self) -> &AcceptRequestPlan {
        &self.plan
    }

    #[must_use]
    pub const fn accepted(&self) -> &AcceptedRequest {
        &self.accepted
    }
}

#[derive(Debug, Error)]
pub enum AcceptError {
    #[error(transparent)]
    Preflight(#[from] RequestMutationPreflightError),
    #[error(transparent)]
    AccountOrRequester(#[from] FinancialValidationError),
    #[error("failed to load authoritative requester details: {source}")]
    RequesterLookup {
        #[source]
        source: ApiOperationFailure,
    },
    #[error("the authoritative requester identity did not match the incoming request")]
    RequesterIdentityMismatch,
    #[error(transparent)]
    RequestValidation(#[from] RequestMutationValidationError),
    #[error("failed to read the Venmo wallet balance: {source}")]
    Balance {
        #[source]
        source: ApiOperationFailure,
    },
    #[error(
        "acceptance requires available Venmo balance covering the full request; the CLI submits no external funding method"
    )]
    InsufficientWalletBalance,
    #[error(
        "request acceptance confirmation requires both stdin and stderr to be terminals; pass `--yes` to authorize non-interactively"
    )]
    ConfirmationRequired,
    #[error("request acceptance cancelled")]
    ConfirmationDeclined,
    #[error("request acceptance confirmation failed: {source}")]
    Confirmation {
        #[source]
        source: PromptError,
    },
    #[error("failed to accept the Venmo request: {source}")]
    Accept {
        #[source]
        source: ApiOperationFailure,
    },
}

impl AcceptError {
    #[must_use]
    pub const fn failure_kind(&self) -> ApplicationFailureKind {
        match self {
            Self::Preflight(source) => source.failure_kind(),
            Self::RequesterLookup { source }
            | Self::Balance { source }
            | Self::Accept { source } => ApplicationFailureKind::Api(source.kind()),
            Self::AccountOrRequester(source) => source.failure_kind(),
            Self::RequesterIdentityMismatch => ApplicationFailureKind::ApiContract,
            Self::RequestValidation(source) => source.failure_kind(),
            Self::InsufficientWalletBalance | Self::ConfirmationRequired => {
                ApplicationFailureKind::Usage
            }
            Self::ConfirmationDeclined => ApplicationFailureKind::Cancelled,
            Self::Confirmation { source } => prompt_failure_kind(source),
        }
    }
}

#[derive(Debug)]
pub(crate) struct AuthorizedAccept(PreparedAccept);

pub(crate) async fn prepare<R, A>(
    credentials: &R,
    api: &A,
    request_id: &RequestId,
) -> Result<PreparedAccept, AcceptError>
where
    R: CredentialReader,
    A: CurrentAccountApi + RequestLookupApi + UserLookupApi + BalanceApi,
    <A as CurrentAccountApi>::Error: ApiFailure,
{
    let (credential, account, request) = preflight::prepare(credentials, api, request_id)
        .await?
        .into_parts();
    validate_private_audience(&request)?;
    let requester = api
        .user_by_id(
            credential.access_token(),
            credential.device_id(),
            request.counterparty().user_id(),
        )
        .await
        .map_err(|source| AcceptError::RequesterLookup {
            source: ApiOperationFailure::new(source),
        })?;
    if requester.user_id() != request.counterparty().user_id() {
        return Err(AcceptError::RequesterIdentityMismatch);
    }
    validate_recipient(&account, &requester)?;
    let request = request.with_counterparty(requester);
    validate_complete_request(&request)?;
    let balance = api
        .balance(credential.access_token(), credential.device_id())
        .await
        .map_err(|source| AcceptError::Balance {
            source: ApiOperationFailure::new(source),
        })?;
    let Ok(available_cents) = u64::try_from(balance.available().cents()) else {
        return Err(AcceptError::InsufficientWalletBalance);
    };
    if available_cents < request.amount().cents() {
        return Err(AcceptError::InsufficientWalletBalance);
    }
    Ok(PreparedAccept::new(
        credential,
        AcceptRequestPlan::new(account, request, balance),
    ))
}

pub(crate) fn authorize<P>(
    prompt: &P,
    prepared: PreparedAccept,
    assume_yes: bool,
) -> Result<AuthorizedAccept, AcceptError>
where
    P: DefaultNoConfirmation,
{
    confirmation::authorize(
        prompt,
        assume_yes,
        "Accept this request and pay its requester?",
    )
    .map_err(|error| match error {
        DefaultNoConfirmationError::Required => AcceptError::ConfirmationRequired,
        DefaultNoConfirmationError::Declined => AcceptError::ConfirmationDeclined,
        DefaultNoConfirmationError::Prompt(source) => AcceptError::Confirmation { source },
    })?;
    Ok(AuthorizedAccept(prepared))
}

pub(crate) async fn execute<A>(
    api: &A,
    authorized: AuthorizedAccept,
) -> Result<AcceptResult, AcceptError>
where
    A: RequestAcceptanceApi,
{
    let AuthorizedAccept(prepared) = authorized;
    let accepted = api
        .accept_request(
            prepared.credential.access_token(),
            prepared.credential.device_id(),
            &prepared.plan,
        )
        .await
        .map_err(|source| AcceptError::Accept {
            source: ApiOperationFailure::new(source),
        })?;
    Ok(AcceptResult::new(prepared.plan, accepted))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

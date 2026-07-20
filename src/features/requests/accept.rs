use thiserror::Error;

use super::preflight;
use super::validation::{
    RequestMutationValidationError, validate_complete_request, validate_private_audience,
};
use super::{
    AcceptRequestPlan, AcceptedRequest, RequestAcceptanceApi, RequestAcceptanceOutcome,
    RequestAcceptanceVerification, RequestApprovalEligibilityApi, RequestApprovalFees,
    RequestApprovalNotificationApi, RequestId, RequestLookupApi, RequestMutationPreflightError,
};
use crate::features::auth::{CurrentAccountApi, PromptError, prompt_failure_kind};
use crate::features::p2p_step_up::{self, P2pStepUpApi, P2pStepUpError, P2pStepUpInput};
use crate::features::payments::confirmation::{self, DefaultNoConfirmationError};
use crate::features::payments::funding::{self, FundingSelectionError};
use crate::features::payments::{
    DefaultNoConfirmation, FinancialValidationError, PeerFundingApi, validate_recipient,
};
use crate::features::people::UserLookupApi;
use crate::features::wallet::{BalanceApi, PaymentMethodId};
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
    #[error("failed to resolve the request's approval notification: {source}")]
    NotificationLookup {
        #[source]
        source: ApiOperationFailure,
    },
    #[error("failed to obtain peer-payment funding methods for request acceptance: {source}")]
    FundingMethods {
        #[source]
        source: ApiOperationFailure,
    },
    #[error(transparent)]
    FundingSelection(#[from] FundingSelectionError),
    #[error(
        "failed to verify request-approval eligibility for the selected funding method: {source}"
    )]
    Eligibility {
        #[source]
        source: ApiOperationFailure,
    },
    #[error("the purchase-protection fee exceeded the request amount")]
    ProtectionFeeExceedsAmount,
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
    #[error(transparent)]
    StepUp(#[from] P2pStepUpError),
}

impl AcceptError {
    #[must_use]
    pub const fn failure_kind(&self) -> ApplicationFailureKind {
        match self {
            Self::Preflight(source) => source.failure_kind(),
            Self::RequesterLookup { source }
            | Self::Balance { source }
            | Self::NotificationLookup { source }
            | Self::FundingMethods { source }
            | Self::Eligibility { source }
            | Self::Accept { source } => ApplicationFailureKind::Api(source.kind()),
            Self::AccountOrRequester(source) => source.failure_kind(),
            Self::FundingSelection(source) => source.failure_kind(),
            Self::RequesterIdentityMismatch => ApplicationFailureKind::ApiContract,
            Self::ProtectionFeeExceedsAmount => ApplicationFailureKind::ApiContract,
            Self::RequestValidation(source) => source.failure_kind(),
            Self::ConfirmationRequired => ApplicationFailureKind::Usage,
            Self::ConfirmationDeclined => ApplicationFailureKind::Cancelled,
            Self::Confirmation { source } => prompt_failure_kind(source),
            Self::StepUp(source) => source.failure_kind(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct AuthorizedAccept(PreparedAccept);

pub(crate) async fn prepare_with_protection<R, A>(
    credentials: &R,
    api: &A,
    request_id: &RequestId,
    requested_source: Option<&PaymentMethodId>,
    protect: bool,
) -> Result<PreparedAccept, AcceptError>
where
    R: CredentialReader,
    A: CurrentAccountApi
        + RequestLookupApi
        + UserLookupApi
        + BalanceApi
        + RequestApprovalNotificationApi
        + PeerFundingApi
        + RequestApprovalEligibilityApi,
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
    let mut plan = AcceptRequestPlan::new(account, request, balance);
    let notification_id = api
        .request_approval_notification_id(
            credential.access_token(),
            credential.device_id(),
            plan.request().id(),
        )
        .await
        .map_err(|source| AcceptError::NotificationLookup {
            source: ApiOperationFailure::new(source),
        })?;
    let sources = api
        .peer_funding_sources(credential.access_token(), credential.device_id())
        .await
        .map_err(|source| AcceptError::FundingMethods {
            source: ApiOperationFailure::new(source),
        })?;
    let (funding_source, funding_source_selection) = funding::select_source(
        &sources,
        plan.balance(),
        plan.request().amount(),
        requested_source,
    )?
    .into_parts();
    let note = plan
        .request()
        .note()
        .ok_or(RequestMutationValidationError::InvalidNote)?;
    let eligibility = api
        .request_approval_eligibility(
            credential.access_token(),
            credential.device_id(),
            plan.request().counterparty(),
            plan.request().amount().cents(),
            note,
            &funding_source,
        )
        .await
        .map_err(|source| AcceptError::Eligibility {
            source: ApiOperationFailure::new(source),
        })?;
    let (eligibility_token, fees) = eligibility.into_parts();
    let fees = if protect {
        if fees.total_cents() > plan.request().amount().cents() {
            return Err(AcceptError::ProtectionFeeExceedsAmount);
        }
        fees
    } else {
        RequestApprovalFees::omitted()
    };
    plan = plan.with_funding(
        notification_id,
        funding_source,
        funding_source_selection,
        eligibility_token,
        fees,
        protect,
    );
    Ok(PreparedAccept::new(credential, plan))
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

pub(crate) async fn execute<A, P>(
    api: &A,
    prompt: &P,
    authorized: AuthorizedAccept,
) -> Result<AcceptResult, AcceptError>
where
    A: RequestAcceptanceApi + P2pStepUpApi,
    P: P2pStepUpInput,
{
    let AuthorizedAccept(prepared) = authorized;
    let first_attempt = api
        .accept_request(
            prepared.credential.access_token(),
            prepared.credential.device_id(),
            &prepared.plan,
            RequestAcceptanceVerification::Unverified,
        )
        .await
        .map_err(|source| AcceptError::Accept {
            source: ApiOperationFailure::new(source),
        })?;
    let accepted = match first_attempt {
        RequestAcceptanceOutcome::Accepted(accepted) => accepted,
        RequestAcceptanceOutcome::OtpStepUpRequired(session_id) => {
            p2p_step_up::complete(api, prompt, &prepared.credential, &session_id).await?;
            match api
                .accept_request(
                    prepared.credential.access_token(),
                    prepared.credential.device_id(),
                    &prepared.plan,
                    RequestAcceptanceVerification::SmsOtpVerified(session_id),
                )
                .await
                .map_err(|source| AcceptError::Accept {
                    source: ApiOperationFailure::new(source),
                })? {
                RequestAcceptanceOutcome::Accepted(accepted) => accepted,
                RequestAcceptanceOutcome::OtpStepUpRequired(_) => {
                    return Err(P2pStepUpError::StillRequired.into());
                }
            }
        }
    };
    Ok(AcceptResult::new(prepared.plan, accepted))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

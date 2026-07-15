use thiserror::Error;

use super::confirmation::{self, DefaultNoConfirmationError};
use super::funding::{self, FundingSelectionError};
use super::preflight::{self, PeerPreflightError};
use super::{
    BlankSourceEligibilityApi, CreatedPayment, DefaultNoConfirmation, FundingChoiceSelection,
    PayPlan, PaymentCreationApi, PeerFundingApi,
};
use crate::features::auth::{CurrentAccountApi, PromptError, prompt_failure_kind};
use crate::features::people::{RecipientInput, UserLookupApi, UserSearchApi};
use crate::features::wallet::{BalanceApi, PaymentMethodId};
use crate::shared::{
    ApiFailure, ApiOperationFailure, ApplicationFailureKind, ClientRequestIdGenerator,
    CredentialEnvelope, CredentialReader, Money, Note,
};

#[derive(Debug)]
pub struct PreparedPay {
    credential: CredentialEnvelope,
    plan: PayPlan,
}

impl PreparedPay {
    #[must_use]
    pub(crate) const fn new(credential: CredentialEnvelope, plan: PayPlan) -> Self {
        Self { credential, plan }
    }

    #[must_use]
    pub const fn plan(&self) -> &PayPlan {
        &self.plan
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct PayResult {
    plan: PayPlan,
    created: CreatedPayment,
}

impl PayResult {
    #[must_use]
    pub(crate) const fn new(plan: PayPlan, created: CreatedPayment) -> Self {
        Self { plan, created }
    }

    #[must_use]
    pub const fn plan(&self) -> &PayPlan {
        &self.plan
    }

    #[must_use]
    pub const fn created(&self) -> &CreatedPayment {
        &self.created
    }
}

#[derive(Debug, Error)]
pub enum PayError {
    #[error(transparent)]
    Preflight(#[from] PeerPreflightError),
    #[error("failed to read the Venmo wallet balance: {source}")]
    Balance {
        #[source]
        source: ApiOperationFailure,
    },
    #[error("failed to obtain peer-payment funding methods: {source}")]
    FundingMethods {
        #[source]
        source: ApiOperationFailure,
    },
    #[error(transparent)]
    FundingSelection(#[from] FundingSelectionError),
    #[error("failed to verify payment eligibility and fees: {source}")]
    Eligibility {
        #[source]
        source: ApiOperationFailure,
    },
    #[error(
        "payment confirmation requires both stdin and stderr to be terminals; pass `--yes` to authorize non-interactively"
    )]
    ConfirmationRequired,
    #[error("payment cancelled")]
    ConfirmationDeclined,
    #[error("payment confirmation failed: {source}")]
    Confirmation {
        #[source]
        source: PromptError,
    },
    #[error("failed to create the Venmo payment: {source}")]
    Create {
        #[source]
        source: ApiOperationFailure,
    },
}

impl PayError {
    #[must_use]
    pub const fn failure_kind(&self) -> ApplicationFailureKind {
        match self {
            Self::Preflight(source) => source.failure_kind(),
            Self::Balance { source }
            | Self::FundingMethods { source }
            | Self::Eligibility { source }
            | Self::Create { source } => ApplicationFailureKind::Api(source.kind()),
            Self::FundingSelection(source) => source.failure_kind(),
            Self::ConfirmationRequired => ApplicationFailureKind::Usage,
            Self::ConfirmationDeclined => ApplicationFailureKind::Cancelled,
            Self::Confirmation { source } => prompt_failure_kind(source),
        }
    }
}

#[derive(Debug)]
pub(crate) struct AuthorizedPay(PreparedPay);

#[allow(clippy::too_many_arguments)]
pub(crate) async fn prepare<R, A, G, P>(
    credentials: &R,
    api: &A,
    generator: &G,
    prompt: &P,
    recipient: &RecipientInput,
    amount: Money,
    note: Note,
    requested_method: Option<&PaymentMethodId>,
) -> Result<PreparedPay, PayError>
where
    R: CredentialReader,
    A: CurrentAccountApi
        + UserLookupApi
        + UserSearchApi
        + BalanceApi
        + PeerFundingApi
        + BlankSourceEligibilityApi,
    <A as CurrentAccountApi>::Error: ApiFailure,
    G: ClientRequestIdGenerator,
    P: FundingChoiceSelection,
{
    let (credential, account, recipient) = preflight::prepare(credentials, api, recipient)
        .await?
        .into_parts();
    let balance = api
        .balance(credential.access_token(), credential.device_id())
        .await
        .map_err(|source| PayError::Balance {
            source: ApiOperationFailure::new(source),
        })?;
    let methods = api
        .peer_funding_methods(credential.access_token(), credential.device_id())
        .await
        .map_err(|source| PayError::FundingMethods {
            source: ApiOperationFailure::new(source),
        })?;
    let funding = funding::select(prompt, &methods, requested_method)?;
    let eligibility = api
        .blank_source_eligibility(
            credential.access_token(),
            credential.device_id(),
            &recipient,
            amount,
            &note,
        )
        .await
        .map_err(|source| PayError::Eligibility {
            source: ApiOperationFailure::new(source),
        })?;
    let eligibility_fee_cents = eligibility.overall_fee_cents();
    let plan = PayPlan::new(
        generator.generate(),
        account,
        recipient,
        amount,
        note,
        balance,
        funding,
        eligibility_fee_cents,
        eligibility.into_token(),
    );
    Ok(PreparedPay::new(credential, plan))
}

pub(crate) fn authorize<P>(
    prompt: &P,
    prepared: PreparedPay,
    assume_yes: bool,
) -> Result<AuthorizedPay, PayError>
where
    P: DefaultNoConfirmation,
{
    confirmation::authorize(prompt, assume_yes, "Send this payment?").map_err(
        |error| match error {
            DefaultNoConfirmationError::Required => PayError::ConfirmationRequired,
            DefaultNoConfirmationError::Declined => PayError::ConfirmationDeclined,
            DefaultNoConfirmationError::Prompt(source) => PayError::Confirmation { source },
        },
    )?;
    Ok(AuthorizedPay(prepared))
}

pub(crate) async fn execute<A>(api: &A, authorized: AuthorizedPay) -> Result<PayResult, PayError>
where
    A: PaymentCreationApi,
{
    let AuthorizedPay(prepared) = authorized;
    let created = api
        .create_payment(
            prepared.credential.access_token(),
            prepared.credential.device_id(),
            &prepared.plan,
        )
        .await
        .map_err(|source| PayError::Create {
            source: ApiOperationFailure::new(source),
        })?;
    Ok(PayResult::new(prepared.plan, created))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

use thiserror::Error;

use super::confirmation::{self, DefaultNoConfirmationError};
use super::funding::{self, FundingSelectionError};
use super::preflight::{self, PeerPreflightError};
use super::{
    BlankSourceEligibilityApi, CreatedPayment, DefaultNoConfirmation, PayPlan, PaymentCreationApi,
    PaymentCreationOutcome, PaymentStepUpApi, PaymentStepUpInput, PaymentVerification,
    PeerFundingApi, ProtectedPaymentEligibilityApi,
};
use crate::features::auth::{CurrentAccountApi, PromptError, prompt_failure_kind};
use crate::features::p2p_step_up::{self, P2pStepUpError};
use crate::features::people::{RecipientInput, UserLookupApi, UserSearchApi};
use crate::features::wallet::{BalanceApi, PaymentMethodId};
use crate::shared::{
    ApiFailure, ApiOperationFailure, ApplicationFailureKind, ClientRequestIdGenerator,
    CredentialEnvelope, CredentialReader, Money, Note, Visibility,
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
    #[error("the purchase-protection seller fee exceeded the payment amount")]
    ProtectionFeeExceedsAmount,
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
    #[error(transparent)]
    StepUp(#[from] P2pStepUpError),
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
            Self::ProtectionFeeExceedsAmount => ApplicationFailureKind::ApiContract,
            Self::ConfirmationRequired => ApplicationFailureKind::Usage,
            Self::ConfirmationDeclined => ApplicationFailureKind::Cancelled,
            Self::Confirmation { source } => prompt_failure_kind(source),
            Self::StepUp(source) => source.failure_kind(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct AuthorizedPay(PreparedPay);

#[derive(Clone, Copy, Debug)]
pub(crate) struct PayOptions<'a> {
    requested_source: Option<&'a PaymentMethodId>,
    visibility: Visibility,
    protect: bool,
}

impl<'a> PayOptions<'a> {
    #[must_use]
    pub(crate) const fn new(
        requested_source: Option<&'a PaymentMethodId>,
        visibility: Visibility,
        protect: bool,
    ) -> Self {
        Self {
            requested_source,
            visibility,
            protect,
        }
    }
}

pub(crate) async fn prepare<R, A, G>(
    credentials: &R,
    api: &A,
    generator: &G,
    recipient: &RecipientInput,
    amount: Money,
    note: Note,
    options: PayOptions<'_>,
) -> Result<PreparedPay, PayError>
where
    R: CredentialReader,
    A: CurrentAccountApi
        + UserLookupApi
        + UserSearchApi
        + BalanceApi
        + PeerFundingApi
        + BlankSourceEligibilityApi
        + ProtectedPaymentEligibilityApi,
    <A as CurrentAccountApi>::Error: ApiFailure,
    G: ClientRequestIdGenerator,
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
    let sources = api
        .peer_funding_sources(credential.access_token(), credential.device_id())
        .await
        .map_err(|source| PayError::FundingMethods {
            source: ApiOperationFailure::new(source),
        })?;
    let (funding_source, funding_source_selection) =
        funding::select_source(&sources, &balance, amount, options.requested_source)?.into_parts();
    let plan = if options.protect {
        let eligibility = api
            .protected_payment_eligibility(
                credential.access_token(),
                credential.device_id(),
                &recipient,
                amount,
                &note,
                &funding_source,
            )
            .await
            .map_err(|source| PayError::Eligibility {
                source: ApiOperationFailure::new(source),
            })?;
        let (eligibility_token, fee) = eligibility.into_parts();
        if fee.calculated_fee_amount_in_cents() > amount.cents() {
            return Err(PayError::ProtectionFeeExceedsAmount);
        }
        PayPlan::new_purchase_protected(
            generator.generate(),
            account,
            recipient,
            amount,
            note,
            balance,
            funding_source,
            funding_source_selection,
            eligibility_token,
            fee,
            options.visibility,
        )
    } else {
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
        PayPlan::new(
            generator.generate(),
            account,
            recipient,
            amount,
            note,
            balance,
            funding_source,
            funding_source_selection,
            eligibility_fee_cents,
            eligibility.into_token(),
            options.visibility,
        )
    };
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

pub(crate) async fn execute<A, P>(
    api: &A,
    prompt: &P,
    authorized: AuthorizedPay,
) -> Result<PayResult, PayError>
where
    A: PaymentCreationApi + PaymentStepUpApi,
    P: PaymentStepUpInput,
{
    let AuthorizedPay(prepared) = authorized;
    let first_attempt = api
        .create_payment(
            prepared.credential.access_token(),
            prepared.credential.device_id(),
            &prepared.plan,
            PaymentVerification::Unverified,
        )
        .await
        .map_err(|source| PayError::Create {
            source: ApiOperationFailure::new(source),
        })?;
    let created = match first_attempt {
        PaymentCreationOutcome::Created(created) => created,
        PaymentCreationOutcome::OtpStepUpRequired => {
            let request_id = prepared.plan.request_id();
            p2p_step_up::complete(api, prompt, &prepared.credential, &request_id).await?;
            match api
                .create_payment(
                    prepared.credential.access_token(),
                    prepared.credential.device_id(),
                    &prepared.plan,
                    PaymentVerification::SmsOtpVerified,
                )
                .await
                .map_err(|source| PayError::Create {
                    source: ApiOperationFailure::new(source),
                })? {
                PaymentCreationOutcome::Created(created) => created,
                PaymentCreationOutcome::OtpStepUpRequired => {
                    return Err(P2pStepUpError::StillRequired.into());
                }
            }
        }
    };
    Ok(PayResult::new(prepared.plan, created))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

use thiserror::Error;

use super::confirmation::{self, DefaultNoConfirmationError};
use super::funding::{self, FundingSelectionError};
use super::preflight::{self, PeerPreflightError};
use super::{
    BlankSourceEligibilityApi, CreatedPayment, DefaultNoConfirmation, PayPlan, PaymentCreationApi,
    PaymentCreationOutcome, PaymentOtpVerification, PaymentStepUpApi, PaymentStepUpInput,
    PaymentVerification, PeerFundingApi,
};
use crate::features::auth::{CurrentAccountApi, PromptError, prompt_failure_kind};
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
    #[error(
        "Venmo requires SMS verification for this payment; rerun in a terminal that can prompt for the code"
    )]
    StepUpPromptRequired,
    #[error("failed to send the Venmo payment-verification code: {source}")]
    StepUpIssue {
        #[source]
        source: ApiOperationFailure,
    },
    #[error("payment-verification prompt failed: {source}")]
    StepUpPrompt {
        #[source]
        source: PromptError,
    },
    #[error("failed to verify the Venmo payment code: {source}")]
    StepUpVerify {
        #[source]
        source: ApiOperationFailure,
    },
    #[error("the Venmo payment-verification code was incorrect")]
    StepUpOtpIncorrect,
    #[error("the Venmo payment-verification code expired")]
    StepUpOtpExpired,
    #[error("Venmo did not expect a payment-verification code for this payment")]
    StepUpOtpUnexpected,
    #[error("Venmo rejected the payment verification after too many incorrect attempts")]
    StepUpOtpTooManyAttempts,
    #[error("Venmo still requires SMS verification after the code was verified")]
    StepUpStillRequired,
}

impl PayError {
    #[must_use]
    pub const fn failure_kind(&self) -> ApplicationFailureKind {
        match self {
            Self::Preflight(source) => source.failure_kind(),
            Self::Balance { source }
            | Self::FundingMethods { source }
            | Self::Eligibility { source }
            | Self::Create { source }
            | Self::StepUpIssue { source }
            | Self::StepUpVerify { source } => ApplicationFailureKind::Api(source.kind()),
            Self::FundingSelection(source) => source.failure_kind(),
            Self::ConfirmationRequired | Self::StepUpPromptRequired => {
                ApplicationFailureKind::Usage
            }
            Self::ConfirmationDeclined => ApplicationFailureKind::Cancelled,
            Self::Confirmation { source } | Self::StepUpPrompt { source } => {
                prompt_failure_kind(source)
            }
            Self::StepUpOtpIncorrect
            | Self::StepUpOtpExpired
            | Self::StepUpOtpUnexpected
            | Self::StepUpOtpTooManyAttempts
            | Self::StepUpStillRequired => {
                ApplicationFailureKind::Api(crate::shared::ApiFailureKind::Rejected)
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct AuthorizedPay(PreparedPay);

#[derive(Clone, Copy, Debug)]
pub(crate) struct PayOptions<'a> {
    requested_source: Option<&'a PaymentMethodId>,
    visibility: Visibility,
}

impl<'a> PayOptions<'a> {
    #[must_use]
    pub(crate) const fn new(
        requested_source: Option<&'a PaymentMethodId>,
        visibility: Visibility,
    ) -> Self {
        Self {
            requested_source,
            visibility,
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
        + BlankSourceEligibilityApi,
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
        funding_source,
        funding_source_selection,
        eligibility_fee_cents,
        eligibility.into_token(),
        options.visibility,
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
            if !prompt.can_prompt() {
                return Err(PayError::StepUpPromptRequired);
            }
            let request_id = prepared.plan.request_id();
            api.issue_payment_otp(
                prepared.credential.access_token(),
                prepared.credential.device_id(),
                &request_id,
            )
            .await
            .map_err(|source| PayError::StepUpIssue {
                source: ApiOperationFailure::new(source),
            })?;
            let otp = prompt
                .read_payment_otp("Venmo SMS payment-verification code")
                .map_err(|source| PayError::StepUpPrompt { source })?;
            let verification = api
                .verify_payment_otp(
                    prepared.credential.access_token(),
                    prepared.credential.device_id(),
                    &request_id,
                    &otp,
                )
                .await
                .map_err(|source| PayError::StepUpVerify {
                    source: ApiOperationFailure::new(source),
                })?;
            match verification {
                PaymentOtpVerification::Verified => {}
                PaymentOtpVerification::Incorrect => return Err(PayError::StepUpOtpIncorrect),
                PaymentOtpVerification::Expired => return Err(PayError::StepUpOtpExpired),
                PaymentOtpVerification::Unexpected => return Err(PayError::StepUpOtpUnexpected),
                PaymentOtpVerification::TooManyIncorrectAttempts => {
                    return Err(PayError::StepUpOtpTooManyAttempts);
                }
            }
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
                    return Err(PayError::StepUpStillRequired);
                }
            }
        }
    };
    Ok(PayResult::new(prepared.plan, created))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

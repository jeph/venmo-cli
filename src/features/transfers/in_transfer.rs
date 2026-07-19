use thiserror::Error;

use super::selection::{self, TransferInSelectionError};
use super::{
    CreatedTransferIn, TransferInCreationApi, TransferInPlan, TransferInstrumentId,
    TransferOptionsApi,
};
use crate::features::auth::{CurrentAccountApi, PromptError, prompt_failure_kind};
use crate::features::payments::confirmation::{self, DefaultNoConfirmationError};
use crate::features::payments::{
    DefaultNoConfirmation, FinancialValidationError, validate_account,
};
use crate::shared::{
    ApiFailure, ApiOperationFailure, ApplicationFailureKind, CredentialAccessError,
    CredentialEnvelope, CredentialReader, Money, require_credential,
};

#[derive(Debug)]
pub struct PreparedTransferIn {
    credential: CredentialEnvelope,
    plan: TransferInPlan,
}

impl PreparedTransferIn {
    #[must_use]
    pub(crate) const fn new(credential: CredentialEnvelope, plan: TransferInPlan) -> Self {
        Self { credential, plan }
    }

    #[must_use]
    pub const fn plan(&self) -> &TransferInPlan {
        &self.plan
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct TransferInResult {
    plan: TransferInPlan,
    created: CreatedTransferIn,
}

impl TransferInResult {
    #[must_use]
    pub(crate) const fn new(plan: TransferInPlan, created: CreatedTransferIn) -> Self {
        Self { plan, created }
    }

    #[must_use]
    pub const fn plan(&self) -> &TransferInPlan {
        &self.plan
    }

    #[must_use]
    pub const fn created(&self) -> &CreatedTransferIn {
        &self.created
    }
}

#[derive(Debug, Error)]
pub enum TransferInError {
    #[error(transparent)]
    Credential(#[from] CredentialAccessError),

    #[error("failed to validate the current Venmo account: {source}")]
    CurrentAccount {
        #[source]
        source: ApiOperationFailure,
    },

    #[error(transparent)]
    Validation(#[from] FinancialValidationError),

    #[error("failed to read Venmo transfer options: {source}")]
    Options {
        #[source]
        source: ApiOperationFailure,
    },

    #[error(transparent)]
    Selection(#[from] TransferInSelectionError),

    #[error("the requested amount exceeds the evidenced mobile add-funds integer range")]
    AmountTooLarge,

    #[error("the transfer-options response did not provide a current add-funds minimum")]
    MissingAmountLimit,

    #[error(
        "the requested amount is below the current add-funds minimum of ${}.{:02}",
        .minimum_cents / 100,
        .minimum_cents % 100
    )]
    AmountBelowMinimum { minimum_cents: u64 },

    #[error(
        "the requested amount exceeds the current add-funds maximum of ${}.{:02}",
        .maximum_cents / 100,
        .maximum_cents % 100
    )]
    AmountAboveMaximum { maximum_cents: u64 },

    #[error(
        "transfer confirmation requires both stdin and stderr to be terminals; pass `--yes` to authorize non-interactively"
    )]
    ConfirmationRequired,

    #[error("transfer cancelled")]
    ConfirmationDeclined,

    #[error("transfer confirmation failed: {source}")]
    Confirmation {
        #[source]
        source: PromptError,
    },

    #[error("failed to add funds to the Venmo balance: {source}")]
    Create {
        #[source]
        source: ApiOperationFailure,
    },
}

impl TransferInError {
    #[must_use]
    pub const fn failure_kind(&self) -> ApplicationFailureKind {
        match self {
            Self::Credential(_) => ApplicationFailureKind::Credential,
            Self::CurrentAccount { source }
            | Self::Options { source }
            | Self::Create { source } => ApplicationFailureKind::Api(source.kind()),
            Self::Validation(source) => source.failure_kind(),
            Self::Selection(source) => source.failure_kind(),
            Self::MissingAmountLimit => ApplicationFailureKind::ApiContract,
            Self::AmountTooLarge
            | Self::AmountBelowMinimum { .. }
            | Self::AmountAboveMaximum { .. }
            | Self::ConfirmationRequired => ApplicationFailureKind::Usage,
            Self::ConfirmationDeclined => ApplicationFailureKind::Cancelled,
            Self::Confirmation { source } => prompt_failure_kind(source),
        }
    }
}

#[derive(Debug)]
pub(crate) struct AuthorizedTransferIn(PreparedTransferIn);

pub(crate) async fn prepare<R, A>(
    credentials: &R,
    api: &A,
    amount: Money,
    requested_source: Option<&TransferInstrumentId>,
) -> Result<PreparedTransferIn, TransferInError>
where
    R: CredentialReader,
    A: CurrentAccountApi + TransferOptionsApi,
    <A as CurrentAccountApi>::Error: ApiFailure,
{
    if amount.cents() > i32::MAX as u64 {
        return Err(TransferInError::AmountTooLarge);
    }
    let loaded = require_credential(credentials)?;
    let credential = loaded.envelope;
    let account = api
        .current_account(credential.access_token(), credential.device_id())
        .await
        .map_err(|source| TransferInError::CurrentAccount {
            source: ApiOperationFailure::new(source),
        })?;
    validate_account(&credential, &account)?;
    let options = api
        .transfer_options(credential.access_token(), credential.device_id())
        .await
        .map_err(|source| TransferInError::Options {
            source: ApiOperationFailure::new(source),
        })?;
    let minimum = options
        .add_funds_minimum_cents()
        .ok_or(TransferInError::MissingAmountLimit)?;
    if amount.cents() < minimum {
        return Err(TransferInError::AmountBelowMinimum {
            minimum_cents: minimum,
        });
    }
    if let Some(maximum) = options
        .add_funds_maximum_cents()
        .filter(|maximum| amount.cents() > *maximum)
    {
        return Err(TransferInError::AmountAboveMaximum {
            maximum_cents: maximum,
        });
    }
    let source = selection::select_standard_in(&options, requested_source)?;
    let plan = TransferInPlan::new(account, amount, source);
    Ok(PreparedTransferIn::new(credential, plan))
}

pub(crate) fn authorize<P>(
    prompt: &P,
    prepared: PreparedTransferIn,
    assume_yes: bool,
) -> Result<AuthorizedTransferIn, TransferInError>
where
    P: DefaultNoConfirmation,
{
    confirmation::authorize(
        prompt,
        assume_yes,
        "Add this money to the Venmo balance from the selected bank?",
    )
    .map_err(|error| match error {
        DefaultNoConfirmationError::Required => TransferInError::ConfirmationRequired,
        DefaultNoConfirmationError::Declined => TransferInError::ConfirmationDeclined,
        DefaultNoConfirmationError::Prompt(source) => TransferInError::Confirmation { source },
    })?;
    Ok(AuthorizedTransferIn(prepared))
}

pub(crate) async fn execute<A>(
    api: &A,
    authorized: AuthorizedTransferIn,
) -> Result<TransferInResult, TransferInError>
where
    A: TransferInCreationApi,
{
    let AuthorizedTransferIn(prepared) = authorized;
    let created = api
        .create_transfer_in(
            prepared.credential.access_token(),
            prepared.credential.device_id(),
            &prepared.plan,
        )
        .await
        .map_err(|source| TransferInError::Create {
            source: ApiOperationFailure::new(source),
        })?;
    Ok(TransferInResult::new(prepared.plan, created))
}

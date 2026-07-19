use thiserror::Error;

use super::selection::{self, TransferSelectionError};
use super::{
    CreatedTransfer, TransferOptionsApi, TransferOutCreationApi, TransferOutPlan, TransferSpeed,
};
use crate::features::auth::{CurrentAccountApi, PromptError, prompt_failure_kind};
use crate::features::payments::confirmation::{self, DefaultNoConfirmationError};
use crate::features::payments::{
    DefaultNoConfirmation, FinancialValidationError, validate_account,
};
use crate::features::wallet::BalanceApi;
use crate::shared::{
    ApiFailure, ApiOperationFailure, ApplicationFailureKind, CredentialAccessError,
    CredentialEnvelope, CredentialReader, Money, require_credential,
};

#[derive(Debug)]
pub struct PreparedTransferOut {
    credential: CredentialEnvelope,
    plan: TransferOutPlan,
}

impl PreparedTransferOut {
    #[must_use]
    pub(crate) const fn new(credential: CredentialEnvelope, plan: TransferOutPlan) -> Self {
        Self { credential, plan }
    }

    #[must_use]
    pub const fn plan(&self) -> &TransferOutPlan {
        &self.plan
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct TransferOutResult {
    plan: TransferOutPlan,
    created: CreatedTransfer,
}

impl TransferOutResult {
    #[must_use]
    pub(crate) const fn new(plan: TransferOutPlan, created: CreatedTransfer) -> Self {
        Self { plan, created }
    }

    #[must_use]
    pub const fn plan(&self) -> &TransferOutPlan {
        &self.plan
    }

    #[must_use]
    pub const fn created(&self) -> &CreatedTransfer {
        &self.created
    }
}

#[derive(Debug, Error)]
pub enum TransferOutError {
    #[error(transparent)]
    Credential(#[from] CredentialAccessError),

    #[error("failed to validate the current Venmo account: {source}")]
    CurrentAccount {
        #[source]
        source: ApiOperationFailure,
    },

    #[error(transparent)]
    Validation(#[from] FinancialValidationError),

    #[error("failed to read the Venmo wallet balance: {source}")]
    Balance {
        #[source]
        source: ApiOperationFailure,
    },

    #[error("failed to read Venmo transfer options: {source}")]
    Options {
        #[source]
        source: ApiOperationFailure,
    },

    #[error(transparent)]
    Selection(#[from] TransferSelectionError),

    #[error("the requested transfer exceeds the available Venmo balance")]
    InsufficientBalance,

    #[error("only standard outbound transfers are currently supported")]
    UnsupportedSpeed,

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

    #[error("failed to create the Venmo transfer: {source}")]
    Create {
        #[source]
        source: ApiOperationFailure,
    },
}

impl TransferOutError {
    #[must_use]
    pub const fn failure_kind(&self) -> ApplicationFailureKind {
        match self {
            Self::Credential(_) => ApplicationFailureKind::Credential,
            Self::CurrentAccount { source }
            | Self::Balance { source }
            | Self::Options { source }
            | Self::Create { source } => ApplicationFailureKind::Api(source.kind()),
            Self::Validation(source) => source.failure_kind(),
            Self::Selection(source) => source.failure_kind(),
            Self::InsufficientBalance | Self::UnsupportedSpeed => ApplicationFailureKind::Usage,
            Self::ConfirmationRequired => ApplicationFailureKind::Usage,
            Self::ConfirmationDeclined => ApplicationFailureKind::Cancelled,
            Self::Confirmation { source } => prompt_failure_kind(source),
        }
    }
}

#[derive(Debug)]
pub(crate) struct AuthorizedTransferOut(PreparedTransferOut);

pub(crate) async fn prepare<R, A>(
    credentials: &R,
    api: &A,
    amount: Money,
    speed: TransferSpeed,
) -> Result<PreparedTransferOut, TransferOutError>
where
    R: CredentialReader,
    A: CurrentAccountApi + BalanceApi + TransferOptionsApi,
    <A as CurrentAccountApi>::Error: ApiFailure,
{
    if speed != TransferSpeed::Standard {
        return Err(TransferOutError::UnsupportedSpeed);
    }
    let loaded = require_credential(credentials)?;
    let credential = loaded.envelope;
    let account = api
        .current_account(credential.access_token(), credential.device_id())
        .await
        .map_err(|source| TransferOutError::CurrentAccount {
            source: ApiOperationFailure::new(source),
        })?;
    validate_account(&credential, &account)?;
    let balance = api
        .balance(credential.access_token(), credential.device_id())
        .await
        .map_err(|source| TransferOutError::Balance {
            source: ApiOperationFailure::new(source),
        })?;
    let amount_cents = i128::from(amount.cents());
    if i128::from(balance.available().cents()) < amount_cents {
        return Err(TransferOutError::InsufficientBalance);
    }
    let options = api
        .transfer_options(credential.access_token(), credential.device_id())
        .await
        .map_err(|source| TransferOutError::Options {
            source: ApiOperationFailure::new(source),
        })?;
    let destination = selection::select_standard_out(&options)?;
    let plan = TransferOutPlan::new(account, balance, amount, speed, destination);
    Ok(PreparedTransferOut::new(credential, plan))
}

pub(crate) fn authorize<P>(
    prompt: &P,
    prepared: PreparedTransferOut,
    assume_yes: bool,
) -> Result<AuthorizedTransferOut, TransferOutError>
where
    P: DefaultNoConfirmation,
{
    confirmation::authorize(
        prompt,
        assume_yes,
        "Transfer this balance to the selected bank?",
    )
    .map_err(|error| match error {
        DefaultNoConfirmationError::Required => TransferOutError::ConfirmationRequired,
        DefaultNoConfirmationError::Declined => TransferOutError::ConfirmationDeclined,
        DefaultNoConfirmationError::Prompt(source) => TransferOutError::Confirmation { source },
    })?;
    Ok(AuthorizedTransferOut(prepared))
}

pub(crate) async fn execute<A>(
    api: &A,
    authorized: AuthorizedTransferOut,
) -> Result<TransferOutResult, TransferOutError>
where
    A: TransferOutCreationApi,
{
    let AuthorizedTransferOut(prepared) = authorized;
    let created = api
        .create_transfer_out(
            prepared.credential.access_token(),
            prepared.credential.device_id(),
            &prepared.plan,
        )
        .await
        .map_err(|source| TransferOutError::Create {
            source: ApiOperationFailure::new(source),
        })?;
    Ok(TransferOutResult::new(prepared.plan, created))
}

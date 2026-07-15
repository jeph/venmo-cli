use super::*;

#[derive(Debug, Eq, PartialEq)]
pub(super) enum PayOutcome {
    Success(Box<PayResult>),
    Failure(PayFailure),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PromptFailure {
    Cancelled,
    NotInteractive,
    InvalidInput,
    NoChoices,
    InvalidSelection { index: usize, choice_count: usize },
    Interaction,
}

impl From<&PromptError> for PromptFailure {
    fn from(error: &PromptError) -> Self {
        match error {
            PromptError::Cancelled => Self::Cancelled,
            PromptError::NotInteractive => Self::NotInteractive,
            PromptError::InvalidAccessToken { .. }
            | PromptError::InvalidDeviceId { .. }
            | PromptError::InvalidLoginIdentifier { .. }
            | PromptError::InvalidAccountPassword { .. }
            | PromptError::InvalidOtpCode { .. } => Self::InvalidInput,
            PromptError::NoChoices => Self::NoChoices,
            PromptError::InvalidSelection {
                index,
                choice_count,
            } => Self::InvalidSelection {
                index: *index,
                choice_count: *choice_count,
            },
            PromptError::Interaction { .. } => Self::Interaction,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FundingFailure {
    NoEligibleMethods,
    ExplicitMethodUnavailable,
    DuplicateMethodIds,
    MultipleDefaults,
    ExplicitMethodRequired,
    Prompt(PromptFailure),
}

impl From<&FundingSelectionError> for FundingFailure {
    fn from(error: &FundingSelectionError) -> Self {
        match error {
            FundingSelectionError::NoEligibleMethods => Self::NoEligibleMethods,
            FundingSelectionError::ExplicitMethodUnavailable => Self::ExplicitMethodUnavailable,
            FundingSelectionError::DuplicateMethodIds => Self::DuplicateMethodIds,
            FundingSelectionError::MultipleDefaults => Self::MultipleDefaults,
            FundingSelectionError::ExplicitMethodRequired => Self::ExplicitMethodRequired,
            FundingSelectionError::Prompt { source } => Self::Prompt(PromptFailure::from(source)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PayFailure {
    CredentialMissing,
    CredentialRead,
    CurrentAccount(ApiFailureKind),
    Validation(FinancialValidationError),
    Recipient(RecipientResolutionFailureKind),
    Balance(ApiFailureKind),
    FundingMethods(ApiFailureKind),
    FundingSelection(FundingFailure),
    Eligibility(ApiFailureKind),
    ConfirmationRequired,
    ConfirmationDeclined,
    Confirmation(PromptFailure),
    Create(ApiFailureKind),
}

pub(super) fn pay_outcome(result: Result<PayResult, PayError>) -> PayOutcome {
    match result {
        Ok(result) => PayOutcome::Success(Box::new(result)),
        Err(error) => PayOutcome::Failure(match error {
            PayError::Preflight(PeerPreflightError::Credential(CredentialAccessError::Missing)) => {
                PayFailure::CredentialMissing
            }
            PayError::Preflight(PeerPreflightError::Credential(CredentialAccessError::Read {
                ..
            })) => PayFailure::CredentialRead,
            PayError::Preflight(PeerPreflightError::CurrentAccount { source }) => {
                PayFailure::CurrentAccount(source.kind())
            }
            PayError::Preflight(PeerPreflightError::Validation(source)) => {
                PayFailure::Validation(source)
            }
            PayError::Preflight(PeerPreflightError::Recipient(source)) => {
                PayFailure::Recipient(source.failure_kind())
            }
            PayError::Balance { source } => PayFailure::Balance(source.kind()),
            PayError::FundingMethods { source } => PayFailure::FundingMethods(source.kind()),
            PayError::FundingSelection(source) => {
                PayFailure::FundingSelection(FundingFailure::from(&source))
            }
            PayError::Eligibility { source } => PayFailure::Eligibility(source.kind()),
            PayError::ConfirmationRequired => PayFailure::ConfirmationRequired,
            PayError::ConfirmationDeclined => PayFailure::ConfirmationDeclined,
            PayError::Confirmation { source } => {
                PayFailure::Confirmation(PromptFailure::from(&source))
            }
            PayError::Create { source } => PayFailure::Create(source.kind()),
        }),
    }
}

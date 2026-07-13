use thiserror::Error;

use crate::domain::{Account, CredentialEnvelope, User, UserProfileKind};

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum FinancialValidationError {
    #[error("the current Venmo account does not match the stored credential")]
    AccountMismatch,
    #[error(
        "Venmo does not allow a financial operation with the authenticated account as recipient"
    )]
    SelfRecipient,
    #[error("the recipient response did not prove the profile type")]
    MissingProfileType,
    #[error("the first financial release supports personal Venmo profiles only")]
    UnsupportedProfileType,
    #[error("the recipient response did not prove peer-payment eligibility")]
    MissingPayability,
    #[error("Venmo reports that the recipient is not payable")]
    RecipientNotPayable,
}

pub(crate) fn validate_account(
    credential: &CredentialEnvelope,
    account: &Account,
) -> Result<(), FinancialValidationError> {
    if credential.user_id() != account.user_id() {
        return Err(FinancialValidationError::AccountMismatch);
    }
    Ok(())
}

pub(crate) fn validate_recipient(
    account: &Account,
    recipient: &User,
) -> Result<(), FinancialValidationError> {
    if account.user_id() == recipient.user_id() {
        return Err(FinancialValidationError::SelfRecipient);
    }
    match recipient.profile_kind() {
        None => return Err(FinancialValidationError::MissingProfileType),
        Some(UserProfileKind::Personal) => {}
        Some(UserProfileKind::Business | UserProfileKind::Charity | UserProfileKind::Unknown) => {
            return Err(FinancialValidationError::UnsupportedProfileType);
        }
    }
    match recipient.is_payable() {
        None => Err(FinancialValidationError::MissingPayability),
        Some(false) => Err(FinancialValidationError::RecipientNotPayable),
        Some(true) => Ok(()),
    }
}

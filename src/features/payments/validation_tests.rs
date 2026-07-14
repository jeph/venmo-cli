use std::error::Error;
use std::str::FromStr;

use time::OffsetDateTime;

use super::*;
use crate::shared::{AccessToken, DeviceId, UserId, Username};

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn account_validation_table_compares_whole_results() -> TestResult {
    let credential = CredentialEnvelope::new(
        AccessToken::from_str("synthetic-token")?,
        DeviceId::from_str("synthetic-device")?,
        UserId::from_str("100")?,
        Username::from_bare("owner")?,
        None,
        OffsetDateTime::UNIX_EPOCH,
    );

    for (account, expected) in [
        (account("100", "owner")?, Ok(())),
        (
            account("200", "different")?,
            Err(FinancialValidationError::AccountMismatch),
        ),
    ] {
        let observed = validate_account(&credential, &account);
        assert_eq!(observed, expected);
    }
    Ok(())
}

#[test]
fn recipient_financial_validation_covers_every_fail_closed_variant() -> TestResult {
    let owner = account("100", "owner")?;
    let cases = [
        (
            user("200", "recipient")?
                .with_optional_financial_attributes(Some(UserProfileKind::Personal), Some(true)),
            Ok(()),
        ),
        (
            user("100", "owner")?
                .with_optional_financial_attributes(Some(UserProfileKind::Personal), Some(true)),
            Err(FinancialValidationError::SelfRecipient),
        ),
        (
            user("200", "recipient")?.with_optional_financial_attributes(None, Some(true)),
            Err(FinancialValidationError::MissingProfileType),
        ),
        (
            user("200", "recipient")?
                .with_optional_financial_attributes(Some(UserProfileKind::Business), Some(true)),
            Err(FinancialValidationError::UnsupportedProfileType),
        ),
        (
            user("200", "recipient")?
                .with_optional_financial_attributes(Some(UserProfileKind::Charity), Some(true)),
            Err(FinancialValidationError::UnsupportedProfileType),
        ),
        (
            user("200", "recipient")?
                .with_optional_financial_attributes(Some(UserProfileKind::Unknown), Some(true)),
            Err(FinancialValidationError::UnsupportedProfileType),
        ),
        (
            user("200", "recipient")?
                .with_optional_financial_attributes(Some(UserProfileKind::Personal), None),
            Err(FinancialValidationError::MissingPayability),
        ),
        (
            user("200", "recipient")?
                .with_optional_financial_attributes(Some(UserProfileKind::Personal), Some(false)),
            Err(FinancialValidationError::RecipientNotPayable),
        ),
    ];

    for (recipient, expected) in cases {
        let observed = validate_recipient(&owner, &recipient);
        assert_eq!(observed, expected);
    }
    Ok(())
}

fn account(id: &str, username: &str) -> Result<Account, Box<dyn Error>> {
    Ok(Account::new(
        UserId::from_str(id)?,
        Username::from_bare(username)?,
        None,
    ))
}

fn user(id: &str, username: &str) -> Result<User, Box<dyn Error>> {
    Ok(User::new(
        UserId::from_str(id)?,
        Some(Username::from_bare(username)?),
        None,
    ))
}

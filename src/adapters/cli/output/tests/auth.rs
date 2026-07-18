use std::error::Error;
use std::str::FromStr;

use super::super::write_password_login_report;
use crate::features::auth::{
    DeviceTrustOutcome, LoginDisposition, LoginResult, PasswordLoginReport,
};
use crate::shared::{Account, ApiFailure, ApiFailureKind, ApiOperationFailure, UserId, Username};

type TestResult = Result<(), Box<dyn Error>>;

#[derive(Debug, thiserror::Error)]
#[error("synthetic trust failure")]
struct FakeAuthApiError;

impl ApiFailure for FakeAuthApiError {
    fn kind(&self) -> ApiFailureKind {
        ApiFailureKind::Internal
    }
}

#[test]
fn password_login_output_reports_device_trust_truthfully() -> TestResult {
    let existing = PasswordLoginReport::new(login_result()?, DeviceTrustOutcome::NotNeeded);
    let trusted = PasswordLoginReport::new(login_result()?, DeviceTrustOutcome::Trusted);
    let failed = PasswordLoginReport::new(
        login_result()?,
        DeviceTrustOutcome::Failed(ApiOperationFailure::new(FakeAuthApiError)),
    );
    let mut existing_stdout = Vec::new();
    let mut existing_stderr = Vec::new();
    let mut trusted_stdout = Vec::new();
    let mut trusted_stderr = Vec::new();
    let mut failed_stdout = Vec::new();
    let mut failed_stderr = Vec::new();

    write_password_login_report(&mut existing_stdout, &mut existing_stderr, &existing)?;
    write_password_login_report(&mut trusted_stdout, &mut trusted_stderr, &trusted)?;
    write_password_login_report(&mut failed_stdout, &mut failed_stderr, &failed)?;

    assert_eq!(
        String::from_utf8(existing_stdout)?,
        "Stored Venmo credential for @alice.\nVenmo accepted the existing trusted login device.\n"
    );
    assert_eq!(existing_stderr, b"");
    assert_eq!(
        String::from_utf8(trusted_stdout)?,
        "Stored Venmo credential for @alice.\nTrusted this login device.\n"
    );
    assert_eq!(trusted_stderr, b"");
    assert_eq!(
        String::from_utf8(failed_stdout)?,
        "Stored Venmo credential for @alice.\n"
    );
    let warning = String::from_utf8(failed_stderr)?;
    assert_eq!(
        warning,
        "warning: the validated credential was stored, but Venmo device trust failed: synthetic trust failure; future login may require SMS verification.\n"
    );
    Ok(())
}

#[test]
fn replacement_login_warning_exposes_no_authentication_material() -> TestResult {
    let report = PasswordLoginReport::new(
        LoginResult::new(
            Account::new(
                UserId::from_str("123")?,
                Username::from_bare("alice")?,
                Some("Alice".to_owned()),
            ),
            time::OffsetDateTime::UNIX_EPOCH,
            LoginDisposition::ReplacedExistingCredential,
        ),
        DeviceTrustOutcome::NotNeeded,
    );
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    write_password_login_report(&mut stdout, &mut stderr, &report)?;

    let rendered = String::from_utf8(stdout)?;
    let warning = String::from_utf8(stderr)?;
    assert_eq!(
        rendered,
        "Stored Venmo credential for @alice.\nVenmo accepted the existing trusted login device.\n"
    );
    assert_eq!(
        warning,
        "warning: the previous bearer token was not revoked; use official Venmo session controls if it must be invalidated.\n"
    );
    for secret in [
        "synthetic-password",
        "123456",
        "synthetic-otp-secret",
        "synthetic-issued-token",
        "stored-trusted-device",
    ] {
        assert!(!rendered.contains(secret));
        assert!(!warning.contains(secret));
    }
    Ok(())
}

fn login_result() -> Result<LoginResult, Box<dyn Error>> {
    Ok(LoginResult::new(
        Account::new(
            UserId::from_str("123")?,
            Username::from_bare("alice")?,
            Some("Alice".to_owned()),
        ),
        time::OffsetDateTime::UNIX_EPOCH,
        LoginDisposition::Created,
    ))
}

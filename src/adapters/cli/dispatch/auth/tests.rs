use std::cell::RefCell;
use std::error::Error;
use std::future::{Future, ready};
use std::str::FromStr;

use super::*;
use crate::features::auth::{DeviceTrustOutcome, LoginDisposition};
use crate::shared::{
    AccessToken, Account, ApiFailure, ApiFailureKind, CredentialCapability,
    CredentialDeleteOutcome, CredentialDeleter, CredentialEnvelope, CredentialFailureKind,
    CredentialFormat, CredentialStoreFailure, DeviceId, LoadedCredential, UserId, Username,
};

type TestResult = Result<(), Box<dyn Error>>;

#[derive(Debug, thiserror::Error)]
#[error("synthetic credential failure")]
struct FakeCredentialError;

impl CredentialStoreFailure for FakeCredentialError {
    fn kind(&self) -> CredentialFailureKind {
        CredentialFailureKind::Internal
    }
}

struct FakeStore {
    credential: Option<LoadedCredential>,
    delete: Result<CredentialDeleteOutcome, FakeCredentialError>,
    calls: RefCell<Vec<&'static str>>,
}

impl CredentialCapability for FakeStore {
    type Error = FakeCredentialError;
}

impl CredentialReader for FakeStore {
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        self.calls.borrow_mut().push("read");
        match &self.credential {
            Some(_) => credential().map(Some),
            None => Ok(None),
        }
    }
}

impl CredentialDeleter for FakeStore {
    fn delete_credential(&self) -> Result<CredentialDeleteOutcome, Self::Error> {
        self.calls.borrow_mut().push("delete");
        match &self.delete {
            Ok(outcome) => Ok(*outcome),
            Err(_) => Err(FakeCredentialError),
        }
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("synthetic API failure")]
struct FakeApiError(ApiFailureKind);

impl ApiFailure for FakeApiError {
    fn kind(&self) -> ApiFailureKind {
        self.0
    }
}

struct FakeApi(Result<Account, FakeApiError>);

impl CurrentAccountApi for FakeApi {
    type Error = FakeApiError;

    fn current_account<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a {
        ready(self.0.clone())
    }
}

#[test]
fn replacement_login_output_warns_that_the_previous_remote_token_remains() -> TestResult {
    let report = auth::PasswordLoginReport::new(
        auth::LoginResult::new(
            account()?,
            time::OffsetDateTime::UNIX_EPOCH,
            LoginDisposition::ReplacedExistingCredential,
        ),
        DeviceTrustOutcome::NotNeeded,
    );
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let result = finish_password_login(&mut stdout, &mut stderr, &report);

    assert!(result.is_ok());
    assert_eq!(
        String::from_utf8(stdout)?,
        "Stored Venmo credential for @alice.\nVenmo accepted the existing trusted login device.\n"
    );
    assert_eq!(
        String::from_utf8(stderr)?,
        "warning: the previous bearer token was not revoked; use official Venmo session controls if it must be invalidated.\n"
    );
    Ok(())
}

#[test]
fn logout_only_deletes_locally_and_reports_remote_session_consequences() -> TestResult {
    let store = FakeStore {
        credential: Some(credential()?),
        delete: Ok(CredentialDeleteOutcome::Deleted),
        calls: RefCell::new(Vec::new()),
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let result = run_logout_local_with(&store, &mut stdout, &mut stderr);

    assert!(result.is_ok());
    assert_eq!(store.calls.borrow().as_slice(), ["delete"]);
    assert_eq!(
        String::from_utf8(stdout)?,
        "Removed the local Venmo credential.\n"
    );
    assert_eq!(
        String::from_utf8(stderr)?,
        "warning: local logout does not revoke the remote bearer token; use official Venmo session controls if it must be invalidated.\n"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn auth_status_converts_the_saved_instant_to_the_selected_local_zone() -> TestResult {
    let store = FakeStore {
        credential: Some(credential()?),
        delete: Ok(CredentialDeleteOutcome::Deleted),
        calls: RefCell::new(Vec::new()),
    };
    let api = FakeApi(Ok(account()?));
    let timestamps =
        output::TimestampFormatter::for_time_zone(jiff::tz::TimeZone::fixed(jiff::tz::offset(-8)));
    let mut stdout = Vec::new();

    let result = run_auth_status_with(&store, &api, &timestamps, &mut stdout).await;

    assert!(result.is_ok());
    assert_eq!(store.calls.borrow().as_slice(), ["read"]);
    assert_eq!(
        String::from_utf8(stdout)?,
        concat!(
            "Username: @alice\n",
            "Display name: Alice\n",
            "User ID: 100\n",
            "Saved at: 1969-12-31T16:00:00\n",
            "Credential format: version 1\n",
        )
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn auth_status_preserves_authentication_failure_classification() -> TestResult {
    let store = FakeStore {
        credential: Some(credential()?),
        delete: Ok(CredentialDeleteOutcome::Deleted),
        calls: RefCell::new(Vec::new()),
    };
    let api = FakeApi(Err(FakeApiError(ApiFailureKind::Authentication)));
    let mut stdout = Vec::new();
    let timestamps = output::TimestampFormatter::for_time_zone(jiff::tz::TimeZone::UTC);

    let error = run_auth_status_with(&store, &api, &timestamps, &mut stdout)
        .await
        .err()
        .ok_or("expected auth status failure")?;

    assert_eq!(
        error.category(),
        crate::adapters::cli::error::ErrorCategory::Authentication
    );
    assert!(error.requires_login());
    assert!(stdout.is_empty());
    Ok(())
}

fn account() -> Result<Account, Box<dyn Error>> {
    Ok(Account::new(
        UserId::from_str("100")?,
        Username::from_bare("alice")?,
        Some("Alice".to_owned()),
    ))
}

fn credential() -> Result<LoadedCredential, FakeCredentialError> {
    let internal = || FakeCredentialError;
    Ok(LoadedCredential {
        envelope: CredentialEnvelope::new(
            AccessToken::from_str("synthetic-token").map_err(|_| internal())?,
            DeviceId::from_str("synthetic-device").map_err(|_| internal())?,
            UserId::from_str("100").map_err(|_| internal())?,
            Username::from_bare("alice").map_err(|_| internal())?,
            Some("Alice".to_owned()),
            time::OffsetDateTime::UNIX_EPOCH,
        ),
        format: CredentialFormat::Version1,
    })
}

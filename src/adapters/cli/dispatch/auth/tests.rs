use std::cell::RefCell;
use std::error::Error;
use std::future::{Future, ready};
use std::str::FromStr;

use super::*;
use crate::features::auth::{DeviceTrustOutcome, LoginDisposition};
use crate::shared::ApiOperationFailure;
use crate::shared::{
    AccessToken, Account, ApiFailure, ApiFailureKind, CredentialBackend, CredentialCapability,
    CredentialDeleteOutcome, CredentialDeleter, CredentialEnvelope, CredentialFailureKind,
    CredentialFormat, CredentialStoreFailure, DeviceId, LoadedCredential, UserId, Username,
};

type TestResult = Result<(), Box<dyn Error>>;

fn human_output<'a, W: std::io::Write, E: std::io::Write>(
    command: crate::adapters::cli::CommandId,
    stdout: &'a mut W,
    stderr: &'a mut E,
) -> output::OutputSession<'a, W, E> {
    output::OutputSession::new(
        crate::adapters::cli::OutputFormat::Human,
        command,
        false,
        stdout,
        stderr,
    )
}

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
    backend: CredentialBackend,
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

    fn credential_backend(&self) -> CredentialBackend {
        self.backend
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
    let response = response::password_login(&report)?;
    let mut output = human_output(
        crate::adapters::cli::CommandId::AuthLogin,
        &mut stdout,
        &mut stderr,
    );

    let result = finish_password_login(&mut output, &response);

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
fn json_incomplete_login_is_one_failure_with_a_partial_result() -> TestResult {
    let report = auth::PasswordLoginReport::new(
        auth::LoginResult::new(
            account()?,
            time::OffsetDateTime::UNIX_EPOCH,
            LoginDisposition::Created,
        ),
        DeviceTrustOutcome::Failed(ApiOperationFailure::new(FakeApiError(
            ApiFailureKind::Internal,
        ))),
    );
    let response = response::password_login(&report)?;
    let mut stdout = Vec::new();
    let mut diagnostics = Vec::new();
    let mut output = output::OutputSession::new(
        crate::adapters::cli::OutputFormat::Json,
        crate::adapters::cli::CommandId::AuthLogin,
        false,
        &mut stdout,
        &mut diagnostics,
    );

    let error = match finish_password_login(&mut output, &response) {
        Err(error) => error,
        Ok(()) => return Err("device trust failure should leave login incomplete".into()),
    };
    let failure = output.into_failure(error);
    let mut stderr = Vec::new();
    crate::adapters::cli::write_cli_failure(&mut stderr, &failure)?;

    assert!(stdout.is_empty());
    assert!(diagnostics.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&stderr)?;
    assert_eq!(value["command"], "auth.login");
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "device_trust_incomplete");
    assert_eq!(value["error"]["outcome"], "partial");
    assert_eq!(value["partial_result"]["credential_stored"], true);
    assert_eq!(value["partial_result"]["account"]["username"], "alice");
    assert_eq!(value["partial_result"]["device_trust"]["status"], "failed");
    assert_eq!(
        value["partial_result"]["previous_remote_token_revoked"],
        serde_json::Value::Null
    );
    Ok(())
}

#[test]
fn logout_only_deletes_locally_and_reports_remote_session_consequences() -> TestResult {
    let store = FakeStore {
        credential: Some(credential()?),
        backend: CredentialBackend::Keyring,
        delete: Ok(CredentialDeleteOutcome::Deleted),
        calls: RefCell::new(Vec::new()),
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut output = human_output(
        crate::adapters::cli::CommandId::AuthLogout,
        &mut stdout,
        &mut stderr,
    );

    let result = run_logout_local_with(&store, &mut output);

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
        backend: CredentialBackend::Keyring,
        delete: Ok(CredentialDeleteOutcome::Deleted),
        calls: RefCell::new(Vec::new()),
    };
    let api = FakeApi(Ok(account()?));
    let timestamps =
        output::TimestampFormatter::for_time_zone(jiff::tz::TimeZone::fixed(jiff::tz::offset(-8)));
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut output = human_output(
        crate::adapters::cli::CommandId::AuthStatus,
        &mut stdout,
        &mut stderr,
    );

    let result = run_auth_status_with(&store, &api, &timestamps, &mut output).await;

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
            "Credential store: keyring\n",
        )
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn auth_status_preserves_authentication_failure_classification() -> TestResult {
    let store = FakeStore {
        credential: Some(credential()?),
        backend: CredentialBackend::Keyring,
        delete: Ok(CredentialDeleteOutcome::Deleted),
        calls: RefCell::new(Vec::new()),
    };
    let api = FakeApi(Err(FakeApiError(ApiFailureKind::Authentication)));
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let timestamps = output::TimestampFormatter::for_time_zone(jiff::tz::TimeZone::UTC);
    let mut output = human_output(
        crate::adapters::cli::CommandId::AuthStatus,
        &mut stdout,
        &mut stderr,
    );

    let error = run_auth_status_with(&store, &api, &timestamps, &mut output)
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

#[tokio::test(flavor = "current_thread")]
async fn auth_status_reports_the_backend_that_supplied_the_credential() -> TestResult {
    let store = FakeStore {
        credential: Some(credential()?),
        backend: CredentialBackend::Xdg,
        delete: Ok(CredentialDeleteOutcome::Deleted),
        calls: RefCell::new(Vec::new()),
    };
    let api = FakeApi(Ok(account()?));
    let timestamps = output::TimestampFormatter::for_time_zone(jiff::tz::TimeZone::UTC);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut output = human_output(
        crate::adapters::cli::CommandId::AuthStatus,
        &mut stdout,
        &mut stderr,
    );

    run_auth_status_with(&store, &api, &timestamps, &mut output).await?;

    assert!(String::from_utf8(stdout)?.ends_with("Credential store: xdg\n"));
    Ok(())
}

struct ConfirmationPrompt {
    interactive: bool,
    answer: bool,
    calls: RefCell<Vec<String>>,
}

impl PromptAvailability for ConfirmationPrompt {
    fn can_prompt(&self) -> bool {
        self.interactive
    }
}

impl DefaultNoConfirmation for ConfirmationPrompt {
    fn confirm_default_no(&self, prompt: &str) -> Result<bool, PromptError> {
        self.calls.borrow_mut().push(prompt.to_owned());
        Ok(self.answer)
    }
}

#[test]
fn xdg_login_requires_an_explicit_plaintext_storage_confirmation() -> TestResult {
    let prompt = ConfirmationPrompt {
        interactive: true,
        answer: true,
        calls: RefCell::new(Vec::new()),
    };
    let mut stderr = Vec::new();

    confirm_plaintext_fallback_if_needed(CredentialBackend::Xdg, &prompt, &mut stderr)?;

    assert_eq!(
        prompt.calls.borrow().as_slice(),
        [PLAINTEXT_FALLBACK_CONFIRMATION]
    );
    assert_eq!(
        String::from_utf8(stderr)?,
        "warning: no Secret Service keyring is available; the Venmo bearer token will be stored unencrypted in an owner-only XDG state file.\n"
    );
    Ok(())
}

#[test]
fn keyring_login_never_prompts_for_plaintext_storage() -> TestResult {
    let prompt = ConfirmationPrompt {
        interactive: true,
        answer: false,
        calls: RefCell::new(Vec::new()),
    };
    let mut stderr = Vec::new();

    confirm_plaintext_fallback_if_needed(CredentialBackend::Keyring, &prompt, &mut stderr)?;

    assert!(prompt.calls.borrow().is_empty());
    assert!(stderr.is_empty());
    Ok(())
}

#[test]
fn declining_xdg_plaintext_storage_cancels_before_authentication() -> TestResult {
    let prompt = ConfirmationPrompt {
        interactive: true,
        answer: false,
        calls: RefCell::new(Vec::new()),
    };
    let mut stderr = Vec::new();

    let error = confirm_plaintext_fallback_if_needed(CredentialBackend::Xdg, &prompt, &mut stderr)
        .err()
        .ok_or_else(|| std::io::Error::other("declined fallback was accepted"))?;

    assert_eq!(
        error.category(),
        crate::adapters::cli::error::ErrorCategory::Cancelled
    );
    assert_eq!(prompt.calls.borrow().len(), 1);
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

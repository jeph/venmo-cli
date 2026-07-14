use std::cell::RefCell;
use std::error::Error;
use std::fmt;
use std::future::{Future, ready};
use std::io::{self, Write};
use std::rc::Rc;
use std::str::FromStr;

use super::*;
use crate::adapters::cli::error::ErrorCategory;
use crate::adapters::venmo::TransportBuildError;
use crate::features::auth::{DeviceTrustOutcome, LoginDisposition};
use crate::shared::test_support::Observed;
use crate::shared::{
    AccessToken, Account, ApiFailure, ApiFailureKind, ApiOperationFailure, CredentialCapability,
    CredentialDeleteOutcome, CredentialEnvelope, CredentialFailureKind, CredentialFormat,
    CredentialStoreFailure, DeviceId, LoadedCredential, UserId, Username,
};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;
type Transcript = Rc<RefCell<Vec<AuthCall>>>;

const SENSITIVE_TOKEN: &str = "sensitive-auth-token";
const SENSITIVE_DEVICE: &str = "sensitive-auth-device";
const SENSITIVE_INITIALIZATION_DETAIL: &str = "sensitive runtime initialization detail";

#[derive(Clone, Debug, Eq, PartialEq)]
enum AuthCall {
    LoadCredential,
    DeleteCredential,
    CurrentAccount,
    RevokeAccessToken,
}

#[derive(Clone, Copy)]
enum LoadBehavior {
    Present(CredentialFormat),
    Missing,
    Fail,
}

#[derive(Clone, Copy)]
enum DeleteBehavior {
    Deleted,
    Missing,
    Fail,
}

#[derive(Clone)]
enum CurrentAccountBehavior {
    Succeed(Account),
    Fail(ApiFailureKind),
}

#[derive(Clone, Copy)]
enum RevokeBehavior {
    Succeed,
    Fail(ApiFailureKind),
}

struct AuthSetup {
    load: LoadBehavior,
    delete: DeleteBehavior,
    current_account: CurrentAccountBehavior,
    revoke: RevokeBehavior,
}

impl AuthSetup {
    fn standard() -> TestResult<Self> {
        Ok(Self {
            load: LoadBehavior::Present(CredentialFormat::Version1),
            delete: DeleteBehavior::Deleted,
            current_account: CurrentAccountBehavior::Succeed(account(
                "100",
                "alice",
                Some("Alice"),
            )?),
            revoke: RevokeBehavior::Succeed,
        })
    }
}

struct FakeStore {
    transcript: Transcript,
    load: LoadBehavior,
    delete: DeleteBehavior,
}

#[derive(thiserror::Error)]
#[error("synthetic credential-store failure")]
struct FakeCredentialError;

impl fmt::Debug for FakeCredentialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("sensitive-credential-store-detail")
    }
}

impl CredentialStoreFailure for FakeCredentialError {
    fn kind(&self) -> CredentialFailureKind {
        CredentialFailureKind::Internal
    }
}

impl CredentialCapability for FakeStore {
    type Error = FakeCredentialError;
}

impl CredentialReader for FakeStore {
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        self.transcript.borrow_mut().push(AuthCall::LoadCredential);
        match self.load {
            LoadBehavior::Present(format) => credential(format).map(Some),
            LoadBehavior::Missing => Ok(None),
            LoadBehavior::Fail => Err(FakeCredentialError),
        }
    }
}

impl CredentialDeleter for FakeStore {
    fn delete_credential(&self) -> Result<CredentialDeleteOutcome, Self::Error> {
        delete_fake_credential(&self.transcript, self.delete)
    }
}

struct DeleteOnlyFake {
    transcript: Transcript,
    delete: DeleteBehavior,
}

impl CredentialCapability for DeleteOnlyFake {
    type Error = FakeCredentialError;
}

impl CredentialDeleter for DeleteOnlyFake {
    fn delete_credential(&self) -> Result<CredentialDeleteOutcome, Self::Error> {
        delete_fake_credential(&self.transcript, self.delete)
    }
}

fn delete_fake_credential(
    transcript: &Transcript,
    behavior: DeleteBehavior,
) -> Result<CredentialDeleteOutcome, FakeCredentialError> {
    transcript.borrow_mut().push(AuthCall::DeleteCredential);
    match behavior {
        DeleteBehavior::Deleted => Ok(CredentialDeleteOutcome::Deleted),
        DeleteBehavior::Missing => Ok(CredentialDeleteOutcome::Missing),
        DeleteBehavior::Fail => Err(FakeCredentialError),
    }
}

#[derive(Clone, thiserror::Error)]
#[error("synthetic authentication API failure")]
struct FakeAuthApiError(ApiFailureKind);

impl fmt::Debug for FakeAuthApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("sensitive-authentication-api-detail")
    }
}

impl ApiFailure for FakeAuthApiError {
    fn kind(&self) -> ApiFailureKind {
        self.0
    }
}

struct FakeAuthApi {
    transcript: Transcript,
    current_account: CurrentAccountBehavior,
    revoke: RevokeBehavior,
}

impl CurrentAccountApi for FakeAuthApi {
    type Error = FakeAuthApiError;

    fn current_account<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(AuthCall::CurrentAccount);
        ready(match &self.current_account {
            CurrentAccountBehavior::Succeed(account) => Ok(account.clone()),
            CurrentAccountBehavior::Fail(kind) => Err(FakeAuthApiError(*kind)),
        })
    }
}

impl TokenRevocationApi for FakeAuthApi {
    type Error = FakeAuthApiError;

    fn revoke_access_token<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
        self.transcript
            .borrow_mut()
            .push(AuthCall::RevokeAccessToken);
        ready(match self.revoke {
            RevokeBehavior::Succeed => Ok(()),
            RevokeBehavior::Fail(kind) => Err(FakeAuthApiError(kind)),
        })
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct WriterState {
    bytes: Vec<u8>,
    flush_count: u32,
    fail_write: bool,
    fail_flush: bool,
}

struct RecordingWriter {
    state: WriterState,
}

impl RecordingWriter {
    const fn new(state: WriterState) -> Self {
        Self { state }
    }
}

impl Write for RecordingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if self.state.fail_write {
            return Err(io::Error::other("synthetic write failure"));
        }
        self.state.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.state.flush_count += 1;
        if self.state.fail_flush {
            Err(io::Error::other("synthetic flush failure"))
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
struct AuthState {
    calls: Vec<AuthCall>,
    stdout: WriterState,
    stderr: WriterState,
}

struct AuthHarness {
    transcript: Transcript,
    store: FakeStore,
    api: FakeAuthApi,
    stdout: RecordingWriter,
    stderr: RecordingWriter,
}

impl AuthHarness {
    fn new(setup: AuthSetup, initial: AuthState) -> Self {
        let transcript = Rc::new(RefCell::new(initial.calls));
        Self {
            store: FakeStore {
                transcript: Rc::clone(&transcript),
                load: setup.load,
                delete: setup.delete,
            },
            api: FakeAuthApi {
                transcript: Rc::clone(&transcript),
                current_account: setup.current_account,
                revoke: setup.revoke,
            },
            transcript,
            stdout: RecordingWriter::new(initial.stdout),
            stderr: RecordingWriter::new(initial.stderr),
        }
    }

    fn observed(self, result: Result<(), AppError>) -> Observed<ResultSnapshot, AuthState> {
        let state = AuthState {
            calls: self.transcript.borrow().clone(),
            stdout: self.stdout.state,
            stderr: self.stderr.state,
        };
        Observed::new(snapshot_result(result), state)
    }

    fn output_text(&self) -> String {
        let mut bytes = self.stdout.state.bytes.clone();
        bytes.extend_from_slice(&self.stderr.state.bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ErrorVariant {
    AuthLoginIncomplete,
    AuthLogoutIncomplete,
    AuthStateOutput,
    AuthStatus,
    Unexpected,
}

#[derive(Debug, Eq, PartialEq)]
enum ResultSnapshot {
    Success,
    Failure {
        variant: ErrorVariant,
        category: ErrorCategory,
        exit_code: u8,
        message: String,
    },
}

#[test]
fn remote_initialization_failures_delete_locally_with_complete_redacted_outcomes() -> TestResult {
    for (failure, expected_reason) in [
        (
            AppError::RuntimeInitialization {
                source: io::Error::other(SENSITIVE_INITIALIZATION_DETAIL),
            },
            "failed to initialize the asynchronous runtime",
        ),
        (
            AppError::ApiInitialization {
                source: TransportBuildError::ClientInitialization,
            },
            "failed to initialize the Venmo API client",
        ),
    ] {
        let setup = AuthSetup::standard()?;
        let initial_state = AuthState::default();
        let expected = Observed::new(
            failure_snapshot(
                ErrorVariant::AuthLogoutIncomplete,
                ErrorCategory::Internal,
                "authentication logout was incomplete; review the reported results",
            ),
            AuthState {
                calls: vec![AuthCall::DeleteCredential],
                stdout: writer_state("Removed the local Venmo credential.\n", 1),
                stderr: writer_state(
                    &format!(
                        "warning: remote token revocation was not attempted: {expected_reason}; remote token state is unknown.\n"
                    ),
                    1,
                ),
            },
        );
        let mut harness = AuthHarness::new(setup, initial_state);

        let result = finish_logout_after_remote_initialization_failure(
            &harness.store,
            &mut harness.stdout,
            &mut harness.stderr,
            failure,
        );
        let output = harness.output_text();
        let observed = harness.observed(result);

        assert_eq!(observed, expected);
        assert_output_excludes_sensitive_material(&output);
    }
    Ok(())
}

#[test]
fn mutating_auth_finishers_have_exact_output_and_flush_state() -> TestResult {
    let login = login_result()?;
    let setup = AuthSetup::standard()?;
    let initial_state = AuthState::default();
    let expected = Observed::new(
        ResultSnapshot::Success,
        AuthState {
            stdout: writer_state("Stored Venmo credential for @alice.\n", 1),
            stderr: writer_state("", 1),
            ..AuthState::default()
        },
    );
    let mut harness = AuthHarness::new(setup, initial_state);

    let result = finish_token_login(&mut harness.stdout, &mut harness.stderr, &login);
    let observed = harness.observed(result);

    assert_eq!(observed, expected);

    for (report, reauthentication, expected_stdout, expected_stderr) in [
        (
            auth::PasswordLoginReport::new(login_result()?, DeviceTrustOutcome::NotNeeded),
            false,
            "Stored Venmo credential for @alice.\nVenmo accepted the existing trusted login device.\n",
            "",
        ),
        (
            auth::PasswordLoginReport::new(login_result()?, DeviceTrustOutcome::Trusted),
            false,
            "Stored Venmo credential for @alice.\nTrusted this login device.\n",
            "",
        ),
        (
            auth::PasswordLoginReport::new(login_result()?, DeviceTrustOutcome::NotNeeded),
            true,
            "Stored Venmo credential for @alice.\nVenmo accepted the existing trusted login device.\n",
            "warning: reauthentication does not revoke the previous bearer token; its remote validity is unknown, so use official Venmo session controls if it must be invalidated.\n",
        ),
    ] {
        let setup = AuthSetup::standard()?;
        let initial_state = AuthState::default();
        let expected = Observed::new(
            ResultSnapshot::Success,
            AuthState {
                stdout: writer_state(expected_stdout, 1),
                stderr: writer_state(expected_stderr, 1),
                ..AuthState::default()
            },
        );
        let mut harness = AuthHarness::new(setup, initial_state);

        let result = if reauthentication {
            finish_reauthentication(&mut harness.stdout, &mut harness.stderr, &report)
        } else {
            finish_password_login(&mut harness.stdout, &mut harness.stderr, &report)
        };
        let observed = harness.observed(result);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn auth_status_renders_each_credential_format_exactly_and_flushes() -> TestResult {
    for (format, returned_account, expected_stdout) in [
        (
            CredentialFormat::Version1,
            account("100", "alice", Some("Alice\n\u{1b}[31m"))?,
            "Username: @alice\nDisplay name: Alice\\n\\u{001B}[31m\nUser ID: 100\nSaved at: 1970-01-01T00:00:00Z\nCredential format: version 1\n",
        ),
        (
            CredentialFormat::LegacyTypeScript,
            account("100", "alice", None)?,
            "Username: @alice\nDisplay name: (not provided)\nUser ID: 100\nSaved at: 1970-01-01T00:00:00Z\nCredential format: legacy TypeScript\n",
        ),
    ] {
        let setup = AuthSetup {
            load: LoadBehavior::Present(format),
            current_account: CurrentAccountBehavior::Succeed(returned_account),
            ..AuthSetup::standard()?
        };
        let initial_state = AuthState::default();
        let expected = Observed::new(
            ResultSnapshot::Success,
            AuthState {
                calls: vec![AuthCall::LoadCredential, AuthCall::CurrentAccount],
                stdout: writer_state(expected_stdout, 1),
                ..AuthState::default()
            },
        );
        let mut harness = AuthHarness::new(setup, initial_state);

        let result = run_auth_status_with(&harness.store, &harness.api, &mut harness.stdout).await;
        let output = harness.output_text();
        let observed = harness.observed(result);

        assert_eq!(observed, expected);
        assert_output_excludes_sensitive_material(&output);
        assert!(!output.contains('\u{1b}'));
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn auth_status_failure_has_no_partial_output_or_flush() -> TestResult {
    let setup = AuthSetup {
        current_account: CurrentAccountBehavior::Fail(ApiFailureKind::Internal),
        ..AuthSetup::standard()?
    };
    let initial_state = AuthState::default();
    let expected = Observed::new(
        failure_snapshot(
            ErrorVariant::AuthStatus,
            ErrorCategory::Internal,
            "the stored bearer token could not be validated: synthetic authentication API failure",
        ),
        AuthState {
            calls: vec![AuthCall::LoadCredential, AuthCall::CurrentAccount],
            ..AuthState::default()
        },
    );
    let mut harness = AuthHarness::new(setup, initial_state);

    let result = run_auth_status_with(&harness.store, &harness.api, &mut harness.stdout).await;
    let output = harness.output_text();
    let observed = harness.observed(result);

    assert_eq!(observed, expected);
    assert_output_excludes_sensitive_material(&output);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn logout_outputs_cover_every_remote_and_local_outcome() -> TestResult {
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let store = DeleteOnlyFake {
        transcript: Rc::clone(&transcript),
        delete: DeleteBehavior::Missing,
    };
    let mut stdout = RecordingWriter::new(WriterState::default());
    let mut stderr = RecordingWriter::new(WriterState::default());
    let expected = Observed::new(
        ResultSnapshot::Success,
        AuthState {
            calls: vec![AuthCall::DeleteCredential],
            stdout: writer_state("No local Venmo credential was stored.\n", 1),
            stderr: writer_state("", 1),
        },
    );

    let result = run_logout_local_with(&store, &mut stdout, &mut stderr);
    let observed = Observed::new(
        snapshot_result(result),
        AuthState {
            calls: transcript.borrow().clone(),
            stdout: stdout.state,
            stderr: stderr.state,
        },
    );

    assert_eq!(observed, expected);

    let setup = AuthSetup::standard()?;
    let initial_state = AuthState::default();
    let expected = Observed::new(
        ResultSnapshot::Success,
        AuthState {
            calls: vec![
                AuthCall::LoadCredential,
                AuthCall::RevokeAccessToken,
                AuthCall::DeleteCredential,
            ],
            stdout: writer_state(
                "Revoked the remote Venmo token.\nRemoved the local Venmo credential.\n",
                1,
            ),
            stderr: writer_state("", 1),
        },
    );
    let mut harness = AuthHarness::new(setup, initial_state);

    let result = run_revoke_logout_with(
        &harness.store,
        &harness.api,
        &mut harness.stdout,
        &mut harness.stderr,
    )
    .await;
    let observed = harness.observed(result);

    assert_eq!(observed, expected);

    let setup = AuthSetup {
        revoke: RevokeBehavior::Fail(ApiFailureKind::Timeout),
        delete: DeleteBehavior::Fail,
        ..AuthSetup::standard()?
    };
    let initial_state = AuthState::default();
    let expected = Observed::new(
        failure_snapshot(
            ErrorVariant::AuthLogoutIncomplete,
            ErrorCategory::Timeout,
            "authentication logout was incomplete; review the reported results",
        ),
        AuthState {
            calls: vec![
                AuthCall::LoadCredential,
                AuthCall::RevokeAccessToken,
                AuthCall::DeleteCredential,
            ],
            stdout: writer_state("", 1),
            stderr: writer_state(
                "warning: remote token revocation failed: synthetic authentication API failure; the token may still be valid.\nerror: local credential deletion failed: synthetic credential-store failure; the local entry may remain.\n",
                1,
            ),
        },
    );
    let mut harness = AuthHarness::new(setup, initial_state);

    let result = run_revoke_logout_with(
        &harness.store,
        &harness.api,
        &mut harness.stdout,
        &mut harness.stderr,
    )
    .await;
    let output = harness.output_text();
    let observed = harness.observed(result);

    assert_eq!(observed, expected);
    assert_output_excludes_sensitive_material(&output);

    let setup = AuthSetup {
        load: LoadBehavior::Fail,
        ..AuthSetup::standard()?
    };
    let initial_state = AuthState::default();
    let expected = Observed::new(
        failure_snapshot(
            ErrorVariant::AuthLogoutIncomplete,
            ErrorCategory::Credential,
            "authentication logout was incomplete; review the reported results",
        ),
        AuthState {
            calls: vec![AuthCall::LoadCredential, AuthCall::DeleteCredential],
            stdout: writer_state("Removed the local Venmo credential.\n", 1),
            stderr: writer_state(
                "warning: remote token revocation was not attempted: synthetic credential-store failure; remote token state is unknown.\n",
                1,
            ),
        },
    );
    let mut harness = AuthHarness::new(setup, initial_state);

    let result = run_revoke_logout_with(
        &harness.store,
        &harness.api,
        &mut harness.stdout,
        &mut harness.stderr,
    )
    .await;
    let output = harness.output_text();
    let observed = harness.observed(result);

    assert_eq!(observed, expected);
    assert_output_excludes_sensitive_material(&output);

    let setup = AuthSetup {
        load: LoadBehavior::Missing,
        delete: DeleteBehavior::Missing,
        ..AuthSetup::standard()?
    };
    let initial_state = AuthState::default();
    let expected = Observed::new(
        ResultSnapshot::Success,
        AuthState {
            calls: vec![AuthCall::LoadCredential, AuthCall::DeleteCredential],
            stdout: writer_state("No local Venmo credential was stored.\n", 1),
            stderr: writer_state("", 1),
        },
    );
    let mut harness = AuthHarness::new(setup, initial_state);

    let result = run_revoke_logout_with(
        &harness.store,
        &harness.api,
        &mut harness.stdout,
        &mut harness.stderr,
    )
    .await;
    let observed = harness.observed(result);

    assert_eq!(observed, expected);
    Ok(())
}

#[test]
fn auth_write_and_flush_failures_have_complete_state_aware_outcomes() -> TestResult {
    for (stdout_state, stderr_state, expected_stdout, expected_stderr) in [
        (
            WriterState {
                fail_write: true,
                ..WriterState::default()
            },
            WriterState::default(),
            WriterState {
                fail_write: true,
                ..WriterState::default()
            },
            WriterState::default(),
        ),
        (
            WriterState::default(),
            WriterState {
                fail_write: true,
                ..WriterState::default()
            },
            writer_state("stdout result", 0),
            WriterState {
                fail_write: true,
                ..WriterState::default()
            },
        ),
        (
            WriterState {
                fail_flush: true,
                ..WriterState::default()
            },
            WriterState::default(),
            WriterState {
                bytes: b"stdout result".to_vec(),
                flush_count: 1,
                fail_flush: true,
                ..WriterState::default()
            },
            writer_state("stderr result", 1),
        ),
        (
            WriterState::default(),
            WriterState {
                fail_flush: true,
                ..WriterState::default()
            },
            writer_state("stdout result", 1),
            WriterState {
                bytes: b"stderr result".to_vec(),
                flush_count: 1,
                fail_flush: true,
                ..WriterState::default()
            },
        ),
    ] {
        let setup = AuthSetup::standard()?;
        let initial_state = AuthState {
            stdout: stdout_state,
            stderr: stderr_state,
            ..AuthState::default()
        };
        let expected = Observed::new(
            failure_snapshot(
                ErrorVariant::AuthStateOutput,
                ErrorCategory::Internal,
                "authentication state may already have changed, but the command result could not be written completely; verify local state with `venmo auth status` and review official Venmo session controls before retrying",
            ),
            AuthState {
                stdout: expected_stdout,
                stderr: expected_stderr,
                ..AuthState::default()
            },
        );
        let mut harness = AuthHarness::new(setup, initial_state);

        let result = write_and_flush_auth_output(
            &mut harness.stdout,
            &mut harness.stderr,
            |stdout, stderr| {
                stdout.write_all(b"stdout result")?;
                stderr.write_all(b"stderr result")
            },
        );
        let observed = harness.observed(result);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[test]
fn incomplete_auth_report_is_flushed_before_its_exact_error() -> TestResult {
    let report = auth::PasswordLoginReport::new(
        login_result()?,
        DeviceTrustOutcome::Failed(ApiOperationFailure::new(FakeAuthApiError(
            ApiFailureKind::Internal,
        ))),
    );
    let setup = AuthSetup::standard()?;
    let initial_state = AuthState::default();
    let expected = Observed::new(
        failure_snapshot(
            ErrorVariant::AuthLoginIncomplete,
            ErrorCategory::Internal,
            "authentication succeeded, but device trust was incomplete; review the reported results",
        ),
        AuthState {
            stdout: writer_state("Stored Venmo credential for @alice.\n", 1),
            stderr: writer_state(
                "warning: the validated credential was stored, but Venmo device trust failed: synthetic authentication API failure; future login may require SMS verification.\n",
                1,
            ),
            ..AuthState::default()
        },
    );
    let mut harness = AuthHarness::new(setup, initial_state);

    let result = finish_password_login(&mut harness.stdout, &mut harness.stderr, &report);
    let output = harness.output_text();
    let observed = harness.observed(result);

    assert_eq!(observed, expected);
    assert_output_excludes_sensitive_material(&output);
    Ok(())
}

fn snapshot_result(result: Result<(), AppError>) -> ResultSnapshot {
    match result {
        Ok(()) => ResultSnapshot::Success,
        Err(error) => ResultSnapshot::Failure {
            variant: match &error {
                AppError::AuthLoginIncomplete { .. } => ErrorVariant::AuthLoginIncomplete,
                AppError::AuthLogoutIncomplete { .. } => ErrorVariant::AuthLogoutIncomplete,
                AppError::AuthStateOutput { .. } => ErrorVariant::AuthStateOutput,
                AppError::AuthStatus { .. } => ErrorVariant::AuthStatus,
                _ => ErrorVariant::Unexpected,
            },
            category: error.category(),
            exit_code: error.exit_code(),
            message: error.to_string(),
        },
    }
}

fn failure_snapshot(
    variant: ErrorVariant,
    category: ErrorCategory,
    message: &str,
) -> ResultSnapshot {
    ResultSnapshot::Failure {
        variant,
        category,
        exit_code: category.exit_code(),
        message: message.to_owned(),
    }
}

fn writer_state(text: &str, flush_count: u32) -> WriterState {
    WriterState {
        bytes: text.as_bytes().to_vec(),
        flush_count,
        ..WriterState::default()
    }
}

fn assert_output_excludes_sensitive_material(rendered: &str) {
    for sensitive in [
        SENSITIVE_TOKEN,
        SENSITIVE_DEVICE,
        SENSITIVE_INITIALIZATION_DETAIL,
        "sensitive-credential-store-detail",
        "sensitive-authentication-api-detail",
    ] {
        assert!(!rendered.contains(sensitive));
    }
}

fn login_result() -> Result<auth::LoginResult, Box<dyn Error>> {
    Ok(auth::LoginResult::new(
        account("100", "alice", Some("Alice"))?,
        time::OffsetDateTime::UNIX_EPOCH,
        LoginDisposition::Created,
    ))
}

fn account(
    id: &str,
    username: &str,
    display_name: Option<&str>,
) -> Result<Account, Box<dyn Error>> {
    Ok(Account::new(
        UserId::from_str(id)?,
        Username::from_bare(username)?,
        display_name.map(str::to_owned),
    ))
}

fn credential(format: CredentialFormat) -> Result<LoadedCredential, FakeCredentialError> {
    Ok(LoadedCredential {
        envelope: CredentialEnvelope::new(
            AccessToken::from_str(SENSITIVE_TOKEN).map_err(|_| FakeCredentialError)?,
            DeviceId::from_str(SENSITIVE_DEVICE).map_err(|_| FakeCredentialError)?,
            UserId::from_str("100").map_err(|_| FakeCredentialError)?,
            Username::from_bare("alice").map_err(|_| FakeCredentialError)?,
            Some("Stored Alice".to_owned()),
            time::OffsetDateTime::UNIX_EPOCH,
        ),
        format,
    })
}

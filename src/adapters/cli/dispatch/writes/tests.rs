use std::cell::RefCell;
use std::error::Error;
use std::fmt;
use std::future::{Future, pending, ready};
use std::io::{self, Write};
use std::pin::Pin;
use std::rc::Rc;
use std::str::FromStr;

use clap::Parser;

use super::*;
use crate::adapters::cli::error::ErrorCategory;
use crate::features::auth::{OtpCode, PromptAvailability, PromptError};
use crate::features::p2p_step_up::{P2pOtpVerification, P2pStepUpApi, P2pStepUpInput};
use crate::features::people::{
    User, UserProfileKind, UserSearchPage, UserSearchPageRequest, UserSearchQuery,
};
use crate::features::requests::{
    CreatedRequest, RequestCreationOutcome, RequestCreationVerification, RequestId, RequestStatus,
};
use crate::shared::test_support::Observed;
use crate::shared::{
    AccessToken, Account, ApiFailureKind, CredentialCapability, CredentialEnvelope,
    CredentialFailureKind, CredentialFormat, CredentialStoreFailure, DeviceId, LoadedCredential,
    UserId, Username, Visibility,
};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;
type Transcript = Rc<RefCell<Vec<RequestCall>>>;
type Interruption = Pin<Box<dyn Future<Output = Result<(), AppError>>>>;

#[derive(Clone, Debug, Eq, PartialEq)]
enum RequestCall {
    ReadCredential,
    CurrentAccount,
    UserById {
        user_id: String,
    },
    SearchUsers,
    InstallInterruption,
    CreateRequest {
        client_request_id: String,
        account_user_id: String,
        recipient_user_id: String,
        amount_cents: u64,
        visibility: Visibility,
    },
}

#[derive(Debug, thiserror::Error)]
#[error("synthetic credential-store failure")]
struct FakeCredentialError;

impl CredentialStoreFailure for FakeCredentialError {
    fn kind(&self) -> CredentialFailureKind {
        CredentialFailureKind::Internal
    }
}

struct FakeRequestReader {
    transcript: Transcript,
    credential_present: bool,
}

impl CredentialCapability for FakeRequestReader {
    type Error = FakeCredentialError;
}

impl CredentialReader for FakeRequestReader {
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        self.transcript
            .borrow_mut()
            .push(RequestCall::ReadCredential);
        if self.credential_present {
            request_credential().map(Some)
        } else {
            Ok(None)
        }
    }
}

#[derive(Clone, thiserror::Error)]
#[error("synthetic request API failure")]
struct FakeRequestApiError;

impl fmt::Debug for FakeRequestApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("sensitive-request-api-detail")
    }
}

impl ApiFailure for FakeRequestApiError {
    fn kind(&self) -> ApiFailureKind {
        ApiFailureKind::Internal
    }
}

#[derive(Clone)]
enum RequestWriteBehavior {
    Complete(CreatedRequest),
    Pending,
}

struct FakeRequestApi {
    transcript: Transcript,
    account: Account,
    recipient: User,
    write_behavior: RequestWriteBehavior,
}

impl CurrentAccountApi for FakeRequestApi {
    type Error = FakeRequestApiError;

    fn current_account<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a {
        self.transcript
            .borrow_mut()
            .push(RequestCall::CurrentAccount);
        ready(Ok(self.account.clone()))
    }
}

impl UserLookupApi for FakeRequestApi {
    type Error = FakeRequestApiError;

    fn user_by_id<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        user_id: &'a UserId,
    ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(RequestCall::UserById {
            user_id: user_id.to_string(),
        });
        ready(Ok(self.recipient.clone()))
    }
}

impl UserSearchApi for FakeRequestApi {
    type Error = FakeRequestApiError;

    fn search_users<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        _query: &'a UserSearchQuery,
        _page: UserSearchPageRequest,
    ) -> impl Future<Output = Result<UserSearchPage, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(RequestCall::SearchUsers);
        ready(Ok(UserSearchPage::new(vec![self.recipient.clone()], None)))
    }
}

impl RequestCreationApi for FakeRequestApi {
    type Error = FakeRequestApiError;

    fn create_request<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        plan: &'a crate::features::requests::CreateRequestPlan,
        _verification: RequestCreationVerification,
    ) -> impl Future<Output = Result<RequestCreationOutcome, Self::Error>> + Send + 'a {
        self.transcript
            .borrow_mut()
            .push(RequestCall::CreateRequest {
                client_request_id: plan.request_id().to_string(),
                account_user_id: plan.account().user_id().to_string(),
                recipient_user_id: plan.recipient().user_id().to_string(),
                amount_cents: plan.amount().cents(),
                visibility: plan.visibility(),
            });
        let behavior = self.write_behavior.clone();
        async move {
            match behavior {
                RequestWriteBehavior::Complete(created) => {
                    Ok(RequestCreationOutcome::Created(created))
                }
                RequestWriteBehavior::Pending => pending().await,
            }
        }
    }
}

impl P2pStepUpApi for FakeRequestApi {
    type Error = FakeRequestApiError;

    fn issue_p2p_otp<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        _session_id: &'a crate::shared::ClientRequestId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
        ready(Err(FakeRequestApiError))
    }

    fn verify_p2p_otp<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        _session_id: &'a crate::shared::ClientRequestId,
        _otp: &'a OtpCode,
    ) -> impl Future<Output = Result<P2pOtpVerification, Self::Error>> + Send + 'a {
        ready(Err(FakeRequestApiError))
    }
}

struct FixedRequestIdGenerator;

impl ClientRequestIdGenerator for FixedRequestIdGenerator {
    fn generate(&self) -> crate::shared::ClientRequestId {
        crate::shared::ClientRequestId::from_str("123e4567-e89b-12d3-a456-426614174000")
            .unwrap_or_else(|_| crate::shared::ClientRequestId::generate())
    }
}

struct UnusedRequestPrompt;

impl PromptAvailability for UnusedRequestPrompt {
    fn can_prompt(&self) -> bool {
        false
    }
}

impl DefaultNoConfirmation for UnusedRequestPrompt {
    fn confirm_default_no(&self, _prompt: &str) -> Result<bool, PromptError> {
        Err(PromptError::Interaction {
            source: io::Error::other("unexpected request-creation prompt"),
        })
    }
}

impl P2pStepUpInput for UnusedRequestPrompt {
    fn read_p2p_otp(&self, _prompt: &str) -> Result<OtpCode, PromptError> {
        Err(PromptError::Interaction {
            source: io::Error::other("unexpected request-creation OTP prompt"),
        })
    }
}

#[derive(Clone, Copy)]
enum InterruptionBehavior {
    Pending,
    Ready,
    FutureFailure,
    InstallationFailure,
}

struct RequestSetup {
    args: RequestArgs,
    credential_present: bool,
    write_behavior: RequestWriteBehavior,
    interruption: InterruptionBehavior,
}

impl RequestSetup {
    fn successful() -> TestResult<Self> {
        Ok(Self {
            args: request_args()?,
            credential_present: true,
            write_behavior: RequestWriteBehavior::Complete(created_request()?),
            interruption: InterruptionBehavior::Pending,
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
struct RequestState {
    calls: Vec<RequestCall>,
    stdout: WriterState,
}

struct RequestHarness {
    args: RequestArgs,
    transcript: Transcript,
    reader: FakeRequestReader,
    api: FakeRequestApi,
    interruption: InterruptionBehavior,
    stdout: RecordingWriter,
}

impl RequestHarness {
    fn new(setup: RequestSetup, initial: RequestState) -> TestResult<Self> {
        let transcript = Rc::new(RefCell::new(initial.calls));
        Ok(Self {
            args: setup.args,
            reader: FakeRequestReader {
                transcript: Rc::clone(&transcript),
                credential_present: setup.credential_present,
            },
            api: FakeRequestApi {
                transcript: Rc::clone(&transcript),
                account: account()?,
                recipient: recipient()?,
                write_behavior: setup.write_behavior,
            },
            transcript,
            interruption: setup.interruption,
            stdout: RecordingWriter::new(initial.stdout),
        })
    }

    async fn execute(&mut self) -> Result<(), AppError> {
        let mut stderr = Vec::new();
        self.execute_with_stderr(&mut stderr).await
    }

    async fn execute_with_stderr<E: Write>(&mut self, stderr: &mut E) -> Result<(), AppError> {
        self.execute_with_format(stderr, super::super::OutputFormat::Human)
            .await
    }

    async fn execute_with_format<E: Write>(
        &mut self,
        stderr: &mut E,
        format: super::super::OutputFormat,
    ) -> Result<(), AppError> {
        let transcript = Rc::clone(&self.transcript);
        let interruption = self.interruption;
        let mut output = output::OutputSession::new(
            format,
            crate::adapters::cli::CommandId::RequestsCreate,
            false,
            &mut self.stdout,
            stderr,
        );
        run_request_create(
            self.args.clone(),
            &self.reader,
            &self.api,
            &FixedRequestIdGenerator,
            &UnusedRequestPrompt,
            &mut output,
            move || {
                transcript
                    .borrow_mut()
                    .push(RequestCall::InstallInterruption);
                match interruption {
                    InterruptionBehavior::Pending => Ok(Box::pin(pending()) as Interruption),
                    InterruptionBehavior::Ready => Ok(Box::pin(ready(Ok(()))) as Interruption),
                    InterruptionBehavior::FutureFailure => Ok(Box::pin(ready(Err(
                        signal_initialization_error("synthetic signal stream failure"),
                    )))
                        as Interruption),
                    InterruptionBehavior::InstallationFailure => Err(signal_initialization_error(
                        "synthetic signal installation failure",
                    )),
                }
            },
        )
        .await
    }

    fn observed(self, result: Result<(), AppError>) -> Observed<ResultSnapshot, RequestState> {
        let state = RequestState {
            calls: self.transcript.borrow().clone(),
            stdout: self.stdout.state,
        };
        Observed::new(snapshot_result(result), state)
    }

    fn output_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout.state.bytes).into_owned()
    }
}

#[tokio::test(flavor = "current_thread")]
async fn request_json_completed_and_dry_run_outputs_are_safe_and_explicit() -> TestResult {
    let mut completed = RequestHarness::new(RequestSetup::successful()?, RequestState::default())?;
    let mut completed_stderr = Vec::new();
    completed
        .execute_with_format(&mut completed_stderr, super::super::OutputFormat::Json)
        .await?;
    assert!(completed_stderr.is_empty());
    let completed_text = String::from_utf8(completed.stdout.state.bytes.clone())?;
    assert!(!completed_text.contains("synthetic-token"));
    assert!(!completed_text.contains("synthetic-device"));
    let completed_json: serde_json::Value = serde_json::from_str(&completed_text)?;
    assert_eq!(completed_json["command"], "requests.create");
    assert_eq!(completed_json["ok"], true);
    assert_eq!(completed_json["data"]["outcome"], "completed");
    assert_eq!(completed_json["data"]["performed"], true);
    assert_eq!(completed_json["data"]["plan"]["amount"]["amount"], "0.01");
    assert_eq!(completed_json["data"]["plan"]["amount"]["currency"], "USD");
    assert_eq!(completed_json["data"]["plan"]["visibility"], "private");
    assert_eq!(completed_json["data"]["plan"]["automatic_retries"], false);
    assert_eq!(completed_json["data"]["result"]["id"], "request-1");

    let mut dry_setup = RequestSetup::successful()?;
    dry_setup.args.yes = false;
    dry_setup.args.dry_run = true;
    let mut dry_run = RequestHarness::new(dry_setup, RequestState::default())?;
    let mut dry_stderr = Vec::new();
    dry_run
        .execute_with_format(&mut dry_stderr, super::super::OutputFormat::Json)
        .await?;
    assert!(dry_stderr.is_empty());
    assert!(!dry_run.transcript.borrow().iter().any(|call| matches!(
        call,
        RequestCall::InstallInterruption | RequestCall::CreateRequest { .. }
    )));
    let dry_json: serde_json::Value = serde_json::from_slice(&dry_run.stdout.state.bytes)?;
    assert_eq!(dry_json["command"], "requests.create");
    assert_eq!(dry_json["data"]["outcome"], "dry_run");
    assert_eq!(dry_json["data"]["performed"], false);
    assert_eq!(dry_json["data"]["result"], serde_json::Value::Null);
    assert_eq!(dry_json["data"]["plan"]["amount"]["amount"], "0.01");
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ErrorVariant {
    Pay,
    RequestCreate,
    Accept,
    Decline,
    FinancialWriteInterruptedUnknown,
    StateWriteInterruptedUnknown,
    SignalInitialization,
    StateSignalInitialization,
    FinancialResultOutput,
    CommandOutput,
    Unexpected,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) enum ResultSnapshot {
    Success,
    Failure {
        variant: ErrorVariant,
        category: ErrorCategory,
        exit_code: u8,
        message: String,
    },
}

#[tokio::test(flavor = "current_thread")]
async fn request_handler_success_has_one_typed_write_and_complete_output_state() -> TestResult {
    let setup = RequestSetup::successful()?;
    let initial_state = RequestState::default();
    let expected = Observed::new(
        ResultSnapshot::Success,
        RequestState {
            calls: successful_calls(true),
            stdout: writer_state(
                "Request ID: request-1\nStatus: pending\nRequested from: @recipient (Synthetic recipient)\nAmount: $0.01\nRequested audience: private\n",
                1,
            ),
        },
    );
    let mut harness = RequestHarness::new(setup, initial_state)?;

    let result = harness.execute().await;
    let output = harness.output_text();
    let observed = harness.observed(result);

    assert_eq!(observed, expected);
    assert!(!output.contains("synthetic-token"));
    assert!(!output.contains("synthetic-device"));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn request_dry_run_completes_full_preflight_without_prompt_signal_or_write() -> TestResult {
    let mut setup = RequestSetup::successful()?;
    setup.args.yes = false;
    setup.args.dry_run = true;
    let expected_calls = vec![
        RequestCall::ReadCredential,
        RequestCall::CurrentAccount,
        RequestCall::SearchUsers,
        RequestCall::UserById {
            user_id: "456".to_owned(),
        },
    ];
    let expected = Observed::new(
        ResultSnapshot::Success,
        RequestState {
            calls: expected_calls,
            stdout: writer_state("Dry run complete; no changes made.\n", 1),
        },
    );
    let mut harness = RequestHarness::new(setup, RequestState::default())?;

    let result = harness.execute().await;
    let observed = harness.observed(result);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn dry_run_completion_output_failure_is_ordinary_command_output() -> TestResult {
    let mut setup = RequestSetup::successful()?;
    setup.args.dry_run = true;
    let initial = RequestState {
        stdout: WriterState {
            fail_write: true,
            ..WriterState::default()
        },
        ..RequestState::default()
    };
    let mut harness = RequestHarness::new(setup, initial)?;

    let result = harness.execute().await;

    assert!(matches!(result, Err(AppError::CommandOutput { .. })));
    assert_eq!(
        harness.transcript.borrow().as_slice(),
        &[
            RequestCall::ReadCredential,
            RequestCall::CurrentAccount,
            RequestCall::SearchUsers,
            RequestCall::UserById {
                user_id: "456".to_owned(),
            },
        ]
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn request_handler_propagates_explicit_visibility_to_plan_and_output() -> TestResult {
    let mut setup = RequestSetup::successful()?;
    setup.args.visibility = crate::adapters::cli::args::VisibilityArg::Friends;
    let expected = Observed::new(
        ResultSnapshot::Success,
        RequestState {
            calls: successful_calls_with_visibility(true, Visibility::Friends),
            stdout: writer_state(
                "Request ID: request-1\nStatus: pending\nRequested from: @recipient (Synthetic recipient)\nAmount: $0.01\nRequested audience: friends\n",
                1,
            ),
        },
    );
    let mut harness = RequestHarness::new(setup, RequestState::default())?;

    let result = harness.execute().await;
    let observed = harness.observed(result);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn request_handler_interruption_is_ambiguous_without_false_output() -> TestResult {
    let setup = RequestSetup {
        write_behavior: RequestWriteBehavior::Pending,
        interruption: InterruptionBehavior::Ready,
        ..RequestSetup::successful()?
    };
    let initial_state = RequestState::default();
    let expected = Observed::new(
        failure_snapshot(
            ErrorVariant::FinancialWriteInterruptedUnknown,
            ErrorCategory::AmbiguousWrite,
            "the financial operation was interrupted after transmission may have begun; do not retry until its status is verified independently",
        ),
        RequestState {
            calls: successful_calls(false),
            ..RequestState::default()
        },
    );
    let mut harness = RequestHarness::new(setup, initial_state)?;

    let result = harness.execute().await;
    let observed = harness.observed(result);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn request_preflight_failure_never_installs_signals_or_starts_a_write() -> TestResult {
    let setup = RequestSetup {
        credential_present: false,
        write_behavior: RequestWriteBehavior::Pending,
        ..RequestSetup::successful()?
    };
    let initial_state = RequestState::default();
    let expected = Observed::new(
        failure_snapshot(
            ErrorVariant::RequestCreate,
            ErrorCategory::Credential,
            "no Venmo credential is stored; run `venmo auth login`",
        ),
        RequestState {
            calls: vec![RequestCall::ReadCredential],
            ..RequestState::default()
        },
    );
    let mut harness = RequestHarness::new(setup, initial_state)?;

    let result = harness.execute().await;
    let observed = harness.observed(result);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn request_creation_detail_output_failure_stops_before_authorization_and_write() -> TestResult
{
    let mut harness = RequestHarness::new(RequestSetup::successful()?, RequestState::default())?;
    let mut stderr = RecordingWriter::new(WriterState {
        fail_write: true,
        ..WriterState::default()
    });

    let result = harness.execute_with_stderr(&mut stderr).await;

    assert!(result.is_err());
    assert_eq!(
        harness.transcript.borrow().as_slice(),
        &[
            RequestCall::ReadCredential,
            RequestCall::CurrentAccount,
            RequestCall::SearchUsers,
            RequestCall::UserById {
                user_id: "456".to_owned(),
            },
        ]
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn signal_installation_and_stream_failures_precede_or_abort_the_write_exactly() -> TestResult
{
    for interruption in [
        InterruptionBehavior::InstallationFailure,
        InterruptionBehavior::FutureFailure,
    ] {
        let setup = RequestSetup {
            interruption,
            ..RequestSetup::successful()?
        };
        let initial_state = RequestState::default();
        let expected = Observed::new(
            failure_snapshot(
                ErrorVariant::SignalInitialization,
                ErrorCategory::Internal,
                "failed to install financial-write interrupt protection",
            ),
            RequestState {
                calls: successful_calls(false),
                ..RequestState::default()
            },
        );
        let mut harness = RequestHarness::new(setup, initial_state)?;

        let result = harness.execute().await;
        let observed = harness.observed(result);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn post_success_write_and_flush_failures_keep_the_specialized_ambiguous_error() -> TestResult
{
    for initial_stdout in [
        WriterState {
            fail_write: true,
            ..WriterState::default()
        },
        WriterState {
            fail_flush: true,
            ..WriterState::default()
        },
    ] {
        let setup = RequestSetup::successful()?;
        let initial_state = RequestState {
            stdout: initial_stdout.clone(),
            ..RequestState::default()
        };
        let expected_stdout = if initial_stdout.fail_write {
            initial_stdout
        } else {
            WriterState {
                bytes: b"Request ID: request-1\nStatus: pending\nRequested from: @recipient (Synthetic recipient)\nAmount: $0.01\nRequested audience: private\n".to_vec(),
                flush_count: 1,
                fail_flush: true,
                ..WriterState::default()
            }
        };
        let expected = Observed::new(
            failure_snapshot(
                ErrorVariant::FinancialResultOutput,
                ErrorCategory::AmbiguousWrite,
                "the financial operation succeeded, but its result could not be written; do not retry it and verify the result through activity or requests and the official Venmo app",
            ),
            RequestState {
                calls: successful_calls(true),
                stdout: expected_stdout,
            },
        );
        let mut harness = RequestHarness::new(setup, initial_state)?;

        let result = harness.execute().await;
        let observed = harness.observed(result);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn financial_protection_covers_normal_interrupt_tie_and_signal_failure_outcomes() {
    let normal = protect_with_interruption(ready("confirmed"), pending::<Result<(), AppError>>())
        .await
        .map_err(snapshot_error);
    let interrupted = protect_with_interruption(pending::<()>(), ready(Ok(())))
        .await
        .map_err(snapshot_error);
    let tie = protect_with_interruption(ready("confirmed"), ready(Ok(())))
        .await
        .map_err(snapshot_error);
    let signal_failure = protect_with_interruption(
        pending::<()>(),
        ready(Err(signal_initialization_error(
            "synthetic signal stream failure",
        ))),
    )
    .await
    .map_err(snapshot_error);

    assert_eq!(normal, Ok("confirmed"));
    assert_eq!(
        interrupted,
        Err(failure_snapshot(
            ErrorVariant::FinancialWriteInterruptedUnknown,
            ErrorCategory::AmbiguousWrite,
            "the financial operation was interrupted after transmission may have begun; do not retry until its status is verified independently",
        ))
    );
    assert_eq!(
        tie,
        Err(failure_snapshot(
            ErrorVariant::FinancialWriteInterruptedUnknown,
            ErrorCategory::AmbiguousWrite,
            "the financial operation was interrupted after transmission may have begun; do not retry until its status is verified independently",
        ))
    );
    assert_eq!(
        signal_failure,
        Err(failure_snapshot(
            ErrorVariant::SignalInitialization,
            ErrorCategory::Internal,
            "failed to install financial-write interrupt protection",
        ))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn state_protection_covers_normal_interrupt_tie_and_signal_failure_outcomes() {
    let normal =
        protect_state_with_interruption(ready("confirmed"), pending::<Result<(), AppError>>())
            .await
            .map_err(snapshot_error);
    let interrupted = protect_state_with_interruption(pending::<()>(), ready(Ok(())))
        .await
        .map_err(snapshot_error);
    let tie = protect_state_with_interruption(ready("confirmed"), ready(Ok(())))
        .await
        .map_err(snapshot_error);
    let signal_failure = protect_state_with_interruption(
        pending::<()>(),
        ready(Err(state_signal_initialization_error(
            "synthetic state signal stream failure",
        ))),
    )
    .await
    .map_err(snapshot_error);

    assert_eq!(normal, Ok("confirmed"));
    assert_eq!(
        interrupted,
        Err(failure_snapshot(
            ErrorVariant::StateWriteInterruptedUnknown,
            ErrorCategory::AmbiguousWrite,
            "the state mutation was interrupted after transmission may have begun; do not retry until its result is verified independently",
        ))
    );
    assert_eq!(
        tie,
        Err(failure_snapshot(
            ErrorVariant::StateWriteInterruptedUnknown,
            ErrorCategory::AmbiguousWrite,
            "the state mutation was interrupted after transmission may have begun; do not retry until its result is verified independently",
        ))
    );
    assert_eq!(
        signal_failure,
        Err(failure_snapshot(
            ErrorVariant::StateSignalInitialization,
            ErrorCategory::Internal,
            "failed to install state-write interrupt protection",
        ))
    );
}

pub(super) fn snapshot_result(result: Result<(), AppError>) -> ResultSnapshot {
    match result {
        Ok(()) => ResultSnapshot::Success,
        Err(error) => snapshot_error(error),
    }
}

fn snapshot_error(error: AppError) -> ResultSnapshot {
    ResultSnapshot::Failure {
        variant: match &error {
            AppError::Pay { .. } => ErrorVariant::Pay,
            AppError::RequestCreate { .. } => ErrorVariant::RequestCreate,
            AppError::Accept { .. } => ErrorVariant::Accept,
            AppError::Decline { .. } => ErrorVariant::Decline,
            AppError::FinancialWriteInterruptedUnknown => {
                ErrorVariant::FinancialWriteInterruptedUnknown
            }
            AppError::StateWriteInterruptedUnknown => ErrorVariant::StateWriteInterruptedUnknown,
            AppError::SignalInitialization { .. } => ErrorVariant::SignalInitialization,
            AppError::StateSignalInitialization { .. } => ErrorVariant::StateSignalInitialization,
            AppError::FinancialResultOutput { .. } => ErrorVariant::FinancialResultOutput,
            AppError::CommandOutput { .. } => ErrorVariant::CommandOutput,
            _ => ErrorVariant::Unexpected,
        },
        category: error.category(),
        exit_code: error.exit_code(),
        message: error.to_string(),
    }
}

pub(super) fn failure_snapshot(
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

fn successful_calls(write_started: bool) -> Vec<RequestCall> {
    successful_calls_with_visibility(write_started, Visibility::Private)
}

fn successful_calls_with_visibility(
    write_started: bool,
    visibility: Visibility,
) -> Vec<RequestCall> {
    let mut calls = vec![
        RequestCall::ReadCredential,
        RequestCall::CurrentAccount,
        RequestCall::SearchUsers,
        RequestCall::UserById {
            user_id: "456".to_owned(),
        },
        RequestCall::InstallInterruption,
    ];
    if write_started {
        calls.push(RequestCall::CreateRequest {
            client_request_id: "123e4567-e89b-12d3-a456-426614174000".to_owned(),
            account_user_id: "123".to_owned(),
            recipient_user_id: "456".to_owned(),
            amount_cents: 1,
            visibility,
        });
    }
    calls
}

fn writer_state(text: &str, flush_count: u32) -> WriterState {
    WriterState {
        bytes: text.as_bytes().to_vec(),
        flush_count,
        ..WriterState::default()
    }
}

fn signal_initialization_error(detail: &str) -> AppError {
    AppError::SignalInitialization {
        source: io::Error::other(detail.to_owned()),
    }
}

fn state_signal_initialization_error(detail: &str) -> AppError {
    AppError::StateSignalInitialization {
        source: io::Error::other(detail.to_owned()),
    }
}

fn request_args() -> Result<RequestArgs, Box<dyn Error>> {
    let cli = crate::adapters::cli::args::Cli::try_parse_from([
        "venmo",
        "requests",
        "create",
        "recipient",
        "0.01",
        "Synthetic request",
        "--yes",
    ])?;
    match cli.command {
        crate::adapters::cli::args::Command::Requests(args) => match args.operation {
            crate::adapters::cli::args::RequestsOperation::Create(args) => Ok(args),
            _ => Err(io::Error::other("request arguments parsed as another operation").into()),
        },
        _ => Err(io::Error::other("request arguments parsed as another command").into()),
    }
}

fn request_credential() -> Result<LoadedCredential, FakeCredentialError> {
    Ok(LoadedCredential {
        envelope: CredentialEnvelope::new(
            AccessToken::from_str("synthetic-token").map_err(|_| FakeCredentialError)?,
            DeviceId::from_str("synthetic-device").map_err(|_| FakeCredentialError)?,
            UserId::from_str("123").map_err(|_| FakeCredentialError)?,
            Username::from_bare("owner").map_err(|_| FakeCredentialError)?,
            Some("Synthetic owner".to_owned()),
            time::OffsetDateTime::UNIX_EPOCH,
        ),
        format: CredentialFormat::Version1,
    })
}

fn account() -> Result<Account, Box<dyn Error>> {
    Ok(Account::new(
        UserId::from_str("123")?,
        Username::from_bare("owner")?,
        Some("Synthetic owner".to_owned()),
    ))
}

fn recipient() -> Result<User, Box<dyn Error>> {
    Ok(User::new(
        UserId::from_str("456")?,
        Some(Username::from_bare("recipient")?),
        Some("Synthetic recipient".to_owned()),
    )
    .with_financial_attributes(UserProfileKind::Personal, true))
}

fn created_request() -> Result<CreatedRequest, Box<dyn Error>> {
    Ok(CreatedRequest::new(
        RequestId::from_str("request-1")?,
        RequestStatus::from_str("pending")?,
    ))
}

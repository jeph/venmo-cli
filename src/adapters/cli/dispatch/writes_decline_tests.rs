use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error;
use std::future::{Future, ready};
use std::io::{self, Write};
use std::pin::Pin;
use std::rc::Rc;
use std::str::FromStr;
use std::task::{Context, Poll};

use clap::Parser;

use super::tests::{ErrorVariant, ResultSnapshot, failure_snapshot, snapshot_result};
use crate::adapters::cli::args::{Cli, Command, DeclineArgs, RequestsOperation};
use crate::adapters::cli::error::{AppError, ErrorCategory};
use crate::features::auth::{CurrentAccountApi, PromptAvailability, PromptError};
use crate::features::payments::DefaultNoConfirmation;
use crate::features::people::User;
use crate::features::requests::{
    DeclineRequestPlan, DeclinedRequest, RequestAction, RequestDeclineApi, RequestDirection,
    RequestId, RequestLookupApi, RequestRecord, RequestStatus,
};
use crate::shared::test_support::Observed;
use crate::shared::{
    AccessToken, Account, ApiFailure, ApiFailureKind, CredentialCapability, CredentialEnvelope,
    CredentialFailureKind, CredentialFormat, CredentialReader, CredentialStoreFailure, DeviceId,
    LoadedCredential, Money, UserId, Username,
};

use super::run_decline_with;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;
type Transcript = Rc<RefCell<Vec<DeclineCall>>>;

#[derive(Clone, Debug, Eq, PartialEq)]
struct DeclinePlanCall {
    account_user_id: String,
    request_id: String,
    requester_user_id: String,
    amount_cents: u64,
    note: Option<String>,
    audience: Option<String>,
}

impl From<&DeclineRequestPlan> for DeclinePlanCall {
    fn from(plan: &DeclineRequestPlan) -> Self {
        Self {
            account_user_id: plan.account().user_id().to_string(),
            request_id: plan.request().id().to_string(),
            requester_user_id: plan.request().counterparty().user_id().to_string(),
            amount_cents: plan.request().amount().cents(),
            note: plan.request().note().map(str::to_owned),
            audience: plan.request().audience().map(str::to_owned),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum DeclineCall {
    ReadCredential,
    CurrentAccount,
    RequestLookup {
        current_user_id: String,
        request_id: String,
    },
    StderrWrite,
    StderrFlush,
    PromptAvailability,
    ConfirmDefaultNo {
        prompt: String,
    },
    InstallInterruption,
    Decline {
        plan: DeclinePlanCall,
    },
    StdoutWrite,
    StdoutFlush,
}

#[derive(Clone, Copy)]
enum CredentialStep {
    Present,
    Missing,
}

#[derive(Clone, Copy)]
enum ApiStep {
    Success,
    Failure,
}

#[derive(Clone, Copy)]
enum WriteStep {
    Success,
    Pending,
    Failure,
}

#[derive(Clone, Copy)]
enum ConfirmationStep {
    Answer(bool),
    Failure,
}

#[derive(Clone, Copy)]
enum InterruptionStep {
    Pending,
    Ready,
    PendingThenReady,
    StreamFailure,
}

struct DeclineScript {
    credentials: Vec<CredentialStep>,
    accounts: Vec<ApiStep>,
    requests: Vec<ApiStep>,
    interactive: bool,
    confirmations: Vec<ConfirmationStep>,
    interruptions: Vec<InterruptionStep>,
    declines: Vec<WriteStep>,
}

impl DeclineScript {
    fn successful() -> Self {
        Self {
            credentials: vec![CredentialStep::Present, CredentialStep::Missing],
            accounts: vec![ApiStep::Success, ApiStep::Failure],
            requests: vec![ApiStep::Success, ApiStep::Failure],
            interactive: true,
            confirmations: vec![ConfirmationStep::Answer(true), ConfirmationStep::Failure],
            interruptions: vec![InterruptionStep::Pending, InterruptionStep::StreamFailure],
            declines: vec![WriteStep::Success, WriteStep::Failure],
        }
    }

    fn with_first_credential(mut self, step: CredentialStep) -> Self {
        self.credentials = vec![step, CredentialStep::Present];
        self
    }

    fn with_first_interruption(mut self, step: InterruptionStep) -> Self {
        self.interruptions = vec![step, InterruptionStep::StreamFailure];
        self
    }

    fn with_interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }

    fn with_first_confirmation(mut self, step: ConfirmationStep) -> Self {
        self.confirmations = vec![step, ConfirmationStep::Failure];
        self
    }

    fn with_first_decline(mut self, step: WriteStep) -> Self {
        self.declines = vec![step, WriteStep::Failure];
        self
    }
}

struct DeclineScriptState {
    credentials: RefCell<VecDeque<CredentialStep>>,
    accounts: RefCell<VecDeque<ApiStep>>,
    requests: RefCell<VecDeque<ApiStep>>,
    interactive: bool,
    confirmations: RefCell<VecDeque<ConfirmationStep>>,
    interruptions: RefCell<VecDeque<InterruptionStep>>,
    declines: RefCell<VecDeque<WriteStep>>,
}

impl DeclineScriptState {
    fn new(script: DeclineScript) -> Self {
        Self {
            credentials: RefCell::new(script.credentials.into()),
            accounts: RefCell::new(script.accounts.into()),
            requests: RefCell::new(script.requests.into()),
            interactive: script.interactive,
            confirmations: RefCell::new(script.confirmations.into()),
            interruptions: RefCell::new(script.interruptions.into()),
            declines: RefCell::new(script.declines.into()),
        }
    }

    fn remaining(&self) -> RemainingDeclineScript {
        RemainingDeclineScript {
            credentials: self.credentials.borrow().len(),
            accounts: self.accounts.borrow().len(),
            requests: self.requests.borrow().len(),
            confirmations: self.confirmations.borrow().len(),
            interruptions: self.interruptions.borrow().len(),
            declines: self.declines.borrow().len(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RemainingDeclineScript {
    credentials: usize,
    accounts: usize,
    requests: usize,
    confirmations: usize,
    interruptions: usize,
    declines: usize,
}

impl RemainingDeclineScript {
    const fn after_success() -> Self {
        Self {
            credentials: 1,
            accounts: 1,
            requests: 1,
            confirmations: 1,
            interruptions: 1,
            declines: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("synthetic credential failure")]
struct FakeCredentialError;

impl CredentialStoreFailure for FakeCredentialError {
    fn kind(&self) -> CredentialFailureKind {
        CredentialFailureKind::Internal
    }
}

struct FakeReader {
    script: Rc<DeclineScriptState>,
    transcript: Transcript,
}

impl CredentialCapability for FakeReader {
    type Error = FakeCredentialError;
}

impl CredentialReader for FakeReader {
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        self.transcript
            .borrow_mut()
            .push(DeclineCall::ReadCredential);
        match self
            .script
            .credentials
            .borrow_mut()
            .pop_front()
            .unwrap_or(CredentialStep::Missing)
        {
            CredentialStep::Present => credential().map(Some),
            CredentialStep::Missing => Ok(None),
        }
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("synthetic decline API failure")]
struct FakeApiError;

impl ApiFailure for FakeApiError {
    fn kind(&self) -> ApiFailureKind {
        ApiFailureKind::Internal
    }
}

struct FakeDeclineApi {
    script: Rc<DeclineScriptState>,
    transcript: Transcript,
}

impl CurrentAccountApi for FakeDeclineApi {
    type Error = FakeApiError;

    fn current_account<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a {
        self.transcript
            .borrow_mut()
            .push(DeclineCall::CurrentAccount);
        ready(match next_api_step(&self.script.accounts) {
            ApiStep::Success => account().map_err(|_| FakeApiError),
            ApiStep::Failure => Err(FakeApiError),
        })
    }
}

impl RequestLookupApi for FakeDeclineApi {
    type Error = FakeApiError;

    fn request_by_id<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        request_id: &'a RequestId,
    ) -> impl Future<Output = Result<RequestRecord, Self::Error>> + Send + 'a {
        self.transcript
            .borrow_mut()
            .push(DeclineCall::RequestLookup {
                current_user_id: current_user_id.to_string(),
                request_id: request_id.to_string(),
            });
        ready(match next_api_step(&self.script.requests) {
            ApiStep::Success => request_record().map_err(|_| FakeApiError),
            ApiStep::Failure => Err(FakeApiError),
        })
    }
}

impl RequestDeclineApi for FakeDeclineApi {
    type Error = FakeApiError;

    fn decline_request<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        plan: &'a DeclineRequestPlan,
    ) -> impl Future<Output = Result<DeclinedRequest, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(DeclineCall::Decline {
            plan: DeclinePlanCall::from(plan),
        });
        let step = self
            .script
            .declines
            .borrow_mut()
            .pop_front()
            .unwrap_or(WriteStep::Failure);
        DeclineFuture::new(step)
    }
}

struct FakePrompt {
    script: Rc<DeclineScriptState>,
    transcript: Transcript,
}

impl PromptAvailability for FakePrompt {
    fn can_prompt(&self) -> bool {
        self.transcript
            .borrow_mut()
            .push(DeclineCall::PromptAvailability);
        self.script.interactive
    }
}

impl DefaultNoConfirmation for FakePrompt {
    fn confirm_default_no(&self, prompt: &str) -> Result<bool, PromptError> {
        self.transcript
            .borrow_mut()
            .push(DeclineCall::ConfirmDefaultNo {
                prompt: prompt.to_owned(),
            });
        match self
            .script
            .confirmations
            .borrow_mut()
            .pop_front()
            .unwrap_or(ConfirmationStep::Failure)
        {
            ConfirmationStep::Answer(answer) => Ok(answer),
            ConfirmationStep::Failure => Err(PromptError::Interaction {
                source: io::Error::other("synthetic decline confirmation failure"),
            }),
        }
    }
}

fn next_api_step(steps: &RefCell<VecDeque<ApiStep>>) -> ApiStep {
    steps.borrow_mut().pop_front().unwrap_or(ApiStep::Failure)
}

struct DeclineFuture {
    step: WriteStep,
}

impl DeclineFuture {
    const fn new(step: WriteStep) -> Self {
        Self { step }
    }
}

impl Future for DeclineFuture {
    type Output = Result<DeclinedRequest, FakeApiError>;

    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        match self.step {
            WriteStep::Success => Poll::Ready(declined_request().map_err(|_| FakeApiError)),
            WriteStep::Pending => Poll::Pending,
            WriteStep::Failure => Poll::Ready(Err(FakeApiError)),
        }
    }
}

struct InterruptionFuture {
    step: InterruptionStep,
    yielded: bool,
}

impl InterruptionFuture {
    const fn new(step: InterruptionStep) -> Self {
        Self {
            step,
            yielded: false,
        }
    }
}

impl Future for InterruptionFuture {
    type Output = Result<(), AppError>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        match self.step {
            InterruptionStep::Pending => Poll::Pending,
            InterruptionStep::Ready => Poll::Ready(Ok(())),
            InterruptionStep::PendingThenReady if self.yielded => Poll::Ready(Ok(())),
            InterruptionStep::PendingThenReady => {
                self.yielded = true;
                context.waker().wake_by_ref();
                Poll::Pending
            }
            InterruptionStep::StreamFailure => Poll::Ready(Err(signal_error())),
        }
    }
}

#[derive(Clone, Copy)]
enum Stream {
    Stdout,
    Stderr,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct WriterState {
    text: String,
    flush_count: u32,
    fail_write: bool,
    fail_flush: bool,
}

struct OrderedWriter {
    stream: Stream,
    state: WriterState,
    transcript: Transcript,
    recorded_write: bool,
}

impl OrderedWriter {
    fn new(stream: Stream, state: WriterState, transcript: Transcript) -> Self {
        Self {
            stream,
            state,
            transcript,
            recorded_write: false,
        }
    }

    fn record_write_once(&mut self) {
        if !self.recorded_write {
            self.recorded_write = true;
            self.transcript.borrow_mut().push(match self.stream {
                Stream::Stdout => DeclineCall::StdoutWrite,
                Stream::Stderr => DeclineCall::StderrWrite,
            });
        }
    }
}

impl Write for OrderedWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.record_write_once();
        if self.state.fail_write {
            return Err(io::Error::other("synthetic output failure"));
        }
        self.state
            .text
            .push_str(std::str::from_utf8(buffer).map_err(io::Error::other)?);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.transcript.borrow_mut().push(match self.stream {
            Stream::Stdout => DeclineCall::StdoutFlush,
            Stream::Stderr => DeclineCall::StderrFlush,
        });
        self.state.flush_count += 1;
        if self.state.fail_flush {
            Err(io::Error::other("synthetic flush failure"))
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
struct DeclineState {
    calls: Vec<DeclineCall>,
    remaining: Option<RemainingDeclineScript>,
    stdout: WriterState,
    stderr: WriterState,
}

struct DeclineHarness {
    args: DeclineArgs,
    script: Rc<DeclineScriptState>,
    transcript: Transcript,
    reader: FakeReader,
    api: FakeDeclineApi,
    prompt: FakePrompt,
    stdout: OrderedWriter,
    stderr: OrderedWriter,
}

impl DeclineHarness {
    fn new(script: DeclineScript, initial: DeclineState) -> TestResult<Self> {
        let transcript = Rc::new(RefCell::new(initial.calls));
        let script = Rc::new(DeclineScriptState::new(script));
        Ok(Self {
            args: decline_args()?,
            reader: FakeReader {
                script: Rc::clone(&script),
                transcript: Rc::clone(&transcript),
            },
            api: FakeDeclineApi {
                script: Rc::clone(&script),
                transcript: Rc::clone(&transcript),
            },
            prompt: FakePrompt {
                script: Rc::clone(&script),
                transcript: Rc::clone(&transcript),
            },
            stdout: OrderedWriter::new(Stream::Stdout, initial.stdout, Rc::clone(&transcript)),
            stderr: OrderedWriter::new(Stream::Stderr, initial.stderr, Rc::clone(&transcript)),
            script,
            transcript,
        })
    }

    async fn execute(&mut self) -> Result<(), AppError> {
        let script = Rc::clone(&self.script);
        let transcript = Rc::clone(&self.transcript);
        run_decline_with(
            self.args.clone(),
            &self.reader,
            &self.api,
            &self.prompt,
            &mut self.stdout,
            &mut self.stderr,
            move || {
                transcript
                    .borrow_mut()
                    .push(DeclineCall::InstallInterruption);
                let step = script
                    .interruptions
                    .borrow_mut()
                    .pop_front()
                    .unwrap_or(InterruptionStep::StreamFailure);
                Ok(InterruptionFuture::new(step))
            },
        )
        .await
    }

    fn observed(self, result: Result<(), AppError>) -> Observed<ResultSnapshot, DeclineState> {
        Observed::new(
            snapshot_result(result),
            DeclineState {
                calls: self.transcript.borrow().clone(),
                remaining: Some(self.script.remaining()),
                stdout: self.stdout.state,
                stderr: self.stderr.state,
            },
        )
    }
}

#[tokio::test(flavor = "current_thread")]
async fn decline_handler_orders_default_no_confirmation_one_write_output_and_flush() -> TestResult {
    // Setup.
    let script = DeclineScript::successful();

    // Immutable initial script/state.
    let initial = DeclineState::default();
    let expected = Observed::new(
        ResultSnapshot::Success,
        DeclineState {
            calls: successful_calls(),
            remaining: Some(RemainingDeclineScript::after_success()),
            stdout: writer_state(DECLINE_RESULT, 1),
            stderr: writer_state(DECLINE_PREFLIGHT, 1),
        },
    );
    let mut harness = DeclineHarness::new(script, initial)?;

    // Execute once.
    let result = harness.execute().await;
    let observed = harness.observed(result);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn decline_confirmation_rejection_and_failures_stop_before_interruption_and_write()
-> TestResult {
    for (script, expected_result, expected_confirmation_count, expected_calls) in [
        (
            DeclineScript::successful()
                .with_interactive(false)
                .with_first_confirmation(ConfirmationStep::Answer(false)),
            failure_snapshot(
                ErrorVariant::Decline,
                ErrorCategory::Usage,
                "request decline confirmation requires both stdin and stderr to be terminals; pass `--yes` to authorize non-interactively",
            ),
            2,
            preparation_and_prompt_availability_calls(),
        ),
        (
            DeclineScript::successful().with_first_confirmation(ConfirmationStep::Answer(false)),
            failure_snapshot(
                ErrorVariant::Decline,
                ErrorCategory::Cancelled,
                "request decline cancelled",
            ),
            1,
            preparation_and_authorization_calls(),
        ),
        (
            DeclineScript::successful().with_first_confirmation(ConfirmationStep::Failure),
            failure_snapshot(
                ErrorVariant::Decline,
                ErrorCategory::Internal,
                "request decline confirmation failed: terminal interaction failed",
            ),
            1,
            preparation_and_authorization_calls(),
        ),
    ] {
        let expected = Observed::new(
            expected_result,
            DeclineState {
                calls: expected_calls,
                remaining: Some(RemainingDeclineScript {
                    credentials: 1,
                    accounts: 1,
                    requests: 1,
                    confirmations: expected_confirmation_count,
                    interruptions: 2,
                    declines: 2,
                }),
                stderr: writer_state(DECLINE_PREFLIGHT, 1),
                ..DeclineState::default()
            },
        );
        let mut harness = DeclineHarness::new(script, DeclineState::default())?;

        let result = harness.execute().await;
        let observed = harness.observed(result);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn decline_yes_retains_preflight_and_one_write_without_prompt_calls() -> TestResult {
    let expected = Observed::new(
        ResultSnapshot::Success,
        DeclineState {
            calls: successful_yes_calls(),
            remaining: Some(RemainingDeclineScript {
                confirmations: 2,
                ..RemainingDeclineScript::after_success()
            }),
            stdout: writer_state(DECLINE_RESULT, 1),
            stderr: writer_state(DECLINE_PREFLIGHT, 1),
        },
    );
    let mut harness = DeclineHarness::new(DeclineScript::successful(), DeclineState::default())?;
    harness.args.yes = true;

    let result = harness.execute().await;
    let observed = harness.observed(result);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn decline_preflight_failure_stops_before_output_interruption_and_write() -> TestResult {
    // Setup.
    let script = DeclineScript::successful().with_first_credential(CredentialStep::Missing);

    // Immutable initial script/state.
    let initial = DeclineState::default();
    let expected = Observed::new(
        failure_snapshot(
            ErrorVariant::Decline,
            ErrorCategory::Credential,
            "no Venmo credential is stored; run `venmo auth login`",
        ),
        DeclineState {
            calls: vec![DeclineCall::ReadCredential],
            remaining: Some(RemainingDeclineScript {
                credentials: 1,
                accounts: 2,
                requests: 2,
                confirmations: 2,
                interruptions: 2,
                declines: 2,
            }),
            ..DeclineState::default()
        },
    );
    let mut harness = DeclineHarness::new(script, initial)?;

    // Execute once.
    let result = harness.execute().await;
    let observed = harness.observed(result);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn decline_interruption_tie_and_signal_failure_preserve_exact_ambiguity_boundaries()
-> TestResult {
    for (script, expected_result, write_started) in [
        (
            DeclineScript::successful()
                .with_first_decline(WriteStep::Pending)
                .with_first_interruption(InterruptionStep::PendingThenReady),
            failure_snapshot(
                ErrorVariant::FinancialWriteInterruptedUnknown,
                ErrorCategory::AmbiguousWrite,
                "the financial operation was interrupted after transmission may have begun; do not retry until its status is verified independently",
            ),
            true,
        ),
        (
            DeclineScript::successful().with_first_interruption(InterruptionStep::Ready),
            failure_snapshot(
                ErrorVariant::FinancialWriteInterruptedUnknown,
                ErrorCategory::AmbiguousWrite,
                "the financial operation was interrupted after transmission may have begun; do not retry until its status is verified independently",
            ),
            false,
        ),
        (
            DeclineScript::successful().with_first_interruption(InterruptionStep::StreamFailure),
            failure_snapshot(
                ErrorVariant::SignalInitialization,
                ErrorCategory::Internal,
                "failed to install financial-write interrupt protection",
            ),
            false,
        ),
    ] {
        // Immutable initial script/state.
        let initial = DeclineState::default();
        let mut calls = preparation_calls();
        calls.extend([DeclineCall::StderrWrite, DeclineCall::StderrFlush]);
        calls.extend(authorization_calls());
        calls.push(DeclineCall::InstallInterruption);
        if write_started {
            calls.push(decline_call());
        }
        let expected = Observed::new(
            expected_result,
            DeclineState {
                calls,
                remaining: Some(RemainingDeclineScript {
                    declines: if write_started { 1 } else { 2 },
                    ..RemainingDeclineScript::after_success()
                }),
                stderr: writer_state(DECLINE_PREFLIGHT, 1),
                ..DeclineState::default()
            },
        );
        let mut harness = DeclineHarness::new(script, initial)?;

        // Execute once.
        let result = harness.execute().await;
        let observed = harness.observed(result);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn decline_post_write_output_failures_are_specialized_ambiguous_outcomes() -> TestResult {
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
        // Setup.
        let script = DeclineScript::successful();

        // Immutable initial script/state.
        let initial = DeclineState {
            stdout: initial_stdout.clone(),
            ..DeclineState::default()
        };
        let expected_stdout = if initial_stdout.fail_write {
            initial_stdout
        } else {
            WriterState {
                text: DECLINE_RESULT.to_owned(),
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
            DeclineState {
                calls: if expected_stdout.fail_write {
                    successful_calls_without_stdout_flush()
                } else {
                    successful_calls()
                },
                remaining: Some(RemainingDeclineScript::after_success()),
                stdout: expected_stdout,
                stderr: writer_state(DECLINE_PREFLIGHT, 1),
            },
        );
        let mut harness = DeclineHarness::new(script, initial)?;

        // Execute once.
        let result = harness.execute().await;
        let observed = harness.observed(result);

        assert_eq!(observed, expected);
    }
    Ok(())
}

fn preparation_calls() -> Vec<DeclineCall> {
    vec![
        DeclineCall::ReadCredential,
        DeclineCall::CurrentAccount,
        DeclineCall::RequestLookup {
            current_user_id: "123".to_owned(),
            request_id: "request-1".to_owned(),
        },
    ]
}

fn authorization_calls() -> Vec<DeclineCall> {
    vec![
        DeclineCall::PromptAvailability,
        DeclineCall::ConfirmDefaultNo {
            prompt: "Decline this request without sending money?".to_owned(),
        },
    ]
}

fn preparation_and_prompt_availability_calls() -> Vec<DeclineCall> {
    let mut calls = preparation_calls();
    calls.extend([
        DeclineCall::StderrWrite,
        DeclineCall::StderrFlush,
        DeclineCall::PromptAvailability,
    ]);
    calls
}

fn preparation_and_authorization_calls() -> Vec<DeclineCall> {
    let mut calls = preparation_calls();
    calls.extend([DeclineCall::StderrWrite, DeclineCall::StderrFlush]);
    calls.extend(authorization_calls());
    calls
}

fn successful_calls_without_stdout_flush() -> Vec<DeclineCall> {
    let mut calls = preparation_calls();
    calls.extend([DeclineCall::StderrWrite, DeclineCall::StderrFlush]);
    calls.extend(authorization_calls());
    calls.extend([
        DeclineCall::InstallInterruption,
        decline_call(),
        DeclineCall::StdoutWrite,
    ]);
    calls
}

fn successful_yes_calls() -> Vec<DeclineCall> {
    let mut calls = preparation_calls();
    calls.extend([
        DeclineCall::StderrWrite,
        DeclineCall::StderrFlush,
        DeclineCall::InstallInterruption,
        decline_call(),
        DeclineCall::StdoutWrite,
        DeclineCall::StdoutFlush,
    ]);
    calls
}

fn successful_calls() -> Vec<DeclineCall> {
    let mut calls = successful_calls_without_stdout_flush();
    calls.push(DeclineCall::StdoutFlush);
    calls
}

fn decline_call() -> DeclineCall {
    DeclineCall::Decline {
        plan: DeclinePlanCall {
            account_user_id: "123".to_owned(),
            request_id: "request-1".to_owned(),
            requester_user_id: "456".to_owned(),
            amount_cents: 1,
            note: Some("Synthetic request".to_owned()),
            audience: Some("private".to_owned()),
        },
    }
}

fn writer_state(text: &str, flush_count: u32) -> WriterState {
    WriterState {
        text: text.to_owned(),
        flush_count,
        ..WriterState::default()
    }
}

fn signal_error() -> AppError {
    AppError::SignalInitialization {
        source: io::Error::other("synthetic signal stream failure"),
    }
}

fn decline_args() -> TestResult<DeclineArgs> {
    let cli = Cli::try_parse_from(["venmo", "requests", "decline", "request-1"])?;
    match cli.command {
        Command::Requests(args) => match args.operation {
            RequestsOperation::Decline(args) => Ok(args),
            _ => Err(io::Error::other("decline arguments parsed as another operation").into()),
        },
        _ => Err(io::Error::other("decline arguments parsed as another command").into()),
    }
}

fn credential() -> Result<LoadedCredential, FakeCredentialError> {
    Ok(LoadedCredential {
        envelope: CredentialEnvelope::new(
            AccessToken::from_str("synthetic-decline-token").map_err(|_| FakeCredentialError)?,
            DeviceId::from_str("synthetic-decline-device").map_err(|_| FakeCredentialError)?,
            UserId::from_str("123").map_err(|_| FakeCredentialError)?,
            Username::from_bare("owner").map_err(|_| FakeCredentialError)?,
            Some("Synthetic owner".to_owned()),
            time::OffsetDateTime::UNIX_EPOCH,
        ),
        format: CredentialFormat::Version1,
    })
}

fn account() -> TestResult<Account> {
    Ok(Account::new(
        UserId::from_str("123")?,
        Username::from_bare("owner")?,
        Some("Synthetic owner".to_owned()),
    ))
}

fn request_record() -> TestResult<RequestRecord> {
    Ok(RequestRecord::new(
        RequestId::from_str("request-1")?,
        RequestAction::Charge,
        RequestDirection::Incoming,
        User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("requester")?),
            Some("Synthetic requester".to_owned()),
        ),
        Money::from_cents(1)?,
        Some("Synthetic request".to_owned()),
        Some(time::OffsetDateTime::UNIX_EPOCH),
        RequestStatus::from_str("pending")?,
    )
    .with_audience(Some("private".to_owned())))
}

fn declined_request() -> TestResult<DeclinedRequest> {
    Ok(DeclinedRequest::new(
        RequestId::from_str("request-1")?,
        RequestStatus::from_str("cancelled")?,
    ))
}

const DECLINE_PREFLIGHT: &str = concat!(
    "Request decline preflight:\n",
    "  Request ID: request-1\n",
    "  Requester: @requester (Synthetic requester) (ID 456)\n",
    "  Amount: $0.01\n",
    "  Note: Synthetic request\n",
    "  Audience: private\n",
    "  Current request status: pending\n",
    "  Created: 1970-01-01T00:00:00Z\n",
    "  Action: decline this exact incoming request without sending money.\n",
    "  Confirmation defaults to No; --yes skips only the confirmation prompt.\n",
);

const DECLINE_RESULT: &str = concat!(
    "Request ID: request-1\n",
    "Server status: cancelled\n",
    "Declined requester: @requester (Synthetic requester)\n",
    "Amount requested: $0.01\n",
    "Money sent: no\n",
);

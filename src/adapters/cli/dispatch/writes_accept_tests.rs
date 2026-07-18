use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error;
use std::future::{Future, ready};
use std::io;
use std::pin::Pin;
use std::rc::Rc;
use std::str::FromStr;
use std::task::{Context, Poll};

use clap::Parser;

use super::tests::{ErrorVariant, ResultSnapshot, failure_snapshot, snapshot_result};
use crate::adapters::cli::args::{AcceptArgs, Cli, Command, RequestsOperation};
use crate::adapters::cli::error::{AppError, ErrorCategory};
use crate::features::auth::{CurrentAccountApi, PromptAvailability, PromptError};
use crate::features::payments::{DefaultNoConfirmation, FinancialStatus, PaymentId};
use crate::features::people::{User, UserLookupApi, UserProfileKind};
use crate::features::requests::{
    AcceptRequestPlan, AcceptedRequest, RequestAcceptanceApi, RequestAction, RequestDirection,
    RequestId, RequestLookupApi, RequestRecord, RequestStatus,
};
use crate::features::wallet::{Balance, BalanceApi, SignedUsdAmount};
use crate::shared::test_support::Observed;
use crate::shared::{
    AccessToken, Account, ApiFailure, ApiFailureKind, CredentialCapability, CredentialEnvelope,
    CredentialFailureKind, CredentialFormat, CredentialReader, CredentialStoreFailure, DeviceId,
    LoadedCredential, Money, UserId, Username,
};

use super::run_accept_with;

#[path = "writes_accept_tests/expectations.rs"]
mod expectations;
#[path = "writes_accept_tests/fixtures.rs"]
mod fixtures;
#[path = "writes_accept_tests/interruption_output.rs"]
mod interruption_output;
#[path = "writes_accept_tests/output.rs"]
mod output;
#[path = "writes_accept_tests/success_preflight.rs"]
mod success_preflight;

use expectations::*;
use fixtures::*;
use output::*;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;
type Transcript = Rc<RefCell<Vec<AcceptCall>>>;

#[derive(Clone, Debug, Eq, PartialEq)]
struct AcceptPlanCall {
    account_user_id: String,
    request_id: String,
    requester_user_id: String,
    amount_cents: u64,
    note: Option<String>,
    audience: Option<String>,
    available_balance_cents: i64,
}

impl From<&AcceptRequestPlan> for AcceptPlanCall {
    fn from(plan: &AcceptRequestPlan) -> Self {
        Self {
            account_user_id: plan.account().user_id().to_string(),
            request_id: plan.request().id().to_string(),
            requester_user_id: plan.request().counterparty().user_id().to_string(),
            amount_cents: plan.request().amount().cents(),
            note: plan.request().note().map(str::to_owned),
            audience: plan.request().audience().map(str::to_owned),
            available_balance_cents: plan.balance().available().cents(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum AcceptCall {
    ReadCredential,
    CurrentAccount,
    RequestLookup {
        current_user_id: String,
        request_id: String,
    },
    UserLookup {
        user_id: String,
    },
    Balance,
    StderrWrite,
    StderrFlush,
    PromptAvailability,
    ConfirmDefaultNo {
        prompt: String,
    },
    InstallInterruption,
    Accept {
        plan: AcceptPlanCall,
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

struct AcceptScript {
    credentials: Vec<CredentialStep>,
    accounts: Vec<ApiStep>,
    requests: Vec<ApiStep>,
    users: Vec<ApiStep>,
    balances: Vec<ApiStep>,
    confirmations: Vec<ConfirmationStep>,
    interruptions: Vec<InterruptionStep>,
    acceptances: Vec<WriteStep>,
}

impl AcceptScript {
    fn successful() -> Self {
        Self {
            credentials: vec![CredentialStep::Present, CredentialStep::Missing],
            accounts: vec![ApiStep::Success, ApiStep::Failure],
            requests: vec![ApiStep::Success, ApiStep::Failure],
            users: vec![ApiStep::Success, ApiStep::Failure],
            balances: vec![ApiStep::Success, ApiStep::Failure],
            confirmations: vec![ConfirmationStep::Answer(true), ConfirmationStep::Failure],
            interruptions: vec![InterruptionStep::Pending, InterruptionStep::StreamFailure],
            acceptances: vec![WriteStep::Success, WriteStep::Failure],
        }
    }

    fn with_first_credential(mut self, step: CredentialStep) -> Self {
        self.credentials = vec![step, CredentialStep::Present];
        self
    }

    fn with_first_confirmation(mut self, step: ConfirmationStep) -> Self {
        self.confirmations = vec![step, ConfirmationStep::Failure];
        self
    }

    fn with_first_interruption(mut self, step: InterruptionStep) -> Self {
        self.interruptions = vec![step, InterruptionStep::StreamFailure];
        self
    }

    fn with_first_acceptance(mut self, step: WriteStep) -> Self {
        self.acceptances = vec![step, WriteStep::Failure];
        self
    }
}

struct AcceptScriptState {
    credentials: RefCell<VecDeque<CredentialStep>>,
    accounts: RefCell<VecDeque<ApiStep>>,
    requests: RefCell<VecDeque<ApiStep>>,
    users: RefCell<VecDeque<ApiStep>>,
    balances: RefCell<VecDeque<ApiStep>>,
    confirmations: RefCell<VecDeque<ConfirmationStep>>,
    interruptions: RefCell<VecDeque<InterruptionStep>>,
    acceptances: RefCell<VecDeque<WriteStep>>,
}

impl AcceptScriptState {
    fn new(script: AcceptScript) -> Self {
        Self {
            credentials: RefCell::new(script.credentials.into()),
            accounts: RefCell::new(script.accounts.into()),
            requests: RefCell::new(script.requests.into()),
            users: RefCell::new(script.users.into()),
            balances: RefCell::new(script.balances.into()),
            confirmations: RefCell::new(script.confirmations.into()),
            interruptions: RefCell::new(script.interruptions.into()),
            acceptances: RefCell::new(script.acceptances.into()),
        }
    }

    fn remaining(&self) -> RemainingAcceptScript {
        RemainingAcceptScript {
            credentials: self.credentials.borrow().len(),
            accounts: self.accounts.borrow().len(),
            requests: self.requests.borrow().len(),
            users: self.users.borrow().len(),
            balances: self.balances.borrow().len(),
            confirmations: self.confirmations.borrow().len(),
            interruptions: self.interruptions.borrow().len(),
            acceptances: self.acceptances.borrow().len(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RemainingAcceptScript {
    credentials: usize,
    accounts: usize,
    requests: usize,
    users: usize,
    balances: usize,
    confirmations: usize,
    interruptions: usize,
    acceptances: usize,
}

impl RemainingAcceptScript {
    const fn after_success() -> Self {
        Self {
            credentials: 1,
            accounts: 1,
            requests: 1,
            users: 1,
            balances: 1,
            confirmations: 1,
            interruptions: 1,
            acceptances: 1,
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
    script: Rc<AcceptScriptState>,
    transcript: Transcript,
}

impl CredentialCapability for FakeReader {
    type Error = FakeCredentialError;
}

impl CredentialReader for FakeReader {
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        self.transcript
            .borrow_mut()
            .push(AcceptCall::ReadCredential);
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
#[error("synthetic acceptance API failure")]
struct FakeApiError;

impl ApiFailure for FakeApiError {
    fn kind(&self) -> ApiFailureKind {
        ApiFailureKind::Internal
    }
}

struct FakeAcceptApi {
    script: Rc<AcceptScriptState>,
    transcript: Transcript,
}

impl CurrentAccountApi for FakeAcceptApi {
    type Error = FakeApiError;

    fn current_account<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a {
        self.transcript
            .borrow_mut()
            .push(AcceptCall::CurrentAccount);
        ready(match next_api_step(&self.script.accounts) {
            ApiStep::Success => account().map_err(|_| FakeApiError),
            ApiStep::Failure => Err(FakeApiError),
        })
    }
}

impl RequestLookupApi for FakeAcceptApi {
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
            .push(AcceptCall::RequestLookup {
                current_user_id: current_user_id.to_string(),
                request_id: request_id.to_string(),
            });
        ready(match next_api_step(&self.script.requests) {
            ApiStep::Success => request_record().map_err(|_| FakeApiError),
            ApiStep::Failure => Err(FakeApiError),
        })
    }
}

impl UserLookupApi for FakeAcceptApi {
    type Error = FakeApiError;

    fn user_by_id<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        user_id: &'a UserId,
    ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(AcceptCall::UserLookup {
            user_id: user_id.to_string(),
        });
        ready(match next_api_step(&self.script.users) {
            ApiStep::Success => requester().map_err(|_| FakeApiError),
            ApiStep::Failure => Err(FakeApiError),
        })
    }
}

impl BalanceApi for FakeAcceptApi {
    type Error = FakeApiError;

    fn balance<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Balance, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(AcceptCall::Balance);
        ready(match next_api_step(&self.script.balances) {
            ApiStep::Success => Ok(balance()),
            ApiStep::Failure => Err(FakeApiError),
        })
    }
}

impl RequestAcceptanceApi for FakeAcceptApi {
    type Error = FakeApiError;

    fn accept_request<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        plan: &'a AcceptRequestPlan,
    ) -> impl Future<Output = Result<AcceptedRequest, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(AcceptCall::Accept {
            plan: AcceptPlanCall::from(plan),
        });
        let step = self
            .script
            .acceptances
            .borrow_mut()
            .pop_front()
            .unwrap_or(WriteStep::Failure);
        AcceptFuture::new(step)
    }
}

fn next_api_step(steps: &RefCell<VecDeque<ApiStep>>) -> ApiStep {
    steps.borrow_mut().pop_front().unwrap_or(ApiStep::Failure)
}

struct AcceptFuture {
    step: WriteStep,
}

impl AcceptFuture {
    const fn new(step: WriteStep) -> Self {
        Self { step }
    }
}

impl Future for AcceptFuture {
    type Output = Result<AcceptedRequest, FakeApiError>;

    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        match self.step {
            WriteStep::Success => Poll::Ready(accepted_request().map_err(|_| FakeApiError)),
            WriteStep::Pending => Poll::Pending,
            WriteStep::Failure => Poll::Ready(Err(FakeApiError)),
        }
    }
}

struct FakePrompt {
    script: Rc<AcceptScriptState>,
    transcript: Transcript,
}

impl PromptAvailability for FakePrompt {
    fn can_prompt(&self) -> bool {
        self.transcript
            .borrow_mut()
            .push(AcceptCall::PromptAvailability);
        true
    }
}

impl DefaultNoConfirmation for FakePrompt {
    fn confirm_default_no(&self, prompt: &str) -> Result<bool, PromptError> {
        self.transcript
            .borrow_mut()
            .push(AcceptCall::ConfirmDefaultNo {
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
                source: io::Error::other("synthetic acceptance confirmation failure"),
            }),
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

#[derive(Debug, Default, Eq, PartialEq)]
struct AcceptState {
    calls: Vec<AcceptCall>,
    remaining: Option<RemainingAcceptScript>,
    stdout: WriterState,
    stderr: WriterState,
}

struct AcceptHarness {
    args: AcceptArgs,
    script: Rc<AcceptScriptState>,
    transcript: Transcript,
    reader: FakeReader,
    api: FakeAcceptApi,
    prompt: FakePrompt,
    stdout: OrderedWriter,
    stderr: OrderedWriter,
}

impl AcceptHarness {
    fn new(script: AcceptScript, initial: AcceptState) -> TestResult<Self> {
        let transcript = Rc::new(RefCell::new(initial.calls));
        let script = Rc::new(AcceptScriptState::new(script));
        Ok(Self {
            args: accept_args()?,
            reader: FakeReader {
                script: Rc::clone(&script),
                transcript: Rc::clone(&transcript),
            },
            api: FakeAcceptApi {
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
        run_accept_with(
            self.args.clone(),
            &self.reader,
            &self.api,
            &self.prompt,
            &mut self.stdout,
            &mut self.stderr,
            move || {
                transcript
                    .borrow_mut()
                    .push(AcceptCall::InstallInterruption);
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

    fn observed(self, result: Result<(), AppError>) -> Observed<ResultSnapshot, AcceptState> {
        Observed::new(
            snapshot_result(result),
            AcceptState {
                calls: self.transcript.borrow().clone(),
                remaining: Some(self.script.remaining()),
                stdout: self.stdout.state,
                stderr: self.stderr.state,
            },
        )
    }
}

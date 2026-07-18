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
use crate::adapters::cli::args::{Cli, Command, PayArgs};
use crate::adapters::cli::error::{AppError, ErrorCategory};
use crate::features::auth::{CurrentAccountApi, PromptAvailability, PromptError};
use crate::features::payments::{
    BlankSourceEligibility, BlankSourceEligibilityApi, CreatedPayment, DefaultNoConfirmation,
    EligibilityToken, FinancialStatus, PayPlan, PaymentCreationApi, PaymentId, PeerFundingApi,
    PeerFundingFee, PeerFundingMethod, PeerFundingRole,
};
use crate::features::people::{
    User, UserLookupApi, UserProfileKind, UserSearchApi, UserSearchPage, UserSearchPageRequest,
    UserSearchQuery,
};
use crate::features::wallet::{
    Balance, BalanceApi, PaymentMethod, PaymentMethodId, SignedUsdAmount,
};
use crate::shared::test_support::Observed;
use crate::shared::{
    AccessToken, Account, ApiFailure, ApiFailureKind, ClientRequestId, ClientRequestIdGenerator,
    CredentialCapability, CredentialEnvelope, CredentialFailureKind, CredentialFormat,
    CredentialReader, CredentialStoreFailure, DeviceId, LoadedCredential, Money, Note, UserId,
    Username, Visibility,
};

use super::run_pay_with;

#[path = "writes_pay_tests/expectations.rs"]
mod expectations;
#[path = "writes_pay_tests/fixtures.rs"]
mod fixtures;
#[path = "writes_pay_tests/interruption_output.rs"]
mod interruption_output;
#[path = "writes_pay_tests/output.rs"]
mod output;
#[path = "writes_pay_tests/success_preflight.rs"]
mod success_preflight;

use expectations::*;
use fixtures::*;
use output::*;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;
type Transcript = Rc<RefCell<Vec<PayCall>>>;

const REQUEST_ID: &str = "123e4567-e89b-12d3-a456-426614174000";

#[derive(Clone, Debug, Eq, PartialEq)]
struct PayPlanCall {
    request_id: String,
    account_user_id: String,
    recipient_user_id: String,
    amount_cents: u64,
    note: String,
    backup_method_id: String,
    eligibility_fee_cents: u64,
    visibility: Visibility,
}

impl From<&PayPlan> for PayPlanCall {
    fn from(plan: &PayPlan) -> Self {
        Self {
            request_id: plan.request_id().to_string(),
            account_user_id: plan.account().user_id().to_string(),
            recipient_user_id: plan.recipient().user_id().to_string(),
            amount_cents: plan.amount().cents(),
            note: plan.note().as_str().to_owned(),
            backup_method_id: plan.backup_method().method().id().to_string(),
            eligibility_fee_cents: plan.eligibility_fee_cents(),
            visibility: plan.visibility(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PayCall {
    ReadCredential,
    CurrentAccount,
    UserById {
        user_id: String,
    },
    SearchUsers,
    Balance,
    FundingMethods,
    Eligibility {
        recipient_user_id: String,
        amount_cents: u64,
        note: String,
    },
    GenerateClientRequestId,
    StderrWrite,
    StderrFlush,
    PromptAvailability,
    ConfirmDefaultNo {
        prompt: String,
    },
    InstallInterruption,
    CreatePayment {
        plan: PayPlanCall,
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
enum CreationStep {
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
    InstallationFailure,
}

struct PayScript {
    credentials: Vec<CredentialStep>,
    accounts: Vec<ApiStep>,
    users: Vec<ApiStep>,
    balances: Vec<ApiStep>,
    funding_methods: Vec<ApiStep>,
    eligibility: Vec<ApiStep>,
    request_ids: Vec<()>,
    confirmations: Vec<ConfirmationStep>,
    interruptions: Vec<InterruptionStep>,
    creations: Vec<CreationStep>,
}

impl PayScript {
    fn successful() -> Self {
        Self {
            credentials: vec![CredentialStep::Present, CredentialStep::Missing],
            accounts: vec![ApiStep::Success, ApiStep::Failure],
            users: vec![ApiStep::Success, ApiStep::Failure],
            balances: vec![ApiStep::Success, ApiStep::Failure],
            funding_methods: vec![ApiStep::Success, ApiStep::Failure],
            eligibility: vec![ApiStep::Success, ApiStep::Failure],
            request_ids: vec![(), ()],
            confirmations: vec![ConfirmationStep::Answer(true), ConfirmationStep::Failure],
            interruptions: vec![InterruptionStep::Pending, InterruptionStep::StreamFailure],
            creations: vec![CreationStep::Success, CreationStep::Failure],
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

    fn with_first_creation(mut self, step: CreationStep) -> Self {
        self.creations = vec![step, CreationStep::Failure];
        self
    }
}

struct PayScriptState {
    credentials: RefCell<VecDeque<CredentialStep>>,
    accounts: RefCell<VecDeque<ApiStep>>,
    users: RefCell<VecDeque<ApiStep>>,
    balances: RefCell<VecDeque<ApiStep>>,
    funding_methods: RefCell<VecDeque<ApiStep>>,
    eligibility: RefCell<VecDeque<ApiStep>>,
    request_ids: RefCell<VecDeque<()>>,
    confirmations: RefCell<VecDeque<ConfirmationStep>>,
    interruptions: RefCell<VecDeque<InterruptionStep>>,
    creations: RefCell<VecDeque<CreationStep>>,
}

impl PayScriptState {
    fn new(script: PayScript) -> Self {
        Self {
            credentials: RefCell::new(script.credentials.into()),
            accounts: RefCell::new(script.accounts.into()),
            users: RefCell::new(script.users.into()),
            balances: RefCell::new(script.balances.into()),
            funding_methods: RefCell::new(script.funding_methods.into()),
            eligibility: RefCell::new(script.eligibility.into()),
            request_ids: RefCell::new(script.request_ids.into()),
            confirmations: RefCell::new(script.confirmations.into()),
            interruptions: RefCell::new(script.interruptions.into()),
            creations: RefCell::new(script.creations.into()),
        }
    }

    fn remaining(&self) -> RemainingPayScript {
        RemainingPayScript {
            credentials: self.credentials.borrow().len(),
            accounts: self.accounts.borrow().len(),
            users: self.users.borrow().len(),
            balances: self.balances.borrow().len(),
            funding_methods: self.funding_methods.borrow().len(),
            eligibility: self.eligibility.borrow().len(),
            request_ids: self.request_ids.borrow().len(),
            confirmations: self.confirmations.borrow().len(),
            interruptions: self.interruptions.borrow().len(),
            creations: self.creations.borrow().len(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RemainingPayScript {
    credentials: usize,
    accounts: usize,
    users: usize,
    balances: usize,
    funding_methods: usize,
    eligibility: usize,
    request_ids: usize,
    confirmations: usize,
    interruptions: usize,
    creations: usize,
}

impl RemainingPayScript {
    const fn after_success() -> Self {
        Self {
            credentials: 1,
            accounts: 1,
            users: 1,
            balances: 1,
            funding_methods: 1,
            eligibility: 1,
            request_ids: 1,
            confirmations: 1,
            interruptions: 1,
            creations: 1,
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
    script: Rc<PayScriptState>,
    transcript: Transcript,
}

impl CredentialCapability for FakeReader {
    type Error = FakeCredentialError;
}

impl CredentialReader for FakeReader {
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        self.transcript.borrow_mut().push(PayCall::ReadCredential);
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
#[error("synthetic payment API failure")]
struct FakeApiError;

impl ApiFailure for FakeApiError {
    fn kind(&self) -> ApiFailureKind {
        ApiFailureKind::Internal
    }
}

struct FakePayApi {
    script: Rc<PayScriptState>,
    transcript: Transcript,
}

impl CurrentAccountApi for FakePayApi {
    type Error = FakeApiError;

    fn current_account<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(PayCall::CurrentAccount);
        ready(match next_api_step(&self.script.accounts) {
            ApiStep::Success => account().map_err(|_| FakeApiError),
            ApiStep::Failure => Err(FakeApiError),
        })
    }
}

impl UserLookupApi for FakePayApi {
    type Error = FakeApiError;

    fn user_by_id<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        user_id: &'a UserId,
    ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(PayCall::UserById {
            user_id: user_id.to_string(),
        });
        ready(match next_api_step(&self.script.users) {
            ApiStep::Success => recipient().map_err(|_| FakeApiError),
            ApiStep::Failure => Err(FakeApiError),
        })
    }
}

impl UserSearchApi for FakePayApi {
    type Error = FakeApiError;

    fn search_users<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        _query: &'a UserSearchQuery,
        _page: UserSearchPageRequest,
    ) -> impl Future<Output = Result<UserSearchPage, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(PayCall::SearchUsers);
        ready(
            recipient()
                .map(|user| UserSearchPage::new(vec![user], None))
                .map_err(|_| FakeApiError),
        )
    }
}

impl BalanceApi for FakePayApi {
    type Error = FakeApiError;

    fn balance<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Balance, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(PayCall::Balance);
        ready(match next_api_step(&self.script.balances) {
            ApiStep::Success => Ok(balance()),
            ApiStep::Failure => Err(FakeApiError),
        })
    }
}

impl PeerFundingApi for FakePayApi {
    type Error = FakeApiError;

    fn peer_funding_methods<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Vec<PeerFundingMethod>, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(PayCall::FundingMethods);
        ready(match next_api_step(&self.script.funding_methods) {
            ApiStep::Success => funding_method()
                .map(|method| vec![method])
                .map_err(|_| FakeApiError),
            ApiStep::Failure => Err(FakeApiError),
        })
    }
}

impl BlankSourceEligibilityApi for FakePayApi {
    type Error = FakeApiError;

    fn blank_source_eligibility<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        recipient: &'a User,
        amount: Money,
        note: &'a Note,
    ) -> impl Future<Output = Result<BlankSourceEligibility, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(PayCall::Eligibility {
            recipient_user_id: recipient.user_id().to_string(),
            amount_cents: amount.cents(),
            note: note.as_str().to_owned(),
        });
        ready(match next_api_step(&self.script.eligibility) {
            ApiStep::Success => EligibilityToken::parse_owned("synthetic-eligibility".to_owned())
                .map(|token| BlankSourceEligibility::new(token, 0))
                .map_err(|_| FakeApiError),
            ApiStep::Failure => Err(FakeApiError),
        })
    }
}

impl PaymentCreationApi for FakePayApi {
    type Error = FakeApiError;

    fn create_payment<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        plan: &'a PayPlan,
    ) -> impl Future<Output = Result<CreatedPayment, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(PayCall::CreatePayment {
            plan: PayPlanCall::from(plan),
        });
        let step = self
            .script
            .creations
            .borrow_mut()
            .pop_front()
            .unwrap_or(CreationStep::Failure);
        CreationFuture::new(step)
    }
}

fn next_api_step(steps: &RefCell<VecDeque<ApiStep>>) -> ApiStep {
    steps.borrow_mut().pop_front().unwrap_or(ApiStep::Failure)
}

struct CreationFuture {
    step: CreationStep,
}

impl CreationFuture {
    const fn new(step: CreationStep) -> Self {
        Self { step }
    }
}

impl Future for CreationFuture {
    type Output = Result<CreatedPayment, FakeApiError>;

    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        match self.step {
            CreationStep::Success => Poll::Ready(created_payment().map_err(|_| FakeApiError)),
            CreationStep::Pending => Poll::Pending,
            CreationStep::Failure => Poll::Ready(Err(FakeApiError)),
        }
    }
}

struct FixedGenerator {
    script: Rc<PayScriptState>,
    transcript: Transcript,
}

impl ClientRequestIdGenerator for FixedGenerator {
    fn generate(&self) -> ClientRequestId {
        self.transcript
            .borrow_mut()
            .push(PayCall::GenerateClientRequestId);
        let _ = self.script.request_ids.borrow_mut().pop_front();
        fixed_request_id()
    }
}

struct FakePrompt {
    script: Rc<PayScriptState>,
    transcript: Transcript,
}

impl PromptAvailability for FakePrompt {
    fn can_prompt(&self) -> bool {
        self.transcript
            .borrow_mut()
            .push(PayCall::PromptAvailability);
        true
    }
}

impl DefaultNoConfirmation for FakePrompt {
    fn confirm_default_no(&self, prompt: &str) -> Result<bool, PromptError> {
        self.transcript
            .borrow_mut()
            .push(PayCall::ConfirmDefaultNo {
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
                source: io::Error::other("synthetic confirmation failure"),
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
            InterruptionStep::StreamFailure | InterruptionStep::InstallationFailure => {
                Poll::Ready(Err(signal_error("synthetic signal stream failure")))
            }
        }
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
struct PayState {
    calls: Vec<PayCall>,
    remaining: Option<RemainingPayScript>,
    stdout: WriterState,
    stderr: WriterState,
}

struct PayHarness {
    args: PayArgs,
    script: Rc<PayScriptState>,
    transcript: Transcript,
    reader: FakeReader,
    api: FakePayApi,
    generator: FixedGenerator,
    prompt: FakePrompt,
    stdout: OrderedWriter,
    stderr: OrderedWriter,
}

impl PayHarness {
    fn new(script: PayScript, initial: PayState) -> TestResult<Self> {
        let transcript = Rc::new(RefCell::new(initial.calls));
        let script = Rc::new(PayScriptState::new(script));
        Ok(Self {
            args: pay_args()?,
            reader: FakeReader {
                script: Rc::clone(&script),
                transcript: Rc::clone(&transcript),
            },
            api: FakePayApi {
                script: Rc::clone(&script),
                transcript: Rc::clone(&transcript),
            },
            generator: FixedGenerator {
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
        run_pay_with(
            self.args.clone(),
            &self.reader,
            &self.api,
            &self.generator,
            &self.prompt,
            &mut self.stdout,
            &mut self.stderr,
            move || {
                transcript.borrow_mut().push(PayCall::InstallInterruption);
                let step = script
                    .interruptions
                    .borrow_mut()
                    .pop_front()
                    .unwrap_or(InterruptionStep::InstallationFailure);
                if matches!(step, InterruptionStep::InstallationFailure) {
                    Err(signal_error("synthetic signal installation failure"))
                } else {
                    Ok(InterruptionFuture::new(step))
                }
            },
        )
        .await
    }

    fn observed(self, result: Result<(), AppError>) -> Observed<ResultSnapshot, PayState> {
        Observed::new(
            snapshot_result(result),
            PayState {
                calls: self.transcript.borrow().clone(),
                remaining: Some(self.script.remaining()),
                stdout: self.stdout.state,
                stderr: self.stderr.state,
            },
        )
    }
}

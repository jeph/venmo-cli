use std::cell::RefCell;
use std::error::Error;
use std::future::{Future, ready};
use std::io;
use std::rc::Rc;
use std::str::FromStr;

use time::OffsetDateTime;

use super::*;
use crate::features::auth::PromptAvailability;
use crate::features::payments::{FinancialStatus, PaymentId};
use crate::features::people::{User, UserProfileKind};
use crate::features::requests::{RequestAction, RequestDirection, RequestRecord, RequestStatus};
use crate::features::wallet::{Balance, SignedUsdAmount};
use crate::shared::{
    AccessToken, Account, ApiFailureKind, CredentialAccessError, CredentialCapability,
    CredentialFailureKind, CredentialFormat, CredentialStoreFailure, DeviceId, LoadedCredential,
    Money, UserId, Username,
};

type TestResult = Result<(), Box<dyn Error>>;
type Transcript = Rc<RefCell<Vec<Call>>>;

const ACCESS_TOKEN: &str = "synthetic-secret-request-token";
const DEVICE_ID: &str = "synthetic-secret-request-device";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RedactedSecret {
    Redacted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AcceptPlanCall {
    account: Account,
    request: RequestRecord,
    balance: Balance,
}

impl From<&AcceptRequestPlan> for AcceptPlanCall {
    fn from(plan: &AcceptRequestPlan) -> Self {
        Self {
            account: plan.account().clone(),
            request: plan.request().clone(),
            balance: plan.balance().clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Call {
    ReadCredential,
    CurrentAccount {
        session: RedactedSecret,
    },
    RequestLookup {
        session: RedactedSecret,
        current_user_id: UserId,
        request_id: RequestId,
    },
    UserLookup {
        session: RedactedSecret,
        user_id: UserId,
    },
    Balance {
        session: RedactedSecret,
    },
    PromptAvailability,
    ConfirmDefaultNo {
        prompt: String,
    },
    Accept {
        session: RedactedSecret,
        plan: Box<AcceptPlanCall>,
    },
}

#[derive(Debug, Eq, PartialEq)]
struct Observation<Outcome> {
    outcome: Outcome,
    transcript: Vec<Call>,
}

impl<Outcome> Observation<Outcome> {
    const fn new(outcome: Outcome, transcript: Vec<Call>) -> Self {
        Self {
            outcome,
            transcript,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PromptFailure {
    Cancelled,
    Interaction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreflightFailure {
    CredentialMissing,
    CredentialRead,
    CurrentAccount(ApiFailureKind),
    Account(FinancialValidationError),
    RequestLookup(ApiFailureKind),
    RequestValidation(RequestMutationValidationError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AcceptFailure {
    Preflight(PreflightFailure),
    AccountOrRequester(FinancialValidationError),
    RequesterLookup(ApiFailureKind),
    RequesterIdentityMismatch,
    RequestValidation(RequestMutationValidationError),
    Balance(ApiFailureKind),
    InsufficientWalletBalance,
    ConfirmationRequired,
    ConfirmationDeclined,
    Confirmation(PromptFailure),
    Accept(ApiFailureKind),
}

#[derive(Debug, Eq, PartialEq)]
enum AcceptOutcome {
    Success(Box<AcceptResult>),
    Failure(AcceptFailure),
}

fn project(result: Result<AcceptResult, AcceptError>) -> AcceptOutcome {
    match result {
        Ok(result) => AcceptOutcome::Success(Box::new(result)),
        Err(error) => AcceptOutcome::Failure(match error {
            AcceptError::Preflight(RequestMutationPreflightError::Credential(
                CredentialAccessError::Missing,
            )) => AcceptFailure::Preflight(PreflightFailure::CredentialMissing),
            AcceptError::Preflight(RequestMutationPreflightError::Credential(
                CredentialAccessError::Read { .. },
            )) => AcceptFailure::Preflight(PreflightFailure::CredentialRead),
            AcceptError::Preflight(RequestMutationPreflightError::CurrentAccount { source }) => {
                AcceptFailure::Preflight(PreflightFailure::CurrentAccount(source.kind()))
            }
            AcceptError::Preflight(RequestMutationPreflightError::Account(source)) => {
                AcceptFailure::Preflight(PreflightFailure::Account(source))
            }
            AcceptError::Preflight(RequestMutationPreflightError::RequestLookup { source }) => {
                AcceptFailure::Preflight(PreflightFailure::RequestLookup(source.kind()))
            }
            AcceptError::Preflight(RequestMutationPreflightError::RequestValidation(source)) => {
                AcceptFailure::Preflight(PreflightFailure::RequestValidation(source))
            }
            AcceptError::AccountOrRequester(source) => AcceptFailure::AccountOrRequester(source),
            AcceptError::RequesterLookup { source } => {
                AcceptFailure::RequesterLookup(source.kind())
            }
            AcceptError::RequesterIdentityMismatch => AcceptFailure::RequesterIdentityMismatch,
            AcceptError::RequestValidation(source) => AcceptFailure::RequestValidation(source),
            AcceptError::Balance { source } => AcceptFailure::Balance(source.kind()),
            AcceptError::InsufficientWalletBalance => AcceptFailure::InsufficientWalletBalance,
            AcceptError::ConfirmationRequired => AcceptFailure::ConfirmationRequired,
            AcceptError::ConfirmationDeclined => AcceptFailure::ConfirmationDeclined,
            AcceptError::Confirmation {
                source: PromptError::Cancelled,
            } => AcceptFailure::Confirmation(PromptFailure::Cancelled),
            AcceptError::Confirmation { .. } => {
                AcceptFailure::Confirmation(PromptFailure::Interaction)
            }
            AcceptError::Accept { source } => AcceptFailure::Accept(source.kind()),
        }),
    }
}

#[derive(Clone, Copy)]
enum ReaderScript {
    Present,
    Missing,
    Failure,
}

struct FakeReader {
    script: ReaderScript,
    transcript: Transcript,
}

impl FakeReader {
    fn new(script: ReaderScript, transcript: Transcript) -> Self {
        Self { script, transcript }
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("synthetic credential failure")]
struct FakeCredentialError;

impl CredentialStoreFailure for FakeCredentialError {
    fn kind(&self) -> CredentialFailureKind {
        CredentialFailureKind::Unavailable
    }
}

impl CredentialCapability for FakeReader {
    type Error = FakeCredentialError;
}

impl CredentialReader for FakeReader {
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        self.transcript.borrow_mut().push(Call::ReadCredential);
        match self.script {
            ReaderScript::Present => loaded_credential().map(Some),
            ReaderScript::Missing => Ok(None),
            ReaderScript::Failure => Err(FakeCredentialError),
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

struct AcceptScript {
    account: Result<Account, FakeApiError>,
    request: Result<RequestRecord, FakeApiError>,
    user: Result<User, FakeApiError>,
    balance: Result<Balance, FakeApiError>,
    accepted: Result<AcceptedRequest, FakeApiError>,
}

impl AcceptScript {
    fn successful() -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            account: Ok(account("123")?),
            request: Ok(standard_request()?),
            user: Ok(financial_requester()?),
            balance: Ok(sufficient_balance()),
            accepted: Ok(accepted_request()?),
        })
    }

    fn with_account(self, account: Result<Account, FakeApiError>) -> Self {
        Self { account, ..self }
    }

    fn with_request(self, request: Result<RequestRecord, FakeApiError>) -> Self {
        Self { request, ..self }
    }

    fn with_user(self, user: Result<User, FakeApiError>) -> Self {
        Self { user, ..self }
    }

    fn with_balance(self, balance: Result<Balance, FakeApiError>) -> Self {
        Self { balance, ..self }
    }

    fn with_accepted(self, accepted: Result<AcceptedRequest, FakeApiError>) -> Self {
        Self { accepted, ..self }
    }
}

struct FakeApi {
    account: Result<Account, FakeApiError>,
    request: Result<RequestRecord, FakeApiError>,
    user: Result<User, FakeApiError>,
    balance: Result<Balance, FakeApiError>,
    accepted: Result<AcceptedRequest, FakeApiError>,
    transcript: Transcript,
}

impl FakeApi {
    fn new(script: AcceptScript, transcript: Transcript) -> Self {
        Self {
            account: script.account,
            request: script.request,
            user: script.user,
            balance: script.balance,
            accepted: script.accepted,
            transcript,
        }
    }
}

impl CurrentAccountApi for FakeApi {
    type Error = FakeApiError;

    fn current_account<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::CurrentAccount {
            session: RedactedSecret::Redacted,
        });
        ready(self.account.clone())
    }
}

impl RequestLookupApi for FakeApi {
    type Error = FakeApiError;

    fn request_by_id<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        request_id: &'a RequestId,
    ) -> impl Future<Output = Result<RequestRecord, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::RequestLookup {
            session: RedactedSecret::Redacted,
            current_user_id: current_user_id.clone(),
            request_id: request_id.clone(),
        });
        ready(self.request.clone())
    }
}

impl UserLookupApi for FakeApi {
    type Error = FakeApiError;

    fn user_by_id<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        user_id: &'a UserId,
    ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::UserLookup {
            session: RedactedSecret::Redacted,
            user_id: user_id.clone(),
        });
        ready(self.user.clone())
    }
}

impl BalanceApi for FakeApi {
    type Error = FakeApiError;

    fn balance<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Balance, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::Balance {
            session: RedactedSecret::Redacted,
        });
        ready(self.balance.clone())
    }
}

impl RequestAcceptanceApi for FakeApi {
    type Error = FakeApiError;

    fn accept_request<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        plan: &'a AcceptRequestPlan,
    ) -> impl Future<Output = Result<AcceptedRequest, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::Accept {
            session: RedactedSecret::Redacted,
            plan: Box::new(AcceptPlanCall::from(plan)),
        });
        ready(self.accepted.clone())
    }
}

#[derive(Clone, Copy)]
enum PromptScript {
    Answer(bool),
    Cancelled,
    Interaction,
}

struct FakePrompt {
    interactive: bool,
    script: PromptScript,
    transcript: Transcript,
}

impl FakePrompt {
    fn new(interactive: bool, script: PromptScript, transcript: Transcript) -> Self {
        Self {
            interactive,
            script,
            transcript,
        }
    }
}

impl PromptAvailability for FakePrompt {
    fn can_prompt(&self) -> bool {
        self.transcript.borrow_mut().push(Call::PromptAvailability);
        self.interactive
    }
}

impl DefaultNoConfirmation for FakePrompt {
    fn confirm_default_no(&self, prompt: &str) -> Result<bool, PromptError> {
        self.transcript.borrow_mut().push(Call::ConfirmDefaultNo {
            prompt: prompt.to_owned(),
        });
        match self.script {
            PromptScript::Answer(answer) => Ok(answer),
            PromptScript::Cancelled => Err(PromptError::Cancelled),
            PromptScript::Interaction => Err(PromptError::Interaction {
                source: io::Error::other("synthetic accept confirmation failure"),
            }),
        }
    }
}

async fn run_accept(
    reader: &FakeReader,
    api: &FakeApi,
    prompt: &FakePrompt,
    request_id: &RequestId,
    assume_yes: bool,
) -> Result<AcceptResult, AcceptError> {
    let prepared = prepare(reader, api, request_id).await?;
    let authorized = authorize(prompt, prepared, assume_yes)?;
    execute(api, authorized).await
}

#[tokio::test(flavor = "current_thread")]
async fn complete_preflight_then_execute_has_one_ordered_write_with_complete_arguments()
-> TestResult {
    // Setup.
    let request_id = RequestId::from_str("request-1")?;
    let initial_request = standard_request()?;
    let requester = financial_requester()?;
    let final_request = initial_request.clone().with_counterparty(requester.clone());
    let account = account("123")?;
    let balance = sufficient_balance();
    let accepted = accepted_request()?;

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::new(ReaderScript::Present, Rc::clone(&transcript));
    let api = FakeApi::new(AcceptScript::successful()?, Rc::clone(&transcript));
    let prompt = FakePrompt::new(false, PromptScript::Answer(false), Rc::clone(&transcript));

    // Complete expected final outcome and state.
    let plan = AcceptRequestPlan::new(account.clone(), final_request.clone(), balance.clone());
    let expected = Observation::new(
        AcceptOutcome::Success(Box::new(AcceptResult::new(plan, accepted))),
        vec![
            Call::ReadCredential,
            current_account_call(),
            request_lookup_call(&request_id)?,
            user_lookup_call(requester.user_id()),
            balance_call(),
            Call::Accept {
                session: RedactedSecret::Redacted,
                plan: Box::new(AcceptPlanCall {
                    account,
                    request: final_request,
                    balance,
                }),
            },
        ],
    );

    // Execute once.
    let result = run_accept(&reader, &api, &prompt, &request_id, true).await;
    let observed = Observation::new(project(result), transcript.borrow().clone());

    assert_eq!(observed, expected);
    let rendered = format!("{:?}", observed.transcript);
    assert!(!rendered.contains(ACCESS_TOKEN));
    assert!(!rendered.contains(DEVICE_ID));
    Ok(())
}

#[derive(Clone, Copy)]
enum FailureStage {
    MissingCredential,
    CredentialRead,
    CurrentAccount,
    RequestLookup,
    RequesterLookup,
    Balance,
    Accept,
}

#[tokio::test(flavor = "current_thread")]
async fn credential_and_api_failures_preserve_categories_and_exact_stop_points() -> TestResult {
    for stage in [
        FailureStage::MissingCredential,
        FailureStage::CredentialRead,
        FailureStage::CurrentAccount,
        FailureStage::RequestLookup,
        FailureStage::RequesterLookup,
        FailureStage::Balance,
        FailureStage::Accept,
    ] {
        // Setup.
        let request_id = RequestId::from_str("request-1")?;
        let (reader_script, api_script, expected_failure, prefix_len) = failure_case(stage)?;

        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader::new(reader_script, Rc::clone(&transcript));
        let api = FakeApi::new(api_script, Rc::clone(&transcript));
        let prompt = FakePrompt::new(false, PromptScript::Answer(false), Rc::clone(&transcript));

        // Complete expected final outcome and state.
        let expected = Observation::new(
            AcceptOutcome::Failure(expected_failure),
            successful_calls(&request_id)?
                .into_iter()
                .take(prefix_len)
                .collect(),
        );

        // Execute once.
        let result = run_accept(&reader, &api, &prompt, &request_id, true).await;
        let observed = Observation::new(project(result), transcript.borrow().clone());

        assert_eq!(observed, expected);
    }
    Ok(())
}

struct UnsafeCase {
    script: AcceptScript,
    returned_request: RequestRecord,
    expected_failure: AcceptFailure,
    prefix_len: usize,
}

#[tokio::test(flavor = "current_thread")]
async fn unsafe_contract_identity_financial_and_balance_variants_never_write() -> TestResult {
    let mismatched_id = request_record(
        "different-request",
        RequestAction::Charge,
        RequestDirection::Incoming,
        financial_requester()?,
        "pending",
        Some("Synthetic request"),
        Some("private"),
        Some(OffsetDateTime::UNIX_EPOCH),
    )?;
    let outgoing = request_record(
        "request-1",
        RequestAction::Charge,
        RequestDirection::Outgoing,
        financial_requester()?,
        "pending",
        Some("Synthetic request"),
        Some("private"),
        Some(OffsetDateTime::UNIX_EPOCH),
    )?;
    let missing_time = request_record(
        "request-1",
        RequestAction::Charge,
        RequestDirection::Incoming,
        financial_requester()?,
        "pending",
        Some("Synthetic request"),
        Some("private"),
        None,
    )?;
    let public = request_record(
        "request-1",
        RequestAction::Charge,
        RequestDirection::Incoming,
        financial_requester()?,
        "pending",
        Some("Synthetic request"),
        Some("public"),
        Some(OffsetDateTime::UNIX_EPOCH),
    )?;
    let standard = standard_request()?;
    let different_user = financial_user("999", Some("different"), UserProfileKind::Personal, true)?;
    let business = financial_user("456", Some("requester"), UserProfileKind::Business, true)?;
    let incomplete = financial_user("456", None, UserProfileKind::Personal, true)?;
    let cases = vec![
        UnsafeCase {
            script: AcceptScript::successful()?.with_request(Ok(mismatched_id.clone())),
            returned_request: mismatched_id,
            expected_failure: AcceptFailure::Preflight(PreflightFailure::RequestValidation(
                RequestMutationValidationError::MismatchedId,
            )),
            prefix_len: 3,
        },
        UnsafeCase {
            script: AcceptScript::successful()?.with_request(Ok(outgoing.clone())),
            returned_request: outgoing,
            expected_failure: AcceptFailure::Preflight(PreflightFailure::RequestValidation(
                RequestMutationValidationError::NotIncoming,
            )),
            prefix_len: 3,
        },
        UnsafeCase {
            script: AcceptScript::successful()?.with_request(Ok(missing_time.clone())),
            returned_request: missing_time,
            expected_failure: AcceptFailure::Preflight(PreflightFailure::RequestValidation(
                RequestMutationValidationError::MissingCreationTime,
            )),
            prefix_len: 3,
        },
        UnsafeCase {
            script: AcceptScript::successful()?.with_request(Ok(public.clone())),
            returned_request: public,
            expected_failure: AcceptFailure::RequestValidation(
                RequestMutationValidationError::UnverifiedAudience,
            ),
            prefix_len: 3,
        },
        UnsafeCase {
            script: AcceptScript::successful()?.with_user(Ok(different_user)),
            returned_request: standard.clone(),
            expected_failure: AcceptFailure::RequesterIdentityMismatch,
            prefix_len: 4,
        },
        UnsafeCase {
            script: AcceptScript::successful()?.with_user(Ok(business)),
            returned_request: standard.clone(),
            expected_failure: AcceptFailure::AccountOrRequester(
                FinancialValidationError::UnsupportedProfileType,
            ),
            prefix_len: 4,
        },
        UnsafeCase {
            script: AcceptScript::successful()?.with_user(Ok(incomplete)),
            returned_request: standard.clone(),
            expected_failure: AcceptFailure::RequestValidation(
                RequestMutationValidationError::IncompleteRequester,
            ),
            prefix_len: 4,
        },
        UnsafeCase {
            script: AcceptScript::successful()?.with_balance(Ok(Balance::new(
                SignedUsdAmount::from_cents(-1),
                SignedUsdAmount::from_cents(0),
            ))),
            returned_request: standard.clone(),
            expected_failure: AcceptFailure::InsufficientWalletBalance,
            prefix_len: 5,
        },
        UnsafeCase {
            script: AcceptScript::successful()?.with_balance(Ok(Balance::new(
                SignedUsdAmount::from_cents(0),
                SignedUsdAmount::from_cents(0),
            ))),
            returned_request: standard,
            expected_failure: AcceptFailure::InsufficientWalletBalance,
            prefix_len: 5,
        },
    ];

    for case in cases {
        // Setup and immutable initial state.
        let request_id = RequestId::from_str("request-1")?;
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader::new(ReaderScript::Present, Rc::clone(&transcript));
        let api = FakeApi::new(case.script, Rc::clone(&transcript));
        let prompt = FakePrompt::new(false, PromptScript::Answer(false), Rc::clone(&transcript));

        // Complete expected final outcome and state.
        let expected = Observation::new(
            AcceptOutcome::Failure(case.expected_failure),
            preparation_calls(&request_id, &case.returned_request)?
                .into_iter()
                .take(case.prefix_len)
                .collect(),
        );

        // Execute once.
        let result = run_accept(&reader, &api, &prompt, &request_id, true).await;
        let observed = Observation::new(project(result), transcript.borrow().clone());

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum ConfirmationCase {
    Required,
    Declined,
    Cancelled,
    Interaction,
    Confirmed,
}

#[tokio::test(flavor = "current_thread")]
async fn confirmation_required_declined_cancelled_failed_and_confirmed_are_distinct() -> TestResult
{
    for case in [
        ConfirmationCase::Required,
        ConfirmationCase::Declined,
        ConfirmationCase::Cancelled,
        ConfirmationCase::Interaction,
        ConfirmationCase::Confirmed,
    ] {
        // Setup.
        let request_id = RequestId::from_str("request-1")?;
        let (interactive, prompt_script, expected_outcome, prompt_calls) = confirmation_case(case)?;

        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader::new(ReaderScript::Present, Rc::clone(&transcript));
        let api = FakeApi::new(AcceptScript::successful()?, Rc::clone(&transcript));
        let prompt = FakePrompt::new(interactive, prompt_script, Rc::clone(&transcript));

        // Complete expected final outcome and state.
        let mut expected_calls = preparation_calls(&request_id, &standard_request()?)?;
        expected_calls.extend(prompt_calls);
        if matches!(case, ConfirmationCase::Confirmed) {
            let write = successful_calls(&request_id)?
                .into_iter()
                .nth(5)
                .ok_or_else(|| io::Error::other("missing synthetic accept call"))?;
            expected_calls.push(write);
        }
        let expected = Observation::new(expected_outcome, expected_calls);

        // Execute once.
        let result = run_accept(&reader, &api, &prompt, &request_id, false).await;
        let observed = Observation::new(project(result), transcript.borrow().clone());

        assert_eq!(observed, expected);
    }
    Ok(())
}

fn failure_case(
    stage: FailureStage,
) -> Result<(ReaderScript, AcceptScript, AcceptFailure, usize), Box<dyn Error>> {
    let script = AcceptScript::successful()?;
    Ok(match stage {
        FailureStage::MissingCredential => (
            ReaderScript::Missing,
            script,
            AcceptFailure::Preflight(PreflightFailure::CredentialMissing),
            1,
        ),
        FailureStage::CredentialRead => (
            ReaderScript::Failure,
            script,
            AcceptFailure::Preflight(PreflightFailure::CredentialRead),
            1,
        ),
        FailureStage::CurrentAccount => (
            ReaderScript::Present,
            script.with_account(Err(FakeApiError(ApiFailureKind::Rejected))),
            AcceptFailure::Preflight(PreflightFailure::CurrentAccount(ApiFailureKind::Rejected)),
            2,
        ),
        FailureStage::RequestLookup => (
            ReaderScript::Present,
            script.with_request(Err(FakeApiError(ApiFailureKind::Timeout))),
            AcceptFailure::Preflight(PreflightFailure::RequestLookup(ApiFailureKind::Timeout)),
            3,
        ),
        FailureStage::RequesterLookup => (
            ReaderScript::Present,
            script.with_user(Err(FakeApiError(ApiFailureKind::Network))),
            AcceptFailure::RequesterLookup(ApiFailureKind::Network),
            4,
        ),
        FailureStage::Balance => (
            ReaderScript::Present,
            script.with_balance(Err(FakeApiError(ApiFailureKind::Contract))),
            AcceptFailure::Balance(ApiFailureKind::Contract),
            5,
        ),
        FailureStage::Accept => (
            ReaderScript::Present,
            script.with_accepted(Err(FakeApiError(ApiFailureKind::AmbiguousWrite))),
            AcceptFailure::Accept(ApiFailureKind::AmbiguousWrite),
            6,
        ),
    })
}

type ConfirmationExpectation = (bool, PromptScript, AcceptOutcome, Vec<Call>);

fn confirmation_case(case: ConfirmationCase) -> Result<ConfirmationExpectation, Box<dyn Error>> {
    Ok(match case {
        ConfirmationCase::Required => (
            false,
            PromptScript::Answer(false),
            AcceptOutcome::Failure(AcceptFailure::ConfirmationRequired),
            vec![Call::PromptAvailability],
        ),
        ConfirmationCase::Declined => (
            true,
            PromptScript::Answer(false),
            AcceptOutcome::Failure(AcceptFailure::ConfirmationDeclined),
            confirmation_calls(),
        ),
        ConfirmationCase::Cancelled => (
            true,
            PromptScript::Cancelled,
            AcceptOutcome::Failure(AcceptFailure::Confirmation(PromptFailure::Cancelled)),
            confirmation_calls(),
        ),
        ConfirmationCase::Interaction => (
            true,
            PromptScript::Interaction,
            AcceptOutcome::Failure(AcceptFailure::Confirmation(PromptFailure::Interaction)),
            confirmation_calls(),
        ),
        ConfirmationCase::Confirmed => {
            let request = standard_request()?.with_counterparty(financial_requester()?);
            (
                true,
                PromptScript::Answer(true),
                AcceptOutcome::Success(Box::new(AcceptResult::new(
                    AcceptRequestPlan::new(account("123")?, request, sufficient_balance()),
                    accepted_request()?,
                ))),
                confirmation_calls(),
            )
        }
    })
}

fn confirmation_calls() -> Vec<Call> {
    vec![
        Call::PromptAvailability,
        Call::ConfirmDefaultNo {
            prompt: "Accept this request and pay its requester?".to_owned(),
        },
    ]
}

fn successful_calls(request_id: &RequestId) -> Result<Vec<Call>, Box<dyn Error>> {
    let initial_request = standard_request()?;
    let final_request = initial_request
        .clone()
        .with_counterparty(financial_requester()?);
    let account = account("123")?;
    let balance = sufficient_balance();
    let mut calls = preparation_calls(request_id, &initial_request)?;
    calls.push(Call::Accept {
        session: RedactedSecret::Redacted,
        plan: Box::new(AcceptPlanCall {
            account,
            request: final_request,
            balance,
        }),
    });
    Ok(calls)
}

fn preparation_calls(
    request_id: &RequestId,
    returned_request: &RequestRecord,
) -> Result<Vec<Call>, Box<dyn Error>> {
    Ok(vec![
        Call::ReadCredential,
        current_account_call(),
        request_lookup_call(request_id)?,
        user_lookup_call(returned_request.counterparty().user_id()),
        balance_call(),
    ])
}

const fn current_account_call() -> Call {
    Call::CurrentAccount {
        session: RedactedSecret::Redacted,
    }
}

fn request_lookup_call(request_id: &RequestId) -> Result<Call, Box<dyn Error>> {
    Ok(Call::RequestLookup {
        session: RedactedSecret::Redacted,
        current_user_id: UserId::from_str("123")?,
        request_id: request_id.clone(),
    })
}

fn user_lookup_call(user_id: &UserId) -> Call {
    Call::UserLookup {
        session: RedactedSecret::Redacted,
        user_id: user_id.clone(),
    }
}

const fn balance_call() -> Call {
    Call::Balance {
        session: RedactedSecret::Redacted,
    }
}

fn loaded_credential() -> Result<LoadedCredential, FakeCredentialError> {
    Ok(LoadedCredential {
        envelope: CredentialEnvelope::new(
            AccessToken::from_str(ACCESS_TOKEN).map_err(|_| FakeCredentialError)?,
            DeviceId::from_str(DEVICE_ID).map_err(|_| FakeCredentialError)?,
            UserId::from_str("123").map_err(|_| FakeCredentialError)?,
            Username::from_bare("owner").map_err(|_| FakeCredentialError)?,
            Some("Synthetic owner".to_owned()),
            OffsetDateTime::UNIX_EPOCH,
        ),
        format: CredentialFormat::Version1,
    })
}

fn account(user_id: &str) -> Result<Account, Box<dyn Error>> {
    Ok(Account::new(
        UserId::from_str(user_id)?,
        Username::from_bare(if user_id == "123" { "owner" } else { "other" })?,
        Some("Synthetic account".to_owned()),
    ))
}

fn financial_requester() -> Result<User, Box<dyn Error>> {
    financial_user("456", Some("requester"), UserProfileKind::Personal, true)
}

fn financial_user(
    user_id: &str,
    username: Option<&str>,
    profile_kind: UserProfileKind,
    is_payable: bool,
) -> Result<User, Box<dyn Error>> {
    Ok(User::new(
        UserId::from_str(user_id)?,
        username.map(Username::from_bare).transpose()?,
        Some("Synthetic requester".to_owned()),
    )
    .with_financial_attributes(profile_kind, is_payable))
}

fn standard_request() -> Result<RequestRecord, Box<dyn Error>> {
    request_record(
        "request-1",
        RequestAction::Charge,
        RequestDirection::Incoming,
        financial_requester()?,
        "pending",
        Some("Synthetic request"),
        Some("private"),
        Some(OffsetDateTime::UNIX_EPOCH),
    )
}

#[allow(clippy::too_many_arguments)]
fn request_record(
    request_id: &str,
    action: RequestAction,
    direction: RequestDirection,
    requester: User,
    status: &str,
    note: Option<&str>,
    audience: Option<&str>,
    created_at: Option<OffsetDateTime>,
) -> Result<RequestRecord, Box<dyn Error>> {
    Ok(RequestRecord::new(
        RequestId::from_str(request_id)?,
        action,
        direction,
        requester,
        Money::from_cents(1)?,
        note.map(str::to_owned),
        created_at,
        RequestStatus::from_str(status)?,
    )
    .with_audience(audience.map(str::to_owned)))
}

fn sufficient_balance() -> Balance {
    Balance::new(
        SignedUsdAmount::from_cents(1),
        SignedUsdAmount::from_cents(0),
    )
}

fn accepted_request() -> Result<AcceptedRequest, Box<dyn Error>> {
    Ok(AcceptedRequest::new(
        PaymentId::from_str("payment-1")?,
        FinancialStatus::Settled,
    ))
}

use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error;
use std::future::{Future, ready};
use std::rc::Rc;
use std::str::FromStr;

use time::OffsetDateTime;

use super::*;
use crate::features::auth::{OtpCode, PromptAvailability};
use crate::features::p2p_step_up::{P2pOtpVerification, P2pStepUpApi, P2pStepUpInput};
use crate::features::payments::FinancialValidationError;
use crate::features::people::{
    User, UserProfileKind, UserSearchPage, UserSearchPageRequest, UserSearchQuery,
};
use crate::features::requests::{RequestId, RequestStatus};
use crate::shared::{
    AccessToken, Account, ApiFailureKind, ClientRequestId, CredentialAccessError,
    CredentialCapability, CredentialFailureKind, CredentialFormat, CredentialStoreFailure,
    DeviceId, Limit, LoadedCredential, Offset, UserId, Username, Visibility,
};

type TestResult = Result<(), Box<dyn Error>>;
type Transcript = Rc<RefCell<Vec<Call>>>;

const ACCESS_TOKEN: &str = "synthetic-secret-create-token";
const DEVICE_ID: &str = "synthetic-secret-create-device";
const REQUEST_UUID: &str = "123e4567-e89b-12d3-a456-426614174000";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RedactedSecret {
    Redacted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CreatePlanCall {
    request_id: ClientRequestId,
    account: Account,
    recipient: User,
    amount: Money,
    note: Note,
    visibility: Visibility,
}

impl From<&CreateRequestPlan> for CreatePlanCall {
    fn from(plan: &CreateRequestPlan) -> Self {
        Self {
            request_id: plan.request_id(),
            account: plan.account().clone(),
            recipient: plan.recipient().clone(),
            amount: plan.amount(),
            note: plan.note().clone(),
            visibility: plan.visibility(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Call {
    ReadCredential,
    CurrentAccount {
        session: RedactedSecret,
    },
    UserLookup {
        session: RedactedSecret,
        user_id: UserId,
    },
    UserSearch {
        session: RedactedSecret,
        query: UserSearchQuery,
        page: UserSearchPageRequest,
    },
    GenerateClientRequestId {
        request_id: ClientRequestId,
    },
    Confirm {
        prompt: &'static str,
    },
    CreateRequest {
        session: RedactedSecret,
        plan: Box<CreatePlanCall>,
    },
}

#[derive(Debug, Eq, PartialEq)]
struct Observation {
    outcome: CreateOutcome,
    transcript: Vec<Call>,
}

#[derive(Debug, Eq, PartialEq)]
enum CreateOutcome {
    Success(Box<RequestCreateResult>),
    Failure(CreateFailure),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CreateFailure {
    CredentialMissing,
    CredentialRead,
    CurrentAccount(ApiFailureKind),
    Validation(FinancialValidationError),
    Recipient(crate::features::people::recipients::RecipientResolutionFailureKind),
    ConfirmationRequired,
    ConfirmationDeclined,
    Confirmation,
    Create(ApiFailureKind),
}

fn project(result: Result<RequestCreateResult, RequestCreateError>) -> CreateOutcome {
    match result {
        Ok(result) => CreateOutcome::Success(Box::new(result)),
        Err(error) => CreateOutcome::Failure(match error {
            RequestCreateError::Preflight(PeerPreflightError::Credential(
                CredentialAccessError::Missing,
            )) => CreateFailure::CredentialMissing,
            RequestCreateError::Preflight(PeerPreflightError::Credential(
                CredentialAccessError::Read { .. },
            )) => CreateFailure::CredentialRead,
            RequestCreateError::Preflight(PeerPreflightError::CurrentAccount { source }) => {
                CreateFailure::CurrentAccount(source.kind())
            }
            RequestCreateError::Preflight(PeerPreflightError::Validation(source)) => {
                CreateFailure::Validation(source)
            }
            RequestCreateError::Preflight(PeerPreflightError::Recipient(source)) => {
                CreateFailure::Recipient(source.failure_kind())
            }
            RequestCreateError::ConfirmationRequired => CreateFailure::ConfirmationRequired,
            RequestCreateError::ConfirmationDeclined => CreateFailure::ConfirmationDeclined,
            RequestCreateError::Confirmation { .. } => CreateFailure::Confirmation,
            RequestCreateError::Create { source } => CreateFailure::Create(source.kind()),
            RequestCreateError::StepUp(_) => CreateFailure::Create(ApiFailureKind::Rejected),
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
            ReaderScript::Present => credential().map(Some),
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

struct CreateScript {
    account: Result<Account, FakeApiError>,
    search_user: User,
    user: Result<User, FakeApiError>,
    creation: Result<CreatedRequest, FakeApiError>,
}

impl CreateScript {
    fn successful() -> Result<Self, Box<dyn Error>> {
        let user = financial_user("456", UserProfileKind::Personal)?;
        Ok(Self {
            account: Ok(account("123")?),
            search_user: user.clone(),
            user: Ok(user),
            creation: Ok(created_request()?),
        })
    }

    fn with_account(self, account: Result<Account, FakeApiError>) -> Self {
        Self { account, ..self }
    }

    fn with_user(self, user: Result<User, FakeApiError>) -> Self {
        Self { user, ..self }
    }

    fn with_creation(self, creation: Result<CreatedRequest, FakeApiError>) -> Self {
        Self { creation, ..self }
    }
}

struct FakeApi {
    account: Result<Account, FakeApiError>,
    search_user: User,
    user: Result<User, FakeApiError>,
    creation: Result<CreatedRequest, FakeApiError>,
    transcript: Transcript,
}

impl FakeApi {
    fn new(script: CreateScript, transcript: Transcript) -> Self {
        Self {
            account: script.account,
            search_user: script.search_user,
            user: script.user,
            creation: script.creation,
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

impl UserSearchApi for FakeApi {
    type Error = FakeApiError;

    fn search_users<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        query: &'a UserSearchQuery,
        page: UserSearchPageRequest,
    ) -> impl Future<Output = Result<UserSearchPage, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::UserSearch {
            session: RedactedSecret::Redacted,
            query: query.clone(),
            page,
        });
        ready(Ok(UserSearchPage::new(
            vec![self.search_user.clone()],
            None,
        )))
    }
}

impl RequestCreationApi for FakeApi {
    type Error = FakeApiError;

    fn create_request<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        plan: &'a CreateRequestPlan,
        _verification: RequestCreationVerification,
    ) -> impl Future<Output = Result<RequestCreationOutcome, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::CreateRequest {
            session: RedactedSecret::Redacted,
            plan: Box::new(CreatePlanCall::from(plan)),
        });
        ready(self.creation.clone().map(RequestCreationOutcome::Created))
    }
}

impl P2pStepUpApi for FakeApi {
    type Error = FakeApiError;

    fn issue_p2p_otp<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        _session_id: &'a ClientRequestId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
        ready(Err(FakeApiError(ApiFailureKind::Internal)))
    }

    fn verify_p2p_otp<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        _session_id: &'a ClientRequestId,
        _otp: &'a OtpCode,
    ) -> impl Future<Output = Result<P2pOtpVerification, Self::Error>> + Send + 'a {
        ready(Err(FakeApiError(ApiFailureKind::Internal)))
    }
}

struct FixedGenerator {
    transcript: Transcript,
}

struct UnusedPrompt;

impl PromptAvailability for UnusedPrompt {
    fn can_prompt(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy)]
enum PromptScript {
    Answer(bool),
    Cancelled,
    Failure,
}

struct FakePrompt {
    can_prompt: bool,
    script: PromptScript,
    transcript: Transcript,
}

impl PromptAvailability for FakePrompt {
    fn can_prompt(&self) -> bool {
        self.can_prompt
    }
}

impl DefaultNoConfirmation for FakePrompt {
    fn confirm_default_no(&self, prompt: &str) -> Result<bool, PromptError> {
        self.transcript.borrow_mut().push(Call::Confirm {
            prompt: if prompt == "Create this payment request?" {
                "Create this payment request?"
            } else {
                "unexpected prompt"
            },
        });
        match self.script {
            PromptScript::Answer(answer) => Ok(answer),
            PromptScript::Cancelled => Err(PromptError::Cancelled),
            PromptScript::Failure => Err(PromptError::Interaction {
                source: std::io::Error::other("synthetic prompt failure"),
            }),
        }
    }
}

impl DefaultNoConfirmation for UnusedPrompt {
    fn confirm_default_no(&self, _prompt: &str) -> Result<bool, PromptError> {
        Err(PromptError::Interaction {
            source: std::io::Error::other("unexpected synthetic prompt"),
        })
    }
}

impl P2pStepUpInput for UnusedPrompt {
    fn read_p2p_otp(&self, _prompt: &str) -> Result<OtpCode, PromptError> {
        Err(PromptError::Interaction {
            source: std::io::Error::other("unexpected synthetic OTP prompt"),
        })
    }
}

impl P2pStepUpInput for FakePrompt {
    fn read_p2p_otp(&self, _prompt: &str) -> Result<OtpCode, PromptError> {
        Err(PromptError::Interaction {
            source: std::io::Error::other("unexpected synthetic OTP prompt"),
        })
    }
}

impl ClientRequestIdGenerator for FixedGenerator {
    fn generate(&self) -> ClientRequestId {
        let request_id = fixed_request_id();
        self.transcript
            .borrow_mut()
            .push(Call::GenerateClientRequestId { request_id });
        request_id
    }
}

async fn run_create(
    reader: &FakeReader,
    api: &FakeApi,
    generator: &FixedGenerator,
    amount: Money,
    note: Note,
    visibility: Visibility,
) -> Result<RequestCreateResult, RequestCreateError> {
    let prepared = prepare_create(reader, api, generator, amount, note, visibility).await?;
    let authorized = authorize(&UnusedPrompt, prepared, true)?;
    execute(api, &UnusedPrompt, authorized).await
}

async fn prepare_create(
    reader: &FakeReader,
    api: &FakeApi,
    generator: &FixedGenerator,
    amount: Money,
    note: Note,
    visibility: Visibility,
) -> Result<PreparedRequest, RequestCreateError> {
    let recipient = RecipientInput::from_str("bob").map_err(|_| {
        RequestCreateError::Preflight(PeerPreflightError::Recipient(
            crate::features::people::recipients::RecipientResolutionError::Internal {
                problem: "synthetic recipient input was invalid",
            },
        ))
    })?;
    prepare(reader, api, generator, &recipient, amount, note, visibility).await
}

#[tokio::test(flavor = "current_thread")]
async fn create_uses_complete_arguments_and_exactly_one_ordered_write() -> TestResult {
    // Setup.
    let amount = Money::from_cents(125)?;
    let note = Note::from_str("Synthetic request note")?;
    let owner = account("123")?;
    let recipient = financial_user("456", UserProfileKind::Personal)?;
    let created = created_request()?;

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader {
        script: ReaderScript::Present,
        transcript: Rc::clone(&transcript),
    };
    let api = FakeApi::new(CreateScript::successful()?, Rc::clone(&transcript));
    let generator = FixedGenerator {
        transcript: Rc::clone(&transcript),
    };

    // Complete expected final outcome and state.
    let plan = CreateRequestPlan::new(
        fixed_request_id(),
        owner.clone(),
        recipient.clone(),
        amount,
        note.clone(),
        Visibility::Public,
    );
    let expected = Observation {
        outcome: CreateOutcome::Success(Box::new(RequestCreateResult::new(plan, created))),
        transcript: vec![
            Call::ReadCredential,
            current_account_call(),
            user_search_call()?,
            user_lookup_call()?,
            Call::GenerateClientRequestId {
                request_id: fixed_request_id(),
            },
            Call::CreateRequest {
                session: RedactedSecret::Redacted,
                plan: Box::new(CreatePlanCall {
                    request_id: fixed_request_id(),
                    account: owner,
                    recipient,
                    amount,
                    note: note.clone(),
                    visibility: Visibility::Public,
                }),
            },
        ],
    };

    // Execute once.
    let result = run_create(&reader, &api, &generator, amount, note, Visibility::Public).await;
    let observed = Observation {
        outcome: project(result),
        transcript: transcript.borrow().clone(),
    };

    assert_eq!(observed, expected);
    let rendered = format!("{:?}", observed.transcript);
    assert!(!rendered.contains(ACCESS_TOKEN));
    assert!(!rendered.contains(DEVICE_ID));
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CreateStepUpCall {
    Create(RequestCreationVerification, ClientRequestId),
    Issue(ClientRequestId),
    Prompt(String),
    Verify(ClientRequestId, String),
}

struct CreateStepUpApi {
    outcomes: RefCell<VecDeque<RequestCreationOutcome>>,
    calls: RefCell<Vec<CreateStepUpCall>>,
}

impl RequestCreationApi for CreateStepUpApi {
    type Error = FakeApiError;

    fn create_request<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        plan: &'a CreateRequestPlan,
        verification: RequestCreationVerification,
    ) -> impl Future<Output = Result<RequestCreationOutcome, Self::Error>> + Send + 'a {
        self.calls
            .borrow_mut()
            .push(CreateStepUpCall::Create(verification, plan.request_id()));
        ready(
            self.outcomes
                .borrow_mut()
                .pop_front()
                .ok_or(FakeApiError(ApiFailureKind::Internal)),
        )
    }
}

impl P2pStepUpApi for CreateStepUpApi {
    type Error = FakeApiError;

    fn issue_p2p_otp<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        session_id: &'a ClientRequestId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
        self.calls
            .borrow_mut()
            .push(CreateStepUpCall::Issue(*session_id));
        ready(Ok(()))
    }

    fn verify_p2p_otp<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        session_id: &'a ClientRequestId,
        otp: &'a OtpCode,
    ) -> impl Future<Output = Result<P2pOtpVerification, Self::Error>> + Send + 'a {
        self.calls.borrow_mut().push(CreateStepUpCall::Verify(
            *session_id,
            otp.expose().to_owned(),
        ));
        ready(Ok(P2pOtpVerification::Verified))
    }
}

struct CreateStepUpPrompt<'a> {
    calls: &'a RefCell<Vec<CreateStepUpCall>>,
}

impl PromptAvailability for CreateStepUpPrompt<'_> {
    fn can_prompt(&self) -> bool {
        true
    }
}

impl P2pStepUpInput for CreateStepUpPrompt<'_> {
    fn read_p2p_otp(&self, prompt: &str) -> Result<OtpCode, PromptError> {
        self.calls
            .borrow_mut()
            .push(CreateStepUpCall::Prompt(prompt.to_owned()));
        OtpCode::parse_owned("123456".to_owned()).map_err(|_| PromptError::Interaction {
            source: std::io::Error::other("synthetic OTP was invalid"),
        })
    }
}

#[tokio::test(flavor = "current_thread")]
async fn request_creation_step_up_uses_the_plan_uuid_and_one_verified_continuation() -> TestResult {
    let plan = CreateRequestPlan::new(
        fixed_request_id(),
        account("123")?,
        financial_user("456", UserProfileKind::Personal)?,
        Money::from_cents(1)?,
        Note::from_str("Synthetic note")?,
        Visibility::Private,
    );
    let api = CreateStepUpApi {
        outcomes: RefCell::new(VecDeque::from([
            RequestCreationOutcome::OtpStepUpRequired,
            RequestCreationOutcome::Created(created_request()?),
        ])),
        calls: RefCell::new(Vec::new()),
    };
    let prompt = CreateStepUpPrompt { calls: &api.calls };
    let authorized = AuthorizedRequest(PreparedRequest::new(credential()?.envelope, plan));

    let result = execute(&api, &prompt, authorized).await?;

    assert_eq!(result.created().id().as_str(), "request-1");
    assert_eq!(
        api.calls.into_inner(),
        vec![
            CreateStepUpCall::Create(RequestCreationVerification::Unverified, fixed_request_id(),),
            CreateStepUpCall::Issue(fixed_request_id()),
            CreateStepUpCall::Prompt("Venmo SMS verification code".to_owned()),
            CreateStepUpCall::Verify(fixed_request_id(), "123456".to_owned()),
            CreateStepUpCall::Create(
                RequestCreationVerification::SmsOtpVerified,
                fixed_request_id(),
            ),
        ]
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn request_creation_stops_after_one_verified_continuation() -> TestResult {
    let plan = CreateRequestPlan::new(
        fixed_request_id(),
        account("123")?,
        financial_user("456", UserProfileKind::Personal)?,
        Money::from_cents(1)?,
        Note::from_str("Synthetic note")?,
        Visibility::Private,
    );
    let api = CreateStepUpApi {
        outcomes: RefCell::new(VecDeque::from([
            RequestCreationOutcome::OtpStepUpRequired,
            RequestCreationOutcome::OtpStepUpRequired,
        ])),
        calls: RefCell::new(Vec::new()),
    };
    let prompt = CreateStepUpPrompt { calls: &api.calls };
    let authorized = AuthorizedRequest(PreparedRequest::new(credential()?.envelope, plan));

    let result = execute(&api, &prompt, authorized).await;

    assert!(matches!(
        result,
        Err(RequestCreateError::StepUp(P2pStepUpError::StillRequired))
    ));
    assert_eq!(
        api.calls
            .borrow()
            .iter()
            .filter(|call| matches!(call, CreateStepUpCall::Create(..)))
            .count(),
        2
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn creation_confirmation_required_declined_failed_and_yes_paths_are_distinct() -> TestResult {
    #[derive(Clone, Copy)]
    enum Expected {
        Required,
        Declined,
        Cancelled,
        Failed,
        Confirmed,
    }

    for (can_prompt, script, assume_yes, expected) in [
        (false, PromptScript::Answer(true), false, Expected::Required),
        (true, PromptScript::Answer(false), false, Expected::Declined),
        (true, PromptScript::Cancelled, false, Expected::Cancelled),
        (true, PromptScript::Failure, false, Expected::Failed),
        (true, PromptScript::Answer(true), false, Expected::Confirmed),
        (false, PromptScript::Failure, true, Expected::Confirmed),
    ] {
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader {
            script: ReaderScript::Present,
            transcript: Rc::clone(&transcript),
        };
        let api = FakeApi::new(CreateScript::successful()?, Rc::clone(&transcript));
        let generator = FixedGenerator {
            transcript: Rc::clone(&transcript),
        };
        let prepared = prepare_create(
            &reader,
            &api,
            &generator,
            Money::from_cents(125)?,
            Note::from_str("Synthetic request note")?,
            Visibility::Private,
        )
        .await?;
        let prompt = FakePrompt {
            can_prompt,
            script,
            transcript: Rc::clone(&transcript),
        };
        let authorized = authorize(&prompt, prepared, assume_yes);

        match (expected, authorized) {
            (Expected::Required, Err(RequestCreateError::ConfirmationRequired))
            | (Expected::Declined, Err(RequestCreateError::ConfirmationDeclined)) => {}
            (
                Expected::Cancelled,
                Err(RequestCreateError::Confirmation {
                    source: PromptError::Cancelled,
                }),
            ) => {}
            (
                Expected::Failed,
                Err(RequestCreateError::Confirmation {
                    source: PromptError::Interaction { .. },
                }),
            ) => {}
            (Expected::Confirmed, Ok(authorized)) => {
                execute(&api, &UnusedPrompt, authorized).await?;
            }
            _ => return Err(std::io::Error::other("unexpected confirmation outcome").into()),
        }

        let calls = transcript.borrow();
        let confirmation_count = calls
            .iter()
            .filter(|call| matches!(call, Call::Confirm { .. }))
            .count();
        assert_eq!(confirmation_count, usize::from(can_prompt && !assume_yes));
        let write_count = calls
            .iter()
            .filter(|call| matches!(call, Call::CreateRequest { .. }))
            .count();
        assert_eq!(
            write_count,
            usize::from(matches!(expected, Expected::Confirmed))
        );
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum FailureStage {
    MissingCredential,
    CredentialRead,
    CurrentAccount,
    UserLookup,
    ContractWrite,
    AmbiguousWrite,
}

#[tokio::test(flavor = "current_thread")]
async fn credential_api_contract_and_ambiguous_failures_stop_exactly_without_retry() -> TestResult {
    for stage in [
        FailureStage::MissingCredential,
        FailureStage::CredentialRead,
        FailureStage::CurrentAccount,
        FailureStage::UserLookup,
        FailureStage::ContractWrite,
        FailureStage::AmbiguousWrite,
    ] {
        // Setup.
        let amount = Money::from_cents(1)?;
        let note = Note::from_str("Synthetic note")?;
        let (reader_script, script, expected_failure, prefix_len) = failure_case(stage)?;

        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader {
            script: reader_script,
            transcript: Rc::clone(&transcript),
        };
        let api = FakeApi::new(script, Rc::clone(&transcript));
        let generator = FixedGenerator {
            transcript: Rc::clone(&transcript),
        };

        // Complete expected final outcome and state.
        let expected = Observation {
            outcome: CreateOutcome::Failure(expected_failure),
            transcript: successful_calls(amount, note.clone())?
                .into_iter()
                .take(prefix_len)
                .collect(),
        };

        // Execute once.
        let result = run_create(&reader, &api, &generator, amount, note, Visibility::Private).await;
        let observed = Observation {
            outcome: project(result),
            transcript: transcript.borrow().clone(),
        };

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn returned_recipient_id_mismatch_and_financial_rejection_never_generate_or_write()
-> TestResult {
    for (user, expected_failure) in [
        (
            financial_user("999", UserProfileKind::Personal)?,
            CreateFailure::Recipient(
                crate::features::people::recipients::RecipientResolutionFailureKind::ApiContract,
            ),
        ),
        (
            financial_user("456", UserProfileKind::Business)?,
            CreateFailure::Validation(FinancialValidationError::UnsupportedProfileType),
        ),
    ] {
        // Setup and immutable initial state.
        let amount = Money::from_cents(1)?;
        let note = Note::from_str("Synthetic note")?;
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader {
            script: ReaderScript::Present,
            transcript: Rc::clone(&transcript),
        };
        let api = FakeApi::new(
            CreateScript::successful()?.with_user(Ok(user)),
            Rc::clone(&transcript),
        );
        let generator = FixedGenerator {
            transcript: Rc::clone(&transcript),
        };

        // Complete expected final outcome and state.
        let expected = Observation {
            outcome: CreateOutcome::Failure(expected_failure),
            transcript: successful_calls(amount, note.clone())?
                .into_iter()
                .take(4)
                .collect(),
        };

        // Execute once.
        let result = run_create(&reader, &api, &generator, amount, note, Visibility::Private).await;
        let observed = Observation {
            outcome: project(result),
            transcript: transcript.borrow().clone(),
        };

        assert_eq!(observed, expected);
    }
    Ok(())
}

fn failure_case(
    stage: FailureStage,
) -> Result<(ReaderScript, CreateScript, CreateFailure, usize), Box<dyn Error>> {
    let script = CreateScript::successful()?;
    Ok(match stage {
        FailureStage::MissingCredential => (
            ReaderScript::Missing,
            script,
            CreateFailure::CredentialMissing,
            1,
        ),
        FailureStage::CredentialRead => (
            ReaderScript::Failure,
            script,
            CreateFailure::CredentialRead,
            1,
        ),
        FailureStage::CurrentAccount => (
            ReaderScript::Present,
            script.with_account(Err(FakeApiError(ApiFailureKind::Rejected))),
            CreateFailure::CurrentAccount(ApiFailureKind::Rejected),
            2,
        ),
        FailureStage::UserLookup => (
            ReaderScript::Present,
            script.with_user(Err(FakeApiError(ApiFailureKind::Timeout))),
            CreateFailure::Recipient(
                crate::features::people::recipients::RecipientResolutionFailureKind::Api(
                    ApiFailureKind::Timeout,
                ),
            ),
            4,
        ),
        FailureStage::ContractWrite => (
            ReaderScript::Present,
            script.with_creation(Err(FakeApiError(ApiFailureKind::Contract))),
            CreateFailure::Create(ApiFailureKind::Contract),
            6,
        ),
        FailureStage::AmbiguousWrite => (
            ReaderScript::Present,
            script.with_creation(Err(FakeApiError(ApiFailureKind::AmbiguousWrite))),
            CreateFailure::Create(ApiFailureKind::AmbiguousWrite),
            6,
        ),
    })
}

fn successful_calls(amount: Money, note: Note) -> Result<Vec<Call>, Box<dyn Error>> {
    Ok(vec![
        Call::ReadCredential,
        current_account_call(),
        user_search_call()?,
        user_lookup_call()?,
        Call::GenerateClientRequestId {
            request_id: fixed_request_id(),
        },
        Call::CreateRequest {
            session: RedactedSecret::Redacted,
            plan: Box::new(CreatePlanCall {
                request_id: fixed_request_id(),
                account: account("123")?,
                recipient: financial_user("456", UserProfileKind::Personal)?,
                amount,
                note,
                visibility: Visibility::Private,
            }),
        },
    ])
}

const fn current_account_call() -> Call {
    Call::CurrentAccount {
        session: RedactedSecret::Redacted,
    }
}

fn user_lookup_call() -> Result<Call, Box<dyn Error>> {
    Ok(Call::UserLookup {
        session: RedactedSecret::Redacted,
        user_id: UserId::from_str("456")?,
    })
}

fn user_search_call() -> Result<Call, Box<dyn Error>> {
    Ok(Call::UserSearch {
        session: RedactedSecret::Redacted,
        query: UserSearchQuery::from_str("bob")?,
        page: UserSearchPageRequest::new(Limit::try_from(50)?, Offset::default()),
    })
}

fn credential() -> Result<LoadedCredential, FakeCredentialError> {
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

fn fixed_request_id() -> ClientRequestId {
    match ClientRequestId::from_str(REQUEST_UUID) {
        Ok(request_id) => request_id,
        Err(_) => ClientRequestId::generate(),
    }
}

fn account(user_id: &str) -> Result<Account, Box<dyn Error>> {
    Ok(Account::new(
        UserId::from_str(user_id)?,
        Username::from_bare(if user_id == "123" { "owner" } else { "other" })?,
        Some("Synthetic owner".to_owned()),
    ))
}

fn financial_user(user_id: &str, profile_kind: UserProfileKind) -> Result<User, Box<dyn Error>> {
    Ok(User::new(
        UserId::from_str(user_id)?,
        Some(Username::from_bare("bob")?),
        Some("Synthetic recipient".to_owned()),
    )
    .with_financial_attributes(profile_kind, true))
}

fn created_request() -> Result<CreatedRequest, Box<dyn Error>> {
    Ok(CreatedRequest::new(
        RequestId::from_str("request-1")?,
        RequestStatus::from_str("pending")?,
    ))
}

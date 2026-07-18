use std::cell::RefCell;
use std::error::Error;
use std::future::{Future, ready};
use std::io;
use std::rc::Rc;
use std::str::FromStr;

use time::OffsetDateTime;

use super::*;
use crate::features::auth::PromptAvailability;
use crate::features::payments::FinancialValidationError;
use crate::features::people::User;
use crate::features::requests::validation::RequestMutationValidationError;
use crate::features::requests::{RequestAction, RequestDirection, RequestRecord, RequestStatus};
use crate::shared::{
    AccessToken, Account, ApiFailureKind, CredentialAccessError, CredentialCapability,
    CredentialFailureKind, CredentialFormat, CredentialStoreFailure, DeviceId, LoadedCredential,
    Money, UserId, Username,
};

type TestResult = Result<(), Box<dyn Error>>;
type Transcript = Rc<RefCell<Vec<Call>>>;

const ACCESS_TOKEN: &str = "synthetic-secret-decline-token";
const DEVICE_ID: &str = "synthetic-secret-decline-device";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RedactedSecret {
    Redacted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DeclinePlanCall {
    account: Account,
    request: RequestRecord,
}

impl From<&DeclineRequestPlan> for DeclinePlanCall {
    fn from(plan: &DeclineRequestPlan) -> Self {
        Self {
            account: plan.account().clone(),
            request: plan.request().clone(),
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
    PromptAvailability,
    ConfirmDefaultNo {
        prompt: String,
    },
    Decline {
        session: RedactedSecret,
        plan: Box<DeclinePlanCall>,
    },
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
enum DeclineFailure {
    Preflight(PreflightFailure),
    ConfirmationRequired,
    ConfirmationDeclined,
    Confirmation(PromptFailure),
    Decline(ApiFailureKind),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PromptFailure {
    Cancelled,
    Interaction,
}

#[derive(Debug, Eq, PartialEq)]
enum DeclineOutcome {
    Success(Box<DeclineResult>),
    Failure(DeclineFailure),
}

#[derive(Debug, Eq, PartialEq)]
struct Observation {
    outcome: DeclineOutcome,
    transcript: Vec<Call>,
}

fn project(result: Result<DeclineResult, DeclineError>) -> DeclineOutcome {
    match result {
        Ok(result) => DeclineOutcome::Success(Box::new(result)),
        Err(error) => DeclineOutcome::Failure(match error {
            DeclineError::Preflight(RequestMutationPreflightError::Credential(
                CredentialAccessError::Missing,
            )) => DeclineFailure::Preflight(PreflightFailure::CredentialMissing),
            DeclineError::Preflight(RequestMutationPreflightError::Credential(
                CredentialAccessError::Read { .. },
            )) => DeclineFailure::Preflight(PreflightFailure::CredentialRead),
            DeclineError::Preflight(RequestMutationPreflightError::CurrentAccount { source }) => {
                DeclineFailure::Preflight(PreflightFailure::CurrentAccount(source.kind()))
            }
            DeclineError::Preflight(RequestMutationPreflightError::Account(source)) => {
                DeclineFailure::Preflight(PreflightFailure::Account(source))
            }
            DeclineError::Preflight(RequestMutationPreflightError::RequestLookup { source }) => {
                DeclineFailure::Preflight(PreflightFailure::RequestLookup(source.kind()))
            }
            DeclineError::Preflight(RequestMutationPreflightError::RequestValidation(source)) => {
                DeclineFailure::Preflight(PreflightFailure::RequestValidation(source))
            }
            DeclineError::ConfirmationRequired => DeclineFailure::ConfirmationRequired,
            DeclineError::ConfirmationDeclined => DeclineFailure::ConfirmationDeclined,
            DeclineError::Confirmation {
                source: PromptError::Cancelled,
            } => DeclineFailure::Confirmation(PromptFailure::Cancelled),
            DeclineError::Confirmation { .. } => {
                DeclineFailure::Confirmation(PromptFailure::Interaction)
            }
            DeclineError::Decline { source } => DeclineFailure::Decline(source.kind()),
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

struct DeclineScript {
    account: Result<Account, FakeApiError>,
    request: Result<RequestRecord, FakeApiError>,
    declined: Result<DeclinedRequest, FakeApiError>,
}

impl DeclineScript {
    fn successful() -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            account: Ok(account("123")?),
            request: Ok(standard_request()?),
            declined: Ok(declined_request()?),
        })
    }

    fn with_account(self, account: Result<Account, FakeApiError>) -> Self {
        Self { account, ..self }
    }

    fn with_request(self, request: Result<RequestRecord, FakeApiError>) -> Self {
        Self { request, ..self }
    }

    fn with_declined(self, declined: Result<DeclinedRequest, FakeApiError>) -> Self {
        Self { declined, ..self }
    }
}

struct FakeApi {
    account: Result<Account, FakeApiError>,
    request: Result<RequestRecord, FakeApiError>,
    declined: Result<DeclinedRequest, FakeApiError>,
    transcript: Transcript,
}

impl FakeApi {
    fn new(script: DeclineScript, transcript: Transcript) -> Self {
        Self {
            account: script.account,
            request: script.request,
            declined: script.declined,
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

impl RequestDeclineApi for FakeApi {
    type Error = FakeApiError;

    fn decline_request<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        plan: &'a DeclineRequestPlan,
    ) -> impl Future<Output = Result<DeclinedRequest, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::Decline {
            session: RedactedSecret::Redacted,
            plan: Box::new(DeclinePlanCall::from(plan)),
        });
        ready(self.declined.clone())
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
                source: io::Error::other("synthetic decline confirmation failure"),
            }),
        }
    }
}

async fn run_decline(
    reader: &FakeReader,
    api: &FakeApi,
    request_id: &RequestId,
) -> Result<DeclineResult, DeclineError> {
    let prepared = prepare(reader, api, request_id).await?;
    let prompt = FakePrompt {
        interactive: false,
        script: PromptScript::Answer(false),
        transcript: Rc::clone(&reader.transcript),
    };
    let authorized = authorize(&prompt, prepared, true)?;
    execute(api, authorized).await
}

async fn run_decline_with_prompt(
    reader: &FakeReader,
    api: &FakeApi,
    prompt: &FakePrompt,
    request_id: &RequestId,
) -> Result<DeclineResult, DeclineError> {
    let prepared = prepare(reader, api, request_id).await?;
    let authorized = authorize(prompt, prepared, false)?;
    execute(api, authorized).await
}

#[tokio::test(flavor = "current_thread")]
async fn decline_uses_complete_lookup_and_plan_arguments_with_exactly_one_write() -> TestResult {
    // Setup.
    let request_id = RequestId::from_str("request-1")?;
    let owner = account("123")?;
    let request = standard_request()?;
    let declined = declined_request()?;

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader {
        script: ReaderScript::Present,
        transcript: Rc::clone(&transcript),
    };
    let api = FakeApi::new(DeclineScript::successful()?, Rc::clone(&transcript));

    // Complete expected final outcome and state.
    let plan = DeclineRequestPlan::new(owner.clone(), request.clone());
    let expected = Observation {
        outcome: DeclineOutcome::Success(Box::new(DeclineResult::new(plan, declined))),
        transcript: vec![
            Call::ReadCredential,
            current_account_call(),
            request_lookup_call(&request_id)?,
            Call::Decline {
                session: RedactedSecret::Redacted,
                plan: Box::new(DeclinePlanCall {
                    account: owner,
                    request,
                }),
            },
        ],
    };

    // Execute once.
    let result = run_decline(&reader, &api, &request_id).await;
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

#[derive(Clone, Copy)]
enum FailureStage {
    MissingCredential,
    CredentialRead,
    CurrentAccount,
    AccountMismatch,
    RequestLookup,
    ContractWrite,
    AmbiguousWrite,
}

#[tokio::test(flavor = "current_thread")]
async fn credential_api_contract_and_ambiguous_failures_have_exact_stop_points() -> TestResult {
    for stage in [
        FailureStage::MissingCredential,
        FailureStage::CredentialRead,
        FailureStage::CurrentAccount,
        FailureStage::AccountMismatch,
        FailureStage::RequestLookup,
        FailureStage::ContractWrite,
        FailureStage::AmbiguousWrite,
    ] {
        // Setup.
        let request_id = RequestId::from_str("request-1")?;
        let (reader_script, script, expected_failure, prefix_len) = failure_case(stage)?;

        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader {
            script: reader_script,
            transcript: Rc::clone(&transcript),
        };
        let api = FakeApi::new(script, Rc::clone(&transcript));

        // Complete expected final outcome and state.
        let expected = Observation {
            outcome: DeclineOutcome::Failure(expected_failure),
            transcript: successful_calls(&request_id)?
                .into_iter()
                .take(prefix_len)
                .collect(),
        };

        // Execute once.
        let result = run_decline(&reader, &api, &request_id).await;
        let observed = Observation {
            outcome: project(result),
            transcript: transcript.borrow().clone(),
        };

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn returned_id_mismatch_missing_creation_and_unsafe_state_never_write() -> TestResult {
    for (request, expected_error) in [
        (
            request_record(
                "different",
                RequestDirection::Incoming,
                "pending",
                Some(OffsetDateTime::UNIX_EPOCH),
            )?,
            RequestMutationValidationError::MismatchedId,
        ),
        (
            request_record("request-1", RequestDirection::Incoming, "pending", None)?,
            RequestMutationValidationError::MissingCreationTime,
        ),
        (
            request_record(
                "request-1",
                RequestDirection::Outgoing,
                "pending",
                Some(OffsetDateTime::UNIX_EPOCH),
            )?,
            RequestMutationValidationError::NotIncoming,
        ),
        (
            request_record(
                "request-1",
                RequestDirection::Incoming,
                "held",
                Some(OffsetDateTime::UNIX_EPOCH),
            )?,
            RequestMutationValidationError::NotPending,
        ),
    ] {
        // Setup and immutable initial state.
        let request_id = RequestId::from_str("request-1")?;
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader {
            script: ReaderScript::Present,
            transcript: Rc::clone(&transcript),
        };
        let api = FakeApi::new(
            DeclineScript::successful()?.with_request(Ok(request)),
            Rc::clone(&transcript),
        );

        // Complete expected final outcome and state.
        let expected = Observation {
            outcome: DeclineOutcome::Failure(DeclineFailure::Preflight(
                PreflightFailure::RequestValidation(expected_error),
            )),
            transcript: successful_calls(&request_id)?.into_iter().take(3).collect(),
        };

        // Execute once.
        let result = run_decline(&reader, &api, &request_id).await;
        let observed = Observation {
            outcome: project(result),
            transcript: transcript.borrow().clone(),
        };

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
        let request_id = RequestId::from_str("request-1")?;
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader {
            script: ReaderScript::Present,
            transcript: Rc::clone(&transcript),
        };
        let api = FakeApi::new(DeclineScript::successful()?, Rc::clone(&transcript));
        let (interactive, script, expected_outcome) = match case {
            ConfirmationCase::Required => (
                false,
                PromptScript::Answer(false),
                DeclineOutcome::Failure(DeclineFailure::ConfirmationRequired),
            ),
            ConfirmationCase::Declined => (
                true,
                PromptScript::Answer(false),
                DeclineOutcome::Failure(DeclineFailure::ConfirmationDeclined),
            ),
            ConfirmationCase::Cancelled => (
                true,
                PromptScript::Cancelled,
                DeclineOutcome::Failure(DeclineFailure::Confirmation(PromptFailure::Cancelled)),
            ),
            ConfirmationCase::Interaction => (
                true,
                PromptScript::Interaction,
                DeclineOutcome::Failure(DeclineFailure::Confirmation(PromptFailure::Interaction)),
            ),
            ConfirmationCase::Confirmed => (
                true,
                PromptScript::Answer(true),
                DeclineOutcome::Success(Box::new(DeclineResult::new(
                    DeclineRequestPlan::new(account("123")?, standard_request()?),
                    declined_request()?,
                ))),
            ),
        };
        let prompt = FakePrompt {
            interactive,
            script,
            transcript: Rc::clone(&transcript),
        };

        let mut expected_calls = successful_calls(&request_id)?
            .into_iter()
            .take(3)
            .collect::<Vec<_>>();
        expected_calls.push(Call::PromptAvailability);
        if !matches!(case, ConfirmationCase::Required) {
            expected_calls.push(Call::ConfirmDefaultNo {
                prompt: "Decline this request without sending money?".to_owned(),
            });
        }
        if matches!(case, ConfirmationCase::Confirmed) {
            expected_calls.push(
                successful_calls(&request_id)?
                    .into_iter()
                    .nth(3)
                    .ok_or_else(|| io::Error::other("missing synthetic decline call"))?,
            );
        }
        let expected = Observation {
            outcome: expected_outcome,
            transcript: expected_calls,
        };

        let result = run_decline_with_prompt(&reader, &api, &prompt, &request_id).await;
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
) -> Result<(ReaderScript, DeclineScript, DeclineFailure, usize), Box<dyn Error>> {
    let script = DeclineScript::successful()?;
    Ok(match stage {
        FailureStage::MissingCredential => (
            ReaderScript::Missing,
            script,
            DeclineFailure::Preflight(PreflightFailure::CredentialMissing),
            1,
        ),
        FailureStage::CredentialRead => (
            ReaderScript::Failure,
            script,
            DeclineFailure::Preflight(PreflightFailure::CredentialRead),
            1,
        ),
        FailureStage::CurrentAccount => (
            ReaderScript::Present,
            script.with_account(Err(FakeApiError(ApiFailureKind::Rejected))),
            DeclineFailure::Preflight(PreflightFailure::CurrentAccount(ApiFailureKind::Rejected)),
            2,
        ),
        FailureStage::AccountMismatch => (
            ReaderScript::Present,
            script.with_account(Ok(account("999")?)),
            DeclineFailure::Preflight(PreflightFailure::Account(
                FinancialValidationError::AccountMismatch,
            )),
            2,
        ),
        FailureStage::RequestLookup => (
            ReaderScript::Present,
            script.with_request(Err(FakeApiError(ApiFailureKind::Timeout))),
            DeclineFailure::Preflight(PreflightFailure::RequestLookup(ApiFailureKind::Timeout)),
            3,
        ),
        FailureStage::ContractWrite => (
            ReaderScript::Present,
            script.with_declined(Err(FakeApiError(ApiFailureKind::Contract))),
            DeclineFailure::Decline(ApiFailureKind::Contract),
            4,
        ),
        FailureStage::AmbiguousWrite => (
            ReaderScript::Present,
            script.with_declined(Err(FakeApiError(ApiFailureKind::AmbiguousWrite))),
            DeclineFailure::Decline(ApiFailureKind::AmbiguousWrite),
            4,
        ),
    })
}

fn successful_calls(request_id: &RequestId) -> Result<Vec<Call>, Box<dyn Error>> {
    Ok(vec![
        Call::ReadCredential,
        current_account_call(),
        request_lookup_call(request_id)?,
        Call::Decline {
            session: RedactedSecret::Redacted,
            plan: Box::new(DeclinePlanCall {
                account: account("123")?,
                request: standard_request()?,
            }),
        },
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

fn account(user_id: &str) -> Result<Account, Box<dyn Error>> {
    Ok(Account::new(
        UserId::from_str(user_id)?,
        Username::from_bare(if user_id == "123" { "owner" } else { "other" })?,
        Some("Synthetic owner".to_owned()),
    ))
}

fn standard_request() -> Result<RequestRecord, Box<dyn Error>> {
    request_record(
        "request-1",
        RequestDirection::Incoming,
        "pending",
        Some(OffsetDateTime::UNIX_EPOCH),
    )
}

fn request_record(
    request_id: &str,
    direction: RequestDirection,
    status: &str,
    created_at: Option<OffsetDateTime>,
) -> Result<RequestRecord, Box<dyn Error>> {
    Ok(RequestRecord::new(
        RequestId::from_str(request_id)?,
        RequestAction::Charge,
        direction,
        User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("requester")?),
            Some("Synthetic requester".to_owned()),
        ),
        Money::from_cents(1)?,
        Some("Synthetic request".to_owned()),
        created_at,
        RequestStatus::from_str(status)?,
    )
    .with_audience(Some("private".to_owned())))
}

fn declined_request() -> Result<DeclinedRequest, Box<dyn Error>> {
    Ok(DeclinedRequest::new(
        RequestId::from_str("request-1")?,
        RequestStatus::from_str("cancelled")?,
    ))
}

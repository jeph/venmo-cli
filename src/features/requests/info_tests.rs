use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error;
use std::future::{Future, ready};
use std::rc::Rc;
use std::str::FromStr;

use time::OffsetDateTime;

use super::*;
use crate::features::people::User;
use crate::features::requests::{RequestDirection, RequestLookupApi, RequestStatus};
use crate::shared::{
    AccessToken, ApiFailure, ApiFailureKind, ApplicationFailureKind, CredentialCapability,
    CredentialEnvelope, CredentialFailureKind, CredentialFormat, CredentialStoreFailure, DeviceId,
    LoadedCredential, Money, UserId, Username,
};

type TestResult = Result<(), Box<dyn Error>>;
type Transcript = Rc<RefCell<Vec<Call>>>;

const TOKEN: &str = "synthetic-request-info-token";
const DEVICE: &str = "synthetic-request-info-device";
const ACCOUNT_ID: &str = "1000";
const API_FAILURE_KINDS: [ApiFailureKind; 7] = [
    ApiFailureKind::Network,
    ApiFailureKind::Timeout,
    ApiFailureKind::Authentication,
    ApiFailureKind::Rejected,
    ApiFailureKind::Contract,
    ApiFailureKind::AmbiguousWrite,
    ApiFailureKind::Internal,
];
const CREDENTIAL_FAILURE_KINDS: [CredentialFailureKind; 8] = [
    CredentialFailureKind::Unavailable,
    CredentialFailureKind::Corrupt,
    CredentialFailureKind::Invalid,
    CredentialFailureKind::UnsupportedVersion,
    CredentialFailureKind::TooLarge,
    CredentialFailureKind::Ambiguous,
    CredentialFailureKind::Platform,
    CredentialFailureKind::Internal,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SecretArgument {
    Fixture,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SessionCall {
    token: SecretArgument,
    device: SecretArgument,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Call {
    ReadCredential,
    RequestById {
        session: SessionCall,
        current_user_id: UserId,
        request_id: RequestId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReaderOutcome {
    Present,
    Missing,
    Failure(CredentialFailureKind),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResponseId {
    Primary,
    UnexpectedSecond,
}

struct Response {
    id: ResponseId,
    result: Result<RequestRecord, FakeApiError>,
}

#[derive(Debug, Eq, PartialEq)]
enum ErrorSnapshot {
    MissingCredential,
    CredentialRead,
    Api(ApiFailureKind),
    ResponseContract(&'static str),
    NotRequest,
    NotOpen,
}

#[derive(Debug, Eq, PartialEq)]
struct FailureSnapshot {
    error: ErrorSnapshot,
    kind: ApplicationFailureKind,
}

#[derive(Debug, Eq, PartialEq)]
enum Outcome {
    Success(RequestInfoResult),
    Error(FailureSnapshot),
}

impl From<Result<RequestInfoResult, RequestInfoError>> for Outcome {
    fn from(result: Result<RequestInfoResult, RequestInfoError>) -> Self {
        match result {
            Ok(result) => Self::Success(result),
            Err(error) => {
                let kind = error.failure_kind();
                let error = match error {
                    RequestInfoError::Credential(CredentialAccessError::Missing) => {
                        ErrorSnapshot::MissingCredential
                    }
                    RequestInfoError::Credential(CredentialAccessError::Read { .. }) => {
                        ErrorSnapshot::CredentialRead
                    }
                    RequestInfoError::Api { source } => ErrorSnapshot::Api(source.kind()),
                    RequestInfoError::ResponseContract { problem } => {
                        ErrorSnapshot::ResponseContract(problem)
                    }
                    RequestInfoError::NotRequest => ErrorSnapshot::NotRequest,
                    RequestInfoError::NotOpen => ErrorSnapshot::NotOpen,
                };
                Self::Error(FailureSnapshot { error, kind })
            }
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct FakeState {
    reader: ReaderOutcome,
    remaining_responses: Vec<ResponseId>,
    calls: Vec<Call>,
}

#[derive(Debug, Eq, PartialEq)]
struct Observation {
    outcome: Outcome,
    state: FakeState,
}

struct FakeReader {
    outcome: ReaderOutcome,
    transcript: Transcript,
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("synthetic credential failure")]
struct FakeCredentialError(CredentialFailureKind);

impl CredentialStoreFailure for FakeCredentialError {
    fn kind(&self) -> CredentialFailureKind {
        self.0
    }
}

impl CredentialCapability for FakeReader {
    type Error = FakeCredentialError;
}

impl CredentialReader for FakeReader {
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        self.transcript.borrow_mut().push(Call::ReadCredential);
        match self.outcome {
            ReaderOutcome::Present => credential().map(Some),
            ReaderOutcome::Missing => Ok(None),
            ReaderOutcome::Failure(kind) => Err(FakeCredentialError(kind)),
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

struct FakeApi {
    responses: RefCell<VecDeque<Response>>,
    transcript: Transcript,
}

impl FakeApi {
    fn new(responses: Vec<Response>, transcript: Transcript) -> Self {
        Self {
            responses: RefCell::new(responses.into()),
            transcript,
        }
    }

    fn remaining(&self) -> Vec<ResponseId> {
        self.responses
            .borrow()
            .iter()
            .map(|response| response.id)
            .collect()
    }
}

impl RequestLookupApi for FakeApi {
    type Error = FakeApiError;

    fn request_by_id<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        request_id: &'a RequestId,
    ) -> impl Future<Output = Result<RequestRecord, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::RequestById {
            session: SessionCall {
                token: secret(access_token.expose_secret(), TOKEN),
                device: secret(device_id.as_str(), DEVICE),
            },
            current_user_id: current_user_id.clone(),
            request_id: request_id.clone(),
        });
        ready(
            self.responses
                .borrow_mut()
                .pop_front()
                .map_or(Err(FakeApiError(ApiFailureKind::Internal)), |response| {
                    response.result
                }),
        )
    }
}

fn secret(actual: &str, expected: &str) -> SecretArgument {
    if actual == expected {
        SecretArgument::Fixture
    } else {
        SecretArgument::Other
    }
}

fn response(id: ResponseId, result: Result<RequestRecord, FakeApiError>) -> Response {
    Response { id, result }
}

fn observe(
    result: Result<RequestInfoResult, RequestInfoError>,
    reader: &FakeReader,
    api: &FakeApi,
    transcript: &Transcript,
) -> Observation {
    Observation {
        outcome: Outcome::from(result),
        state: FakeState {
            reader: reader.outcome,
            remaining_responses: api.remaining(),
            calls: transcript.borrow().clone(),
        },
    }
}

fn expected_call(request_id: &RequestId) -> Result<Call, Box<dyn Error>> {
    Ok(Call::RequestById {
        session: SessionCall {
            token: SecretArgument::Fixture,
            device: SecretArgument::Fixture,
        },
        current_user_id: UserId::from_str(ACCOUNT_ID)?,
        request_id: request_id.clone(),
    })
}

fn failure(error: ErrorSnapshot, kind: ApplicationFailureKind) -> Outcome {
    Outcome::Error(FailureSnapshot { error, kind })
}

#[tokio::test(flavor = "current_thread")]
async fn info_accepts_pending_and_held_charge_requests_in_both_directions() -> TestResult {
    // Setup.
    let cases = [
        (RequestDirection::Incoming, "pending"),
        (RequestDirection::Incoming, "held"),
        (RequestDirection::Outgoing, "pending"),
        (RequestDirection::Outgoing, "held"),
    ];

    for (direction, status) in cases {
        let requested = RequestId::from_str(&format!("request-{direction}-{status}"))?;
        let returned = request(requested.clone(), RequestAction::Charge, direction, status)?;
        let expected_request = returned.clone();

        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader {
            outcome: ReaderOutcome::Present,
            transcript: Rc::clone(&transcript),
        };
        let api = FakeApi::new(
            vec![
                response(ResponseId::Primary, Ok(returned)),
                response(
                    ResponseId::UnexpectedSecond,
                    Err(FakeApiError(ApiFailureKind::Internal)),
                ),
            ],
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Observation {
            outcome: Outcome::Success(RequestInfoResult::new(expected_request)),
            state: FakeState {
                reader: ReaderOutcome::Present,
                remaining_responses: vec![ResponseId::UnexpectedSecond],
                calls: vec![Call::ReadCredential, expected_call(&requested)?],
            },
        };

        // Execute once.
        let result = info(&reader, &api, &requested).await;
        let observed = observe(result, &reader, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn info_rejects_pay_and_every_terminal_charge_status_as_usage() -> TestResult {
    // Setup.
    let cases = [
        (RequestAction::Pay, "pending", ErrorSnapshot::NotRequest),
        (RequestAction::Pay, "settled", ErrorSnapshot::NotRequest),
        (RequestAction::Charge, "settled", ErrorSnapshot::NotOpen),
        (RequestAction::Charge, "cancelled", ErrorSnapshot::NotOpen),
        (RequestAction::Charge, "failed", ErrorSnapshot::NotOpen),
    ];

    for (action, status, expected_error) in cases {
        let requested = RequestId::from_str(&format!("record-{status}"))?;
        let returned = request(
            requested.clone(),
            action,
            RequestDirection::Incoming,
            status,
        )?;

        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader {
            outcome: ReaderOutcome::Present,
            transcript: Rc::clone(&transcript),
        };
        let api = FakeApi::new(
            vec![response(ResponseId::Primary, Ok(returned))],
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Observation {
            outcome: failure(expected_error, ApplicationFailureKind::Usage),
            state: FakeState {
                reader: ReaderOutcome::Present,
                remaining_responses: Vec::new(),
                calls: vec![Call::ReadCredential, expected_call(&requested)?],
            },
        };

        // Execute once.
        let result = info(&reader, &api, &requested).await;
        let observed = observe(result, &reader, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn info_exact_id_contract_precedes_public_record_filtering() -> TestResult {
    // Setup.
    let requested = RequestId::from_str("request-wanted")?;
    let returned = request(
        RequestId::from_str("request-other")?,
        RequestAction::Pay,
        RequestDirection::Outgoing,
        "settled",
    )?;

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader {
        outcome: ReaderOutcome::Present,
        transcript: Rc::clone(&transcript),
    };
    let api = FakeApi::new(
        vec![response(ResponseId::Primary, Ok(returned))],
        Rc::clone(&transcript),
    );

    // Complete expected outcome and final fake state.
    let expected = Observation {
        outcome: failure(
            ErrorSnapshot::ResponseContract("the API returned a different request ID"),
            ApplicationFailureKind::ApiContract,
        ),
        state: FakeState {
            reader: ReaderOutcome::Present,
            remaining_responses: Vec::new(),
            calls: vec![Call::ReadCredential, expected_call(&requested)?],
        },
    };

    // Execute once.
    let result = info(&reader, &api, &requested).await;
    let observed = observe(result, &reader, &api, &transcript);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn info_credential_failure_matrix_stops_before_lookup() -> TestResult {
    // Setup.
    let cases = std::iter::once((ReaderOutcome::Missing, ErrorSnapshot::MissingCredential)).chain(
        CREDENTIAL_FAILURE_KINDS
            .into_iter()
            .map(|kind| (ReaderOutcome::Failure(kind), ErrorSnapshot::CredentialRead)),
    );
    let requested = RequestId::from_str("request-1")?;

    for (reader_outcome, expected_error) in cases {
        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader {
            outcome: reader_outcome,
            transcript: Rc::clone(&transcript),
        };
        let api = FakeApi::new(
            vec![response(
                ResponseId::Primary,
                Ok(request(
                    requested.clone(),
                    RequestAction::Charge,
                    RequestDirection::Incoming,
                    "pending",
                )?),
            )],
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Observation {
            outcome: failure(expected_error, ApplicationFailureKind::Credential),
            state: FakeState {
                reader: reader_outcome,
                remaining_responses: vec![ResponseId::Primary],
                calls: vec![Call::ReadCredential],
            },
        };

        // Execute once.
        let result = info(&reader, &api, &requested).await;
        let observed = observe(result, &reader, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn info_api_failure_kind_matrix_is_preserved_without_retry() -> TestResult {
    // Setup.
    let requested = RequestId::from_str("request-1")?;

    for kind in API_FAILURE_KINDS {
        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader {
            outcome: ReaderOutcome::Present,
            transcript: Rc::clone(&transcript),
        };
        let api = FakeApi::new(
            vec![
                response(ResponseId::Primary, Err(FakeApiError(kind))),
                response(
                    ResponseId::UnexpectedSecond,
                    Ok(request(
                        requested.clone(),
                        RequestAction::Charge,
                        RequestDirection::Incoming,
                        "pending",
                    )?),
                ),
            ],
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Observation {
            outcome: failure(ErrorSnapshot::Api(kind), ApplicationFailureKind::Api(kind)),
            state: FakeState {
                reader: ReaderOutcome::Present,
                remaining_responses: vec![ResponseId::UnexpectedSecond],
                calls: vec![Call::ReadCredential, expected_call(&requested)?],
            },
        };

        // Execute once.
        let result = info(&reader, &api, &requested).await;
        let observed = observe(result, &reader, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

fn credential() -> Result<LoadedCredential, FakeCredentialError> {
    let internal = || FakeCredentialError(CredentialFailureKind::Internal);
    Ok(LoadedCredential {
        envelope: CredentialEnvelope::new(
            AccessToken::from_str(TOKEN).map_err(|_| internal())?,
            DeviceId::from_str(DEVICE).map_err(|_| internal())?,
            UserId::from_str(ACCOUNT_ID).map_err(|_| internal())?,
            Username::from_bare("owner").map_err(|_| internal())?,
            Some("Synthetic owner".to_owned()),
            OffsetDateTime::UNIX_EPOCH,
        ),
        format: CredentialFormat::Version1,
    })
}

fn request(
    id: RequestId,
    action: RequestAction,
    direction: RequestDirection,
    status: &str,
) -> Result<RequestRecord, Box<dyn Error>> {
    Ok(RequestRecord::new(
        id,
        action,
        direction,
        User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("counterparty")?),
            Some("Synthetic counterparty".to_owned()),
        ),
        Money::from_str("1.25")?,
        Some("Synthetic note".to_owned()),
        Some(OffsetDateTime::UNIX_EPOCH),
        RequestStatus::from_str(status)?,
    ))
}

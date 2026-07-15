use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error;
use std::future::{Future, ready};
use std::rc::Rc;
use std::str::FromStr;

use time::OffsetDateTime;

use super::*;
use crate::features::people::{UserLookupApi, UserProfileKind};
use crate::shared::{
    AccessToken, ApiFailure, ApiFailureKind, CredentialCapability, CredentialEnvelope,
    CredentialFailureKind, CredentialFormat, CredentialStoreFailure, DeviceId, LoadedCredential,
    UserId, Username,
};

type TestResult = Result<(), Box<dyn Error>>;
type Transcript = Rc<RefCell<Vec<Call>>>;

const TOKEN: &str = "synthetic-user-info-token";
const DEVICE: &str = "synthetic-user-info-device";
const ACCOUNT_ID: &str = "1000";
const API_FAILURE_KINDS: [ApiFailureKind; 6] = [
    ApiFailureKind::Network,
    ApiFailureKind::Timeout,
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
    UserById {
        session: SessionCall,
        user_id: UserId,
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
    result: Result<User, FakeApiError>,
}

#[derive(Debug, Eq, PartialEq)]
enum ErrorSnapshot {
    MissingCredential,
    CredentialRead,
    Api(ApiFailureKind),
    ResponseContract(&'static str),
}

#[derive(Debug, Eq, PartialEq)]
enum Outcome {
    Success(UserInfoResult),
    Error(ErrorSnapshot),
}

impl From<Result<UserInfoResult, UserInfoError>> for Outcome {
    fn from(result: Result<UserInfoResult, UserInfoError>) -> Self {
        match result {
            Ok(result) => Self::Success(result),
            Err(UserInfoError::Credential(CredentialAccessError::Missing)) => {
                Self::Error(ErrorSnapshot::MissingCredential)
            }
            Err(UserInfoError::Credential(CredentialAccessError::Read { .. })) => {
                Self::Error(ErrorSnapshot::CredentialRead)
            }
            Err(UserInfoError::Api { source }) => Self::Error(ErrorSnapshot::Api(source.kind())),
            Err(UserInfoError::ResponseContract { problem }) => {
                Self::Error(ErrorSnapshot::ResponseContract(problem))
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

impl UserLookupApi for FakeApi {
    type Error = FakeApiError;

    fn user_by_id<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        user_id: &'a UserId,
    ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::UserById {
            session: SessionCall {
                token: secret(access_token.expose_secret(), TOKEN),
                device: secret(device_id.as_str(), DEVICE),
            },
            user_id: user_id.clone(),
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

fn response(id: ResponseId, result: Result<User, FakeApiError>) -> Response {
    Response { id, result }
}

fn observe(
    result: Result<UserInfoResult, UserInfoError>,
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

fn expected_call(user_id: &UserId) -> Call {
    Call::UserById {
        session: SessionCall {
            token: SecretArgument::Fixture,
            device: SecretArgument::Fixture,
        },
        user_id: user_id.clone(),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn info_returns_the_complete_exact_id_user_after_one_lookup() -> TestResult {
    // Setup.
    let requested = UserId::from_str("456")?;
    let returned = User::new(
        requested.clone(),
        Some(Username::from_bare("alice")?),
        Some("Alice Example".to_owned()),
    )
    .with_financial_attributes(UserProfileKind::Personal, true);
    let expected_user = returned.clone();

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
        outcome: Outcome::Success(UserInfoResult::new(expected_user)),
        state: FakeState {
            reader: ReaderOutcome::Present,
            remaining_responses: vec![ResponseId::UnexpectedSecond],
            calls: vec![Call::ReadCredential, expected_call(&requested)],
        },
    };

    // Execute once.
    let result = info(&reader, &api, &requested).await;
    let observed = observe(result, &reader, &api, &transcript);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn info_rejects_a_lookup_response_with_a_different_user_id() -> TestResult {
    // Setup.
    let requested = UserId::from_str("456")?;
    let returned = User::new(UserId::from_str("789")?, None, None);

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
        outcome: Outcome::Error(ErrorSnapshot::ResponseContract(
            "the API returned a different user ID",
        )),
        state: FakeState {
            reader: ReaderOutcome::Present,
            remaining_responses: Vec::new(),
            calls: vec![Call::ReadCredential, expected_call(&requested)],
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
    let requested = UserId::from_str("456")?;

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
                Ok(User::new(requested.clone(), None, None)),
            )],
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Observation {
            outcome: Outcome::Error(expected_error),
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
    let requested = UserId::from_str("456")?;

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
                    Ok(User::new(requested.clone(), None, None)),
                ),
            ],
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Observation {
            outcome: Outcome::Error(ErrorSnapshot::Api(kind)),
            state: FakeState {
                reader: ReaderOutcome::Present,
                remaining_responses: vec![ResponseId::UnexpectedSecond],
                calls: vec![Call::ReadCredential, expected_call(&requested)],
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

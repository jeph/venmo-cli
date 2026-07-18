use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error;
use std::future::{Future, ready};
use std::rc::Rc;
use std::str::FromStr;

use time::OffsetDateTime;

use super::*;
use crate::features::wallet::SignedUsdAmount;
use crate::shared::{
    AccessToken, ApiFailure, ApiFailureKind, CredentialCapability, CredentialEnvelope,
    CredentialFailureKind, CredentialFormat, CredentialStoreFailure, DeviceId, LoadedCredential,
    UserId, Username,
};

type TestResult = Result<(), Box<dyn Error>>;
type Transcript = Rc<RefCell<Vec<Call>>>;

const FIXTURE_TOKEN: &str = "synthetic-token";
const FIXTURE_DEVICE_ID: &str = "synthetic-device";
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
enum SensitiveArgument {
    Fixture,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SessionCall {
    access_token: SensitiveArgument,
    device_id: SensitiveArgument,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Call {
    ReadCredential,
    Balance { session: SessionCall },
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

struct ScriptedResponse {
    id: ResponseId,
    result: Result<Balance, FakeApiError>,
}

#[derive(Debug, Eq, PartialEq)]
enum Outcome {
    Success(BalanceResult),
    MissingCredential,
    CredentialReadFailure,
    ApiFailure(ApiFailureKind),
}

impl From<Result<BalanceResult, BalanceError>> for Outcome {
    fn from(result: Result<BalanceResult, BalanceError>) -> Self {
        match result {
            Ok(result) => Self::Success(result),
            Err(BalanceError::Credential(CredentialAccessError::Missing)) => {
                Self::MissingCredential
            }
            Err(BalanceError::Credential(CredentialAccessError::Read { .. })) => {
                Self::CredentialReadFailure
            }
            Err(BalanceError::Api { source }) => Self::ApiFailure(source.kind()),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct FakeState {
    reader: ReaderOutcome,
    remaining_responses: Vec<ResponseId>,
    transcript: Vec<Call>,
}

#[derive(Debug, Eq, PartialEq)]
struct Snapshot {
    outcome: Outcome,
    state: FakeState,
}

struct FakeCredentialReader {
    outcome: ReaderOutcome,
    transcript: Transcript,
}

impl FakeCredentialReader {
    fn new(outcome: ReaderOutcome, transcript: Transcript) -> Self {
        Self {
            outcome,
            transcript,
        }
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error, Eq, PartialEq)]
#[error("fake credential store failure")]
struct FakeCredentialError(CredentialFailureKind);

impl CredentialStoreFailure for FakeCredentialError {
    fn kind(&self) -> CredentialFailureKind {
        self.0
    }
}

impl CredentialCapability for FakeCredentialReader {
    type Error = FakeCredentialError;
}

impl CredentialReader for FakeCredentialReader {
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        self.transcript.borrow_mut().push(Call::ReadCredential);
        match self.outcome {
            ReaderOutcome::Present => loaded_credential().map(Some),
            ReaderOutcome::Missing => Ok(None),
            ReaderOutcome::Failure(kind) => Err(FakeCredentialError(kind)),
        }
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error, Eq, PartialEq)]
#[error("fake API failure")]
struct FakeApiError(ApiFailureKind);

impl ApiFailure for FakeApiError {
    fn kind(&self) -> ApiFailureKind {
        self.0
    }
}

struct FakeBalanceApi {
    responses: RefCell<VecDeque<ScriptedResponse>>,
    transcript: Transcript,
}

impl FakeBalanceApi {
    fn new(responses: Vec<ScriptedResponse>, transcript: Transcript) -> Self {
        Self {
            responses: RefCell::new(responses.into()),
            transcript,
        }
    }

    fn remaining_responses(&self) -> Vec<ResponseId> {
        self.responses
            .borrow()
            .iter()
            .map(|response| response.id)
            .collect()
    }
}

impl BalanceApi for FakeBalanceApi {
    type Error = FakeApiError;

    fn balance<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Balance, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::Balance {
            session: session_call(access_token, device_id),
        });
        let result = self
            .responses
            .borrow_mut()
            .pop_front()
            .map_or(Err(FakeApiError(ApiFailureKind::Internal)), |response| {
                response.result
            });
        ready(result)
    }
}

fn session_call(access_token: &AccessToken, device_id: &DeviceId) -> SessionCall {
    SessionCall {
        access_token: if access_token.expose_secret() == FIXTURE_TOKEN {
            SensitiveArgument::Fixture
        } else {
            SensitiveArgument::Other
        },
        device_id: if device_id.as_str() == FIXTURE_DEVICE_ID {
            SensitiveArgument::Fixture
        } else {
            SensitiveArgument::Other
        },
    }
}

const fn fixture_session() -> SessionCall {
    SessionCall {
        access_token: SensitiveArgument::Fixture,
        device_id: SensitiveArgument::Fixture,
    }
}

fn scripted(id: ResponseId, result: Result<Balance, FakeApiError>) -> ScriptedResponse {
    ScriptedResponse { id, result }
}

fn snapshot(
    result: Result<BalanceResult, BalanceError>,
    reader: &FakeCredentialReader,
    api: &FakeBalanceApi,
    transcript: &Transcript,
) -> Snapshot {
    Snapshot {
        outcome: Outcome::from(result),
        state: FakeState {
            reader: reader.outcome,
            remaining_responses: api.remaining_responses(),
            transcript: transcript.borrow().clone(),
        },
    }
}

#[tokio::test(flavor = "current_thread")]
async fn get_returns_the_complete_typed_balance() -> TestResult {
    // Setup.
    let returned_balance = balance(12_345, -67);
    let expected_balance = returned_balance.clone();

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeCredentialReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
    let api = FakeBalanceApi::new(
        vec![
            scripted(ResponseId::Primary, Ok(returned_balance)),
            scripted(
                ResponseId::UnexpectedSecond,
                Err(FakeApiError(ApiFailureKind::Internal)),
            ),
        ],
        Rc::clone(&transcript),
    );

    // Complete expected outcome and final fake state.
    let expected = Snapshot {
        outcome: Outcome::Success(BalanceResult::new(expected_balance)),
        state: FakeState {
            reader: ReaderOutcome::Present,
            remaining_responses: vec![ResponseId::UnexpectedSecond],
            transcript: vec![
                Call::ReadCredential,
                Call::Balance {
                    session: fixture_session(),
                },
            ],
        },
    };

    // Execute once.
    let result = get(&reader, &api).await;
    let observed = snapshot(result, &reader, &api, &transcript);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn credential_load_matrix_stops_before_balance_api() -> TestResult {
    // Setup.
    let cases = std::iter::once((ReaderOutcome::Missing, Outcome::MissingCredential)).chain(
        CREDENTIAL_FAILURE_KINDS
            .into_iter()
            .map(|kind| (ReaderOutcome::Failure(kind), Outcome::CredentialReadFailure)),
    );

    for (reader_outcome, expected_outcome) in cases {
        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeCredentialReader::new(reader_outcome, Rc::clone(&transcript));
        let api = FakeBalanceApi::new(
            vec![scripted(ResponseId::Primary, Ok(balance(0, 0)))],
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Snapshot {
            outcome: expected_outcome,
            state: FakeState {
                reader: reader_outcome,
                remaining_responses: vec![ResponseId::Primary],
                transcript: vec![Call::ReadCredential],
            },
        };

        // Execute once.
        let result = get(&reader, &api).await;
        let observed = snapshot(result, &reader, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn api_failure_kind_matrix_is_preserved() -> TestResult {
    // Setup.
    for kind in API_FAILURE_KINDS {
        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeCredentialReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
        let api = FakeBalanceApi::new(
            vec![scripted(ResponseId::Primary, Err(FakeApiError(kind)))],
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Snapshot {
            outcome: Outcome::ApiFailure(kind),
            state: FakeState {
                reader: ReaderOutcome::Present,
                remaining_responses: Vec::new(),
                transcript: vec![
                    Call::ReadCredential,
                    Call::Balance {
                        session: fixture_session(),
                    },
                ],
            },
        };

        // Execute once.
        let result = get(&reader, &api).await;
        let observed = snapshot(result, &reader, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

fn loaded_credential() -> Result<LoadedCredential, FakeCredentialError> {
    let internal = || FakeCredentialError(CredentialFailureKind::Internal);
    Ok(LoadedCredential {
        envelope: CredentialEnvelope::new(
            AccessToken::from_str(FIXTURE_TOKEN).map_err(|_| internal())?,
            DeviceId::from_str(FIXTURE_DEVICE_ID).map_err(|_| internal())?,
            UserId::from_str("1000").map_err(|_| internal())?,
            Username::from_bare("tester".to_owned()).map_err(|_| internal())?,
            Some("Test User".to_owned()),
            OffsetDateTime::UNIX_EPOCH,
        ),
        format: CredentialFormat::Version1,
    })
}

fn balance(available_cents: i64, on_hold_cents: i64) -> Balance {
    Balance::new(
        SignedUsdAmount::from_cents(available_cents),
        SignedUsdAmount::from_cents(on_hold_cents),
    )
}

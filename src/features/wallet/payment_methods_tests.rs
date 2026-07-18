use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error;
use std::future::{Future, ready};
use std::rc::Rc;
use std::str::FromStr;

use time::OffsetDateTime;

use super::*;
use crate::features::wallet::PaymentMethodId;
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
    PaymentMethods { session: SessionCall },
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
    result: Result<Vec<PaymentMethod>, FakeApiError>,
}

#[derive(Debug, Eq, PartialEq)]
enum Outcome {
    Success(PaymentMethodsResult),
    MissingCredential,
    CredentialReadFailure,
    ApiFailure(ApiFailureKind),
}

impl From<Result<PaymentMethodsResult, PaymentMethodsError>> for Outcome {
    fn from(result: Result<PaymentMethodsResult, PaymentMethodsError>) -> Self {
        match result {
            Ok(result) => Self::Success(result),
            Err(PaymentMethodsError::Credential(CredentialAccessError::Missing)) => {
                Self::MissingCredential
            }
            Err(PaymentMethodsError::Credential(CredentialAccessError::Read { .. })) => {
                Self::CredentialReadFailure
            }
            Err(PaymentMethodsError::Api { source }) => Self::ApiFailure(source.kind()),
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

struct FakeReader {
    outcome: ReaderOutcome,
    transcript: Transcript,
}

impl FakeReader {
    fn new(outcome: ReaderOutcome, transcript: Transcript) -> Self {
        Self {
            outcome,
            transcript,
        }
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error, Eq, PartialEq)]
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
            ReaderOutcome::Present => loaded_credential().map(Some),
            ReaderOutcome::Missing => Ok(None),
            ReaderOutcome::Failure(kind) => Err(FakeCredentialError(kind)),
        }
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error, Eq, PartialEq)]
#[error("synthetic API failure")]
struct FakeApiError(ApiFailureKind);

impl ApiFailure for FakeApiError {
    fn kind(&self) -> ApiFailureKind {
        self.0
    }
}

struct FakeApi {
    responses: RefCell<VecDeque<ScriptedResponse>>,
    transcript: Transcript,
}

impl FakeApi {
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

impl PaymentMethodsApi for FakeApi {
    type Error = FakeApiError;

    fn payment_methods<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Vec<PaymentMethod>, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::PaymentMethods {
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

fn scripted(id: ResponseId, result: Result<Vec<PaymentMethod>, FakeApiError>) -> ScriptedResponse {
    ScriptedResponse { id, result }
}

fn snapshot(
    result: Result<PaymentMethodsResult, PaymentMethodsError>,
    reader: &FakeReader,
    api: &FakeApi,
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
async fn list_returns_the_complete_typed_method_collection() -> TestResult {
    // Setup.
    let returned_methods = vec![payment_method("method-1")?, payment_method("method-2")?];
    let expected_methods = returned_methods.clone();

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
    let api = FakeApi::new(
        vec![
            scripted(ResponseId::Primary, Ok(returned_methods)),
            scripted(
                ResponseId::UnexpectedSecond,
                Err(FakeApiError(ApiFailureKind::Internal)),
            ),
        ],
        Rc::clone(&transcript),
    );

    // Complete expected outcome and final fake state.
    let expected = Snapshot {
        outcome: Outcome::Success(PaymentMethodsResult::from_methods(expected_methods)),
        state: FakeState {
            reader: ReaderOutcome::Present,
            remaining_responses: vec![ResponseId::UnexpectedSecond],
            transcript: vec![
                Call::ReadCredential,
                Call::PaymentMethods {
                    session: fixture_session(),
                },
            ],
        },
    };

    // Execute once.
    let result = list(&reader, &api).await;
    let observed = snapshot(result, &reader, &api, &transcript);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn credential_load_matrix_stops_before_payment_methods_api() -> TestResult {
    // Setup.
    let cases = std::iter::once((ReaderOutcome::Missing, Outcome::MissingCredential)).chain(
        CREDENTIAL_FAILURE_KINDS
            .into_iter()
            .map(|kind| (ReaderOutcome::Failure(kind), Outcome::CredentialReadFailure)),
    );

    for (reader_outcome, expected_outcome) in cases {
        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader::new(reader_outcome, Rc::clone(&transcript));
        let api = FakeApi::new(
            vec![scripted(ResponseId::Primary, Ok(Vec::new()))],
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
        let result = list(&reader, &api).await;
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
        let reader = FakeReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
        let api = FakeApi::new(
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
                    Call::PaymentMethods {
                        session: fixture_session(),
                    },
                ],
            },
        };

        // Execute once.
        let result = list(&reader, &api).await;
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
            UserId::from_str("123").map_err(|_| internal())?,
            Username::from_bare("alice".to_owned()).map_err(|_| internal())?,
            Some("Alice".to_owned()),
            OffsetDateTime::UNIX_EPOCH,
        ),
        format: CredentialFormat::Version1,
    })
}

fn payment_method(id: &str) -> Result<PaymentMethod, Box<dyn Error>> {
    Ok(PaymentMethod::new(
        PaymentMethodId::from_str(id)?,
        Some("Synthetic method".to_owned()),
        Some("bank".to_owned()),
        Some("1234".to_owned()),
        true,
    ))
}

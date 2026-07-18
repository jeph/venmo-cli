use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error;
use std::future::{Future, ready};
use std::rc::Rc;
use std::str::FromStr;

use time::OffsetDateTime;

use super::*;
use crate::shared::{
    AccessToken, ApiFailure, CredentialCapability, CredentialFailureKind, CredentialFormat,
    CredentialStoreFailure, DeviceId, LoadedCredential, Username,
};

type TestResult = Result<(), Box<dyn Error>>;
type Transcript = Rc<RefCell<Vec<Call>>>;

const FIXTURE_TOKEN: &str = "synthetic-token";
const FIXTURE_DEVICE_ID: &str = "synthetic-device";
const FIXTURE_USER_ID: &str = "123";
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
    SearchUsers {
        session: SessionCall,
        query: UserSearchQuery,
        page: UserSearchPageRequest,
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
    First,
    Second,
    Third,
    Fourth,
    UnexpectedNext,
}

struct ScriptedResponse {
    id: ResponseId,
    result: Result<UserSearchPage, FakeApiError>,
}

#[derive(Debug, Eq, PartialEq)]
enum PublicOutcome {
    Success(UserSearchResult),
    MissingCredential,
    CredentialReadFailure,
    ApiFailure(ApiFailureKind),
    ResponseContract(&'static str),
    Internal(&'static str),
}

impl From<Result<UserSearchResult, UserSearchError>> for PublicOutcome {
    fn from(result: Result<UserSearchResult, UserSearchError>) -> Self {
        match result {
            Ok(result) => Self::Success(result),
            Err(UserSearchError::Credential(CredentialAccessError::Missing)) => {
                Self::MissingCredential
            }
            Err(UserSearchError::Credential(CredentialAccessError::Read { .. })) => {
                Self::CredentialReadFailure
            }
            Err(UserSearchError::Api { source }) => Self::ApiFailure(source.kind()),
            Err(UserSearchError::ResponseContract { problem }) => Self::ResponseContract(problem),
            Err(UserSearchError::Internal { problem }) => Self::Internal(problem),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
enum ExhaustiveOutcome {
    Success(ExhaustiveUserSearchResult),
    ApiFailure(ApiFailureKind),
    ResponseContract(&'static str),
    Internal(&'static str),
}

impl From<Result<ExhaustiveUserSearchResult, UserSearchError>> for ExhaustiveOutcome {
    fn from(result: Result<ExhaustiveUserSearchResult, UserSearchError>) -> Self {
        match result {
            Ok(result) => Self::Success(result),
            Err(UserSearchError::Api { source }) => Self::ApiFailure(source.kind()),
            Err(UserSearchError::ResponseContract { problem }) => Self::ResponseContract(problem),
            Err(UserSearchError::Internal { problem }) => Self::Internal(problem),
            Err(UserSearchError::Credential(_)) => {
                Self::Internal("an exhaustive search unexpectedly loaded a credential")
            }
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct FakeState {
    reader: Option<ReaderOutcome>,
    remaining_responses: Vec<ResponseId>,
    transcript: Vec<Call>,
}

#[derive(Debug, Eq, PartialEq)]
struct Snapshot<O> {
    outcome: O,
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
            ReaderOutcome::Present => credential().map(Some),
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

impl UserSearchApi for FakeApi {
    type Error = FakeApiError;

    fn search_users<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        query: &'a UserSearchQuery,
        page: UserSearchPageRequest,
    ) -> impl Future<Output = Result<UserSearchPage, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::SearchUsers {
            session: session_call(access_token, device_id),
            query: query.clone(),
            page,
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

fn scripted(id: ResponseId, result: Result<UserSearchPage, FakeApiError>) -> ScriptedResponse {
    ScriptedResponse { id, result }
}

fn fake_state(reader: Option<ReaderOutcome>, api: &FakeApi, transcript: &Transcript) -> FakeState {
    FakeState {
        reader,
        remaining_responses: api.remaining_responses(),
        transcript: transcript.borrow().clone(),
    }
}

fn public_snapshot(
    result: Result<UserSearchResult, UserSearchError>,
    reader: &FakeReader,
    api: &FakeApi,
    transcript: &Transcript,
) -> Snapshot<PublicOutcome> {
    Snapshot {
        outcome: PublicOutcome::from(result),
        state: fake_state(Some(reader.outcome), api, transcript),
    }
}

fn exhaustive_snapshot(
    result: Result<ExhaustiveUserSearchResult, UserSearchError>,
    api: &FakeApi,
    transcript: &Transcript,
) -> Snapshot<ExhaustiveOutcome> {
    Snapshot {
        outcome: ExhaustiveOutcome::from(result),
        state: fake_state(None, api, transcript),
    }
}

fn expected_call(query: &UserSearchQuery, page_size: u32, offset: u32) -> TestResultCall {
    Ok(Call::SearchUsers {
        session: fixture_session(),
        query: query.clone(),
        page: UserSearchPageRequest::new(Limit::try_from(page_size)?, Offset::new(offset)),
    })
}

type TestResultCall = Result<Call, Box<dyn Error>>;

#[tokio::test(flavor = "current_thread")]
async fn public_search_fetches_exactly_one_native_page() -> TestResult {
    // Setup.
    let returned_users = users(51, 2)?;
    let expected_users = returned_users.clone();
    let query = UserSearchQuery::from_str("alice")?;
    let limit = Limit::try_from(2)?;
    let offset = Offset::new(50);

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
    let api = FakeApi::new(
        vec![
            scripted(
                ResponseId::First,
                Ok(UserSearchPage::new(returned_users, Some(Offset::new(52)))),
            ),
            scripted(
                ResponseId::UnexpectedNext,
                Err(FakeApiError(ApiFailureKind::Internal)),
            ),
        ],
        Rc::clone(&transcript),
    );

    // Complete expected outcome and final fake state.
    let expected = Snapshot {
        outcome: PublicOutcome::Success(UserSearchResult::new(
            expected_users,
            Some(Offset::new(52)),
        )),
        state: FakeState {
            reader: Some(ReaderOutcome::Present),
            remaining_responses: vec![ResponseId::UnexpectedNext],
            transcript: vec![
                Call::ReadCredential,
                Call::SearchUsers {
                    session: fixture_session(),
                    query: query.clone(),
                    page: UserSearchPageRequest::new(limit, offset),
                },
            ],
        },
    };

    // Execute once.
    let result = search(&reader, &api, &query, limit, offset).await;
    let observed = public_snapshot(result, &reader, &api, &transcript);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn public_empty_page_without_continuation_is_terminal() -> TestResult {
    // Setup.
    let query = UserSearchQuery::from_str("nobody")?;
    let limit = Limit::try_from(7)?;
    let offset = Offset::new(14);

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
    let api = FakeApi::new(
        vec![scripted(
            ResponseId::First,
            Ok(UserSearchPage::new(Vec::new(), None)),
        )],
        Rc::clone(&transcript),
    );

    // Complete expected outcome and final fake state.
    let expected = Snapshot {
        outcome: PublicOutcome::Success(UserSearchResult::new(Vec::new(), None)),
        state: FakeState {
            reader: Some(ReaderOutcome::Present),
            remaining_responses: Vec::new(),
            transcript: vec![
                Call::ReadCredential,
                Call::SearchUsers {
                    session: fixture_session(),
                    query: query.clone(),
                    page: UserSearchPageRequest::new(limit, offset),
                },
            ],
        },
    };

    // Execute once.
    let result = search(&reader, &api, &query, limit, offset).await;
    let observed = public_snapshot(result, &reader, &api, &transcript);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn public_page_contract_matrix_fails_closed() -> TestResult {
    // Setup.
    let cases = [
        (
            UserSearchPage::new(vec![user(1, "First")?, user(1, "Changed")?], None),
            2,
            "the API returned conflicting records for one user ID",
        ),
        (
            UserSearchPage::new(users(1, 2)?, None),
            1,
            "the API returned more users than requested",
        ),
        (
            UserSearchPage::new(Vec::new(), Some(Offset::new(11))),
            1,
            "the API returned an empty page with a continuation offset",
        ),
        (
            UserSearchPage::new(vec![user(1, "User 1")?], Some(Offset::new(10))),
            1,
            "the API returned a repeated or non-progressing continuation offset",
        ),
    ];

    for (page, page_size, problem) in cases {
        let query = UserSearchQuery::from_str("alice")?;
        let limit = Limit::try_from(page_size)?;
        let offset = Offset::new(10);

        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
        let api = FakeApi::new(
            vec![scripted(ResponseId::First, Ok(page))],
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Snapshot {
            outcome: PublicOutcome::ResponseContract(problem),
            state: FakeState {
                reader: Some(ReaderOutcome::Present),
                remaining_responses: Vec::new(),
                transcript: vec![
                    Call::ReadCredential,
                    Call::SearchUsers {
                        session: fixture_session(),
                        query: query.clone(),
                        page: UserSearchPageRequest::new(limit, offset),
                    },
                ],
            },
        };

        // Execute once.
        let result = search(&reader, &api, &query, limit, offset).await;
        let observed = public_snapshot(result, &reader, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn public_credential_load_matrix_stops_before_the_api() -> TestResult {
    // Setup.
    let cases = std::iter::once((ReaderOutcome::Missing, PublicOutcome::MissingCredential)).chain(
        CREDENTIAL_FAILURE_KINDS.into_iter().map(|kind| {
            (
                ReaderOutcome::Failure(kind),
                PublicOutcome::CredentialReadFailure,
            )
        }),
    );

    for (reader_outcome, expected_outcome) in cases {
        let query = UserSearchQuery::from_str("alice")?;
        let limit = Limit::try_from(10)?;
        let offset = Offset::default();

        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader::new(reader_outcome, Rc::clone(&transcript));
        let api = FakeApi::new(
            vec![scripted(
                ResponseId::First,
                Ok(UserSearchPage::new(Vec::new(), None)),
            )],
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Snapshot {
            outcome: expected_outcome,
            state: FakeState {
                reader: Some(reader_outcome),
                remaining_responses: vec![ResponseId::First],
                transcript: vec![Call::ReadCredential],
            },
        };

        // Execute once.
        let result = search(&reader, &api, &query, limit, offset).await;
        let observed = public_snapshot(result, &reader, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn public_api_failure_kind_matrix_is_preserved() -> TestResult {
    // Setup.
    let query = UserSearchQuery::from_str("alice")?;
    let limit = Limit::try_from(10)?;
    let offset = Offset::new(30);

    for kind in API_FAILURE_KINDS {
        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
        let api = FakeApi::new(
            vec![scripted(ResponseId::First, Err(FakeApiError(kind)))],
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Snapshot {
            outcome: PublicOutcome::ApiFailure(kind),
            state: FakeState {
                reader: Some(ReaderOutcome::Present),
                remaining_responses: Vec::new(),
                transcript: vec![
                    Call::ReadCredential,
                    Call::SearchUsers {
                        session: fixture_session(),
                        query: query.clone(),
                        page: UserSearchPageRequest::new(limit, offset),
                    },
                ],
            },
        };

        // Execute once.
        let result = search(&reader, &api, &query, limit, offset).await;
        let observed = public_snapshot(result, &reader, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn exhaustive_search_uses_exact_native_pages_until_exhausted() -> TestResult {
    // Setup.
    let first_page = users(1, 50)?;
    let second_page = users(51, 3)?;
    let mut expected_users = first_page.clone();
    expected_users.extend(second_page.clone());
    let query = UserSearchQuery::from_str("@alice")?;
    let loaded = credential()?;

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let api = FakeApi::new(
        vec![
            scripted(
                ResponseId::First,
                Ok(UserSearchPage::new(first_page, Some(Offset::new(50)))),
            ),
            scripted(
                ResponseId::Second,
                Ok(UserSearchPage::new(second_page, None)),
            ),
        ],
        Rc::clone(&transcript),
    );

    // Complete expected outcome and final fake state.
    let expected = Snapshot {
        outcome: ExhaustiveOutcome::Success(ExhaustiveUserSearchResult {
            users: expected_users,
        }),
        state: FakeState {
            reader: None,
            remaining_responses: Vec::new(),
            transcript: vec![
                expected_call(&query, 50, 0)?,
                expected_call(&query, 50, 50)?,
            ],
        },
    };

    // Execute once.
    let result = search_exhaustively_with_credential(&loaded.envelope, &api, &query).await;
    let observed = exhaustive_snapshot(result, &api, &transcript);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn exhaustive_search_stops_at_four_pages_and_two_hundred_records() -> TestResult {
    // Setup.
    let mut responses = Vec::new();
    let mut expected_users = Vec::new();
    for (page_index, response_id) in [
        ResponseId::First,
        ResponseId::Second,
        ResponseId::Third,
        ResponseId::Fourth,
    ]
    .into_iter()
    .enumerate()
    {
        let page_index = u32::try_from(page_index)?;
        let page_users = users(page_index * 50 + 1, 50)?;
        expected_users.extend(page_users.clone());
        responses.push(scripted(
            response_id,
            Ok(UserSearchPage::new(
                page_users,
                Some(Offset::new((page_index + 1) * 50)),
            )),
        ));
    }
    let query = UserSearchQuery::from_str("alice")?;
    let loaded = credential()?;

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let api = FakeApi::new(responses, Rc::clone(&transcript));

    // Complete expected outcome and final fake state.
    let expected = Snapshot {
        outcome: ExhaustiveOutcome::Success(ExhaustiveUserSearchResult {
            users: expected_users,
        }),
        state: FakeState {
            reader: None,
            remaining_responses: Vec::new(),
            transcript: vec![
                expected_call(&query, 50, 0)?,
                expected_call(&query, 50, 50)?,
                expected_call(&query, 50, 100)?,
                expected_call(&query, 50, 150)?,
            ],
        },
    };

    // Execute once.
    let result = search_exhaustively_with_credential(&loaded.envelope, &api, &query).await;
    let observed = exhaustive_snapshot(result, &api, &transcript);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn exhaustive_empty_page_continuation_contract_is_explicit() -> TestResult {
    // Setup.
    let cases = [
        (
            UserSearchPage::new(Vec::new(), None),
            ExhaustiveOutcome::Success(ExhaustiveUserSearchResult { users: Vec::new() }),
        ),
        (
            UserSearchPage::new(Vec::new(), Some(Offset::new(50))),
            ExhaustiveOutcome::ResponseContract(
                "the API returned an empty page with a continuation offset",
            ),
        ),
    ];

    for (page, expected_outcome) in cases {
        let query = UserSearchQuery::from_str("nobody")?;
        let loaded = credential()?;

        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let api = FakeApi::new(
            vec![scripted(ResponseId::First, Ok(page))],
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Snapshot {
            outcome: expected_outcome,
            state: FakeState {
                reader: None,
                remaining_responses: Vec::new(),
                transcript: vec![expected_call(&query, 50, 0)?],
            },
        };

        // Execute once.
        let result = search_exhaustively_with_credential(&loaded.envelope, &api, &query).await;
        let observed = exhaustive_snapshot(result, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn exhaustive_continuation_must_add_a_new_user() -> TestResult {
    // Setup.
    let repeated = user(1, "User 1")?;
    let query = UserSearchQuery::from_str("alice")?;
    let loaded = credential()?;

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let api = FakeApi::new(
        vec![
            scripted(
                ResponseId::First,
                Ok(UserSearchPage::new(
                    vec![repeated.clone()],
                    Some(Offset::new(50)),
                )),
            ),
            scripted(
                ResponseId::Second,
                Ok(UserSearchPage::new(vec![repeated], Some(Offset::new(100)))),
            ),
        ],
        Rc::clone(&transcript),
    );

    // Complete expected outcome and final fake state.
    let expected = Snapshot {
        outcome: ExhaustiveOutcome::ResponseContract(
            "the API returned a continuation page with no new users",
        ),
        state: FakeState {
            reader: None,
            remaining_responses: Vec::new(),
            transcript: vec![
                expected_call(&query, 50, 0)?,
                expected_call(&query, 50, 50)?,
            ],
        },
    };

    // Execute once.
    let result = search_exhaustively_with_credential(&loaded.envelope, &api, &query).await;
    let observed = exhaustive_snapshot(result, &api, &transcript);

    assert_eq!(observed, expected);
    Ok(())
}

fn credential() -> Result<LoadedCredential, FakeCredentialError> {
    let internal = || FakeCredentialError(CredentialFailureKind::Internal);
    Ok(LoadedCredential {
        envelope: CredentialEnvelope::new(
            AccessToken::from_str(FIXTURE_TOKEN).map_err(|_| internal())?,
            DeviceId::from_str(FIXTURE_DEVICE_ID).map_err(|_| internal())?,
            UserId::from_str(FIXTURE_USER_ID).map_err(|_| internal())?,
            Username::from_bare("alice").map_err(|_| internal())?,
            Some("Alice".to_owned()),
            OffsetDateTime::UNIX_EPOCH,
        ),
        format: CredentialFormat::Version1,
    })
}

fn users(first_id: u32, count: u32) -> Result<Vec<User>, Box<dyn Error>> {
    (first_id..first_id + count)
        .map(|id| user(id, &format!("User {id}")))
        .collect()
}

fn user(id: u32, name: &str) -> Result<User, Box<dyn Error>> {
    Ok(User::new(
        UserId::from_str(&id.to_string())?,
        Some(Username::from_bare(format!("user{id}"))?),
        Some(name.to_owned()),
    ))
}

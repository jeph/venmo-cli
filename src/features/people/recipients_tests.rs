use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error;
use std::future::{Future, ready};
use std::rc::Rc;
use std::str::FromStr;

use time::OffsetDateTime;

use super::*;
use crate::features::people::{UserSearchPage, UserSearchPageRequest};
use crate::shared::{
    AccessToken, ApiFailure, CredentialFailureKind, CredentialFormat, CredentialStoreFailure,
    DeviceId, Limit, LoadedCredential, Offset, UserId,
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
    SearchUsers {
        session: SessionCall,
        query: UserSearchQuery,
        page: UserSearchPageRequest,
    },
    UserById {
        session: SessionCall,
        user_id: UserId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResponseId {
    First,
    Second,
    Third,
    Fourth,
    Unexpected,
}

struct SearchResponse {
    id: ResponseId,
    result: Result<UserSearchPage, FakeApiError>,
}

struct LookupResponse {
    id: ResponseId,
    result: Result<User, FakeApiError>,
}

#[derive(Debug, Eq, PartialEq)]
enum SearchErrorSnapshot {
    Credential,
    Api(ApiFailureKind),
    ResponseContract(&'static str),
    Internal(&'static str),
}

impl From<UserSearchError> for SearchErrorSnapshot {
    fn from(error: UserSearchError) -> Self {
        match error {
            UserSearchError::Credential(_) => Self::Credential,
            UserSearchError::Api { source } => Self::Api(source.kind()),
            UserSearchError::ResponseContract { problem } => Self::ResponseContract(problem),
            UserSearchError::Internal { problem } => Self::Internal(problem),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
enum Outcome {
    Success(ResolvedRecipient),
    ApiFailure(ApiFailureKind),
    LookupContract(&'static str),
    UsernameSearch {
        kind: RecipientResolutionFailureKind,
        source: SearchErrorSnapshot,
    },
    UsernameNotFound,
    AmbiguousUsername,
    Internal(&'static str),
}

impl From<Result<ResolvedRecipient, RecipientResolutionError>> for Outcome {
    fn from(result: Result<ResolvedRecipient, RecipientResolutionError>) -> Self {
        match result {
            Ok(recipient) => Self::Success(recipient),
            Err(RecipientResolutionError::Api { source }) => Self::ApiFailure(source.kind()),
            Err(RecipientResolutionError::LookupContract { problem }) => {
                Self::LookupContract(problem)
            }
            Err(RecipientResolutionError::UsernameSearch { kind, source }) => {
                Self::UsernameSearch {
                    kind,
                    source: SearchErrorSnapshot::from(source),
                }
            }
            Err(RecipientResolutionError::UsernameNotFound) => Self::UsernameNotFound,
            Err(RecipientResolutionError::AmbiguousUsername) => Self::AmbiguousUsername,
            Err(RecipientResolutionError::Internal { problem }) => Self::Internal(problem),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct FakeState {
    remaining_search_responses: Vec<ResponseId>,
    remaining_lookup_responses: Vec<ResponseId>,
    transcript: Vec<Call>,
}

#[derive(Debug, Eq, PartialEq)]
struct Snapshot {
    outcome: Outcome,
    state: FakeState,
}

#[derive(Clone, Copy, Debug, thiserror::Error, Eq, PartialEq)]
#[error("synthetic credential failure")]
struct FakeCredentialError;

impl CredentialStoreFailure for FakeCredentialError {
    fn kind(&self) -> CredentialFailureKind {
        CredentialFailureKind::Internal
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
    lookups: RefCell<VecDeque<LookupResponse>>,
    searches: RefCell<VecDeque<SearchResponse>>,
    transcript: Transcript,
}

impl FakeApi {
    fn new(
        searches: Vec<SearchResponse>,
        lookups: Vec<LookupResponse>,
        transcript: Transcript,
    ) -> Self {
        Self {
            lookups: RefCell::new(lookups.into()),
            searches: RefCell::new(searches.into()),
            transcript,
        }
    }

    fn remaining_search_responses(&self) -> Vec<ResponseId> {
        self.searches
            .borrow()
            .iter()
            .map(|response| response.id)
            .collect()
    }

    fn remaining_lookup_responses(&self) -> Vec<ResponseId> {
        self.lookups
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
            session: session_call(access_token, device_id),
            user_id: user_id.clone(),
        });
        let result = self
            .lookups
            .borrow_mut()
            .pop_front()
            .map_or(Err(FakeApiError(ApiFailureKind::Internal)), |response| {
                response.result
            });
        ready(result)
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
            .searches
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

fn search_response(id: ResponseId, result: Result<UserSearchPage, FakeApiError>) -> SearchResponse {
    SearchResponse { id, result }
}

fn lookup_response(id: ResponseId, result: Result<User, FakeApiError>) -> LookupResponse {
    LookupResponse { id, result }
}

fn snapshot(
    result: Result<ResolvedRecipient, RecipientResolutionError>,
    api: &FakeApi,
    transcript: &Transcript,
) -> Snapshot {
    Snapshot {
        outcome: Outcome::from(result),
        state: FakeState {
            remaining_search_responses: api.remaining_search_responses(),
            remaining_lookup_responses: api.remaining_lookup_responses(),
            transcript: transcript.borrow().clone(),
        },
    }
}

fn expected_search_call(query: &UserSearchQuery, offset: u32) -> Result<Call, Box<dyn Error>> {
    Ok(Call::SearchUsers {
        session: fixture_session(),
        query: query.clone(),
        page: UserSearchPageRequest::new(Limit::try_from(50)?, Offset::new(offset)),
    })
}

fn expected_lookup_call(user_id: &str) -> Result<Call, Box<dyn Error>> {
    Ok(Call::UserById {
        session: fixture_session(),
        user_id: UserId::from_str(user_id)?,
    })
}

#[tokio::test(flavor = "current_thread")]
async fn username_uses_one_case_insensitive_exact_match_then_authoritative_detail() -> TestResult {
    // Setup.
    let detail = user("123", Some("ALICE"))?;
    let expected_recipient = ResolvedRecipient::new(detail.clone());
    let input = RecipientInput::from_str("@Alice")?;
    let query = UserSearchQuery::from_str("@Alice")?;
    let loaded = credential()?;

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let api = FakeApi::new(
        vec![search_response(
            ResponseId::First,
            Ok(UserSearchPage::new(
                vec![user("123", Some("alice"))?, user("124", Some("alice2"))?],
                None,
            )),
        )],
        vec![lookup_response(ResponseId::First, Ok(detail))],
        Rc::clone(&transcript),
    );

    // Complete expected outcome and final fake state.
    let expected = Snapshot {
        outcome: Outcome::Success(expected_recipient),
        state: FakeState {
            remaining_search_responses: Vec::new(),
            remaining_lookup_responses: Vec::new(),
            transcript: vec![
                expected_search_call(&query, 0)?,
                expected_lookup_call("123")?,
            ],
        },
    };

    // Execute once.
    let result = resolve_with_credential(&loaded.envelope, &api, &input).await;
    let observed = snapshot(result, &api, &transcript);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn username_requires_matching_authoritative_detail() -> TestResult {
    // Setup.
    let cases = [
        (
            user("999", Some("alice"))?,
            "the user detail response returned a different user ID",
        ),
        (
            user("123", Some("other"))?,
            "the user detail response returned a different username",
        ),
        (
            user("123", None)?,
            "the user detail response returned a different username",
        ),
    ];

    for (detail, problem) in cases {
        let input = RecipientInput::from_str("@alice")?;
        let query = UserSearchQuery::from_str("@alice")?;
        let loaded = credential()?;

        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let api = FakeApi::new(
            vec![search_response(
                ResponseId::First,
                Ok(UserSearchPage::new(vec![user("123", Some("alice"))?], None)),
            )],
            vec![lookup_response(ResponseId::First, Ok(detail))],
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Snapshot {
            outcome: Outcome::LookupContract(problem),
            state: FakeState {
                remaining_search_responses: Vec::new(),
                remaining_lookup_responses: Vec::new(),
                transcript: vec![
                    expected_search_call(&query, 0)?,
                    expected_lookup_call("123")?,
                ],
            },
        };

        // Execute once.
        let result = resolve_with_credential(&loaded.envelope, &api, &input).await;
        let observed = snapshot(result, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn exhausted_username_search_distinguishes_missing_and_ambiguous() -> TestResult {
    // Setup.
    let cases = [
        (vec![user("123", Some("other"))?], Outcome::UsernameNotFound),
        (
            vec![user("123", Some("alice"))?, user("124", Some("ALICE"))?],
            Outcome::AmbiguousUsername,
        ),
    ];

    for (page_users, expected_outcome) in cases {
        let input = RecipientInput::from_str("@alice")?;
        let query = UserSearchQuery::from_str("@alice")?;
        let loaded = credential()?;

        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let api = FakeApi::new(
            vec![search_response(
                ResponseId::First,
                Ok(UserSearchPage::new(page_users, None)),
            )],
            vec![lookup_response(
                ResponseId::Unexpected,
                Err(FakeApiError(ApiFailureKind::Internal)),
            )],
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Snapshot {
            outcome: expected_outcome,
            state: FakeState {
                remaining_search_responses: Vec::new(),
                remaining_lookup_responses: vec![ResponseId::Unexpected],
                transcript: vec![expected_search_call(&query, 0)?],
            },
        };

        // Execute once.
        let result = resolve_with_credential(&loaded.envelope, &api, &input).await;
        let observed = snapshot(result, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn exact_match_at_the_search_bound_is_verified_by_detail() -> TestResult {
    // Setup.
    let searches = bounded_search_responses(true)?;
    let detail = user("1", Some("Alice"))?;
    let expected_recipient = ResolvedRecipient::new(detail.clone());
    let input = RecipientInput::from_str("@alice")?;
    let query = UserSearchQuery::from_str("@alice")?;
    let loaded = credential()?;

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let api = FakeApi::new(
        searches,
        vec![lookup_response(ResponseId::First, Ok(detail))],
        Rc::clone(&transcript),
    );

    // Complete expected outcome and final fake state.
    let expected = Snapshot {
        outcome: Outcome::Success(expected_recipient),
        state: FakeState {
            remaining_search_responses: Vec::new(),
            remaining_lookup_responses: Vec::new(),
            transcript: vec![
                expected_search_call(&query, 0)?,
                expected_search_call(&query, 50)?,
                expected_search_call(&query, 100)?,
                expected_search_call(&query, 150)?,
                expected_lookup_call("1")?,
            ],
        },
    };

    // Execute once.
    let result = resolve_with_credential(&loaded.envelope, &api, &input).await;
    let observed = snapshot(result, &api, &transcript);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn no_exact_match_at_the_search_bound_is_not_found() -> TestResult {
    // Setup.
    let input = RecipientInput::from_str("@alice")?;
    let query = UserSearchQuery::from_str("@alice")?;
    let loaded = credential()?;

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let api = FakeApi::new(
        bounded_search_responses(false)?,
        vec![lookup_response(
            ResponseId::Unexpected,
            Err(FakeApiError(ApiFailureKind::Internal)),
        )],
        Rc::clone(&transcript),
    );

    // Complete expected outcome and final fake state.
    let expected = Snapshot {
        outcome: Outcome::UsernameNotFound,
        state: FakeState {
            remaining_search_responses: Vec::new(),
            remaining_lookup_responses: vec![ResponseId::Unexpected],
            transcript: vec![
                expected_search_call(&query, 0)?,
                expected_search_call(&query, 50)?,
                expected_search_call(&query, 100)?,
                expected_search_call(&query, 150)?,
            ],
        },
    };

    // Execute once.
    let result = resolve_with_credential(&loaded.envelope, &api, &input).await;
    let observed = snapshot(result, &api, &transcript);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn username_pagination_contract_failures_are_preserved() -> TestResult {
    // Setup.
    let repeated = user("1", Some("other"))?;
    let cases = vec![
        (
            vec![search_response(
                ResponseId::First,
                Ok(UserSearchPage::new(Vec::new(), Some(Offset::new(50)))),
            )],
            "the API returned an empty page with a continuation offset",
            vec![0],
        ),
        (
            vec![search_response(
                ResponseId::First,
                Ok(UserSearchPage::new(
                    vec![user("1", Some("other"))?],
                    Some(Offset::new(0)),
                )),
            )],
            "the API returned a repeated or non-progressing continuation offset",
            vec![0],
        ),
        (
            vec![search_response(
                ResponseId::First,
                Ok(UserSearchPage::new(other_users(1, 51)?, None)),
            )],
            "the API returned more users than requested",
            vec![0],
        ),
        (
            vec![search_response(
                ResponseId::First,
                Ok(UserSearchPage::new(
                    vec![user("1", Some("first"))?, user("1", Some("changed"))?],
                    None,
                )),
            )],
            "the API returned conflicting records for one user ID",
            vec![0],
        ),
        (
            vec![
                search_response(
                    ResponseId::First,
                    Ok(UserSearchPage::new(
                        vec![repeated.clone()],
                        Some(Offset::new(50)),
                    )),
                ),
                search_response(
                    ResponseId::Second,
                    Ok(UserSearchPage::new(vec![repeated], Some(Offset::new(100)))),
                ),
            ],
            "the API returned a continuation page with no new users",
            vec![0, 50],
        ),
    ];

    for (responses, problem, offsets) in cases {
        let input = RecipientInput::from_str("@alice")?;
        let query = UserSearchQuery::from_str("@alice")?;
        let loaded = credential()?;

        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let api = FakeApi::new(responses, Vec::new(), Rc::clone(&transcript));

        // Complete expected outcome and final fake state.
        let expected_calls = offsets
            .into_iter()
            .map(|offset| expected_search_call(&query, offset))
            .collect::<Result<Vec<_>, _>>()?;
        let expected = Snapshot {
            outcome: Outcome::UsernameSearch {
                kind: RecipientResolutionFailureKind::ApiContract,
                source: SearchErrorSnapshot::ResponseContract(problem),
            },
            state: FakeState {
                remaining_search_responses: Vec::new(),
                remaining_lookup_responses: Vec::new(),
                transcript: expected_calls,
            },
        };

        // Execute once.
        let result = resolve_with_credential(&loaded.envelope, &api, &input).await;
        let observed = snapshot(result, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn username_search_api_failure_kind_matrix_is_preserved() -> TestResult {
    // Setup.
    let input = RecipientInput::from_str("@alice")?;
    let query = UserSearchQuery::from_str("@alice")?;
    let loaded = credential()?;

    for kind in API_FAILURE_KINDS {
        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let api = FakeApi::new(
            vec![search_response(ResponseId::First, Err(FakeApiError(kind)))],
            Vec::new(),
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Snapshot {
            outcome: Outcome::UsernameSearch {
                kind: RecipientResolutionFailureKind::Api(kind),
                source: SearchErrorSnapshot::Api(kind),
            },
            state: FakeState {
                remaining_search_responses: Vec::new(),
                remaining_lookup_responses: Vec::new(),
                transcript: vec![expected_search_call(&query, 0)?],
            },
        };

        // Execute once.
        let result = resolve_with_credential(&loaded.envelope, &api, &input).await;
        let observed = snapshot(result, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn authoritative_detail_api_failure_kind_matrix_is_preserved() -> TestResult {
    // Setup.
    let input = RecipientInput::from_str("@alice")?;
    let query = UserSearchQuery::from_str("@alice")?;
    let loaded = credential()?;

    for kind in API_FAILURE_KINDS {
        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let api = FakeApi::new(
            vec![search_response(
                ResponseId::First,
                Ok(UserSearchPage::new(vec![user("123", Some("alice"))?], None)),
            )],
            vec![lookup_response(ResponseId::First, Err(FakeApiError(kind)))],
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Snapshot {
            outcome: Outcome::ApiFailure(kind),
            state: FakeState {
                remaining_search_responses: Vec::new(),
                remaining_lookup_responses: Vec::new(),
                transcript: vec![
                    expected_search_call(&query, 0)?,
                    expected_lookup_call("123")?,
                ],
            },
        };

        // Execute once.
        let result = resolve_with_credential(&loaded.envelope, &api, &input).await;
        let observed = snapshot(result, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

fn bounded_search_responses(include_match: bool) -> Result<Vec<SearchResponse>, Box<dyn Error>> {
    [
        ResponseId::First,
        ResponseId::Second,
        ResponseId::Third,
        ResponseId::Fourth,
    ]
    .into_iter()
    .enumerate()
    .map(|(page_index, response_id)| {
        let page_index = u32::try_from(page_index)?;
        let first_id = page_index * 50 + 1;
        let page_users = (first_id..first_id + 50)
            .map(|id| {
                let username = if include_match && id == 1 {
                    "alice".to_owned()
                } else {
                    format!("user{id}")
                };
                user(&id.to_string(), Some(&username))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(search_response(
            response_id,
            Ok(UserSearchPage::new(
                page_users,
                Some(Offset::new((page_index + 1) * 50)),
            )),
        ))
    })
    .collect()
}

fn other_users(first_id: u32, count: u32) -> Result<Vec<User>, Box<dyn Error>> {
    (first_id..first_id + count)
        .map(|id| {
            let username = format!("user{id}");
            user(&id.to_string(), Some(&username))
        })
        .collect()
}

fn credential() -> Result<LoadedCredential, FakeCredentialError> {
    Ok(LoadedCredential {
        envelope: CredentialEnvelope::new(
            AccessToken::from_str(FIXTURE_TOKEN).map_err(|_| FakeCredentialError)?,
            DeviceId::from_str(FIXTURE_DEVICE_ID).map_err(|_| FakeCredentialError)?,
            UserId::from_str("999").map_err(|_| FakeCredentialError)?,
            Username::from_bare("owner").map_err(|_| FakeCredentialError)?,
            Some("Owner".to_owned()),
            OffsetDateTime::UNIX_EPOCH,
        ),
        format: CredentialFormat::Version1,
    })
}

fn user(id: &str, username: Option<&str>) -> Result<User, Box<dyn Error>> {
    Ok(User::new(
        UserId::from_str(id)?,
        username.map(Username::from_bare).transpose()?,
        None,
    ))
}

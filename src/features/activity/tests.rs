use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error;
use std::future::{Future, ready};
use std::rc::Rc;
use std::str::FromStr;

use thiserror::Error;
use time::{Duration, OffsetDateTime};

use super::*;
use crate::features::people::User;
use crate::shared::{
    AccessToken, ApiFailure, ApiFailureKind, CredentialAccessError, CredentialCapability,
    CredentialEnvelope, CredentialFailureKind, CredentialFormat, CredentialReader,
    CredentialStoreFailure, DeviceId, Limit, LoadedCredential, Money, UserId, Username,
};

type TestResult = Result<(), Box<dyn Error>>;
type Transcript = Rc<RefCell<Vec<Call>>>;

const FIXTURE_TOKEN: &str = "synthetic-token";
const FIXTURE_DEVICE_ID: &str = "synthetic-device";
const FIXTURE_USER_ID: &str = "1000";
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
    ActivityList {
        session: SessionCall,
        current_user_id: UserId,
        request: ActivityPageRequest,
    },
    ActivityDetail {
        session: SessionCall,
        current_user_id: UserId,
        activity_id: ActivityId,
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
    UnexpectedOtherOperation,
}

struct ListResponse {
    id: ResponseId,
    result: Result<ActivityPage, FakeApiError>,
}

struct DetailResponse {
    id: ResponseId,
    result: Result<Activity, FakeApiError>,
}

#[derive(Debug, Eq, PartialEq)]
enum ErrorSnapshot {
    MissingCredential,
    CredentialReadFailure,
    ApiFailure(ApiFailureKind),
    ResponseContract(&'static str),
}

impl From<ActivityError> for ErrorSnapshot {
    fn from(error: ActivityError) -> Self {
        match error {
            ActivityError::Credential(CredentialAccessError::Missing) => Self::MissingCredential,
            ActivityError::Credential(CredentialAccessError::Read { .. }) => {
                Self::CredentialReadFailure
            }
            ActivityError::Api { source } => Self::ApiFailure(source.kind()),
            ActivityError::ResponseContract { problem } => Self::ResponseContract(problem),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
enum ListOutcome {
    Success(ActivityListResult),
    Error(ErrorSnapshot),
}

impl From<Result<ActivityListResult, ActivityError>> for ListOutcome {
    fn from(result: Result<ActivityListResult, ActivityError>) -> Self {
        match result {
            Ok(result) => Self::Success(result),
            Err(error) => Self::Error(ErrorSnapshot::from(error)),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
enum ShowOutcome {
    Success(Box<ActivityShowResult>),
    Error(ErrorSnapshot),
}

impl From<Result<ActivityShowResult, ActivityError>> for ShowOutcome {
    fn from(result: Result<ActivityShowResult, ActivityError>) -> Self {
        match result {
            Ok(result) => Self::Success(Box::new(result)),
            Err(error) => Self::Error(ErrorSnapshot::from(error)),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct FakeState {
    reader: ReaderOutcome,
    remaining_list_responses: Vec<ResponseId>,
    remaining_detail_responses: Vec<ResponseId>,
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

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
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

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("synthetic API failure")]
struct FakeApiError(ApiFailureKind);

impl ApiFailure for FakeApiError {
    fn kind(&self) -> ApiFailureKind {
        self.0
    }
}

struct FakeApi {
    list_responses: RefCell<VecDeque<ListResponse>>,
    detail_responses: RefCell<VecDeque<DetailResponse>>,
    transcript: Transcript,
}

impl FakeApi {
    fn new(
        list_responses: Vec<ListResponse>,
        detail_responses: Vec<DetailResponse>,
        transcript: Transcript,
    ) -> Self {
        Self {
            list_responses: RefCell::new(list_responses.into()),
            detail_responses: RefCell::new(detail_responses.into()),
            transcript,
        }
    }

    fn remaining_list_responses(&self) -> Vec<ResponseId> {
        self.list_responses
            .borrow()
            .iter()
            .map(|response| response.id)
            .collect()
    }

    fn remaining_detail_responses(&self) -> Vec<ResponseId> {
        self.detail_responses
            .borrow()
            .iter()
            .map(|response| response.id)
            .collect()
    }
}

impl ActivityListApi for FakeApi {
    type Error = FakeApiError;

    fn activity<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        request: ActivityPageRequest,
    ) -> impl Future<Output = Result<ActivityPage, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::ActivityList {
            session: session_call(access_token, device_id),
            current_user_id: current_user_id.clone(),
            request,
        });
        let result = self
            .list_responses
            .borrow_mut()
            .pop_front()
            .map_or(Err(FakeApiError(ApiFailureKind::Internal)), |response| {
                response.result
            });
        ready(result)
    }
}

impl ActivityDetailApi for FakeApi {
    type Error = FakeApiError;

    fn activity_by_id<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        activity_id: &'a ActivityId,
    ) -> impl Future<Output = Result<Activity, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::ActivityDetail {
            session: session_call(access_token, device_id),
            current_user_id: current_user_id.clone(),
            activity_id: activity_id.clone(),
        });
        let result = self
            .detail_responses
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

fn list_response(id: ResponseId, result: Result<ActivityPage, FakeApiError>) -> ListResponse {
    ListResponse { id, result }
}

fn detail_response(id: ResponseId, result: Result<Activity, FakeApiError>) -> DetailResponse {
    DetailResponse { id, result }
}

fn fake_state(reader: &FakeReader, api: &FakeApi, transcript: &Transcript) -> FakeState {
    FakeState {
        reader: reader.outcome,
        remaining_list_responses: api.remaining_list_responses(),
        remaining_detail_responses: api.remaining_detail_responses(),
        transcript: transcript.borrow().clone(),
    }
}

fn list_snapshot(
    result: Result<ActivityListResult, ActivityError>,
    reader: &FakeReader,
    api: &FakeApi,
    transcript: &Transcript,
) -> Snapshot<ListOutcome> {
    Snapshot {
        outcome: ListOutcome::from(result),
        state: fake_state(reader, api, transcript),
    }
}

fn show_snapshot(
    result: Result<ActivityShowResult, ActivityError>,
    reader: &FakeReader,
    api: &FakeApi,
    transcript: &Transcript,
) -> Snapshot<ShowOutcome> {
    Snapshot {
        outcome: ShowOutcome::from(result),
        state: fake_state(reader, api, transcript),
    }
}

fn expected_list_call(
    limit: Limit,
    before_id: Option<ActivityBeforeId>,
) -> Result<Call, Box<dyn Error>> {
    Ok(Call::ActivityList {
        session: fixture_session(),
        current_user_id: UserId::from_str(FIXTURE_USER_ID)?,
        request: ActivityPageRequest::new(limit, before_id),
    })
}

fn expected_detail_call(activity_id: &ActivityId) -> Result<Call, Box<dyn Error>> {
    Ok(Call::ActivityDetail {
        session: fixture_session(),
        current_user_id: UserId::from_str(FIXTURE_USER_ID)?,
        activity_id: activity_id.clone(),
    })
}

#[tokio::test(flavor = "current_thread")]
async fn list_fetches_one_exact_page_and_returns_native_before_id() -> TestResult {
    // Setup.
    let duplicate = activity(1, "note")?;
    let expected_activity = duplicate.clone();
    let before = ActivityBeforeId::from_str("story-current")?;
    let next = ActivityBeforeId::from_str("story-next")?;
    let limit = Limit::try_from(2)?;

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
    let api = FakeApi::new(
        vec![
            list_response(
                ResponseId::Primary,
                Ok(ActivityPage::new(
                    vec![duplicate.clone(), duplicate],
                    Some(next.clone()),
                )),
            ),
            list_response(
                ResponseId::UnexpectedSecond,
                Err(FakeApiError(ApiFailureKind::Internal)),
            ),
        ],
        Vec::new(),
        Rc::clone(&transcript),
    );

    // Complete expected outcome and final fake state.
    let expected = Snapshot {
        outcome: ListOutcome::Success(ActivityListResult::new(vec![expected_activity], Some(next))),
        state: FakeState {
            reader: ReaderOutcome::Present,
            remaining_list_responses: vec![ResponseId::UnexpectedSecond],
            remaining_detail_responses: Vec::new(),
            transcript: vec![
                Call::ReadCredential,
                expected_list_call(limit, Some(before.clone()))?,
            ],
        },
    };

    // Execute once.
    let result = list(&reader, &api, limit, Some(&before)).await;
    let observed = list_snapshot(result, &reader, &api, &transcript);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn list_empty_page_without_continuation_is_terminal() -> TestResult {
    // Setup.
    let limit = Limit::try_from(5)?;

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
    let api = FakeApi::new(
        vec![list_response(
            ResponseId::Primary,
            Ok(ActivityPage::new(Vec::new(), None)),
        )],
        Vec::new(),
        Rc::clone(&transcript),
    );

    // Complete expected outcome and final fake state.
    let expected = Snapshot {
        outcome: ListOutcome::Success(ActivityListResult::new(Vec::new(), None)),
        state: FakeState {
            reader: ReaderOutcome::Present,
            remaining_list_responses: Vec::new(),
            remaining_detail_responses: Vec::new(),
            transcript: vec![Call::ReadCredential, expected_list_call(limit, None)?],
        },
    };

    // Execute once.
    let result = list(&reader, &api, limit, None).await;
    let observed = list_snapshot(result, &reader, &api, &transcript);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn malformed_list_pages_fail_after_one_exact_request() -> TestResult {
    // Setup.
    let before = ActivityBeforeId::from_str("same")?;
    let cases = [
        (
            ActivityPage::new(vec![activity(1, "first")?, activity(1, "changed")?], None),
            2,
            "the API returned conflicting records for one activity ID",
        ),
        (
            ActivityPage::new(vec![activity(1, "one")?, activity(2, "two")?], None),
            1,
            "the API returned more activity records than requested",
        ),
        (
            ActivityPage::new(Vec::new(), Some(ActivityBeforeId::from_str("next")?)),
            1,
            "the API returned an empty page with a before-id continuation",
        ),
        (
            ActivityPage::new(
                vec![activity(1, "one")?],
                Some(ActivityBeforeId::from_str("same")?),
            ),
            1,
            "the API returned a repeated or non-progressing before-id continuation",
        ),
    ];

    for (page, page_size, problem) in cases {
        let limit = Limit::try_from(page_size)?;

        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
        let api = FakeApi::new(
            vec![list_response(ResponseId::Primary, Ok(page))],
            Vec::new(),
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Snapshot {
            outcome: ListOutcome::Error(ErrorSnapshot::ResponseContract(problem)),
            state: FakeState {
                reader: ReaderOutcome::Present,
                remaining_list_responses: Vec::new(),
                remaining_detail_responses: Vec::new(),
                transcript: vec![
                    Call::ReadCredential,
                    expected_list_call(limit, Some(before.clone()))?,
                ],
            },
        };

        // Execute once.
        let result = list(&reader, &api, limit, Some(&before)).await;
        let observed = list_snapshot(result, &reader, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn list_credential_load_matrix_stops_before_the_api() -> TestResult {
    // Setup.
    let cases = std::iter::once((ReaderOutcome::Missing, ErrorSnapshot::MissingCredential)).chain(
        CREDENTIAL_FAILURE_KINDS.into_iter().map(|kind| {
            (
                ReaderOutcome::Failure(kind),
                ErrorSnapshot::CredentialReadFailure,
            )
        }),
    );
    let limit = Limit::try_from(10)?;

    for (reader_outcome, expected_error) in cases {
        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader::new(reader_outcome, Rc::clone(&transcript));
        let api = FakeApi::new(
            vec![list_response(
                ResponseId::Primary,
                Ok(ActivityPage::new(Vec::new(), None)),
            )],
            Vec::new(),
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Snapshot {
            outcome: ListOutcome::Error(expected_error),
            state: FakeState {
                reader: reader_outcome,
                remaining_list_responses: vec![ResponseId::Primary],
                remaining_detail_responses: Vec::new(),
                transcript: vec![Call::ReadCredential],
            },
        };

        // Execute once.
        let result = list(&reader, &api, limit, None).await;
        let observed = list_snapshot(result, &reader, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn list_api_failure_kind_matrix_is_preserved() -> TestResult {
    // Setup.
    let limit = Limit::try_from(10)?;
    let before = ActivityBeforeId::from_str("before-10")?;

    for kind in API_FAILURE_KINDS {
        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
        let api = FakeApi::new(
            vec![list_response(ResponseId::Primary, Err(FakeApiError(kind)))],
            Vec::new(),
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Snapshot {
            outcome: ListOutcome::Error(ErrorSnapshot::ApiFailure(kind)),
            state: FakeState {
                reader: ReaderOutcome::Present,
                remaining_list_responses: Vec::new(),
                remaining_detail_responses: Vec::new(),
                transcript: vec![
                    Call::ReadCredential,
                    expected_list_call(limit, Some(before.clone()))?,
                ],
            },
        };

        // Execute once.
        let result = list(&reader, &api, limit, Some(&before)).await;
        let observed = list_snapshot(result, &reader, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn show_returns_only_the_exact_requested_activity() -> TestResult {
    // Setup.
    let requested = ActivityId::from_str("story-1")?;
    let returned = activity(1, "complete detail")?;
    let expected_activity = returned.clone();

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
    let api = FakeApi::new(
        vec![list_response(
            ResponseId::UnexpectedOtherOperation,
            Err(FakeApiError(ApiFailureKind::Internal)),
        )],
        vec![
            detail_response(ResponseId::Primary, Ok(returned)),
            detail_response(
                ResponseId::UnexpectedSecond,
                Err(FakeApiError(ApiFailureKind::Internal)),
            ),
        ],
        Rc::clone(&transcript),
    );

    // Complete expected outcome and final fake state.
    let expected = Snapshot {
        outcome: ShowOutcome::Success(Box::new(ActivityShowResult::new(expected_activity))),
        state: FakeState {
            reader: ReaderOutcome::Present,
            remaining_list_responses: vec![ResponseId::UnexpectedOtherOperation],
            remaining_detail_responses: vec![ResponseId::UnexpectedSecond],
            transcript: vec![Call::ReadCredential, expected_detail_call(&requested)?],
        },
    };

    // Execute once.
    let result = show(&reader, &api, &requested).await;
    let observed = show_snapshot(result, &reader, &api, &transcript);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn show_rejects_a_different_response_activity_id() -> TestResult {
    // Setup.
    let requested = ActivityId::from_str("story-1")?;

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
    let api = FakeApi::new(
        Vec::new(),
        vec![detail_response(
            ResponseId::Primary,
            Ok(activity(2, "wrong activity")?),
        )],
        Rc::clone(&transcript),
    );

    // Complete expected outcome and final fake state.
    let expected = Snapshot {
        outcome: ShowOutcome::Error(ErrorSnapshot::ResponseContract(
            "the API returned a different activity ID",
        )),
        state: FakeState {
            reader: ReaderOutcome::Present,
            remaining_list_responses: Vec::new(),
            remaining_detail_responses: Vec::new(),
            transcript: vec![Call::ReadCredential, expected_detail_call(&requested)?],
        },
    };

    // Execute once.
    let result = show(&reader, &api, &requested).await;
    let observed = show_snapshot(result, &reader, &api, &transcript);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn show_credential_load_matrix_stops_before_detail_api() -> TestResult {
    // Setup.
    let cases = std::iter::once((ReaderOutcome::Missing, ErrorSnapshot::MissingCredential)).chain(
        CREDENTIAL_FAILURE_KINDS.into_iter().map(|kind| {
            (
                ReaderOutcome::Failure(kind),
                ErrorSnapshot::CredentialReadFailure,
            )
        }),
    );
    let requested = ActivityId::from_str("story-1")?;

    for (reader_outcome, expected_error) in cases {
        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader::new(reader_outcome, Rc::clone(&transcript));
        let api = FakeApi::new(
            Vec::new(),
            vec![detail_response(
                ResponseId::Primary,
                Ok(activity(1, "unused")?),
            )],
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Snapshot {
            outcome: ShowOutcome::Error(expected_error),
            state: FakeState {
                reader: reader_outcome,
                remaining_list_responses: Vec::new(),
                remaining_detail_responses: vec![ResponseId::Primary],
                transcript: vec![Call::ReadCredential],
            },
        };

        // Execute once.
        let result = show(&reader, &api, &requested).await;
        let observed = show_snapshot(result, &reader, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn show_api_failure_kind_and_exact_id_matrix_is_preserved() -> TestResult {
    // Setup.
    let requested = ActivityId::from_str("story-detail-42")?;

    for kind in API_FAILURE_KINDS {
        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
        let api = FakeApi::new(
            Vec::new(),
            vec![detail_response(
                ResponseId::Primary,
                Err(FakeApiError(kind)),
            )],
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Snapshot {
            outcome: ShowOutcome::Error(ErrorSnapshot::ApiFailure(kind)),
            state: FakeState {
                reader: ReaderOutcome::Present,
                remaining_list_responses: Vec::new(),
                remaining_detail_responses: Vec::new(),
                transcript: vec![Call::ReadCredential, expected_detail_call(&requested)?],
            },
        };

        // Execute once.
        let result = show(&reader, &api, &requested).await;
        let observed = show_snapshot(result, &reader, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

fn credential() -> Result<LoadedCredential, FakeCredentialError> {
    let internal = || FakeCredentialError(CredentialFailureKind::Internal);
    Ok(LoadedCredential {
        envelope: CredentialEnvelope::new(
            AccessToken::from_str(FIXTURE_TOKEN).map_err(|_| internal())?,
            DeviceId::from_str(FIXTURE_DEVICE_ID).map_err(|_| internal())?,
            UserId::from_str(FIXTURE_USER_ID).map_err(|_| internal())?,
            Username::from_bare("tester").map_err(|_| internal())?,
            Some("Test User".to_owned()),
            OffsetDateTime::UNIX_EPOCH,
        ),
        format: CredentialFormat::Version1,
    })
}

fn activity(id: u32, note: &str) -> Result<Activity, Box<dyn Error>> {
    Ok(Activity::new(
        ActivityId::from_str(&format!("story-{id}"))?,
        OffsetDateTime::UNIX_EPOCH + Duration::seconds(i64::from(id)),
        ActivityAction::from_str("payment")?,
        ActivityDirection::Outgoing,
        ActivityCounterparty::user(User::new(
            UserId::from_str(&(id + 10_000).to_string())?,
            Some(Username::from_bare(format!("counterparty{id}"))?),
            None,
        )),
        Money::from_cents(u64::from(id) + 100)?,
        ActivityStatus::from_str("settled")?,
        Some(note.to_owned()),
        Some("friends".to_owned()),
    ))
}

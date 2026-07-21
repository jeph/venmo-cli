use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error;
use std::future::{Future, ready};
use std::rc::Rc;
use std::str::FromStr;

use thiserror::Error;
use time::{Duration, OffsetDateTime};

use super::*;
use crate::features::people::{
    User, UserLookupApi, UserProfileKind, UserSearchApi, UserSearchPage, UserSearchPageRequest,
    UserSearchQuery,
};
use crate::shared::{
    AccessToken, ApiFailure, ApiFailureKind, CredentialAccessError, CredentialCapability,
    CredentialEnvelope, CredentialFailureKind, CredentialFormat, CredentialReader,
    CredentialStoreFailure, DeviceId, Limit, LoadedCredential, Money, Offset, UserId, Username,
};

type TestResult = Result<(), Box<dyn Error>>;
type Transcript = Rc<RefCell<Vec<Call>>>;

const FIXTURE_TOKEN: &str = "synthetic-token";
const FIXTURE_DEVICE_ID: &str = "synthetic-device";
const FIXTURE_USER_ID: &str = "1000";
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
    ActivityList {
        session: SessionCall,
        scope: ActivityFeedScope,
        request: ActivityPageRequest,
    },
    ActivityDetail {
        session: SessionCall,
        current_user_id: UserId,
        activity_id: ActivityId,
    },
    SearchUsers {
        session: SessionCall,
        query: UserSearchQuery,
        request: UserSearchPageRequest,
    },
    UserDetail {
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
    UnexpectedOtherOperation,
}

struct ListResponse {
    id: ResponseId,
    result: Result<ActivityPage, FakeApiError>,
}

struct DetailResponse {
    id: ResponseId,
    result: Result<ActivityDetail, FakeApiError>,
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
            ActivityError::UserLookup { .. }
            | ActivityError::MissingProfileType
            | ActivityError::UnsupportedProfileType
            | ActivityError::CommentsUnavailable
            | ActivityError::CommentsIncomplete => {
                Self::ResponseContract("unexpected activity-subject failure in self-feed test")
            }
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
enum InfoOutcome {
    Success(Box<ActivityInfoResult>),
    Error(ErrorSnapshot),
}

impl From<Result<ActivityInfoResult, ActivityError>> for InfoOutcome {
    fn from(result: Result<ActivityInfoResult, ActivityError>) -> Self {
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
        scope: &'a ActivityFeedScope,
        request: ActivityPageRequest,
    ) -> impl Future<Output = Result<ActivityPage, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::ActivityList {
            session: session_call(access_token, device_id),
            scope: scope.clone(),
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
    ) -> impl Future<Output = Result<ActivityDetail, Self::Error>> + Send + 'a {
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

impl UserSearchApi for FakeApi {
    type Error = FakeApiError;

    fn search_users<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        query: &'a UserSearchQuery,
        request: UserSearchPageRequest,
    ) -> impl Future<Output = Result<UserSearchPage, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::SearchUsers {
            session: session_call(access_token, device_id),
            query: query.clone(),
            request,
        });
        ready(subject_user().map(|user| UserSearchPage::new(vec![user], None)))
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
        self.transcript.borrow_mut().push(Call::UserDetail {
            session: session_call(access_token, device_id),
            user_id: user_id.clone(),
        });
        ready(subject_user())
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

fn detail_response(id: ResponseId, result: Result<ActivityDetail, FakeApiError>) -> DetailResponse {
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

fn info_snapshot(
    result: Result<ActivityInfoResult, ActivityError>,
    reader: &FakeReader,
    api: &FakeApi,
    transcript: &Transcript,
) -> Snapshot<InfoOutcome> {
    Snapshot {
        outcome: InfoOutcome::from(result),
        state: fake_state(reader, api, transcript),
    }
}

fn expected_list_call(
    limit: Limit,
    before_id: Option<ActivityBeforeId>,
) -> Result<Call, Box<dyn Error>> {
    Ok(Call::ActivityList {
        session: fixture_session(),
        scope: ActivityFeedScope::new(
            UserId::from_str(FIXTURE_USER_ID)?,
            UserId::from_str(FIXTURE_USER_ID)?,
            ActivityFeedKind::CurrentUser,
        ),
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

fn subject_user() -> Result<User, FakeApiError> {
    Ok(User::new(
        UserId::from_str("2000").map_err(|_| FakeApiError(ApiFailureKind::Internal))?,
        Some(Username::from_bare("alice").map_err(|_| FakeApiError(ApiFailureKind::Internal))?),
        Some("Alice".to_owned()),
    )
    .with_financial_attributes(UserProfileKind::Personal, true))
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
async fn list_for_user_resolves_exact_personal_subject_before_one_feed_page() -> TestResult {
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
    let username = Username::from_bare("alice")?;

    let result = list_for_user(&reader, &api, &username, Limit::MIN, None).await?;

    let subject = result
        .subject()
        .ok_or_else(|| std::io::Error::other("missing resolved activity subject"))?;
    assert_eq!(subject.username().as_str(), "alice");
    assert_eq!(subject.user_id().as_str(), "2000");
    assert_eq!(subject.kind(), ActivityFeedKind::OtherPersonalUser);
    let calls = transcript.borrow();
    assert!(matches!(calls.as_slice(), [
        Call::ReadCredential,
        Call::SearchUsers { request, .. },
        Call::UserDetail { user_id, .. },
        Call::ActivityList { scope, request: activity_request, .. }
    ] if request == &UserSearchPageRequest::new(Limit::try_from(50)?, Offset::default())
        && user_id.as_str() == "2000"
        && scope.viewer_user_id().as_str() == "1000"
        && scope.subject_user_id().as_str() == "2000"
        && scope.kind() == ActivityFeedKind::OtherPersonalUser
        && activity_request == &ActivityPageRequest::new(Limit::MIN, None)));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn list_for_own_username_uses_credential_identity_without_lookup() -> TestResult {
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
    let username = Username::from_bare("TESTER")?;

    let result = list_for_user(&reader, &api, &username, Limit::MIN, None).await?;

    assert!(result.subject().is_none());
    assert_eq!(
        transcript.borrow().as_slice(),
        [Call::ReadCredential, expected_list_call(Limit::MIN, None)?]
    );
    Ok(())
}

#[test]
fn other_user_activity_subject_requires_authoritative_personal_profile() -> TestResult {
    let missing = User::new(
        UserId::from_str("2000")?,
        Some(Username::from_bare("alice")?),
        None,
    );
    assert!(matches!(
        super::list::subject_from_user(missing),
        Err(ActivityError::MissingProfileType)
    ));

    for kind in [
        UserProfileKind::Business,
        UserProfileKind::Charity,
        UserProfileKind::Unknown,
    ] {
        let user = User::new(
            UserId::from_str("2000")?,
            Some(Username::from_bare("alice")?),
            None,
        )
        .with_financial_attributes(kind, true);
        assert!(matches!(
            super::list::subject_from_user(user),
            Err(ActivityError::UnsupportedProfileType)
        ));
    }
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
async fn info_returns_only_the_exact_requested_activity() -> TestResult {
    // Setup.
    let requested = ActivityId::from_str("story-1")?;
    let returned = activity_detail(1, "complete detail")?;
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
        outcome: InfoOutcome::Success(Box::new(ActivityInfoResult::new(expected_activity))),
        state: FakeState {
            reader: ReaderOutcome::Present,
            remaining_list_responses: vec![ResponseId::UnexpectedOtherOperation],
            remaining_detail_responses: vec![ResponseId::UnexpectedSecond],
            transcript: vec![Call::ReadCredential, expected_detail_call(&requested)?],
        },
    };

    // Execute once.
    let result = info(&reader, &api, &requested).await;
    let observed = info_snapshot(result, &reader, &api, &transcript);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn info_rejects_a_different_response_activity_id() -> TestResult {
    // Setup.
    let requested = ActivityId::from_str("story-1")?;

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
    let api = FakeApi::new(
        Vec::new(),
        vec![detail_response(
            ResponseId::Primary,
            Ok(activity_detail(2, "wrong activity")?),
        )],
        Rc::clone(&transcript),
    );

    // Complete expected outcome and final fake state.
    let expected = Snapshot {
        outcome: InfoOutcome::Error(ErrorSnapshot::ResponseContract(
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
    let result = info(&reader, &api, &requested).await;
    let observed = info_snapshot(result, &reader, &api, &transcript);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn info_credential_load_matrix_stops_before_detail_api() -> TestResult {
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
                Ok(activity_detail(1, "unused")?),
            )],
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Snapshot {
            outcome: InfoOutcome::Error(expected_error),
            state: FakeState {
                reader: reader_outcome,
                remaining_list_responses: Vec::new(),
                remaining_detail_responses: vec![ResponseId::Primary],
                transcript: vec![Call::ReadCredential],
            },
        };

        // Execute once.
        let result = info(&reader, &api, &requested).await;
        let observed = info_snapshot(result, &reader, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn info_api_failure_kind_and_exact_id_matrix_is_preserved() -> TestResult {
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
            outcome: InfoOutcome::Error(ErrorSnapshot::ApiFailure(kind)),
            state: FakeState {
                reader: ReaderOutcome::Present,
                remaining_list_responses: Vec::new(),
                remaining_detail_responses: Vec::new(),
                transcript: vec![Call::ReadCredential, expected_detail_call(&requested)?],
            },
        };

        // Execute once.
        let result = info(&reader, &api, &requested).await;
        let observed = info_snapshot(result, &reader, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn comment_list_uses_standard_limit_and_offset_over_one_complete_detail_read() -> TestResult {
    let requested = ActivityId::from_str("story-42")?;
    let comments = (0_u32..12)
        .map(activity_comment)
        .collect::<Result<Vec<_>, _>>()?;
    let detail = activity_detail_with_comments(42, comments, true)?;
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
    let api = FakeApi::new(
        Vec::new(),
        vec![detail_response(ResponseId::Primary, Ok(detail))],
        Rc::clone(&transcript),
    );

    let result = comment_list::list(
        &reader,
        &api,
        &requested,
        Limit::try_from(10)?,
        Offset::default(),
    )
    .await?;

    assert_eq!(result.activity_id(), &requested);
    assert_eq!(result.total_count(), 12);
    assert_eq!(result.offset(), Offset::default());
    assert_eq!(result.comments().len(), 10);
    assert_eq!(result.comments()[0].id().as_str(), "comment-0");
    assert_eq!(result.comments()[9].id().as_str(), "comment-9");
    assert_eq!(result.next_offset(), Some(Offset::new(10)));
    assert_eq!(
        transcript.borrow().as_slice(),
        [Call::ReadCredential, expected_detail_call(&requested)?]
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn comment_list_preserves_source_order_and_signals_the_final_local_page() -> TestResult {
    let requested = ActivityId::from_str("story-42")?;
    let comments = (0_u32..12)
        .map(activity_comment)
        .collect::<Result<Vec<_>, _>>()?;
    let detail = activity_detail_with_comments(42, comments, true)?;
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
    let api = FakeApi::new(
        Vec::new(),
        vec![detail_response(ResponseId::Primary, Ok(detail))],
        Rc::clone(&transcript),
    );

    let result = comment_list::list(
        &reader,
        &api,
        &requested,
        Limit::try_from(10)?,
        Offset::new(10),
    )
    .await?;

    assert_eq!(result.comments().len(), 2);
    assert_eq!(result.comments()[0].id().as_str(), "comment-10");
    assert_eq!(result.comments()[1].id().as_str(), "comment-11");
    assert_eq!(result.next_offset(), None);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn comment_list_fails_closed_for_missing_or_partial_embedded_data() -> TestResult {
    for (detail, expected) in [
        (activity_detail(42, "note")?, "missing"),
        (
            activity_detail_with_comments(42, vec![activity_comment(0)?], false)?,
            "partial",
        ),
    ] {
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
        let api = FakeApi::new(
            Vec::new(),
            vec![detail_response(ResponseId::Primary, Ok(detail))],
            transcript,
        );
        let error = comment_list::list(
            &reader,
            &api,
            &ActivityId::from_str("story-42")?,
            Limit::MIN,
            Offset::default(),
        )
        .await
        .err()
        .ok_or_else(|| std::io::Error::other("comment listing unexpectedly succeeded"))?;
        assert!(matches!(
            (expected, error),
            ("missing", ActivityError::CommentsUnavailable)
                | ("partial", ActivityError::CommentsIncomplete)
        ));
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
        Some(Money::from_cents(u64::from(id) + 100)?),
        ActivityStatus::from_str("settled")?,
        Some(note.to_owned()),
        Some("friends".to_owned()),
    ))
}

fn activity_detail(id: u32, note: &str) -> Result<ActivityDetail, Box<dyn Error>> {
    Ok(ActivityDetail::relative(activity(id, note)?))
}

fn activity_detail_with_comments(
    id: u32,
    comments: Vec<ActivityComment>,
    complete: bool,
) -> Result<ActivityDetail, Box<dyn Error>> {
    let count = u64::try_from(comments.len())?;
    Ok(
        activity_detail(id, "note")?.with_social(ActivitySocial::new(
            None,
            Some(ActivitySocialCollection::new(count, comments, complete)),
        )),
    )
}

fn activity_comment(index: u32) -> Result<ActivityComment, Box<dyn Error>> {
    Ok(ActivityComment::new(
        ActivityCommentId::from_str(&format!("comment-{index}"))?,
        User::new(
            UserId::from_str("1000")?,
            Some(Username::from_bare("tester")?),
            Some("Test User".to_owned()),
        ),
        format!("comment message {index}"),
        OffsetDateTime::UNIX_EPOCH + Duration::seconds(i64::from(index)),
    ))
}

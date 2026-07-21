use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error;
use std::future::{Future, ready};
use std::io;
use std::rc::Rc;
use std::str::FromStr;

use clap::Parser;

use super::*;
use crate::adapters::cli::args::{
    Cli, Command, FriendsListArgs, FriendsOperation, RequestsOperation, UsersOperation,
};
use crate::adapters::cli::error::ErrorCategory;
use crate::adapters::cli::output::TimestampFormatter;
use crate::features::activity::{
    Activity, ActivityAction, ActivityBeforeId, ActivityCounterparty, ActivityDetail,
    ActivityDirection, ActivityFeedKind, ActivityFeedScope, ActivityId, ActivityPage,
    ActivityPageRequest, ActivityStatus,
};
use crate::features::people::{
    FriendsPage, FriendsPageRequest, FriendshipStatus, User, UserProfileKind, UserSearchPage,
    UserSearchPageRequest, UserSearchQuery,
};
use crate::features::requests::{
    PendingRequestsPage, PendingRequestsPageRequest, RequestAction, RequestDirection, RequestId,
    RequestRecord, RequestStatus, RequestsBefore,
};
use crate::features::wallet::{Balance, PaymentMethod, PaymentMethodId, SignedUsdAmount};
use crate::shared::test_support::Observed;
use crate::shared::{
    AccessToken, ApiFailure, ApiFailureKind, CredentialCapability, CredentialEnvelope,
    CredentialFailureKind, CredentialFormat, CredentialStoreFailure, DeviceId, Limit,
    LoadedCredential, Money, Offset, UserId, Username,
};

#[path = "reads_tests/activity_requests.rs"]
mod activity_requests;
#[path = "reads_tests/fixtures.rs"]
mod fixtures;
#[path = "reads_tests/output.rs"]
mod output;
#[path = "reads_tests/people.rs"]
mod people;
#[path = "reads_tests/wallet.rs"]
mod wallet;

use fixtures::*;
use output::*;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;
type Transcript = Rc<RefCell<Vec<ReadCall>>>;

const TOKEN: &str = "synthetic-read-token";
const DEVICE: &str = "synthetic-read-device";

fn timestamps() -> TimestampFormatter {
    TimestampFormatter::for_time_zone(jiff::tz::TimeZone::UTC)
}

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
enum ReadCall {
    ReadCredential,
    PaymentMethods {
        session: SessionCall,
    },
    SearchUsers {
        session: SessionCall,
        query: UserSearchQuery,
        page: UserSearchPageRequest,
    },
    UserInfo {
        session: SessionCall,
        user_id: UserId,
    },
    Friends {
        session: SessionCall,
        subject_user_id: UserId,
        page: FriendsPageRequest,
    },
    Balance {
        session: SessionCall,
    },
    ActivityList {
        session: SessionCall,
        scope: ActivityFeedScope,
        page: ActivityPageRequest,
    },
    ActivityInfo {
        session: SessionCall,
        current_user_id: UserId,
        activity_id: ActivityId,
    },
    PendingRequests {
        session: SessionCall,
        current_user_id: UserId,
        page: PendingRequestsPageRequest,
    },
    RequestInfo {
        session: SessionCall,
        current_user_id: UserId,
        request_id: RequestId,
    },
    StdoutWrite,
    StderrWrite,
    StdoutFlush,
    StderrFlush,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResponseId {
    Primary,
    UnexpectedSecond,
}

struct ScriptedResponse<T> {
    id: ResponseId,
    result: Result<T, FakeApiError>,
}

struct ResponseQueue<T> {
    responses: RefCell<VecDeque<ScriptedResponse<T>>>,
}

impl<T> ResponseQueue<T> {
    fn successful(value: T) -> Self {
        Self {
            responses: RefCell::new(
                vec![
                    ScriptedResponse {
                        id: ResponseId::Primary,
                        result: Ok(value),
                    },
                    ScriptedResponse {
                        id: ResponseId::UnexpectedSecond,
                        result: Err(FakeApiError),
                    },
                ]
                .into(),
            ),
        }
    }

    fn take(&self) -> Result<T, FakeApiError> {
        self.responses
            .borrow_mut()
            .pop_front()
            .map_or(Err(FakeApiError), |response| response.result)
    }

    fn remaining(&self) -> Vec<ResponseId> {
        self.responses
            .borrow()
            .iter()
            .map(|response| response.id)
            .collect()
    }
}

#[derive(Clone, Copy)]
enum CredentialOutcome {
    Present,
    Missing,
}

struct ScriptedCredential {
    id: ResponseId,
    outcome: CredentialOutcome,
}

struct FakeReader {
    responses: RefCell<VecDeque<ScriptedCredential>>,
    transcript: Transcript,
}

impl FakeReader {
    fn standard(transcript: Transcript) -> Self {
        Self {
            responses: RefCell::new(
                vec![
                    ScriptedCredential {
                        id: ResponseId::Primary,
                        outcome: CredentialOutcome::Present,
                    },
                    ScriptedCredential {
                        id: ResponseId::UnexpectedSecond,
                        outcome: CredentialOutcome::Missing,
                    },
                ]
                .into(),
            ),
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

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("synthetic credential failure")]
struct FakeCredentialError;

impl CredentialStoreFailure for FakeCredentialError {
    fn kind(&self) -> CredentialFailureKind {
        CredentialFailureKind::Internal
    }
}

impl CredentialCapability for FakeReader {
    type Error = FakeCredentialError;
}

impl CredentialReader for FakeReader {
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        self.transcript.borrow_mut().push(ReadCall::ReadCredential);
        match self
            .responses
            .borrow_mut()
            .pop_front()
            .map_or(CredentialOutcome::Missing, |response| response.outcome)
        {
            CredentialOutcome::Present => credential().map(Some),
            CredentialOutcome::Missing => Ok(None),
        }
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("synthetic read API failure")]
struct FakeApiError;

impl ApiFailure for FakeApiError {
    fn kind(&self) -> ApiFailureKind {
        ApiFailureKind::Internal
    }
}

struct PaymentMethodsFake {
    responses: ResponseQueue<Vec<PaymentMethod>>,
    transcript: Transcript,
}

impl PaymentMethodsApi for PaymentMethodsFake {
    type Error = FakeApiError;

    fn payment_methods<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Vec<PaymentMethod>, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(ReadCall::PaymentMethods {
            session: session_call(access_token, device_id),
        });
        ready(self.responses.take())
    }
}

struct UserSearchFake {
    responses: ResponseQueue<UserSearchPage>,
    transcript: Transcript,
}

impl UserSearchApi for UserSearchFake {
    type Error = FakeApiError;

    fn search_users<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        query: &'a UserSearchQuery,
        page: UserSearchPageRequest,
    ) -> impl Future<Output = Result<UserSearchPage, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(ReadCall::SearchUsers {
            session: session_call(access_token, device_id),
            query: query.clone(),
            page,
        });
        ready(self.responses.take())
    }
}

struct UserInfoFake {
    responses: ResponseQueue<User>,
    search_responses: Option<ResponseQueue<UserSearchPage>>,
    transcript: Transcript,
}

impl UserLookupApi for UserInfoFake {
    type Error = FakeApiError;

    fn user_by_id<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        user_id: &'a UserId,
    ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(ReadCall::UserInfo {
            session: session_call(access_token, device_id),
            user_id: user_id.clone(),
        });
        ready(self.responses.take())
    }
}

impl UserSearchApi for UserInfoFake {
    type Error = FakeApiError;

    fn search_users<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        query: &'a UserSearchQuery,
        page: UserSearchPageRequest,
    ) -> impl Future<Output = Result<UserSearchPage, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(ReadCall::SearchUsers {
            session: session_call(access_token, device_id),
            query: query.clone(),
            page,
        });
        ready(
            self.search_responses
                .as_ref()
                .map_or(Err(FakeApiError), ResponseQueue::take),
        )
    }
}

struct FriendsFake {
    responses: ResponseQueue<FriendsPage>,
    search_responses: Option<ResponseQueue<UserSearchPage>>,
    detail_responses: Option<ResponseQueue<User>>,
    transcript: Transcript,
}

impl FriendsApi for FriendsFake {
    type Error = FakeApiError;

    fn friends<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        subject_user_id: &'a UserId,
        page: FriendsPageRequest,
    ) -> impl Future<Output = Result<FriendsPage, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(ReadCall::Friends {
            session: session_call(access_token, device_id),
            subject_user_id: subject_user_id.clone(),
            page,
        });
        ready(self.responses.take())
    }
}

impl UserLookupApi for FriendsFake {
    type Error = FakeApiError;

    fn user_by_id<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        user_id: &'a UserId,
    ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(ReadCall::UserInfo {
            session: session_call(access_token, device_id),
            user_id: user_id.clone(),
        });
        ready(
            self.detail_responses
                .as_ref()
                .map_or(Err(FakeApiError), ResponseQueue::take),
        )
    }
}

impl UserSearchApi for FriendsFake {
    type Error = FakeApiError;

    fn search_users<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        query: &'a UserSearchQuery,
        page: UserSearchPageRequest,
    ) -> impl Future<Output = Result<UserSearchPage, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(ReadCall::SearchUsers {
            session: session_call(access_token, device_id),
            query: query.clone(),
            page,
        });
        ready(
            self.search_responses
                .as_ref()
                .map_or(Err(FakeApiError), ResponseQueue::take),
        )
    }
}

struct BalanceFake {
    responses: ResponseQueue<Balance>,
    transcript: Transcript,
}

impl BalanceApi for BalanceFake {
    type Error = FakeApiError;

    fn balance<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Balance, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(ReadCall::Balance {
            session: session_call(access_token, device_id),
        });
        ready(self.responses.take())
    }
}

struct ActivityFake {
    list_responses: ResponseQueue<ActivityPage>,
    info_responses: ResponseQueue<ActivityDetail>,
    transcript: Transcript,
}

impl ActivityListApi for ActivityFake {
    type Error = FakeApiError;

    fn activity<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        scope: &'a ActivityFeedScope,
        page: ActivityPageRequest,
    ) -> impl Future<Output = Result<ActivityPage, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(ReadCall::ActivityList {
            session: session_call(access_token, device_id),
            scope: scope.clone(),
            page,
        });
        ready(self.list_responses.take())
    }
}

impl ActivityDetailApi for ActivityFake {
    type Error = FakeApiError;

    fn activity_by_id<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        activity_id: &'a ActivityId,
    ) -> impl Future<Output = Result<ActivityDetail, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(ReadCall::ActivityInfo {
            session: session_call(access_token, device_id),
            current_user_id: current_user_id.clone(),
            activity_id: activity_id.clone(),
        });
        ready(self.info_responses.take())
    }
}

impl UserLookupApi for ActivityFake {
    type Error = FakeApiError;

    fn user_by_id<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        _user_id: &'a UserId,
    ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a {
        ready(Err(FakeApiError))
    }
}

impl UserSearchApi for ActivityFake {
    type Error = FakeApiError;

    fn search_users<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        _query: &'a UserSearchQuery,
        _page: UserSearchPageRequest,
    ) -> impl Future<Output = Result<UserSearchPage, Self::Error>> + Send + 'a {
        ready(Err(FakeApiError))
    }
}

struct RequestsListFake {
    responses: ResponseQueue<PendingRequestsPage>,
    transcript: Transcript,
}

impl RequestsApi for RequestsListFake {
    type Error = FakeApiError;

    fn pending_requests<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        page: PendingRequestsPageRequest,
    ) -> impl Future<Output = Result<PendingRequestsPage, Self::Error>> + Send + 'a {
        self.transcript
            .borrow_mut()
            .push(ReadCall::PendingRequests {
                session: session_call(access_token, device_id),
                current_user_id: current_user_id.clone(),
                page,
            });
        ready(self.responses.take())
    }
}

struct RequestInfoFake {
    responses: ResponseQueue<RequestRecord>,
    transcript: Transcript,
}

impl RequestLookupApi for RequestInfoFake {
    type Error = FakeApiError;

    fn request_by_id<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        request_id: &'a RequestId,
    ) -> impl Future<Output = Result<RequestRecord, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(ReadCall::RequestInfo {
            session: session_call(access_token, device_id),
            current_user_id: current_user_id.clone(),
            request_id: request_id.clone(),
        });
        ready(self.responses.take())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ErrorVariant {
    CommandOutput,
    Unexpected,
}

#[derive(Debug, Eq, PartialEq)]
enum ResultSnapshot {
    Success,
    Failure {
        variant: ErrorVariant,
        category: ErrorCategory,
        exit_code: u8,
        message: String,
    },
}

#[derive(Debug, Eq, PartialEq)]
struct ReadState<ApiState> {
    calls: Vec<ReadCall>,
    remaining_credentials: Vec<ResponseId>,
    api: ApiState,
    stdout: WriterState,
    stderr: WriterState,
}

#[derive(Debug, Eq, PartialEq)]
struct ActivityApiState {
    list: Vec<ResponseId>,
    info: Vec<ResponseId>,
}

fn observation<ApiState>(
    result: Result<(), AppError>,
    transcript: &Transcript,
    reader: &FakeReader,
    api: ApiState,
    stdout: WriterState,
    stderr: WriterState,
) -> Observed<ResultSnapshot, ReadState<ApiState>> {
    Observed::new(
        snapshot_result(result),
        ReadState {
            calls: transcript.borrow().clone(),
            remaining_credentials: reader.remaining(),
            api,
            stdout,
            stderr,
        },
    )
}

fn snapshot_result(result: Result<(), AppError>) -> ResultSnapshot {
    match result {
        Ok(()) => ResultSnapshot::Success,
        Err(error) => ResultSnapshot::Failure {
            variant: if matches!(error, AppError::CommandOutput { .. }) {
                ErrorVariant::CommandOutput
            } else {
                ErrorVariant::Unexpected
            },
            category: error.category(),
            exit_code: error.exit_code(),
            message: error.to_string(),
        },
    }
}

fn failure_snapshot() -> ResultSnapshot {
    ResultSnapshot::Failure {
        variant: ErrorVariant::CommandOutput,
        category: ErrorCategory::Internal,
        exit_code: 1,
        message: "failed to write command output".to_owned(),
    }
}

use std::cell::RefCell;
use std::error::Error;
use std::future::{Future, ready};
use std::rc::Rc;
use std::str::FromStr;

use time::{Duration, OffsetDateTime};

use super::*;
use crate::features::people::User;
use crate::features::requests::{
    PendingRequestsPage, RequestAction, RequestDirection, RequestId, RequestStatus,
};
use crate::shared::{
    AccessToken, ApiFailure, ApiFailureKind, CredentialCapability, CredentialEnvelope,
    CredentialFailureKind, CredentialFormat, CredentialStoreFailure, DeviceId, LoadedCredential,
    Money, UserId, Username,
};

type TestResult = Result<(), Box<dyn Error>>;
type Transcript = Rc<RefCell<Vec<Call>>>;

const ACCESS_TOKEN: &str = "synthetic-secret-list-token";
const DEVICE_ID: &str = "synthetic-secret-list-device";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RedactedSecret {
    Redacted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Call {
    ReadCredential,
    PendingRequests {
        session: RedactedSecret,
        current_user_id: UserId,
        page: PendingRequestsPageRequest,
    },
}

#[derive(Debug, Eq, PartialEq)]
enum ListFailure {
    CredentialMissing,
    CredentialRead,
    Api(ApiFailureKind),
    ResponseContract(&'static str),
}

#[derive(Debug, Eq, PartialEq)]
enum ListOutcome {
    Success(RequestsResult),
    Failure(ListFailure),
}

#[derive(Debug, Eq, PartialEq)]
struct Observation {
    outcome: ListOutcome,
    transcript: Vec<Call>,
}

fn project(result: Result<RequestsResult, RequestsError>) -> ListOutcome {
    match result {
        Ok(result) => ListOutcome::Success(result),
        Err(error) => ListOutcome::Failure(match error {
            RequestsError::Credential(CredentialAccessError::Missing) => {
                ListFailure::CredentialMissing
            }
            RequestsError::Credential(CredentialAccessError::Read { .. }) => {
                ListFailure::CredentialRead
            }
            RequestsError::Api { source } => ListFailure::Api(source.kind()),
            RequestsError::ResponseContract { problem } => ListFailure::ResponseContract(problem),
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

struct FakeApi {
    page: RefCell<Option<Result<PendingRequestsPage, FakeApiError>>>,
    transcript: Transcript,
}

impl FakeApi {
    fn new(page: Result<PendingRequestsPage, FakeApiError>, transcript: Transcript) -> Self {
        Self {
            page: RefCell::new(Some(page)),
            transcript,
        }
    }
}

impl RequestsApi for FakeApi {
    type Error = FakeApiError;

    fn pending_requests<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        page: PendingRequestsPageRequest,
    ) -> impl Future<Output = Result<PendingRequestsPage, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::PendingRequests {
            session: RedactedSecret::Redacted,
            current_user_id: current_user_id.clone(),
            page,
        });
        let result = match self.page.borrow_mut().take() {
            Some(result) => result,
            None => Err(FakeApiError(ApiFailureKind::Internal)),
        };
        ready(result)
    }
}

#[tokio::test(flavor = "current_thread")]
async fn direction_filter_is_local_to_exactly_one_source_page_and_preserves_continuation()
-> TestResult {
    // Setup.
    let before = RequestsBefore::from_str("request-current")?;
    let next = RequestsBefore::from_str("request-next")?;
    let outgoing = request(1, RequestDirection::Outgoing, "one")?;
    let incoming = request(2, RequestDirection::Incoming, "two")?;
    let limit = Limit::try_from(2)?;

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader {
        script: ReaderScript::Present,
        transcript: Rc::clone(&transcript),
    };
    let api = FakeApi::new(
        Ok(PendingRequestsPage::new(
            vec![outgoing, incoming.clone()],
            Some(next.clone()),
        )),
        Rc::clone(&transcript),
    );

    // Complete expected final outcome and state.
    let expected = Observation {
        outcome: ListOutcome::Success(RequestsResult::new(
            vec![incoming],
            RequestDirectionFilter::Incoming,
            Some(next),
        )),
        transcript: expected_calls(limit, Some(before.clone()))?,
    };

    // Execute once.
    let result = list(
        &reader,
        &api,
        RequestDirectionFilter::Incoming,
        limit,
        Some(&before),
    )
    .await;
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

#[tokio::test(flavor = "current_thread")]
async fn empty_filtered_and_duplicate_pages_compare_complete_results_after_one_call() -> TestResult
{
    let duplicate = request(3, RequestDirection::Incoming, "same")?;
    for (page, expected_requests, next) in [
        (
            vec![request(1, RequestDirection::Outgoing, "filtered")?],
            Vec::new(),
            Some(RequestsBefore::from_str("next-filtered")?),
        ),
        (
            Vec::new(),
            Vec::new(),
            Some(RequestsBefore::from_str("next-empty")?),
        ),
        (
            vec![duplicate.clone(), duplicate.clone()],
            vec![duplicate],
            None,
        ),
    ] {
        // Setup and immutable initial state.
        let limit = Limit::try_from(10)?;
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader {
            script: ReaderScript::Present,
            transcript: Rc::clone(&transcript),
        };
        let api = FakeApi::new(
            Ok(PendingRequestsPage::new(page, next.clone())),
            Rc::clone(&transcript),
        );

        // Complete expected final outcome and state.
        let expected = Observation {
            outcome: ListOutcome::Success(RequestsResult::new(
                expected_requests,
                RequestDirectionFilter::Incoming,
                next,
            )),
            transcript: expected_calls(limit, None)?,
        };

        // Execute once.
        let result = list(&reader, &api, RequestDirectionFilter::Incoming, limit, None).await;
        let observed = Observation {
            outcome: project(result),
            transcript: transcript.borrow().clone(),
        };

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn response_contract_failures_are_rejected_after_exactly_one_source_call() -> TestResult {
    let before = RequestsBefore::from_str("same")?;
    for (page, limit, expected_problem) in [
        (
            PendingRequestsPage::new(
                vec![
                    request(1, RequestDirection::Incoming, "first")?,
                    request(1, RequestDirection::Incoming, "changed")?,
                ],
                None,
            ),
            Limit::try_from(2)?,
            "the API returned conflicting records for one request ID",
        ),
        (
            PendingRequestsPage::new(
                vec![
                    request(1, RequestDirection::Incoming, "one")?,
                    request(2, RequestDirection::Incoming, "two")?,
                ],
                None,
            ),
            Limit::try_from(1)?,
            "the API returned more pending requests than requested",
        ),
        (
            PendingRequestsPage::new(
                vec![request(1, RequestDirection::Incoming, "one")?],
                Some(before.clone()),
            ),
            Limit::try_from(1)?,
            "the API returned a repeated or non-progressing before continuation",
        ),
    ] {
        // Setup and immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader {
            script: ReaderScript::Present,
            transcript: Rc::clone(&transcript),
        };
        let api = FakeApi::new(Ok(page), Rc::clone(&transcript));

        // Complete expected final outcome and state.
        let expected = Observation {
            outcome: ListOutcome::Failure(ListFailure::ResponseContract(expected_problem)),
            transcript: expected_calls(limit, Some(before.clone()))?,
        };

        // Execute once.
        let result = list(
            &reader,
            &api,
            RequestDirectionFilter::All,
            limit,
            Some(&before),
        )
        .await;
        let observed = Observation {
            outcome: project(result),
            transcript: transcript.borrow().clone(),
        };

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn credential_missing_read_failure_and_api_failure_have_exact_transcripts() -> TestResult {
    for (reader_script, page, expected_failure, expect_api) in [
        (
            ReaderScript::Missing,
            Ok(PendingRequestsPage::new(Vec::new(), None)),
            ListFailure::CredentialMissing,
            false,
        ),
        (
            ReaderScript::Failure,
            Ok(PendingRequestsPage::new(Vec::new(), None)),
            ListFailure::CredentialRead,
            false,
        ),
        (
            ReaderScript::Present,
            Err(FakeApiError(ApiFailureKind::Timeout)),
            ListFailure::Api(ApiFailureKind::Timeout),
            true,
        ),
    ] {
        // Setup and immutable initial state.
        let limit = Limit::try_from(10)?;
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader {
            script: reader_script,
            transcript: Rc::clone(&transcript),
        };
        let api = FakeApi::new(page, Rc::clone(&transcript));

        // Complete expected final outcome and state.
        let expected_calls = if expect_api {
            expected_calls(limit, None)?
        } else {
            vec![Call::ReadCredential]
        };
        let expected = Observation {
            outcome: ListOutcome::Failure(expected_failure),
            transcript: expected_calls,
        };

        // Execute once.
        let result = list(&reader, &api, RequestDirectionFilter::All, limit, None).await;
        let observed = Observation {
            outcome: project(result),
            transcript: transcript.borrow().clone(),
        };

        assert_eq!(observed, expected);
    }
    Ok(())
}

fn expected_calls(
    limit: Limit,
    before: Option<RequestsBefore>,
) -> Result<Vec<Call>, Box<dyn Error>> {
    Ok(vec![
        Call::ReadCredential,
        Call::PendingRequests {
            session: RedactedSecret::Redacted,
            current_user_id: UserId::from_str("1000")?,
            page: PendingRequestsPageRequest::new(limit, before),
        },
    ])
}

fn credential() -> Result<LoadedCredential, FakeCredentialError> {
    Ok(LoadedCredential {
        envelope: CredentialEnvelope::new(
            AccessToken::from_str(ACCESS_TOKEN).map_err(|_| FakeCredentialError)?,
            DeviceId::from_str(DEVICE_ID).map_err(|_| FakeCredentialError)?,
            UserId::from_str("1000").map_err(|_| FakeCredentialError)?,
            Username::from_bare("tester").map_err(|_| FakeCredentialError)?,
            Some("Synthetic tester".to_owned()),
            OffsetDateTime::UNIX_EPOCH,
        ),
        format: CredentialFormat::Version1,
    })
}

fn request(
    id: u32,
    direction: RequestDirection,
    note: &str,
) -> Result<RequestRecord, Box<dyn Error>> {
    Ok(RequestRecord::new(
        RequestId::from_str(&format!("request-{id}"))?,
        RequestAction::Charge,
        direction,
        User::new(
            UserId::from_str(&(id + 10_000).to_string())?,
            Some(Username::from_bare(format!("counterparty{id}"))?),
            None,
        ),
        Money::from_cents(u64::from(id) + 100)?,
        Some(note.to_owned()),
        Some(OffsetDateTime::UNIX_EPOCH + Duration::seconds(i64::from(id))),
        RequestStatus::from_str("pending")?,
    ))
}

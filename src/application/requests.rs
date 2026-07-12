use std::collections::HashMap;
use std::str::FromStr;

use thiserror::Error;

use super::auth::OperationFailure;
use super::ports::{
    ApiFailure, ApiFailureKind, CredentialStore, PendingRequestsPageRequest,
    PendingRequestsPageToken, RequestsApi,
};
use super::read::ReadFailureKind;
use crate::domain::{Limit, PendingRequest, RequestDirectionFilter, RequestId, RequestsBefore};

#[derive(Debug)]
pub struct RequestsResult {
    requests: Vec<PendingRequest>,
    direction: RequestDirectionFilter,
    next_before: Option<RequestsBefore>,
}

impl RequestsResult {
    #[must_use]
    pub(crate) fn new(
        requests: Vec<PendingRequest>,
        direction: RequestDirectionFilter,
        next_before: Option<RequestsBefore>,
    ) -> Self {
        Self {
            requests,
            direction,
            next_before,
        }
    }

    #[must_use]
    pub fn requests(&self) -> &[PendingRequest] {
        &self.requests
    }

    #[must_use]
    pub const fn direction(&self) -> RequestDirectionFilter {
        self.direction
    }

    #[must_use]
    pub const fn next_before(&self) -> Option<&RequestsBefore> {
        self.next_before.as_ref()
    }
}

#[derive(Debug, Error)]
pub enum RequestsError {
    #[error("no Venmo credential is stored; run `venmo auth login`")]
    MissingCredential,

    #[error("failed to read the OS credential entry: {source}")]
    CredentialLoad {
        #[source]
        source: OperationFailure,
    },

    #[error("failed to list pending Venmo requests: {source}")]
    Api {
        kind: ApiFailureKind,
        #[source]
        source: OperationFailure,
    },

    #[error("cannot use Venmo pending-request pagination because {problem}")]
    PaginationContract { problem: &'static str },
}

impl RequestsError {
    #[must_use]
    pub const fn failure_kind(&self) -> ReadFailureKind {
        match self {
            Self::MissingCredential | Self::CredentialLoad { .. } => ReadFailureKind::Credential,
            Self::Api { kind, .. } => ReadFailureKind::Api(*kind),
            Self::PaginationContract { .. } => ReadFailureKind::PaginationContract,
        }
    }
}

pub async fn list<S, A>(
    store: &S,
    api: &A,
    direction: RequestDirectionFilter,
    limit: Limit,
    before: Option<&RequestsBefore>,
) -> Result<RequestsResult, RequestsError>
where
    S: CredentialStore,
    A: RequestsApi,
{
    let loaded = store
        .load()
        .map_err(|source| RequestsError::CredentialLoad {
            source: OperationFailure::new(source),
        })?
        .ok_or(RequestsError::MissingCredential)?;
    let current = before.map(|value| PendingRequestsPageToken::new(value.as_str().to_owned()));
    let page = api
        .pending_requests(
            loaded.envelope.access_token(),
            loaded.envelope.device_id(),
            loaded.envelope.user_id(),
            PendingRequestsPageRequest::new(limit.as_nonzero(), current.clone()),
        )
        .await
        .map_err(|source| RequestsError::Api {
            kind: source.kind(),
            source: OperationFailure::new(source),
        })?;
    let (page_requests, next_token) = page.into_parts();
    validate_page_len(page_requests.len(), limit)?;
    validate_next_token(current.as_ref(), next_token.as_ref())?;
    let requests = buffer_and_filter(page_requests, direction)?;
    let next_before = next_token
        .map(|token| RequestsBefore::from_str(token.as_str()))
        .transpose()
        .map_err(|_| RequestsError::PaginationContract {
            problem: "the API returned an invalid before continuation",
        })?;
    Ok(RequestsResult::new(requests, direction, next_before))
}

fn buffer_and_filter(
    page: Vec<PendingRequest>,
    direction: RequestDirectionFilter,
) -> Result<Vec<PendingRequest>, RequestsError> {
    let mut requests = Vec::with_capacity(page.len());
    let mut seen = HashMap::<RequestId, PendingRequest>::with_capacity(page.len());
    for request in page {
        match seen.get(request.id()) {
            Some(existing) if existing == &request => {}
            Some(_) => {
                return Err(RequestsError::PaginationContract {
                    problem: "the API returned conflicting records for one request ID",
                });
            }
            None => {
                seen.insert(request.id().clone(), request.clone());
                if direction.matches(request.direction()) {
                    requests.push(request);
                }
            }
        }
    }
    Ok(requests)
}

fn validate_next_token(
    current: Option<&PendingRequestsPageToken>,
    next: Option<&PendingRequestsPageToken>,
) -> Result<(), RequestsError> {
    if current == next && next.is_some() {
        return Err(RequestsError::PaginationContract {
            problem: "the API returned a repeated or non-progressing before continuation",
        });
    }
    Ok(())
}

fn validate_page_len(actual: usize, requested: Limit) -> Result<(), RequestsError> {
    if actual > requested.get() as usize {
        return Err(RequestsError::PaginationContract {
            problem: "the API returned more pending requests than requested",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::error::Error;
    use std::future::{Future, ready};

    use time::{Duration, OffsetDateTime};

    use super::*;
    use crate::application::ports::{
        CredentialDeleteOutcome, CredentialFailureKind, CredentialFormat, CredentialStoreFailure,
        LoadedCredential, PendingRequestsPage,
    };
    use crate::domain::{
        AccessToken, CredentialEnvelope, DeviceId, Money, RequestDirection, RequestStatus, User,
        UserId, Username,
    };

    type TestResult = Result<(), Box<dyn Error>>;

    struct FakeStore(bool);

    #[derive(Debug, Error)]
    #[error("synthetic credential failure")]
    struct FakeCredentialError;

    impl CredentialStoreFailure for FakeCredentialError {
        fn kind(&self) -> CredentialFailureKind {
            CredentialFailureKind::Internal
        }
    }

    impl CredentialStore for FakeStore {
        type Error = FakeCredentialError;

        fn load(&self) -> Result<Option<LoadedCredential>, Self::Error> {
            self.0.then(credential).transpose()
        }

        fn save(&self, _credential: &CredentialEnvelope) -> Result<(), Self::Error> {
            Err(FakeCredentialError)
        }

        fn delete(&self) -> Result<CredentialDeleteOutcome, Self::Error> {
            Err(FakeCredentialError)
        }
    }

    #[derive(Clone, Copy, Debug, Error)]
    #[error("synthetic API failure")]
    struct FakeApiError(ApiFailureKind);

    impl ApiFailure for FakeApiError {
        fn kind(&self) -> ApiFailureKind {
            self.0
        }
    }

    struct FakeApi {
        pages: RefCell<VecDeque<Result<PendingRequestsPage, FakeApiError>>>,
        list_calls: Cell<usize>,
        lookup_calls: Cell<usize>,
        sizes: RefCell<Vec<u32>>,
        tokens: RefCell<Vec<Option<String>>>,
    }

    impl FakeApi {
        fn new(pages: Vec<Result<PendingRequestsPage, FakeApiError>>) -> Self {
            Self {
                pages: RefCell::new(pages.into()),
                list_calls: Cell::new(0),
                lookup_calls: Cell::new(0),
                sizes: RefCell::new(Vec::new()),
                tokens: RefCell::new(Vec::new()),
            }
        }
    }

    impl RequestsApi for FakeApi {
        type Error = FakeApiError;

        fn pending_requests<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            _current_user_id: &'a UserId,
            request: PendingRequestsPageRequest,
        ) -> impl Future<Output = Result<PendingRequestsPage, Self::Error>> + Send + 'a {
            self.list_calls.set(self.list_calls.get() + 1);
            self.sizes.borrow_mut().push(request.page_size().get());
            self.tokens.borrow_mut().push(
                request
                    .token()
                    .map(PendingRequestsPageToken::as_str)
                    .map(str::to_owned),
            );
            ready(
                self.pages
                    .borrow_mut()
                    .pop_front()
                    .unwrap_or(Err(FakeApiError(ApiFailureKind::Internal))),
            )
        }

        fn pending_request_by_id<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            _current_user_id: &'a UserId,
            _request_id: &'a RequestId,
        ) -> impl Future<Output = Result<PendingRequest, Self::Error>> + Send + 'a {
            self.lookup_calls.set(self.lookup_calls.get() + 1);
            ready(Err(FakeApiError(ApiFailureKind::Internal)))
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn direction_is_local_to_one_source_page_and_source_continuation_is_preserved()
    -> TestResult {
        let api = FakeApi::new(vec![
            Ok(PendingRequestsPage::new(
                vec![
                    request(1, RequestDirection::Outgoing, "one")?,
                    request(2, RequestDirection::Incoming, "two")?,
                ],
                Some(PendingRequestsPageToken::new("request-next".to_owned())),
            )),
            Err(FakeApiError(ApiFailureKind::Internal)),
        ]);
        let before = RequestsBefore::from_str("request-current")?;
        let result = list(
            &FakeStore(true),
            &api,
            RequestDirectionFilter::Incoming,
            Limit::try_from(2)?,
            Some(&before),
        )
        .await?;

        assert_eq!(result.requests().len(), 1);
        assert_eq!(result.requests()[0].id().as_str(), "request-2");
        assert_eq!(result.direction(), RequestDirectionFilter::Incoming);
        assert_eq!(
            result.next_before().map(RequestsBefore::as_str),
            Some("request-next")
        );
        assert_eq!(api.list_calls.get(), 1);
        assert_eq!(api.sizes.borrow().as_slice(), &[2]);
        assert_eq!(
            api.tokens.borrow().as_slice(),
            &[Some("request-current".to_owned())]
        );
        assert_eq!(api.lookup_calls.get(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn empty_filtered_or_source_page_can_still_return_source_continuation() -> TestResult {
        for page in [
            PendingRequestsPage::new(
                vec![request(1, RequestDirection::Outgoing, "one")?],
                Some(PendingRequestsPageToken::new("next-one".to_owned())),
            ),
            PendingRequestsPage::new(
                Vec::new(),
                Some(PendingRequestsPageToken::new("next-two".to_owned())),
            ),
        ] {
            let api = FakeApi::new(vec![Ok(page)]);
            let result = list(
                &FakeStore(true),
                &api,
                RequestDirectionFilter::Incoming,
                Limit::try_from(10)?,
                None,
            )
            .await?;
            assert!(result.requests().is_empty());
            assert!(result.next_before().is_some());
            assert_eq!(api.list_calls.get(), 1);
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn malformed_pages_fail_closed_after_one_request() -> TestResult {
        let before = RequestsBefore::from_str("same")?;
        let cases = [
            (
                PendingRequestsPage::new(
                    vec![
                        request(1, RequestDirection::Incoming, "first")?,
                        request(1, RequestDirection::Incoming, "changed")?,
                    ],
                    None,
                ),
                2,
            ),
            (
                PendingRequestsPage::new(
                    vec![
                        request(1, RequestDirection::Incoming, "one")?,
                        request(2, RequestDirection::Incoming, "two")?,
                    ],
                    None,
                ),
                1,
            ),
            (
                PendingRequestsPage::new(
                    vec![request(1, RequestDirection::Incoming, "one")?],
                    Some(PendingRequestsPageToken::new("same".to_owned())),
                ),
                1,
            ),
            (
                PendingRequestsPage::new(
                    vec![request(1, RequestDirection::Incoming, "one")?],
                    Some(PendingRequestsPageToken::new("bad token".to_owned())),
                ),
                1,
            ),
        ];
        for (page, limit) in cases {
            let api = FakeApi::new(vec![Ok(page)]);
            let result = list(
                &FakeStore(true),
                &api,
                RequestDirectionFilter::All,
                Limit::try_from(limit)?,
                Some(&before),
            )
            .await;
            assert!(matches!(
                result,
                Err(RequestsError::PaginationContract { .. })
            ));
            assert_eq!(api.list_calls.get(), 1);
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn missing_credentials_make_no_api_request() -> TestResult {
        let api = FakeApi::new(Vec::new());
        let result = list(
            &FakeStore(false),
            &api,
            RequestDirectionFilter::All,
            Limit::try_from(10)?,
            None,
        )
        .await;
        assert!(matches!(result, Err(RequestsError::MissingCredential)));
        assert_eq!(api.list_calls.get(), 0);
        Ok(())
    }

    fn credential() -> Result<LoadedCredential, FakeCredentialError> {
        Ok(LoadedCredential {
            envelope: CredentialEnvelope::new(
                AccessToken::from_str("synthetic-token").map_err(|_| FakeCredentialError)?,
                DeviceId::from_str("synthetic-device").map_err(|_| FakeCredentialError)?,
                UserId::from_str("1000").map_err(|_| FakeCredentialError)?,
                Username::from_bare("tester").map_err(|_| FakeCredentialError)?,
                Some("Test User".to_owned()),
                OffsetDateTime::UNIX_EPOCH,
            ),
            format: CredentialFormat::Version1,
        })
    }

    fn request(
        id: u32,
        direction: RequestDirection,
        note: &str,
    ) -> Result<PendingRequest, Box<dyn Error>> {
        Ok(PendingRequest::new(
            RequestId::from_str(&format!("request-{id}"))?,
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
}

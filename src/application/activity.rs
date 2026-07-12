use std::collections::HashMap;
use std::str::FromStr;

use thiserror::Error;

use super::auth::OperationFailure;
use super::ports::{
    ActivityApi, ActivityPageRequest, ActivityPageToken, ApiFailure, ApiFailureKind,
    CredentialStore,
};
use super::read::ReadFailureKind;
use crate::domain::{Activity, ActivityBeforeId, ActivityId, Limit};

#[derive(Debug)]
pub struct ActivityListResult {
    activities: Vec<Activity>,
    next_before_id: Option<ActivityBeforeId>,
}

impl ActivityListResult {
    #[must_use]
    pub(crate) fn new(activities: Vec<Activity>, next_before_id: Option<ActivityBeforeId>) -> Self {
        Self {
            activities,
            next_before_id,
        }
    }

    #[must_use]
    pub fn activities(&self) -> &[Activity] {
        &self.activities
    }

    #[must_use]
    pub const fn next_before_id(&self) -> Option<&ActivityBeforeId> {
        self.next_before_id.as_ref()
    }
}

#[derive(Debug)]
pub struct ActivityShowResult {
    activity: Activity,
}

impl ActivityShowResult {
    #[must_use]
    pub(crate) const fn new(activity: Activity) -> Self {
        Self { activity }
    }

    #[must_use]
    pub const fn activity(&self) -> &Activity {
        &self.activity
    }
}

#[derive(Debug, Error)]
pub enum ActivityError {
    #[error("no Venmo credential is stored; run `venmo auth login`")]
    MissingCredential,

    #[error("failed to read the OS credential entry: {source}")]
    CredentialLoad {
        #[source]
        source: OperationFailure,
    },

    #[error("failed to read Venmo activity: {source}")]
    Api {
        kind: ApiFailureKind,
        #[source]
        source: OperationFailure,
    },

    #[error("cannot use Venmo activity pagination because {problem}")]
    PaginationContract { problem: &'static str },
}

impl ActivityError {
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
    limit: Limit,
    before_id: Option<&ActivityBeforeId>,
) -> Result<ActivityListResult, ActivityError>
where
    S: CredentialStore,
    A: ActivityApi,
{
    let loaded = load(store)?;
    let current = before_id.map(|value| ActivityPageToken::new(value.as_str().to_owned()));
    let page = api
        .activity(
            loaded.envelope.access_token(),
            loaded.envelope.device_id(),
            loaded.envelope.user_id(),
            ActivityPageRequest::new(limit.as_nonzero(), current.clone()),
        )
        .await
        .map_err(|source| ActivityError::Api {
            kind: source.kind(),
            source: OperationFailure::new(source),
        })?;
    let (page_activities, next_token) = page.into_parts();
    validate_page_len(page_activities.len(), limit)?;
    validate_next_token(current.as_ref(), next_token.as_ref())?;
    if page_activities.is_empty() && next_token.is_some() {
        return Err(ActivityError::PaginationContract {
            problem: "the API returned an empty page with a before-id continuation",
        });
    }
    let activities = buffer_activities(page_activities)?;
    let next_before_id = next_token
        .map(|token| ActivityBeforeId::from_str(token.as_str()))
        .transpose()
        .map_err(|_| ActivityError::PaginationContract {
            problem: "the API returned an invalid before-id continuation",
        })?;
    Ok(ActivityListResult::new(activities, next_before_id))
}

fn buffer_activities(page: Vec<Activity>) -> Result<Vec<Activity>, ActivityError> {
    let mut activities = Vec::with_capacity(page.len());
    let mut seen = HashMap::<ActivityId, Activity>::with_capacity(page.len());
    for activity in page {
        match seen.get(activity.id()) {
            Some(existing) if existing == &activity => {}
            Some(_) => {
                return Err(ActivityError::PaginationContract {
                    problem: "the API returned conflicting records for one activity ID",
                });
            }
            None => {
                seen.insert(activity.id().clone(), activity.clone());
                activities.push(activity);
            }
        }
    }
    Ok(activities)
}

fn validate_next_token(
    current: Option<&ActivityPageToken>,
    next: Option<&ActivityPageToken>,
) -> Result<(), ActivityError> {
    if current == next && next.is_some() {
        return Err(ActivityError::PaginationContract {
            problem: "the API returned a repeated or non-progressing before-id continuation",
        });
    }
    Ok(())
}

pub async fn show<S, A>(
    store: &S,
    api: &A,
    activity_id: &ActivityId,
) -> Result<ActivityShowResult, ActivityError>
where
    S: CredentialStore,
    A: ActivityApi,
{
    let loaded = load(store)?;
    let activity = api
        .activity_by_id(
            loaded.envelope.access_token(),
            loaded.envelope.device_id(),
            loaded.envelope.user_id(),
            activity_id,
        )
        .await
        .map_err(|source| ActivityError::Api {
            kind: source.kind(),
            source: OperationFailure::new(source),
        })?;
    if activity.id() != activity_id {
        return Err(ActivityError::PaginationContract {
            problem: "the API returned a different activity ID",
        });
    }
    Ok(ActivityShowResult::new(activity))
}

fn load<S: CredentialStore>(store: &S) -> Result<super::ports::LoadedCredential, ActivityError> {
    store
        .load()
        .map_err(|source| ActivityError::CredentialLoad {
            source: OperationFailure::new(source),
        })?
        .ok_or(ActivityError::MissingCredential)
}

fn validate_page_len(actual: usize, requested: Limit) -> Result<(), ActivityError> {
    if actual > requested.get() as usize {
        return Err(ActivityError::PaginationContract {
            problem: "the API returned more activity records than requested",
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
        ActivityPage, CredentialDeleteOutcome, CredentialFailureKind, CredentialFormat,
        CredentialStoreFailure, LoadedCredential,
    };
    use crate::domain::{
        AccessToken, ActivityAction, ActivityCounterparty, ActivityDirection, ActivityStatus,
        CredentialEnvelope, DeviceId, Money, User, UserId, Username,
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
        pages: RefCell<VecDeque<Result<ActivityPage, FakeApiError>>>,
        lookups: RefCell<VecDeque<Result<Activity, FakeApiError>>>,
        list_calls: Cell<usize>,
        show_calls: Cell<usize>,
        sizes: RefCell<Vec<u32>>,
        tokens: RefCell<Vec<Option<String>>>,
    }

    impl FakeApi {
        fn pages(pages: Vec<Result<ActivityPage, FakeApiError>>) -> Self {
            Self {
                pages: RefCell::new(pages.into()),
                lookups: RefCell::new(VecDeque::new()),
                list_calls: Cell::new(0),
                show_calls: Cell::new(0),
                sizes: RefCell::new(Vec::new()),
                tokens: RefCell::new(Vec::new()),
            }
        }

        fn lookups(lookups: Vec<Result<Activity, FakeApiError>>) -> Self {
            Self {
                pages: RefCell::new(VecDeque::new()),
                lookups: RefCell::new(lookups.into()),
                list_calls: Cell::new(0),
                show_calls: Cell::new(0),
                sizes: RefCell::new(Vec::new()),
                tokens: RefCell::new(Vec::new()),
            }
        }
    }

    impl ActivityApi for FakeApi {
        type Error = FakeApiError;

        fn activity<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            _current_user_id: &'a UserId,
            request: ActivityPageRequest,
        ) -> impl Future<Output = Result<ActivityPage, Self::Error>> + Send + 'a {
            self.list_calls.set(self.list_calls.get() + 1);
            self.sizes.borrow_mut().push(request.page_size().get());
            self.tokens.borrow_mut().push(
                request
                    .token()
                    .map(ActivityPageToken::as_str)
                    .map(str::to_owned),
            );
            ready(
                self.pages
                    .borrow_mut()
                    .pop_front()
                    .unwrap_or(Err(FakeApiError(ApiFailureKind::Internal))),
            )
        }

        fn activity_by_id<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            _current_user_id: &'a UserId,
            _activity_id: &'a ActivityId,
        ) -> impl Future<Output = Result<Activity, Self::Error>> + Send + 'a {
            self.show_calls.set(self.show_calls.get() + 1);
            ready(
                self.lookups
                    .borrow_mut()
                    .pop_front()
                    .unwrap_or(Err(FakeApiError(ApiFailureKind::Internal))),
            )
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn list_fetches_one_page_and_returns_native_before_id() -> TestResult {
        let duplicate = activity(1, "note")?;
        let api = FakeApi::pages(vec![
            Ok(ActivityPage::new(
                vec![duplicate.clone(), duplicate],
                Some(ActivityPageToken::new("story-next".to_owned())),
            )),
            Err(FakeApiError(ApiFailureKind::Internal)),
        ]);
        let before = ActivityBeforeId::from_str("story-current")?;
        let result = list(&FakeStore(true), &api, Limit::try_from(2)?, Some(&before)).await?;

        assert_eq!(result.activities().len(), 1);
        assert_eq!(
            result.next_before_id().map(ActivityBeforeId::as_str),
            Some("story-next")
        );
        assert_eq!(api.list_calls.get(), 1);
        assert_eq!(api.sizes.borrow().as_slice(), &[2]);
        assert_eq!(
            api.tokens.borrow().as_slice(),
            &[Some("story-current".to_owned())]
        );
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn malformed_pages_fail_without_a_second_request() -> TestResult {
        let before = ActivityBeforeId::from_str("same")?;
        let cases = [
            (
                ActivityPage::new(vec![activity(1, "first")?, activity(1, "changed")?], None),
                2,
            ),
            (
                ActivityPage::new(vec![activity(1, "one")?, activity(2, "two")?], None),
                1,
            ),
            (
                ActivityPage::new(Vec::new(), Some(ActivityPageToken::new("next".to_owned()))),
                1,
            ),
            (
                ActivityPage::new(
                    vec![activity(1, "one")?],
                    Some(ActivityPageToken::new("same".to_owned())),
                ),
                1,
            ),
            (
                ActivityPage::new(
                    vec![activity(1, "one")?],
                    Some(ActivityPageToken::new("bad token".to_owned())),
                ),
                1,
            ),
        ];
        for (page, limit) in cases {
            let api = FakeApi::pages(vec![Ok(page)]);
            let result = list(
                &FakeStore(true),
                &api,
                Limit::try_from(limit)?,
                Some(&before),
            )
            .await;
            assert!(matches!(
                result,
                Err(ActivityError::PaginationContract { .. })
            ));
            assert_eq!(api.list_calls.get(), 1);
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn show_requires_the_exact_activity_id() -> TestResult {
        let requested = ActivityId::from_str("story-1")?;
        let mismatch_api = FakeApi::lookups(vec![Ok(activity(2, "note")?)]);
        let mismatch = show(&FakeStore(true), &mismatch_api, &requested).await;
        assert!(matches!(
            mismatch,
            Err(ActivityError::PaginationContract { .. })
        ));

        let exact_api = FakeApi::lookups(vec![Ok(activity(1, "note")?)]);
        let exact = show(&FakeStore(true), &exact_api, &requested).await?;
        assert_eq!(exact.activity().id(), &requested);
        assert_eq!(exact_api.show_calls.get(), 1);
        assert_eq!(exact_api.list_calls.get(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn missing_credentials_make_no_api_request() -> TestResult {
        let api = FakeApi::pages(Vec::new());
        let result = list(&FakeStore(false), &api, Limit::try_from(10)?, None).await;
        assert!(matches!(result, Err(ActivityError::MissingCredential)));
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
}

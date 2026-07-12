use std::collections::HashMap;
use std::num::NonZeroU32;

use thiserror::Error;

use super::auth::OperationFailure;
use super::ports::{
    ApiFailure, ApiFailureKind, CredentialStore, UserSearchPageRequest, UserSearchPageToken,
    UsersApi,
};
use crate::domain::{
    CredentialEnvelope, Limit, MAX_EXHAUSTIVE_USER_SEARCH_RESULTS, Offset, User, UserId,
    UserSearchQuery,
};

const EXHAUSTIVE_PAGE_SIZE: u32 = 50;
const MAX_EXHAUSTIVE_PAGE_REQUESTS: u8 = 4;

#[derive(Debug)]
pub struct UserSearchResult {
    users: Vec<User>,
    next_offset: Option<Offset>,
}

impl UserSearchResult {
    #[must_use]
    pub(crate) fn new(users: Vec<User>, next_offset: Option<Offset>) -> Self {
        Self { users, next_offset }
    }

    #[must_use]
    pub fn users(&self) -> &[User] {
        &self.users
    }

    #[must_use]
    pub const fn next_offset(&self) -> Option<Offset> {
        self.next_offset
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExhaustiveSearchCompletion {
    Exhausted,
    SafetyBoundReached,
}

#[derive(Debug)]
pub(crate) struct ExhaustiveUserSearchResult {
    users: Vec<User>,
    completion: ExhaustiveSearchCompletion,
}

impl ExhaustiveUserSearchResult {
    pub(crate) fn users(&self) -> &[User] {
        &self.users
    }

    pub(crate) const fn completion(&self) -> ExhaustiveSearchCompletion {
        self.completion
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserSearchFailureKind {
    Credential,
    Api(ApiFailureKind),
    PaginationContract,
    Internal,
}

#[derive(Debug, Error)]
pub enum UserSearchError {
    #[error("no Venmo credential is stored; run `venmo auth login`")]
    MissingCredential,

    #[error("failed to read the OS credential entry: {source}")]
    CredentialLoad {
        #[source]
        source: OperationFailure,
    },

    #[error("failed to search Venmo users: {source}")]
    Api {
        kind: ApiFailureKind,
        #[source]
        source: OperationFailure,
    },

    #[error("cannot use Venmo user-search pagination because {problem}")]
    PaginationContract { problem: &'static str },

    #[error("user-search processing failed because {problem}")]
    Internal { problem: &'static str },
}

impl UserSearchError {
    #[must_use]
    pub const fn failure_kind(&self) -> UserSearchFailureKind {
        match self {
            Self::MissingCredential | Self::CredentialLoad { .. } => {
                UserSearchFailureKind::Credential
            }
            Self::Api { kind, .. } => UserSearchFailureKind::Api(*kind),
            Self::PaginationContract { .. } => UserSearchFailureKind::PaginationContract,
            Self::Internal { .. } => UserSearchFailureKind::Internal,
        }
    }
}

pub async fn search<S, A>(
    store: &S,
    api: &A,
    query: &UserSearchQuery,
    limit: Limit,
    offset: Offset,
) -> Result<UserSearchResult, UserSearchError>
where
    S: CredentialStore,
    A: UsersApi,
{
    let loaded = store
        .load()
        .map_err(|source| UserSearchError::CredentialLoad {
            source: OperationFailure::new(source),
        })?
        .ok_or(UserSearchError::MissingCredential)?;
    let current = (offset.get() != 0).then(|| UserSearchPageToken::from_offset(offset.get()));
    let page = request_page(&loaded.envelope, api, query, limit.as_nonzero(), current).await?;
    let (page_users, next_token) = page.into_parts();
    validate_page_len(page_users.len(), limit.as_nonzero())?;
    validate_next_offset(offset.get(), next_token)?;
    if page_users.is_empty() && next_token.is_some() {
        return Err(UserSearchError::PaginationContract {
            problem: "the API returned an empty page with a continuation offset",
        });
    }
    let users = buffer_public_page(page_users)?;
    Ok(UserSearchResult::new(
        users,
        next_token.map(|token| Offset::new(token.offset())),
    ))
}

pub(crate) async fn search_exhaustively_with_credential<A>(
    credential: &CredentialEnvelope,
    api: &A,
    query: &UserSearchQuery,
) -> Result<ExhaustiveUserSearchResult, UserSearchError>
where
    A: UsersApi,
{
    let capacity = usize::try_from(MAX_EXHAUSTIVE_USER_SEARCH_RESULTS).map_err(|_| {
        UserSearchError::Internal {
            problem: "the exhaustive user-search bound did not fit in memory addressing",
        }
    })?;
    let mut users = Vec::with_capacity(capacity);
    let mut seen = HashMap::<UserId, User>::with_capacity(capacity);
    let mut current_offset = 0_u32;

    for _ in 0..MAX_EXHAUSTIVE_PAGE_REQUESTS {
        let collected = u32::try_from(users.len()).map_err(|_| UserSearchError::Internal {
            problem: "the collected user count exceeded its bounded representation",
        })?;
        let page_size = MAX_EXHAUSTIVE_USER_SEARCH_RESULTS
            .saturating_sub(collected)
            .min(EXHAUSTIVE_PAGE_SIZE);
        let page_size = NonZeroU32::new(page_size).ok_or(UserSearchError::Internal {
            problem: "the exhaustive user search attempted a zero-sized page",
        })?;
        let current =
            (current_offset != 0).then(|| UserSearchPageToken::from_offset(current_offset));
        let page = request_page(credential, api, query, page_size, current).await?;
        let (page_users, next_token) = page.into_parts();
        validate_page_len(page_users.len(), page_size)?;
        validate_next_offset(current_offset, next_token)?;
        if page_users.is_empty() {
            if next_token.is_some() {
                return Err(UserSearchError::PaginationContract {
                    problem: "the API returned an empty page with a continuation offset",
                });
            }
            return Ok(ExhaustiveUserSearchResult {
                users,
                completion: ExhaustiveSearchCompletion::Exhausted,
            });
        }

        let previous_len = users.len();
        merge_users(page_users, &mut users, &mut seen)?;
        if users.len() == previous_len && next_token.is_some() {
            return Err(UserSearchError::PaginationContract {
                problem: "the API returned a continuation page with no new users",
            });
        }
        let Some(next) = next_token else {
            return Ok(ExhaustiveUserSearchResult {
                users,
                completion: ExhaustiveSearchCompletion::Exhausted,
            });
        };
        current_offset = next.offset();
        if users.len() >= capacity {
            return Ok(ExhaustiveUserSearchResult {
                users,
                completion: ExhaustiveSearchCompletion::SafetyBoundReached,
            });
        }
    }

    Ok(ExhaustiveUserSearchResult {
        users,
        completion: ExhaustiveSearchCompletion::SafetyBoundReached,
    })
}

async fn request_page<A: UsersApi>(
    credential: &CredentialEnvelope,
    api: &A,
    query: &UserSearchQuery,
    page_size: NonZeroU32,
    token: Option<UserSearchPageToken>,
) -> Result<super::ports::UserSearchPage, UserSearchError> {
    api.search_users(
        credential.access_token(),
        credential.device_id(),
        query,
        UserSearchPageRequest::new(page_size, token),
    )
    .await
    .map_err(|source| UserSearchError::Api {
        kind: source.kind(),
        source: OperationFailure::new(source),
    })
}

fn buffer_public_page(page_users: Vec<User>) -> Result<Vec<User>, UserSearchError> {
    let mut users = Vec::with_capacity(page_users.len());
    let mut seen = HashMap::with_capacity(page_users.len());
    merge_users(page_users, &mut users, &mut seen)?;
    Ok(users)
}

fn merge_users(
    page_users: Vec<User>,
    users: &mut Vec<User>,
    seen: &mut HashMap<UserId, User>,
) -> Result<(), UserSearchError> {
    for user in page_users {
        match seen.get(user.user_id()) {
            Some(existing) if existing == &user => {}
            Some(_) => {
                return Err(UserSearchError::PaginationContract {
                    problem: "the API returned conflicting records for one user ID",
                });
            }
            None => {
                seen.insert(user.user_id().clone(), user.clone());
                users.push(user);
            }
        }
    }
    Ok(())
}

fn validate_next_offset(
    current_offset: u32,
    next: Option<UserSearchPageToken>,
) -> Result<(), UserSearchError> {
    if next.is_some_and(|next| next.offset() <= current_offset) {
        return Err(UserSearchError::PaginationContract {
            problem: "the API returned a repeated or non-progressing continuation offset",
        });
    }
    Ok(())
}

fn validate_page_len(actual: usize, requested: NonZeroU32) -> Result<(), UserSearchError> {
    if actual > requested.get() as usize {
        return Err(UserSearchError::PaginationContract {
            problem: "the API returned more users than requested",
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
    use std::str::FromStr;

    use time::OffsetDateTime;

    use super::*;
    use crate::application::ports::{
        CredentialDeleteOutcome, CredentialFailureKind, CredentialFormat, CredentialStoreFailure,
        LoadedCredential, UserSearchPage,
    };
    use crate::domain::{AccessToken, DeviceId, Username};

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
        pages: RefCell<VecDeque<Result<UserSearchPage, FakeApiError>>>,
        calls: Cell<usize>,
        sizes: RefCell<Vec<u32>>,
        offsets: RefCell<Vec<Option<u32>>>,
    }

    impl FakeApi {
        fn new(pages: Vec<Result<UserSearchPage, FakeApiError>>) -> Self {
            Self {
                pages: RefCell::new(pages.into()),
                calls: Cell::new(0),
                sizes: RefCell::new(Vec::new()),
                offsets: RefCell::new(Vec::new()),
            }
        }
    }

    impl UsersApi for FakeApi {
        type Error = FakeApiError;

        fn user_by_id<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            _user_id: &'a UserId,
        ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a {
            ready(Err(FakeApiError(ApiFailureKind::Internal)))
        }

        fn search_users<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            _query: &'a UserSearchQuery,
            page: UserSearchPageRequest,
        ) -> impl Future<Output = Result<UserSearchPage, Self::Error>> + Send + 'a {
            self.calls.set(self.calls.get() + 1);
            self.sizes.borrow_mut().push(page.page_size().get());
            self.offsets
                .borrow_mut()
                .push(page.token().map(UserSearchPageToken::offset));
            ready(
                self.pages
                    .borrow_mut()
                    .pop_front()
                    .unwrap_or(Err(FakeApiError(ApiFailureKind::Internal))),
            )
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn public_search_fetches_exactly_one_page_at_the_native_offset() -> TestResult {
        let api = FakeApi::new(vec![
            Ok(UserSearchPage::new(
                users(51, 2)?,
                Some(UserSearchPageToken::from_offset(52)),
            )),
            Err(FakeApiError(ApiFailureKind::Internal)),
        ]);
        let result = search(
            &FakeStore(true),
            &api,
            &UserSearchQuery::from_str("alice")?,
            Limit::try_from(2)?,
            Offset::new(50),
        )
        .await?;

        assert_eq!(result.users().len(), 2);
        assert_eq!(result.next_offset().map(Offset::get), Some(52));
        assert_eq!(api.calls.get(), 1);
        assert_eq!(api.sizes.borrow().as_slice(), &[2]);
        assert_eq!(api.offsets.borrow().as_slice(), &[Some(50)]);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn internal_exact_recipient_search_remains_bounded_and_exhaustive() -> TestResult {
        let api = FakeApi::new(vec![
            Ok(UserSearchPage::new(
                users(1, 50)?,
                Some(UserSearchPageToken::from_offset(50)),
            )),
            Ok(UserSearchPage::new(users(51, 3)?, None)),
        ]);
        let loaded = credential()?;
        let result = search_exhaustively_with_credential(
            &loaded.envelope,
            &api,
            &UserSearchQuery::from_str("@alice")?,
        )
        .await?;

        assert_eq!(result.users().len(), 53);
        assert_eq!(result.completion(), ExhaustiveSearchCompletion::Exhausted);
        assert_eq!(api.calls.get(), 2);
        assert_eq!(api.sizes.borrow().as_slice(), &[50, 50]);
        assert_eq!(api.offsets.borrow().as_slice(), &[None, Some(50)]);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn internal_search_fails_closed_at_four_pages_and_two_hundred_records() -> TestResult {
        let mut pages = Vec::new();
        for page in 0_u32..4 {
            pages.push(Ok(UserSearchPage::new(
                users(page * 50 + 1, 50)?,
                Some(UserSearchPageToken::from_offset((page + 1) * 50)),
            )));
        }
        let api = FakeApi::new(pages);
        let loaded = credential()?;
        let result = search_exhaustively_with_credential(
            &loaded.envelope,
            &api,
            &UserSearchQuery::from_str("alice")?,
        )
        .await?;

        assert_eq!(result.users().len(), 200);
        assert_eq!(
            result.completion(),
            ExhaustiveSearchCompletion::SafetyBoundReached
        );
        assert_eq!(api.calls.get(), 4);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn public_page_conflicts_oversize_and_nonprogress_fail_closed() -> TestResult {
        let cases = [
            (
                UserSearchPage::new(vec![user(1, "First")?, user(1, "Changed")?], None),
                2,
            ),
            (UserSearchPage::new(users(1, 2)?, None), 1),
            (
                UserSearchPage::new(Vec::new(), Some(UserSearchPageToken::from_offset(11))),
                1,
            ),
            (
                UserSearchPage::new(
                    vec![user(1, "User 1")?],
                    Some(UserSearchPageToken::from_offset(10)),
                ),
                1,
            ),
        ];
        for (page, limit) in cases {
            let api = FakeApi::new(vec![Ok(page)]);
            let result = search(
                &FakeStore(true),
                &api,
                &UserSearchQuery::from_str("alice")?,
                Limit::try_from(limit)?,
                Offset::new(10),
            )
            .await;
            assert!(matches!(
                result,
                Err(UserSearchError::PaginationContract { .. })
            ));
            assert_eq!(api.calls.get(), 1);
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn missing_credentials_make_no_api_request() -> TestResult {
        let api = FakeApi::new(Vec::new());
        let result = search(
            &FakeStore(false),
            &api,
            &UserSearchQuery::from_str("alice")?,
            Limit::try_from(10)?,
            Offset::default(),
        )
        .await;
        assert!(matches!(result, Err(UserSearchError::MissingCredential)));
        assert_eq!(api.calls.get(), 0);
        Ok(())
    }

    fn credential() -> Result<LoadedCredential, FakeCredentialError> {
        Ok(LoadedCredential {
            envelope: CredentialEnvelope::new(
                AccessToken::from_str("synthetic-token").map_err(|_| FakeCredentialError)?,
                DeviceId::from_str("synthetic-device").map_err(|_| FakeCredentialError)?,
                UserId::from_str("123").map_err(|_| FakeCredentialError)?,
                Username::from_bare("alice").map_err(|_| FakeCredentialError)?,
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
}

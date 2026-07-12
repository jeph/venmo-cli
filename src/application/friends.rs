use std::collections::HashMap;

use thiserror::Error;

use super::auth::OperationFailure;
use super::ports::{
    ApiFailure, ApiFailureKind, CredentialStore, FriendsApi, FriendsPageRequest, FriendsPageToken,
};
use super::read::ReadFailureKind;
use crate::domain::{Limit, Offset, User, UserId};

#[derive(Debug)]
pub struct FriendsResult {
    users: Vec<User>,
    next_offset: Option<Offset>,
}

impl FriendsResult {
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

#[derive(Debug, Error)]
pub enum FriendsError {
    #[error("no Venmo credential is stored; run `venmo auth login`")]
    MissingCredential,

    #[error("failed to read the OS credential entry: {source}")]
    CredentialLoad {
        #[source]
        source: OperationFailure,
    },

    #[error("failed to list Venmo friends: {source}")]
    Api {
        kind: ApiFailureKind,
        #[source]
        source: OperationFailure,
    },

    #[error("cannot use Venmo friend-list pagination because {problem}")]
    PaginationContract { problem: &'static str },
}

impl FriendsError {
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
    offset: Offset,
) -> Result<FriendsResult, FriendsError>
where
    S: CredentialStore,
    A: FriendsApi,
{
    let loaded = store
        .load()
        .map_err(|source| FriendsError::CredentialLoad {
            source: OperationFailure::new(source),
        })?
        .ok_or(FriendsError::MissingCredential)?;
    let current = (offset.get() != 0).then(|| FriendsPageToken::from_offset(offset.get()));
    let page = api
        .friends(
            loaded.envelope.access_token(),
            loaded.envelope.device_id(),
            loaded.envelope.user_id(),
            FriendsPageRequest::new(limit.as_nonzero(), current),
        )
        .await
        .map_err(|source| FriendsError::Api {
            kind: source.kind(),
            source: OperationFailure::new(source),
        })?;
    let (page_users, next_token) = page.into_parts();
    validate_page_len(page_users.len(), limit)?;
    validate_next_offset(offset, next_token)?;
    if page_users.is_empty() && next_token.is_some() {
        return Err(FriendsError::PaginationContract {
            problem: "the API returned an empty page with a continuation offset",
        });
    }
    let users = buffer_users(page_users)?;
    Ok(FriendsResult::new(
        users,
        next_token.map(|token| Offset::new(token.offset())),
    ))
}

fn buffer_users(page_users: Vec<User>) -> Result<Vec<User>, FriendsError> {
    let mut users = Vec::with_capacity(page_users.len());
    let mut seen = HashMap::<UserId, User>::with_capacity(page_users.len());
    for user in page_users {
        match seen.get(user.user_id()) {
            Some(existing) if existing == &user => {}
            Some(_) => {
                return Err(FriendsError::PaginationContract {
                    problem: "the API returned conflicting records for one user ID",
                });
            }
            None => {
                seen.insert(user.user_id().clone(), user.clone());
                users.push(user);
            }
        }
    }
    Ok(users)
}

fn validate_next_offset(
    current: Offset,
    next: Option<FriendsPageToken>,
) -> Result<(), FriendsError> {
    if next.is_some_and(|next| next.offset() <= current.get()) {
        return Err(FriendsError::PaginationContract {
            problem: "the API returned a repeated or non-progressing continuation offset",
        });
    }
    Ok(())
}

fn validate_page_len(actual: usize, requested: Limit) -> Result<(), FriendsError> {
    if actual > requested.get() as usize {
        return Err(FriendsError::PaginationContract {
            problem: "the API returned more friends than requested",
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
        FriendsPage, LoadedCredential,
    };
    use crate::domain::{AccessToken, CredentialEnvelope, DeviceId, Username};

    type TestResult = Result<(), Box<dyn Error>>;

    struct FakeStore {
        present: bool,
    }

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
            self.present.then(credential).transpose()
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
        pages: RefCell<VecDeque<Result<FriendsPage, FakeApiError>>>,
        calls: Cell<usize>,
        sizes: RefCell<Vec<u32>>,
        offsets: RefCell<Vec<Option<u32>>>,
    }

    impl FakeApi {
        fn new(pages: Vec<Result<FriendsPage, FakeApiError>>) -> Self {
            Self {
                pages: RefCell::new(pages.into()),
                calls: Cell::new(0),
                sizes: RefCell::new(Vec::new()),
                offsets: RefCell::new(Vec::new()),
            }
        }
    }

    impl FriendsApi for FakeApi {
        type Error = FakeApiError;

        fn friends<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            _current_user_id: &'a UserId,
            request: FriendsPageRequest,
        ) -> impl Future<Output = Result<FriendsPage, Self::Error>> + Send + 'a {
            self.calls.set(self.calls.get() + 1);
            self.sizes.borrow_mut().push(request.page_size().get());
            self.offsets
                .borrow_mut()
                .push(request.token().map(FriendsPageToken::offset));
            ready(
                self.pages
                    .borrow_mut()
                    .pop_front()
                    .unwrap_or(Err(FakeApiError(ApiFailureKind::Internal))),
            )
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn one_invocation_fetches_exactly_one_native_page() -> TestResult {
        let duplicate = user("11", "alice", "Alice")?;
        let api = FakeApi::new(vec![
            Ok(FriendsPage::new(
                vec![duplicate.clone(), duplicate],
                Some(FriendsPageToken::from_offset(12)),
            )),
            Err(FakeApiError(ApiFailureKind::Internal)),
        ]);

        let result = list(
            &FakeStore { present: true },
            &api,
            Limit::try_from(2)?,
            Offset::new(10),
        )
        .await?;

        assert_eq!(result.users().len(), 1);
        assert_eq!(result.next_offset().map(Offset::get), Some(12));
        assert_eq!(api.calls.get(), 1);
        assert_eq!(api.sizes.borrow().as_slice(), &[2]);
        assert_eq!(api.offsets.borrow().as_slice(), &[Some(10)]);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn conflicting_oversized_empty_and_nonprogressing_pages_fail_closed() -> TestResult {
        let cases = [
            (
                FriendsPage::new(
                    vec![user("1", "alice", "Alice")?, user("1", "alice", "Changed")?],
                    None,
                ),
                2,
            ),
            (
                FriendsPage::new(
                    vec![user("1", "alice", "Alice")?, user("2", "bob", "Bob")?],
                    None,
                ),
                1,
            ),
            (
                FriendsPage::new(Vec::new(), Some(FriendsPageToken::from_offset(11))),
                1,
            ),
            (
                FriendsPage::new(
                    vec![user("1", "alice", "Alice")?],
                    Some(FriendsPageToken::from_offset(10)),
                ),
                1,
            ),
        ];

        for (page, limit) in cases {
            let api = FakeApi::new(vec![Ok(page)]);
            let result = list(
                &FakeStore { present: true },
                &api,
                Limit::try_from(limit)?,
                Offset::new(10),
            )
            .await;
            assert!(matches!(
                result,
                Err(FriendsError::PaginationContract { .. })
            ));
            assert_eq!(api.calls.get(), 1);
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn missing_credentials_and_api_failures_preserve_categories() -> TestResult {
        let missing_api = FakeApi::new(Vec::new());
        let missing = list(
            &FakeStore { present: false },
            &missing_api,
            Limit::try_from(10)?,
            Offset::default(),
        )
        .await;
        assert!(matches!(missing, Err(FriendsError::MissingCredential)));
        assert_eq!(missing_api.calls.get(), 0);

        let timeout_api = FakeApi::new(vec![Err(FakeApiError(ApiFailureKind::Timeout))]);
        let timeout = list(
            &FakeStore { present: true },
            &timeout_api,
            Limit::try_from(10)?,
            Offset::default(),
        )
        .await;
        assert_eq!(
            timeout.as_ref().err().map(FriendsError::failure_kind),
            Some(ReadFailureKind::Api(ApiFailureKind::Timeout))
        );
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

    fn user(id: &str, username: &str, name: &str) -> Result<User, Box<dyn Error>> {
        Ok(User::new(
            UserId::from_str(id)?,
            Some(Username::from_bare(username)?),
            Some(name.to_owned()),
        ))
    }
}

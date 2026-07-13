use std::str::FromStr;

use thiserror::Error;

use super::auth::OperationFailure;
use super::ports::{ApiFailure, ApiFailureKind, CredentialStore, UsersApi};
use super::users::{
    ExhaustiveSearchCompletion, UserSearchError, UserSearchFailureKind,
    search_exhaustively_with_credential,
};
use crate::domain::{
    CredentialEnvelope, RecipientInput, ResolvedRecipient, User, UserSearchQuery, Username,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecipientResolutionFailureKind {
    Credential,
    Api(ApiFailureKind),
    NotFound,
    Ambiguous,
    IncompleteSearch,
    Internal,
}

#[derive(Debug, Error)]
pub enum RecipientResolutionError {
    #[error("no Venmo credential is stored; run `venmo auth login`")]
    MissingCredential,

    #[error("failed to read the OS credential entry: {source}")]
    CredentialLoad {
        #[source]
        source: OperationFailure,
    },

    #[error("failed to look up the Venmo recipient: {source}")]
    Api {
        kind: ApiFailureKind,
        #[source]
        source: OperationFailure,
    },

    #[error("cannot use the recipient lookup result because {problem}")]
    LookupContract { problem: &'static str },

    #[error("failed to complete the exact Venmo username search: {source}")]
    UsernameSearch {
        kind: RecipientResolutionFailureKind,
        #[source]
        source: UserSearchError,
    },

    #[error("no Venmo user has that exact username")]
    UsernameNotFound,

    #[error("multiple Venmo users matched that exact username")]
    AmbiguousUsername,

    #[error(
        "the bounded username search was incomplete, so the recipient cannot be resolved safely; use a numeric Venmo user ID"
    )]
    IncompleteUsernameSearch,

    #[error("recipient resolution failed because {problem}")]
    Internal { problem: &'static str },
}

impl RecipientResolutionError {
    #[must_use]
    pub const fn failure_kind(&self) -> RecipientResolutionFailureKind {
        match self {
            Self::MissingCredential | Self::CredentialLoad { .. } => {
                RecipientResolutionFailureKind::Credential
            }
            Self::Api { kind, .. } => RecipientResolutionFailureKind::Api(*kind),
            Self::LookupContract { .. } => {
                RecipientResolutionFailureKind::Api(ApiFailureKind::Contract)
            }
            Self::UsernameSearch { kind, .. } => *kind,
            Self::UsernameNotFound => RecipientResolutionFailureKind::NotFound,
            Self::AmbiguousUsername => RecipientResolutionFailureKind::Ambiguous,
            Self::IncompleteUsernameSearch => RecipientResolutionFailureKind::IncompleteSearch,
            Self::Internal { .. } => RecipientResolutionFailureKind::Internal,
        }
    }
}

pub async fn resolve<S, A>(
    store: &S,
    api: &A,
    recipient: &RecipientInput,
) -> Result<ResolvedRecipient, RecipientResolutionError>
where
    S: CredentialStore,
    A: UsersApi,
{
    let loaded = store
        .load()
        .map_err(|source| RecipientResolutionError::CredentialLoad {
            source: OperationFailure::new(source),
        })?
        .ok_or(RecipientResolutionError::MissingCredential)?;
    resolve_with_credential(&loaded.envelope, api, recipient).await
}

pub(crate) async fn resolve_with_credential<A>(
    credential: &CredentialEnvelope,
    api: &A,
    recipient: &RecipientInput,
) -> Result<ResolvedRecipient, RecipientResolutionError>
where
    A: UsersApi,
{
    let user = match recipient {
        RecipientInput::UserId(user_id) => {
            let user = api
                .user_by_id(credential.access_token(), credential.device_id(), user_id)
                .await
                .map_err(|source| RecipientResolutionError::Api {
                    kind: source.kind(),
                    source: OperationFailure::new(source),
                })?;
            if user.user_id() != user_id {
                return Err(RecipientResolutionError::LookupContract {
                    problem: "the API returned a different user ID",
                });
            }
            user
        }
        RecipientInput::Username(username) => {
            resolve_exact_username(credential, api, username).await?
        }
    };
    Ok(ResolvedRecipient::new(user))
}

async fn resolve_exact_username<A>(
    credential: &CredentialEnvelope,
    api: &A,
    username: &Username,
) -> Result<User, RecipientResolutionError>
where
    A: UsersApi,
{
    let query = UserSearchQuery::from_str(&username.to_string()).map_err(|_| {
        RecipientResolutionError::Internal {
            problem: "a validated username could not form an exact-search query",
        }
    })?;
    let result = search_exhaustively_with_credential(credential, api, &query)
        .await
        .map_err(|source| RecipientResolutionError::UsernameSearch {
            kind: map_search_failure(source.failure_kind()),
            source,
        })?;

    let mut matches = result.users().iter().filter(|user| {
        user.username()
            .is_some_and(|candidate| username_matches(candidate, username))
    });
    let first = matches.next().cloned();
    let second_exists = matches.next().is_some();

    if second_exists {
        return Err(RecipientResolutionError::AmbiguousUsername);
    }
    let Some(search_match) = first else {
        return if result.completion() == ExhaustiveSearchCompletion::Exhausted {
            Err(RecipientResolutionError::UsernameNotFound)
        } else {
            Err(RecipientResolutionError::IncompleteUsernameSearch)
        };
    };
    let expected_user_id = search_match.user_id().clone();
    let user = api
        .user_by_id(
            credential.access_token(),
            credential.device_id(),
            &expected_user_id,
        )
        .await
        .map_err(|source| RecipientResolutionError::Api {
            kind: source.kind(),
            source: OperationFailure::new(source),
        })?;
    if user.user_id() != &expected_user_id {
        return Err(RecipientResolutionError::LookupContract {
            problem: "the recipient detail response returned a different user ID",
        });
    }
    if !user
        .username()
        .is_some_and(|candidate| username_matches(candidate, username))
    {
        return Err(RecipientResolutionError::LookupContract {
            problem: "the recipient detail response returned a different username",
        });
    }
    Ok(user)
}

fn username_matches(candidate: &Username, requested: &Username) -> bool {
    candidate.as_str().to_lowercase() == requested.as_str().to_lowercase()
}

const fn map_search_failure(kind: UserSearchFailureKind) -> RecipientResolutionFailureKind {
    match kind {
        UserSearchFailureKind::Credential => RecipientResolutionFailureKind::Credential,
        UserSearchFailureKind::Api(kind) => RecipientResolutionFailureKind::Api(kind),
        UserSearchFailureKind::PaginationContract => {
            RecipientResolutionFailureKind::IncompleteSearch
        }
        UserSearchFailureKind::Internal => RecipientResolutionFailureKind::Internal,
    }
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
        LoadedCredential, UserSearchPage, UserSearchPageRequest, UserSearchPageToken,
    };
    use crate::domain::{AccessToken, DeviceId, UserId};

    type TestResult = Result<(), Box<dyn Error>>;

    struct FakeStore {
        available: bool,
    }

    impl CredentialStore for FakeStore {
        type Error = FakeCredentialError;

        fn load(&self) -> Result<Option<LoadedCredential>, Self::Error> {
            if self.available {
                credential().map(Some)
            } else {
                Ok(None)
            }
        }

        fn save(&self, _credential: &CredentialEnvelope) -> Result<(), Self::Error> {
            Err(FakeCredentialError)
        }

        fn delete(&self) -> Result<CredentialDeleteOutcome, Self::Error> {
            Err(FakeCredentialError)
        }
    }

    #[derive(Debug, Error)]
    #[error("synthetic credential failure")]
    struct FakeCredentialError;

    impl CredentialStoreFailure for FakeCredentialError {
        fn kind(&self) -> CredentialFailureKind {
            CredentialFailureKind::Internal
        }
    }

    struct FakeApi {
        lookups: RefCell<VecDeque<Result<User, FakeApiError>>>,
        pages: RefCell<VecDeque<Result<crate::application::ports::UserSearchPage, FakeApiError>>>,
        lookup_calls: Cell<u32>,
        search_calls: Cell<u32>,
    }

    impl UsersApi for FakeApi {
        type Error = FakeApiError;

        fn user_by_id<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            _user_id: &'a UserId,
        ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a {
            self.lookup_calls.set(self.lookup_calls.get() + 1);
            let result = match self.lookups.borrow_mut().pop_front() {
                Some(result) => result,
                None => Err(FakeApiError(ApiFailureKind::Internal)),
            };
            ready(result)
        }

        fn search_users<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            _query: &'a UserSearchQuery,
            _page: UserSearchPageRequest,
        ) -> impl Future<Output = Result<UserSearchPage, Self::Error>> + Send + 'a {
            self.search_calls.set(self.search_calls.get() + 1);
            let result = match self.pages.borrow_mut().pop_front() {
                Some(result) => result,
                None => Err(FakeApiError(ApiFailureKind::Internal)),
            };
            ready(result)
        }
    }

    #[derive(Clone, Debug, Error)]
    #[error("synthetic API failure")]
    struct FakeApiError(ApiFailureKind);

    impl ApiFailure for FakeApiError {
        fn kind(&self) -> ApiFailureKind {
            self.0
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn numeric_id_uses_direct_lookup_only() -> TestResult {
        let api = FakeApi {
            lookups: RefCell::new(VecDeque::from([Ok(user("123", Some("alice"))?)])),
            pages: RefCell::new(VecDeque::new()),
            lookup_calls: Cell::new(0),
            search_calls: Cell::new(0),
        };
        let input = RecipientInput::from_str("123")?;

        let resolved = resolve(&FakeStore { available: true }, &api, &input).await?;

        assert_eq!(resolved.user().user_id().as_str(), "123");
        assert_eq!(api.lookup_calls.get(), 1);
        assert_eq!(api.search_calls.get(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn username_requires_one_case_insensitive_exact_match_and_exhaustion() -> TestResult {
        let api = FakeApi {
            lookups: RefCell::new(VecDeque::from([Ok(user("123", Some("ALICE"))?)])),
            pages: RefCell::new(VecDeque::from([Ok(UserSearchPage::new(
                vec![user("123", Some("alice"))?, user("124", Some("alice2"))?],
                None,
            ))])),
            lookup_calls: Cell::new(0),
            search_calls: Cell::new(0),
        };
        let input = RecipientInput::from_str("@Alice")?;

        let resolved = resolve(&FakeStore { available: true }, &api, &input).await?;

        assert_eq!(resolved.user().user_id().as_str(), "123");
        assert_eq!(api.lookup_calls.get(), 1);
        assert_eq!(api.search_calls.get(), 1);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn username_requires_matching_authoritative_detail() -> TestResult {
        for detail in [user("999", Some("alice"))?, user("123", Some("other"))?] {
            let api = FakeApi {
                lookups: RefCell::new(VecDeque::from([Ok(detail)])),
                pages: RefCell::new(VecDeque::from([Ok(UserSearchPage::new(
                    vec![user("123", Some("alice"))?],
                    None,
                ))])),
                lookup_calls: Cell::new(0),
                search_calls: Cell::new(0),
            };
            let input = RecipientInput::from_str("@alice")?;

            let result = resolve(&FakeStore { available: true }, &api, &input).await;

            assert!(matches!(
                result,
                Err(RecipientResolutionError::LookupContract { .. })
            ));
            assert_eq!(api.lookup_calls.get(), 1);
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn exhausted_username_search_distinguishes_missing_and_ambiguous() -> TestResult {
        let missing_api = FakeApi {
            lookups: RefCell::new(VecDeque::new()),
            pages: RefCell::new(VecDeque::from([Ok(UserSearchPage::new(
                vec![user("123", Some("other"))?],
                None,
            ))])),
            lookup_calls: Cell::new(0),
            search_calls: Cell::new(0),
        };
        let ambiguous_api = FakeApi {
            lookups: RefCell::new(VecDeque::new()),
            pages: RefCell::new(VecDeque::from([Ok(UserSearchPage::new(
                vec![user("123", Some("alice"))?, user("124", Some("ALICE"))?],
                None,
            ))])),
            lookup_calls: Cell::new(0),
            search_calls: Cell::new(0),
        };
        let input = RecipientInput::from_str("@alice")?;

        let missing = resolve(&FakeStore { available: true }, &missing_api, &input).await;
        let ambiguous = resolve(&FakeStore { available: true }, &ambiguous_api, &input).await;

        assert!(matches!(
            missing,
            Err(RecipientResolutionError::UsernameNotFound)
        ));
        assert!(matches!(
            ambiguous,
            Err(RecipientResolutionError::AmbiguousUsername)
        ));
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn exact_match_is_verified_by_authoritative_detail_when_search_reaches_its_bound()
    -> TestResult {
        let mut pages = VecDeque::new();
        for page_index in 0_u32..4 {
            let start = page_index * 50 + 1;
            let mut page_users = Vec::with_capacity(50);
            for offset in 0_u32..50 {
                let id = start + offset;
                let username = if id == 1 {
                    "alice".to_owned()
                } else {
                    format!("user{id}")
                };
                page_users.push(user(&id.to_string(), Some(&username))?);
            }
            pages.push_back(Ok(UserSearchPage::new(
                page_users,
                Some(UserSearchPageToken::from_offset((page_index + 1) * 50)),
            )));
        }
        let api = FakeApi {
            lookups: RefCell::new(VecDeque::from([Ok(user("1", Some("Alice"))?)])),
            pages: RefCell::new(pages),
            lookup_calls: Cell::new(0),
            search_calls: Cell::new(0),
        };
        let input = RecipientInput::from_str("@alice")?;

        let result = resolve(&FakeStore { available: true }, &api, &input).await?;

        assert_eq!(result.user().user_id().as_str(), "1");
        assert_eq!(api.search_calls.get(), 4);
        assert_eq!(api.lookup_calls.get(), 1);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn missing_credential_and_api_failures_preserve_categories() -> TestResult {
        let api = FakeApi {
            lookups: RefCell::new(VecDeque::from([Err(FakeApiError(ApiFailureKind::Timeout))])),
            pages: RefCell::new(VecDeque::new()),
            lookup_calls: Cell::new(0),
            search_calls: Cell::new(0),
        };
        let input = RecipientInput::from_str("123")?;

        let missing = resolve(&FakeStore { available: false }, &api, &input).await;
        assert!(matches!(
            missing,
            Err(RecipientResolutionError::MissingCredential)
        ));
        assert_eq!(api.lookup_calls.get(), 0);

        let timeout = resolve(&FakeStore { available: true }, &api, &input).await;
        assert!(matches!(
            timeout,
            Err(RecipientResolutionError::Api {
                kind: ApiFailureKind::Timeout,
                ..
            })
        ));
        Ok(())
    }

    fn credential() -> Result<LoadedCredential, FakeCredentialError> {
        let access_token =
            AccessToken::from_str("synthetic-token").map_err(|_| FakeCredentialError)?;
        let device_id = DeviceId::from_str("synthetic-device").map_err(|_| FakeCredentialError)?;
        let user_id = UserId::from_str("999").map_err(|_| FakeCredentialError)?;
        let username = Username::from_bare("owner").map_err(|_| FakeCredentialError)?;
        Ok(LoadedCredential {
            envelope: CredentialEnvelope::new(
                access_token,
                device_id,
                user_id,
                username,
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
}

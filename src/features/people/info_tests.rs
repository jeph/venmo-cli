use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error;
use std::future::{Future, ready};
use std::rc::Rc;
use std::str::FromStr;

use time::OffsetDateTime;

use super::*;
use crate::features::people::{UserSearchPage, UserSearchPageRequest, UserSearchQuery};
use crate::shared::{
    AccessToken, ApiFailure, ApiFailureKind, CredentialCapability, CredentialEnvelope,
    CredentialFailureKind, CredentialFormat, CredentialStoreFailure, DeviceId, LoadedCredential,
    UserId, Username,
};

type TestResult = Result<(), Box<dyn Error>>;
type Transcript = Rc<RefCell<Vec<Call>>>;

#[derive(Clone, Debug, Eq, PartialEq)]
enum Call {
    ReadCredential,
    Search(UserSearchQuery, UserSearchPageRequest),
    Detail(UserId),
}

struct FakeReader {
    present: bool,
    transcript: Transcript,
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
        self.transcript.borrow_mut().push(Call::ReadCredential);
        if self.present {
            credential().map(Some)
        } else {
            Ok(None)
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
    search: RefCell<VecDeque<Result<UserSearchPage, FakeApiError>>>,
    detail: RefCell<VecDeque<Result<User, FakeApiError>>>,
    transcript: Transcript,
}

impl FakeApi {
    fn new(
        search: Vec<Result<UserSearchPage, FakeApiError>>,
        detail: Vec<Result<User, FakeApiError>>,
        transcript: Transcript,
    ) -> Self {
        Self {
            search: RefCell::new(search.into()),
            detail: RefCell::new(detail.into()),
            transcript,
        }
    }
}

impl UserSearchApi for FakeApi {
    type Error = FakeApiError;

    fn search_users<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        query: &'a UserSearchQuery,
        page: UserSearchPageRequest,
    ) -> impl Future<Output = Result<UserSearchPage, Self::Error>> + Send + 'a {
        self.transcript
            .borrow_mut()
            .push(Call::Search(query.clone(), page));
        ready(
            self.search
                .borrow_mut()
                .pop_front()
                .unwrap_or(Err(FakeApiError(ApiFailureKind::Internal))),
        )
    }
}

impl UserLookupApi for FakeApi {
    type Error = FakeApiError;

    fn user_by_id<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        user_id: &'a UserId,
    ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a {
        self.transcript
            .borrow_mut()
            .push(Call::Detail(user_id.clone()));
        ready(
            self.detail
                .borrow_mut()
                .pop_front()
                .unwrap_or(Err(FakeApiError(ApiFailureKind::Internal))),
        )
    }
}

#[tokio::test(flavor = "current_thread")]
async fn info_resolves_exact_username_before_authoritative_detail() -> TestResult {
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let search_user = user("456", "alice")?;
    let detail_user = user("456", "ALICE")?;
    let api = FakeApi::new(
        vec![Ok(UserSearchPage::new(vec![search_user], None))],
        vec![Ok(detail_user.clone())],
        Rc::clone(&transcript),
    );
    let reader = FakeReader {
        present: true,
        transcript: Rc::clone(&transcript),
    };

    let result = info(&reader, &api, &Username::from_str("@Alice")?).await?;

    assert_eq!(result.user(), &detail_user);
    assert_eq!(
        transcript.borrow().as_slice(),
        [
            Call::ReadCredential,
            Call::Search(
                UserSearchQuery::from_str("Alice")?,
                UserSearchPageRequest::new(crate::shared::Limit::try_from(50)?, Default::default()),
            ),
            Call::Detail(UserId::from_str("456")?),
        ]
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn info_missing_credential_stops_before_search() -> TestResult {
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let api = FakeApi::new(Vec::new(), Vec::new(), Rc::clone(&transcript));
    let reader = FakeReader {
        present: false,
        transcript: Rc::clone(&transcript),
    };

    let result = info(&reader, &api, &Username::from_str("alice")?).await;

    assert!(matches!(
        result,
        Err(UserInfoError::Credential(CredentialAccessError::Missing))
    ));
    assert_eq!(transcript.borrow().as_slice(), [Call::ReadCredential]);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn info_reports_no_exact_search_match_as_username_not_found() -> TestResult {
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let api = FakeApi::new(
        vec![Ok(UserSearchPage::new(vec![user("456", "other")?], None))],
        Vec::new(),
        Rc::clone(&transcript),
    );
    let reader = FakeReader {
        present: true,
        transcript: Rc::clone(&transcript),
    };

    let result = info(&reader, &api, &Username::from_str("alice")?).await;

    assert!(matches!(
        result,
        Err(UserInfoError::Lookup {
            source: UserLookupError::UsernameNotFound
        })
    ));
    assert_eq!(transcript.borrow().len(), 2);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn info_preserves_authoritative_detail_failures() -> TestResult {
    for (detail, expected_kind) in [
        (
            Ok(user("999", "alice")?),
            ApplicationFailureKind::ApiContract,
        ),
        (
            Err(FakeApiError(ApiFailureKind::Timeout)),
            ApplicationFailureKind::Api(ApiFailureKind::Timeout),
        ),
    ] {
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let api = FakeApi::new(
            vec![Ok(UserSearchPage::new(vec![user("456", "alice")?], None))],
            vec![detail],
            Rc::clone(&transcript),
        );
        let reader = FakeReader {
            present: true,
            transcript: Rc::clone(&transcript),
        };

        let error = info(&reader, &api, &Username::from_str("alice")?)
            .await
            .err()
            .ok_or("expected user-info failure")?;

        assert_eq!(error.failure_kind(), expected_kind);
        assert_eq!(transcript.borrow().len(), 3);
    }
    Ok(())
}

fn credential() -> Result<LoadedCredential, FakeCredentialError> {
    Ok(LoadedCredential {
        envelope: CredentialEnvelope::new(
            AccessToken::from_str("synthetic-token").map_err(|_| FakeCredentialError)?,
            DeviceId::from_str("synthetic-device").map_err(|_| FakeCredentialError)?,
            UserId::from_str("123").map_err(|_| FakeCredentialError)?,
            Username::from_bare("owner").map_err(|_| FakeCredentialError)?,
            Some("Synthetic owner".to_owned()),
            OffsetDateTime::UNIX_EPOCH,
        ),
        format: CredentialFormat::Version1,
    })
}

fn user(id: &str, username: &str) -> Result<User, Box<dyn Error>> {
    Ok(User::new(
        UserId::from_str(id)?,
        Some(Username::from_bare(username)?),
        Some("Synthetic user".to_owned()),
    ))
}

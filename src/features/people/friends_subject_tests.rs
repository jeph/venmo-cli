use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error;
use std::future::{Future, ready};
use std::io;
use std::rc::Rc;
use std::str::FromStr;

use time::OffsetDateTime;

use super::*;
use crate::features::people::{
    FriendsPage, UserSearchPage, UserSearchPageRequest, UserSearchQuery,
};
use crate::shared::{
    AccessToken, ApiFailure, ApiFailureKind, CredentialCapability, CredentialEnvelope,
    CredentialFailureKind, CredentialFormat, CredentialStoreFailure, DeviceId, LoadedCredential,
};

type TestResult = Result<(), Box<dyn Error>>;
type Transcript = Rc<RefCell<Vec<Call>>>;

#[derive(Clone, Debug, Eq, PartialEq)]
enum Call {
    Credential,
    Search {
        query: String,
        page: UserSearchPageRequest,
    },
    Detail(UserId),
    Friends {
        subject_user_id: UserId,
        page: FriendsPageRequest,
    },
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("synthetic failure")]
struct FakeError;

impl CredentialStoreFailure for FakeError {
    fn kind(&self) -> CredentialFailureKind {
        CredentialFailureKind::Internal
    }
}

impl ApiFailure for FakeError {
    fn kind(&self) -> ApiFailureKind {
        ApiFailureKind::Rejected
    }
}

struct Reader(Transcript);

impl CredentialCapability for Reader {
    type Error = FakeError;
}

impl CredentialReader for Reader {
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        self.0.borrow_mut().push(Call::Credential);
        Ok(Some(LoadedCredential {
            envelope: CredentialEnvelope::new(
                AccessToken::from_str("synthetic-token").map_err(|_| FakeError)?,
                DeviceId::from_str("synthetic-device").map_err(|_| FakeError)?,
                UserId::from_str("1000").map_err(|_| FakeError)?,
                Username::from_bare("owner").map_err(|_| FakeError)?,
                Some("Owner".to_owned()),
                OffsetDateTime::UNIX_EPOCH,
            ),
            format: CredentialFormat::Version1,
        }))
    }
}

struct Api {
    transcript: Transcript,
    search: RefCell<VecDeque<Result<UserSearchPage, FakeError>>>,
    detail: RefCell<VecDeque<Result<User, FakeError>>>,
    friends: RefCell<VecDeque<Result<FriendsPage, FakeError>>>,
}

impl Api {
    fn new(
        transcript: Transcript,
        search: Vec<Result<UserSearchPage, FakeError>>,
        detail: Vec<Result<User, FakeError>>,
        friends: Vec<Result<FriendsPage, FakeError>>,
    ) -> Self {
        Self {
            transcript,
            search: RefCell::new(search.into()),
            detail: RefCell::new(detail.into()),
            friends: RefCell::new(friends.into()),
        }
    }
}

impl UserSearchApi for Api {
    type Error = FakeError;

    fn search_users<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        query: &'a UserSearchQuery,
        page: UserSearchPageRequest,
    ) -> impl Future<Output = Result<UserSearchPage, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::Search {
            query: query.as_str().to_owned(),
            page,
        });
        ready(
            self.search
                .borrow_mut()
                .pop_front()
                .unwrap_or(Err(FakeError)),
        )
    }
}

impl UserLookupApi for Api {
    type Error = FakeError;

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
                .unwrap_or(Err(FakeError)),
        )
    }
}

impl FriendsApi for Api {
    type Error = FakeError;

    fn friends<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        subject_user_id: &'a UserId,
        page: FriendsPageRequest,
    ) -> impl Future<Output = Result<FriendsPage, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::Friends {
            subject_user_id: subject_user_id.clone(),
            page,
        });
        ready(
            self.friends
                .borrow_mut()
                .pop_front()
                .unwrap_or(Err(FakeError)),
        )
    }
}

#[tokio::test(flavor = "current_thread")]
async fn matching_active_username_uses_stored_id_without_lookup() -> TestResult {
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = Reader(Rc::clone(&transcript));
    let api = Api::new(
        Rc::clone(&transcript),
        Vec::new(),
        Vec::new(),
        vec![Ok(FriendsPage::new(Vec::new(), None))],
    );
    let limit = Limit::try_from(7)?;
    let offset = Offset::new(3);

    let result =
        list_for_user(&reader, &api, &Username::from_bare("OwNeR")?, limit, offset).await?;

    assert_eq!(result.subject(), None);
    assert_eq!(
        transcript.borrow().as_slice(),
        [
            Call::Credential,
            Call::Friends {
                subject_user_id: UserId::from_str("1000")?,
                page: FriendsPageRequest::new(limit, offset),
            },
        ]
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn other_personal_user_resolves_authoritatively_before_listing_target() -> TestResult {
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = Reader(Rc::clone(&transcript));
    let search_user = user("2000", "alice")?;
    let detail_user =
        user("2000", "Alice")?.with_financial_attributes(UserProfileKind::Personal, true);
    let friend = user("3000", "bob")?;
    let api = Api::new(
        Rc::clone(&transcript),
        vec![Ok(UserSearchPage::new(vec![search_user], None))],
        vec![Ok(detail_user)],
        vec![Ok(FriendsPage::new(
            vec![friend.clone()],
            Some(Offset::new(6)),
        ))],
    );
    let limit = Limit::try_from(5)?;
    let offset = Offset::new(1);

    let result =
        list_for_user(&reader, &api, &Username::from_bare("alice")?, limit, offset).await?;

    assert_eq!(result.users(), [friend]);
    assert_eq!(result.next_offset(), Some(Offset::new(6)));
    assert_eq!(
        result.subject(),
        Some(&FriendsSubject::new(
            UserId::from_str("2000")?,
            Username::from_bare("Alice")?,
        ))
    );
    assert_eq!(
        transcript.borrow().as_slice(),
        [
            Call::Credential,
            Call::Search {
                query: "alice".to_owned(),
                page: UserSearchPageRequest::new(Limit::try_from(50)?, Offset::default()),
            },
            Call::Detail(UserId::from_str("2000")?),
            Call::Friends {
                subject_user_id: UserId::from_str("2000")?,
                page: FriendsPageRequest::new(limit, offset),
            },
        ]
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn unsupported_or_incomplete_profile_stops_before_friend_list() -> TestResult {
    for (detail, expected) in [
        (
            user("2000", "alice")?.with_financial_attributes(UserProfileKind::Business, true),
            ApplicationFailureKind::Usage,
        ),
        (user("2000", "alice")?, ApplicationFailureKind::Usage),
    ] {
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = Reader(Rc::clone(&transcript));
        let api = Api::new(
            Rc::clone(&transcript),
            vec![Ok(UserSearchPage::new(vec![user("2000", "alice")?], None))],
            vec![Ok(detail)],
            vec![Ok(FriendsPage::new(Vec::new(), None))],
        );

        let result = list_for_user(
            &reader,
            &api,
            &Username::from_bare("alice")?,
            Limit::try_from(10)?,
            Offset::default(),
        )
        .await;
        let Err(error) = result else {
            return Err(io::Error::other("profile should be rejected").into());
        };

        assert_eq!(error.failure_kind(), expected);
        assert!(
            !transcript
                .borrow()
                .iter()
                .any(|call| matches!(call, Call::Friends { .. }))
        );
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn lookup_api_failure_preserves_api_category_and_stops_before_friend_list() -> TestResult {
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = Reader(Rc::clone(&transcript));
    let api = Api::new(
        Rc::clone(&transcript),
        vec![Err(FakeError)],
        Vec::new(),
        vec![Ok(FriendsPage::new(Vec::new(), None))],
    );

    let result = list_for_user(
        &reader,
        &api,
        &Username::from_bare("alice")?,
        Limit::try_from(10)?,
        Offset::default(),
    )
    .await;
    let Err(error) = result else {
        return Err(io::Error::other("lookup should fail").into());
    };

    assert_eq!(
        error.failure_kind(),
        ApplicationFailureKind::Api(ApiFailureKind::Rejected)
    );
    assert!(
        !transcript
            .borrow()
            .iter()
            .any(|call| matches!(call, Call::Friends { .. }))
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn authoritative_subject_without_username_is_a_contract_failure_before_friends() -> TestResult
{
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = Reader(Rc::clone(&transcript));
    let api = Api::new(
        Rc::clone(&transcript),
        vec![Ok(UserSearchPage::new(vec![user("2000", "alice")?], None))],
        vec![Ok(User::new(
            UserId::from_str("2000")?,
            None,
            Some("Alice".to_owned()),
        )
        .with_financial_attributes(UserProfileKind::Personal, true))],
        vec![Ok(FriendsPage::new(Vec::new(), None))],
    );

    let result = list_for_user(
        &reader,
        &api,
        &Username::from_bare("alice")?,
        Limit::try_from(10)?,
        Offset::default(),
    )
    .await;
    let Err(error) = result else {
        return Err(io::Error::other("missing authoritative username should fail").into());
    };

    assert_eq!(error.failure_kind(), ApplicationFailureKind::ApiContract);
    assert!(
        !transcript
            .borrow()
            .iter()
            .any(|call| matches!(call, Call::Friends { .. }))
    );
    Ok(())
}

fn user(id: &str, username: &str) -> Result<User, Box<dyn Error>> {
    Ok(User::new(
        UserId::from_str(id)?,
        Some(Username::from_bare(username)?),
        Some("Synthetic User".to_owned()),
    ))
}

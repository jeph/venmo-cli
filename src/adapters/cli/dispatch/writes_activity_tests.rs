use std::cell::RefCell;
use std::error::Error;
use std::future::{Future, pending, ready};
use std::rc::Rc;
use std::str::FromStr;

use time::OffsetDateTime;

use super::*;
use crate::adapters::cli::output::TimestampFormatter;
use crate::features::activity::{
    ActivityAction, ActivityCommentId, ActivityCommentMessage, ActivityDetail, ActivityLikeState,
    ActivitySocial, ActivitySocialCollection, ActivityStatus,
};
use crate::features::auth::{PromptAvailability, PromptError};
use crate::features::people::User;
use crate::shared::{
    AccessToken, ApiFailureKind, CredentialCapability, CredentialEnvelope, CredentialFailureKind,
    CredentialFormat, CredentialStoreFailure, DeviceId, LoadedCredential, Money, UserId, Username,
};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;
type Fixture = (Rc<RefCell<Vec<Call>>>, Reader, Api, Prompt);

#[derive(Clone, Debug, Eq, PartialEq)]
enum Call {
    ReadCredential,
    Detail,
    Like,
    RemoveComment,
    Confirm,
    InstallInterruption,
}

#[derive(Debug, thiserror::Error)]
#[error("synthetic credential failure")]
struct FakeCredentialError;

impl CredentialStoreFailure for FakeCredentialError {
    fn kind(&self) -> CredentialFailureKind {
        CredentialFailureKind::Internal
    }
}

struct Reader {
    calls: Rc<RefCell<Vec<Call>>>,
}

impl CredentialCapability for Reader {
    type Error = FakeCredentialError;
}

impl CredentialReader for Reader {
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        self.calls.borrow_mut().push(Call::ReadCredential);
        Ok(Some(LoadedCredential {
            envelope: CredentialEnvelope::new(
                AccessToken::from_str("synthetic-token").map_err(|_| FakeCredentialError)?,
                DeviceId::from_str("synthetic-device").map_err(|_| FakeCredentialError)?,
                UserId::from_str("123").map_err(|_| FakeCredentialError)?,
                Username::from_bare("owner").map_err(|_| FakeCredentialError)?,
                None,
                OffsetDateTime::UNIX_EPOCH,
            ),
            format: CredentialFormat::Version1,
        }))
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("synthetic activity API failure")]
struct FakeApiError;

impl ApiFailure for FakeApiError {
    fn kind(&self) -> ApiFailureKind {
        ApiFailureKind::Internal
    }
}

struct Api {
    calls: Rc<RefCell<Vec<Call>>>,
    before: ActivityDetail,
    after: ActivityDetail,
}

impl ActivityDetailApi for Api {
    type Error = FakeApiError;

    fn activity_by_id<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        _current_user_id: &'a UserId,
        _activity_id: &'a ActivityId,
    ) -> impl Future<Output = Result<ActivityDetail, Self::Error>> + Send + 'a {
        self.calls.borrow_mut().push(Call::Detail);
        ready(Ok(self.before.clone()))
    }
}

impl ActivitySocialMutationApi for Api {
    type Error = FakeApiError;

    fn like_activity<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        _current_user_id: &'a UserId,
        _activity_id: &'a ActivityId,
    ) -> impl Future<Output = Result<ActivityDetail, Self::Error>> + Send + 'a {
        self.calls.borrow_mut().push(Call::Like);
        ready(Ok(self.after.clone()))
    }

    fn unlike_activity<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        _current_user_id: &'a UserId,
        _activity_id: &'a ActivityId,
    ) -> impl Future<Output = Result<ActivityDetail, Self::Error>> + Send + 'a {
        ready(Err(FakeApiError))
    }

    fn add_activity_comment<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        _current_user_id: &'a UserId,
        _activity_id: &'a ActivityId,
        _message: &'a ActivityCommentMessage,
    ) -> impl Future<Output = Result<ActivityDetail, Self::Error>> + Send + 'a {
        ready(Err(FakeApiError))
    }
}

impl ActivityCommentRemovalApi for Api {
    type Error = FakeApiError;

    fn remove_activity_comment<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        _comment_id: &'a ActivityCommentId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
        self.calls.borrow_mut().push(Call::RemoveComment);
        ready(Ok(()))
    }
}

struct Prompt {
    calls: Rc<RefCell<Vec<Call>>>,
}

impl PromptAvailability for Prompt {
    fn can_prompt(&self) -> bool {
        true
    }
}

impl DefaultNoConfirmation for Prompt {
    fn confirm_default_no(&self, _prompt: &str) -> Result<bool, PromptError> {
        self.calls.borrow_mut().push(Call::Confirm);
        Ok(true)
    }
}

fn detail(liked: bool) -> TestResult<ActivityDetail> {
    let owner = User::new(
        UserId::from_str("123")?,
        Some(Username::from_bare("owner")?),
        None,
    );
    let other = User::new(
        UserId::from_str("456")?,
        Some(Username::from_bare("other")?),
        None,
    );
    let likes = if liked {
        vec![owner.clone()]
    } else {
        Vec::new()
    };
    Ok(ActivityDetail::payment(
        ActivityId::from_str("story-1")?,
        OffsetDateTime::UNIX_EPOCH,
        ActivityAction::from_str("pay")?,
        owner,
        other,
        Some(Money::from_cents(100)?),
        Some(ActivityStatus::from_str("settled")?),
        Some("Synthetic activity".to_owned()),
        Some("private".to_owned()),
    )
    .with_social(ActivitySocial::new(
        Some(ActivitySocialCollection::new(u64::from(liked), likes, true)),
        Some(ActivitySocialCollection::new(0, Vec::new(), true)),
    )))
}

fn fixture() -> TestResult<Fixture> {
    let calls = Rc::new(RefCell::new(Vec::new()));
    Ok((
        Rc::clone(&calls),
        Reader {
            calls: Rc::clone(&calls),
        },
        Api {
            calls: Rc::clone(&calls),
            before: detail(false)?,
            after: detail(true)?,
        },
        Prompt {
            calls: Rc::clone(&calls),
        },
    ))
}

#[tokio::test(flavor = "current_thread")]
async fn activity_social_dry_run_stops_after_preflight_without_prompt_or_interruption() -> TestResult
{
    let (calls, reader, api, prompt) = fixture()?;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut output = human_output(
        crate::adapters::cli::CommandId::ActivityLike,
        &mut stdout,
        &mut stderr,
    );
    let timestamps = TimestampFormatter::for_time_zone(jiff::tz::TimeZone::UTC);
    let activity_id = ActivityId::from_str("story-1")?;

    run_activity_social_with(
        &activity_id,
        ActivitySocialIntent::Like,
        false,
        true,
        &reader,
        &api,
        &prompt,
        &timestamps,
        &mut output,
        || {
            calls.borrow_mut().push(Call::InstallInterruption);
            Ok(pending())
        },
    )
    .await?;

    assert_eq!(*calls.borrow(), vec![Call::ReadCredential, Call::Detail]);
    assert_eq!(
        String::from_utf8(stdout)?,
        "Dry run complete; no changes made.\n"
    );
    let details = String::from_utf8(stderr)?;
    assert!(details.contains("Action: like activity"));
    assert!(details.contains("Current like state: not liked"));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn activity_social_yes_path_skips_prompt_and_executes_one_state_write() -> TestResult {
    let (calls, reader, api, prompt) = fixture()?;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut output = human_output(
        crate::adapters::cli::CommandId::ActivityLike,
        &mut stdout,
        &mut stderr,
    );
    let timestamps = TimestampFormatter::for_time_zone(jiff::tz::TimeZone::UTC);
    let activity_id = ActivityId::from_str("story-1")?;

    run_activity_social_with(
        &activity_id,
        ActivitySocialIntent::Like,
        true,
        false,
        &reader,
        &api,
        &prompt,
        &timestamps,
        &mut output,
        || {
            calls.borrow_mut().push(Call::InstallInterruption);
            Ok(pending())
        },
    )
    .await?;

    assert_eq!(
        *calls.borrow(),
        vec![
            Call::ReadCredential,
            Call::Detail,
            Call::InstallInterruption,
            Call::Like,
        ]
    );
    assert!(!calls.borrow().contains(&Call::Confirm));
    assert!(String::from_utf8(stdout)?.contains("Result: Activity liked"));
    assert_eq!(
        detail(true)?.social().like_state(&UserId::from_str("123")?),
        ActivityLikeState::Liked
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn comment_remove_dry_run_stops_after_credential_and_discloses_no_preflight() -> TestResult {
    let (calls, reader, api, prompt) = fixture()?;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut output = human_output(
        crate::adapters::cli::CommandId::ActivityCommentsRemove,
        &mut stdout,
        &mut stderr,
    );

    run_activity_comment_remove_with(
        ActivityCommentId::from_str("comment-1")?,
        false,
        true,
        &reader,
        &api,
        &prompt,
        &mut output,
        || {
            calls.borrow_mut().push(Call::InstallInterruption);
            Ok(pending())
        },
    )
    .await?;

    assert_eq!(*calls.borrow(), vec![Call::ReadCredential]);
    assert_eq!(
        String::from_utf8(stdout)?,
        "Dry run complete; no changes made.\n"
    );
    let details = String::from_utf8(stderr)?;
    assert!(details.contains("Comment ID: comment-1"));
    assert!(details.contains("parent activity, authorship, and comment text are not validated"));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn comment_remove_yes_path_sends_exactly_one_write_without_detail_read() -> TestResult {
    let (calls, reader, api, prompt) = fixture()?;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut output = human_output(
        crate::adapters::cli::CommandId::ActivityCommentsRemove,
        &mut stdout,
        &mut stderr,
    );

    run_activity_comment_remove_with(
        ActivityCommentId::from_str("comment-1")?,
        true,
        false,
        &reader,
        &api,
        &prompt,
        &mut output,
        || {
            calls.borrow_mut().push(Call::InstallInterruption);
            Ok(pending())
        },
    )
    .await?;

    assert_eq!(
        *calls.borrow(),
        vec![
            Call::ReadCredential,
            Call::InstallInterruption,
            Call::RemoveComment,
        ]
    );
    assert!(!calls.borrow().contains(&Call::Confirm));
    assert!(String::from_utf8(stdout)?.contains("Result: Comment removal accepted by Venmo"));
    Ok(())
}

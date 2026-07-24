use std::cell::RefCell;
use std::error::Error;
use std::future::{Future, pending, ready};
use std::rc::Rc;
use std::str::FromStr;

use time::OffsetDateTime;

use super::*;
use crate::adapters::cli::output::TimestampFormatter;
use crate::features::activity::reactions::ActivityReactionMutationError;
use crate::features::activity::{
    ActivityAction, ActivityCommentId, ActivityCommentMessage, ActivityDetail, ActivityReaction,
    ActivityReactionEmoji, ActivityReactionMutationApi, ActivityReactionTarget, ActivityReactions,
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
    AddReaction(ActivityReactionTarget),
    RemoveReaction(ActivityReactionTarget),
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

impl ActivityReactionMutationApi for Api {
    type Error = FakeApiError;

    fn add_activity_reaction<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        _current_user_id: &'a UserId,
        _activity_id: &'a ActivityId,
        target: &'a ActivityReactionTarget,
    ) -> impl Future<Output = Result<ActivityDetail, Self::Error>> + Send + 'a {
        self.calls
            .borrow_mut()
            .push(Call::AddReaction(target.clone()));
        ready(Ok(self.after.clone()))
    }

    fn remove_activity_reaction<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        _current_user_id: &'a UserId,
        _activity_id: &'a ActivityId,
        target: &'a ActivityReactionTarget,
    ) -> impl Future<Output = Result<ActivityDetail, Self::Error>> + Send + 'a {
        self.calls
            .borrow_mut()
            .push(Call::RemoveReaction(target.clone()));
        ready(Ok(self.after.clone()))
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

fn reaction_detail(reacted: bool) -> TestResult<ActivityDetail> {
    let reactions = if reacted {
        vec![ActivityReaction::new(
            ActivityReactionEmoji::from_str("🔥")?,
            1,
            true,
        )]
    } else {
        Vec::new()
    };
    Ok(detail(false)?.with_social(
        ActivitySocial::new(
            Some(ActivitySocialCollection::new(0, Vec::new(), true)),
            Some(ActivitySocialCollection::new(0, Vec::new(), true)),
        )
        .with_reactions(Some(ActivityReactions::try_new(reactions)?)),
    ))
}

fn reaction_fixture(before: bool, after: bool) -> TestResult<Fixture> {
    let calls = Rc::new(RefCell::new(Vec::new()));
    Ok((
        Rc::clone(&calls),
        Reader {
            calls: Rc::clone(&calls),
        },
        Api {
            calls: Rc::clone(&calls),
            before: reaction_detail(before)?,
            after: reaction_detail(after)?,
        },
        Prompt {
            calls: Rc::clone(&calls),
        },
    ))
}

fn like_reaction_detail(reacted: bool) -> TestResult<ActivityDetail> {
    let detail = detail(reacted)?;
    let likes = detail.social().likes().cloned();
    let comments = detail.social().comments().cloned();
    let reactions = vec![ActivityReaction::new(
        ActivityReactionEmoji::from_str("❤️")?,
        u64::from(reacted),
        reacted,
    )];
    Ok(detail.with_social(
        ActivitySocial::new(likes, comments)
            .with_reactions(Some(ActivityReactions::try_new(reactions)?)),
    ))
}

fn like_reaction_fixture(before: bool, after: bool) -> TestResult<Fixture> {
    let calls = Rc::new(RefCell::new(Vec::new()));
    Ok((
        Rc::clone(&calls),
        Reader {
            calls: Rc::clone(&calls),
        },
        Api {
            calls: Rc::clone(&calls),
            before: like_reaction_detail(before)?,
            after: like_reaction_detail(after)?,
        },
        Prompt {
            calls: Rc::clone(&calls),
        },
    ))
}

#[tokio::test(flavor = "current_thread")]
async fn activity_reaction_dry_run_stops_after_authoritative_preflight() -> TestResult {
    let (calls, reader, api, prompt) = reaction_fixture(false, true)?;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut output = human_output(
        crate::adapters::cli::CommandId::ActivityReactionsAdd,
        &mut stdout,
        &mut stderr,
    );
    let timestamps = TimestampFormatter::for_time_zone(jiff::tz::TimeZone::UTC);
    let activity_id = ActivityId::from_str("story-1")?;

    run_activity_reaction_with(
        ActivityReactionCommand::new(
            &activity_id,
            ActivityReactionIntent::Add(ActivityReactionEmoji::from_str("🔥")?.into()),
            false,
            true,
        ),
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
    assert!(details.contains("Action: add activity reaction"));
    assert!(details.contains("Reaction: 🔥"));
    assert!(!details.contains("Current reaction state:"));
    assert!(!details.contains("Automatic retries:"));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn activity_reaction_yes_path_executes_one_state_write_and_reports_reconciliation()
-> TestResult {
    let (calls, reader, api, prompt) = reaction_fixture(false, true)?;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut output = human_output(
        crate::adapters::cli::CommandId::ActivityReactionsAdd,
        &mut stdout,
        &mut stderr,
    );
    let timestamps = TimestampFormatter::for_time_zone(jiff::tz::TimeZone::UTC);
    let activity_id = ActivityId::from_str("story-1")?;
    let emoji = ActivityReactionEmoji::from_str("🔥")?;

    run_activity_reaction_with(
        ActivityReactionCommand::new(
            &activity_id,
            ActivityReactionIntent::Add(emoji.clone().into()),
            true,
            false,
        ),
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
            Call::AddReaction(emoji.into()),
        ]
    );
    assert!(!calls.borrow().contains(&Call::Confirm));
    assert_eq!(String::from_utf8(stdout)?, "Reaction added\n");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn activity_like_target_uses_the_unified_reaction_pipeline() -> TestResult {
    let (calls, reader, api, prompt) = like_reaction_fixture(false, true)?;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut output = output::OutputSession::new(
        crate::adapters::cli::OutputFormat::Json,
        crate::adapters::cli::CommandId::ActivityReactionsAdd,
        false,
        &mut stdout,
        &mut stderr,
    );
    let timestamps = TimestampFormatter::for_time_zone(jiff::tz::TimeZone::UTC);
    let activity_id = ActivityId::from_str("story-1")?;

    run_activity_reaction_with(
        ActivityReactionCommand::new(
            &activity_id,
            ActivityReactionIntent::Add(ActivityReactionTarget::Like),
            true,
            false,
        ),
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
            Call::AddReaction(ActivityReactionTarget::Like),
        ]
    );
    assert!(!calls.borrow().contains(&Call::Confirm));
    assert!(stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&stdout)?;
    assert_eq!(json["command"], "activity.reactions.add");
    assert_eq!(json["data"]["plan"]["target"]["kind"], "like");
    assert_eq!(json["data"]["plan"]["target"]["value"], "like");
    assert_eq!(json["data"]["result"]["target"]["kind"], "like");
    assert_eq!(json["data"]["result"]["target"]["value"], "like");
    assert_eq!(json["data"]["result"]["state"], "present");
    assert_eq!(json["data"]["result"]["count"], 1);
    assert_eq!(json["data"]["result"]["reacted_by_current_user"], true);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn activity_reaction_json_dry_run_and_yes_outputs_are_explicit() -> TestResult {
    let timestamps = TimestampFormatter::for_time_zone(jiff::tz::TimeZone::UTC);
    let activity_id = ActivityId::from_str("story-1")?;
    let emoji = ActivityReactionEmoji::from_str("🔥")?;

    let (dry_calls, dry_reader, dry_api, dry_prompt) = reaction_fixture(false, true)?;
    let mut dry_stdout = Vec::new();
    let mut dry_stderr = Vec::new();
    let mut dry_output = output::OutputSession::new(
        crate::adapters::cli::OutputFormat::Json,
        crate::adapters::cli::CommandId::ActivityReactionsAdd,
        false,
        &mut dry_stdout,
        &mut dry_stderr,
    );

    run_activity_reaction_with(
        ActivityReactionCommand::new(
            &activity_id,
            ActivityReactionIntent::Add(emoji.clone().into()),
            false,
            true,
        ),
        &dry_reader,
        &dry_api,
        &dry_prompt,
        &timestamps,
        &mut dry_output,
        || {
            dry_calls.borrow_mut().push(Call::InstallInterruption);
            Ok(pending())
        },
    )
    .await?;

    assert_eq!(
        *dry_calls.borrow(),
        vec![Call::ReadCredential, Call::Detail]
    );
    assert!(dry_stderr.is_empty());
    let dry_json: serde_json::Value = serde_json::from_slice(&dry_stdout)?;
    assert_eq!(dry_json["command"], "activity.reactions.add");
    assert_eq!(dry_json["ok"], true);
    assert_eq!(dry_json["data"]["outcome"], "dry_run");
    assert_eq!(dry_json["data"]["performed"], false);
    assert_eq!(dry_json["data"]["plan"]["action"], "add_reaction");
    assert_eq!(dry_json["data"]["plan"]["target"]["kind"], "unicode_emoji");
    assert_eq!(dry_json["data"]["plan"]["target"]["value"], "🔥");
    assert_eq!(dry_json["data"]["plan"]["previous_state"], "absent");
    assert_eq!(dry_json["data"]["plan"]["automatic_retries"], false);
    assert_eq!(dry_json["data"]["result"], serde_json::Value::Null);

    let (yes_calls, yes_reader, yes_api, yes_prompt) = reaction_fixture(false, true)?;
    let mut yes_stdout = Vec::new();
    let mut yes_stderr = Vec::new();
    let mut yes_output = output::OutputSession::new(
        crate::adapters::cli::OutputFormat::Json,
        crate::adapters::cli::CommandId::ActivityReactionsAdd,
        false,
        &mut yes_stdout,
        &mut yes_stderr,
    );

    run_activity_reaction_with(
        ActivityReactionCommand::new(
            &activity_id,
            ActivityReactionIntent::Add(emoji.clone().into()),
            true,
            false,
        ),
        &yes_reader,
        &yes_api,
        &yes_prompt,
        &timestamps,
        &mut yes_output,
        || {
            yes_calls.borrow_mut().push(Call::InstallInterruption);
            Ok(pending())
        },
    )
    .await?;

    assert_eq!(
        *yes_calls.borrow(),
        vec![
            Call::ReadCredential,
            Call::Detail,
            Call::InstallInterruption,
            Call::AddReaction(emoji.into()),
        ]
    );
    assert!(!yes_calls.borrow().contains(&Call::Confirm));
    assert!(yes_stderr.is_empty());
    let yes_json: serde_json::Value = serde_json::from_slice(&yes_stdout)?;
    assert_eq!(yes_json["command"], "activity.reactions.add");
    assert_eq!(yes_json["ok"], true);
    assert_eq!(yes_json["data"]["outcome"], "completed");
    assert_eq!(yes_json["data"]["performed"], true);
    assert_eq!(yes_json["data"]["result"]["activity_id"], "story-1");
    assert_eq!(
        yes_json["data"]["result"]["target"]["kind"],
        "unicode_emoji"
    );
    assert_eq!(yes_json["data"]["result"]["target"]["value"], "🔥");
    assert_eq!(yes_json["data"]["result"]["state"], "present");
    assert_eq!(yes_json["data"]["result"]["count"], 1);
    assert_eq!(yes_json["data"]["result"]["reacted_by_current_user"], true);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn activity_reaction_remove_json_uses_observed_absence() -> TestResult {
    let (calls, reader, api, prompt) = reaction_fixture(true, false)?;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut output = output::OutputSession::new(
        crate::adapters::cli::OutputFormat::Json,
        crate::adapters::cli::CommandId::ActivityReactionsRemove,
        false,
        &mut stdout,
        &mut stderr,
    );
    let timestamps = TimestampFormatter::for_time_zone(jiff::tz::TimeZone::UTC);
    let activity_id = ActivityId::from_str("story-1")?;
    let emoji = ActivityReactionEmoji::from_str("🔥")?;

    run_activity_reaction_with(
        ActivityReactionCommand::new(
            &activity_id,
            ActivityReactionIntent::Remove(emoji.clone().into()),
            true,
            false,
        ),
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
            Call::RemoveReaction(emoji.into()),
        ]
    );
    assert!(!calls.borrow().contains(&Call::Confirm));
    assert!(stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&stdout)?;
    assert_eq!(json["command"], "activity.reactions.remove");
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["outcome"], "completed");
    assert_eq!(json["data"]["performed"], true);
    assert_eq!(json["data"]["result"]["activity_id"], "story-1");
    assert_eq!(json["data"]["result"]["target"]["kind"], "unicode_emoji");
    assert_eq!(json["data"]["result"]["target"]["value"], "🔥");
    assert_eq!(json["data"]["result"]["state"], "absent");
    assert_eq!(json["data"]["result"]["count"], serde_json::Value::Null);
    assert_eq!(json["data"]["result"]["reacted_by_current_user"], false);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn activity_reaction_feature_rejects_unproven_reconciled_state() -> TestResult {
    let (calls, reader, api, prompt) = reaction_fixture(false, false)?;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut output = human_output(
        crate::adapters::cli::CommandId::ActivityReactionsAdd,
        &mut stdout,
        &mut stderr,
    );
    let timestamps = TimestampFormatter::for_time_zone(jiff::tz::TimeZone::UTC);
    let activity_id = ActivityId::from_str("story-1")?;
    let emoji = ActivityReactionEmoji::from_str("🔥")?;

    let result = run_activity_reaction_with(
        ActivityReactionCommand::new(
            &activity_id,
            ActivityReactionIntent::Add(emoji.clone().into()),
            true,
            false,
        ),
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
    .await;

    assert!(matches!(
        result,
        Err(AppError::ActivityReactionMutation {
            source: ActivityReactionMutationError::OutcomeUnknown { .. }
        })
    ));
    assert_eq!(
        *calls.borrow(),
        vec![
            Call::ReadCredential,
            Call::Detail,
            Call::InstallInterruption,
            Call::AddReaction(emoji.into()),
        ]
    );
    assert!(stdout.is_empty());
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

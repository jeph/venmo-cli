use std::error::Error;
use std::str::FromStr;

use super::super::{write_activity_info, write_activity_list, write_request_info, write_requests};
use crate::features::activity::{
    Activity, ActivityAction, ActivityBeforeId, ActivityComment, ActivityCommentId,
    ActivityCounterparty, ActivityDetail, ActivityDirection, ActivityFeedKind, ActivityId,
    ActivityInfoResult, ActivityListResult, ActivitySocial, ActivitySocialCollection,
    ActivityStatus, ActivitySubject,
};
use crate::features::people::User;
use crate::features::requests::list::RequestsResult;
use crate::features::requests::{
    RequestAction, RequestDirection, RequestDirectionFilter, RequestId, RequestInfoResult,
    RequestRecord, RequestStatus, RequestsBefore,
};
use crate::shared::{Money, UserId, Username};

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn request_info_output_preserves_open_request_fields_and_sanitizes_text() -> TestResult {
    let request = RequestRecord::new(
        RequestId::from_str("request-1")?,
        RequestAction::Charge,
        RequestDirection::Outgoing,
        User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("bob")?),
            Some("Bob\n\u{1b}[31mExample".to_owned()),
        ),
        Money::from_str("12.34")?,
        Some("note\u{202e}text\nline".to_owned()),
        Some(time::OffsetDateTime::UNIX_EPOCH),
        RequestStatus::from_str("held")?,
    )
    .with_audience(Some("private\u{7}".to_owned()));
    let result = RequestInfoResult::new(request);
    let mut output = Vec::new();
    let timestamps = super::local_timestamps();

    write_request_info(&mut output, &result, &timestamps)?;
    let output = String::from_utf8(output)?;

    insta::assert_snapshot!("request_info", output);
    assert!(!output.contains('\u{1b}'));
    assert!(!output.contains('\u{202e}'));
    assert!(!output.contains('\u{7}'));
    Ok(())
}

#[test]
fn activity_list_and_info_render_authoritative_status_as_data() -> TestResult {
    let activity = synthetic_activity("failed", "note\n\u{1b}[31mline")?;
    let transfer = Activity::new(
        ActivityId::from_str("story-transfer")?,
        time::OffsetDateTime::UNIX_EPOCH,
        ActivityAction::from_str("transfer:standard")?,
        ActivityDirection::Outgoing,
        ActivityCounterparty::external(
            "Bank\nname".to_owned(),
            "bank".to_owned(),
            Some("1234".to_owned()),
        ),
        Some(Money::from_str("12.34")?),
        ActivityStatus::from_str("issued")?,
        None,
        Some("private".to_owned()),
    );
    let list = ActivityListResult::new(
        vec![activity.clone(), transfer],
        Some(ActivityBeforeId::from_str("story-next")?),
    );
    let info = ActivityInfoResult::new(ActivityDetail::payment(
        activity.id().clone(),
        activity.occurred_at(),
        activity.action().clone(),
        synthetic_user("123", "alice")?,
        synthetic_user("456", "bob")?,
        activity.amount(),
        activity.status().clone(),
        activity.note().map(str::to_owned),
        activity.audience().map(str::to_owned),
    ));
    let mut list_stdout = Vec::new();
    let mut list_stderr = Vec::new();
    let mut info_stdout = Vec::new();
    let timestamps = super::local_timestamps();

    write_activity_list(&mut list_stdout, &mut list_stderr, &list, &timestamps)?;
    write_activity_info(&mut info_stdout, &info, &timestamps)?;
    let list_stdout = String::from_utf8(list_stdout)?;
    let info_stdout = String::from_utf8(info_stdout)?;

    assert_eq!(
        String::from_utf8(list_stderr)?,
        "Next before-id: story-next\n"
    );
    insta::assert_snapshot!("activity_list", list_stdout);
    assert!(!list_stdout.contains('\u{1b}'));
    insta::assert_snapshot!("activity_info", info_stdout);
    assert!(!info_stdout.contains('\u{1b}'));
    Ok(())
}

#[test]
fn other_user_activity_output_names_its_subject_and_perspective() -> TestResult {
    let subject = ActivitySubject::new(
        UserId::from_str("456")?,
        Username::from_bare("alice")?,
        ActivityFeedKind::OtherPersonalUser,
    );
    let activity = synthetic_activity("settled", "visible note")?;
    let result = ActivityListResult::for_subject(subject, vec![activity], None);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    write_activity_list(
        &mut stdout,
        &mut stderr,
        &result,
        &super::local_timestamps(),
    )?;

    insta::assert_snapshot!("other_user_activity", String::from_utf8(stdout)?);
    assert!(stderr.is_empty());
    Ok(())
}

#[test]
fn external_activity_detail_omits_an_unavailable_amount() -> TestResult {
    let info = ActivityInfoResult::new(ActivityDetail::payment(
        ActivityId::from_str("story-1")?,
        time::OffsetDateTime::UNIX_EPOCH,
        ActivityAction::from_str("pay")?,
        synthetic_user("456", "alice")?,
        synthetic_user("789", "bob")?,
        None,
        ActivityStatus::from_str("settled")?,
        Some("visible note".to_owned()),
        Some("public".to_owned()),
    ));
    let mut stdout = Vec::new();

    write_activity_info(&mut stdout, &info, &super::local_timestamps())?;
    let stdout = String::from_utf8(stdout)?;

    assert!(!stdout.contains("Amount:"));
    insta::assert_snapshot!("external_activity_info_without_amount", stdout);
    Ok(())
}

#[test]
fn activity_info_renders_partial_embedded_social_data_and_sanitizes_it() -> TestResult {
    let liker = User::new(
        UserId::from_str("789")?,
        Some(Username::from_bare("liker")?),
        Some("Liker\nName".to_owned()),
    );
    let comment = ActivityComment::new(
        ActivityCommentId::from_str("comment-1")?,
        synthetic_user("123", "alice")?,
        "message\n\u{1b}[31mline".to_owned(),
        time::OffsetDateTime::UNIX_EPOCH,
    );
    let detail = ActivityDetail::payment(
        ActivityId::from_str("story-1")?,
        time::OffsetDateTime::UNIX_EPOCH,
        ActivityAction::from_str("pay")?,
        synthetic_user("123", "alice")?,
        synthetic_user("456", "bob")?,
        Some(Money::from_str("1.25")?),
        ActivityStatus::from_str("settled")?,
        Some("visible note".to_owned()),
        Some("friends".to_owned()),
    )
    .with_social(ActivitySocial::new(
        Some(ActivitySocialCollection::new(2, vec![liker], false)),
        Some(ActivitySocialCollection::new(2, vec![comment], false)),
    ));
    let mut stdout = Vec::new();

    write_activity_info(
        &mut stdout,
        &ActivityInfoResult::new(detail),
        &super::local_timestamps(),
    )?;
    let stdout = String::from_utf8(stdout)?;

    insta::assert_snapshot!("activity_info_partial_social", stdout);
    assert!(!stdout.contains('\u{1b}'));
    Ok(())
}

#[test]
fn pending_request_output_preserves_ids_and_sanitizes_notes() -> TestResult {
    let requests = RequestsResult::new(
        vec![RequestRecord::new(
            RequestId::from_str("request-1")?,
            RequestAction::Charge,
            RequestDirection::Incoming,
            synthetic_user("456", "bob")?,
            Money::from_str("0.01")?,
            Some("request\u{202e}note".to_owned()),
            Some(time::OffsetDateTime::UNIX_EPOCH),
            RequestStatus::from_str("pending")?,
        )],
        RequestDirectionFilter::Incoming,
        Some(RequestsBefore::from_str("request-next")?),
    );
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let timestamps = super::local_timestamps();

    write_requests(&mut stdout, &mut stderr, &requests, &timestamps)?;
    let stdout = String::from_utf8(stdout)?;
    let stderr = String::from_utf8(stderr)?;

    insta::assert_snapshot!("pending_requests", stdout);
    assert!(!stdout.contains('\u{202e}'));
    assert_eq!(stderr, "Next before: request-next\n");
    Ok(())
}

#[test]
fn empty_activity_and_request_lists_have_exact_messages_and_no_continuations() -> TestResult {
    let activity = ActivityListResult::new(Vec::new(), None);
    let other_activity = ActivityListResult::for_subject(
        ActivitySubject::new(
            UserId::from_str("456")?,
            Username::from_bare("alice")?,
            ActivityFeedKind::OtherPersonalUser,
        ),
        Vec::new(),
        None,
    );
    let requests = RequestsResult::new(Vec::new(), RequestDirectionFilter::All, None);
    let mut activity_stdout = Vec::new();
    let mut activity_stderr = Vec::new();
    let mut other_activity_stdout = Vec::new();
    let mut other_activity_stderr = Vec::new();
    let mut requests_stdout = Vec::new();
    let mut requests_stderr = Vec::new();
    let timestamps = super::local_timestamps();

    write_activity_list(
        &mut activity_stdout,
        &mut activity_stderr,
        &activity,
        &timestamps,
    )?;
    write_requests(
        &mut requests_stdout,
        &mut requests_stderr,
        &requests,
        &timestamps,
    )?;
    write_activity_list(
        &mut other_activity_stdout,
        &mut other_activity_stderr,
        &other_activity,
        &timestamps,
    )?;

    assert_eq!(activity_stdout, b"No activity found.\n");
    assert_eq!(activity_stderr, b"");
    assert_eq!(
        other_activity_stdout,
        b"Activity for @alice\nDirection and counterparty are relative to @alice.\nNo visible activity found for @alice.\n"
    );
    assert_eq!(other_activity_stderr, b"");
    assert_eq!(requests_stdout, b"No pending requests found.\n");
    assert_eq!(requests_stderr, b"");
    Ok(())
}

fn synthetic_activity(status: &str, note: &str) -> Result<Activity, Box<dyn Error>> {
    Ok(Activity::new(
        ActivityId::from_str("story-1")?,
        time::OffsetDateTime::UNIX_EPOCH,
        ActivityAction::from_str("pay")?,
        ActivityDirection::Outgoing,
        ActivityCounterparty::user(synthetic_user("456", "bob")?),
        Some(Money::from_str("1.25")?),
        ActivityStatus::from_str(status)?,
        Some(note.to_owned()),
        Some("private".to_owned()),
    ))
}

fn synthetic_user(id: &str, username: &str) -> Result<User, Box<dyn Error>> {
    Ok(User::new(
        UserId::from_str(id)?,
        Some(Username::from_bare(username)?),
        Some("Synthetic User".to_owned()),
    ))
}

use std::error::Error;
use std::str::FromStr;

use super::super::{write_activity_list, write_activity_show, write_requests};
use crate::features::activity::{
    Activity, ActivityAction, ActivityBeforeId, ActivityCounterparty, ActivityDirection,
    ActivityId, ActivityListResult, ActivityShowResult, ActivityStatus,
};
use crate::features::people::User;
use crate::features::requests::list::RequestsResult;
use crate::features::requests::{
    RequestAction, RequestDirection, RequestDirectionFilter, RequestId, RequestRecord,
    RequestStatus, RequestsBefore,
};
use crate::shared::{Money, UserId, Username};

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn activity_list_and_show_render_authoritative_status_as_data() -> TestResult {
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
        Money::from_str("12.34")?,
        ActivityStatus::from_str("issued")?,
        None,
        Some("private".to_owned()),
    );
    let list = ActivityListResult::new(
        vec![activity.clone(), transfer],
        Some(ActivityBeforeId::from_str("story-next")?),
    );
    let show = ActivityShowResult::new(activity);
    let mut list_stdout = Vec::new();
    let mut list_stderr = Vec::new();
    let mut show_stdout = Vec::new();

    write_activity_list(&mut list_stdout, &mut list_stderr, &list)?;
    write_activity_show(&mut show_stdout, &show)?;
    let list_stdout = String::from_utf8(list_stdout)?;
    let show_stdout = String::from_utf8(show_stdout)?;

    assert_eq!(
        String::from_utf8(list_stderr)?,
        "Next before-id: story-next\n"
    );
    insta::assert_snapshot!("activity_list", list_stdout);
    assert!(!list_stdout.contains('\u{1b}'));
    insta::assert_snapshot!("activity_show", show_stdout);
    assert!(!show_stdout.contains('\u{1b}'));
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

    write_requests(&mut stdout, &mut stderr, &requests)?;
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
    let requests = RequestsResult::new(Vec::new(), RequestDirectionFilter::All, None);
    let mut activity_stdout = Vec::new();
    let mut activity_stderr = Vec::new();
    let mut requests_stdout = Vec::new();
    let mut requests_stderr = Vec::new();

    write_activity_list(&mut activity_stdout, &mut activity_stderr, &activity)?;
    write_requests(&mut requests_stdout, &mut requests_stderr, &requests)?;

    assert_eq!(activity_stdout, b"No activity found.\n");
    assert_eq!(activity_stderr, b"");
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
        Money::from_str("1.25")?,
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

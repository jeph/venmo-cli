use std::io::{self, Write};

use tabled::builder::Builder;

use crate::features::activity::comment_remove::{
    ActivityCommentRemovalResult, PreparedActivityCommentRemoval,
};
use crate::features::activity::social::{
    ActivitySocialAction, ActivitySocialMutationResult, PreparedActivitySocialMutation,
};
use crate::features::activity::{
    ActivityBeforeId, ActivityComment, ActivityCommentListResult, ActivityCounterparty,
    ActivityInfoResult, ActivityLikeState, ActivityListResult,
};

use super::TimestampFormatter;
use super::shared::{sanitize_terminal_text, user_label, write_table};

const ACTIVITY_INFO_COMMENT_LIMIT: usize = 5;

pub(crate) fn write_activity_list<W: Write, E: Write>(
    stdout: &mut W,
    stderr: &mut E,
    result: &ActivityListResult,
    timestamps: &TimestampFormatter,
) -> io::Result<()> {
    if let Some(subject) = result.subject() {
        writeln!(stdout, "Activity for {}", subject.username())?;
        writeln!(
            stdout,
            "Direction and counterparty are relative to {}.",
            subject.username()
        )?;
    }
    if result.activities().is_empty() {
        if let Some(subject) = result.subject() {
            writeln!(
                stdout,
                "No visible activity found for {}.",
                subject.username()
            )?;
        } else {
            writeln!(stdout, "No activity found.")?;
        }
    } else {
        let mut builder = Builder::default();
        let other_user_feed = result.subject().is_some();
        if other_user_feed {
            builder.push_record([
                "Id",
                "Time",
                "Action",
                "Direction",
                "Counterparty",
                "Status",
                "Note",
            ]);
        } else {
            builder.push_record([
                "Id",
                "Time",
                "Action",
                "Direction",
                "Counterparty",
                "Amount",
                "Status",
                "Note",
            ]);
        }
        for activity in result.activities() {
            let timestamp = timestamps.format(activity.occurred_at())?;
            let common = [
                sanitize_terminal_text(activity.id().as_str()),
                timestamp,
                sanitize_terminal_text(activity.action().as_str()),
                activity.direction().to_string(),
                sanitize_terminal_text(&activity_counterparty_label(activity.counterparty())),
            ];
            if other_user_feed {
                builder.push_record(common.into_iter().chain([
                    sanitize_terminal_text(activity.status().as_str()),
                    sanitize_terminal_text(activity.note().unwrap_or("")),
                ]));
            } else {
                builder.push_record(
                    common.into_iter().chain([
                        activity
                            .amount()
                            .map(|amount| format!("${amount}"))
                            .unwrap_or_default(),
                        sanitize_terminal_text(activity.status().as_str()),
                        sanitize_terminal_text(activity.note().unwrap_or("")),
                    ]),
                );
            }
        }
        write_table(stdout, builder)?;
    }
    write_next_before_id(stderr, result.next_before_id())
}

pub(crate) fn write_activity_info<W: Write>(
    writer: &mut W,
    result: &ActivityInfoResult,
    timestamps: &TimestampFormatter,
) -> io::Result<()> {
    let activity = result.activity();
    let timestamp = timestamps.format(activity.occurred_at())?;
    writeln!(
        writer,
        "Activity ID: {}",
        sanitize_terminal_text(activity.id().as_str())
    )?;
    writeln!(writer, "Time: {timestamp}")?;
    writeln!(
        writer,
        "Action: {}",
        sanitize_terminal_text(activity.action().as_str())
    )?;
    if let Some((actor, target)) = activity.parties().payment_parties() {
        writeln!(
            writer,
            "Actor: {}",
            sanitize_terminal_text(&user_label(actor))
        )?;
        writeln!(
            writer,
            "Target: {}",
            sanitize_terminal_text(&user_label(target))
        )?;
    } else if let Some((direction, counterparty)) = activity.parties().relative_parts() {
        writeln!(writer, "Direction: {direction}")?;
        writeln!(
            writer,
            "Counterparty: {}",
            sanitize_terminal_text(&activity_counterparty_label(counterparty))
        )?;
    }
    if let Some(amount) = activity.amount() {
        writeln!(writer, "Amount: ${amount}")?;
    }
    writeln!(
        writer,
        "Status: {}",
        sanitize_terminal_text(activity.status().as_str())
    )?;
    writeln!(
        writer,
        "Note: {}",
        sanitize_terminal_text(activity.note().unwrap_or(""))
    )?;
    writeln!(
        writer,
        "Audience: {}",
        sanitize_terminal_text(activity.audience().unwrap_or("(not provided)"))
    )?;
    match activity.social().likes() {
        Some(likes) => {
            writeln!(writer, "Likes: {}", likes.count())?;
            writeln!(
                writer,
                "Likers shown: {} ({})",
                likes.items().len(),
                if likes.is_complete() {
                    "complete"
                } else {
                    "partial"
                }
            )?;
            for liker in likes.items() {
                writeln!(
                    writer,
                    "  Liker: {}",
                    sanitize_terminal_text(&user_label(liker))
                )?;
            }
        }
        None => writeln!(writer, "Likes: (not provided)")?,
    }
    match activity.social().comments() {
        Some(comments) => {
            let displayed_count = comments.items().len().min(ACTIVITY_INFO_COMMENT_LIMIT);
            writeln!(writer, "Comments: {}", comments.count())?;
            writeln!(
                writer,
                "Comments shown: {} ({})",
                displayed_count,
                if !comments.is_complete() {
                    "partial source"
                } else if comments.items().len() > displayed_count {
                    "display limited"
                } else {
                    "complete"
                }
            )?;
            for comment in comments.items().iter().take(displayed_count) {
                write_activity_comment(writer, comment, timestamps)?;
            }
            if comments.is_complete() && comments.items().len() > ACTIVITY_INFO_COMMENT_LIMIT {
                writeln!(
                    writer,
                    "More comments: venmo activity comments list {} --offset {}",
                    sanitize_terminal_text(activity.id().as_str()),
                    ACTIVITY_INFO_COMMENT_LIMIT
                )?;
            } else if !comments.is_complete()
                && comments.count() > u64::try_from(comments.items().len()).unwrap_or(u64::MAX)
            {
                writeln!(
                    writer,
                    "Additional comments were not included in Venmo's activity response."
                )?;
            }
        }
        None => writeln!(writer, "Comments: (not provided)")?,
    }
    Ok(())
}

pub(crate) fn write_activity_comments<W: Write, E: Write>(
    stdout: &mut W,
    stderr: &mut E,
    result: &ActivityCommentListResult,
    timestamps: &TimestampFormatter,
) -> io::Result<()> {
    writeln!(
        stdout,
        "Comments for activity {}",
        sanitize_terminal_text(result.activity_id().as_str())
    )?;
    writeln!(stdout, "Comments available: {}", result.total_count())?;
    writeln!(stdout, "Offset: {}", result.offset())?;
    writeln!(stdout, "Comments shown: {}", result.comments().len())?;
    if result.comments().is_empty() {
        writeln!(stdout, "No comments found at this offset.")?;
    } else {
        for comment in result.comments() {
            write_activity_comment(stdout, comment, timestamps)?;
        }
    }
    if let Some(next_offset) = result.next_offset() {
        writeln!(stderr, "Next offset: {next_offset}")?;
    }
    Ok(())
}

fn write_activity_comment(
    writer: &mut impl Write,
    comment: &ActivityComment,
    timestamps: &TimestampFormatter,
) -> io::Result<()> {
    writeln!(
        writer,
        "  Comment ID: {}",
        sanitize_terminal_text(comment.id().as_str())
    )?;
    writeln!(
        writer,
        "    Author: {}",
        sanitize_terminal_text(&user_label(comment.author()))
    )?;
    writeln!(
        writer,
        "    Time: {}",
        timestamps.format(comment.created_at())?
    )?;
    writeln!(
        writer,
        "    Message: {}",
        sanitize_terminal_text(comment.message())
    )
}

pub(crate) fn write_activity_social_details(
    writer: &mut impl Write,
    prepared: &PreparedActivitySocialMutation,
    timestamps: &TimestampFormatter,
) -> io::Result<()> {
    let plan = prepared.plan();
    let activity = plan.activity();
    writeln!(writer, "Activity social details:")?;
    writeln!(writer, "  Action: {}", plan.action().label())?;
    writeln!(
        writer,
        "  Activity ID: {}",
        sanitize_terminal_text(activity.id().as_str())
    )?;
    writeln!(
        writer,
        "  Activity time: {}",
        timestamps.format(activity.occurred_at())?
    )?;
    writeln!(
        writer,
        "  Activity note: {}",
        sanitize_terminal_text(activity.note().unwrap_or(""))
    )?;
    writeln!(
        writer,
        "  Audience: {}",
        sanitize_terminal_text(activity.audience().unwrap_or("(not provided)"))
    )?;
    writeln!(
        writer,
        "  Current like state: {}",
        like_state_label(plan.previous_like_state())
    )?;
    match plan.action() {
        ActivitySocialAction::AddComment(message) => writeln!(
            writer,
            "  Comment: {}",
            sanitize_terminal_text(message.as_str())
        )?,
        ActivitySocialAction::Like | ActivitySocialAction::Unlike => {}
    }
    writeln!(writer, "  Automatic retries: disabled")
}

pub(crate) fn write_activity_comment_removal_details(
    writer: &mut impl Write,
    prepared: &PreparedActivityCommentRemoval,
) -> io::Result<()> {
    writeln!(writer, "Activity comment removal details:")?;
    writeln!(writer, "  Action: remove activity comment")?;
    writeln!(
        writer,
        "  Comment ID: {}",
        sanitize_terminal_text(prepared.plan().comment_id().as_str())
    )?;
    writeln!(
        writer,
        "  Preflight: parent activity, authorship, and comment text are not validated"
    )?;
    writeln!(
        writer,
        "  Reconciliation: verify independently with `activity info <ACTIVITY_ID>`"
    )?;
    writeln!(writer, "  Automatic retries: disabled")
}

pub(crate) fn write_activity_comment_removal_result(
    writer: &mut impl Write,
    result: &ActivityCommentRemovalResult,
) -> io::Result<()> {
    writeln!(writer, "Action: remove activity comment")?;
    writeln!(
        writer,
        "Comment ID: {}",
        sanitize_terminal_text(result.plan().comment_id().as_str())
    )?;
    writeln!(writer, "Result: Comment removal accepted by Venmo")?;
    writeln!(
        writer,
        "Verification: check the parent activity independently before retrying"
    )
}

pub(crate) fn write_activity_social_result(
    writer: &mut impl Write,
    result: &ActivitySocialMutationResult,
) -> io::Result<()> {
    writeln!(writer, "Action: {}", result.plan().action().label())?;
    writeln!(
        writer,
        "Activity ID: {}",
        sanitize_terminal_text(result.activity().id().as_str())
    )?;
    writeln!(writer, "Result: {}", result.plan().action().result_label())?;
    if let Some(likes) = result.activity().social().likes() {
        writeln!(writer, "Likes: {}", likes.count())?;
    }
    if let Some(comments) = result.activity().social().comments() {
        writeln!(writer, "Comments: {}", comments.count())?;
    }
    Ok(())
}

const fn like_state_label(state: ActivityLikeState) -> &'static str {
    match state {
        ActivityLikeState::Liked => "liked",
        ActivityLikeState::NotLiked => "not liked",
        ActivityLikeState::Unknown => "unknown (embedded liker data is incomplete or unavailable)",
    }
}

fn activity_counterparty_label(counterparty: &ActivityCounterparty) -> String {
    match counterparty {
        ActivityCounterparty::User(user) => user_label(user),
        ActivityCounterparty::External {
            name,
            kind,
            last_four: Some(last_four),
        } => format!("{name} ({kind} ••••{last_four})"),
        ActivityCounterparty::External {
            name,
            kind,
            last_four: None,
        } => format!("{name} ({kind})"),
    }
}

fn write_next_before_id(
    writer: &mut impl Write,
    before_id: Option<&ActivityBeforeId>,
) -> io::Result<()> {
    match before_id {
        Some(before_id) => writeln!(writer, "Next before-id: {}", before_id.as_str()),
        None => Ok(()),
    }
}

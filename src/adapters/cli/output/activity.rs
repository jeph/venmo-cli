use std::io::{self, Write};

use tabled::builder::Builder;

use crate::features::activity::comment_remove::{
    ActivityCommentRemovalPlan, ActivityCommentRemovalResult,
};
use crate::features::activity::reactions::{
    ActivityReactionListResult, ActivityReactionMutationResult, ActivityReactionPlan,
};
use crate::features::activity::social::{
    ActivitySocialAction, ActivitySocialMutationResult, ActivitySocialPlan,
};
use crate::features::activity::{
    ActivityBeforeId, ActivityComment, ActivityCommentListResult, ActivityCounterparty,
    ActivityInfoResult, ActivityLikeState, ActivityListResult,
};

use super::super::response::HumanSource;
use super::TimestampFormatter;
use super::shared::{sanitize_terminal_text, user_label, write_table};

const ACTIVITY_INFO_COMMENT_LIMIT: usize = 5;

pub(crate) fn write_activity_list<W: Write, E: Write>(
    stdout: &mut W,
    stderr: &mut E,
    response: &impl HumanSource<ActivityListResult>,
    timestamps: &TimestampFormatter,
) -> io::Result<()> {
    let result = response.human_source();
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
                    sanitize_terminal_text(activity.status().map_or("", |status| status.as_str())),
                    sanitize_terminal_text(activity.note().unwrap_or("")),
                ]));
            } else {
                builder.push_record(
                    common.into_iter().chain([
                        activity
                            .amount()
                            .map(|amount| format!("${amount}"))
                            .unwrap_or_default(),
                        sanitize_terminal_text(
                            activity.status().map_or("", |status| status.as_str()),
                        ),
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
    response: &impl HumanSource<ActivityInfoResult>,
    timestamps: &TimestampFormatter,
) -> io::Result<()> {
    let result = response.human_source();
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
    } else if let Some((account, direction, counterparty)) = activity.parties().account_parts() {
        writeln!(
            writer,
            "Account: {}",
            sanitize_terminal_text(&user_label(account))
        )?;
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
    if let Some(status) = activity.status() {
        writeln!(
            writer,
            "Status: {}",
            sanitize_terminal_text(status.as_str())
        )?;
    }
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
            for liker in likes.items() {
                writeln!(
                    writer,
                    "  Liker: {}",
                    sanitize_terminal_text(&user_label(liker))
                )?;
            }
            if !likes.is_complete() {
                writeln!(
                    writer,
                    "Additional likers were not included in Venmo's activity response."
                )?;
            }
        }
        None => writeln!(writer, "Likes: (not provided)")?,
    }
    match activity.social().comments() {
        Some(comments) => {
            let displayed_count = comments.items().len().min(ACTIVITY_INFO_COMMENT_LIMIT);
            writeln!(writer, "Comments: {}", comments.count())?;
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
            } else if !comments.is_complete() {
                writeln!(
                    writer,
                    "Additional comments were not included in Venmo's activity response."
                )?;
            }
        }
        None => writeln!(writer, "Comments: (not provided)")?,
    }
    match activity.social().reactions() {
        Some(reactions) => writeln!(writer, "Reactions: {}", reactions.total_count())?,
        None => writeln!(writer, "Reactions: (not provided)")?,
    }
    Ok(())
}

pub(crate) fn write_activity_reactions<W: Write>(
    writer: &mut W,
    response: &impl HumanSource<ActivityReactionListResult>,
) -> io::Result<()> {
    let result = response.human_source();
    writeln!(
        writer,
        "Reactions for activity {}",
        sanitize_terminal_text(result.activity_id().as_str())
    )?;
    writeln!(
        writer,
        "Total reactions: {}",
        result.reactions().total_count()
    )?;
    if result.reactions().items().is_empty() {
        return writeln!(writer, "No reactions found.");
    }
    let mut builder = Builder::default();
    builder.push_record(["Emoji", "Count", "You reacted"]);
    for reaction in result.reactions().items() {
        builder.push_record([
            sanitize_terminal_text(reaction.emoji().as_str()),
            reaction.count().to_string(),
            if reaction.reacted_by_current_user() {
                "yes".to_owned()
            } else {
                "no".to_owned()
            },
        ]);
    }
    write_table(writer, builder)
}

pub(crate) fn write_activity_comments<W: Write, E: Write>(
    stdout: &mut W,
    stderr: &mut E,
    response: &impl HumanSource<ActivityCommentListResult>,
    timestamps: &TimestampFormatter,
) -> io::Result<()> {
    let result = response.human_source();
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
    response: &impl HumanSource<ActivitySocialPlan>,
    timestamps: &TimestampFormatter,
) -> io::Result<()> {
    let plan = response.human_source();
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

pub(crate) fn write_activity_reaction_details(
    writer: &mut impl Write,
    response: &impl HumanSource<ActivityReactionPlan>,
    timestamps: &TimestampFormatter,
) -> io::Result<()> {
    let plan = response.human_source();
    let activity = plan.activity();
    writeln!(writer, "Activity reaction details:")?;
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
        "  Reaction: {}",
        sanitize_terminal_text(plan.action().emoji().as_str())
    )
}

pub(crate) fn write_activity_comment_removal_details(
    writer: &mut impl Write,
    response: &impl HumanSource<ActivityCommentRemovalPlan>,
) -> io::Result<()> {
    let plan = response.human_source();
    writeln!(writer, "Activity comment removal details:")?;
    writeln!(writer, "  Action: remove activity comment")?;
    writeln!(
        writer,
        "  Comment ID: {}",
        sanitize_terminal_text(plan.comment_id().as_str())
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
    response: &impl HumanSource<ActivityCommentRemovalResult>,
) -> io::Result<()> {
    let result = response.human_source();
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
    response: &impl HumanSource<ActivitySocialMutationResult>,
) -> io::Result<()> {
    let result = response.human_source();
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

pub(crate) fn write_activity_reaction_result(
    writer: &mut impl Write,
    response: &impl HumanSource<ActivityReactionMutationResult>,
) -> io::Result<()> {
    writeln!(
        writer,
        "{}",
        response.human_source().plan().action().result_label()
    )
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

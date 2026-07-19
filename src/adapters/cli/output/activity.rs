use std::io::{self, Write};

use tabled::builder::Builder;

use crate::features::activity::{
    ActivityBeforeId, ActivityCounterparty, ActivityInfoResult, ActivityListResult,
};

use super::TimestampFormatter;
use super::shared::{sanitize_terminal_text, user_label, write_table};

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
        for activity in result.activities() {
            let timestamp = timestamps.format(activity.occurred_at())?;
            builder.push_record([
                sanitize_terminal_text(activity.id().as_str()),
                timestamp,
                sanitize_terminal_text(activity.action().as_str()),
                activity.direction().to_string(),
                sanitize_terminal_text(&activity_counterparty_label(activity.counterparty())),
                format!("${}", activity.amount()),
                sanitize_terminal_text(activity.status().as_str()),
                sanitize_terminal_text(activity.note().unwrap_or("")),
            ]);
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
    writeln!(writer, "Amount: ${}", activity.amount())?;
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
    )
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

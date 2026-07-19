use std::io::{self, Write};

use tabled::builder::Builder;

use crate::features::requests::list::RequestsResult;
use crate::features::requests::{RequestAction, RequestInfoResult, RequestsBefore};

use super::TimestampFormatter;
use super::shared::{sanitize_terminal_text, user_label, write_table};

pub(crate) fn write_request_info(
    writer: &mut impl Write,
    result: &RequestInfoResult,
    timestamps: &TimestampFormatter,
) -> io::Result<()> {
    let request = result.request();
    let counterparty = request.counterparty();
    let action = match request.action() {
        RequestAction::Charge => "charge",
        RequestAction::Pay => "pay",
    };
    let created = request
        .created_at()
        .map(|value| timestamps.format(value))
        .transpose()?
        .unwrap_or_else(|| "(not provided)".to_owned());
    let username = counterparty
        .username()
        .map(ToString::to_string)
        .unwrap_or_else(|| "(not provided)".to_owned());
    writeln!(
        writer,
        "Request ID: {}",
        sanitize_terminal_text(request.id().as_str())
    )?;
    writeln!(
        writer,
        "Status: {}",
        sanitize_terminal_text(request.status().as_str())
    )?;
    writeln!(writer, "Direction: {}", request.direction())?;
    writeln!(writer, "Action: {action}")?;
    writeln!(
        writer,
        "Counterparty name: {}",
        sanitize_terminal_text(counterparty.display_name().unwrap_or("(not provided)"))
    )?;
    writeln!(
        writer,
        "Counterparty username: {}",
        sanitize_terminal_text(&username)
    )?;
    writeln!(
        writer,
        "Counterparty user ID: {}",
        sanitize_terminal_text(counterparty.user_id().as_str())
    )?;
    writeln!(writer, "Amount: ${}", request.amount())?;
    writeln!(writer, "Created: {created}")?;
    writeln!(
        writer,
        "Note: {}",
        sanitize_terminal_text(request.note().unwrap_or("(not provided)"))
    )?;
    writeln!(
        writer,
        "Audience: {}",
        sanitize_terminal_text(request.audience().unwrap_or("(not provided)"))
    )
}

pub(crate) fn write_requests<W: Write, E: Write>(
    stdout: &mut W,
    stderr: &mut E,
    result: &RequestsResult,
    timestamps: &TimestampFormatter,
) -> io::Result<()> {
    if result.requests().is_empty() {
        writeln!(stdout, "No pending requests found.")?;
    } else {
        let mut builder = Builder::default();
        builder.push_record([
            "Id",
            "Direction",
            "Counterparty",
            "Amount",
            "Created",
            "Status",
            "Note",
        ]);
        for request in result.requests() {
            let created = request
                .created_at()
                .map(|value| timestamps.format(value))
                .transpose()?
                .unwrap_or_default();
            builder.push_record([
                sanitize_terminal_text(request.id().as_str()),
                request.direction().to_string(),
                sanitize_terminal_text(&user_label(request.counterparty())),
                format!("${}", request.amount()),
                created,
                sanitize_terminal_text(request.status().as_str()),
                sanitize_terminal_text(request.note().unwrap_or("")),
            ]);
        }
        write_table(stdout, builder)?;
    }
    write_next_before(stderr, result.next_before())
}

fn write_next_before(writer: &mut impl Write, before: Option<&RequestsBefore>) -> io::Result<()> {
    match before {
        Some(before) => writeln!(writer, "Next before: {}", before.as_str()),
        None => Ok(()),
    }
}

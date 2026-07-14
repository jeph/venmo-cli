use std::io::{self, Write};

use tabled::builder::Builder;
use time::format_description::well_known::Rfc3339;

use crate::features::requests::RequestsBefore;
use crate::features::requests::list::RequestsResult;

use super::shared::{sanitize_terminal_text, user_label, write_table};

pub(crate) fn write_requests<W: Write, E: Write>(
    stdout: &mut W,
    stderr: &mut E,
    result: &RequestsResult,
) -> io::Result<()> {
    if result.requests().is_empty() {
        writeln!(stdout, "No pending requests found.")?;
    } else {
        let mut builder = Builder::default();
        builder.push_record([
            "ID",
            "DIRECTION",
            "COUNTERPARTY",
            "AMOUNT",
            "CREATED",
            "STATUS",
            "NOTE",
        ]);
        for request in result.requests() {
            let created = request
                .created_at()
                .map(|value| value.format(&Rfc3339))
                .transpose()
                .map_err(io::Error::other)?
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

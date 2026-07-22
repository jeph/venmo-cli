use std::io::{self, Write};

use super::super::command::OutputFormat;
use super::super::error::AppError;
use super::super::failure::CliFailure;
use super::shared::sanitize_terminal_text;

pub fn write_error<W: Write>(writer: &mut W, error: &AppError) -> io::Result<()> {
    writeln!(
        writer,
        "error: {}",
        sanitize_terminal_text(&error.to_string())
    )?;
    if error.requires_login() {
        writeln!(writer, "Run `venmo auth login` to authenticate again.")?;
    }
    Ok(())
}

pub fn write_cli_failure<W: Write>(writer: &mut W, failure: &CliFailure) -> io::Result<()> {
    match failure.format() {
        OutputFormat::Human => write_error(writer, failure.error()),
        OutputFormat::Json => super::json::write_failure(writer, failure),
    }
}

use std::io::{self, Write};

use super::super::error::AppError;
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

use std::io::{self, Write};

use crate::features::auth::{
    AuthStatus, DeviceTrustOutcome, LocalDeletionOutcome, LoginDisposition, LoginResult,
    LogoutReport, PasswordLoginReport,
};
use crate::shared::CredentialFormat;

use super::TimestampFormatter;
use super::shared::sanitize_terminal_text;

pub(crate) fn write_login_result<W: Write>(writer: &mut W, result: &LoginResult) -> io::Result<()> {
    writeln!(
        writer,
        "Stored Venmo credential for {}.",
        sanitize_terminal_text(&result.account().username().to_string())
    )
}

pub(crate) fn write_password_login_report<W: Write, E: Write>(
    stdout: &mut W,
    stderr: &mut E,
    report: &PasswordLoginReport,
) -> io::Result<()> {
    write_login_result(stdout, report.login())?;
    if report.login().disposition() == LoginDisposition::ReplacedExistingCredential {
        writeln!(
            stderr,
            "warning: the previous bearer token was not revoked; use official Venmo session controls if it must be invalidated."
        )?;
    }
    match report.device_trust() {
        DeviceTrustOutcome::NotNeeded => {
            writeln!(stdout, "Venmo accepted the existing trusted login device.")
        }
        DeviceTrustOutcome::Trusted => writeln!(stdout, "Trusted this login device."),
        DeviceTrustOutcome::Failed(source) => writeln!(
            stderr,
            "warning: the validated credential was stored, but Venmo device trust failed: {}; future login may require SMS verification.",
            sanitize_terminal_text(&source.to_string())
        ),
    }
}

pub(crate) fn write_auth_status<W: Write>(
    writer: &mut W,
    status: &AuthStatus,
    timestamps: &TimestampFormatter,
) -> io::Result<()> {
    let saved_at = timestamps.format(status.saved_at())?;
    let display_name = status.account().display_name().unwrap_or("(not provided)");
    let format = match status.credential_format() {
        CredentialFormat::Version1 => "version 1",
        CredentialFormat::LegacyTypeScript => "legacy TypeScript",
    };

    writeln!(
        writer,
        "Username: {}",
        sanitize_terminal_text(&status.account().username().to_string())
    )?;
    writeln!(
        writer,
        "Display name: {}",
        sanitize_terminal_text(display_name)
    )?;
    writeln!(writer, "User ID: {}", status.account().user_id())?;
    writeln!(writer, "Saved at: {saved_at}")?;
    writeln!(writer, "Credential format: {format}")
}

pub(crate) fn write_logout_report<W: Write, E: Write>(
    stdout: &mut W,
    stderr: &mut E,
    report: &LogoutReport,
) -> io::Result<()> {
    match report.local() {
        LocalDeletionOutcome::Deleted => {
            writeln!(stdout, "Removed the local Venmo credential.")?;
            writeln!(
                stderr,
                "warning: local logout does not revoke the remote bearer token; use official Venmo session controls if it must be invalidated."
            )?;
        }
        LocalDeletionOutcome::Missing => {
            writeln!(stdout, "No local Venmo credential was stored.")?;
        }
        LocalDeletionOutcome::Failed(source) => {
            writeln!(
                stderr,
                "error: local credential deletion failed: {}; the local entry may remain.",
                sanitize_terminal_text(&source.to_string())
            )?;
        }
    }
    Ok(())
}

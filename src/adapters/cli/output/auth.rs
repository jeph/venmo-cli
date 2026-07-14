use std::io::{self, Write};

use time::format_description::well_known::Rfc3339;

use crate::features::auth::{
    AuthStatus, DeviceTrustOutcome, LocalDeletionOutcome, LoginResult, LogoutReport,
    PasswordLoginReport, RemoteRevocationOutcome,
};
use crate::shared::CredentialFormat;

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

pub(crate) fn write_reauthentication_report<W: Write, E: Write>(
    stdout: &mut W,
    stderr: &mut E,
    report: &PasswordLoginReport,
) -> io::Result<()> {
    write_password_login_report(stdout, stderr, report)?;
    writeln!(
        stderr,
        "warning: reauthentication does not revoke the previous bearer token; its remote validity is unknown, so use official Venmo session controls if it must be invalidated."
    )
}

pub(crate) fn write_auth_status<W: Write>(writer: &mut W, status: &AuthStatus) -> io::Result<()> {
    let saved_at = status
        .saved_at()
        .format(&Rfc3339)
        .map_err(io::Error::other)?;
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
    match report.remote() {
        RemoteRevocationOutcome::NotRequested | RemoteRevocationOutcome::NotNeeded => {}
        RemoteRevocationOutcome::Revoked => {
            writeln!(stdout, "Revoked the remote Venmo token.")?;
        }
        RemoteRevocationOutcome::Failed(source) => {
            writeln!(
                stderr,
                "warning: remote token revocation failed: {}; the token may still be valid.",
                sanitize_terminal_text(&source.to_string())
            )?;
        }
        RemoteRevocationOutcome::NotAttempted { source, .. } => {
            writeln!(
                stderr,
                "warning: remote token revocation was not attempted: {}; remote token state is unknown.",
                sanitize_terminal_text(&source.to_string())
            )?;
        }
    }

    match report.local() {
        LocalDeletionOutcome::Deleted => {
            writeln!(stdout, "Removed the local Venmo credential.")?;
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

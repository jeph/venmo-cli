use std::io::{self, Write};

use crate::adapters::credentials::CredentialAccessPurpose;
use crate::adapters::system::SystemClock;
use crate::features::auth::{self as auth, CurrentAccountApi, PromptAvailability, PromptError};
use crate::features::payments::DefaultNoConfirmation;
use crate::shared::{CredentialDeleter, CredentialReader};

use super::super::args::{AuthArgs, AuthOperation};
use super::super::error::AppError;
use super::super::output;
use super::composition::ProductionProvider;
use super::write_and_flush;

pub(super) async fn run_production<W, E>(
    args: AuthArgs,
    provider: ProductionProvider,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<(), AppError>
where
    W: Write,
    E: Write,
{
    match args.operation {
        AuthOperation::Login => {
            let store = provider.credential_store(CredentialAccessPurpose::Login)?;
            let api = provider.api()?;
            let prompt = provider.prompt();
            confirm_plaintext_fallback_if_needed(store.backend(), &prompt, stderr)?;
            let report = auth::login_with_password(&store, &prompt, &api, &SystemClock).await?;
            store
                .retire_fallback_after_verified_login()
                .map_err(|source| AppError::CredentialFallbackCleanup {
                    source: Box::new(source),
                })?;
            finish_password_login(stdout, stderr, &report)
        }
        AuthOperation::Status => {
            let (store, api) = provider.credential_store_and_api()?;
            let timestamps = provider.timestamps();
            run_auth_status_with(&store, &api, &timestamps, stdout).await
        }
        AuthOperation::Logout => {
            let store = provider.credential_store(CredentialAccessPurpose::Logout)?;
            run_logout_local_with(&store, stdout, stderr)
        }
    }
}

const PLAINTEXT_FALLBACK_CONFIRMATION: &str =
    "Store the bearer token in an owner-only plaintext XDG state file?";

fn confirm_plaintext_fallback_if_needed<P, E>(
    backend: crate::shared::CredentialBackend,
    prompt: &P,
    stderr: &mut E,
) -> Result<(), AppError>
where
    P: DefaultNoConfirmation + PromptAvailability,
    E: Write,
{
    if backend != crate::shared::CredentialBackend::Xdg {
        return Ok(());
    }
    writeln!(
        stderr,
        "warning: no Secret Service keyring is available; the Venmo bearer token will be stored unencrypted in an owner-only XDG state file."
    )?;
    stderr.flush()?;
    if !prompt.can_prompt() {
        return Err(auth::LoginError::Prompt(PromptError::NotInteractive).into());
    }
    if !prompt
        .confirm_default_no(PLAINTEXT_FALLBACK_CONFIRMATION)
        .map_err(auth::LoginError::Prompt)?
    {
        return Err(auth::LoginError::Prompt(PromptError::Cancelled).into());
    }
    Ok(())
}

async fn run_auth_status_with<R, A, W>(
    store: &R,
    api: &A,
    timestamps: &output::TimestampFormatter,
    stdout: &mut W,
) -> Result<(), AppError>
where
    R: CredentialReader,
    A: CurrentAccountApi,
    W: Write,
{
    let status = auth::status(store, api).await?;
    write_and_flush(stdout, &status, |writer, status| {
        output::write_auth_status(writer, status, timestamps)
    })?;
    Ok(())
}

pub(super) fn run_logout_local_with<S, W, E>(
    store: &S,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<(), AppError>
where
    S: CredentialDeleter,
    W: Write,
    E: Write,
{
    let report = auth::logout_local(store);
    finish_logout(stdout, stderr, &report)
}

fn finish_password_login(
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    report: &auth::PasswordLoginReport,
) -> Result<(), AppError> {
    write_and_flush_auth_output(stdout, stderr, |stdout, stderr| {
        output::write_password_login_report(stdout, stderr, report)
    })?;
    ensure_login_complete(report)
}

fn ensure_login_complete(report: &auth::PasswordLoginReport) -> Result<(), AppError> {
    match report.device_trust() {
        auth::DeviceTrustOutcome::NotNeeded | auth::DeviceTrustOutcome::Trusted => Ok(()),
        auth::DeviceTrustOutcome::Failed(source) => Err(AppError::AuthLoginIncomplete {
            kind: source.kind(),
        }),
    }
}

fn finish_logout(
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    report: &auth::LogoutReport,
) -> Result<(), AppError> {
    write_and_flush_auth_output(stdout, stderr, |stdout, stderr| {
        output::write_logout_report(stdout, stderr, report)
    })?;
    match report.failure_kind() {
        None => Ok(()),
        Some(kind) => Err(AppError::AuthLogoutIncomplete { kind }),
    }
}

fn write_and_flush_auth_output<W, E>(
    stdout: &mut W,
    stderr: &mut E,
    write_output: impl FnOnce(&mut W, &mut E) -> io::Result<()>,
) -> Result<(), AppError>
where
    W: Write,
    E: Write,
{
    write_output(stdout, stderr).map_err(|source| AppError::AuthStateOutput { source })?;

    let stdout_result = stdout.flush();
    let stderr_result = stderr.flush();
    stdout_result
        .and(stderr_result)
        .map_err(|source| AppError::AuthStateOutput { source })
}

#[cfg(test)]
mod tests;

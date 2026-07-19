use std::io::{self, Write};

use crate::adapters::system::SystemClock;
use crate::features::auth::{
    self as auth, AuthenticationInput, CurrentAccountApi, PasswordLoginApi,
};
use crate::shared::{Clock, CredentialDeleter, CredentialReader, CredentialWriter};

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
            let (store, api) = provider.credential_store_and_api()?;
            let prompt = provider.prompt();
            run_password_login_with(&store, &prompt, &api, &SystemClock, stdout, stderr).await
        }
        AuthOperation::Status => {
            let (store, api) = provider.credential_store_and_api()?;
            let timestamps = provider.timestamps();
            run_auth_status_with(&store, &api, &timestamps, stdout).await
        }
        AuthOperation::Logout => {
            let store = provider.credential_store();
            run_logout_local_with(&store, stdout, stderr)
        }
    }
}

async fn run_password_login_with<S, P, A, C, W, E>(
    store: &S,
    prompt: &P,
    api: &A,
    clock: &C,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<(), AppError>
where
    S: CredentialReader + CredentialWriter,
    P: AuthenticationInput,
    A: CurrentAccountApi + PasswordLoginApi,
    C: Clock,
    W: Write,
    E: Write,
{
    let report = auth::login_with_password(store, prompt, api, clock).await?;
    finish_password_login(stdout, stderr, &report)
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

use std::io::Write;

use crate::adapters::credentials::CredentialAccessPurpose;
use crate::adapters::system::SystemClock;
use crate::features::auth::{self as auth, CurrentAccountApi, PromptAvailability, PromptError};
use crate::features::payments::DefaultNoConfirmation;
use crate::shared::{CredentialDeleter, CredentialReader};

use super::super::args::{AuthArgs, AuthOperation};
use super::super::error::AppError;
use super::super::output;
use super::super::response;
use super::composition::ProductionProvider;

pub(super) async fn run_production<W, E>(
    args: AuthArgs,
    provider: ProductionProvider,
    output: &mut output::OutputSession<'_, W, E>,
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
            confirm_plaintext_fallback_if_needed(store.backend(), &prompt, output.stderr())?;
            let report = auth::login_with_password(&store, &prompt, &api, &SystemClock).await?;
            let response = response::password_login(&report)
                .map_err(|source| AppError::AuthStateOutput { source })?;
            output.record_partial(&response);
            store
                .retire_fallback_after_verified_login()
                .map_err(|source| AppError::CredentialFallbackCleanup {
                    source: Box::new(source),
                })?;
            finish_password_login(output, &response)
        }
        AuthOperation::Status => {
            let (store, api) = provider.credential_store_and_api()?;
            let timestamps = provider.timestamps();
            run_auth_status_with(&store, &api, &timestamps, output).await
        }
        AuthOperation::Logout => {
            let store = provider.credential_store(CredentialAccessPurpose::Logout)?;
            run_logout_local_with(&store, output)
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

async fn run_auth_status_with<R, A, W, E>(
    store: &R,
    api: &A,
    timestamps: &output::TimestampFormatter,
    output: &mut output::OutputSession<'_, W, E>,
) -> Result<(), AppError>
where
    R: CredentialReader,
    A: CurrentAccountApi,
    W: Write,
    E: Write,
{
    let status = auth::status(store, api).await?;
    let response = response::auth_status(&status)?;
    output.write_success(
        &response,
        output::OutputClass::AuthState,
        |stdout, _stderr, response| output::write_auth_status(stdout, response, timestamps),
    )?;
    Ok(())
}

pub(super) fn run_logout_local_with<S, W, E>(
    store: &S,
    output: &mut output::OutputSession<'_, W, E>,
) -> Result<(), AppError>
where
    S: CredentialDeleter,
    W: Write,
    E: Write,
{
    let report = auth::logout_local(store);
    let response = response::logout(&report);
    finish_logout(output, &response)
}

fn finish_password_login<W: Write, E: Write>(
    output: &mut output::OutputSession<'_, W, E>,
    response: &response::Response<'_, auth::PasswordLoginReport>,
) -> Result<(), AppError> {
    let report = response.source();
    match ensure_login_complete(report) {
        Ok(()) => {
            output.clear_partial();
            output.mark_completed();
            output.write_success(
                response,
                output::OutputClass::AuthState,
                |stdout, stderr, response| {
                    output::write_password_login_report(stdout, stderr, response)
                },
            )
        }
        Err(error) => {
            output.write_partial(response, |stdout, stderr, response| {
                output::write_password_login_report(stdout, stderr, response)
            })?;
            Err(error)
        }
    }
}

fn ensure_login_complete(report: &auth::PasswordLoginReport) -> Result<(), AppError> {
    match report.device_trust() {
        auth::DeviceTrustOutcome::NotNeeded | auth::DeviceTrustOutcome::Trusted => Ok(()),
        auth::DeviceTrustOutcome::Failed(source) => Err(AppError::AuthLoginIncomplete {
            kind: source.kind(),
        }),
    }
}

fn finish_logout<W: Write, E: Write>(
    output: &mut output::OutputSession<'_, W, E>,
    response: &response::Response<'_, auth::LogoutReport>,
) -> Result<(), AppError> {
    let report = response.source();
    match report.failure_kind() {
        None => {
            output.mark_completed();
            output.write_success(
                response,
                output::OutputClass::AuthState,
                |stdout, stderr, response| output::write_logout_report(stdout, stderr, response),
            )
        }
        Some(kind) => {
            output.write_partial(response, |stdout, stderr, response| {
                output::write_logout_report(stdout, stderr, response)
            })?;
            Err(AppError::AuthLogoutIncomplete { kind })
        }
    }
}

#[cfg(test)]
mod tests;

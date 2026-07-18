use std::future::Future;
use std::io::{self, Write};

use super::args::{AuthArgs, AuthOperation, Cli, Command};
use super::completions;
use super::error::AppError;
use super::logging::InitializationError;
use super::prompt::TerminalCapabilities;
use crate::adapters::credentials::NativeCredentialStore;
use crate::features::auth::{LoginError, PromptError};

mod auth;
mod composition;
mod doctor;
mod reads;
mod writes;

/// Runs production command composition with process terminal state.
pub async fn run<W, E>(cli: Cli, stdout: &mut W, stderr: &mut E) -> Result<(), AppError>
where
    W: Write,
    E: Write,
{
    let terminal_capabilities = TerminalCapabilities::from_process();
    run_production_with(
        cli,
        stdout,
        stderr,
        terminal_capabilities,
        super::logging::initialize,
        move |command, stdout, stderr| {
            composition::run_production(command, stdout, stderr, terminal_capabilities)
        },
    )
    .await
}

async fn run_production_with<'a, W, E, I, X, F>(
    cli: Cli,
    stdout: &'a mut W,
    stderr: &'a mut E,
    terminal_capabilities: TerminalCapabilities,
    initialize_logging: I,
    execute: X,
) -> Result<(), AppError>
where
    W: Write + 'a,
    E: Write + 'a,
    I: FnOnce(bool) -> Result<(), InitializationError> + 'a,
    X: FnOnce(Command, &'a mut W, &'a mut E) -> F + 'a,
    F: Future<Output = Result<(), AppError>> + 'a,
{
    let verbose = cli.verbose;
    run_with(
        cli,
        stdout,
        stderr,
        terminal_capabilities,
        move |command, stdout, stderr| {
            execute_after_logging(
                verbose,
                command,
                stdout,
                stderr,
                initialize_logging,
                execute,
            )
        },
    )
    .await
}

async fn execute_after_logging<'a, W, E, I, X, F>(
    verbose: bool,
    command: Command,
    stdout: &'a mut W,
    stderr: &'a mut E,
    initialize_logging: I,
    execute: X,
) -> Result<(), AppError>
where
    W: Write + 'a,
    E: Write + 'a,
    I: FnOnce(bool) -> Result<(), InitializationError> + 'a,
    X: FnOnce(Command, &'a mut W, &'a mut E) -> F + 'a,
    F: Future<Output = Result<(), AppError>> + 'a,
{
    initialize_logging(verbose).map_err(|source| AppError::LoggingInitialization { source })?;
    execute(command, stdout, stderr).await
}

/// Test seam for the service-free dispatch boundary.
///
/// The executor is reached only after completions and authentication terminal preconditions have
/// been resolved without constructing a credential store or API client.
async fn run_with<'a, W, E, X, F>(
    cli: Cli,
    stdout: &'a mut W,
    stderr: &'a mut E,
    terminal_capabilities: TerminalCapabilities,
    execute: X,
) -> Result<(), AppError>
where
    W: Write + 'a,
    E: Write + 'a,
    X: FnOnce(Command, &'a mut W, &'a mut E) -> F,
    F: Future<Output = Result<(), AppError>> + 'a,
{
    match cli.command {
        Command::Completions(args) => completions::write(args.shell, stdout)
            .map_err(|source| AppError::CompletionOutput { source }),
        Command::Auth(args)
            if auth_requires_interactive_terminal(&args) && !terminal_capabilities.can_prompt() =>
        {
            Err(LoginError::Prompt(PromptError::NotInteractive).into())
        }
        command => execute(command, stdout, stderr).await,
    }
}

fn auth_requires_interactive_terminal(args: &AuthArgs) -> bool {
    matches!(args.operation, AuthOperation::Login)
}

/// Preserves synchronous/service-free behavior if the one process runtime cannot be built.
/// Local-only logout still deletes the keyring entry without constructing a runtime.
pub fn handle_runtime_initialization_failure<W, E>(
    cli: Cli,
    stdout: &mut W,
    stderr: &mut E,
    source: io::Error,
) -> Result<(), AppError>
where
    W: Write,
    E: Write,
{
    handle_runtime_initialization_failure_with(
        cli,
        stdout,
        stderr,
        TerminalCapabilities::from_process(),
        source,
        super::logging::initialize,
    )
}

fn handle_runtime_initialization_failure_with<W, E, I>(
    cli: Cli,
    stdout: &mut W,
    stderr: &mut E,
    terminal_capabilities: TerminalCapabilities,
    source: io::Error,
    initialize_logging: I,
) -> Result<(), AppError>
where
    W: Write,
    E: Write,
    I: FnOnce(bool) -> Result<(), InitializationError>,
{
    let command = match cli.command {
        Command::Completions(args) => {
            return completions::write(args.shell, stdout)
                .map_err(|source| AppError::CompletionOutput { source });
        }
        Command::Auth(args)
            if auth_requires_interactive_terminal(&args) && !terminal_capabilities.can_prompt() =>
        {
            return Err(LoginError::Prompt(PromptError::NotInteractive).into());
        }
        command => command,
    };

    initialize_logging(cli.verbose).map_err(|source| AppError::LoggingInitialization { source })?;
    let failure = AppError::RuntimeInitialization { source };
    match command {
        Command::Auth(AuthArgs {
            operation: AuthOperation::Logout,
        }) => {
            let store = NativeCredentialStore::new();
            auth::run_logout_local_with(&store, stdout, stderr)
        }
        _ => Err(failure),
    }
}

fn write_and_flush<W, T>(
    writer: &mut W,
    value: &T,
    write_output: impl FnOnce(&mut W, &T) -> io::Result<()>,
) -> io::Result<()>
where
    W: Write,
{
    write_output(writer, value)?;
    writer.flush()
}

#[cfg(test)]
mod tests;

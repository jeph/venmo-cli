use std::future::Future;
use std::io::{self, Write};

use super::args::{AuthArgs, AuthOperation, Cli, Command};
use super::command::OutputFormat;
use super::error::AppError;
use super::failure::CliFailure;
use super::logging::InitializationError;
use super::output::OutputSession;
use super::prompt::TerminalCapabilities;
use crate::adapters::credentials::{CredentialAccessPurpose, CredentialStore};
use crate::features::auth::{LoginError, PromptError};

mod auth;
mod composition;
mod reads;
mod writes;

/// Runs production command composition with process terminal state.
pub async fn run<W, E>(cli: Cli, stdout: &mut W, stderr: &mut E) -> Result<(), CliFailure>
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
        move |command, format, stdout, stderr| {
            composition::run_production(command, format, stdout, stderr, terminal_capabilities)
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
) -> Result<(), CliFailure>
where
    W: Write + 'a,
    E: Write + 'a,
    I: FnOnce(bool) -> Result<(), InitializationError> + 'a,
    X: FnOnce(Command, OutputFormat, &'a mut W, &'a mut E) -> F + 'a,
    F: Future<Output = Result<(), CliFailure>> + 'a,
{
    let debug = cli.debug;
    let format = cli.output_format();
    run_with(
        cli,
        stdout,
        stderr,
        terminal_capabilities,
        move |command, stdout, stderr| {
            execute_after_logging(
                debug,
                format,
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
    debug: bool,
    format: OutputFormat,
    command: Command,
    stdout: &'a mut W,
    stderr: &'a mut E,
    initialize_logging: I,
    execute: X,
) -> Result<(), CliFailure>
where
    W: Write + 'a,
    E: Write + 'a,
    I: FnOnce(bool) -> Result<(), InitializationError> + 'a,
    X: FnOnce(Command, OutputFormat, &'a mut W, &'a mut E) -> F + 'a,
    F: Future<Output = Result<(), CliFailure>> + 'a,
{
    let command_id = command.id();
    initialize_logging(debug).map_err(|source| {
        CliFailure::plain(
            AppError::LoggingInitialization { source },
            command_id,
            format,
        )
    })?;
    let command_name = debug_command_name(&command);
    trace_command_started(command_name);
    let result = execute(command, format, stdout, stderr).await;
    trace_command_result(command_name, &result);
    result
}

/// Test seam for the service-free dispatch boundary.
///
/// The executor is reached only after authentication terminal preconditions have been resolved
/// without constructing a credential store or API client.
async fn run_with<'a, W, E, X, F>(
    cli: Cli,
    stdout: &'a mut W,
    stderr: &'a mut E,
    terminal_capabilities: TerminalCapabilities,
    execute: X,
) -> Result<(), CliFailure>
where
    W: Write + 'a,
    E: Write + 'a,
    X: FnOnce(Command, &'a mut W, &'a mut E) -> F,
    F: Future<Output = Result<(), CliFailure>> + 'a,
{
    let format = cli.output_format();
    match cli.command {
        Command::Auth(args)
            if auth_requires_interactive_terminal(&args) && !terminal_capabilities.can_prompt() =>
        {
            let command = Command::Auth(args);
            Err(CliFailure::plain(
                LoginError::Prompt(PromptError::NotInteractive).into(),
                command.id(),
                format,
            ))
        }
        command => execute(command, stdout, stderr).await,
    }
}

fn auth_requires_interactive_terminal(args: &AuthArgs) -> bool {
    matches!(args.operation, AuthOperation::Login)
}

/// Preserves synchronous/service-free behavior if the one process runtime cannot be built.
/// Local-only logout still deletes selected local credential state without constructing a runtime.
pub fn handle_runtime_initialization_failure<W, E>(
    cli: Cli,
    stdout: &mut W,
    stderr: &mut E,
    source: io::Error,
) -> Result<(), CliFailure>
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
) -> Result<(), CliFailure>
where
    W: Write,
    E: Write,
    I: FnOnce(bool) -> Result<(), InitializationError>,
{
    let format = cli.output_format();
    let command = match cli.command {
        Command::Auth(args)
            if auth_requires_interactive_terminal(&args) && !terminal_capabilities.can_prompt() =>
        {
            let command = Command::Auth(args);
            return Err(CliFailure::plain(
                LoginError::Prompt(PromptError::NotInteractive).into(),
                command.id(),
                format,
            ));
        }
        command => command,
    };

    let command_id = command.id();
    initialize_logging(cli.debug).map_err(|source| {
        CliFailure::plain(
            AppError::LoggingInitialization { source },
            command_id,
            format,
        )
    })?;
    let command_name = debug_command_name(&command);
    trace_command_started(command_name);
    let failure = AppError::RuntimeInitialization { source };
    let mut output = OutputSession::new(
        format,
        command_id,
        terminal_capabilities.can_prompt(),
        stdout,
        stderr,
    );
    let result = match command {
        Command::Auth(AuthArgs {
            operation: AuthOperation::Logout,
        }) => match CredentialStore::production(CredentialAccessPurpose::Logout) {
            Ok(store) => auth::run_logout_local_with(&store, &mut output),
            Err(source) => Err(AppError::CredentialStoreInitialization {
                source: Box::new(source),
            }),
        },
        _ => Err(failure),
    };
    let result = result.map_err(|error| output.into_failure(error));
    trace_command_result(command_name, &result);
    result
}

fn trace_command_started(command_name: &'static str) {
    tracing::debug!(cli.command = command_name, "CLI command started");
}

fn trace_command_result(command_name: &'static str, result: &Result<(), CliFailure>) {
    match result {
        Ok(()) => tracing::debug!(cli.command = command_name, "CLI command completed"),
        Err(failure) => tracing::debug!(
            cli.command = command_name,
            error.category = ?failure.error().category(),
            error.exit_code = failure.exit_code(),
            "CLI command failed"
        ),
    }
}

fn debug_command_name(command: &Command) -> &'static str {
    command.id().as_str()
}

#[cfg(test)]
mod tests;

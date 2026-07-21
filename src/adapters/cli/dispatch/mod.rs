use std::future::Future;
use std::io::{self, Write};

use super::args::{
    ActivityCommentsOperation, ActivityOperation, AuthArgs, AuthOperation, Cli, Command,
    FriendsOperation, PayOperation, RequestsOperation, TransferOperation, UsersOperation,
};
use super::error::AppError;
use super::logging::InitializationError;
use super::prompt::TerminalCapabilities;
use crate::adapters::credentials::NativeCredentialStore;
use crate::features::auth::{LoginError, PromptError};

mod auth;
mod composition;
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
    let debug = cli.debug;
    run_with(
        cli,
        stdout,
        stderr,
        terminal_capabilities,
        move |command, stdout, stderr| {
            execute_after_logging(debug, command, stdout, stderr, initialize_logging, execute)
        },
    )
    .await
}

async fn execute_after_logging<'a, W, E, I, X, F>(
    debug: bool,
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
    initialize_logging(debug).map_err(|source| AppError::LoggingInitialization { source })?;
    let command_name = debug_command_name(&command);
    trace_command_started(command_name);
    let result = execute(command, stdout, stderr).await;
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
) -> Result<(), AppError>
where
    W: Write + 'a,
    E: Write + 'a,
    X: FnOnce(Command, &'a mut W, &'a mut E) -> F,
    F: Future<Output = Result<(), AppError>> + 'a,
{
    match cli.command {
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
        Command::Auth(args)
            if auth_requires_interactive_terminal(&args) && !terminal_capabilities.can_prompt() =>
        {
            return Err(LoginError::Prompt(PromptError::NotInteractive).into());
        }
        command => command,
    };

    initialize_logging(cli.debug).map_err(|source| AppError::LoggingInitialization { source })?;
    let command_name = debug_command_name(&command);
    trace_command_started(command_name);
    let failure = AppError::RuntimeInitialization { source };
    let result = match command {
        Command::Auth(AuthArgs {
            operation: AuthOperation::Logout,
        }) => {
            let store = NativeCredentialStore::new();
            auth::run_logout_local_with(&store, stdout, stderr)
        }
        _ => Err(failure),
    };
    trace_command_result(command_name, &result);
    result
}

fn trace_command_started(command_name: &'static str) {
    tracing::debug!(cli.command = command_name, "CLI command started");
}

fn trace_command_result(command_name: &'static str, result: &Result<(), AppError>) {
    match result {
        Ok(()) => tracing::debug!(cli.command = command_name, "CLI command completed"),
        Err(error) => tracing::debug!(
            cli.command = command_name,
            error.category = ?error.category(),
            error.exit_code = error.exit_code(),
            "CLI command failed"
        ),
    }
}

fn debug_command_name(command: &Command) -> &'static str {
    match command {
        Command::Auth(args) => match args.operation {
            AuthOperation::Login => "auth.login",
            AuthOperation::Logout => "auth.logout",
            AuthOperation::Status => "auth.status",
        },
        Command::Pay(args) => match args.operation {
            PayOperation::Options => "pay.options",
            PayOperation::User(_) => "pay.user",
        },
        Command::Friends(args) => match args.operation {
            FriendsOperation::List(_) => "friends.list",
            FriendsOperation::Add(_) => "friends.add",
            FriendsOperation::Remove(_) => "friends.remove",
        },
        Command::Users(args) => match args.operation {
            UsersOperation::Search(_) => "users.search",
            UsersOperation::Info(_) => "users.info",
        },
        Command::Balance => "balance",
        Command::Activity(args) => match &args.operation {
            ActivityOperation::List(_) => "activity.list",
            ActivityOperation::Info(_) => "activity.info",
            ActivityOperation::Like(_) => "activity.like",
            ActivityOperation::Unlike(_) => "activity.unlike",
            ActivityOperation::Comments(args) => match args.operation {
                ActivityCommentsOperation::List(_) => "activity.comments.list",
                ActivityCommentsOperation::Add(_) => "activity.comments.add",
                ActivityCommentsOperation::Remove(_) => "activity.comments.remove",
            },
        },
        Command::Requests(args) => match args.operation {
            RequestsOperation::List(_) => "requests.list",
            RequestsOperation::Create(_) => "requests.create",
            RequestsOperation::Accept(_) => "requests.accept",
            RequestsOperation::Decline(_) => "requests.decline",
            RequestsOperation::Cancel(_) => "requests.cancel",
            RequestsOperation::Info(_) => "requests.info",
        },
        Command::Transfer(args) => match args.operation {
            TransferOperation::Options => "transfer.options",
            TransferOperation::Out(_) => "transfer.out",
        },
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

use std::cell::RefCell;
use std::error::Error;
use std::future::ready;
use std::io::{self, Write};
use std::rc::Rc;

use clap::Parser;

use super::*;
use crate::adapters::cli::error::ErrorCategory;
use crate::shared::test_support::Observed;

type TestResult = Result<(), Box<dyn Error>>;

#[derive(Clone, Debug, Eq, PartialEq)]
enum DispatchCall {
    InitializeLogging { verbose: bool },
    Execute(Command),
}

#[derive(Debug, Default, Eq, PartialEq)]
struct DispatchState {
    calls: Vec<DispatchCall>,
    stdout: WriterState,
    stderr: WriterState,
}

#[derive(Debug, Default, Eq, PartialEq)]
struct WriterState {
    bytes: Vec<u8>,
    flush_count: u32,
}

struct RecordingWriter {
    state: WriterState,
}

impl RecordingWriter {
    const fn new(state: WriterState) -> Self {
        Self { state }
    }
}

impl Write for RecordingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.state.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.state.flush_count += 1;
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum ExecutorBehavior {
    Succeed,
    Fail,
}

#[derive(Clone, Copy)]
enum LoggingBehavior {
    Succeed,
    Fail,
}

struct DispatchSetup {
    cli: Cli,
    terminals: TerminalCapabilities,
    logging_behavior: LoggingBehavior,
    executor_behavior: ExecutorBehavior,
}

impl DispatchSetup {
    fn parse(arguments: &[&str], terminals: TerminalCapabilities) -> Result<Self, clap::Error> {
        Ok(Self {
            cli: Cli::try_parse_from(arguments)?,
            terminals,
            logging_behavior: LoggingBehavior::Succeed,
            executor_behavior: ExecutorBehavior::Succeed,
        })
    }

    const fn with_logging_behavior(mut self, logging_behavior: LoggingBehavior) -> Self {
        self.logging_behavior = logging_behavior;
        self
    }

    const fn with_executor_behavior(mut self, executor_behavior: ExecutorBehavior) -> Self {
        self.executor_behavior = executor_behavior;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ErrorVariant {
    LoggingInitialization,
    RuntimeInitialization,
    AuthLogin,
    Unexpected,
}

#[derive(Debug, Eq, PartialEq)]
enum ResultSnapshot {
    Success,
    Failure {
        variant: ErrorVariant,
        category: ErrorCategory,
        exit_code: u8,
        message: String,
    },
}

#[tokio::test(flavor = "current_thread")]
async fn service_free_dispatch_branches_have_complete_outcomes() -> TestResult {
    let terminals = TerminalCapabilities::new(false, false);
    for (arguments, variant, message) in [(
        &["venmo", "auth", "login"][..],
        ErrorVariant::AuthLogin,
        "an interactive terminal is required",
    )] {
        let setup = DispatchSetup::parse(arguments, terminals)?;
        let initial_state = DispatchState::default();
        let expected = Observed::new(
            ResultSnapshot::Failure {
                variant,
                category: if variant == ErrorVariant::AuthLogin {
                    ErrorCategory::Usage
                } else {
                    ErrorCategory::Internal
                },
                exit_code: if variant == ErrorVariant::AuthLogin {
                    2
                } else {
                    1
                },
                message: message.to_owned(),
            },
            DispatchState::default(),
        );

        let observed = execute_dispatch(setup, initial_state).await;

        assert_eq!(observed, expected, "arguments: {arguments:?}");
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn production_preconditions_do_not_call_the_logging_initializer() -> TestResult {
    let terminals = TerminalCapabilities::new(false, false);
    for (arguments, variant, message) in [(
        &["venmo", "--verbose", "auth", "login"][..],
        ErrorVariant::AuthLogin,
        "an interactive terminal is required",
    )] {
        let setup = DispatchSetup::parse(arguments, terminals)?
            .with_logging_behavior(LoggingBehavior::Fail);
        let initial_state = DispatchState::default();
        let expected = Observed::new(
            ResultSnapshot::Failure {
                variant,
                category: if variant == ErrorVariant::AuthLogin {
                    ErrorCategory::Usage
                } else {
                    ErrorCategory::Internal
                },
                exit_code: if variant == ErrorVariant::AuthLogin {
                    2
                } else {
                    1
                },
                message: message.to_owned(),
            },
            DispatchState::default(),
        );

        let observed = execute_production_dispatch(setup, initial_state).await;

        assert_eq!(observed, expected, "arguments: {arguments:?}");
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn delegated_production_commands_initialize_logging_before_execution() -> TestResult {
    for arguments in [
        &["venmo", "--verbose", "balance"][..],
        &[
            "venmo",
            "--verbose",
            "requests",
            "accept",
            "request-1",
            "--yes",
        ][..],
        &[
            "venmo",
            "--verbose",
            "requests",
            "decline",
            "request-1",
            "--yes",
        ][..],
    ] {
        let setup = DispatchSetup::parse(arguments, TerminalCapabilities::new(false, false))?;
        let delegated_command = setup.cli.command.clone();
        let initial_state = DispatchState::default();
        let expected = Observed::new(
            ResultSnapshot::Success,
            DispatchState {
                calls: vec![
                    DispatchCall::InitializeLogging { verbose: true },
                    DispatchCall::Execute(delegated_command),
                ],
                ..DispatchState::default()
            },
        );

        let observed = execute_production_dispatch(setup, initial_state).await;

        assert_eq!(observed, expected, "arguments: {arguments:?}");
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn logging_initialization_failures_prevent_production_execution() -> TestResult {
    for arguments in [
        &["venmo", "--verbose", "balance"][..],
        &[
            "venmo",
            "--verbose",
            "requests",
            "accept",
            "request-1",
            "--yes",
        ][..],
        &[
            "venmo",
            "--verbose",
            "requests",
            "decline",
            "request-1",
            "--yes",
        ][..],
    ] {
        let setup = DispatchSetup::parse(arguments, TerminalCapabilities::new(false, false))?
            .with_logging_behavior(LoggingBehavior::Fail);
        let initial_state = DispatchState::default();
        let expected = Observed::new(
            ResultSnapshot::Failure {
                variant: ErrorVariant::LoggingInitialization,
                category: ErrorCategory::Internal,
                exit_code: 1,
                message: "failed to initialize verbose diagnostics".to_owned(),
            },
            DispatchState {
                calls: vec![DispatchCall::InitializeLogging { verbose: true }],
                ..DispatchState::default()
            },
        );

        let observed = execute_production_dispatch(setup, initial_state).await;

        assert_eq!(observed, expected, "arguments: {arguments:?}");
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn every_service_command_can_reach_the_typed_executor_without_production_services()
-> TestResult {
    for (arguments, terminals) in [
        (
            &["venmo", "auth", "login"][..],
            TerminalCapabilities::new(true, true),
        ),
        (
            &["venmo", "auth", "status"][..],
            TerminalCapabilities::new(false, false),
        ),
        (
            &["venmo", "auth", "logout"][..],
            TerminalCapabilities::new(false, false),
        ),
        (
            &[
                "venmo",
                "pay",
                "user",
                "@alice",
                "0.01",
                "Synthetic",
                "--yes",
            ][..],
            TerminalCapabilities::new(false, false),
        ),
        (
            &["venmo", "requests", "create", "456", "0.01", "Synthetic"][..],
            TerminalCapabilities::new(false, false),
        ),
        (
            &["venmo", "requests", "accept", "request-1", "--yes"][..],
            TerminalCapabilities::new(false, false),
        ),
        (
            &["venmo", "requests", "decline", "request-1", "--yes"][..],
            TerminalCapabilities::new(false, false),
        ),
        (
            &["venmo", "friends", "list"][..],
            TerminalCapabilities::new(false, false),
        ),
        (
            &["venmo", "users", "search", "alice"][..],
            TerminalCapabilities::new(false, false),
        ),
        (
            &["venmo", "pay", "methods"][..],
            TerminalCapabilities::new(false, false),
        ),
        (
            &["venmo", "balance"][..],
            TerminalCapabilities::new(false, false),
        ),
        (
            &["venmo", "activity", "list"][..],
            TerminalCapabilities::new(false, false),
        ),
        (
            &["venmo", "activity", "info", "story-1"][..],
            TerminalCapabilities::new(false, false),
        ),
        (
            &["venmo", "requests", "list"][..],
            TerminalCapabilities::new(false, false),
        ),
    ] {
        let setup = DispatchSetup::parse(arguments, terminals)?;
        let delegated_command = setup.cli.command.clone();
        let initial_state = DispatchState::default();
        let expected = Observed::new(
            ResultSnapshot::Success,
            DispatchState {
                calls: vec![DispatchCall::Execute(delegated_command)],
                ..DispatchState::default()
            },
        );

        let observed = execute_dispatch(setup, initial_state).await;

        assert_eq!(observed, expected, "arguments: {arguments:?}");
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn executor_failures_preserve_the_complete_result_and_state() -> TestResult {
    let setup = DispatchSetup::parse(
        &["venmo", "balance"],
        TerminalCapabilities::new(false, false),
    )?
    .with_executor_behavior(ExecutorBehavior::Fail);
    let delegated_command = setup.cli.command.clone();
    let initial_state = DispatchState::default();
    let expected = Observed::new(
        ResultSnapshot::Failure {
            variant: ErrorVariant::Unexpected,
            category: ErrorCategory::Internal,
            exit_code: 1,
            message: "failed to write command output".to_owned(),
        },
        DispatchState {
            calls: vec![DispatchCall::Execute(delegated_command)],
            ..DispatchState::default()
        },
    );

    let observed = execute_dispatch(setup, initial_state).await;

    assert_eq!(observed, expected);
    Ok(())
}

#[test]
fn runtime_initialization_keeps_every_service_free_precondition_service_free() -> TestResult {
    let terminals = TerminalCapabilities::new(false, false);
    for (arguments, variant, category, exit_code, message) in [
        (
            &["venmo", "--verbose", "auth", "login"][..],
            ErrorVariant::AuthLogin,
            ErrorCategory::Usage,
            2,
            "an interactive terminal is required",
        ),
        (
            &["venmo", "balance"][..],
            ErrorVariant::RuntimeInitialization,
            ErrorCategory::Internal,
            1,
            "failed to initialize the asynchronous runtime",
        ),
        (
            &["venmo", "requests", "accept", "request-1", "--yes"][..],
            ErrorVariant::RuntimeInitialization,
            ErrorCategory::Internal,
            1,
            "failed to initialize the asynchronous runtime",
        ),
        (
            &["venmo", "requests", "decline", "request-1", "--yes"][..],
            ErrorVariant::RuntimeInitialization,
            ErrorCategory::Internal,
            1,
            "failed to initialize the asynchronous runtime",
        ),
        (
            &["venmo", "auth", "login"][..],
            ErrorVariant::RuntimeInitialization,
            ErrorCategory::Internal,
            1,
            "failed to initialize the asynchronous runtime",
        ),
    ] {
        let case_terminals = if variant == ErrorVariant::RuntimeInitialization
            && arguments == ["venmo", "auth", "login"]
        {
            TerminalCapabilities::new(true, true)
        } else {
            terminals
        };
        let setup = DispatchSetup::parse(arguments, case_terminals)?;
        let setup = if variant == ErrorVariant::RuntimeInitialization {
            setup
        } else {
            setup.with_logging_behavior(LoggingBehavior::Fail)
        };
        let initial_state = DispatchState::default();
        let expected = Observed::new(
            ResultSnapshot::Failure {
                variant,
                category,
                exit_code,
                message: message.to_owned(),
            },
            DispatchState {
                calls: if variant == ErrorVariant::RuntimeInitialization {
                    vec![DispatchCall::InitializeLogging { verbose: false }]
                } else {
                    Vec::new()
                },
                ..DispatchState::default()
            },
        );

        let observed = execute_runtime_failure(setup, initial_state);

        assert_eq!(observed, expected, "arguments: {arguments:?}");
    }
    Ok(())
}

#[test]
fn runtime_failure_preserves_logging_errors_before_delegated_fallbacks() -> TestResult {
    let setup = DispatchSetup::parse(
        &["venmo", "--verbose", "balance"],
        TerminalCapabilities::new(false, false),
    )?
    .with_logging_behavior(LoggingBehavior::Fail);
    let initial_state = DispatchState::default();
    let expected = Observed::new(
        ResultSnapshot::Failure {
            variant: ErrorVariant::LoggingInitialization,
            category: ErrorCategory::Internal,
            exit_code: 1,
            message: "failed to initialize verbose diagnostics".to_owned(),
        },
        DispatchState {
            calls: vec![DispatchCall::InitializeLogging { verbose: true }],
            ..DispatchState::default()
        },
    );

    let observed = execute_runtime_failure(setup, initial_state);

    assert_eq!(observed, expected);
    Ok(())
}

async fn execute_dispatch(
    setup: DispatchSetup,
    initial_state: DispatchState,
) -> Observed<ResultSnapshot, DispatchState> {
    let calls = Rc::new(RefCell::new(initial_state.calls));
    let transcript = Rc::clone(&calls);
    let behavior = setup.executor_behavior;
    let mut stdout = RecordingWriter::new(initial_state.stdout);
    let mut stderr = RecordingWriter::new(initial_state.stderr);
    let result = run_with(
        setup.cli,
        &mut stdout,
        &mut stderr,
        setup.terminals,
        move |command, _, _| {
            transcript.borrow_mut().push(DispatchCall::Execute(command));
            ready(match behavior {
                ExecutorBehavior::Succeed => Ok(()),
                ExecutorBehavior::Fail => Err(AppError::CommandOutput {
                    source: io::Error::other("synthetic executor failure"),
                }),
            })
        },
    )
    .await;
    let state = DispatchState {
        calls: calls.borrow().clone(),
        stdout: stdout.state,
        stderr: stderr.state,
    };
    Observed::new(snapshot_result(result), state)
}

async fn execute_production_dispatch(
    setup: DispatchSetup,
    initial_state: DispatchState,
) -> Observed<ResultSnapshot, DispatchState> {
    let calls = Rc::new(RefCell::new(initial_state.calls));
    let logging_transcript = Rc::clone(&calls);
    let executor_transcript = Rc::clone(&calls);
    let logging_behavior = setup.logging_behavior;
    let executor_behavior = setup.executor_behavior;
    let mut stdout = RecordingWriter::new(initial_state.stdout);
    let mut stderr = RecordingWriter::new(initial_state.stderr);
    let result = run_production_with(
        setup.cli,
        &mut stdout,
        &mut stderr,
        setup.terminals,
        move |verbose| {
            logging_transcript
                .borrow_mut()
                .push(DispatchCall::InitializeLogging { verbose });
            match logging_behavior {
                LoggingBehavior::Succeed => Ok(()),
                LoggingBehavior::Fail => Err(Box::new(io::Error::other(
                    "synthetic logging initialization failure",
                )) as InitializationError),
            }
        },
        move |command, _, _| {
            executor_transcript
                .borrow_mut()
                .push(DispatchCall::Execute(command));
            ready(match executor_behavior {
                ExecutorBehavior::Succeed => Ok(()),
                ExecutorBehavior::Fail => Err(AppError::CommandOutput {
                    source: io::Error::other("synthetic executor failure"),
                }),
            })
        },
    )
    .await;
    let state = DispatchState {
        calls: calls.borrow().clone(),
        stdout: stdout.state,
        stderr: stderr.state,
    };
    Observed::new(snapshot_result(result), state)
}

fn execute_runtime_failure(
    setup: DispatchSetup,
    initial_state: DispatchState,
) -> Observed<ResultSnapshot, DispatchState> {
    let calls = Rc::new(RefCell::new(initial_state.calls));
    let logging_transcript = Rc::clone(&calls);
    let logging_behavior = setup.logging_behavior;
    let mut stdout = RecordingWriter::new(initial_state.stdout);
    let mut stderr = RecordingWriter::new(initial_state.stderr);
    let result = handle_runtime_initialization_failure_with(
        setup.cli,
        &mut stdout,
        &mut stderr,
        setup.terminals,
        io::Error::other("sensitive runtime detail"),
        move |verbose| {
            logging_transcript
                .borrow_mut()
                .push(DispatchCall::InitializeLogging { verbose });
            match logging_behavior {
                LoggingBehavior::Succeed => Ok(()),
                LoggingBehavior::Fail => Err(Box::new(io::Error::other(
                    "synthetic logging initialization failure",
                )) as InitializationError),
            }
        },
    );
    Observed::new(
        snapshot_result(result),
        DispatchState {
            calls: calls.borrow().clone(),
            stdout: stdout.state,
            stderr: stderr.state,
        },
    )
}

fn snapshot_result(result: Result<(), AppError>) -> ResultSnapshot {
    match result {
        Ok(()) => ResultSnapshot::Success,
        Err(error) => ResultSnapshot::Failure {
            variant: match &error {
                AppError::LoggingInitialization { .. } => ErrorVariant::LoggingInitialization,
                AppError::RuntimeInitialization { .. } => ErrorVariant::RuntimeInitialization,
                AppError::AuthLogin { .. } => ErrorVariant::AuthLogin,
                _ => ErrorVariant::Unexpected,
            },
            category: error.category(),
            exit_code: error.exit_code(),
            message: error.to_string(),
        },
    }
}

#![forbid(unsafe_code)]

use std::ffi::OsStr;
use std::io;
use std::process::ExitCode;

use clap::{Parser, error::ErrorKind};
#[cfg(test)]
use venmo_cli::cli::{AppError, write_error};
use venmo_cli::cli::{
    Cli, CliFailure, handle_runtime_initialization_failure, run, write_cli_failure,
    write_parse_error_json,
};

fn main() -> ExitCode {
    let args = std::env::args_os().collect::<Vec<_>>();
    let cli = match Cli::try_parse_from(&args) {
        Ok(cli) => cli,
        Err(error) => return render_parse_error(error, json_intent(&args)),
    };
    let stdout = io::stdout();
    let stderr = io::stderr();
    let mut stdout = stdout.lock();
    let mut stderr = stderr.lock();

    let result = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime.block_on(run(cli, &mut stdout, &mut stderr)),
        Err(source) => handle_runtime_initialization_failure(cli, &mut stdout, &mut stderr, source),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => render_failure(&mut stderr, &failure),
    }
}

fn json_intent(args: &[impl AsRef<OsStr>]) -> bool {
    args.iter()
        .skip(1)
        .map(AsRef::as_ref)
        .take_while(|argument| *argument != OsStr::new("--"))
        .any(|argument| argument == OsStr::new("--json"))
}

fn render_parse_error(error: clap::Error, json: bool) -> ExitCode {
    let kind = error.kind();
    if matches!(kind, ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) {
        let _ = error.print();
        return ExitCode::SUCCESS;
    }

    let exit_code = u8::try_from(error.exit_code()).unwrap_or(2);
    if json {
        let stderr = io::stderr();
        let mut stderr = stderr.lock();
        let message = error.to_string();
        let _ = write_parse_error_json(&mut stderr, message.trim_end(), clap_error_kind(kind));
    } else {
        let _ = error.print();
    }
    ExitCode::from(exit_code)
}

fn clap_error_kind(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::InvalidValue => "invalid_value",
        ErrorKind::UnknownArgument => "unknown_argument",
        ErrorKind::InvalidSubcommand => "invalid_subcommand",
        ErrorKind::NoEquals => "missing_equals",
        ErrorKind::ValueValidation => "value_validation",
        ErrorKind::TooManyValues => "too_many_values",
        ErrorKind::TooFewValues => "too_few_values",
        ErrorKind::WrongNumberOfValues => "wrong_number_of_values",
        ErrorKind::ArgumentConflict => "argument_conflict",
        ErrorKind::MissingRequiredArgument => "missing_required_argument",
        ErrorKind::MissingSubcommand => "missing_subcommand",
        _ => "invalid_arguments",
    }
}

fn render_failure(stderr: &mut impl io::Write, failure: &CliFailure) -> ExitCode {
    let exit_code = failure.exit_code();
    let _ = write_cli_failure(stderr, failure);
    ExitCode::from(exit_code)
}

#[cfg(test)]
fn render_error(stderr: &mut impl io::Write, error: &AppError) -> ExitCode {
    let exit_code = error.exit_code();
    let _ = write_error(stderr, error);
    ExitCode::from(exit_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailingWriter;

    impl io::Write for FailingWriter {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("synthetic output failure"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("synthetic output failure"))
        }
    }

    #[test]
    fn error_exit_code_survives_stderr_failure() {
        let exit = render_error(
            &mut FailingWriter,
            &AppError::FinancialWriteInterruptedUnknown,
        );

        assert_eq!(exit, ExitCode::from(3));
    }
}

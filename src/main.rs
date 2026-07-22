#![forbid(unsafe_code)]

use std::io;
use std::process::ExitCode;

use clap::Parser;
#[cfg(test)]
use venmo_cli::cli::{AppError, write_error};
use venmo_cli::cli::{
    Cli, CliFailure, handle_runtime_initialization_failure, run, write_cli_failure,
};

fn main() -> ExitCode {
    let cli = Cli::parse();
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

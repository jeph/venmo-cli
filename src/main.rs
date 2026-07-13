#![forbid(unsafe_code)]

use std::io;
use std::process::ExitCode;

use clap::Parser;
use venmo_cli::cli::{Cli, dispatch, output};
use venmo_cli::error::AppError;
use venmo_cli::infrastructure::logging;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let stdout = io::stdout();
    let stderr = io::stderr();
    let mut stdout = stdout.lock();
    let mut stderr = stderr.lock();

    if let Err(source) = logging::initialize(cli.verbose) {
        return render_error(&mut stderr, &AppError::LoggingInitialization { source });
    }

    match dispatch::run(cli, &mut stdout, &mut stderr) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => render_error(&mut stderr, &error),
    }
}

fn render_error(stderr: &mut impl io::Write, error: &AppError) -> ExitCode {
    let exit_code = error.exit_code();
    let _ = output::write_error(stderr, error);
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

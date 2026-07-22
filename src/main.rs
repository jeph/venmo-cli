#![forbid(unsafe_code)]

use std::io;
use std::process::ExitCode;

use clap::Parser;
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
    fn failure_exit_code_survives_stderr_failure() -> Result<(), Box<dyn std::error::Error>> {
        let cli = Cli::try_parse_from(["venmo", "--json", "balance"])?;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let failure = handle_runtime_initialization_failure(
            cli,
            &mut stdout,
            &mut stderr,
            io::Error::other("synthetic runtime failure"),
        )
        .err()
        .ok_or("expected runtime initialization failure")?;
        let expected = ExitCode::from(failure.exit_code());

        let exit = render_failure(&mut FailingWriter, &failure);

        assert_eq!(expected, ExitCode::from(1));
        assert_eq!(exit, expected);
        Ok(())
    }
}

use std::io::{self, Write};

use clap::CommandFactory;
use clap_complete::{generate, shells};

use super::args::{Cli, CompletionShell};

pub fn write<W: Write>(shell: CompletionShell, writer: &mut W) -> io::Result<()> {
    let mut command = Cli::command();
    let mut output = Vec::new();

    match shell {
        CompletionShell::Bash => generate(shells::Bash, &mut command, "venmo", &mut output),
        CompletionShell::Zsh => generate(shells::Zsh, &mut command, "venmo", &mut output),
        CompletionShell::Fish => generate(shells::Fish, &mut command, "venmo", &mut output),
        CompletionShell::PowerShell => {
            generate(shells::PowerShell, &mut command, "venmo", &mut output)
        }
    }

    writer.write_all(&output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::test_support::Observed;

    #[derive(Debug, Default, Eq, PartialEq)]
    struct WriterState {
        attempted_bytes: Vec<u8>,
        write_count: u32,
        flush_count: u32,
    }

    struct FailingWriter {
        state: WriterState,
    }

    impl Write for FailingWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.state.write_count += 1;
            self.state.attempted_bytes.extend_from_slice(buffer);
            Err(io::Error::other("synthetic completion output failure"))
        }

        fn flush(&mut self) -> io::Result<()> {
            self.state.flush_count += 1;
            Ok(())
        }
    }

    #[derive(Debug, Eq, PartialEq)]
    enum ResultSnapshot {
        Success,
        Failure {
            kind: io::ErrorKind,
            message: String,
        },
    }

    #[test]
    fn every_supported_shell_has_an_exact_completion_snapshot() -> io::Result<()> {
        for (snapshot_name, shell) in [
            ("bash", CompletionShell::Bash),
            ("zsh", CompletionShell::Zsh),
            ("fish", CompletionShell::Fish),
            ("powershell", CompletionShell::PowerShell),
        ] {
            let mut output = Vec::new();

            write(shell, &mut output)?;
            let output = String::from_utf8(output).map_err(io::Error::other)?;

            insta::assert_snapshot!(snapshot_name, output);
        }

        Ok(())
    }

    #[test]
    fn every_shell_preserves_the_failing_writer_outcome_and_complete_state() -> io::Result<()> {
        for shell in [
            CompletionShell::Bash,
            CompletionShell::Zsh,
            CompletionShell::Fish,
            CompletionShell::PowerShell,
        ] {
            let initial_state = WriterState::default();
            let expected = Observed::new(
                ResultSnapshot::Failure {
                    kind: io::ErrorKind::Other,
                    message: "synthetic completion output failure".to_owned(),
                },
                WriterState {
                    attempted_bytes: completion_output(shell)?,
                    write_count: 1,
                    flush_count: 0,
                },
            );
            let mut writer = FailingWriter {
                state: initial_state,
            };

            let result = write(shell, &mut writer);
            let observed = Observed::new(snapshot_result(result), writer.state);

            assert_eq!(observed, expected, "shell: {shell:?}");
        }
        Ok(())
    }

    fn completion_output(shell: CompletionShell) -> io::Result<Vec<u8>> {
        let mut output = Vec::new();
        write(shell, &mut output)?;
        Ok(output)
    }

    fn snapshot_result(result: io::Result<()>) -> ResultSnapshot {
        match result {
            Ok(()) => ResultSnapshot::Success,
            Err(error) => ResultSnapshot::Failure {
                kind: error.kind(),
                message: error.to_string(),
            },
        }
    }
}

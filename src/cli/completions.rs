use std::io::{self, Write};

use clap::CommandFactory;
use clap_complete::{generate, shells};

use super::args::{Cli, CompletionShell};

pub fn write<W: Write>(shell: CompletionShell, writer: &mut W) -> io::Result<()> {
    let mut command = Cli::command();

    match shell {
        CompletionShell::Bash => generate(shells::Bash, &mut command, "venmo", writer),
        CompletionShell::Zsh => generate(shells::Zsh, &mut command, "venmo", writer),
        CompletionShell::Fish => generate(shells::Fish, &mut command, "venmo", writer),
        CompletionShell::PowerShell => {
            generate(shells::PowerShell, &mut command, "venmo", writer);
        }
    }

    Ok(())
}

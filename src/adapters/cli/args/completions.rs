use clap::{Args, ValueEnum};

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct CompletionsArgs {
    /// Shell whose completion source should be generated.
    #[arg(value_enum)]
    pub shell: CompletionShell,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum CompletionShell {
    Bash,
    Zsh,
    Fish,
    #[value(name = "powershell")]
    PowerShell,
}

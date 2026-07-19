use clap::{Args, Subcommand, ValueEnum};

use crate::features::people::RecipientInput;
use crate::shared::{Money, Note, Visibility};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum VisibilityArg {
    #[default]
    Private,
    Friends,
    Public,
}

impl From<VisibilityArg> for Visibility {
    fn from(value: VisibilityArg) -> Self {
        match value {
            VisibilityArg::Private => Self::Private,
            VisibilityArg::Friends => Self::Friends,
            VisibilityArg::Public => Self::Public,
        }
    }
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct PayArgs {
    #[command(subcommand)]
    pub operation: PayOperation,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum PayOperation {
    /// List payment methods and copyable IDs.
    Methods,

    /// Pay one person.
    User(PayUserArgs),
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
#[command(
    after_long_help = "Financial exit code 3 means the payment outcome must be verified independently. Do not retry; check `activity list` and the official Venmo app."
)]
pub struct PayUserArgs {
    /// Exact username with an optional leading @.
    #[arg(value_name = "USERNAME")]
    pub recipient: RecipientInput,

    /// Positive USD amount with at most two fractional digits.
    #[arg(value_name = "AMOUNT")]
    pub amount: Money,

    /// Non-empty payment note.
    #[arg(long, value_name = "NOTE")]
    pub note: Note,

    /// Visibility of the created payment.
    #[arg(long, value_enum, default_value_t = VisibilityArg::Private)]
    pub visibility: VisibilityArg,

    /// Skip only the final default-No confirmation.
    #[arg(long)]
    pub yes: bool,
}

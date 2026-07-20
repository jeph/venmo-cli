use clap::{Args, Subcommand, ValueEnum};

use crate::features::people::RecipientInput;
use crate::features::wallet::PaymentMethodId;
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
    /// List payment options and copyable source IDs.
    Options,

    /// Pay one person.
    User(PayUserArgs),
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
#[command(
    after_long_help = "Without `--source`, payment uses a peer-eligible Venmo balance source when the available balance covers the amount; otherwise it selects the unique default or sole eligible external peer method. `--source` submits that exact peer-eligible balance, bank, or card ID and never silently substitutes another source. Financial exit code 3 means the payment outcome must be verified independently. Do not retry; check `activity list` and the official Venmo app."
)]
pub struct PayUserArgs {
    /// Exact username with an optional leading @.
    #[arg(value_name = "USERNAME")]
    pub recipient: RecipientInput,

    /// Positive USD amount with at most two fractional digits.
    #[arg(value_name = "AMOUNT")]
    pub amount: Money,

    /// Non-empty payment note.
    #[arg(value_name = "NOTE")]
    pub note: Note,

    /// Peer-eligible source ID from `venmo pay options`.
    #[arg(long, value_name = "SOURCE_ID")]
    pub source: Option<PaymentMethodId>,

    /// Visibility of the created payment.
    #[arg(long, value_enum, default_value_t = VisibilityArg::Private)]
    pub visibility: VisibilityArg,

    /// Skip only the final default-No confirmation.
    #[arg(long)]
    pub yes: bool,
}

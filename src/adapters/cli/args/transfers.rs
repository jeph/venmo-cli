use clap::{Args, Subcommand, ValueEnum};

use crate::features::transfers::{TransferInstrumentId, TransferSpeed};
use crate::shared::Money;

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct TransferArgs {
    #[command(subcommand)]
    pub operation: TransferOperation,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum TransferOperation {
    /// Show current inbound and outbound transfer eligibility without moving money.
    Options,

    /// Transfer Venmo balance to the unique selected standard bank destination.
    Out(TransferOutArgs),

    /// Add money from an eligible standard bank source to the Venmo balance.
    In(TransferInArgs),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum TransferSpeedArg {
    Standard,
}

impl From<TransferSpeedArg> for TransferSpeed {
    fn from(value: TransferSpeedArg) -> Self {
        match value {
            TransferSpeedArg::Standard => Self::Standard,
        }
    }
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
#[command(
    after_long_help = "Confirmation defaults to No. Financial exit code 3 means the transfer outcome must be verified independently. Do not retry; check `activity list` and the official Venmo app. This subcommand supports only standard bank cash-out; instant, debit, manual destination selection, and challenge continuation remain unavailable."
)]
pub struct TransferOutArgs {
    /// Positive USD amount with at most two fractional digits.
    #[arg(value_name = "AMOUNT")]
    pub amount: Money,

    /// Transfer speed; defaults to standard, the only currently accepted value.
    #[arg(long, value_enum, default_value = "standard")]
    pub speed: TransferSpeedArg,

    /// Skip only the final default-No confirmation.
    #[arg(long)]
    pub yes: bool,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
#[command(
    after_long_help = "Confirmation defaults to No. Financial exit code 3 means the add-funds outcome must be verified independently. Do not retry; check `activity list` and the official Venmo app. The source must be a currently eligible standard bank source. When --source is omitted, exactly one source must be marked as default. Instant and debit add-funds remain unavailable."
)]
pub struct TransferInArgs {
    /// Positive USD amount with at most two fractional digits.
    #[arg(value_name = "AMOUNT")]
    pub amount: Money,

    /// Exact eligible source ID shown by `venmo transfer options`.
    #[arg(long, value_name = "SOURCE_ID")]
    pub source: Option<TransferInstrumentId>,

    /// Skip only the final default-No confirmation.
    #[arg(long)]
    pub yes: bool,
}

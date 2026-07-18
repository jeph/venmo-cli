use clap::{Args, Subcommand, ValueEnum};

use crate::features::transfers::TransferSpeed;
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
    after_long_help = "Confirmation defaults to No. Financial exit code 3 means the transfer outcome must be verified independently. Do not retry; check `activity list` and the official Venmo app. Only standard bank cash-out is supported; inbound, instant, debit, manual destination selection, and challenge continuation remain unavailable."
)]
pub struct TransferOutArgs {
    /// Positive USD amount with at most two fractional digits.
    #[arg(value_name = "AMOUNT")]
    pub amount: Money,

    /// Transfer speed; only standard is currently accepted.
    #[arg(long, value_enum)]
    pub speed: TransferSpeedArg,

    /// Skip only the final default-No confirmation.
    #[arg(long)]
    pub yes: bool,
}

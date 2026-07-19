use std::str::FromStr;

use clap::{Args, Subcommand, ValueEnum};

use crate::features::transfers::{TransferOutAmount, TransferSpeed};
use crate::shared::{Money, MoneyParseError};

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct TransferArgs {
    #[command(subcommand)]
    pub operation: TransferOperation,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum TransferOperation {
    /// Show current outbound transfer eligibility without moving money.
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferAmountArg {
    Exact(Money),
    All,
}

impl FromStr for TransferAmountArg {
    type Err = MoneyParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value == "all" {
            Ok(Self::All)
        } else {
            value.parse().map(Self::Exact)
        }
    }
}

impl From<TransferAmountArg> for TransferOutAmount {
    fn from(value: TransferAmountArg) -> Self {
        match value {
            TransferAmountArg::Exact(amount) => Self::Exact(amount),
            TransferAmountArg::All => Self::AllAvailable,
        }
    }
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
#[command(
    after_long_help = "Confirmation defaults to No. Exact lowercase `all` resolves once from a fresh available-balance snapshot, excludes on-hold funds, and is not an atomic account drain. Financial exit code 3 means the transfer outcome must be verified independently. Do not retry; check `activity list` and the official Venmo app. This subcommand supports only standard bank cash-out; instant, debit, manual destination selection, and challenge continuation remain unavailable."
)]
pub struct TransferOutArgs {
    /// Positive USD amount, or `all` to use the fresh available-balance snapshot.
    #[arg(value_name = "AMOUNT_OR_ALL")]
    pub amount: TransferAmountArg,

    /// Transfer speed; defaults to standard, the only currently accepted value.
    #[arg(long, value_enum, default_value = "standard")]
    pub speed: TransferSpeedArg,

    /// Skip only the final default-No confirmation.
    #[arg(long)]
    pub yes: bool,
}

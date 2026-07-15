use clap::{Args, ValueEnum};

use crate::features::people::RecipientInput;
use crate::features::requests::RequestId;
use crate::features::wallet::PaymentMethodId;
use crate::shared::{Money, Note, Visibility};

use super::parsers::RedactedRequestIdParser;

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
#[command(
    after_long_help = "Financial exit code 3 means the payment outcome must be verified independently. Do not retry; check `activity list` and the official Venmo app."
)]
pub struct PayArgs {
    /// Exact @username or positive numeric Venmo user ID.
    #[arg(value_name = "RECIPIENT")]
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

    /// Preferred external/backup method ID; fees do not block selection and balance may be used first.
    #[arg(long, value_name = "METHOD_ID")]
    pub from: Option<PaymentMethodId>,

    /// Skip only the final default-No confirmation.
    #[arg(long)]
    pub yes: bool,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
#[command(
    after_long_help = "Direct request creation writes immediately after validation. It does not prompt and has no `--yes` option. Financial exit code 3 means the request outcome must be verified independently. Do not retry; check `requests list` and the official Venmo app."
)]
pub struct RequestArgs {
    /// Exact @username or positive numeric Venmo user ID.
    #[arg(value_name = "RECIPIENT")]
    pub recipient: RecipientInput,

    /// Positive USD amount with at most two fractional digits.
    #[arg(value_name = "AMOUNT")]
    pub amount: Money,

    /// Non-empty request note.
    #[arg(long, value_name = "NOTE")]
    pub note: Note,

    /// Visibility of the created request.
    #[arg(long, value_enum, default_value_t = VisibilityArg::Private)]
    pub visibility: VisibilityArg,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
#[command(
    after_long_help = "Accepting sends the exact requested amount to the requester and requires an available Venmo balance snapshot covering it. The approval submits no external funding-method identifier, but neither the snapshot nor the response proves the final funding source or fee. Confirmation defaults to No. Financial exit code 3 means the acceptance outcome must be verified independently. Do not retry; check `activity list`, `requests list`, and the official Venmo app."
)]
pub struct AcceptArgs {
    /// Canonical incoming request ID.
    #[arg(value_name = "REQUEST_ID", value_parser = RedactedRequestIdParser)]
    pub request_id: RequestId,

    /// Skip only the final default-No confirmation.
    #[arg(long)]
    pub yes: bool,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
#[command(
    after_long_help = "Declining displays the authoritative request plan and then writes immediately without pausing for confirmation. It sends no money and has no `--yes` option. Exit code 3 means the request state must be verified independently. Do not retry; check `requests list` and the official Venmo app."
)]
pub struct DeclineArgs {
    /// Canonical incoming request ID.
    #[arg(value_name = "REQUEST_ID", value_parser = RedactedRequestIdParser)]
    pub request_id: RequestId,
}

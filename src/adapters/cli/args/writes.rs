use clap::Args;

use crate::features::people::RecipientInput;
use crate::features::requests::RequestId;
use crate::features::wallet::PaymentMethodId;
use crate::shared::{Money, Note};

use super::parsers::RedactedRequestIdParser;

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

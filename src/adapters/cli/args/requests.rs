use clap::{Args, Subcommand, ValueEnum};

use crate::features::people::RecipientInput;
use crate::features::requests::{RequestDirectionFilter, RequestId, RequestsBefore};
use crate::shared::{Limit, Money, Note};

use super::parsers::{RedactedRequestIdParser, RedactedRequestsBeforeParser};
use super::writes::VisibilityArg;

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct RequestsArgs {
    #[command(subcommand)]
    pub operation: RequestsOperation,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum RequestsOperation {
    /// List pending requests involving the active account.
    List(RequestsListArgs),

    /// Create a request immediately.
    Create(RequestArgs),

    /// Accept an incoming request after authoritative validation and confirmation.
    Accept(AcceptArgs),

    /// Decline an incoming request without sending money.
    Decline(DeclineArgs),

    /// Show information about one open request.
    Info(RequestInfoArgs),
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct RequestsListArgs {
    /// Filter by the request's direction.
    #[arg(long, value_enum, default_value_t = RequestDirectionArg::All)]
    pub direction: RequestDirectionArg,

    /// Server request page size before local direction filtering.
    #[arg(long, value_name = "N", default_value = "10")]
    pub limit: Limit,

    /// Fetch pending requests before this endpoint-native token.
    #[arg(long, value_name = "TOKEN", value_parser = RedactedRequestsBeforeParser)]
    pub before: Option<RequestsBefore>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum RequestDirectionArg {
    #[default]
    All,
    Incoming,
    Outgoing,
}

impl From<RequestDirectionArg> for RequestDirectionFilter {
    fn from(value: RequestDirectionArg) -> Self {
        match value {
            RequestDirectionArg::All => Self::All,
            RequestDirectionArg::Incoming => Self::Incoming,
            RequestDirectionArg::Outgoing => Self::Outgoing,
        }
    }
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
#[command(
    after_long_help = "Direct request creation writes immediately after validation. It does not prompt and has no `--yes` option. Financial exit code 3 means the request outcome must be verified independently. Do not retry; check `requests list` and the official Venmo app."
)]
pub struct RequestArgs {
    /// Exact username with an optional leading @.
    #[arg(value_name = "USERNAME")]
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
    after_long_help = "Declining sends no money. Confirmation defaults to No and requires both stdin and stderr to be terminals unless `--yes` is supplied. Exit code 3 means the request state must be verified independently. Do not retry; check `requests list` and the official Venmo app."
)]
pub struct DeclineArgs {
    /// Canonical incoming request ID.
    #[arg(value_name = "REQUEST_ID", value_parser = RedactedRequestIdParser)]
    pub request_id: RequestId,

    /// Skip only the final default-No confirmation.
    #[arg(long)]
    pub yes: bool,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct RequestInfoArgs {
    /// Canonical request ID.
    #[arg(value_name = "REQUEST_ID")]
    pub request_id: RequestId,
}

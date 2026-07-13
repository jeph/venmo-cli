use std::ffi::OsStr;
use std::str::FromStr;

use clap::builder::TypedValueParser;
use clap::error::ErrorKind;
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::domain::{
    ActivityBeforeId, ActivityId, Limit, Money, Note, Offset, PaymentMethodId, RecipientInput,
    RequestDirectionFilter, RequestId, RequestsBefore, UserSearchQuery,
};

#[derive(Clone, Debug, Eq, Parser, PartialEq)]
#[command(
    name = "venmo",
    version,
    about = "An unofficial Venmo command-line client",
    long_about = None,
    after_long_help = "Financial exit code 3 means the write outcome must be verified independently. Do not retry; check `activity list`, `requests list`, and the official Venmo app."
)]
pub struct Cli {
    /// Write redacted diagnostics to stderr.
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum Command {
    /// Validate and manage the stored Venmo bearer token.
    Auth(AuthArgs),

    /// Pay one person.
    Pay(PayArgs),

    /// Create a request immediately.
    Request(RequestArgs),

    /// Candidate incoming-request acceptance; unavailable pending live validation.
    Accept(AcceptArgs),

    /// Candidate incoming-request decline; unavailable pending live validation.
    Decline(DeclineArgs),

    /// Inspect friends of the active account.
    Friends(FriendsArgs),

    /// Find Venmo users.
    Users(UsersArgs),

    /// Inspect available payment methods.
    PaymentMethods(PaymentMethodsArgs),

    /// Show the Venmo wallet balance.
    Balance,

    /// Inspect account activity.
    Activity(ActivityArgs),

    /// Inspect pending requests.
    Requests(RequestsArgs),

    /// Run read-only diagnostics.
    Doctor,

    /// Generate shell completion source.
    Completions(CompletionsArgs),
}

impl Command {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Auth(_) => "auth",
            Self::Pay(_) => "pay",
            Self::Request(_) => "request",
            Self::Accept(_) => "accept",
            Self::Decline(_) => "decline",
            Self::Friends(_) => "friends",
            Self::Users(_) => "users",
            Self::PaymentMethods(_) => "payment-methods",
            Self::Balance => "balance",
            Self::Activity(_) => "activity",
            Self::Requests(_) => "requests",
            Self::Doctor => "doctor",
            Self::Completions(_) => "completions",
        }
    }
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub operation: AuthOperation,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum AuthOperation {
    /// Authenticate with a trusted device and password/SMS OTP, or import a bearer token.
    Login(LoginArgs),

    /// Replace an expired token using the stored trusted device and password/SMS OTP.
    #[command(
        after_long_help = "Uses the device ID from the one readable stored credential and never prompts for or prints it. Account identifier, password, and an SMS code for the exact recognized challenge are entered only through interactive prompts. This performs a full password login, not a token refresh."
    )]
    Reauthenticate,

    /// Remove the locally stored credential.
    Logout(LogoutArgs),

    /// Validate the credential and show the active account.
    Status,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
#[command(
    after_long_help = "Password login requires a v_id/device ID that Venmo already trusts. Every credential value is entered through an interactive prompt; never put a device ID, password, OTP, or bearer token in command arguments."
)]
pub struct LoginArgs {
    /// Import a bearer token and prompt for its matching device ID when needed.
    #[arg(long)]
    pub token: bool,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct LogoutArgs {
    /// Attempt to revoke the bearer token before deleting it locally.
    #[arg(long)]
    pub revoke: bool,
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

    /// Preferred external/backup method ID; Venmo balance may be used first.
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
    after_long_help = "Accepting sends the exact requested amount to the requester and requires enough available Venmo balance to cover it. Confirmation defaults to No. Financial exit code 3 means the acceptance outcome must be verified independently. Do not retry; check `activity list`, `requests list`, and the official Venmo app."
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

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct FriendsArgs {
    #[command(subcommand)]
    pub operation: FriendsOperation,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum FriendsOperation {
    /// List friends of the active account.
    List(FriendsListArgs),
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct FriendsListArgs {
    /// Server request page size.
    #[arg(long, value_name = "N", default_value = "10")]
    pub limit: Limit,

    /// Zero-based friend-list offset.
    #[arg(long, value_name = "N", default_value = "0")]
    pub offset: Offset,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct UsersArgs {
    #[command(subcommand)]
    pub operation: UsersOperation,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum UsersOperation {
    /// Search for Venmo users.
    Search(UserSearchArgs),
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct UserSearchArgs {
    /// Search text.
    #[arg(value_name = "QUERY")]
    pub query: UserSearchQuery,

    /// Server request page size.
    #[arg(long, value_name = "N", default_value = "10")]
    pub limit: Limit,

    /// Zero-based user-search offset.
    #[arg(long, value_name = "N", default_value = "0")]
    pub offset: Offset,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct PaymentMethodsArgs {
    #[command(subcommand)]
    pub operation: PaymentMethodsOperation,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum PaymentMethodsOperation {
    /// List payment methods and copyable IDs.
    List,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct ActivityArgs {
    #[command(subcommand)]
    pub operation: ActivityOperation,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum ActivityOperation {
    /// List recent activity.
    List(ActivityListArgs),

    /// Show one activity record.
    Show(ActivityShowArgs),
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct ActivityListArgs {
    /// Server request page size.
    #[arg(long, value_name = "N", default_value = "10")]
    pub limit: Limit,

    /// Fetch activity before this endpoint-native token.
    #[arg(long, value_name = "TOKEN", value_parser = RedactedActivityBeforeIdParser)]
    pub before_id: Option<ActivityBeforeId>,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct ActivityShowArgs {
    /// Canonical activity ID.
    #[arg(value_name = "ACTIVITY_ID")]
    pub activity_id: ActivityId,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct RequestsArgs {
    #[command(subcommand)]
    pub operation: RequestsOperation,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum RequestsOperation {
    /// List pending requests involving the active account.
    List(RequestsListArgs),
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

#[derive(Clone)]
struct RedactedActivityBeforeIdParser;

impl TypedValueParser for RedactedActivityBeforeIdParser {
    type Value = ActivityBeforeId;

    fn parse_ref(
        &self,
        command: &clap::Command,
        _argument: Option<&clap::Arg>,
        value: &OsStr,
    ) -> Result<Self::Value, clap::Error> {
        let parsed = value.to_str().ok_or_else(|| {
            continuation_parse_error(command, "before-id", "token is not valid UTF-8")
        })?;
        ActivityBeforeId::from_str(parsed)
            .map_err(|source| continuation_parse_error(command, "before-id", &source))
    }
}

#[derive(Clone)]
struct RedactedRequestsBeforeParser;

impl TypedValueParser for RedactedRequestsBeforeParser {
    type Value = RequestsBefore;

    fn parse_ref(
        &self,
        command: &clap::Command,
        _argument: Option<&clap::Arg>,
        value: &OsStr,
    ) -> Result<Self::Value, clap::Error> {
        let parsed = value.to_str().ok_or_else(|| {
            continuation_parse_error(command, "before", "token is not valid UTF-8")
        })?;
        RequestsBefore::from_str(parsed)
            .map_err(|source| continuation_parse_error(command, "before", &source))
    }
}

#[derive(Clone)]
struct RedactedRequestIdParser;

impl TypedValueParser for RedactedRequestIdParser {
    type Value = RequestId;

    fn parse_ref(
        &self,
        command: &clap::Command,
        _argument: Option<&clap::Arg>,
        value: &OsStr,
    ) -> Result<Self::Value, clap::Error> {
        let parsed = value
            .to_str()
            .ok_or_else(|| request_id_parse_error(command, "request ID is not valid UTF-8"))?;
        RequestId::from_str(parsed).map_err(|source| request_id_parse_error(command, &source))
    }
}

fn request_id_parse_error(command: &clap::Command, source: impl std::fmt::Display) -> clap::Error {
    let mut command = command.clone();
    command.error(
        ErrorKind::ValueValidation,
        format_args!("invalid request ID: {source}"),
    )
}

fn continuation_parse_error(
    command: &clap::Command,
    argument: &str,
    source: impl std::fmt::Display,
) -> clap::Error {
    let mut command = command.clone();
    command.error(
        ErrorKind::ValueValidation,
        format_args!("invalid {argument} continuation token: {source}"),
    )
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

use clap::{Parser, Subcommand};

mod auth;
mod completions;
mod parsers;
mod reads;
mod writes;

pub use auth::{AuthArgs, AuthOperation, LoginArgs, LogoutArgs};
pub use completions::{CompletionShell, CompletionsArgs};
pub use reads::{
    ActivityArgs, ActivityListArgs, ActivityOperation, ActivityShowArgs, FriendsArgs,
    FriendsListArgs, FriendsOperation, PaymentMethodsArgs, PaymentMethodsOperation,
    RequestDirectionArg, RequestsArgs, RequestsListArgs, RequestsOperation, UserSearchArgs,
    UsersArgs, UsersOperation,
};
pub use writes::{AcceptArgs, DeclineArgs, PayArgs, RequestArgs};

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

    /// Accept an incoming request after authoritative validation and confirmation.
    Accept(AcceptArgs),

    /// Decline an incoming request without sending money.
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

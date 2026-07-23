use clap::{Parser, Subcommand};

const CLI_VERSION: &str = match option_env!("VENMO_BUILD_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

mod auth;
mod friends;
mod parsers;
mod reads;
mod requests;
mod transfers;
mod writes;

pub use auth::{AuthArgs, AuthOperation};
pub use friends::{
    FriendAddArgs, FriendRemoveArgs, FriendsArgs, FriendsListArgs, FriendsOperation,
};
pub use reads::{
    ActivityArgs, ActivityCommentAddArgs, ActivityCommentListArgs, ActivityCommentRemoveArgs,
    ActivityCommentsArgs, ActivityCommentsOperation, ActivityInfoArgs, ActivityLikeArgs,
    ActivityListArgs, ActivityOperation, ActivityUnlikeArgs, UserInfoArgs, UserSearchArgs,
    UsersArgs, UsersOperation,
};
pub use requests::{
    AcceptArgs, CancelArgs, DeclineArgs, RequestArgs, RequestDirectionArg, RequestInfoArgs,
    RequestsArgs, RequestsListArgs, RequestsOperation,
};
pub use transfers::{
    TransferAmountArg, TransferArgs, TransferOperation, TransferOutArgs, TransferSpeedArg,
};
pub use writes::{PayArgs, PayOperation, PayUserArgs, VisibilityArg};

#[derive(Clone, Debug, Eq, Parser, PartialEq)]
#[command(
    name = "venmo",
    bin_name = "venmo",
    version = CLI_VERSION,
    about = "An unofficial Venmo command-line client",
    long_about = None,
    after_long_help = "Financial exit code 3 means the write outcome must be verified independently. Do not retry; check `activity list`, `requests list`, and the official Venmo app."
)]
pub struct Cli {
    /// Write bounded debug diagnostics to stderr.
    #[arg(long, global = true)]
    pub debug: bool,

    /// Write the final command result as compact JSON.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum Command {
    /// Validate and manage the stored Venmo bearer token.
    Auth(AuthArgs),

    /// Pay users or inspect payment options.
    Pay(PayArgs),

    /// Inspect visible friend lists and manage active-account friendships.
    Friends(FriendsArgs),

    /// Find Venmo users.
    Users(UsersArgs),

    /// Show the Venmo wallet balance.
    Balance,

    /// Inspect account activity.
    Activity(ActivityArgs),

    /// Inspect and manage requests.
    Requests(RequestsArgs),

    /// Inspect transfer eligibility or perform confirmed standard-bank transfers.
    Transfer(TransferArgs),
}

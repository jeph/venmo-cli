use clap::{Args, Subcommand};

use crate::shared::{Limit, Offset, Username};

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct FriendsArgs {
    #[command(subcommand)]
    pub operation: FriendsOperation,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum FriendsOperation {
    /// List visible friends of the active account or another personal profile.
    List(FriendsListArgs),

    /// Send or accept a friend request according to the current relationship.
    Add(FriendAddArgs),

    /// Remove a friend or cancel an outgoing friend request.
    Remove(FriendRemoveArgs),
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct FriendsListArgs {
    /// Exact username whose visible friends should be listed.
    #[arg(long, value_name = "USERNAME")]
    pub user: Option<Username>,

    /// Server request page size (maximum 50).
    #[arg(long, value_name = "N", default_value = "10")]
    pub limit: Limit,

    /// Zero-based friend-list offset.
    #[arg(long, value_name = "N", default_value = "0")]
    pub offset: Offset,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
#[command(
    after_long_help = "This command follows Venmo's native relationship semantics: it sends a request for a nonfriend and accepts an incoming request. It never retries automatically. Exit code 3 means the friendship outcome must be verified independently."
)]
pub struct FriendAddArgs {
    /// Exact username with an optional leading @.
    #[arg(value_name = "USERNAME")]
    pub username: Username,

    /// Skip only the final default-No confirmation.
    #[arg(long, conflicts_with = "dry_run")]
    pub yes: bool,

    /// Complete preflight and show details without changing the friendship.
    #[arg(long, conflicts_with = "yes")]
    pub dry_run: bool,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
#[command(
    after_long_help = "This command follows Venmo's native relationship semantics: it removes an established friend or cancels an outgoing request. It does not decline incoming requests and never retries automatically. Exit code 3 means the friendship outcome must be verified independently."
)]
pub struct FriendRemoveArgs {
    /// Exact username with an optional leading @.
    #[arg(value_name = "USERNAME")]
    pub username: Username,

    /// Skip only the final default-No confirmation.
    #[arg(long, conflicts_with = "dry_run")]
    pub yes: bool,

    /// Complete preflight and show details without changing the friendship.
    #[arg(long, conflicts_with = "yes")]
    pub dry_run: bool,
}

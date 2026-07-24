use clap::{Args, Subcommand};

use crate::features::activity::{
    ActivityBeforeId, ActivityCommentId, ActivityCommentMessage, ActivityId, ActivityReactionTarget,
};
use crate::features::people::UserSearchQuery;
use crate::shared::{Limit, Offset, Username};

use super::parsers::RedactedActivityBeforeIdParser;

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct UsersArgs {
    #[command(subcommand)]
    pub operation: UsersOperation,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum UsersOperation {
    /// Search for Venmo users.
    Search(UserSearchArgs),

    /// Show information about one Venmo user.
    Info(UserInfoArgs),
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct UserInfoArgs {
    /// Username with an optional leading @.
    #[arg(value_name = "USERNAME")]
    pub username: Username,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct UserSearchArgs {
    /// Username (with optional @) or multi-word search text.
    #[arg(value_name = "QUERY")]
    pub query: UserSearchQuery,

    /// Server request page size (maximum 50).
    #[arg(long, value_name = "N", default_value = "10")]
    pub limit: Limit,

    /// Zero-based user-search offset.
    #[arg(long, value_name = "N", default_value = "0")]
    pub offset: Offset,
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

    /// Show information about one activity record.
    Info(ActivityInfoArgs),

    /// List, add, or remove activity comments.
    Comments(ActivityCommentsArgs),

    /// List, add, or remove activity reactions.
    Reactions(ActivityReactionsArgs),
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct ActivityListArgs {
    /// Exact personal-profile username whose visible activity should be listed.
    #[arg(long, value_name = "USERNAME")]
    pub user: Option<Username>,

    /// Server request page size (maximum 50).
    #[arg(long, value_name = "N", default_value = "10")]
    pub limit: Limit,

    /// Fetch activity before this endpoint-native token.
    #[arg(long, value_name = "TOKEN", value_parser = RedactedActivityBeforeIdParser)]
    pub before_id: Option<ActivityBeforeId>,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct ActivityInfoArgs {
    /// Canonical activity ID.
    #[arg(value_name = "ACTIVITY_ID")]
    pub activity_id: ActivityId,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct ActivityCommentsArgs {
    #[command(subcommand)]
    pub operation: ActivityCommentsOperation,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum ActivityCommentsOperation {
    /// List activity comments.
    List(ActivityCommentListArgs),

    /// Add a comment to one activity record.
    Add(ActivityCommentAddArgs),

    /// Remove one comment by its canonical comment ID.
    Remove(ActivityCommentRemoveArgs),
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct ActivityCommentListArgs {
    /// Canonical activity ID.
    #[arg(value_name = "ACTIVITY_ID")]
    pub activity_id: ActivityId,

    /// Number of comments to display (maximum 50).
    #[arg(long, value_name = "N", default_value = "10")]
    pub limit: Limit,

    /// Zero-based offset into the complete embedded comment collection.
    #[arg(long, value_name = "N", default_value = "0")]
    pub offset: Offset,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
#[command(
    after_long_help = "Comments must contain non-whitespace text and cannot exceed 2000 characters. The command performs an authoritative activity preflight and never retries automatically. Exit code 3 means the comment outcome must be verified independently."
)]
pub struct ActivityCommentAddArgs {
    /// Canonical activity ID.
    #[arg(value_name = "ACTIVITY_ID")]
    pub activity_id: ActivityId,

    /// Comment text.
    #[arg(value_name = "MESSAGE")]
    pub message: ActivityCommentMessage,

    /// Skip only the final default-No confirmation.
    #[arg(long, conflicts_with = "dry_run")]
    pub yes: bool,

    /// Complete preflight and show details without adding the comment.
    #[arg(long, conflicts_with = "yes")]
    pub dry_run: bool,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
#[command(
    after_long_help = "Venmo authorizes removal from the comment ID alone. The CLI cannot preflight the parent activity, authorship, or comment text and cannot reconcile absence without an activity ID. The command never retries automatically. Exit code 3 means the removal outcome must be verified independently."
)]
pub struct ActivityCommentRemoveArgs {
    /// Canonical comment ID shown by `activity info`.
    #[arg(value_name = "COMMENT_ID")]
    pub comment_id: ActivityCommentId,

    /// Skip only the final default-No confirmation.
    #[arg(long, conflicts_with = "dry_run")]
    pub yes: bool,

    /// Show the exact deletion intent without removing the comment.
    #[arg(long, conflicts_with = "yes")]
    pub dry_run: bool,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct ActivityReactionsArgs {
    #[command(subcommand)]
    pub operation: ActivityReactionsOperation,
}

#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum ActivityReactionsOperation {
    /// List aggregate reactions and the authenticated account's reaction state.
    List(ActivityReactionListArgs),

    /// Add one Unicode emoji reaction or the special `like` target.
    Add(ActivityReactionAddArgs),

    /// Remove one Unicode emoji reaction or the special `like` target.
    Remove(ActivityReactionRemoveArgs),
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct ActivityReactionListArgs {
    /// Canonical activity ID.
    #[arg(value_name = "ACTIVITY_ID")]
    pub activity_id: ActivityId,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
#[command(
    after_long_help = "REACTION must be exact lowercase 'like' or one complete Unicode emoji grapheme, such as '🔥' or '❤️'. On the Venmo backend, 'like' and the ❤️ emoji are equivalent."
)]
pub struct ActivityReactionAddArgs {
    /// Canonical activity ID.
    #[arg(value_name = "ACTIVITY_ID")]
    pub activity_id: ActivityId,

    /// Exact emoji glyph or lowercase `like` target to add.
    #[arg(value_name = "REACTION")]
    pub reaction: ActivityReactionTarget,

    /// Skip only the final default-No confirmation.
    #[arg(long, conflicts_with = "dry_run")]
    pub yes: bool,

    /// Complete preflight and show details without adding the reaction.
    #[arg(long, conflicts_with = "yes")]
    pub dry_run: bool,
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
#[command(
    after_long_help = "REACTION must be exact lowercase 'like' or exactly match one Unicode emoji reaction already applied by the authenticated account. On the Venmo backend, 'like' and the ❤️ emoji are equivalent."
)]
pub struct ActivityReactionRemoveArgs {
    /// Canonical activity ID.
    #[arg(value_name = "ACTIVITY_ID")]
    pub activity_id: ActivityId,

    /// Exact emoji glyph or lowercase `like` target to remove.
    #[arg(value_name = "REACTION")]
    pub reaction: ActivityReactionTarget,

    /// Skip only the final default-No confirmation.
    #[arg(long, conflicts_with = "dry_run")]
    pub yes: bool,

    /// Complete preflight and show details without removing the reaction.
    #[arg(long, conflicts_with = "yes")]
    pub dry_run: bool,
}

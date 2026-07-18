use clap::{Args, Subcommand, ValueEnum};

use crate::features::activity::{ActivityBeforeId, ActivityId};
use crate::features::people::UserSearchQuery;
use crate::features::requests::{RequestDirectionFilter, RequestId, RequestsBefore};
use crate::shared::{Limit, Offset, Username};

use super::parsers::{RedactedActivityBeforeIdParser, RedactedRequestsBeforeParser};

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

    /// Show information about one activity record.
    Info(ActivityInfoArgs),
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
pub struct ActivityInfoArgs {
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

    /// Show information about one open request.
    Info(RequestInfoArgs),
}

#[derive(Args, Clone, Debug, Eq, PartialEq)]
pub struct RequestInfoArgs {
    /// Canonical request ID.
    #[arg(value_name = "REQUEST_ID")]
    pub request_id: RequestId,
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

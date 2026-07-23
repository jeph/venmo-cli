use super::args::{
    ActivityCommentsOperation, ActivityOperation, ActivityReactionsOperation, AuthOperation, Cli,
    Command, FriendsOperation, PayOperation, RequestsOperation, TransferOperation, UsersOperation,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputFormat {
    Human,
    Json,
}

impl Cli {
    #[must_use]
    pub const fn output_format(&self) -> OutputFormat {
        if self.json {
            OutputFormat::Json
        } else {
            OutputFormat::Human
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandId {
    AuthLogin,
    AuthLogout,
    AuthStatus,
    PayOptions,
    PayUser,
    FriendsList,
    FriendsAdd,
    FriendsRemove,
    UsersSearch,
    UsersInfo,
    Balance,
    ActivityList,
    ActivityInfo,
    ActivityLike,
    ActivityUnlike,
    ActivityCommentsList,
    ActivityCommentsAdd,
    ActivityCommentsRemove,
    ActivityReactionsList,
    ActivityReactionsAdd,
    ActivityReactionsRemove,
    RequestsList,
    RequestsCreate,
    RequestsAccept,
    RequestsDecline,
    RequestsCancel,
    RequestsInfo,
    TransferOptions,
    TransferOut,
}

impl CommandId {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AuthLogin => "auth.login",
            Self::AuthLogout => "auth.logout",
            Self::AuthStatus => "auth.status",
            Self::PayOptions => "pay.options",
            Self::PayUser => "pay.user",
            Self::FriendsList => "friends.list",
            Self::FriendsAdd => "friends.add",
            Self::FriendsRemove => "friends.remove",
            Self::UsersSearch => "users.search",
            Self::UsersInfo => "users.info",
            Self::Balance => "balance",
            Self::ActivityList => "activity.list",
            Self::ActivityInfo => "activity.info",
            Self::ActivityLike => "activity.like",
            Self::ActivityUnlike => "activity.unlike",
            Self::ActivityCommentsList => "activity.comments.list",
            Self::ActivityCommentsAdd => "activity.comments.add",
            Self::ActivityCommentsRemove => "activity.comments.remove",
            Self::ActivityReactionsList => "activity.reactions.list",
            Self::ActivityReactionsAdd => "activity.reactions.add",
            Self::ActivityReactionsRemove => "activity.reactions.remove",
            Self::RequestsList => "requests.list",
            Self::RequestsCreate => "requests.create",
            Self::RequestsAccept => "requests.accept",
            Self::RequestsDecline => "requests.decline",
            Self::RequestsCancel => "requests.cancel",
            Self::RequestsInfo => "requests.info",
            Self::TransferOptions => "transfer.options",
            Self::TransferOut => "transfer.out",
        }
    }
}

impl Command {
    #[must_use]
    pub const fn id(&self) -> CommandId {
        match self {
            Self::Auth(args) => match args.operation {
                AuthOperation::Login => CommandId::AuthLogin,
                AuthOperation::Logout => CommandId::AuthLogout,
                AuthOperation::Status => CommandId::AuthStatus,
            },
            Self::Pay(args) => match args.operation {
                PayOperation::Options => CommandId::PayOptions,
                PayOperation::User(_) => CommandId::PayUser,
            },
            Self::Friends(args) => match args.operation {
                FriendsOperation::List(_) => CommandId::FriendsList,
                FriendsOperation::Add(_) => CommandId::FriendsAdd,
                FriendsOperation::Remove(_) => CommandId::FriendsRemove,
            },
            Self::Users(args) => match args.operation {
                UsersOperation::Search(_) => CommandId::UsersSearch,
                UsersOperation::Info(_) => CommandId::UsersInfo,
            },
            Self::Balance => CommandId::Balance,
            Self::Activity(args) => match &args.operation {
                ActivityOperation::List(_) => CommandId::ActivityList,
                ActivityOperation::Info(_) => CommandId::ActivityInfo,
                ActivityOperation::Like(_) => CommandId::ActivityLike,
                ActivityOperation::Unlike(_) => CommandId::ActivityUnlike,
                ActivityOperation::Comments(args) => match args.operation {
                    ActivityCommentsOperation::List(_) => CommandId::ActivityCommentsList,
                    ActivityCommentsOperation::Add(_) => CommandId::ActivityCommentsAdd,
                    ActivityCommentsOperation::Remove(_) => CommandId::ActivityCommentsRemove,
                },
                ActivityOperation::Reactions(args) => match args.operation {
                    ActivityReactionsOperation::List(_) => CommandId::ActivityReactionsList,
                    ActivityReactionsOperation::Add(_) => CommandId::ActivityReactionsAdd,
                    ActivityReactionsOperation::Remove(_) => CommandId::ActivityReactionsRemove,
                },
            },
            Self::Requests(args) => match args.operation {
                RequestsOperation::List(_) => CommandId::RequestsList,
                RequestsOperation::Create(_) => CommandId::RequestsCreate,
                RequestsOperation::Accept(_) => CommandId::RequestsAccept,
                RequestsOperation::Decline(_) => CommandId::RequestsDecline,
                RequestsOperation::Cancel(_) => CommandId::RequestsCancel,
                RequestsOperation::Info(_) => CommandId::RequestsInfo,
            },
            Self::Transfer(args) => match args.operation {
                TransferOperation::Options => CommandId::TransferOptions,
                TransferOperation::Out(_) => CommandId::TransferOut,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn all_command_ids_are_unique_stable_dotted_names() {
        // Keep this list synchronized with `CommandId`; unlike the exhaustive `as_str` match, it
        // is responsible for checking uniqueness across variants.
        let ids = [
            CommandId::AuthLogin,
            CommandId::AuthLogout,
            CommandId::AuthStatus,
            CommandId::PayOptions,
            CommandId::PayUser,
            CommandId::FriendsList,
            CommandId::FriendsAdd,
            CommandId::FriendsRemove,
            CommandId::UsersSearch,
            CommandId::UsersInfo,
            CommandId::Balance,
            CommandId::ActivityList,
            CommandId::ActivityInfo,
            CommandId::ActivityLike,
            CommandId::ActivityUnlike,
            CommandId::ActivityCommentsList,
            CommandId::ActivityCommentsAdd,
            CommandId::ActivityCommentsRemove,
            CommandId::ActivityReactionsList,
            CommandId::ActivityReactionsAdd,
            CommandId::ActivityReactionsRemove,
            CommandId::RequestsList,
            CommandId::RequestsCreate,
            CommandId::RequestsAccept,
            CommandId::RequestsDecline,
            CommandId::RequestsCancel,
            CommandId::RequestsInfo,
            CommandId::TransferOptions,
            CommandId::TransferOut,
        ];
        let names = ids.map(CommandId::as_str);
        assert_eq!(names.len(), 29);
        assert_eq!(names.into_iter().collect::<BTreeSet<_>>().len(), 29);
        assert!(names.into_iter().all(|name| {
            !name.is_empty()
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'.')
        }));
    }
}

use serde::Deserialize;

use super::common::{PaginationDto, StringOrInteger};

#[derive(Debug, Deserialize)]
pub(crate) struct UsersEnvelope {
    pub data: UsersData,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum UsersData {
    Wrapped { users: Vec<UserDto> },
    Direct(Vec<UserDto>),
}

impl UsersData {
    pub(crate) fn into_users(self) -> Vec<UserDto> {
        match self {
            Self::Wrapped { users } | Self::Direct(users) => users,
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct UserDto {
    pub id: StringOrInteger,
    pub username: Option<String>,
    #[serde(default, alias = "displayName", alias = "name")]
    pub display_name: Option<String>,
    #[serde(default)]
    pub identity_type: Option<String>,
    #[serde(default)]
    pub is_payable: Option<bool>,
    #[serde(default)]
    pub friend_status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FriendMutationEnvelope {
    pub data: FriendMutationData,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FriendMutationData {
    pub user: UserDto,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UserEnvelope {
    pub data: UserData,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum UserData {
    Wrapped { user: UserDto },
    Direct(UserDto),
}

impl UserData {
    pub(crate) fn into_user(self) -> UserDto {
        match self {
            Self::Wrapped { user } | Self::Direct(user) => user,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct FriendsEnvelope {
    pub data: Vec<UserDto>,
    #[serde(default)]
    pub pagination: PaginationDto,
}

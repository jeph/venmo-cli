use super::{FriendshipStatus, User, UserSearchQuery};
use crate::shared::{AccessToken, ApiFailure, DeviceId, Limit, Offset, UserId};
use std::future::Future;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserSearchPageRequest {
    page_size: Limit,
    offset: Offset,
}

impl UserSearchPageRequest {
    #[must_use]
    pub const fn new(page_size: Limit, offset: Offset) -> Self {
        Self { page_size, offset }
    }

    #[must_use]
    pub const fn page_size(self) -> Limit {
        self.page_size
    }

    #[must_use]
    pub const fn offset(self) -> Offset {
        self.offset
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct UserSearchPage {
    users: Vec<User>,
    next_offset: Option<Offset>,
}

impl UserSearchPage {
    #[must_use]
    pub fn new(users: Vec<User>, next_offset: Option<Offset>) -> Self {
        Self { users, next_offset }
    }

    #[must_use]
    pub fn into_parts(self) -> (Vec<User>, Option<Offset>) {
        (self.users, self.next_offset)
    }
}

pub trait UserLookupApi {
    type Error: ApiFailure;

    fn user_by_id<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        user_id: &'a UserId,
    ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a;
}

pub trait UserSearchApi {
    type Error: ApiFailure;

    fn search_users<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        query: &'a UserSearchQuery,
        page: UserSearchPageRequest,
    ) -> impl Future<Output = Result<UserSearchPage, Self::Error>> + Send + 'a;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FriendsPageRequest {
    page_size: Limit,
    offset: Offset,
}

impl FriendsPageRequest {
    #[must_use]
    pub const fn new(page_size: Limit, offset: Offset) -> Self {
        Self { page_size, offset }
    }

    #[must_use]
    pub const fn page_size(self) -> Limit {
        self.page_size
    }

    #[must_use]
    pub const fn offset(self) -> Offset {
        self.offset
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct FriendsPage {
    users: Vec<User>,
    next_offset: Option<Offset>,
}

impl FriendsPage {
    #[must_use]
    pub fn new(users: Vec<User>, next_offset: Option<Offset>) -> Self {
        Self { users, next_offset }
    }

    #[must_use]
    pub fn into_parts(self) -> (Vec<User>, Option<Offset>) {
        (self.users, self.next_offset)
    }
}

pub trait FriendsApi {
    type Error: ApiFailure;

    fn friends<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        page: FriendsPageRequest,
    ) -> impl Future<Output = Result<FriendsPage, Self::Error>> + Send + 'a;
}

pub trait FriendshipMutationApi {
    type Error: ApiFailure;

    fn add_or_accept_friend<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        target_user_id: &'a UserId,
        expected_status: FriendshipStatus,
    ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a;

    fn remove_or_cancel_friend<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        self_user_id: &'a UserId,
        target_user_id: &'a UserId,
    ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a;
}

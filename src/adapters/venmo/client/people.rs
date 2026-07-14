use crate::features::people::{
    FriendsApi, FriendsPage, FriendsPageRequest, User, UserLookupApi, UserSearchApi,
    UserSearchPage, UserSearchPageRequest, UserSearchQuery,
};
use crate::shared::{AccessToken, DeviceId, Limit, Offset, UserId};
use std::future::Future;

use super::super::dto::{FriendsEnvelope, UserEnvelope, UsersEnvelope};
use super::super::transport::{ApiSession, ApiTransport, HttpRequest};
use super::error::VenmoApiError;
use super::response::decode_success;
use super::support::{
    map_user, map_users, require_query_value, validate_page_count, validate_query_keys,
};
use super::{FRIENDS_OPERATION, USER_LOOKUP_OPERATION, USER_SEARCH_OPERATION, VenmoApiClient};

impl<T: ApiTransport> VenmoApiClient<T> {
    pub(super) async fn fetch_friends_page(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        current_user_id: &UserId,
        page: FriendsPageRequest,
    ) -> Result<FriendsPage, VenmoApiError> {
        let offset = page.offset().get();
        let offset_value = offset.to_string();
        let limit_value = page.page_size().get().to_string();
        let path_segments = ["users", current_user_id.as_str(), "friends"];
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::read(
                    "/users/{user-id}/friends",
                    &path_segments,
                    &[
                        ("limit", limit_value.as_str()),
                        ("offset", offset_value.as_str()),
                    ],
                ),
            )
            .await?;
        let envelope: FriendsEnvelope = decode_success(
            FRIENDS_OPERATION,
            response,
            "the friend-list response did not match the supported envelope",
        )?;
        let users = map_users(envelope.data, FRIENDS_OPERATION)?;
        validate_page_count(FRIENDS_OPERATION, users.len(), page.page_size())?;
        let next_token = self.parse_friends_next_link(
            envelope.pagination.next.as_deref(),
            &path_segments,
            page.page_size(),
        )?;
        Ok(FriendsPage::new(users, next_token))
    }

    fn parse_friends_next_link(
        &self,
        raw: Option<&str>,
        path_segments: &[&str],
        page_size: Limit,
    ) -> Result<Option<Offset>, VenmoApiError> {
        let Some(raw) = raw else {
            return Ok(None);
        };
        let pairs = self.transport.parse_trusted_next_link(raw, path_segments)?;
        let query = validate_query_keys(FRIENDS_OPERATION, &pairs, &["limit", "offset"])?;
        require_query_value(
            FRIENDS_OPERATION,
            &query,
            "limit",
            &page_size.get().to_string(),
        )?;
        let offset = query
            .get("offset")
            .ok_or(VenmoApiError::Contract {
                operation: FRIENDS_OPERATION,
                problem: "the friend-list continuation omitted its offset",
            })?
            .parse::<Offset>()
            .map_err(|_| VenmoApiError::Contract {
                operation: FRIENDS_OPERATION,
                problem: "the friend-list continuation contained an invalid offset",
            })?;
        Ok(Some(offset))
    }

    async fn fetch_user_search_page(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        query: &UserSearchQuery,
        page: UserSearchPageRequest,
    ) -> Result<UserSearchPage, VenmoApiError> {
        let offset = page.offset().get();
        let offset_value = offset.to_string();
        let limit_value = page.page_size().get().to_string();
        let username_query = query.username_query();
        let query_value = username_query.unwrap_or(query.as_str());
        let query_pairs = [
            ("query", query_value),
            ("limit", limit_value.as_str()),
            ("offset", offset_value.as_str()),
        ]
        .into_iter()
        .chain(username_query.map(|_| ("type", "username")))
        .collect::<Vec<_>>();
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::read("/users", &["users"], &query_pairs),
            )
            .await?;
        let envelope: UsersEnvelope = decode_success(
            USER_SEARCH_OPERATION,
            response,
            "the user-search response did not match the supported envelope",
        )?;
        let users = map_users(envelope.data.into_users(), USER_SEARCH_OPERATION)?;
        let page_size = validate_page_count(USER_SEARCH_OPERATION, users.len(), page.page_size())?;
        let next_token = if !users.is_empty() && users.len() == page_size {
            let returned = u32::try_from(users.len()).map_err(|_| VenmoApiError::Contract {
                operation: USER_SEARCH_OPERATION,
                problem: "the returned page size could not be represented safely",
            })?;
            let next_offset = offset
                .checked_add(returned)
                .ok_or(VenmoApiError::Contract {
                    operation: USER_SEARCH_OPERATION,
                    problem: "the user-search offset overflowed",
                })?;
            Some(Offset::new(next_offset))
        } else {
            None
        };
        Ok(UserSearchPage::new(users, next_token))
    }

    async fn fetch_user_by_id(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        user_id: &UserId,
    ) -> Result<User, VenmoApiError> {
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::read("/users/{user-id}", &["users", user_id.as_str()], &[]),
            )
            .await?;
        let envelope: UserEnvelope = decode_success(
            USER_LOOKUP_OPERATION,
            response,
            "the user-lookup response did not match the supported envelope",
        )?;
        let user = map_user(envelope.data.into_user(), USER_LOOKUP_OPERATION)?;
        if user.user_id() != user_id {
            return Err(VenmoApiError::Contract {
                operation: USER_LOOKUP_OPERATION,
                problem: "the user-lookup response returned a different user ID",
            });
        }
        Ok(user)
    }
}

impl<T: ApiTransport> UserLookupApi for VenmoApiClient<T> {
    type Error = VenmoApiError;
    fn user_by_id<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        user_id: &'a UserId,
    ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a {
        self.fetch_user_by_id(access_token, device_id, user_id)
    }
}

impl<T: ApiTransport> UserSearchApi for VenmoApiClient<T> {
    type Error = VenmoApiError;
    fn search_users<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        query: &'a UserSearchQuery,
        page: UserSearchPageRequest,
    ) -> impl Future<Output = Result<UserSearchPage, Self::Error>> + Send + 'a {
        self.fetch_user_search_page(access_token, device_id, query, page)
    }
}

impl<T: ApiTransport> FriendsApi for VenmoApiClient<T> {
    type Error = VenmoApiError;
    fn friends<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        page: FriendsPageRequest,
    ) -> impl Future<Output = Result<FriendsPage, Self::Error>> + Send + 'a {
        self.fetch_friends_page(access_token, device_id, current_user_id, page)
    }
}

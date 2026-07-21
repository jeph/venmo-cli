use super::{
    Activity, ActivityBeforeId, ActivityCommentId, ActivityCommentMessage, ActivityDetail,
    ActivityFeedScope, ActivityId,
};
use crate::shared::{AccessToken, ApiFailure, DeviceId, Limit, UserId};
use std::future::Future;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityPageRequest {
    page_size: Limit,
    before_id: Option<ActivityBeforeId>,
}

impl ActivityPageRequest {
    #[must_use]
    pub const fn new(page_size: Limit, before_id: Option<ActivityBeforeId>) -> Self {
        Self {
            page_size,
            before_id,
        }
    }

    #[must_use]
    pub const fn page_size(&self) -> Limit {
        self.page_size
    }

    #[must_use]
    pub fn before_id(&self) -> Option<&ActivityBeforeId> {
        self.before_id.as_ref()
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct ActivityPage {
    activities: Vec<Activity>,
    next_before_id: Option<ActivityBeforeId>,
}

impl ActivityPage {
    #[must_use]
    pub fn new(activities: Vec<Activity>, next_before_id: Option<ActivityBeforeId>) -> Self {
        Self {
            activities,
            next_before_id,
        }
    }

    #[must_use]
    pub fn into_parts(self) -> (Vec<Activity>, Option<ActivityBeforeId>) {
        (self.activities, self.next_before_id)
    }
}

pub trait ActivityListApi {
    type Error: ApiFailure;

    fn activity<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        scope: &'a ActivityFeedScope,
        page: ActivityPageRequest,
    ) -> impl Future<Output = Result<ActivityPage, Self::Error>> + Send + 'a;
}

pub trait ActivityDetailApi {
    type Error: ApiFailure;

    fn activity_by_id<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        activity_id: &'a ActivityId,
    ) -> impl Future<Output = Result<ActivityDetail, Self::Error>> + Send + 'a;
}

pub trait ActivitySocialMutationApi {
    type Error: ApiFailure;

    fn like_activity<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        activity_id: &'a ActivityId,
    ) -> impl Future<Output = Result<ActivityDetail, Self::Error>> + Send + 'a;

    fn unlike_activity<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        activity_id: &'a ActivityId,
    ) -> impl Future<Output = Result<ActivityDetail, Self::Error>> + Send + 'a;

    fn add_activity_comment<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        activity_id: &'a ActivityId,
        message: &'a ActivityCommentMessage,
    ) -> impl Future<Output = Result<ActivityDetail, Self::Error>> + Send + 'a;
}

pub trait ActivityCommentRemovalApi {
    type Error: ApiFailure;

    fn remove_activity_comment<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        comment_id: &'a ActivityCommentId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;
}

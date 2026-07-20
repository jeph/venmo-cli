use super::{
    AcceptRequestPlan, AcceptedRequest, CreateRequestPlan, CreatedRequest, DeclineRequestPlan,
    DeclinedRequest, RequestApprovalFees, RequestId, RequestNotificationId, RequestRecord,
    RequestsBefore,
};
use crate::features::payments::{EligibilityToken, PeerFundingSource};
use crate::features::people::User;
use crate::shared::{AccessToken, ApiFailure, DeviceId, Limit, UserId};
use std::future::Future;

pub(crate) trait RequestCreationApi {
    type Error: ApiFailure;

    fn create_request<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        plan: &'a CreateRequestPlan,
    ) -> impl Future<Output = Result<CreatedRequest, Self::Error>> + Send + 'a;
}

pub(crate) trait RequestAcceptanceApi {
    type Error: ApiFailure;

    fn accept_request<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        plan: &'a AcceptRequestPlan,
    ) -> impl Future<Output = Result<AcceptedRequest, Self::Error>> + Send + 'a;
}

pub(crate) struct RequestApprovalEligibility {
    token: EligibilityToken,
    fees: RequestApprovalFees,
}

impl RequestApprovalEligibility {
    #[must_use]
    pub const fn new(token: EligibilityToken, fees: RequestApprovalFees) -> Self {
        Self { token, fees }
    }

    #[must_use]
    pub fn into_parts(self) -> (EligibilityToken, RequestApprovalFees) {
        (self.token, self.fees)
    }
}

pub(crate) trait RequestApprovalEligibilityApi {
    type Error: ApiFailure;

    #[allow(clippy::too_many_arguments)]
    fn request_approval_eligibility<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        requester: &'a User,
        amount_cents: u64,
        note: &'a str,
        funding: &'a PeerFundingSource,
    ) -> impl Future<Output = Result<RequestApprovalEligibility, Self::Error>> + Send + 'a;
}

pub(crate) trait RequestApprovalNotificationApi {
    type Error: ApiFailure;

    fn request_approval_notification_id<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        request_id: &'a RequestId,
    ) -> impl Future<Output = Result<RequestNotificationId, Self::Error>> + Send + 'a;
}

pub(crate) trait RequestDeclineApi {
    type Error: ApiFailure;

    fn decline_request<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        plan: &'a DeclineRequestPlan,
    ) -> impl Future<Output = Result<DeclinedRequest, Self::Error>> + Send + 'a;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingRequestsPageRequest {
    page_size: Limit,
    before: Option<RequestsBefore>,
}

impl PendingRequestsPageRequest {
    #[must_use]
    pub const fn new(page_size: Limit, before: Option<RequestsBefore>) -> Self {
        Self { page_size, before }
    }

    #[must_use]
    pub const fn page_size(&self) -> Limit {
        self.page_size
    }

    #[must_use]
    pub fn before(&self) -> Option<&RequestsBefore> {
        self.before.as_ref()
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct PendingRequestsPage {
    requests: Vec<RequestRecord>,
    next_before: Option<RequestsBefore>,
}

impl PendingRequestsPage {
    #[must_use]
    pub fn new(requests: Vec<RequestRecord>, next_before: Option<RequestsBefore>) -> Self {
        Self {
            requests,
            next_before,
        }
    }

    #[must_use]
    pub fn into_parts(self) -> (Vec<RequestRecord>, Option<RequestsBefore>) {
        (self.requests, self.next_before)
    }
}

pub trait RequestsApi {
    type Error: ApiFailure;

    fn pending_requests<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        page: PendingRequestsPageRequest,
    ) -> impl Future<Output = Result<PendingRequestsPage, Self::Error>> + Send + 'a;
}

pub(crate) trait RequestLookupApi {
    type Error: ApiFailure;

    fn request_by_id<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        request_id: &'a RequestId,
    ) -> impl Future<Output = Result<RequestRecord, Self::Error>> + Send + 'a;
}

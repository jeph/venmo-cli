use crate::features::activity::ActivityPageRequest;
use crate::features::doctor::{DoctorApi, RequiredShape, ShapeProbeOutcome};
use crate::features::people::FriendsPageRequest;
use crate::features::requests::PendingRequestsPageRequest;
use crate::shared::{AccessToken, Account, ApiFailure, DeviceId, Limit, Offset, UserId};

use super::super::transport::{ApiTransport, HttpRequest};
use super::VenmoApiClient;
use super::error::VenmoApiError;

impl<T: ApiTransport> DoctorApi for VenmoApiClient<T> {
    type Error = VenmoApiError;

    async fn connectivity(&self) -> Result<(), Self::Error> {
        self.transport
            .send_unauthenticated(HttpRequest::read("/account", &["account"], &[]))
            .await?;
        Ok(())
    }

    async fn diagnostic_current_account(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
    ) -> Result<Account, Self::Error> {
        self.fetch_current_account(access_token, device_id).await
    }

    async fn required_shapes(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        current_user_id: &UserId,
    ) -> Vec<ShapeProbeOutcome> {
        let page_size = Limit::MIN;
        vec![
            shape_probe(
                RequiredShape::Balance,
                self.fetch_balance(access_token, device_id).await,
            ),
            shape_probe(
                RequiredShape::PaymentMethods,
                self.fetch_payment_methods(access_token, device_id).await,
            ),
            shape_probe(
                RequiredShape::Friends,
                self.fetch_friends_page(
                    access_token,
                    device_id,
                    current_user_id,
                    FriendsPageRequest::new(page_size, Offset::default()),
                )
                .await,
            ),
            shape_probe(
                RequiredShape::Activity,
                self.fetch_activity_page(
                    access_token,
                    device_id,
                    current_user_id,
                    ActivityPageRequest::new(page_size, None),
                )
                .await,
            ),
            shape_probe(
                RequiredShape::PendingRequests,
                self.fetch_pending_requests_page(
                    access_token,
                    device_id,
                    current_user_id,
                    PendingRequestsPageRequest::new(page_size, None),
                )
                .await,
            ),
        ]
    }
}

fn shape_probe<T>(shape: RequiredShape, result: Result<T, VenmoApiError>) -> ShapeProbeOutcome {
    match result {
        Ok(_) => ShapeProbeOutcome::passed(shape),
        Err(error) => ShapeProbeOutcome::failed(shape, error.kind()),
    }
}

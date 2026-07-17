use std::future::Future;

use super::{CreatedTransfer, TransferOptions, TransferOutPlan};
use crate::shared::{AccessToken, ApiFailure, DeviceId};

pub trait TransferOptionsApi {
    type Error: ApiFailure;

    fn transfer_options<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<TransferOptions, Self::Error>> + Send + 'a;
}

pub trait TransferCreationApi {
    type Error: ApiFailure;

    fn create_transfer<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        plan: &'a TransferOutPlan,
    ) -> impl Future<Output = Result<CreatedTransfer, Self::Error>> + Send + 'a;
}

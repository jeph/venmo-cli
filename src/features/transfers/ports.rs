use std::future::Future;

use super::{CreatedTransfer, CreatedTransferIn, TransferInPlan, TransferOptions, TransferOutPlan};
use crate::shared::{AccessToken, ApiFailure, DeviceId};

pub trait TransferOptionsApi {
    type Error: ApiFailure;

    fn transfer_options<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<TransferOptions, Self::Error>> + Send + 'a;
}

pub trait TransferOutCreationApi {
    type Error: ApiFailure;

    fn create_transfer_out<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        plan: &'a TransferOutPlan,
    ) -> impl Future<Output = Result<CreatedTransfer, Self::Error>> + Send + 'a;
}

pub trait TransferInCreationApi {
    type Error: ApiFailure;

    fn create_transfer_in<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        plan: &'a TransferInPlan,
    ) -> impl Future<Output = Result<CreatedTransferIn, Self::Error>> + Send + 'a;
}

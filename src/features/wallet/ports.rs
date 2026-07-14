use std::future::Future;

use super::{Balance, PaymentMethod};
use crate::shared::{AccessToken, ApiFailure, DeviceId};

pub trait PaymentMethodsApi {
    type Error: ApiFailure;

    fn payment_methods<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Vec<PaymentMethod>, Self::Error>> + Send + 'a;
}

pub trait BalanceApi {
    type Error: ApiFailure;

    fn balance<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Balance, Self::Error>> + Send + 'a;
}

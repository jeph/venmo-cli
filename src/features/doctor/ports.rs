use std::future::Future;

use crate::shared::{AccessToken, Account, ApiFailure, ApiFailureKind, DeviceId, UserId};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RequiredShape {
    Balance,
    PaymentMethods,
    Friends,
    Activity,
    PendingRequests,
}

impl RequiredShape {
    pub const ALL: [Self; 5] = [
        Self::Balance,
        Self::PaymentMethods,
        Self::Friends,
        Self::Activity,
        Self::PendingRequests,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Balance => "balance",
            Self::PaymentMethods => "payment methods",
            Self::Friends => "friends",
            Self::Activity => "activity",
            Self::PendingRequests => "pending requests",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShapeProbeOutcome {
    shape: RequiredShape,
    failure: Option<ApiFailureKind>,
}

impl ShapeProbeOutcome {
    #[must_use]
    pub const fn passed(shape: RequiredShape) -> Self {
        Self {
            shape,
            failure: None,
        }
    }

    #[must_use]
    pub const fn failed(shape: RequiredShape, failure: ApiFailureKind) -> Self {
        Self {
            shape,
            failure: Some(failure),
        }
    }

    #[must_use]
    pub const fn shape(self) -> RequiredShape {
        self.shape
    }

    #[must_use]
    pub const fn failure(self) -> Option<ApiFailureKind> {
        self.failure
    }
}

pub trait DoctorApi {
    type Error: ApiFailure;

    fn connectivity(&self) -> impl Future<Output = Result<(), Self::Error>> + Send + '_;

    fn diagnostic_current_account<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a;

    fn required_shapes<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
    ) -> impl Future<Output = Vec<ShapeProbeOutcome>> + Send + 'a;
}

use std::future::Future;

use super::{CreatedPayment, EligibilityToken, PayPlan, PeerFundingSources};
use crate::features::auth::{OtpCode, PromptAvailability, PromptError};
use crate::features::people::User;
use crate::shared::{AccessToken, ApiFailure, ClientRequestId, DeviceId, Money, Note};

pub trait DefaultNoConfirmation: PromptAvailability {
    fn confirm_default_no(&self, prompt: &str) -> Result<bool, PromptError>;
}

pub(crate) trait PaymentStepUpInput: PromptAvailability {
    fn read_payment_otp(&self, prompt: &str) -> Result<OtpCode, PromptError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PaymentVerification {
    Unverified,
    SmsOtpVerified,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PaymentCreationOutcome {
    Created(CreatedPayment),
    OtpStepUpRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PaymentOtpVerification {
    Verified,
    Incorrect,
    Expired,
    Unexpected,
    TooManyIncorrectAttempts,
}

#[derive(Debug)]
pub(crate) struct BlankSourceEligibility {
    token: EligibilityToken,
    overall_fee_cents: u64,
}

impl BlankSourceEligibility {
    #[must_use]
    pub const fn new(token: EligibilityToken, overall_fee_cents: u64) -> Self {
        Self {
            token,
            overall_fee_cents,
        }
    }

    #[cfg(test)]
    #[must_use]
    pub const fn token(&self) -> &EligibilityToken {
        &self.token
    }

    #[must_use]
    pub const fn overall_fee_cents(&self) -> u64 {
        self.overall_fee_cents
    }

    #[must_use]
    pub fn into_token(self) -> EligibilityToken {
        self.token
    }
}

pub(crate) trait PeerFundingApi {
    type Error: ApiFailure;

    fn peer_funding_sources<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<PeerFundingSources, Self::Error>> + Send + 'a;
}

pub(crate) trait BlankSourceEligibilityApi {
    type Error: ApiFailure;

    fn blank_source_eligibility<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        recipient: &'a User,
        amount: Money,
        note: &'a Note,
    ) -> impl Future<Output = Result<BlankSourceEligibility, Self::Error>> + Send + 'a;
}

pub(crate) trait PaymentCreationApi {
    type Error: ApiFailure;

    fn create_payment<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        plan: &'a PayPlan,
        verification: PaymentVerification,
    ) -> impl Future<Output = Result<PaymentCreationOutcome, Self::Error>> + Send + 'a;
}

pub(crate) trait PaymentStepUpApi {
    type Error: ApiFailure;

    fn issue_payment_otp<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        request_id: &'a ClientRequestId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;

    fn verify_payment_otp<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        request_id: &'a ClientRequestId,
        otp: &'a OtpCode,
    ) -> impl Future<Output = Result<PaymentOtpVerification, Self::Error>> + Send + 'a;
}

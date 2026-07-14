use std::future::Future;

use super::{CreatedPayment, EligibilityToken, PayPlan, PeerFundingMethod};
use crate::features::auth::{PromptAvailability, PromptError};
use crate::features::people::User;
use crate::features::wallet::PaymentMethod;
use crate::shared::{AccessToken, ApiFailure, DeviceId, Money, Note};

pub trait DefaultNoConfirmation: PromptAvailability {
    fn confirm_default_no(&self, prompt: &str) -> Result<bool, PromptError>;
}

pub trait FundingChoiceSelection: PromptAvailability {
    fn select_funding_choice(
        &self,
        prompt: &str,
        choices: &[&PaymentMethod],
    ) -> Result<usize, PromptError>;
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

    fn peer_funding_methods<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Vec<PeerFundingMethod>, Self::Error>> + Send + 'a;
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
    ) -> impl Future<Output = Result<CreatedPayment, Self::Error>> + Send + 'a;
}

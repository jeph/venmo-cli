#![allow(private_bounds)]

mod activity;
mod auth;
mod error;
mod payments;
mod people;
mod requests;
mod response;
mod support;
mod transfers;
mod wallet;

use super::transport::{TransportBuildError, VenmoHttpTransport};

pub(crate) struct VenmoApiClient<T = VenmoHttpTransport> {
    transport: T,
}

impl VenmoApiClient<VenmoHttpTransport> {
    pub(crate) fn production() -> Result<Self, TransportBuildError> {
        VenmoHttpTransport::production().map(Self::new)
    }
}

impl<T> VenmoApiClient<T> {
    #[must_use]
    pub(super) const fn new(transport: T) -> Self {
        Self { transport }
    }
}

pub(super) const ACTIVITY_DETAIL_OPERATION: &str = "activity detail";
pub(super) const ACTIVITY_LIST_OPERATION: &str = "activity listing";
pub(super) const BALANCE_OPERATION: &str = "wallet balance";
pub(super) const CURRENT_ACCOUNT_OPERATION: &str = "current account";
pub(super) const DEVICE_TRUST_OPERATION: &str = "device trust";
pub(super) const FRIENDS_OPERATION: &str = "friend listing";
pub(super) const OTP_COMPLETION_OPERATION: &str = "OTP login completion";
pub(super) const OTP_REQUEST_OPERATION: &str = "SMS OTP request";
pub(super) const PASSWORD_LOGIN_OPERATION: &str = "password login";
pub(super) const PAYMENT_CREATION_OPERATION: &str = "payment creation";
pub(super) const BLANK_SOURCE_ELIGIBILITY_OPERATION: &str = "payment eligibility";
pub(super) const PAYMENT_METHODS_OPERATION: &str = "payment-method listing";
pub(super) const PEER_FUNDING_OPERATION: &str = "peer funding-method listing";
pub(super) const REQUEST_CREATION_OPERATION: &str = "request creation";
pub(super) const REQUEST_ACCEPTANCE_OPERATION: &str = "request acceptance";
pub(super) const REQUEST_DECLINE_OPERATION: &str = "request decline";
pub(super) const REQUEST_DETAIL_OPERATION: &str = "request detail";
pub(super) const REQUEST_LIST_OPERATION: &str = "pending-request listing";
pub(super) const USER_LOOKUP_OPERATION: &str = "user lookup";
pub(super) const USER_SEARCH_OPERATION: &str = "user search";
pub(super) const TRANSFER_OPTIONS_OPERATION: &str = "transfer options";
pub(super) const TRANSFER_CREATION_OPERATION: &str = "transfer creation";

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

use super::common::StringOrInteger;

#[derive(Serialize)]
pub(crate) struct UpdatePaymentRequest {
    pub action: &'static str,
}

#[derive(Serialize)]
pub(crate) struct CreateRequestRequest<'a> {
    pub uuid: &'a str,
    pub user_id: &'a str,
    pub audience: &'static str,
    pub amount: &'a serde_json::Number,
    pub note: &'a str,
}

#[derive(Deserialize)]
pub(crate) struct RequestApprovalEligibilityEnvelope {
    pub data: RequestApprovalEligibilityDto,
}

#[derive(Deserialize)]
pub(crate) struct RequestApprovalNotificationsEnvelope {
    pub data: Vec<RequestApprovalNotificationDto>,
}

#[derive(Deserialize)]
pub(crate) struct RequestApprovalNotificationDto {
    pub id: StringOrInteger,
    #[serde(default)]
    pub payment: Option<RequestApprovalNotificationPaymentDto>,
}

#[derive(Deserialize)]
pub(crate) struct RequestApprovalNotificationPaymentDto {
    pub id: StringOrInteger,
}

#[derive(Deserialize)]
pub(crate) struct RequestApprovalEligibilityDto {
    pub eligible: bool,
    #[serde(default)]
    pub eligibility_token: Option<String>,
    #[serde(default)]
    pub fees: Option<Vec<RequestApprovalFeeDto>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RequestApprovalFeeDto {
    pub product_uri: String,
    pub applied_to: String,
    pub fee_token: String,
    #[serde(default)]
    pub base_fee_amount: Option<u64>,
    #[serde(default)]
    pub fee_percentage: Option<serde_json::Number>,
    pub calculated_fee_amount_in_cents: u64,
}

#[derive(Serialize)]
pub(crate) struct FundedRequestApproval<'a> {
    pub funding_source_id: &'a str,
    pub eligibility_token: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fees: Option<&'a [FundedRequestApprovalFee<'a>]>,
    pub metadata: RequestApprovalMetadata,
}

#[derive(Serialize)]
pub(crate) struct FundedRequestApprovalFee<'a> {
    pub product_uri: &'a str,
    pub applied_to: &'a str,
    pub fee_token: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_fee_amount: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_percentage: Option<serde_json::Number>,
    pub calculated_fee_amount_in_cents: u64,
}

#[derive(Serialize)]
pub(crate) struct RequestApprovalMetadata {
    pub quasi_cash_disclaimer_viewed: bool,
}

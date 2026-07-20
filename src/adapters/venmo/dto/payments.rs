use serde::{Deserialize, Serialize};

use super::common::{PaginationDto, StringOrInteger, StringOrNumber};
use super::people::UserDto;
use super::wallet::FeeDto;

#[derive(Deserialize)]
pub(crate) struct PaymentsEnvelope {
    pub data: Vec<PaymentRecordDto>,
    #[serde(default)]
    pub pagination: PaginationDto,
}

#[derive(Deserialize)]
pub(crate) struct PaymentEnvelope {
    pub data: PaymentData,
}

#[derive(Deserialize)]
pub(crate) struct CreatedPaymentEnvelope {
    pub data: CreatedPaymentData,
}

#[derive(Deserialize)]
pub(crate) struct CreatedPaymentData {
    pub payment: PaymentRecordDto,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum PaymentData {
    Wrapped { payment: PaymentRecordDto },
    Direct(PaymentRecordDto),
}

impl PaymentData {
    pub(crate) fn into_payment(self) -> PaymentRecordDto {
        match self {
            Self::Wrapped { payment } | Self::Direct(payment) => payment,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct PaymentRecordDto {
    pub id: StringOrInteger,
    pub status: String,
    pub action: String,
    pub amount: StringOrNumber,
    pub actor: UserDto,
    pub target: PaymentTargetDto,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub audience: Option<String>,
    #[serde(default)]
    pub date_created: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct PaymentTargetDto {
    pub user: UserDto,
}

#[derive(Serialize)]
pub(crate) struct BlankSourceEligibilityRequest<'a> {
    pub funding_source_id: &'static str,
    pub action: &'static str,
    pub country_code: &'static str,
    pub target_type: &'static str,
    pub note: &'a str,
    pub target_id: &'a str,
    pub amount: u64,
}

#[derive(Deserialize)]
pub(crate) struct BlankSourceEligibilityEnvelope {
    pub data: BlankSourceEligibilityDto,
}

#[derive(Deserialize)]
pub(crate) struct BlankSourceEligibilityDto {
    pub eligibility_token: String,
    pub eligible: bool,
    pub fees: Vec<FeeDto>,
    pub fee_disclaimer: String,
    #[serde(default)]
    pub ineligible_reason: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct CreatePaymentRequest<'a> {
    pub uuid: &'a str,
    pub user_id: &'a str,
    pub audience: &'static str,
    pub amount: &'a serde_json::Number,
    pub note: &'a str,
    pub eligibility_token: &'a str,
    pub funding_source_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<CreatePaymentMetadata<'a>>,
}

#[derive(Serialize)]
pub(crate) struct CreatePaymentMetadata<'a> {
    pub verification_method: &'a [&'a str],
    pub verification_status: &'a str,
}

#[derive(Serialize)]
pub(crate) struct PaymentOtpGraphQlRequest<'a, T> {
    pub query: &'static str,
    pub variables: PaymentOtpGraphQlVariables<'a, T>,
}

#[derive(Serialize)]
pub(crate) struct PaymentOtpGraphQlVariables<'a, T> {
    pub input: PaymentOtpInput<'a, T>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PaymentOtpInput<'a, T> {
    pub flow_type: &'static str,
    #[serde(flatten)]
    pub action: T,
    pub uuid: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IssuePaymentOtpAction {
    pub delivery_method: &'static str,
}

#[derive(Serialize)]
pub(crate) struct VerifyPaymentOtpAction<'a> {
    pub otp: &'a str,
}

#[derive(Deserialize)]
pub(crate) struct IssuePaymentOtpEnvelope {
    pub data: Option<IssuePaymentOtpData>,
    #[serde(default)]
    pub errors: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IssuePaymentOtpData {
    pub send_otp: IssuePaymentOtpResult,
}

#[derive(Deserialize)]
pub(crate) struct IssuePaymentOtpResult {
    pub success: bool,
}

#[derive(Deserialize)]
pub(crate) struct VerifyPaymentOtpEnvelope {
    pub data: Option<VerifyPaymentOtpData>,
    #[serde(default)]
    pub errors: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VerifyPaymentOtpData {
    pub validate_otp: Option<VerifyPaymentOtpResult>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VerifyPaymentOtpResult {
    pub validated: bool,
    pub reason_code: Option<String>,
}

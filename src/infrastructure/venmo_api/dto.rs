use serde::{Deserialize, Serialize};

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) struct PasswordLoginRequest<'a> {
    pub phone_email_or_username: &'a str,
    pub client_id: &'static str,
    pub password: &'a str,
}

#[derive(Serialize)]
pub(super) struct SmsOtpRequest {
    pub via: &'static str,
}

#[derive(Debug, Deserialize)]
pub(super) struct AccountEnvelope {
    pub data: AccountData,
}

#[derive(Deserialize)]
pub(super) struct BalanceEnvelope {
    pub data: BalanceDto,
}

#[derive(Deserialize)]
pub(super) struct BalanceDto {
    pub balance: String,
    pub balance_on_hold: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum AccountData {
    Wrapped { user: UserDto },
    Direct(UserDto),
}

impl AccountData {
    pub(super) fn into_user(self) -> UserDto {
        match self {
            Self::Wrapped { user } | Self::Direct(user) => user,
        }
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct UsersEnvelope {
    pub data: UsersData,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum UsersData {
    Wrapped { users: Vec<UserDto> },
    Direct(Vec<UserDto>),
}

impl UsersData {
    pub(super) fn into_users(self) -> Vec<UserDto> {
        match self {
            Self::Wrapped { users } | Self::Direct(users) => users,
        }
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct UserDto {
    pub id: StringOrInteger,
    pub username: Option<String>,
    #[serde(default, alias = "displayName", alias = "name")]
    pub display_name: Option<String>,
    #[serde(default)]
    pub identity_type: Option<String>,
    #[serde(default)]
    pub is_payable: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct UserEnvelope {
    pub data: UserData,
}

#[derive(Deserialize)]
pub(super) struct FriendsEnvelope {
    pub data: Vec<UserDto>,
    #[serde(default)]
    pub pagination: PaginationDto,
}

#[derive(Default, Deserialize)]
pub(super) struct PaginationDto {
    #[serde(default)]
    pub next: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct StoriesEnvelope {
    pub data: Vec<StoryDto>,
    #[serde(default)]
    pub pagination: PaginationDto,
}

#[derive(Deserialize)]
pub(super) struct StoryEnvelope {
    pub data: StoryData,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum StoryData {
    Wrapped { story: StoryDto },
    Direct(StoryDto),
}

impl StoryData {
    pub(super) fn into_story(self) -> StoryDto {
        match self {
            Self::Wrapped { story } | Self::Direct(story) => story,
        }
    }
}

#[derive(Deserialize)]
pub(super) struct StoryDto {
    pub id: StringOrInteger,
    #[serde(default)]
    pub date_created: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub audience: Option<String>,
    #[serde(default)]
    pub payment: Option<PaymentRecordDto>,
    #[serde(default)]
    pub transfer: Option<TransferDto>,
    #[serde(default)]
    pub authorization: Option<AuthorizationDto>,
}

#[derive(Deserialize)]
pub(super) struct AuthorizationDto {
    pub id: StringOrInteger,
    pub status: String,
    pub amount: StringOrNumber,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub descriptor: Option<String>,
    pub merchant: AuthorizationMerchantDto,
    pub user: UserDto,
}

#[derive(Deserialize)]
pub(super) struct AuthorizationMerchantDto {
    pub display_name: String,
}

#[derive(Deserialize)]
pub(super) struct TransferDto {
    #[serde(default)]
    pub id: Option<StringOrInteger>,
    pub status: String,
    #[serde(rename = "type")]
    pub transfer_type: String,
    pub amount: StringOrNumber,
    #[serde(default)]
    pub date_requested: Option<String>,
    #[serde(default)]
    pub destination: Option<TransferEndpointDto>,
    #[serde(default)]
    pub source: Option<TransferEndpointDto>,
}

#[derive(Deserialize)]
pub(super) struct TransferEndpointDto {
    pub name: String,
    #[serde(rename = "type")]
    pub endpoint_type: String,
    #[serde(default)]
    pub last_four: Option<StringOrInteger>,
}

#[derive(Deserialize)]
pub(super) struct PaymentsEnvelope {
    pub data: Vec<PaymentRecordDto>,
    #[serde(default)]
    pub pagination: PaginationDto,
}

#[derive(Deserialize)]
pub(super) struct PaymentEnvelope {
    pub data: PaymentData,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub(super) struct CreatedPaymentEnvelope {
    pub data: CreatedPaymentData,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub(super) struct CreatedPaymentData {
    pub payment: PaymentRecordDto,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum PaymentData {
    Wrapped { payment: PaymentRecordDto },
    Direct(PaymentRecordDto),
}

impl PaymentData {
    pub(super) fn into_payment(self) -> PaymentRecordDto {
        match self {
            Self::Wrapped { payment } | Self::Direct(payment) => payment,
        }
    }
}

#[derive(Deserialize)]
pub(super) struct PaymentRecordDto {
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
pub(super) struct PaymentTargetDto {
    pub user: UserDto,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum UserData {
    Wrapped { user: UserDto },
    Direct(UserDto),
}

impl UserData {
    pub(super) fn into_user(self) -> UserDto {
        match self {
            Self::Wrapped { user } | Self::Direct(user) => user,
        }
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct PaymentMethodsEnvelope {
    pub data: PaymentMethodsData,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum PaymentMethodsData {
    Wrapped {
        payment_methods: Vec<PaymentMethodDto>,
    },
    Direct(Vec<PaymentMethodDto>),
}

impl PaymentMethodsData {
    pub(super) fn into_methods(self) -> Vec<PaymentMethodDto> {
        match self {
            Self::Wrapped { payment_methods } => payment_methods,
            Self::Direct(methods) => methods,
        }
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct PaymentMethodDto {
    pub id: StringOrInteger,
    #[serde(default, alias = "display_name", alias = "label")]
    pub name: Option<StringOrInteger>,
    #[serde(default, rename = "type", alias = "payment_method_type")]
    pub method_type: Option<StringOrInteger>,
    #[serde(default, alias = "lastFour")]
    pub last_four: Option<StringOrInteger>,
    #[serde(default, alias = "isDefault")]
    pub is_default: Option<bool>,
    #[serde(default)]
    pub role: Option<StringOrInteger>,
    #[serde(default)]
    pub payment_method_role: Option<StringOrInteger>,
    #[serde(default)]
    pub peer_payment_role: Option<StringOrInteger>,
    #[serde(default)]
    pub merchant_payment_role: Option<StringOrInteger>,
    #[serde(default)]
    #[allow(dead_code)]
    pub fee: Option<FeeDto>,
}

impl PaymentMethodDto {
    pub(super) fn is_default(&self) -> bool {
        self.is_default == Some(true)
            || self
                .role_values()
                .flatten()
                .any(|role| role.as_str().to_ascii_lowercase().contains("default"))
    }

    fn role_values(&self) -> impl Iterator<Item = Option<&StringOrInteger>> {
        [
            self.role.as_ref(),
            self.payment_method_role.as_ref(),
            self.peer_payment_role.as_ref(),
            self.merchant_payment_role.as_ref(),
        ]
        .into_iter()
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum FeeDto {
    Calculated { calculated_fee_amount_in_cents: u64 },
    Unknown(serde_json::Value),
}

impl FeeDto {
    pub(super) const fn calculated_cents(&self) -> Option<u64> {
        match self {
            Self::Calculated {
                calculated_fee_amount_in_cents,
            } => Some(*calculated_fee_amount_in_cents),
            Self::Unknown(value) => {
                let _ = value;
                None
            }
        }
    }
}

#[derive(Serialize)]
#[allow(dead_code)]
pub(super) struct PaymentEligibilityRequest<'a> {
    pub funding_source_id: &'static str,
    pub action: &'static str,
    pub country_code: &'static str,
    pub target_type: &'static str,
    pub note: &'a str,
    pub target_id: &'a str,
    pub amount: u64,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub(super) struct PaymentEligibilityEnvelope {
    pub data: PaymentEligibilityDto,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub(super) struct PaymentEligibilityDto {
    pub eligibility_token: String,
    pub eligible: bool,
    pub fees: Vec<FeeDto>,
    pub fee_disclaimer: String,
    #[serde(default)]
    pub ineligible_reason: Option<String>,
}

#[derive(Serialize)]
#[allow(dead_code)]
pub(super) struct CreatePaymentRequest<'a> {
    pub uuid: &'a str,
    pub user_id: &'a str,
    pub audience: &'static str,
    pub amount: &'a serde_json::Number,
    pub note: &'a str,
    pub eligibility_token: &'a str,
    pub funding_source_id: &'a str,
}

#[derive(Serialize)]
#[allow(dead_code)]
pub(super) struct CreateRequestRequest<'a> {
    pub uuid: &'a str,
    pub user_id: &'a str,
    pub audience: &'static str,
    pub amount: &'a serde_json::Number,
    pub note: &'a str,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum StringOrInteger {
    String(String),
    Unsigned(u64),
    Signed(i64),
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum StringOrNumber {
    String(String),
    Number(serde_json::Number),
}

impl StringOrNumber {
    pub(super) fn into_string(self) -> String {
        match self {
            Self::String(value) => value,
            Self::Number(value) => value.to_string(),
        }
    }
}

impl StringOrInteger {
    pub(super) fn into_string(self) -> String {
        match self {
            Self::String(value) => value,
            Self::Unsigned(value) => value.to_string(),
            Self::Signed(value) => value.to_string(),
        }
    }

    pub(super) fn as_str(&self) -> std::borrow::Cow<'_, str> {
        match self {
            Self::String(value) => std::borrow::Cow::Borrowed(value),
            Self::Unsigned(value) => std::borrow::Cow::Owned(value.to_string()),
            Self::Signed(value) => std::borrow::Cow::Owned(value.to_string()),
        }
    }
}

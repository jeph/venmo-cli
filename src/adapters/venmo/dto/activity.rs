use serde::Deserialize;

use super::common::{StringOrInteger, StringOrNumber};
use super::payments::PaymentTargetDto;
use super::people::UserDto;

#[derive(Deserialize)]
pub(crate) struct StoriesEnvelope {
    pub data: Vec<StoryDto>,
    #[serde(default)]
    pub pagination: super::common::PaginationDto,
}

#[derive(Deserialize)]
pub(crate) struct StoryEnvelope {
    pub data: StoryData,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum StoryData {
    Wrapped { story: StoryDto },
    Direct(StoryDto),
}

impl StoryData {
    pub(crate) fn into_story(self) -> StoryDto {
        match self {
            Self::Wrapped { story } | Self::Direct(story) => story,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct StoryDto {
    pub id: StringOrInteger,
    #[serde(default)]
    pub date_created: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub audience: Option<String>,
    #[serde(default)]
    pub payment: Option<ActivityPaymentRecordDto>,
    #[serde(default)]
    pub transfer: Option<TransferDto>,
    #[serde(default)]
    pub authorization: Option<AuthorizationDto>,
}

#[derive(Deserialize)]
pub(crate) struct ActivityPaymentRecordDto {
    pub id: StringOrInteger,
    pub status: String,
    pub action: String,
    #[serde(default)]
    pub amount: Option<StringOrNumber>,
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
pub(crate) struct AuthorizationDto {
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
pub(crate) struct AuthorizationMerchantDto {
    pub display_name: String,
}

#[derive(Deserialize)]
pub(crate) struct TransferDto {
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
pub(crate) struct TransferEndpointDto {
    pub name: String,
    #[serde(rename = "type")]
    pub endpoint_type: String,
    #[serde(default)]
    pub last_four: Option<StringOrInteger>,
}

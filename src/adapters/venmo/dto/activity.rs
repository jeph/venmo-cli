use serde::{Deserialize, Serialize};

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
    #[serde(default, rename = "type")]
    pub story_type: Option<String>,
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
    #[serde(default)]
    pub disbursement: Option<DisbursementDto>,
    #[serde(default)]
    pub likes: Option<StorySocialCollectionDto<UserDto>>,
    #[serde(default)]
    pub comments: Option<StorySocialCollectionDto<ActivityCommentDto>>,
    #[serde(default)]
    pub reactions: Option<Vec<ActivityReactionDto>>,
}

#[derive(Deserialize)]
pub(crate) struct DisbursementDto {
    pub id: StringOrInteger,
    #[serde(default)]
    pub date_created: Option<String>,
    pub merchant: DisbursementMerchantDto,
    pub user: UserDto,
    pub rewards_earned: bool,
}

#[derive(Deserialize)]
pub(crate) struct DisbursementMerchantDto {
    pub display_name: String,
}

#[derive(Deserialize)]
pub(crate) struct StorySocialCollectionDto<T> {
    pub count: u64,
    pub data: Vec<T>,
    #[serde(default)]
    pub pagination: Option<super::common::PaginationDto>,
}

#[derive(Deserialize)]
pub(crate) struct ActivityCommentDto {
    pub id: StringOrInteger,
    pub user: UserDto,
    pub message: String,
    pub date_created: String,
}

#[derive(Deserialize)]
pub(crate) struct ActivityCommentEnvelope {
    pub data: ActivityCommentData,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum ActivityCommentData {
    Wrapped { comment: ActivityCommentDto },
    Direct(ActivityCommentDto),
}

impl ActivityCommentData {
    pub(crate) fn into_comment(self) -> ActivityCommentDto {
        match self {
            Self::Wrapped { comment } | Self::Direct(comment) => comment,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct AddActivityCommentRequest<'a> {
    pub message: &'a str,
}

#[derive(Deserialize)]
pub(crate) struct ActivityReactionDto {
    pub emoji: String,
    pub count: u64,
    #[serde(default)]
    pub reacted_by_user: bool,
}

#[derive(Serialize)]
pub(crate) struct ActivityReactionRequest<'a> {
    pub emoji: &'a str,
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

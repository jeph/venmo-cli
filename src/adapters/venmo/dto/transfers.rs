use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize};

#[derive(Deserialize)]
pub(crate) struct TransferOptionsEnvelope {
    pub data: TransferOptionsDto,
}

#[derive(Deserialize)]
pub(crate) struct TransferOptionsDto {
    pub preferred_transfer_type: PreferredTransferTypeDto,
    pub standard: TransferModeOptionsDto,
    pub instant: TransferModeOptionsDto,
}

#[derive(Deserialize)]
pub(crate) struct PreferredTransferTypeDto {
    #[serde(default, rename = "out")]
    pub outbound: RequiredNullableString,
}

#[derive(Default)]
pub(crate) enum RequiredNullableString {
    #[default]
    Missing,
    Present(Option<String>),
}

impl<'de> Deserialize<'de> for RequiredNullableString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<String>::deserialize(deserializer).map(Self::Present)
    }
}

#[derive(Deserialize)]
pub(crate) struct TransferModeOptionsDto {
    pub eligible_destinations: Vec<TransferInstrumentDto>,
    pub fee: TransferFeeDto,
    pub transfer_to_estimate: String,
}

#[derive(Deserialize)]
pub(crate) struct TransferInstrumentDto {
    pub id: String,
    pub name: String,
    pub asset_name: String,
    #[serde(rename = "type")]
    pub instrument_type: String,
    pub last_four: String,
    pub is_default: bool,
    pub transfer_to_estimate: String,
}

#[derive(Deserialize)]
pub(crate) struct TransferFeeDto {
    #[serde(default)]
    pub minimum_amount: Option<serde_json::Number>,
    #[serde(default)]
    pub maximum_amount: Option<serde_json::Number>,
    #[serde(default)]
    pub variable_percentage: Option<serde_json::Number>,
    #[serde(flatten)]
    pub additional: HashMap<String, serde_json::Value>,
}

#[derive(Serialize)]
pub(crate) struct CreateTransferRequest<'a> {
    pub amount: u64,
    pub destination_id: &'a str,
    pub final_amount: u64,
    pub transfer_type: &'a str,
}

#[derive(Deserialize)]
pub(crate) struct CreatedTransferEnvelope {
    pub data: CreatedTransferDto,
}

#[derive(Deserialize)]
pub(crate) struct CreatedTransferDto {
    pub id: super::common::StringOrInteger,
    pub status: String,
    #[serde(rename = "type")]
    pub transfer_type: String,
    pub amount: super::common::StringOrNumber,
    pub amount_cents: u64,
    pub amount_fee_cents: u64,
    pub amount_requested_cents: u64,
    pub date_requested: String,
    pub destination: CreatedTransferDestinationDto,
}

#[derive(Deserialize)]
pub(crate) struct CreatedTransferDestinationDto {
    pub id: String,
    #[serde(rename = "type")]
    pub instrument_type: String,
    pub last_four: String,
}

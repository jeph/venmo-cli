use std::collections::HashSet;
use std::future::Future;
use std::str::FromStr;

use reqwest::StatusCode;

use crate::features::transfers::{
    CreatedTransfer, TransferFeeMetadata, TransferInstrument, TransferInstrumentId,
    TransferInstrumentSuffix, TransferModeOptions, TransferOptions, TransferOptionsApi,
    TransferOutCreationApi, TransferOutPlan, TransferSpeed,
};
use crate::shared::{AccessToken, DeviceId, Money};

use super::super::dto::{
    CreateTransferRequest, CreatedTransferEnvelope, PreferredTransferTypeDto,
    RequiredNullableString, TransferFeeDto, TransferInstrumentDto, TransferModeOptionsDto,
    TransferOptionsDto, TransferOptionsEnvelope,
};
use super::super::transport::{ApiSession, ApiTransport, HttpRequest, JsonBody};
use super::error::VenmoApiError;
use super::payments::{financial_contract_unknown, require_financial_timestamp};
use super::response::{decode_success, require_financial_success_json};
use super::support::{bounded_required_label, bounded_required_text};
use super::{TRANSFER_OPTIONS_OPERATION, TRANSFER_OUT_CREATION_OPERATION, VenmoApiClient};

const MAX_TRANSFER_INSTRUMENTS_PER_BRANCH: usize = 100;

impl<T: ApiTransport> VenmoApiClient<T> {
    async fn fetch_transfer_options(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
    ) -> Result<TransferOptions, VenmoApiError> {
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::read("/transfers/options", &["transfers", "options"], &[]),
            )
            .await?;
        let envelope: TransferOptionsEnvelope = decode_success(
            TRANSFER_OPTIONS_OPERATION,
            response,
            "the transfer-options response did not match the supported envelope",
        )?;
        map_transfer_options(envelope.data)
    }

    async fn post_standard_transfer(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        plan: &TransferOutPlan,
    ) -> Result<CreatedTransfer, VenmoApiError> {
        if plan.speed() != TransferSpeed::Standard {
            return Err(VenmoApiError::RequestEncoding {
                operation: TRANSFER_OUT_CREATION_OPERATION,
            });
        }
        let amount = plan.amount().cents();
        let body = JsonBody::encode(&CreateTransferRequest {
            amount,
            destination_id: plan.destination().id().as_str(),
            final_amount: amount,
            transfer_type: plan.speed().as_str(),
        })
        .map_err(|_| VenmoApiError::RequestEncoding {
            operation: TRANSFER_OUT_CREATION_OPERATION,
        })?;
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::financial_json_post("/transfers", &["transfers"], &[], body),
            )
            .await?;
        let status = response.status();
        let value = require_financial_success_json(TRANSFER_OUT_CREATION_OPERATION, response)?;
        if status != StatusCode::CREATED {
            return financial_contract_unknown(
                TRANSFER_OUT_CREATION_OPERATION,
                "the transfer response did not use the validated HTTP 201 status",
            );
        }
        parse_created_transfer(value, plan)
    }
}

impl<T: ApiTransport> TransferOptionsApi for VenmoApiClient<T> {
    type Error = VenmoApiError;

    fn transfer_options<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<TransferOptions, Self::Error>> + Send + 'a {
        self.fetch_transfer_options(access_token, device_id)
    }
}

impl<T: ApiTransport> TransferOutCreationApi for VenmoApiClient<T> {
    type Error = VenmoApiError;

    fn create_transfer_out<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        plan: &'a TransferOutPlan,
    ) -> impl Future<Output = Result<CreatedTransfer, Self::Error>> + Send + 'a {
        self.post_standard_transfer(access_token, device_id, plan)
    }
}

fn map_transfer_options(dto: TransferOptionsDto) -> Result<TransferOptions, VenmoApiError> {
    let TransferOptionsDto {
        preferred_transfer_type,
        standard,
        instant,
    } = dto;
    let PreferredTransferTypeDto { outbound } = preferred_transfer_type;
    Ok(TransferOptions::new(
        map_required_optional_speed(outbound)?,
        map_mode_options(standard)?,
        map_mode_options(instant)?,
    ))
}

fn map_required_optional_speed(
    value: RequiredNullableString,
) -> Result<Option<TransferSpeed>, VenmoApiError> {
    match value {
        RequiredNullableString::Missing => Err(VenmoApiError::Contract {
            operation: TRANSFER_OPTIONS_OPERATION,
            problem: "the transfer-options response omitted a preferred direction",
        }),
        RequiredNullableString::Present(value) => map_optional_speed(value),
    }
}

fn map_optional_speed(value: Option<String>) -> Result<Option<TransferSpeed>, VenmoApiError> {
    value
        .map(|value| match value.as_str() {
            "standard" => Ok(TransferSpeed::Standard),
            "instant" => Ok(TransferSpeed::Instant),
            _ => Err(VenmoApiError::Contract {
                operation: TRANSFER_OPTIONS_OPERATION,
                problem: "the transfer-options response contained an unsupported preferred speed",
            }),
        })
        .transpose()
}

fn map_mode_options(dto: TransferModeOptionsDto) -> Result<TransferModeOptions, VenmoApiError> {
    if dto.eligible_destinations.len() > MAX_TRANSFER_INSTRUMENTS_PER_BRANCH {
        return Err(VenmoApiError::Contract {
            operation: TRANSFER_OPTIONS_OPERATION,
            problem: "the transfer-options response exceeded the supported instrument count",
        });
    }
    let destinations = dto
        .eligible_destinations
        .into_iter()
        .map(map_transfer_instrument)
        .collect::<Result<Vec<_>, _>>()?;
    validate_unique_instrument_ids(&destinations)?;
    let estimate = bounded_required_text(
        dto.transfer_to_estimate,
        TRANSFER_OPTIONS_OPERATION,
        "the transfer-options response contained an invalid mode estimate",
    )?;
    Ok(TransferModeOptions::new(
        destinations,
        map_fee(dto.fee),
        estimate,
    ))
}

fn validate_unique_instrument_ids(instruments: &[TransferInstrument]) -> Result<(), VenmoApiError> {
    let mut ids = HashSet::with_capacity(instruments.len());
    if instruments
        .iter()
        .any(|instrument| !ids.insert(instrument.id()))
    {
        return Err(VenmoApiError::Contract {
            operation: TRANSFER_OPTIONS_OPERATION,
            problem: "the transfer-options response contained duplicate IDs within one branch",
        });
    }
    Ok(())
}

fn map_transfer_instrument(
    dto: TransferInstrumentDto,
) -> Result<TransferInstrument, VenmoApiError> {
    let id = TransferInstrumentId::from_str(&dto.id).map_err(|_| VenmoApiError::Contract {
        operation: TRANSFER_OPTIONS_OPERATION,
        problem: "the transfer-options response contained an invalid instrument ID",
    })?;
    let name = bounded_required_text(
        dto.name,
        TRANSFER_OPTIONS_OPERATION,
        "the transfer-options response contained an invalid instrument name",
    )?;
    let asset_name = bounded_required_text(
        dto.asset_name,
        TRANSFER_OPTIONS_OPERATION,
        "the transfer-options response contained an invalid asset name",
    )?;
    let instrument_type = bounded_required_label(
        dto.instrument_type,
        TRANSFER_OPTIONS_OPERATION,
        "the transfer-options response contained an invalid instrument type",
    )?;
    let last_four = TransferInstrumentSuffix::from_str(&dto.last_four).map_err(|_| {
        VenmoApiError::Contract {
            operation: TRANSFER_OPTIONS_OPERATION,
            problem: "the transfer-options response contained an invalid instrument suffix",
        }
    })?;
    let estimate = bounded_required_text(
        dto.transfer_to_estimate,
        TRANSFER_OPTIONS_OPERATION,
        "the transfer-options response contained an invalid instrument estimate",
    )?;
    Ok(TransferInstrument::new(
        id,
        name,
        asset_name,
        instrument_type,
        last_four,
        dto.is_default,
        estimate,
    ))
}

fn map_fee(dto: TransferFeeDto) -> TransferFeeMetadata {
    TransferFeeMetadata::new(
        dto.minimum_amount.map(|value| value.to_string()),
        dto.maximum_amount.map(|value| value.to_string()),
        dto.variable_percentage.map(|value| value.to_string()),
        dto.additional.values().any(|value| !value.is_null()),
    )
}

fn parse_created_transfer(
    value: serde_json::Value,
    plan: &TransferOutPlan,
) -> Result<CreatedTransfer, VenmoApiError> {
    let envelope: CreatedTransferEnvelope =
        serde_json::from_value(value).map_err(|_| VenmoApiError::FinancialOutcomeUnknown {
            operation: TRANSFER_OUT_CREATION_OPERATION,
            problem: "the successful response did not match the validated transfer envelope",
        })?;
    let transfer = envelope.data;
    if transfer.status != "pending" {
        return financial_contract_unknown(
            TRANSFER_OUT_CREATION_OPERATION,
            "the transfer response did not prove pending status",
        );
    }
    if transfer.transfer_type != plan.speed().as_str() {
        return financial_contract_unknown(
            TRANSFER_OUT_CREATION_OPERATION,
            "the transfer response returned a different speed",
        );
    }
    if transfer.amount_requested_cents != plan.amount().cents() {
        return financial_contract_unknown(
            TRANSFER_OUT_CREATION_OPERATION,
            "the transfer response returned a different requested amount",
        );
    }
    if transfer.amount_cents.checked_add(transfer.amount_fee_cents)
        != Some(transfer.amount_requested_cents)
    {
        return financial_contract_unknown(
            TRANSFER_OUT_CREATION_OPERATION,
            "the transfer response returned inconsistent net and fee amounts",
        );
    }
    let net_amount = Money::from_cents(transfer.amount_cents).map_err(|_| {
        VenmoApiError::FinancialOutcomeUnknown {
            operation: TRANSFER_OUT_CREATION_OPERATION,
            problem: "the transfer response returned an invalid net amount",
        }
    })?;
    let rendered_amount = Money::from_str(transfer.amount.as_str().as_ref()).map_err(|_| {
        VenmoApiError::FinancialOutcomeUnknown {
            operation: TRANSFER_OUT_CREATION_OPERATION,
            problem: "the transfer response returned an invalid displayed amount",
        }
    })?;
    if rendered_amount != net_amount {
        return financial_contract_unknown(
            TRANSFER_OUT_CREATION_OPERATION,
            "the transfer response returned conflicting amount units",
        );
    }
    let expected = plan.destination();
    let destination = transfer.destination;
    if destination.id != expected.id().as_str()
        || destination.instrument_type != expected.instrument_type()
        || destination.last_four != expected.last_four()
    {
        return financial_contract_unknown(
            TRANSFER_OUT_CREATION_OPERATION,
            "the transfer response returned a different destination",
        );
    }
    let requested_at = require_financial_timestamp(
        TRANSFER_OUT_CREATION_OPERATION,
        Some(&transfer.date_requested),
        "the transfer response omitted the request timestamp",
        "the transfer response contained an invalid request timestamp",
    )?;
    let id = crate::features::transfers::model::TransferId::from_str(transfer.id.as_str().as_ref())
        .map_err(|_| VenmoApiError::FinancialOutcomeUnknown {
            operation: TRANSFER_OUT_CREATION_OPERATION,
            problem: "the transfer response contained an invalid transfer ID",
        })?;
    Ok(CreatedTransfer::new(
        id,
        transfer.status,
        requested_at,
        net_amount,
        transfer.amount_fee_cents,
    ))
}

use std::collections::HashSet;
use std::future::Future;
use std::str::FromStr;

use reqwest::StatusCode;

use crate::features::transfers::{
    CreatedTransfer, CreatedTransferIn, TransferFeeMetadata, TransferInCreationApi, TransferInPlan,
    TransferInstrument, TransferInstrumentId, TransferInstrumentSuffix, TransferModeOptions,
    TransferOptions, TransferOptionsApi, TransferOutCreationApi, TransferOutPlan, TransferPayoutId,
    TransferSpeed,
};
use crate::shared::{AccessToken, DeviceId, Money};

use super::super::dto::{
    CreateTransferInRequest, CreateTransferRequest, CreatedTransferEnvelope,
    CreatedTransferInEnvelope, PreferredTransferTypeDto, RequiredNullableString,
    TransferAmountLimitDto, TransferFeeDto, TransferInstrumentDto, TransferModeOptionsDto,
    TransferOptionsDto, TransferOptionsEnvelope,
};
use super::super::transport::{ApiSession, ApiTransport, HttpRequest, JsonBody};
use super::error::VenmoApiError;
use super::payments::{financial_contract_unknown, require_financial_timestamp};
use super::response::{decode_success, require_financial_success_json};
use super::support::{bounded_required_label, bounded_required_text};
use super::{
    TRANSFER_IN_CREATION_OPERATION, TRANSFER_OPTIONS_OPERATION, TRANSFER_OUT_CREATION_OPERATION,
    VenmoApiClient,
};

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

    async fn post_standard_transfer_in(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        plan: &TransferInPlan,
    ) -> Result<CreatedTransferIn, VenmoApiError> {
        let body = JsonBody::encode(&CreateTransferInRequest {
            amount: plan.amount().cents(),
            payment_method_id: plan.source().id().as_str(),
            funding_request_source: "funds-in",
            instant: false,
        })
        .map_err(|_| VenmoApiError::RequestEncoding {
            operation: TRANSFER_IN_CREATION_OPERATION,
        })?;
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::financial_json_post("/funds", &["funds"], &[], body),
            )
            .await?;
        let status = response.status();
        let value = require_financial_success_json(TRANSFER_IN_CREATION_OPERATION, response)?;
        if status != StatusCode::OK {
            return financial_contract_unknown(
                TRANSFER_IN_CREATION_OPERATION,
                "the provisional add-funds contract requires HTTP 200 pending live validation",
            );
        }
        parse_created_transfer_in(value, plan)
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

impl<T: ApiTransport> TransferInCreationApi for VenmoApiClient<T> {
    type Error = VenmoApiError;

    fn create_transfer_in<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        plan: &'a TransferInPlan,
    ) -> impl Future<Output = Result<CreatedTransferIn, Self::Error>> + Send + 'a {
        self.post_standard_transfer_in(access_token, device_id, plan)
    }
}

fn map_transfer_options(dto: TransferOptionsDto) -> Result<TransferOptions, VenmoApiError> {
    let TransferOptionsDto {
        preferred_transfer_type,
        standard,
        instant,
        add_funds_single_transaction_limit,
    } = dto;
    let PreferredTransferTypeDto { inbound, outbound } = preferred_transfer_type;
    let (minimum_cents, maximum_cents) = map_add_funds_limits(add_funds_single_transaction_limit)?;
    Ok(TransferOptions::new(
        map_required_optional_speed(inbound)?,
        map_required_optional_speed(outbound)?,
        map_mode_options(standard)?,
        map_mode_options(instant)?,
    )
    .with_add_funds_limits(minimum_cents, maximum_cents))
}

fn map_add_funds_limits(
    value: Option<TransferAmountLimitDto>,
) -> Result<(Option<u64>, Option<u64>), VenmoApiError> {
    let Some(value) = value else {
        return Ok((None, None));
    };
    let minimum = value.minimum_amount.map(parse_integral_cents).transpose()?;
    let maximum = value.maximum_amount.map(parse_integral_cents).transpose()?;
    if minimum
        .zip(maximum)
        .is_some_and(|(minimum, maximum)| minimum > maximum)
    {
        return Err(VenmoApiError::Contract {
            operation: TRANSFER_OPTIONS_OPERATION,
            problem: "the add-funds transaction limits were reversed",
        });
    }
    Ok((minimum, maximum))
}

fn parse_integral_cents(value: serde_json::Number) -> Result<u64, VenmoApiError> {
    let rendered = value.to_string();
    let integer = rendered
        .split_once('.')
        .map_or(rendered.as_str(), |(integer, fraction)| {
            if fraction.chars().all(|digit| digit == '0') {
                integer
            } else {
                ""
            }
        });
    integer.parse().map_err(|_| VenmoApiError::Contract {
        operation: TRANSFER_OPTIONS_OPERATION,
        problem: "the add-funds transaction limit was not an unsigned integer number of cents",
    })
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
    if dto.eligible_sources.len() > MAX_TRANSFER_INSTRUMENTS_PER_BRANCH
        || dto.eligible_destinations.len() > MAX_TRANSFER_INSTRUMENTS_PER_BRANCH
    {
        return Err(VenmoApiError::Contract {
            operation: TRANSFER_OPTIONS_OPERATION,
            problem: "the transfer-options response exceeded the supported instrument count",
        });
    }
    let sources = dto
        .eligible_sources
        .into_iter()
        .map(map_transfer_instrument)
        .collect::<Result<Vec<_>, _>>()?;
    let destinations = dto
        .eligible_destinations
        .into_iter()
        .map(map_transfer_instrument)
        .collect::<Result<Vec<_>, _>>()?;
    validate_unique_instrument_ids(&sources)?;
    validate_unique_instrument_ids(&destinations)?;
    let estimate = bounded_required_text(
        dto.transfer_to_estimate,
        TRANSFER_OPTIONS_OPERATION,
        "the transfer-options response contained an invalid mode estimate",
    )?;
    Ok(TransferModeOptions::new(
        sources,
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

fn parse_created_transfer_in(
    value: serde_json::Value,
    plan: &TransferInPlan,
) -> Result<CreatedTransferIn, VenmoApiError> {
    let envelope: CreatedTransferInEnvelope = serde_json::from_value(value).map_err(|_| {
        VenmoApiError::FinancialOutcomeUnknown {
            operation: TRANSFER_IN_CREATION_OPERATION,
            problem: "the successful response did not match the statically evidenced add-funds envelope",
        }
    })?;
    let transfer = envelope.data;
    if transfer.status != "pending" {
        return financial_contract_unknown(
            TRANSFER_IN_CREATION_OPERATION,
            "the add-funds response did not prove pending status",
        );
    }
    if transfer.amount != plan.amount().cents() {
        return financial_contract_unknown(
            TRANSFER_IN_CREATION_OPERATION,
            "the add-funds response returned a different amount",
        );
    }
    if i32::try_from(transfer.balance).is_err() {
        return financial_contract_unknown(
            TRANSFER_IN_CREATION_OPERATION,
            "the add-funds response returned a balance outside the evidenced integer range",
        );
    }
    let payout_id = TransferPayoutId::from_str(&transfer.payout_id).map_err(|_| {
        VenmoApiError::FinancialOutcomeUnknown {
            operation: TRANSFER_IN_CREATION_OPERATION,
            problem: "the add-funds response contained an invalid payout ID",
        }
    })?;
    let expected_at = require_financial_timestamp(
        TRANSFER_IN_CREATION_OPERATION,
        Some(&transfer.expected_date),
        "the add-funds response omitted the expected-completion timestamp",
        "the add-funds response contained an invalid expected-completion timestamp",
    )?;
    Ok(CreatedTransferIn::new(
        payout_id,
        transfer.status,
        plan.amount(),
        expected_at,
    ))
}

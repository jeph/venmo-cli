use std::str::FromStr;

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::shared::ClientRequestId;

use super::super::transport::HttpResponse;
use super::error::{ApiCodeSuffix, VenmoApiError};
use super::{PAYMENT_CREATION_OPERATION, REQUEST_ACCEPTANCE_OPERATION, REQUEST_CREATION_OPERATION};

pub(super) fn decode_success<T: DeserializeOwned>(
    operation: &'static str,
    response: HttpResponse,
    problem: &'static str,
) -> Result<T, VenmoApiError> {
    let value =
        require_success_value(operation, response, true)?.ok_or(VenmoApiError::Contract {
            operation,
            problem: "the response body was empty",
        })?;
    serde_json::from_value(value).map_err(|_| VenmoApiError::Contract { operation, problem })
}

pub(super) fn require_financial_success_json(
    operation: &'static str,
    response: HttpResponse,
) -> Result<Value, VenmoApiError> {
    let status = response.status();
    if response.body().is_empty() {
        return Err(VenmoApiError::FinancialOutcomeUnknown {
            operation,
            problem: "the response body was empty",
        });
    }
    let value = serde_json::from_slice::<Value>(response.body()).map_err(|_| {
        VenmoApiError::FinancialOutcomeUnknown {
            operation,
            problem: "the response body was not valid JSON",
        }
    })?;
    let error_code = extract_error_code(&value);
    let confirmed_error_code = extract_root_error_code(&value);
    if !status.is_success() {
        if let Some(error) = confirmed_financial_rejection(
            operation,
            status.as_u16(),
            confirmed_error_code.as_deref(),
            error_code.as_deref(),
        ) {
            return Err(error);
        }
        return Err(VenmoApiError::FinancialHttpOutcomeUnknown {
            operation,
            status: status.as_u16(),
            code_suffix: ApiCodeSuffix::from_remote(error_code.as_deref()),
        });
    }
    if error_code.as_deref().is_some_and(is_failure_error_code) {
        return Err(VenmoApiError::FinancialOutcomeUnknown {
            operation,
            problem: "the successful HTTP response contained an unverified API error",
        });
    }
    Ok(value)
}

pub(super) fn is_p2p_otp_step_up_required(response: &HttpResponse) -> bool {
    response.status().as_u16() == 403
        && serde_json::from_slice::<Value>(response.body())
            .ok()
            .and_then(|value| {
                value
                    .get("error")?
                    .as_object()?
                    .get("title")?
                    .as_str()
                    .map(str::to_owned)
            })
            .as_deref()
            == Some("OTP_STEP_UP_REQUIRED")
}

pub(super) fn p2p_otp_step_up_session_id(
    operation: &'static str,
    response: &HttpResponse,
) -> Result<Option<ClientRequestId>, VenmoApiError> {
    if !is_p2p_otp_step_up_required(response) {
        return Ok(None);
    }
    let value =
        serde_json::from_slice::<Value>(response.body()).map_err(|_| VenmoApiError::Contract {
            operation,
            problem: "the SMS-verification challenge was not valid JSON",
        })?;
    let session_id = value
        .pointer("/error/metadata/uuid")
        .and_then(Value::as_str)
        .ok_or(VenmoApiError::Contract {
            operation,
            problem: "the SMS-verification challenge omitted its session UUID",
        })?;
    ClientRequestId::from_str(session_id)
        .map(Some)
        .map_err(|_| VenmoApiError::Contract {
            operation,
            problem: "the SMS-verification challenge contained an invalid session UUID",
        })
}

pub(super) fn require_state_write_success_json(
    operation: &'static str,
    response: HttpResponse,
) -> Result<Value, VenmoApiError> {
    let status = response.status();
    if !status.is_success() {
        let value = parse_response_value(operation, status, response.body())?;
        require_success_parsed_with_authentication(operation, status, value, true)?;
        return Err(VenmoApiError::StateMutationOutcomeUnknown {
            operation,
            problem: "an unsuccessful response was accepted unexpectedly",
        });
    }
    if response.body().is_empty() {
        return Err(VenmoApiError::StateMutationOutcomeUnknown {
            operation,
            problem: "the successful response body was empty",
        });
    }
    let value = serde_json::from_slice::<Value>(response.body()).map_err(|_| {
        VenmoApiError::StateMutationOutcomeUnknown {
            operation,
            problem: "the successful response body was not valid JSON",
        }
    })?;
    if extract_error_code(&value)
        .as_deref()
        .is_some_and(is_failure_error_code)
    {
        return Err(VenmoApiError::StateMutationOutcomeUnknown {
            operation,
            problem: "the successful HTTP response contained an API error",
        });
    }
    Ok(value)
}

pub(super) fn require_state_write_success(
    operation: &'static str,
    response: HttpResponse,
) -> Result<(), VenmoApiError> {
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let value = parse_response_value(operation, status, response.body())?;
    require_success_parsed_with_authentication(operation, status, value, true).map(|_| ())
}

fn confirmed_financial_rejection(
    operation: &'static str,
    status: u16,
    root_code: Option<&str>,
    displayed_code: Option<&str>,
) -> Option<VenmoApiError> {
    match (operation, status, root_code) {
        (PAYMENT_CREATION_OPERATION | REQUEST_CREATION_OPERATION, 403, Some("1360")) => {
            Some(VenmoApiError::DuplicatePaymentRejected)
        }
        (REQUEST_ACCEPTANCE_OPERATION, 400, Some("1360")) => {
            Some(VenmoApiError::DuplicateRequestAcceptanceRejected)
        }
        (PAYMENT_CREATION_OPERATION, 403, Some("10100")) => {
            Some(VenmoApiError::TemporaryPaymentRejected)
        }
        (PAYMENT_CREATION_OPERATION, 400 | 403, Some("1396"))
        | (PAYMENT_CREATION_OPERATION, 400, Some("13006")) => Some(VenmoApiError::Http {
            operation,
            status,
            code_suffix: ApiCodeSuffix::from_remote(displayed_code),
        }),
        _ => None,
    }
}

pub(super) fn require_success(
    operation: &'static str,
    response: HttpResponse,
) -> Result<(), VenmoApiError> {
    require_success_value(operation, response, false).map(|_| ())
}

fn require_success_value(
    operation: &'static str,
    response: HttpResponse,
    authenticated: bool,
) -> Result<Option<Value>, VenmoApiError> {
    let status = response.status();
    let value = parse_response_value(operation, status, response.body())?;
    require_success_parsed_with_authentication(operation, status, value, authenticated)
}

pub(super) fn require_authenticated_success(
    operation: &'static str,
    response: HttpResponse,
) -> Result<(), VenmoApiError> {
    require_success_value(operation, response, true).map(|_| ())
}

pub(super) fn parse_response_value(
    operation: &'static str,
    status: reqwest::StatusCode,
    body: &[u8],
) -> Result<Option<Value>, VenmoApiError> {
    if body.is_empty() {
        return Ok(None);
    }
    match serde_json::from_slice::<Value>(body) {
        Ok(value) => Ok(Some(value)),
        Err(_) if !status.is_success() => Ok(None),
        Err(_) => Err(VenmoApiError::MalformedJson { operation }),
    }
}

pub(super) fn require_success_parsed(
    operation: &'static str,
    status: reqwest::StatusCode,
    value: Option<Value>,
) -> Result<Option<Value>, VenmoApiError> {
    require_success_parsed_with_authentication(operation, status, value, false)
}

fn require_success_parsed_with_authentication(
    operation: &'static str,
    status: reqwest::StatusCode,
    value: Option<Value>,
    authenticated: bool,
) -> Result<Option<Value>, VenmoApiError> {
    let error_code = value.as_ref().and_then(extract_error_code);
    let code_suffix = ApiCodeSuffix::from_remote(error_code.as_deref());
    if !status.is_success() {
        return Err(if authenticated {
            VenmoApiError::AuthenticatedHttp {
                operation,
                status: status.as_u16(),
                code_suffix,
            }
        } else {
            VenmoApiError::Http {
                operation,
                status: status.as_u16(),
                code_suffix,
            }
        });
    }
    if error_code.as_deref().is_some_and(is_failure_error_code) {
        return Err(VenmoApiError::ApiFailure {
            operation,
            code_suffix,
        });
    }
    Ok(value)
}

pub(super) fn sanitize_api_code(code: &str) -> Option<String> {
    const MAX_CODE_BYTES: usize = 64;
    if code.is_empty()
        || code.len() > MAX_CODE_BYTES
        || !code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
    {
        return None;
    }
    Some(code.to_owned())
}

fn is_failure_error_code(code: &str) -> bool {
    !matches!(code, "0" | "0.0")
}

pub(super) fn extract_error_code(value: &Value) -> Option<String> {
    const PATHS: &[&[&str]] = &[
        &["error", "code"],
        &["error_code"],
        &["code"],
        &["data", "error_code"],
        &["data", "error", "code"],
    ];
    PATHS
        .iter()
        .find_map(|path| value_at_path(value, path).and_then(json_code))
}

fn extract_root_error_code(value: &Value) -> Option<String> {
    value_at_path(value, &["error", "code"]).and_then(json_code)
}

fn value_at_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter()
        .try_fold(value, |current, segment| current.get(*segment))
}

fn json_code(value: &Value) -> Option<String> {
    if let Some(code) = value.as_str() {
        Some(code.to_owned())
    } else if value.is_number() {
        Some(value.to_string())
    } else {
        None
    }
}

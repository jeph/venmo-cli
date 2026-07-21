use std::str::FromStr;

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::shared::ClientRequestId;

use super::super::transport::HttpResponse;
use super::error::{
    ApiCodeSuffix, ConfirmedFinancialRejection, FinancialErrorContext, FinancialErrorContextSuffix,
    UnsupportedFinancialContinuation, VenmoApiError,
};
use super::{
    PAYMENT_CREATION_OPERATION, REQUEST_ACCEPTANCE_OPERATION, REQUEST_CANCELLATION_OPERATION,
    REQUEST_CREATION_OPERATION, REQUEST_DECLINE_OPERATION, TRANSFER_OUT_CREATION_OPERATION,
};

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
    let root_title = extract_root_error_title(&value);
    if !status.is_success() {
        if let Some(error) = confirmed_financial_rejection(
            operation,
            status.as_u16(),
            confirmed_error_code.as_deref(),
            root_title,
            error_code.as_deref(),
        ) {
            return Err(error);
        }
        return Err(VenmoApiError::FinancialHttpOutcomeUnknown {
            operation,
            status: status.as_u16(),
            code_suffix: ApiCodeSuffix::from_remote(error_code.as_deref()),
            context_suffix: FinancialErrorContextSuffix::new(financial_error_context(
                operation,
                confirmed_error_code.as_deref(),
            )),
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
    root_title: Option<&str>,
    displayed_code: Option<&str>,
) -> Option<VenmoApiError> {
    let operation_kind = FinancialOperation::from_label(operation)?;
    match (operation_kind, status, root_code, root_title) {
        (
            FinancialOperation::PaymentCreation | FinancialOperation::RequestCreation,
            403,
            Some("1360"),
            _,
        ) => Some(VenmoApiError::DuplicatePaymentRejected),
        (FinancialOperation::RequestAcceptance, 400, Some("1360"), _) => {
            Some(VenmoApiError::DuplicateRequestAcceptanceRejected)
        }
        (FinancialOperation::PaymentCreation, 403, Some("10100"), _) => {
            Some(VenmoApiError::TemporaryPaymentRejected)
        }
        (FinancialOperation::PaymentCreation, 400 | 403, Some("1396"), _)
        | (FinancialOperation::PaymentCreation, 400, Some("13006"), _) => {
            Some(VenmoApiError::Http {
                operation,
                status,
                code_suffix: ApiCodeSuffix::from_remote(displayed_code),
            })
        }
        (
            FinancialOperation::PaymentCreation | FinancialOperation::RequestCreation,
            _,
            Some("1393"),
            _,
        ) => Some(VenmoApiError::ConfirmedFinancialRejection(
            ConfirmedFinancialRejection::PeerDeclined {
                operation: operation_kind.user_action(),
            },
        )),
        (
            FinancialOperation::PaymentCreation | FinancialOperation::RequestCreation,
            _,
            Some("10104"),
            _,
        ) => Some(VenmoApiError::ConfirmedFinancialRejection(
            ConfirmedFinancialRejection::PeerRiskDeclined {
                operation: operation_kind.user_action(),
            },
        )),
        (
            FinancialOperation::PaymentCreation | FinancialOperation::RequestCreation,
            _,
            Some("230500"),
            _,
        ) => Some(VenmoApiError::ConfirmedFinancialRejection(
            ConfirmedFinancialRejection::PendingTeenAccount {
                operation: operation_kind.user_action(),
            },
        )),
        (
            FinancialOperation::PaymentCreation | FinancialOperation::RequestCreation,
            _,
            Some("10200"),
            _,
        ) => Some(VenmoApiError::UnsupportedFinancialContinuation(
            UnsupportedFinancialContinuation::ScamWarning {
                operation: operation_kind.user_action(),
                code: "10200",
            },
        )),
        (
            FinancialOperation::PaymentCreation | FinancialOperation::RequestCreation,
            _,
            Some("10201"),
            _,
        ) => Some(VenmoApiError::UnsupportedFinancialContinuation(
            UnsupportedFinancialContinuation::ScamWarning {
                operation: operation_kind.user_action(),
                code: "10201",
            },
        )),
        (
            FinancialOperation::PaymentCreation | FinancialOperation::RequestAcceptance,
            _,
            Some("17461"),
            _,
        ) => Some(VenmoApiError::UnsupportedFinancialContinuation(
            UnsupportedFinancialContinuation::PlaidRelink {
                operation: operation_kind.user_action(),
            },
        )),
        (FinancialOperation::RequestAcceptance, 403, _, Some("RISK_DECLINED")) => {
            Some(VenmoApiError::ConfirmedFinancialRejection(
                ConfirmedFinancialRejection::RequestAcceptanceRiskDeclined,
            ))
        }
        (FinancialOperation::RequestAcceptance, 403, _, Some("RISK_INELIGIBLE")) => {
            Some(VenmoApiError::ConfirmedFinancialRejection(
                ConfirmedFinancialRejection::RequestAcceptanceRiskIneligible,
            ))
        }
        (FinancialOperation::TransferOutCreation, 403, _, Some("OTP_STEP_UP_REQUIRED")) => {
            Some(VenmoApiError::UnsupportedFinancialContinuation(
                UnsupportedFinancialContinuation::TransferSmsVerification,
            ))
        }
        _ => None,
    }
}

fn financial_error_context(
    operation: &'static str,
    root_code: Option<&str>,
) -> Option<FinancialErrorContext> {
    let operation = FinancialOperation::from_label(operation)?;
    let code = root_code?.parse::<u32>().ok()?;
    match operation {
        FinancialOperation::PaymentCreation | FinancialOperation::RequestCreation => match code {
            10101..=10103 | 10105..=10199 => Some(FinancialErrorContext::PeerRiskDecline),
            60000..=60099 => Some(FinancialErrorContext::PeerDecline),
            _ => None,
        },
        FinancialOperation::TransferOutCreation => match code {
            1319 | 9903 => Some(FinancialErrorContext::TransferInsufficientFunds),
            1358 | 1346 | 1757 | 1758 | 1766 | 13010 | 80907 => {
                Some(FinancialErrorContext::TransferAmountOrLimit)
            }
            5204 | 5207 | 9902 | 1734 => Some(FinancialErrorContext::TransferMethodUnavailable),
            1361 | 1362 | 1364 | 81005 => Some(FinancialErrorContext::TransferAccountRestriction),
            1760 | 1763 | 99027 => Some(FinancialErrorContext::TransferRiskDecline),
            _ => None,
        },
        FinancialOperation::RequestAcceptance
        | FinancialOperation::RequestDecline
        | FinancialOperation::RequestCancellation => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FinancialOperation {
    PaymentCreation,
    RequestCreation,
    RequestAcceptance,
    RequestDecline,
    RequestCancellation,
    TransferOutCreation,
}

impl FinancialOperation {
    fn from_label(operation: &'static str) -> Option<Self> {
        match operation {
            PAYMENT_CREATION_OPERATION => Some(Self::PaymentCreation),
            REQUEST_CREATION_OPERATION => Some(Self::RequestCreation),
            REQUEST_ACCEPTANCE_OPERATION => Some(Self::RequestAcceptance),
            REQUEST_DECLINE_OPERATION => Some(Self::RequestDecline),
            REQUEST_CANCELLATION_OPERATION => Some(Self::RequestCancellation),
            TRANSFER_OUT_CREATION_OPERATION => Some(Self::TransferOutCreation),
            _ => None,
        }
    }

    const fn user_action(self) -> &'static str {
        match self {
            Self::PaymentCreation => "payment",
            Self::RequestCreation => "request",
            Self::RequestAcceptance => "request acceptance",
            Self::RequestDecline => "request decline",
            Self::RequestCancellation => "request cancellation",
            Self::TransferOutCreation => "outbound transfer",
        }
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

fn extract_root_error_title(value: &Value) -> Option<&str> {
    value_at_path(value, &["error", "title"]).and_then(Value::as_str)
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

use thiserror::Error;

use crate::shared::{ApiFailure, ApiFailureKind};

use super::super::transport::TransportError;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub(crate) enum VenmoApiError {
    #[error(transparent)]
    Transport(#[from] TransportError),

    #[error("Venmo rejected the {operation} request with HTTP {status}{code_suffix}")]
    Http {
        operation: &'static str,
        status: u16,
        code_suffix: ApiCodeSuffix,
    },

    #[error("Venmo rejected the authenticated {operation} request with HTTP {status}{code_suffix}")]
    AuthenticatedHttp {
        operation: &'static str,
        status: u16,
        code_suffix: ApiCodeSuffix,
    },

    #[error("Venmo reported that the {operation} request failed{code_suffix}")]
    ApiFailure {
        operation: &'static str,
        code_suffix: ApiCodeSuffix,
    },

    #[error("Venmo reported that this personal payment is not eligible")]
    EligibilityDenied,

    #[error("Venmo reported that this payment is not eligible for Purchase Protection")]
    ProtectedPaymentEligibilityDenied,

    #[error(
        "Venmo reported that this request approval is not eligible for the selected funding method"
    )]
    RequestApprovalEligibilityDenied,

    #[error(
        "Venmo rejected this payment or request as a duplicate (error code 1360); the same person, amount, and note were used within Venmo's 10-minute duplicate window. Check activity and requests before trying again"
    )]
    DuplicatePaymentRejected,

    #[error(
        "Venmo rejected this request acceptance because a matching payment or charge was recently submitted (error code 1360). This acceptance did not satisfy the request; check activity and request status before retrying because a later retry could send a second payment"
    )]
    DuplicateRequestAcceptanceRejected,

    #[error(
        "Venmo's server-side checks blocked this payment (error code 10100); try again later or use the official Venmo app"
    )]
    TemporaryPaymentRejected,

    #[error(transparent)]
    ConfirmedFinancialRejection(ConfirmedFinancialRejection),

    #[error(transparent)]
    UnsupportedFinancialContinuation(UnsupportedFinancialContinuation),

    #[error("the successful {operation} response was not valid JSON")]
    MalformedJson { operation: &'static str },

    #[error("failed to encode the {operation} request")]
    RequestEncoding { operation: &'static str },

    #[error(
        "the successful {operation} response could not prove the issued token; authentication outcome is unknown and a remote token may remain active because {problem}"
    )]
    AuthenticationOutcomeUnknown {
        operation: &'static str,
        problem: &'static str,
    },

    #[error(
        "the {operation} outcome is unknown and must be reconciled before retrying because {problem}"
    )]
    FinancialOutcomeUnknown {
        operation: &'static str,
        problem: &'static str,
    },

    #[error(
        "the {operation} outcome is unknown and must be reconciled before retrying because Venmo returned HTTP {status}{code_suffix} without proving that no write occurred{context_suffix}"
    )]
    FinancialHttpOutcomeUnknown {
        operation: &'static str,
        status: u16,
        code_suffix: ApiCodeSuffix,
        context_suffix: FinancialErrorContextSuffix,
    },

    #[error(
        "the {operation} outcome is unknown and must be reconciled before retrying because {problem}"
    )]
    StateMutationOutcomeUnknown {
        operation: &'static str,
        problem: &'static str,
    },

    #[error("cannot use the {operation} response because {problem}")]
    Contract {
        operation: &'static str,
        problem: &'static str,
    },
}

impl ApiFailure for VenmoApiError {
    fn kind(&self) -> ApiFailureKind {
        match self {
            Self::Transport(TransportError::Timeout) => ApiFailureKind::Timeout,
            Self::Transport(
                TransportError::Network
                | TransportError::UnexpectedRedirect
                | TransportError::ResponseRead,
            ) => ApiFailureKind::Network,
            Self::Transport(TransportError::FinancialWriteOutcomeUnknown { .. })
            | Self::FinancialOutcomeUnknown { .. }
            | Self::FinancialHttpOutcomeUnknown { .. } => ApiFailureKind::AmbiguousWrite,
            Self::Transport(TransportError::StateWriteOutcomeUnknown { .. })
            | Self::StateMutationOutcomeUnknown { .. } => ApiFailureKind::AmbiguousWrite,
            Self::Transport(TransportError::AuthenticationOutcomeUnknown { .. }) => {
                ApiFailureKind::Internal
            }
            Self::Transport(
                TransportError::ResponseTooLarge { .. }
                | TransportError::InvalidRoute
                | TransportError::InvalidQuery
                | TransportError::InvalidContinuationLink
                | TransportError::InvalidAuthenticationResponseHeader,
            )
            | Self::MalformedJson { .. }
            | Self::Contract { .. } => ApiFailureKind::Contract,
            Self::Transport(
                TransportError::InvalidAuthenticationHeader
                | TransportError::RequestConstruction
                | TransportError::ResourceExhaustion,
            )
            | Self::RequestEncoding { .. }
            | Self::AuthenticationOutcomeUnknown { .. } => ApiFailureKind::Internal,
            Self::AuthenticatedHttp { status: 401, .. } => ApiFailureKind::Authentication,
            Self::Http { .. }
            | Self::AuthenticatedHttp { .. }
            | Self::ApiFailure { .. }
            | Self::EligibilityDenied
            | Self::ProtectedPaymentEligibilityDenied
            | Self::RequestApprovalEligibilityDenied
            | Self::DuplicatePaymentRejected
            | Self::DuplicateRequestAcceptanceRejected
            | Self::TemporaryPaymentRejected
            | Self::ConfirmedFinancialRejection(_)
            | Self::UnsupportedFinancialContinuation(_) => ApiFailureKind::Rejected,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum ConfirmedFinancialRejection {
    #[error(
        "Venmo declined the {operation} (error code 1393). Check activity and requests before trying again, and use the official Venmo app if the decline persists"
    )]
    PeerDeclined { operation: &'static str },

    #[error(
        "Venmo's risk checks declined the {operation} (error code 10104). Check activity and requests before trying again; use the official Venmo app or contact Venmo if the decline persists"
    )]
    PeerRiskDeclined { operation: &'static str },

    #[error(
        "Venmo rejected the {operation} because error code 230500 indicates a pending teen-account restriction. Review the account in the official Venmo app before trying again"
    )]
    PendingTeenAccount { operation: &'static str },

    #[error(
        "Venmo's risk checks declined this request acceptance. The request was not accepted; check its current status and use the official Venmo app before trying again"
    )]
    RequestAcceptanceRiskDeclined,

    #[error(
        "Venmo reported that this request acceptance is not eligible under its risk checks. The request was not accepted; check its current status and use the official Venmo app before trying again"
    )]
    RequestAcceptanceRiskIneligible,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum UnsupportedFinancialContinuation {
    #[error(
        "Venmo requires an in-app scam-warning review before the {operation} can continue (error code {code}). The CLI will not bypass that review or retry the transaction; review the recipient and use the official Venmo app only if you still intend to proceed"
    )]
    ScamWarning {
        operation: &'static str,
        code: &'static str,
    },

    #[error(
        "Venmo requires the linked bank to be reconnected with Plaid before the {operation} can continue (error code 17461). Reconnect it in the official Venmo app; the CLI did not retry the transaction"
    )]
    PlaidRelink { operation: &'static str },

    #[error(
        "Venmo requires SMS verification before this outbound transfer can continue. The CLI does not support transfer verification and did not retry the transfer; use the official Venmo app"
    )]
    TransferSmsVerification,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FinancialErrorContext {
    PeerRiskDecline,
    PeerDecline,
    TransferInsufficientFunds,
    TransferAmountOrLimit,
    TransferMethodUnavailable,
    TransferAccountRestriction,
    TransferRiskDecline,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct FinancialErrorContextSuffix(Option<FinancialErrorContext>);

impl FinancialErrorContextSuffix {
    pub(super) const fn new(context: Option<FinancialErrorContext>) -> Self {
        Self(context)
    }
}

impl std::fmt::Display for FinancialErrorContextSuffix {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Some(context) = self.0 else {
            return Ok(());
        };
        let guidance = match context {
            FinancialErrorContext::PeerRiskDecline => {
                "; this peer-transaction code indicates a risk decline. Check activity and requests before retrying, then use the official app or contact Venmo if nothing was recorded"
            }
            FinancialErrorContext::PeerDecline => {
                "; this peer-transaction code indicates a decline. Check activity and requests before retrying"
            }
            FinancialErrorContext::TransferInsufficientFunds => {
                "; this outbound-transfer code indicates insufficient available funds. Check balance and activity before retrying"
            }
            FinancialErrorContext::TransferAmountOrLimit => {
                "; this outbound-transfer code indicates an invalid amount, transfer limit, or rate limit. Review transfer limits and check activity before retrying"
            }
            FinancialErrorContext::TransferMethodUnavailable => {
                "; this outbound-transfer code indicates an unavailable or unlinked bank method. Review linked banks and activity in the official app before retrying"
            }
            FinancialErrorContext::TransferAccountRestriction => {
                "; this outbound-transfer code indicates an account, identity, or location restriction. Review the account in the official app and check activity before retrying"
            }
            FinancialErrorContext::TransferRiskDecline => {
                "; this outbound-transfer code indicates a transfer or risk decline. Check activity before retrying, then use the official app or contact Venmo"
            }
        };
        formatter.write_str(guidance)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ApiCodeSuffix(Option<String>);

impl ApiCodeSuffix {
    pub(super) fn from_remote(code: Option<&str>) -> Self {
        Self(code.and_then(super::response::sanitize_api_code))
    }
}

impl std::fmt::Display for ApiCodeSuffix {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Some(code) => write!(formatter, " (error code {code})"),
            None => Ok(()),
        }
    }
}

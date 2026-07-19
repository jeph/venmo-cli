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
            | Self::FinancialOutcomeUnknown { .. } => ApiFailureKind::AmbiguousWrite,
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
            | Self::EligibilityDenied => ApiFailureKind::Rejected,
        }
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

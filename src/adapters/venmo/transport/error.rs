use std::fmt;

use thiserror::Error;

use super::request::OperationClass;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AmbiguousWriteCause {
    Timeout,
    Network,
    Redirect,
    ResponseCapture,
    ResponseRead,
    ResponseTooLarge,
    ResourceExhaustion,
}

impl fmt::Display for AmbiguousWriteCause {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Timeout => "the request timed out after transmission may have begun",
            Self::Network => "the connection failed after transmission may have begun",
            Self::Redirect => "the server returned an unexpected redirect",
            Self::ResponseCapture => "the response metadata could not be captured safely",
            Self::ResponseRead => "the response ended before it could be read completely",
            Self::ResponseTooLarge => "the response exceeded the configured safety limit",
            Self::ResourceExhaustion => "the response could not be buffered safely",
        })
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum TransportBuildError {
    #[error("the production Venmo API origin is invalid")]
    InvalidProductionOrigin,
    #[error("the HTTP transport requires a fixed HTTPS origin")]
    InsecureOrigin,
    #[error("the HTTP transport origin cannot contain credentials, query data, or a fragment")]
    UnsafeOrigin,
    #[error("the HTTP response size limit must be positive")]
    InvalidResponseLimit,
    #[error("failed to initialize the HTTPS client")]
    ClientInitialization,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum TransportError {
    #[error("the API route is invalid")]
    InvalidRoute,
    #[error("the API query is invalid")]
    InvalidQuery,
    #[error("the API returned an untrusted or malformed continuation link")]
    InvalidContinuationLink,
    #[error("failed to construct a sensitive API header")]
    InvalidAuthenticationHeader,
    #[error("the API returned an invalid authentication response header")]
    InvalidAuthenticationResponseHeader,
    #[error("failed to construct the API request")]
    RequestConstruction,
    #[error("the API request timed out")]
    Timeout,
    #[error("the API request failed before a usable response was received")]
    Network,
    #[error("the API returned an unexpected redirect")]
    UnexpectedRedirect,
    #[error("the API response exceeded the {maximum_bytes}-byte safety limit")]
    ResponseTooLarge { maximum_bytes: usize },
    #[error("the API response ended before it could be read completely")]
    ResponseRead,
    #[error("the API response could not be buffered safely")]
    ResourceExhaustion,
    #[error(
        "financial write outcome is unknown: {cause}; do not retry until activity or requests and the official app verify the result"
    )]
    FinancialWriteOutcomeUnknown { cause: AmbiguousWriteCause },
    #[error(
        "state-changing write outcome is unknown: {cause}; do not retry until the remote state is verified"
    )]
    StateWriteOutcomeUnknown { cause: AmbiguousWriteCause },
    #[error("authentication outcome is unknown: {cause}; a remote token may have been issued")]
    AuthenticationOutcomeUnknown { cause: AmbiguousWriteCause },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PostSendFailure {
    Redirect,
    ResponseCapture,
    ResponseRead,
    ResponseTooLarge,
    ResourceExhaustion,
}

pub(super) fn classify_send_error(
    operation: OperationClass,
    error: &reqwest::Error,
) -> TransportError {
    if error.is_connect() {
        return if error.is_timeout() {
            TransportError::Timeout
        } else {
            TransportError::Network
        };
    }
    let cause = if error.is_timeout() {
        AmbiguousWriteCause::Timeout
    } else {
        AmbiguousWriteCause::Network
    };
    match operation {
        OperationClass::FinancialWrite => {
            return TransportError::FinancialWriteOutcomeUnknown { cause };
        }
        OperationClass::StateWrite => {
            return TransportError::StateWriteOutcomeUnknown { cause };
        }
        OperationClass::AuthenticationWrite => {
            return TransportError::AuthenticationOutcomeUnknown { cause };
        }
        OperationClass::Read | OperationClass::NonFinancialWrite => {}
    }

    if error.is_timeout() {
        TransportError::Timeout
    } else {
        TransportError::Network
    }
}

pub(super) fn classify_post_send_failure(
    operation: OperationClass,
    failure: PostSendFailure,
    maximum_bytes: usize,
) -> TransportError {
    let cause = match failure {
        PostSendFailure::Redirect => AmbiguousWriteCause::Redirect,
        PostSendFailure::ResponseCapture => AmbiguousWriteCause::ResponseCapture,
        PostSendFailure::ResponseRead => AmbiguousWriteCause::ResponseRead,
        PostSendFailure::ResponseTooLarge => AmbiguousWriteCause::ResponseTooLarge,
        PostSendFailure::ResourceExhaustion => AmbiguousWriteCause::ResourceExhaustion,
    };
    match operation {
        OperationClass::FinancialWrite => {
            return TransportError::FinancialWriteOutcomeUnknown { cause };
        }
        OperationClass::StateWrite => {
            return TransportError::StateWriteOutcomeUnknown { cause };
        }
        OperationClass::AuthenticationWrite => {
            return TransportError::AuthenticationOutcomeUnknown { cause };
        }
        OperationClass::Read | OperationClass::NonFinancialWrite => {}
    }

    match failure {
        PostSendFailure::Redirect => TransportError::UnexpectedRedirect,
        PostSendFailure::ResponseCapture => TransportError::InvalidAuthenticationResponseHeader,
        PostSendFailure::ResponseRead => TransportError::ResponseRead,
        PostSendFailure::ResponseTooLarge => TransportError::ResponseTooLarge { maximum_bytes },
        PostSendFailure::ResourceExhaustion => TransportError::ResourceExhaustion,
    }
}

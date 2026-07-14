use std::io;

use thiserror::Error;

use crate::adapters::venmo::TransportBuildError;
use crate::features::activity::ActivityError;
use crate::features::auth::{AuthStatusError, LoginError};
use crate::features::payments::pay::PayError;
use crate::features::people::friends::FriendsError;
use crate::features::people::users::{UserSearchError, UserSearchFailureKind};
use crate::features::requests::accept::AcceptError;
use crate::features::requests::create::RequestCreateError;
use crate::features::requests::decline::DeclineError;
use crate::features::requests::list::RequestsError;
use crate::features::wallet::balance::BalanceError;
use crate::features::wallet::payment_methods::PaymentMethodsError;
use crate::shared::{ApiFailureKind, ApplicationFailureKind, ReadFailureKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorCategory {
    Usage,
    Cancelled,
    Credential,
    Network,
    Timeout,
    Api,
    ApiContract,
    AmbiguousWrite,
    Internal,
}

impl ErrorCategory {
    #[must_use]
    pub const fn exit_code(self) -> u8 {
        match self {
            Self::AmbiguousWrite => 3,
            Self::Usage => 2,
            Self::Cancelled
            | Self::Credential
            | Self::Network
            | Self::Timeout
            | Self::Api
            | Self::ApiContract
            | Self::Internal => 1,
        }
    }
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error(
        "the `{command}` command is implemented as a candidate but unavailable pending controlled live validation"
    )]
    CommandUnavailable { command: &'static str },

    #[error("failed to write shell completions")]
    CompletionOutput {
        #[source]
        source: io::Error,
    },

    #[error("failed to initialize verbose diagnostics")]
    LoggingInitialization {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("failed to initialize the asynchronous runtime")]
    RuntimeInitialization {
        #[source]
        source: io::Error,
    },

    #[error("failed to install financial-write interrupt protection")]
    SignalInitialization {
        #[source]
        source: io::Error,
    },

    #[error(
        "the financial operation was interrupted after transmission may have begun; do not retry until its status is verified independently"
    )]
    FinancialWriteInterruptedUnknown,

    #[error("failed to initialize the Venmo API client")]
    ApiInitialization {
        #[source]
        source: TransportBuildError,
    },

    #[error(transparent)]
    AuthLogin {
        #[from]
        source: LoginError,
    },

    #[error(transparent)]
    AuthStatus {
        #[from]
        source: AuthStatusError,
    },

    #[error("authentication logout was incomplete; review the reported results")]
    AuthLogoutIncomplete { kind: ApplicationFailureKind },

    #[error(
        "authentication succeeded, but device trust was incomplete; review the reported results"
    )]
    AuthLoginIncomplete { kind: ApiFailureKind },

    #[error(
        "authentication state may already have changed, but the command result could not be written completely; verify local state with `venmo auth status` and review official Venmo session controls before retrying"
    )]
    AuthStateOutput {
        #[source]
        source: io::Error,
    },

    #[error(transparent)]
    PaymentMethods {
        #[from]
        source: PaymentMethodsError,
    },

    #[error(transparent)]
    Pay {
        #[from]
        source: PayError,
    },

    #[error(transparent)]
    RequestCreate {
        #[from]
        source: RequestCreateError,
    },

    #[error(transparent)]
    Accept {
        #[from]
        source: AcceptError,
    },

    #[error(transparent)]
    Decline {
        #[from]
        source: DeclineError,
    },

    #[error(transparent)]
    UserSearch {
        #[from]
        source: UserSearchError,
    },

    #[error(transparent)]
    Friends {
        #[from]
        source: FriendsError,
    },

    #[error(transparent)]
    Balance {
        #[from]
        source: BalanceError,
    },

    #[error(transparent)]
    Activity {
        #[from]
        source: ActivityError,
    },

    #[error(transparent)]
    Requests {
        #[from]
        source: RequestsError,
    },

    #[error("doctor found one or more required failures; review the report")]
    DoctorIncomplete,

    #[error("failed to write command output")]
    CommandOutput {
        #[from]
        #[source]
        source: io::Error,
    },

    #[error(
        "the financial operation succeeded, but its result could not be written; do not retry it and verify the result through activity or requests and the official Venmo app"
    )]
    FinancialResultOutput {
        #[source]
        source: io::Error,
    },
}

impl AppError {
    #[must_use]
    pub const fn category(&self) -> ErrorCategory {
        match self {
            Self::AuthLogin { source } => application_failure_category(source.failure_kind()),
            Self::AuthStatus { source } => application_failure_category(source.failure_kind()),
            Self::AuthLogoutIncomplete { kind } => application_failure_category(*kind),
            Self::AuthLoginIncomplete { kind } => api_failure_category(*kind),
            Self::PaymentMethods { source } => application_failure_category(source.failure_kind()),
            Self::Pay { source } => application_failure_category(source.failure_kind()),
            Self::RequestCreate { source } => application_failure_category(source.failure_kind()),
            Self::Accept { source } => application_failure_category(source.failure_kind()),
            Self::Decline { source } => application_failure_category(source.failure_kind()),
            Self::UserSearch { source } => match source.failure_kind() {
                UserSearchFailureKind::Credential => ErrorCategory::Credential,
                UserSearchFailureKind::Api(kind) => api_failure_category(kind),
                UserSearchFailureKind::ResponseContract => ErrorCategory::ApiContract,
                UserSearchFailureKind::Internal => ErrorCategory::Internal,
            },
            Self::Friends { source } => read_failure_category(source.failure_kind()),
            Self::Balance { source } => read_failure_category(source.failure_kind()),
            Self::Activity { source } => read_failure_category(source.failure_kind()),
            Self::Requests { source } => read_failure_category(source.failure_kind()),
            Self::DoctorIncomplete => ErrorCategory::Api,
            Self::FinancialWriteInterruptedUnknown => ErrorCategory::AmbiguousWrite,
            Self::FinancialResultOutput { .. } => ErrorCategory::AmbiguousWrite,
            Self::CommandUnavailable { .. }
            | Self::CompletionOutput { .. }
            | Self::LoggingInitialization { .. }
            | Self::RuntimeInitialization { .. }
            | Self::SignalInitialization { .. }
            | Self::ApiInitialization { .. }
            | Self::AuthStateOutput { .. }
            | Self::CommandOutput { .. } => ErrorCategory::Internal,
        }
    }

    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        self.category().exit_code()
    }
}

const fn application_failure_category(kind: ApplicationFailureKind) -> ErrorCategory {
    match kind {
        ApplicationFailureKind::Credential => ErrorCategory::Credential,
        ApplicationFailureKind::Usage => ErrorCategory::Usage,
        ApplicationFailureKind::Cancelled => ErrorCategory::Cancelled,
        ApplicationFailureKind::Api(kind) => api_failure_category(kind),
        ApplicationFailureKind::ApiContract => ErrorCategory::ApiContract,
        ApplicationFailureKind::Internal => ErrorCategory::Internal,
    }
}

const fn api_failure_category(kind: ApiFailureKind) -> ErrorCategory {
    match kind {
        ApiFailureKind::Network => ErrorCategory::Network,
        ApiFailureKind::Timeout => ErrorCategory::Timeout,
        ApiFailureKind::Rejected => ErrorCategory::Api,
        ApiFailureKind::Contract => ErrorCategory::ApiContract,
        ApiFailureKind::AmbiguousWrite => ErrorCategory::AmbiguousWrite,
        ApiFailureKind::Internal => ErrorCategory::Internal,
    }
}

const fn read_failure_category(kind: ReadFailureKind) -> ErrorCategory {
    match kind {
        ReadFailureKind::Credential => ErrorCategory::Credential,
        ReadFailureKind::Api(kind) => api_failure_category(kind),
        ReadFailureKind::ResponseContract => ErrorCategory::ApiContract,
        ReadFailureKind::Internal => ErrorCategory::Internal,
    }
}

#[cfg(test)]
mod tests;

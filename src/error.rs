use std::io;

use thiserror::Error;

use crate::application::activity::ActivityError;
use crate::application::auth::{AuthStatusError, LoginError};
use crate::application::balance::BalanceError;
use crate::application::friends::FriendsError;
use crate::application::payment_methods::PaymentMethodsError;
use crate::application::ports::{ApiFailureKind, PromptError};
use crate::application::read::ReadFailureKind;
use crate::application::requests::RequestsError;
use crate::application::users::{UserSearchError, UserSearchFailureKind};
use crate::infrastructure::venmo_api::TransportBuildError;

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
    #[error("the `{command}` command is not implemented in this development build")]
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
    AuthLogoutIncomplete,

    #[error(
        "authentication succeeded, but device trust was incomplete; review the reported results"
    )]
    AuthLoginIncomplete,

    #[error(transparent)]
    PaymentMethods {
        #[from]
        source: PaymentMethodsError,
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
        #[source]
        source: io::Error,
    },
}

impl AppError {
    #[must_use]
    pub const fn category(&self) -> ErrorCategory {
        match self {
            Self::AuthLogin { source } => match source {
                LoginError::Prompt(PromptError::Cancelled) => ErrorCategory::Cancelled,
                LoginError::CredentialLoad { .. }
                | LoginError::CredentialAlreadyStored
                | LoginError::ReauthenticationCredentialMissing
                | LoginError::DifferentAccount
                | LoginError::IssuedTokenDifferentAccount
                | LoginError::CredentialStorageStateUnknown { .. }
                | LoginError::IssuedCredentialStorageStateUnknown { .. } => {
                    ErrorCategory::Credential
                }
                LoginError::TokenValidation { .. }
                | LoginError::IssuedTokenValidation { .. }
                | LoginError::PasswordAuthentication { .. }
                | LoginError::OtpRequest { .. }
                | LoginError::OtpCompletion { .. } => ErrorCategory::Api,
                LoginError::Prompt(_) => ErrorCategory::Internal,
            },
            Self::AuthStatus { source } => match source {
                AuthStatusError::MissingCredential
                | AuthStatusError::CredentialLoad { .. }
                | AuthStatusError::AccountMismatch => ErrorCategory::Credential,
                AuthStatusError::TokenValidation { .. } => ErrorCategory::Api,
            },
            Self::AuthLogoutIncomplete => ErrorCategory::Credential,
            Self::AuthLoginIncomplete => ErrorCategory::Api,
            Self::PaymentMethods { source } => match source.api_failure_kind() {
                Some(kind) => api_failure_category(kind),
                None => ErrorCategory::Credential,
            },
            Self::UserSearch { source } => match source.failure_kind() {
                UserSearchFailureKind::Credential => ErrorCategory::Credential,
                UserSearchFailureKind::Api(kind) => api_failure_category(kind),
                UserSearchFailureKind::PaginationContract => ErrorCategory::ApiContract,
                UserSearchFailureKind::Internal => ErrorCategory::Internal,
            },
            Self::Friends { source } => read_failure_category(source.failure_kind()),
            Self::Balance { source } => read_failure_category(source.failure_kind()),
            Self::Activity { source } => read_failure_category(source.failure_kind()),
            Self::Requests { source } => read_failure_category(source.failure_kind()),
            Self::DoctorIncomplete => ErrorCategory::Api,
            Self::CommandUnavailable { .. }
            | Self::CompletionOutput { .. }
            | Self::LoggingInitialization { .. }
            | Self::RuntimeInitialization { .. }
            | Self::ApiInitialization { .. }
            | Self::CommandOutput { .. } => ErrorCategory::Internal,
        }
    }

    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        self.category().exit_code()
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
        ReadFailureKind::PaginationContract => ErrorCategory::ApiContract,
        ReadFailureKind::Internal => ErrorCategory::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_continuation_failures_are_contract_errors() {
        let continuation = AppError::from(FriendsError::PaginationContract {
            problem: "synthetic continuation failure",
        });

        assert_eq!(continuation.category(), ErrorCategory::ApiContract);
        assert_eq!(continuation.exit_code(), 1);
    }
}

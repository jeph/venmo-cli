use serde_json::Value;

use super::command::{CommandId, OutputFormat};
use super::error::{AppError, ErrorCategory};
use crate::features::auth::LoginError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FailureOutcome {
    NotPerformed,
    Partial,
    Unknown,
    Completed,
}

impl FailureOutcome {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::NotPerformed => "not_performed",
            Self::Partial => "partial",
            Self::Unknown => "unknown",
            Self::Completed => "completed",
        }
    }
}

pub struct CliFailure {
    pub(crate) error: AppError,
    pub(crate) command: CommandId,
    pub(crate) format: OutputFormat,
    pub(crate) outcome: FailureOutcome,
    pub(crate) plan: Option<Value>,
    pub(crate) partial_result: Option<Value>,
}

impl CliFailure {
    pub(crate) fn plain(error: AppError, command: CommandId, format: OutputFormat) -> Self {
        let outcome = intrinsic_outcome(&error);
        Self {
            error,
            command,
            format,
            outcome,
            plan: None,
            partial_result: None,
        }
    }

    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        self.error.exit_code()
    }

    #[must_use]
    pub const fn format(&self) -> OutputFormat {
        self.format
    }

    #[must_use]
    pub const fn error(&self) -> &AppError {
        &self.error
    }
}

impl std::fmt::Debug for CliFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CliFailure")
            .field("error", &self.error)
            .field("command", &self.command)
            .field("format", &self.format)
            .field("outcome", &self.outcome)
            .field("plan", &self.plan.as_ref().map(|_| "[REDACTED]"))
            .field(
                "partial_result",
                &self.partial_result.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

pub(crate) fn error_code(error: &AppError) -> &'static str {
    match error {
        AppError::FinancialWriteInterruptedUnknown | AppError::StateWriteInterruptedUnknown => {
            "write_outcome_unknown"
        }
        AppError::FinancialResultOutput { .. }
        | AppError::StateMutationResultOutput { .. }
        | AppError::AuthStateOutput { .. } => "result_output_failed",
        AppError::AuthLoginIncomplete { .. } => "device_trust_incomplete",
        AppError::AuthLogoutIncomplete { .. } => "logout_incomplete",
        AppError::CredentialFallbackCleanup { .. } => "credential_cleanup_incomplete",
        AppError::AuthLogin {
            source: LoginError::IssuedCredentialStorageStateUnknown { .. },
        } => "credential_state_unknown",
        _ => match error.category() {
            ErrorCategory::Usage => "usage_error",
            ErrorCategory::Cancelled => "cancelled",
            ErrorCategory::Credential => "credential_error",
            ErrorCategory::Authentication => "authentication_required",
            ErrorCategory::Network => "network_error",
            ErrorCategory::Timeout => "timeout",
            ErrorCategory::Api => "api_rejected",
            ErrorCategory::ApiContract => "api_contract_violation",
            ErrorCategory::AmbiguousWrite => "write_outcome_unknown",
            ErrorCategory::Internal => "internal_error",
        },
    }
}

pub(crate) const fn intrinsic_outcome(error: &AppError) -> FailureOutcome {
    match error {
        AppError::AuthLoginIncomplete { .. }
        | AppError::AuthLogoutIncomplete { .. }
        | AppError::CredentialFallbackCleanup { .. }
        | AppError::AuthLogin {
            source: LoginError::IssuedTokenValidation { .. },
        } => FailureOutcome::Partial,
        AppError::AuthLogin {
            source: LoginError::IssuedCredentialStorageStateUnknown { .. },
        }
        | AppError::FinancialWriteInterruptedUnknown
        | AppError::StateWriteInterruptedUnknown => FailureOutcome::Unknown,
        AppError::FinancialResultOutput { .. } | AppError::StateMutationResultOutput { .. } => {
            FailureOutcome::Completed
        }
        _ if matches!(error.category(), ErrorCategory::AmbiguousWrite) => FailureOutcome::Unknown,
        _ => FailureOutcome::NotPerformed,
    }
}

use super::super::{api_operation_failure, operation_failure};
use crate::adapters::cli::error::{AppError, ErrorCategory};
use crate::features::auth::PromptError;
use crate::features::payments::FinancialValidationError;
use crate::features::requests::RequestMutationPreflightError;
use crate::features::requests::accept::AcceptError;
use crate::features::requests::decline::DeclineError;
use crate::features::requests::validation::RequestMutationValidationError;
use crate::shared::{ApiFailureKind, CredentialAccessError};

#[test]
fn request_mutation_variants_have_deliberate_categories() {
    let cases = [
        (
            AppError::from(AcceptError::Preflight(
                RequestMutationPreflightError::Credential(CredentialAccessError::Missing),
            )),
            ErrorCategory::Credential,
        ),
        (
            AppError::from(AcceptError::Preflight(
                RequestMutationPreflightError::Credential(CredentialAccessError::Read {
                    source: operation_failure(),
                }),
            )),
            ErrorCategory::Credential,
        ),
        (
            AppError::from(AcceptError::Preflight(
                RequestMutationPreflightError::CurrentAccount {
                    source: api_operation_failure(ApiFailureKind::Network),
                },
            )),
            ErrorCategory::Network,
        ),
        (
            AppError::from(AcceptError::AccountOrRequester(
                FinancialValidationError::MissingProfileType,
            )),
            ErrorCategory::ApiContract,
        ),
        (
            AppError::from(AcceptError::Preflight(
                RequestMutationPreflightError::RequestLookup {
                    source: api_operation_failure(ApiFailureKind::Timeout),
                },
            )),
            ErrorCategory::Timeout,
        ),
        (
            AppError::from(AcceptError::RequesterLookup {
                source: api_operation_failure(ApiFailureKind::Rejected),
            }),
            ErrorCategory::Api,
        ),
        (
            AppError::from(AcceptError::RequesterIdentityMismatch),
            ErrorCategory::ApiContract,
        ),
        (
            AppError::from(AcceptError::Preflight(
                RequestMutationPreflightError::RequestValidation(
                    RequestMutationValidationError::NotPending,
                ),
            )),
            ErrorCategory::Usage,
        ),
        (
            AppError::from(AcceptError::Balance {
                source: api_operation_failure(ApiFailureKind::Contract),
            }),
            ErrorCategory::ApiContract,
        ),
        (
            AppError::from(AcceptError::InsufficientWalletBalance),
            ErrorCategory::Usage,
        ),
        (
            AppError::from(AcceptError::ConfirmationRequired),
            ErrorCategory::Usage,
        ),
        (
            AppError::from(AcceptError::ConfirmationDeclined),
            ErrorCategory::Cancelled,
        ),
        (
            AppError::from(AcceptError::Confirmation {
                source: PromptError::Cancelled,
            }),
            ErrorCategory::Cancelled,
        ),
        (
            AppError::from(AcceptError::Accept {
                source: api_operation_failure(ApiFailureKind::AmbiguousWrite),
            }),
            ErrorCategory::AmbiguousWrite,
        ),
        (
            AppError::from(DeclineError::Preflight(
                RequestMutationPreflightError::Credential(CredentialAccessError::Missing),
            )),
            ErrorCategory::Credential,
        ),
        (
            AppError::from(DeclineError::Preflight(
                RequestMutationPreflightError::Credential(CredentialAccessError::Read {
                    source: operation_failure(),
                }),
            )),
            ErrorCategory::Credential,
        ),
        (
            AppError::from(DeclineError::Preflight(
                RequestMutationPreflightError::CurrentAccount {
                    source: api_operation_failure(ApiFailureKind::Network),
                },
            )),
            ErrorCategory::Network,
        ),
        (
            AppError::from(DeclineError::Preflight(
                RequestMutationPreflightError::Account(FinancialValidationError::AccountMismatch),
            )),
            ErrorCategory::Credential,
        ),
        (
            AppError::from(DeclineError::Preflight(
                RequestMutationPreflightError::RequestLookup {
                    source: api_operation_failure(ApiFailureKind::Contract),
                },
            )),
            ErrorCategory::ApiContract,
        ),
        (
            AppError::from(DeclineError::Preflight(
                RequestMutationPreflightError::RequestValidation(
                    RequestMutationValidationError::MismatchedId,
                ),
            )),
            ErrorCategory::ApiContract,
        ),
        (
            AppError::from(DeclineError::Decline {
                source: api_operation_failure(ApiFailureKind::AmbiguousWrite),
            }),
            ErrorCategory::AmbiguousWrite,
        ),
    ];

    for (error, expected) in cases {
        assert_eq!(error.category(), expected, "{error:?}");
    }
}

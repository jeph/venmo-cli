use std::io;

use super::super::{api_operation_failure, operation_failure};
use crate::adapters::cli::error::{AppError, ErrorCategory};
use crate::features::auth::PromptError;
use crate::features::payments::FinancialValidationError;
use crate::features::transfers::options::TransferOptionsError;
use crate::features::transfers::out::TransferOutError;
use crate::features::transfers::selection::TransferSelectionError;
use crate::shared::{ApiFailureKind, CredentialAccessError};

#[test]
fn transfer_read_and_guarded_write_variants_have_deliberate_categories() {
    let cases = [
        (
            AppError::from(TransferOptionsError::Credential(
                CredentialAccessError::Missing,
            )),
            ErrorCategory::Credential,
        ),
        (
            AppError::from(TransferOptionsError::Api {
                source: api_operation_failure(ApiFailureKind::Contract),
            }),
            ErrorCategory::ApiContract,
        ),
        (
            AppError::from(TransferOutError::Credential(CredentialAccessError::Read {
                source: operation_failure(),
            })),
            ErrorCategory::Credential,
        ),
        (
            AppError::from(TransferOutError::CurrentAccount {
                source: api_operation_failure(ApiFailureKind::Network),
            }),
            ErrorCategory::Network,
        ),
        (
            AppError::from(TransferOutError::Validation(
                FinancialValidationError::AccountMismatch,
            )),
            ErrorCategory::Credential,
        ),
        (
            AppError::from(TransferOutError::Balance {
                source: api_operation_failure(ApiFailureKind::Timeout),
            }),
            ErrorCategory::Timeout,
        ),
        (
            AppError::from(TransferOutError::Options {
                source: api_operation_failure(ApiFailureKind::Rejected),
            }),
            ErrorCategory::Api,
        ),
        (
            AppError::from(TransferOutError::Selection(
                TransferSelectionError::DuplicateDestinationIds,
            )),
            ErrorCategory::ApiContract,
        ),
        (
            AppError::from(TransferOutError::Selection(
                TransferSelectionError::AmbiguousAutomaticSelection,
            )),
            ErrorCategory::Usage,
        ),
        (
            AppError::from(TransferOutError::InsufficientBalance),
            ErrorCategory::Usage,
        ),
        (
            AppError::from(TransferOutError::UnsupportedSpeed),
            ErrorCategory::Usage,
        ),
        (
            AppError::from(TransferOutError::ConfirmationRequired),
            ErrorCategory::Usage,
        ),
        (
            AppError::from(TransferOutError::ConfirmationDeclined),
            ErrorCategory::Cancelled,
        ),
        (
            AppError::from(TransferOutError::Confirmation {
                source: PromptError::Interaction {
                    source: io::Error::other("synthetic prompt failure"),
                },
            }),
            ErrorCategory::Internal,
        ),
        (
            AppError::from(TransferOutError::Create {
                source: api_operation_failure(ApiFailureKind::AmbiguousWrite),
            }),
            ErrorCategory::AmbiguousWrite,
        ),
    ];

    for (error, expected) in cases {
        assert_eq!(error.category(), expected, "{error:?}");
    }
}

use std::io;

use super::super::{api_operation_failure, operation_failure};
use crate::adapters::cli::error::{AppError, ErrorCategory};
use crate::features::auth::PromptError;
use crate::features::payments::funding::FundingSelectionError;
use crate::features::payments::pay::PayError;
use crate::features::payments::{FinancialValidationError, PeerPreflightError};
use crate::features::people::recipients::RecipientResolutionError;
use crate::features::requests::create::RequestCreateError;
use crate::shared::{ApiFailureKind, CredentialAccessError};

#[test]
fn pay_and_request_creation_variants_have_deliberate_categories() {
    let cases = [
        (
            AppError::from(PayError::Preflight(PeerPreflightError::Credential(
                CredentialAccessError::Missing,
            ))),
            ErrorCategory::Credential,
        ),
        (
            AppError::from(PayError::Preflight(PeerPreflightError::Credential(
                CredentialAccessError::Read {
                    source: operation_failure(),
                },
            ))),
            ErrorCategory::Credential,
        ),
        (
            AppError::from(PayError::Preflight(PeerPreflightError::CurrentAccount {
                source: api_operation_failure(ApiFailureKind::Network),
            })),
            ErrorCategory::Network,
        ),
        (
            AppError::from(PayError::Preflight(PeerPreflightError::Validation(
                FinancialValidationError::AccountMismatch,
            ))),
            ErrorCategory::Credential,
        ),
        (
            AppError::from(PayError::Preflight(PeerPreflightError::Recipient(
                RecipientResolutionError::LookupContract {
                    problem: "synthetic contract failure",
                },
            ))),
            ErrorCategory::ApiContract,
        ),
        (
            AppError::from(PayError::Balance {
                source: api_operation_failure(ApiFailureKind::Timeout),
            }),
            ErrorCategory::Timeout,
        ),
        (
            AppError::from(PayError::FundingMethods {
                source: api_operation_failure(ApiFailureKind::Rejected),
            }),
            ErrorCategory::Api,
        ),
        (
            AppError::from(PayError::FundingSelection(
                FundingSelectionError::DuplicateMethodIds,
            )),
            ErrorCategory::ApiContract,
        ),
        (
            AppError::from(PayError::Eligibility {
                source: api_operation_failure(ApiFailureKind::Contract),
            }),
            ErrorCategory::ApiContract,
        ),
        (
            AppError::from(PayError::ConfirmationRequired),
            ErrorCategory::Usage,
        ),
        (
            AppError::from(PayError::ConfirmationDeclined),
            ErrorCategory::Cancelled,
        ),
        (
            AppError::from(PayError::Confirmation {
                source: PromptError::Interaction {
                    source: io::Error::other("synthetic confirmation failure"),
                },
            }),
            ErrorCategory::Internal,
        ),
        (
            AppError::from(PayError::Create {
                source: api_operation_failure(ApiFailureKind::AmbiguousWrite),
            }),
            ErrorCategory::AmbiguousWrite,
        ),
        (
            AppError::from(RequestCreateError::Preflight(
                PeerPreflightError::Credential(CredentialAccessError::Missing),
            )),
            ErrorCategory::Credential,
        ),
        (
            AppError::from(RequestCreateError::Preflight(
                PeerPreflightError::Credential(CredentialAccessError::Read {
                    source: operation_failure(),
                }),
            )),
            ErrorCategory::Credential,
        ),
        (
            AppError::from(RequestCreateError::Preflight(
                PeerPreflightError::CurrentAccount {
                    source: api_operation_failure(ApiFailureKind::Internal),
                },
            )),
            ErrorCategory::Internal,
        ),
        (
            AppError::from(RequestCreateError::Preflight(
                PeerPreflightError::Validation(FinancialValidationError::SelfRecipient),
            )),
            ErrorCategory::Usage,
        ),
        (
            AppError::from(RequestCreateError::Preflight(
                PeerPreflightError::Recipient(RecipientResolutionError::Internal {
                    problem: "synthetic internal failure",
                }),
            )),
            ErrorCategory::Internal,
        ),
        (
            AppError::from(RequestCreateError::ConfirmationRequired),
            ErrorCategory::Usage,
        ),
        (
            AppError::from(RequestCreateError::ConfirmationDeclined),
            ErrorCategory::Cancelled,
        ),
        (
            AppError::from(RequestCreateError::Confirmation {
                source: PromptError::Interaction {
                    source: io::Error::other("synthetic request confirmation failure"),
                },
            }),
            ErrorCategory::Internal,
        ),
        (
            AppError::from(RequestCreateError::Create {
                source: api_operation_failure(ApiFailureKind::AmbiguousWrite),
            }),
            ErrorCategory::AmbiguousWrite,
        ),
    ];

    for (error, expected) in cases {
        assert_eq!(error.category(), expected, "{error:?}");
    }
}

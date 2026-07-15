use super::{api_operation_failure, operation_failure};
use crate::adapters::cli::error::{AppError, ErrorCategory};
use crate::features::payments::FinancialValidationError;
use crate::features::payments::funding::FundingSelectionError;
use crate::features::people::info::UserInfoError;
use crate::features::people::recipients::{
    RecipientResolutionError, RecipientResolutionFailureKind,
};
use crate::features::people::users::UserSearchError;
use crate::features::requests::info::RequestInfoError;
use crate::features::requests::validation::RequestMutationValidationError;
use crate::shared::{ApiFailureKind, ApplicationFailureKind, CredentialAccessError};

#[test]
fn user_and_request_info_error_variants_have_exhaustive_cli_classifications() {
    let user_cases = [
        (
            UserInfoError::Credential(CredentialAccessError::Missing),
            ErrorCategory::Credential,
        ),
        (
            UserInfoError::Credential(CredentialAccessError::Read {
                source: operation_failure(),
            }),
            ErrorCategory::Credential,
        ),
        (
            UserInfoError::Api {
                source: api_operation_failure(ApiFailureKind::Timeout),
            },
            ErrorCategory::Timeout,
        ),
        (
            UserInfoError::ResponseContract {
                problem: "synthetic contract failure",
            },
            ErrorCategory::ApiContract,
        ),
    ];
    for (error, expected_category) in user_cases {
        let error = AppError::from(error);
        assert_eq!(error.category(), expected_category);
        assert_eq!(error.exit_code(), expected_category.exit_code());
    }

    let request_cases = [
        (
            RequestInfoError::Credential(CredentialAccessError::Missing),
            ErrorCategory::Credential,
        ),
        (
            RequestInfoError::Credential(CredentialAccessError::Read {
                source: operation_failure(),
            }),
            ErrorCategory::Credential,
        ),
        (
            RequestInfoError::Api {
                source: api_operation_failure(ApiFailureKind::Network),
            },
            ErrorCategory::Network,
        ),
        (
            RequestInfoError::ResponseContract {
                problem: "synthetic contract failure",
            },
            ErrorCategory::ApiContract,
        ),
        (RequestInfoError::NotRequest, ErrorCategory::Usage),
        (RequestInfoError::NotOpen, ErrorCategory::Usage),
    ];
    for (error, expected_category) in request_cases {
        let error = AppError::from(error);
        assert_eq!(error.category(), expected_category);
        assert_eq!(error.exit_code(), expected_category.exit_code());
    }
}

#[test]
fn nested_financial_failure_variants_have_deliberate_classes() {
    let financial_validation = [
        (
            FinancialValidationError::AccountMismatch,
            ApplicationFailureKind::Credential,
        ),
        (
            FinancialValidationError::SelfRecipient,
            ApplicationFailureKind::Usage,
        ),
        (
            FinancialValidationError::MissingProfileType,
            ApplicationFailureKind::ApiContract,
        ),
        (
            FinancialValidationError::UnsupportedProfileType,
            ApplicationFailureKind::Usage,
        ),
        (
            FinancialValidationError::MissingPayability,
            ApplicationFailureKind::ApiContract,
        ),
        (
            FinancialValidationError::RecipientNotPayable,
            ApplicationFailureKind::Usage,
        ),
    ];
    for (error, expected) in financial_validation {
        assert_eq!(error.failure_kind(), expected, "{error:?}");
    }

    let funding = [
        (
            FundingSelectionError::NoEligibleMethods,
            ApplicationFailureKind::Usage,
        ),
        (
            FundingSelectionError::DuplicateMethodIds,
            ApplicationFailureKind::ApiContract,
        ),
        (
            FundingSelectionError::MultipleDefaults,
            ApplicationFailureKind::ApiContract,
        ),
        (
            FundingSelectionError::AmbiguousAutomaticSelection,
            ApplicationFailureKind::Usage,
        ),
    ];
    for (error, expected) in funding {
        assert_eq!(error.failure_kind(), expected, "{error:?}");
    }

    let request_validation = [
        (
            RequestMutationValidationError::NotIncoming,
            ApplicationFailureKind::Usage,
        ),
        (
            RequestMutationValidationError::NotPending,
            ApplicationFailureKind::Usage,
        ),
        (
            RequestMutationValidationError::UnverifiedAudience,
            ApplicationFailureKind::Usage,
        ),
        (
            RequestMutationValidationError::InvalidNote,
            ApplicationFailureKind::ApiContract,
        ),
        (
            RequestMutationValidationError::UnsupportedAudience,
            ApplicationFailureKind::ApiContract,
        ),
        (
            RequestMutationValidationError::IncompleteRequester,
            ApplicationFailureKind::ApiContract,
        ),
        (
            RequestMutationValidationError::MissingCreationTime,
            ApplicationFailureKind::ApiContract,
        ),
        (
            RequestMutationValidationError::NotChargeRequest,
            ApplicationFailureKind::Usage,
        ),
        (
            RequestMutationValidationError::MismatchedId,
            ApplicationFailureKind::ApiContract,
        ),
    ];
    for (error, expected) in request_validation {
        assert_eq!(error.failure_kind(), expected, "{error:?}");
    }
}

#[test]
fn recipient_resolution_variants_have_deliberate_classes() {
    let cases = [
        (
            RecipientResolutionError::Api {
                source: api_operation_failure(ApiFailureKind::Network),
            },
            ApplicationFailureKind::Api(ApiFailureKind::Network),
        ),
        (
            RecipientResolutionError::LookupContract {
                problem: "synthetic contract failure",
            },
            ApplicationFailureKind::ApiContract,
        ),
        (
            RecipientResolutionError::UsernameSearch {
                kind: RecipientResolutionFailureKind::ApiContract,
                source: UserSearchError::ResponseContract {
                    problem: "synthetic pagination failure",
                },
            },
            ApplicationFailureKind::ApiContract,
        ),
        (
            RecipientResolutionError::UsernameNotFound,
            ApplicationFailureKind::Usage,
        ),
        (
            RecipientResolutionError::AmbiguousUsername,
            ApplicationFailureKind::Usage,
        ),
        (
            RecipientResolutionError::IncompleteUsernameSearch,
            ApplicationFailureKind::Usage,
        ),
        (
            RecipientResolutionError::Internal {
                problem: "synthetic internal failure",
            },
            ApplicationFailureKind::Internal,
        ),
    ];

    for (error, expected) in cases {
        assert_eq!(error.application_failure_kind(), expected, "{error:?}");
    }
}

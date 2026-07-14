use super::api_operation_failure;
use crate::features::auth::PromptError;
use crate::features::payments::FinancialValidationError;
use crate::features::payments::funding::FundingSelectionError;
use crate::features::people::recipients::{
    RecipientResolutionError, RecipientResolutionFailureKind,
};
use crate::features::people::users::UserSearchError;
use crate::features::requests::validation::RequestMutationValidationError;
use crate::shared::{ApiFailureKind, ApplicationFailureKind};

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
            FundingSelectionError::NoProvenZeroFeeMethods,
            ApplicationFailureKind::Usage,
        ),
        (
            FundingSelectionError::ExplicitMethodUnavailable,
            ApplicationFailureKind::Usage,
        ),
        (
            FundingSelectionError::ExplicitMethodFeeNotZero,
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
            FundingSelectionError::ExplicitMethodRequired,
            ApplicationFailureKind::Usage,
        ),
        (
            FundingSelectionError::Prompt {
                source: PromptError::Cancelled,
            },
            ApplicationFailureKind::Cancelled,
        ),
        (
            FundingSelectionError::Prompt {
                source: PromptError::NoChoices,
            },
            ApplicationFailureKind::Internal,
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

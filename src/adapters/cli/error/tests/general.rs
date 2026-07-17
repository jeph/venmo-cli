use std::io;

use crate::adapters::cli::error::{AppError, ErrorCategory};
use crate::adapters::venmo::TransportBuildError;
use crate::features::activity::ActivityError;
use crate::features::auth::{AuthStatusError, LoginError, PromptError};
use crate::features::payments::PeerPreflightError;
use crate::features::payments::pay::PayError;
use crate::features::people::friends::FriendsError;
use crate::features::people::info::UserInfoError;
use crate::features::people::users::UserSearchError;
use crate::features::requests::RequestMutationPreflightError;
use crate::features::requests::accept::AcceptError;
use crate::features::requests::create::RequestCreateError;
use crate::features::requests::decline::DeclineError;
use crate::features::requests::info::RequestInfoError;
use crate::features::requests::list::RequestsError;
use crate::features::transfers::options::TransferOptionsError;
use crate::features::transfers::out::TransferOutError;
use crate::features::wallet::balance::BalanceError;
use crate::features::wallet::payment_methods::PaymentMethodsError;
use crate::shared::{ApiFailureKind, ApplicationFailureKind, CredentialAccessError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AppErrorVariant {
    CompletionOutput,
    LoggingInitialization,
    RuntimeInitialization,
    SignalInitialization,
    FinancialWriteInterruptedUnknown,
    ApiInitialization,
    AuthLogin,
    AuthStatus,
    AuthLogoutIncomplete,
    AuthLoginIncomplete,
    AuthStateOutput,
    PaymentMethods,
    Pay,
    RequestCreate,
    Accept,
    Decline,
    UserSearch,
    UserInfo,
    Friends,
    Balance,
    Activity,
    Requests,
    RequestInfo,
    TransferOptions,
    TransferOut,
    DoctorIncomplete,
    CommandOutput,
    FinancialResultOutput,
}

#[derive(Debug, Eq, PartialEq)]
struct Classification {
    variant: AppErrorVariant,
    category: ErrorCategory,
    exit_code: u8,
}

#[test]
fn every_app_error_variant_has_a_complete_deliberate_classification() {
    let errors = [
        AppError::CompletionOutput {
            source: io::Error::other("synthetic completion output failure"),
        },
        AppError::LoggingInitialization {
            source: Box::new(io::Error::other("synthetic logging failure")),
        },
        AppError::RuntimeInitialization {
            source: io::Error::other("synthetic runtime failure"),
        },
        AppError::SignalInitialization {
            source: io::Error::other("synthetic signal failure"),
        },
        AppError::FinancialWriteInterruptedUnknown,
        AppError::ApiInitialization {
            source: TransportBuildError::ClientInitialization,
        },
        AppError::from(LoginError::Prompt(PromptError::NotInteractive)),
        AppError::from(AuthStatusError::Credential(CredentialAccessError::Missing)),
        AppError::AuthLogoutIncomplete {
            kind: ApplicationFailureKind::Credential,
        },
        AppError::AuthLoginIncomplete {
            kind: ApiFailureKind::Network,
        },
        AppError::AuthStateOutput {
            source: io::Error::other("synthetic auth output failure"),
        },
        AppError::from(PaymentMethodsError::Credential(
            CredentialAccessError::Missing,
        )),
        AppError::from(PayError::ConfirmationRequired),
        AppError::from(RequestCreateError::Preflight(
            PeerPreflightError::Credential(CredentialAccessError::Missing),
        )),
        AppError::from(AcceptError::ConfirmationRequired),
        AppError::from(DeclineError::Preflight(
            RequestMutationPreflightError::Credential(CredentialAccessError::Missing),
        )),
        AppError::from(UserSearchError::Internal {
            problem: "synthetic internal failure",
        }),
        AppError::from(UserInfoError::ResponseContract {
            problem: "synthetic response failure",
        }),
        AppError::from(FriendsError::ResponseContract {
            problem: "synthetic response failure",
        }),
        AppError::from(BalanceError::Credential(CredentialAccessError::Missing)),
        AppError::from(ActivityError::ResponseContract {
            problem: "synthetic response failure",
        }),
        AppError::from(RequestsError::ResponseContract {
            problem: "synthetic response failure",
        }),
        AppError::from(RequestInfoError::NotRequest),
        AppError::from(TransferOptionsError::Credential(
            CredentialAccessError::Missing,
        )),
        AppError::from(TransferOutError::ConfirmationRequired),
        AppError::DoctorIncomplete,
        AppError::CommandOutput {
            source: io::Error::other("synthetic command output failure"),
        },
        AppError::FinancialResultOutput {
            source: io::Error::other("synthetic financial output failure"),
        },
    ];
    let expected = [
        classification(AppErrorVariant::CompletionOutput, ErrorCategory::Internal),
        classification(
            AppErrorVariant::LoggingInitialization,
            ErrorCategory::Internal,
        ),
        classification(
            AppErrorVariant::RuntimeInitialization,
            ErrorCategory::Internal,
        ),
        classification(
            AppErrorVariant::SignalInitialization,
            ErrorCategory::Internal,
        ),
        classification(
            AppErrorVariant::FinancialWriteInterruptedUnknown,
            ErrorCategory::AmbiguousWrite,
        ),
        classification(AppErrorVariant::ApiInitialization, ErrorCategory::Internal),
        classification(AppErrorVariant::AuthLogin, ErrorCategory::Usage),
        classification(AppErrorVariant::AuthStatus, ErrorCategory::Credential),
        classification(
            AppErrorVariant::AuthLogoutIncomplete,
            ErrorCategory::Credential,
        ),
        classification(AppErrorVariant::AuthLoginIncomplete, ErrorCategory::Network),
        classification(AppErrorVariant::AuthStateOutput, ErrorCategory::Internal),
        classification(AppErrorVariant::PaymentMethods, ErrorCategory::Credential),
        classification(AppErrorVariant::Pay, ErrorCategory::Usage),
        classification(AppErrorVariant::RequestCreate, ErrorCategory::Credential),
        classification(AppErrorVariant::Accept, ErrorCategory::Usage),
        classification(AppErrorVariant::Decline, ErrorCategory::Credential),
        classification(AppErrorVariant::UserSearch, ErrorCategory::Internal),
        classification(AppErrorVariant::UserInfo, ErrorCategory::ApiContract),
        classification(AppErrorVariant::Friends, ErrorCategory::ApiContract),
        classification(AppErrorVariant::Balance, ErrorCategory::Credential),
        classification(AppErrorVariant::Activity, ErrorCategory::ApiContract),
        classification(AppErrorVariant::Requests, ErrorCategory::ApiContract),
        classification(AppErrorVariant::RequestInfo, ErrorCategory::Usage),
        classification(AppErrorVariant::TransferOptions, ErrorCategory::Credential),
        classification(AppErrorVariant::TransferOut, ErrorCategory::Usage),
        classification(AppErrorVariant::DoctorIncomplete, ErrorCategory::Api),
        classification(AppErrorVariant::CommandOutput, ErrorCategory::Internal),
        classification(
            AppErrorVariant::FinancialResultOutput,
            ErrorCategory::AmbiguousWrite,
        ),
    ];

    let observed = errors.map(|error| Classification {
        variant: variant(&error),
        category: error.category(),
        exit_code: error.exit_code(),
    });

    assert_eq!(observed, expected);
}

const fn classification(variant: AppErrorVariant, category: ErrorCategory) -> Classification {
    Classification {
        variant,
        category,
        exit_code: category.exit_code(),
    }
}

const fn variant(error: &AppError) -> AppErrorVariant {
    match error {
        AppError::CompletionOutput { .. } => AppErrorVariant::CompletionOutput,
        AppError::LoggingInitialization { .. } => AppErrorVariant::LoggingInitialization,
        AppError::RuntimeInitialization { .. } => AppErrorVariant::RuntimeInitialization,
        AppError::SignalInitialization { .. } => AppErrorVariant::SignalInitialization,
        AppError::FinancialWriteInterruptedUnknown => {
            AppErrorVariant::FinancialWriteInterruptedUnknown
        }
        AppError::ApiInitialization { .. } => AppErrorVariant::ApiInitialization,
        AppError::AuthLogin { .. } => AppErrorVariant::AuthLogin,
        AppError::AuthStatus { .. } => AppErrorVariant::AuthStatus,
        AppError::AuthLogoutIncomplete { .. } => AppErrorVariant::AuthLogoutIncomplete,
        AppError::AuthLoginIncomplete { .. } => AppErrorVariant::AuthLoginIncomplete,
        AppError::AuthStateOutput { .. } => AppErrorVariant::AuthStateOutput,
        AppError::PaymentMethods { .. } => AppErrorVariant::PaymentMethods,
        AppError::Pay { .. } => AppErrorVariant::Pay,
        AppError::RequestCreate { .. } => AppErrorVariant::RequestCreate,
        AppError::Accept { .. } => AppErrorVariant::Accept,
        AppError::Decline { .. } => AppErrorVariant::Decline,
        AppError::UserSearch { .. } => AppErrorVariant::UserSearch,
        AppError::UserInfo { .. } => AppErrorVariant::UserInfo,
        AppError::Friends { .. } => AppErrorVariant::Friends,
        AppError::Balance { .. } => AppErrorVariant::Balance,
        AppError::Activity { .. } => AppErrorVariant::Activity,
        AppError::Requests { .. } => AppErrorVariant::Requests,
        AppError::RequestInfo { .. } => AppErrorVariant::RequestInfo,
        AppError::TransferOptions { .. } => AppErrorVariant::TransferOptions,
        AppError::TransferOut { .. } => AppErrorVariant::TransferOut,
        AppError::DoctorIncomplete => AppErrorVariant::DoctorIncomplete,
        AppError::CommandOutput { .. } => AppErrorVariant::CommandOutput,
        AppError::FinancialResultOutput { .. } => AppErrorVariant::FinancialResultOutput,
    }
}

#[test]
fn ordinary_io_errors_convert_to_command_output_errors() {
    let output = AppError::from(io::Error::other("sensitive synthetic output detail"));

    assert!(matches!(&output, AppError::CommandOutput { .. }));
    assert_eq!(output.category(), ErrorCategory::Internal);
    assert_eq!(output.to_string(), "failed to write command output");
}

#[test]
fn api_continuation_failures_are_contract_errors() {
    let continuation = AppError::from(FriendsError::ResponseContract {
        problem: "synthetic continuation failure",
    });

    assert_eq!(continuation.category(), ErrorCategory::ApiContract);
    assert_eq!(continuation.exit_code(), 1);
}

#[test]
fn payment_confirmation_failures_have_stable_categories() {
    let required = AppError::from(PayError::ConfirmationRequired);
    let declined = AppError::from(PayError::ConfirmationDeclined);

    assert_eq!(required.category(), ErrorCategory::Usage);
    assert_eq!(required.exit_code(), 2);
    assert_eq!(declined.category(), ErrorCategory::Cancelled);
    assert_eq!(declined.exit_code(), 1);

    let interrupted = AppError::FinancialWriteInterruptedUnknown;
    assert_eq!(interrupted.category(), ErrorCategory::AmbiguousWrite);
    assert_eq!(interrupted.exit_code(), 3);

    let output = AppError::FinancialResultOutput {
        source: io::Error::other("synthetic output failure"),
    };
    assert_eq!(output.category(), ErrorCategory::AmbiguousWrite);
    assert_eq!(
        output.to_string(),
        "the financial operation succeeded, but its result could not be written; do not retry it and verify the result through activity or requests and the official Venmo app"
    );
}

#[test]
fn request_acceptance_confirmation_has_stable_categories() {
    let required = AppError::from(AcceptError::ConfirmationRequired);
    let declined = AppError::from(AcceptError::ConfirmationDeclined);

    assert_eq!(required.category(), ErrorCategory::Usage);
    assert_eq!(required.exit_code(), 2);
    assert_eq!(declined.category(), ErrorCategory::Cancelled);
    assert_eq!(declined.exit_code(), 1);
}

#[test]
fn authentication_state_output_failure_has_a_stable_category_and_recovery_message() {
    let error = AppError::AuthStateOutput {
        source: io::Error::other("sensitive synthetic output detail"),
    };

    assert_eq!(error.category(), ErrorCategory::Internal);
    assert_eq!(error.exit_code(), 1);

    let rendered = error.to_string();
    assert_eq!(
        rendered,
        "authentication state may already have changed, but the command result could not be written completely; verify local state with `venmo auth status` and review official Venmo session controls before retrying"
    );
    assert!(!rendered.contains("sensitive synthetic output detail"));
}

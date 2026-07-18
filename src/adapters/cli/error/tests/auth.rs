use std::io;

use super::{SENSITIVE_SOURCE_MARKER, api_operation_failure, operation_failure};
use crate::adapters::cli::error::{AppError, ErrorCategory};
use crate::adapters::cli::write_error;
use crate::features::auth::{AuthStatusError, LoginError, PromptError};
use crate::shared::{ApiFailureKind, ApplicationFailureKind, CredentialAccessError};

#[test]
fn authentication_preserves_every_api_failure_kind_and_redacts_its_source() {
    for (kind, expected) in [
        (ApiFailureKind::Network, ErrorCategory::Network),
        (ApiFailureKind::Timeout, ErrorCategory::Timeout),
        (
            ApiFailureKind::Authentication,
            ErrorCategory::Authentication,
        ),
        (ApiFailureKind::Rejected, ErrorCategory::Api),
        (ApiFailureKind::Contract, ErrorCategory::ApiContract),
        (
            ApiFailureKind::AmbiguousWrite,
            ErrorCategory::AmbiguousWrite,
        ),
        (ApiFailureKind::Internal, ErrorCategory::Internal),
    ] {
        let errors = [
            AppError::from(LoginError::IssuedTokenValidation {
                source: api_operation_failure(kind),
            }),
            AppError::from(LoginError::PasswordAuthentication {
                source: api_operation_failure(kind),
            }),
            AppError::from(LoginError::OtpRequest {
                source: api_operation_failure(kind),
            }),
            AppError::from(LoginError::OtpCompletion {
                source: api_operation_failure(kind),
            }),
            AppError::from(AuthStatusError::TokenValidation {
                source: api_operation_failure(kind),
            }),
            AppError::AuthLoginIncomplete { kind },
            AppError::AuthLogoutIncomplete {
                kind: ApplicationFailureKind::Api(kind),
            },
        ];

        for error in errors {
            assert_eq!(error.category(), expected, "{kind:?}: {error:?}");
            assert!(!format!("{error:?}").contains(SENSITIVE_SOURCE_MARKER));
        }
    }
}

#[test]
fn authentication_non_api_variants_have_deliberate_categories() {
    let cases = [
        (
            AppError::from(LoginError::Prompt(PromptError::Cancelled)),
            ErrorCategory::Cancelled,
        ),
        (
            AppError::from(LoginError::Prompt(PromptError::NotInteractive)),
            ErrorCategory::Usage,
        ),
        (
            AppError::from(LoginError::Prompt(PromptError::Interaction {
                source: io::Error::other("synthetic prompt failure"),
            })),
            ErrorCategory::Internal,
        ),
        (
            AppError::from(LoginError::CredentialLoad {
                source: operation_failure(),
            }),
            ErrorCategory::Credential,
        ),
        (
            AppError::from(LoginError::IssuedCredentialStorageStateUnknown { source: None }),
            ErrorCategory::Credential,
        ),
        (
            AppError::from(AuthStatusError::Credential(CredentialAccessError::Missing)),
            ErrorCategory::Credential,
        ),
        (
            AppError::from(AuthStatusError::Credential(CredentialAccessError::Read {
                source: operation_failure(),
            })),
            ErrorCategory::Credential,
        ),
        (
            AppError::from(AuthStatusError::AccountMismatch),
            ErrorCategory::Credential,
        ),
        (
            AppError::AuthLogoutIncomplete {
                kind: ApplicationFailureKind::Internal,
            },
            ErrorCategory::Internal,
        ),
    ];

    for (error, expected) in cases {
        assert_eq!(error.category(), expected, "{error:?}");
    }
}

#[test]
fn authenticated_401_errors_direct_the_user_to_explicit_login_without_mutation_claims()
-> Result<(), Box<dyn std::error::Error>> {
    let error = AppError::from(AuthStatusError::TokenValidation {
        source: api_operation_failure(ApiFailureKind::Authentication),
    });
    let mut output = Vec::new();

    write_error(&mut output, &error)?;

    let output = String::from_utf8(output)?;
    assert!(output.contains("Run `venmo auth login` to authenticate again."));
    assert!(!output.contains("deleted"));
    assert!(!output.contains("retry"));

    let login_error = AppError::from(LoginError::IssuedTokenValidation {
        source: api_operation_failure(ApiFailureKind::Authentication),
    });
    assert!(login_error.requires_login());
    Ok(())
}

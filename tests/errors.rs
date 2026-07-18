use venmo_cli::cli::ErrorCategory;

#[test]
fn error_categories_have_stable_exit_codes() {
    let operational = [
        ErrorCategory::Cancelled,
        ErrorCategory::Credential,
        ErrorCategory::Authentication,
        ErrorCategory::Network,
        ErrorCategory::Timeout,
        ErrorCategory::Api,
        ErrorCategory::ApiContract,
        ErrorCategory::Internal,
    ];

    assert_eq!(ErrorCategory::Usage.exit_code(), 2);
    assert_eq!(ErrorCategory::AmbiguousWrite.exit_code(), 3);
    for category in operational {
        assert_eq!(category.exit_code(), 1);
    }
}

use super::*;

#[test]
fn api_failure_kinds_preserve_operational_categories() {
    for (transport, expected) in [
        (TransportError::InvalidRoute, ApiFailureKind::Contract),
        (TransportError::InvalidQuery, ApiFailureKind::Contract),
        (
            TransportError::InvalidContinuationLink,
            ApiFailureKind::Contract,
        ),
        (
            TransportError::InvalidAuthenticationHeader,
            ApiFailureKind::Internal,
        ),
        (
            TransportError::InvalidAuthenticationResponseHeader,
            ApiFailureKind::Contract,
        ),
        (
            TransportError::RequestConstruction,
            ApiFailureKind::Internal,
        ),
        (TransportError::Timeout, ApiFailureKind::Timeout),
        (TransportError::Network, ApiFailureKind::Network),
        (TransportError::UnexpectedRedirect, ApiFailureKind::Network),
        (
            TransportError::ResponseTooLarge { maximum_bytes: 1 },
            ApiFailureKind::Contract,
        ),
        (TransportError::ResponseRead, ApiFailureKind::Network),
        (TransportError::ResourceExhaustion, ApiFailureKind::Internal),
        (
            TransportError::FinancialWriteOutcomeUnknown {
                cause: AmbiguousWriteCause::Timeout,
            },
            ApiFailureKind::AmbiguousWrite,
        ),
        (
            TransportError::AuthenticationOutcomeUnknown {
                cause: AmbiguousWriteCause::Timeout,
            },
            ApiFailureKind::Internal,
        ),
    ] {
        assert_eq!(VenmoApiError::Transport(transport).kind(), expected);
    }
    assert_eq!(
        VenmoApiError::Contract {
            operation: CURRENT_ACCOUNT_OPERATION,
            problem: "synthetic contract failure",
        }
        .kind(),
        ApiFailureKind::Contract
    );
}

#[tokio::test(flavor = "current_thread")]
async fn errors_expose_only_safe_status_and_code() -> TestResult {
    // Setup.
    let body = serde_json::json!({
        "error": {"code":"AUTH-1","message":"secret\u{1b}[31mtext"}
    })
    .to_string();
    let response = scripted_response(401, body.into_bytes())?;
    let (token, device_id) = test_session()?;

    // Immutable initial script/state.
    let script = [Ok(response)];
    let (client, transport) = scripted_client(script)?;

    // Complete expected observation.
    let expected = ScriptedObservation::expected(
        Err(ApiErrorSnapshot::http(
            CURRENT_ACCOUNT_OPERATION,
            401,
            Some("AUTH-1"),
        )),
        vec![authenticated_read_request("/account", &["account"], &[])],
    );

    // Execute once.
    let result = client.current_account(&token, &device_id).await;
    let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);

    assert_eq!(observed, expected);
    let rendered = format!("{observed:?}");
    assert!(rendered.contains("AUTH-1"));
    assert!(!rendered.contains("secret"));
    assert!(!rendered.contains('\u{1b}'));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn malformed_or_incomplete_success_is_a_contract_error() -> TestResult {
    for (body, expected_error) in [
        (
            "not-json",
            ApiErrorSnapshot::malformed_json(CURRENT_ACCOUNT_OPERATION),
        ),
        (
            r#"{"data":{"user":{"id":"123"}}}"#,
            ApiErrorSnapshot::contract(CURRENT_ACCOUNT_OPERATION),
        ),
    ] {
        // Setup.
        let response = scripted_response(200, body.as_bytes().to_vec())?;
        let (token, device_id) = test_session()?;

        // Immutable initial script/state.
        let script = [Ok(response)];
        let (client, transport) = scripted_client(script)?;

        // Complete expected observation.
        let expected = ScriptedObservation::expected(
            Err(expected_error),
            vec![authenticated_read_request("/account", &["account"], &[])],
        );

        // Execute once.
        let result = client.current_account(&token, &device_id).await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[test]
fn unsafe_error_codes_are_not_rendered() {
    assert_eq!(sanitize_api_code("AUTH-1"), Some("AUTH-1".to_owned()));
    assert_eq!(sanitize_api_code("bad\ncode"), None);
    assert_eq!(sanitize_api_code(&"x".repeat(65)), None);
}

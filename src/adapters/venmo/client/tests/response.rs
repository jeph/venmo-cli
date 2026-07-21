use super::super::error::{ConfirmedFinancialRejection, UnsupportedFinancialContinuation};
use super::super::response::{
    is_p2p_otp_step_up_required, p2p_otp_step_up_session_id, require_financial_success_json,
};
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

#[test]
fn p2p_step_up_detection_matches_the_current_app_exactly() -> TestResult {
    let required = scripted_json_response(
        403,
        serde_json::json!({"error":{"title":"OTP_STEP_UP_REQUIRED"}}),
    )?;
    assert!(is_p2p_otp_step_up_required(&required));

    for (status, body) in [
        (
            400,
            serde_json::json!({"error":{"title":"OTP_STEP_UP_REQUIRED"}}),
        ),
        (403, serde_json::json!({"error":{"code":1396}})),
        (
            403,
            serde_json::json!({"error":{"title":"otp_step_up_required"}}),
        ),
        (403, serde_json::json!({"title":"OTP_STEP_UP_REQUIRED"})),
        (
            403,
            serde_json::json!({"error":{"metadata":{"title":"OTP_STEP_UP_REQUIRED"}}}),
        ),
    ] {
        let response = scripted_json_response(status, body)?;
        assert!(!is_p2p_otp_step_up_required(&response));
    }
    Ok(())
}

#[test]
fn acceptance_step_up_requires_a_valid_root_metadata_uuid() -> TestResult {
    let session_id = ClientRequestId::from_str("123e4567-e89b-12d3-a456-426614174000")?;
    let response = scripted_json_response(
        403,
        serde_json::json!({
            "error": {
                "title":"OTP_STEP_UP_REQUIRED",
                "metadata":{"uuid":session_id.to_string()}
            }
        }),
    )?;
    assert_eq!(
        p2p_otp_step_up_session_id(REQUEST_ACCEPTANCE_OPERATION, &response)?,
        Some(session_id)
    );

    for body in [
        serde_json::json!({"error":{"title":"OTP_STEP_UP_REQUIRED"}}),
        serde_json::json!({
            "error":{"title":"OTP_STEP_UP_REQUIRED","metadata":{"uuid":"not-a-uuid"}}
        }),
        serde_json::json!({
            "error":{"title":"OTP_STEP_UP_REQUIRED"},
            "metadata":{"uuid":"123e4567-e89b-12d3-a456-426614174000"}
        }),
    ] {
        let response = scripted_json_response(403, body)?;
        assert!(matches!(
            p2p_otp_step_up_session_id(REQUEST_ACCEPTANCE_OPERATION, &response),
            Err(VenmoApiError::Contract { .. })
        ));
    }
    Ok(())
}

#[test]
fn exact_operation_aware_financial_errors_have_actionable_messages() -> TestResult {
    let cases = [
        (
            PAYMENT_CREATION_OPERATION,
            400,
            serde_json::json!({"error":{"code":1393}}),
            VenmoApiError::ConfirmedFinancialRejection(ConfirmedFinancialRejection::PeerDeclined {
                operation: "payment",
            }),
        ),
        (
            REQUEST_CREATION_OPERATION,
            422,
            serde_json::json!({"error":{"code":"10104"}}),
            VenmoApiError::ConfirmedFinancialRejection(
                ConfirmedFinancialRejection::PeerRiskDeclined {
                    operation: "request",
                },
            ),
        ),
        (
            PAYMENT_CREATION_OPERATION,
            400,
            serde_json::json!({"error":{"code":230500}}),
            VenmoApiError::ConfirmedFinancialRejection(
                ConfirmedFinancialRejection::PendingTeenAccount {
                    operation: "payment",
                },
            ),
        ),
        (
            PAYMENT_CREATION_OPERATION,
            400,
            serde_json::json!({"error":{"code":10200}}),
            VenmoApiError::UnsupportedFinancialContinuation(
                UnsupportedFinancialContinuation::ScamWarning {
                    operation: "payment",
                    code: "10200",
                },
            ),
        ),
        (
            REQUEST_CREATION_OPERATION,
            400,
            serde_json::json!({"error":{"code":"10201"}}),
            VenmoApiError::UnsupportedFinancialContinuation(
                UnsupportedFinancialContinuation::ScamWarning {
                    operation: "request",
                    code: "10201",
                },
            ),
        ),
        (
            REQUEST_ACCEPTANCE_OPERATION,
            400,
            serde_json::json!({"error":{"code":17461}}),
            VenmoApiError::UnsupportedFinancialContinuation(
                UnsupportedFinancialContinuation::PlaidRelink {
                    operation: "request acceptance",
                },
            ),
        ),
        (
            REQUEST_ACCEPTANCE_OPERATION,
            403,
            serde_json::json!({"error":{"title":"RISK_DECLINED"}}),
            VenmoApiError::ConfirmedFinancialRejection(
                ConfirmedFinancialRejection::RequestAcceptanceRiskDeclined,
            ),
        ),
        (
            REQUEST_ACCEPTANCE_OPERATION,
            403,
            serde_json::json!({"error":{"title":"RISK_INELIGIBLE"}}),
            VenmoApiError::ConfirmedFinancialRejection(
                ConfirmedFinancialRejection::RequestAcceptanceRiskIneligible,
            ),
        ),
        (
            TRANSFER_OUT_CREATION_OPERATION,
            403,
            serde_json::json!({"error":{"title":"OTP_STEP_UP_REQUIRED"}}),
            VenmoApiError::UnsupportedFinancialContinuation(
                UnsupportedFinancialContinuation::TransferSmsVerification,
            ),
        ),
    ];

    for (operation, status, body, expected) in cases {
        let error = financial_response_error(operation, status, body)?;
        assert_eq!(error, expected);
        assert_eq!(error.kind(), ApiFailureKind::Rejected);
        let message = error.to_string();
        assert!(
            message.contains("Check")
                || message.contains("Review")
                || message.contains("Reconnect")
                || message.contains("use the official Venmo app"),
            "message lacked a recovery action: {message}"
        );
        assert!(!message.contains("outcome is unknown"));
    }
    Ok(())
}

#[test]
fn user_facing_financial_recovery_messages_are_exact() {
    let cases = [
        (
            VenmoApiError::ConfirmedFinancialRejection(ConfirmedFinancialRejection::PeerDeclined {
                operation: "payment",
            }),
            "Venmo declined the payment (error code 1393). Check activity and requests before trying again, and use the official Venmo app if the decline persists",
        ),
        (
            VenmoApiError::ConfirmedFinancialRejection(
                ConfirmedFinancialRejection::RequestAcceptanceRiskIneligible,
            ),
            "Venmo reported that this request acceptance is not eligible under its risk checks. The request was not accepted; check its current status and use the official Venmo app before trying again",
        ),
        (
            VenmoApiError::UnsupportedFinancialContinuation(
                UnsupportedFinancialContinuation::ScamWarning {
                    operation: "request",
                    code: "10201",
                },
            ),
            "Venmo requires an in-app scam-warning review before the request can continue (error code 10201). The CLI will not bypass that review or retry the transaction; review the recipient and use the official Venmo app only if you still intend to proceed",
        ),
        (
            VenmoApiError::UnsupportedFinancialContinuation(
                UnsupportedFinancialContinuation::PlaidRelink {
                    operation: "request acceptance",
                },
            ),
            "Venmo requires the linked bank to be reconnected with Plaid before the request acceptance can continue (error code 17461). Reconnect it in the official Venmo app; the CLI did not retry the transaction",
        ),
        (
            VenmoApiError::UnsupportedFinancialContinuation(
                UnsupportedFinancialContinuation::TransferSmsVerification,
            ),
            "Venmo requires SMS verification before this outbound transfer can continue. The CLI does not support transfer verification and did not retry the transfer; use the official Venmo app",
        ),
    ];

    for (error, expected) in cases {
        assert_eq!(error.to_string(), expected);
    }
}

#[test]
fn apk_context_improves_ambiguous_messages_without_changing_exit_semantics() -> TestResult {
    let cases: &[(&str, &[u32], &str)] = &[
        (
            PAYMENT_CREATION_OPERATION,
            &[10101, 10199],
            "peer-transaction code indicates a risk decline",
        ),
        (
            REQUEST_CREATION_OPERATION,
            &[60000, 60099],
            "peer-transaction code indicates a decline",
        ),
        (
            TRANSFER_OUT_CREATION_OPERATION,
            &[1319, 9903],
            "insufficient available funds",
        ),
        (
            TRANSFER_OUT_CREATION_OPERATION,
            &[1358, 1346, 1757, 1758, 1766, 13010, 80907],
            "invalid amount, transfer limit, or rate limit",
        ),
        (
            TRANSFER_OUT_CREATION_OPERATION,
            &[5204, 5207, 9902, 1734],
            "unavailable or unlinked bank method",
        ),
        (
            TRANSFER_OUT_CREATION_OPERATION,
            &[1361, 1362, 1364, 81005],
            "account, identity, or location restriction",
        ),
        (
            TRANSFER_OUT_CREATION_OPERATION,
            &[1760, 1763, 99027],
            "transfer or risk decline",
        ),
    ];

    for (operation, codes, expected_guidance) in cases {
        for code in *codes {
            let error = financial_response_error(
                operation,
                400,
                serde_json::json!({"error":{"code":code}}),
            )?;
            assert_eq!(error.kind(), ApiFailureKind::AmbiguousWrite);
            let message = error.to_string();
            assert!(message.contains("outcome is unknown"));
            assert!(message.contains("before retrying"));
            assert!(message.contains(expected_guidance), "{message}");
        }
    }
    Ok(())
}

#[test]
fn financial_codes_and_titles_never_inherit_meaning_from_another_operation() -> TestResult {
    let cases = [
        (
            REQUEST_ACCEPTANCE_OPERATION,
            400,
            serde_json::json!({"error":{"code":1393}}),
        ),
        (
            TRANSFER_OUT_CREATION_OPERATION,
            400,
            serde_json::json!({"error":{"code":10104}}),
        ),
        (
            REQUEST_DECLINE_OPERATION,
            400,
            serde_json::json!({"error":{"code":10200}}),
        ),
        (
            REQUEST_CANCELLATION_OPERATION,
            400,
            serde_json::json!({"error":{"code":17461}}),
        ),
        (
            REQUEST_CREATION_OPERATION,
            400,
            serde_json::json!({"error":{"code":17461}}),
        ),
        (
            PAYMENT_CREATION_OPERATION,
            400,
            serde_json::json!({"error":{"code":1319}}),
        ),
        (
            REQUEST_CREATION_OPERATION,
            403,
            serde_json::json!({"error":{"title":"RISK_DECLINED"}}),
        ),
        (
            REQUEST_ACCEPTANCE_OPERATION,
            403,
            serde_json::json!({"error":{"title":"OTP_STEP_UP_REQUIRED"}}),
        ),
    ];

    for (operation, status, body) in cases {
        let error = financial_response_error(operation, status, body)?;
        assert_eq!(error.kind(), ApiFailureKind::AmbiguousWrite);
        let message = error.to_string();
        assert!(message.contains("outcome is unknown"));
        assert!(!message.contains("risk decline"));
        assert!(!message.contains("scam-warning"));
        assert!(!message.contains("Plaid"));
        assert!(!message.contains("insufficient available funds"));
        assert!(!message.contains("SMS verification"));
    }
    Ok(())
}

#[test]
fn financial_guidance_requires_exact_status_root_location_and_spelling() -> TestResult {
    let cases = [
        (
            REQUEST_ACCEPTANCE_OPERATION,
            400,
            serde_json::json!({"error":{"title":"RISK_DECLINED"}}),
        ),
        (
            REQUEST_ACCEPTANCE_OPERATION,
            403,
            serde_json::json!({"title":"RISK_DECLINED"}),
        ),
        (
            REQUEST_ACCEPTANCE_OPERATION,
            403,
            serde_json::json!({"error":{"metadata":{"title":"RISK_DECLINED"}}}),
        ),
        (
            REQUEST_ACCEPTANCE_OPERATION,
            403,
            serde_json::json!({"error":{"title":"risk_declined"}}),
        ),
        (
            TRANSFER_OUT_CREATION_OPERATION,
            400,
            serde_json::json!({"error":{"title":"OTP_STEP_UP_REQUIRED"}}),
        ),
        (
            TRANSFER_OUT_CREATION_OPERATION,
            403,
            serde_json::json!({"title":"OTP_STEP_UP_REQUIRED"}),
        ),
        (
            PAYMENT_CREATION_OPERATION,
            400,
            serde_json::json!({"error_code":60000}),
        ),
        (
            TRANSFER_OUT_CREATION_OPERATION,
            400,
            serde_json::json!({"data":{"error":{"code":1319}}}),
        ),
        (
            PAYMENT_CREATION_OPERATION,
            400,
            serde_json::json!({"error":{"code":59999}}),
        ),
        (
            PAYMENT_CREATION_OPERATION,
            400,
            serde_json::json!({"error":{"code":60100}}),
        ),
        (
            TRANSFER_OUT_CREATION_OPERATION,
            400,
            serde_json::json!({"error":{"code":1749}}),
        ),
        (
            TRANSFER_OUT_CREATION_OPERATION,
            400,
            serde_json::json!({"error":{"code":88021}}),
        ),
        (
            TRANSFER_OUT_CREATION_OPERATION,
            400,
            serde_json::json!({"error":{"code":999999}}),
        ),
    ];

    for (operation, status, body) in cases {
        let error = financial_response_error(operation, status, body)?;
        assert_eq!(error.kind(), ApiFailureKind::AmbiguousWrite);
        let message = error.to_string();
        assert!(message.contains("outcome is unknown"));
        assert!(!message.contains("risk checks declined"));
        assert!(!message.contains("SMS verification"));
        assert!(!message.contains("peer-transaction code indicates"));
        assert!(!message.contains("insufficient available funds"));
    }

    let success_with_error = financial_response_error(
        PAYMENT_CREATION_OPERATION,
        200,
        serde_json::json!({"error":{"code":1393}}),
    )?;
    assert!(matches!(
        success_with_error,
        VenmoApiError::FinancialOutcomeUnknown { .. }
    ));
    Ok(())
}

fn financial_response_error(
    operation: &'static str,
    status: u16,
    body: serde_json::Value,
) -> Result<VenmoApiError, Box<dyn std::error::Error>> {
    let response = scripted_json_response(status, body)?;
    match require_financial_success_json(operation, response) {
        Err(error) => Ok(error),
        Ok(_) => Err(std::io::Error::other(
            "synthetic response unexpectedly proved financial success",
        )
        .into()),
    }
}

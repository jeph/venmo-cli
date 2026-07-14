use super::*;

#[tokio::test(flavor = "current_thread")]
async fn blank_source_eligibility_uses_integer_cents_and_returns_a_redacted_token() -> TestResult {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/protection/eligibility"))
        .and(header("authorization", "Bearer synthetic-token"))
        .and(header("device-id", "synthetic-device"))
        .and(body_string(BLANK_SOURCE_ELIGIBILITY_REQUEST_BODY))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {
                "eligibility_token": "synthetic-eligibility-token",
                "eligible": true,
                "fees": [{"calculated_fee_amount_in_cents": 0}],
                "fee_disclaimer": "Synthetic zero fee",
                "ineligible_reason": null
            }
        })))
        .mount(&server)
        .await;
    let client = test_client(&server)?;
    let (token, device_id) = test_session()?;
    let recipient = financial_user("456", "bob")?;
    let amount = Money::from_cents(1)?;
    let note = Note::from_str("Synthetic note")?;

    let eligibility = client
        .blank_source_eligibility(&token, &device_id, &recipient, amount, &note)
        .await?;

    assert_eq!(eligibility.overall_fee_cents(), 0);
    assert_eq!(eligibility.token().expose(), "synthetic-eligibility-token");
    assert!(!format!("{:?}", eligibility.token()).contains("synthetic-eligibility-token"));
    assert_request_count(&server, 1).await;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn ineligible_payment_is_a_confirmed_prewrite_rejection() -> TestResult {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/protection/eligibility"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {
                "eligibility_token": "synthetic-eligibility-token",
                "eligible": false,
                "fees": [],
                "fee_disclaimer": "Not eligible",
                "ineligible_reason": "synthetic_reason"
            }
        })))
        .mount(&server)
        .await;
    let client = test_client(&server)?;
    let (token, device_id) = test_session()?;
    let recipient = financial_user("456", "bob")?;
    let note = Note::from_str("Synthetic note")?;
    let result = client
        .blank_source_eligibility(&token, &device_id, &recipient, Money::from_cents(1)?, &note)
        .await;
    assert!(matches!(result, Err(VenmoApiError::EligibilityDenied)));
    assert_eq!(
        result.as_ref().err().map(ApiFailure::kind),
        Some(ApiFailureKind::Rejected)
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn payment_creation_sends_exact_candidate_body_and_validates_success() -> TestResult {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/payments"))
        .and(header("authorization", "Bearer synthetic-token"))
        .and(header("device-id", "synthetic-device"))
        .and(body_string(PAYMENT_CREATION_REQUEST_BODY))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(created_payment_body(
                "payment-1",
                "pay",
                "settled",
                "123",
                "456",
            )),
        )
        .mount(&server)
        .await;
    let client = test_client(&server)?;
    let (token, device_id) = test_session()?;
    let created = client
        .create_payment(&token, &device_id, &pay_plan()?)
        .await?;
    assert_eq!(created.id().as_str(), "payment-1");
    assert_eq!(created.status(), FinancialStatus::Settled);
    assert_request_count(&server, 1).await;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn request_creation_sends_negative_amount_without_payment_only_fields() -> TestResult {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/payments"))
        .and(header("authorization", "Bearer synthetic-token"))
        .and(header("device-id", "synthetic-device"))
        .and(body_string(REQUEST_CREATION_REQUEST_BODY))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(created_payment_body(
                "request-1",
                "charge",
                "pending",
                "123",
                "456",
            )),
        )
        .mount(&server)
        .await;
    let client = test_client(&server)?;
    let (token, device_id) = test_session()?;
    let created = client
        .create_request(&token, &device_id, &request_plan()?)
        .await?;
    assert_eq!(created.id().as_str(), "request-1");
    assert_eq!(created.status().as_str(), "pending");
    assert_request_count(&server, 1).await;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn request_creation_cannot_validate_a_payment_action_or_status() -> TestResult {
    for body in [
        created_payment_body("request-1", "pay", "pending", "123", "456"),
        created_payment_body("request-1", "charge", "settled", "123", "456"),
    ] {
        // Setup.
        let response = scripted_json_response(200, body)?;
        let (token, device_id) = test_session()?;

        // Immutable initial script/state.
        let script = [Ok(response)];
        let (client, transport) = scripted_client(script)?;

        // Complete expected observation.
        let expected = ScriptedObservation::expected(
            Err(ApiErrorSnapshot::financial_unknown(
                REQUEST_CREATION_OPERATION,
            )),
            vec![payment_creation_request(REQUEST_CREATION_REQUEST_BODY)],
        );

        // Execute once.
        let result = client
            .create_request(&token, &device_id, &request_plan()?)
            .await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn malformed_mismatched_and_unverified_write_responses_are_ambiguous() -> TestResult {
    let direct_payment = created_payment_body("payment-1", "pay", "settled", "123", "456")["data"]
        ["payment"]
        .clone();
    let mut missing_timestamp = created_payment_body("payment-1", "pay", "settled", "123", "456");
    missing_timestamp["data"]["payment"]["date_created"] = Value::Null;
    let mut invalid_timestamp = missing_timestamp.clone();
    invalid_timestamp["data"]["payment"]["date_created"] = Value::String("invalid".to_owned());
    let bodies = [
        (200_u16, "not-json".to_owned()),
        (200, serde_json::json!({"data": direct_payment}).to_string()),
        (200, missing_timestamp.to_string()),
        (200, invalid_timestamp.to_string()),
        (
            200,
            created_payment_body("payment-1", "pay", "settled", "123", "999").to_string(),
        ),
        (
            500,
            serde_json::json!({"error": {"code": "unknown"}}).to_string(),
        ),
        (
            500,
            serde_json::json!({"error": {"code": "1396"}}).to_string(),
        ),
        (400, serde_json::json!({"error_code": "1396"}).to_string()),
        (
            200,
            serde_json::json!({"error": {"code": "1396"}}).to_string(),
        ),
    ];
    for (status, body) in bodies {
        // Setup.
        let response = scripted_response(status, body.into_bytes())?;
        let (token, device_id) = test_session()?;

        // Immutable initial script/state.
        let script = [Ok(response)];
        let (client, transport) = scripted_client(script)?;

        // Complete expected observation.
        let expected = ScriptedObservation::expected(
            Err(ApiErrorSnapshot::financial_unknown(
                PAYMENT_CREATION_OPERATION,
            )),
            vec![payment_creation_request(PAYMENT_CREATION_REQUEST_BODY)],
        );

        // Execute once.
        let result = client
            .create_payment(&token, &device_id, &pay_plan()?)
            .await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);

        assert_eq!(observed, expected);
        assert!(!format!("{observed:?}").contains("synthetic-eligibility-token"));
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn scripted_financial_transport_error_preserves_ambiguous_write_semantics() -> TestResult {
    // Setup.
    let transport_error = TransportError::FinancialWriteOutcomeUnknown {
        cause: AmbiguousWriteCause::Timeout,
    };
    let (token, device_id) = test_session()?;

    // Immutable initial script/state.
    let script = [Err(transport_error.clone())];
    let (client, transport) = scripted_client(script)?;

    // Complete expected observation.
    let expected = ScriptedObservation::expected(
        Err(ApiErrorSnapshot::transport(
            transport_error,
            ApiFailureKind::AmbiguousWrite,
        )),
        vec![payment_creation_request(PAYMENT_CREATION_REQUEST_BODY)],
    );

    // Execute once.
    let result = client
        .create_payment(&token, &device_id, &pay_plan()?)
        .await;
    let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);

    assert_eq!(observed, expected);
    assert!(!format!("{observed:?}").contains("synthetic-eligibility-token"));
    Ok(())
}

#[test]
fn financial_json_numbers_preserve_every_cent_exactly() -> TestResult {
    let largest = Money::from_cents(u64::MAX)?;
    let payment = money_json_number(largest, PeerCreation::Payment)?;
    let request = money_json_number(largest, PeerCreation::Request)?;
    assert_eq!(payment.to_string(), "184467440737095516.15");
    assert_eq!(request.to_string(), "-184467440737095516.15");
    assert_eq!(serde_json::to_string(&payment)?, "184467440737095516.15");
    assert_eq!(serde_json::to_string(&request)?, "-184467440737095516.15");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn only_dossier_known_payment_errors_are_confirmed_rejections() -> TestResult {
    for code in ["1396", "13006"] {
        // Setup.
        let response = scripted_json_response(400, serde_json::json!({"error": {"code": code}}))?;
        let (token, device_id) = test_session()?;

        // Immutable initial script/state.
        let script = [Ok(response)];
        let (client, transport) = scripted_client(script)?;

        // Complete expected observation.
        let expected = ScriptedObservation::expected(
            Err(ApiErrorSnapshot::http(
                PAYMENT_CREATION_OPERATION,
                400,
                Some(code),
            )),
            vec![payment_creation_request(PAYMENT_CREATION_REQUEST_BODY)],
        );

        // Execute once.
        let result = client
            .create_payment(&token, &device_id, &pay_plan()?)
            .await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);

        assert_eq!(observed, expected);
    }

    // Setup.
    let response = scripted_json_response(400, serde_json::json!({"error": {"code": "13006"}}))?;
    let (token, device_id) = test_session()?;

    // Immutable initial script/state.
    let script = [Ok(response)];
    let (client, transport) = scripted_client(script)?;

    // Complete expected observation.
    let expected = ScriptedObservation::expected(
        Err(ApiErrorSnapshot::financial_unknown(
            REQUEST_CREATION_OPERATION,
        )),
        vec![payment_creation_request(REQUEST_CREATION_REQUEST_BODY)],
    );

    // Execute once.
    let request_result = client
        .create_request(&token, &device_id, &request_plan()?)
        .await;
    let observed =
        ScriptedObservation::observed(project_result(request_result, |_| ()), &transport);

    assert_eq!(observed, expected);
    Ok(())
}

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
    for (visibility, expected_body) in [
        (Visibility::Private, PAYMENT_CREATION_REQUEST_BODY),
        (Visibility::Friends, PAYMENT_CREATION_FRIENDS_REQUEST_BODY),
        (Visibility::Public, PAYMENT_CREATION_PUBLIC_REQUEST_BODY),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/payments"))
            .and(header("authorization", "Bearer synthetic-token"))
            .and(header("device-id", "synthetic-device"))
            .and(body_string(expected_body))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                created_payment_body_with_visibility(
                    "payment-1",
                    "pay",
                    "settled",
                    "123",
                    "456",
                    visibility,
                ),
            ))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;
        let created = client
            .create_payment(
                &token,
                &device_id,
                &pay_plan_with_visibility(visibility)?,
                PaymentVerification::Unverified,
            )
            .await?;
        let PaymentCreationOutcome::Created(created) = created else {
            return Err(io::Error::other("payment unexpectedly required OTP").into());
        };
        assert_eq!(created.id().as_str(), "payment-1");
        assert_eq!(created.status(), FinancialStatus::Settled);
        assert_request_count(&server, 1).await;
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn payment_creation_maps_current_otp_step_up_and_resubmits_verified_metadata() -> TestResult {
    let (token, device_id) = test_session()?;
    for status in [400, 403] {
        let step_up_response = scripted_json_response(
            status,
            serde_json::json!({
                "error": {
                    "code": 1396,
                    "title": "OTP_STEP_UP_REQUIRED"
                }
            }),
        )?;
        let (client, transport) = scripted_client([Ok(step_up_response)])?;

        let result = client
            .create_payment(
                &token,
                &device_id,
                &pay_plan()?,
                PaymentVerification::Unverified,
            )
            .await;

        let expected = ScriptedObservation::expected(
            Ok(()),
            vec![payment_creation_request(PAYMENT_CREATION_REQUEST_BODY)],
        );
        let observed = ScriptedObservation::observed(
            project_result(result, |outcome| {
                assert_eq!(outcome, PaymentCreationOutcome::OtpStepUpRequired);
            }),
            &transport,
        );
        assert_eq!(observed, expected);
    }

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/payments"))
        .and(body_string(PAYMENT_CREATION_VERIFIED_REQUEST_BODY))
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
    let outcome = client
        .create_payment(
            &token,
            &device_id,
            &pay_plan()?,
            PaymentVerification::SmsOtpVerified,
        )
        .await?;
    assert!(matches!(outcome, PaymentCreationOutcome::Created(_)));
    assert_request_count(&server, 1).await;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn payment_otp_graphql_calls_match_the_current_app_contract() -> TestResult {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(header("authorization", "Bearer synthetic-token"))
        .and(header("device-id", "synthetic-device"))
        .and(body_string(ISSUE_PAYMENT_OTP_REQUEST_BODY))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {"sendOtp": {"success": true}}
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(header("authorization", "Bearer synthetic-token"))
        .and(header("device-id", "synthetic-device"))
        .and(body_string(VERIFY_PAYMENT_OTP_REQUEST_BODY))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {"validateOtp": {"validated": true, "reasonCode": null}}
        })))
        .mount(&server)
        .await;
    let client = test_client(&server)?;
    let (token, device_id) = test_session()?;
    let request_id =
        crate::shared::ClientRequestId::from_str("123e4567-e89b-12d3-a456-426614174000")?;
    let otp = OtpCode::parse_owned("123456".to_owned())?;

    client
        .issue_payment_otp(&token, &device_id, &request_id)
        .await?;
    let verification = client
        .verify_payment_otp(&token, &device_id, &request_id, &otp)
        .await?;

    assert_eq!(verification, PaymentOtpVerification::Verified);
    assert_request_count(&server, 2).await;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn payment_otp_verification_preserves_known_rejection_reasons() -> TestResult {
    for (reason, expected) in [
        ("otpIncorrect", PaymentOtpVerification::Incorrect),
        ("otpExpired", PaymentOtpVerification::Expired),
        ("otpUnexpected", PaymentOtpVerification::Unexpected),
        (
            "tooManyIncorrectAttempts",
            PaymentOtpVerification::TooManyIncorrectAttempts,
        ),
    ] {
        let response = scripted_json_response(
            200,
            serde_json::json!({
                "data": {"validateOtp": {"validated": false, "reasonCode": reason}}
            }),
        )?;
        let (client, _transport) = scripted_client([Ok(response)])?;
        let (token, device_id) = test_session()?;
        let request_id =
            crate::shared::ClientRequestId::from_str("123e4567-e89b-12d3-a456-426614174000")?;
        let otp = OtpCode::parse_owned("123456".to_owned())?;

        assert_eq!(
            client
                .verify_payment_otp(&token, &device_id, &request_id, &otp)
                .await?,
            expected
        );
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn payment_otp_verification_prioritizes_validated_success_over_reason_code() -> TestResult {
    for reason in [
        serde_json::Value::Null,
        serde_json::json!("otpIncorrect"),
        serde_json::json!("unknownSuccessDetail"),
    ] {
        let response = scripted_json_response(
            200,
            serde_json::json!({
                "data": {"validateOtp": {"validated": true, "reasonCode": reason}}
            }),
        )?;
        let (client, _transport) = scripted_client([Ok(response)])?;
        let (token, device_id) = test_session()?;
        let request_id =
            crate::shared::ClientRequestId::from_str("123e4567-e89b-12d3-a456-426614174000")?;
        let otp = OtpCode::parse_owned("123456".to_owned())?;

        assert_eq!(
            client
                .verify_payment_otp(&token, &device_id, &request_id, &otp)
                .await?,
            PaymentOtpVerification::Verified
        );
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn request_creation_sends_negative_amount_without_payment_only_fields() -> TestResult {
    for (visibility, expected_body) in [
        (Visibility::Private, REQUEST_CREATION_REQUEST_BODY),
        (Visibility::Friends, REQUEST_CREATION_FRIENDS_REQUEST_BODY),
        (Visibility::Public, REQUEST_CREATION_PUBLIC_REQUEST_BODY),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/payments"))
            .and(header("authorization", "Bearer synthetic-token"))
            .and(header("device-id", "synthetic-device"))
            .and(body_string(expected_body))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                created_payment_body_with_visibility(
                    "request-1",
                    "charge",
                    "pending",
                    "123",
                    "456",
                    visibility,
                ),
            ))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;
        let created = client
            .create_request(
                &token,
                &device_id,
                &request_plan_with_visibility(visibility)?,
            )
            .await?;
        assert_eq!(created.id().as_str(), "request-1");
        assert_eq!(created.status().as_str(), "pending");
        assert_request_count(&server, 1).await;
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn creation_accepts_a_supported_response_audience_no_more_public_than_requested() -> TestResult
{
    let payment_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/payments"))
        .and(body_string(PAYMENT_CREATION_PUBLIC_REQUEST_BODY))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            created_payment_body_with_visibility(
                "payment-1",
                "pay",
                "settled",
                "123",
                "456",
                Visibility::Private,
            ),
        ))
        .mount(&payment_server)
        .await;
    let client = test_client(&payment_server)?;
    let (token, device_id) = test_session()?;

    let payment = client
        .create_payment(
            &token,
            &device_id,
            &pay_plan_with_visibility(Visibility::Public)?,
            PaymentVerification::Unverified,
        )
        .await?;
    let PaymentCreationOutcome::Created(payment) = payment else {
        return Err(io::Error::other("payment unexpectedly required OTP").into());
    };

    assert_eq!(payment.id().as_str(), "payment-1");
    assert_request_count(&payment_server, 1).await;

    let request_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/payments"))
        .and(body_string(REQUEST_CREATION_FRIENDS_REQUEST_BODY))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            created_payment_body_with_visibility(
                "request-1",
                "charge",
                "pending",
                "123",
                "456",
                Visibility::Private,
            ),
        ))
        .mount(&request_server)
        .await;
    let client = test_client(&request_server)?;

    let request = client
        .create_request(
            &token,
            &device_id,
            &request_plan_with_visibility(Visibility::Friends)?,
        )
        .await?;

    assert_eq!(request.id().as_str(), "request-1");
    assert_request_count(&request_server, 1).await;
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
    let mut missing_audience = created_payment_body("payment-1", "pay", "settled", "123", "456");
    missing_audience["data"]["payment"]["audience"] = Value::Null;
    let mut mismatched_audience = created_payment_body("payment-1", "pay", "settled", "123", "456");
    mismatched_audience["data"]["payment"]["audience"] = Value::String("public".to_owned());
    let mut unknown_audience = created_payment_body("payment-1", "pay", "settled", "123", "456");
    unknown_audience["data"]["payment"]["audience"] = Value::String("synthetic".to_owned());
    let bodies = [
        (200_u16, "not-json".to_owned(), None),
        (
            200,
            serde_json::json!({"data": direct_payment}).to_string(),
            None,
        ),
        (200, missing_timestamp.to_string(), None),
        (200, invalid_timestamp.to_string(), None),
        (200, missing_audience.to_string(), None),
        (200, mismatched_audience.to_string(), None),
        (200, unknown_audience.to_string(), None),
        (
            200,
            created_payment_body("payment-1", "pay", "settled", "123", "999").to_string(),
            None,
        ),
        (
            500,
            serde_json::json!({"error": {"code": "unknown"}}).to_string(),
            Some("unknown"),
        ),
        (
            500,
            serde_json::json!({"error": {"code": "1396"}}).to_string(),
            Some("1396"),
        ),
        (
            400,
            serde_json::json!({"error_code": "1396"}).to_string(),
            Some("1396"),
        ),
        (
            200,
            serde_json::json!({"error": {"code": "1396"}}).to_string(),
            Some("1396"),
        ),
    ];
    for (status, body, code) in bodies {
        // Setup.
        let response = scripted_response(status, body.into_bytes())?;
        let (token, device_id) = test_session()?;

        // Immutable initial script/state.
        let script = [Ok(response)];
        let (client, transport) = scripted_client(script)?;

        // Complete expected observation.
        let expected_error = if status >= 400 {
            ApiErrorSnapshot::financial_http_unknown(PAYMENT_CREATION_OPERATION, status, code)
        } else {
            ApiErrorSnapshot::financial_unknown(PAYMENT_CREATION_OPERATION)
        };
        let expected = ScriptedObservation::expected(
            Err(expected_error),
            vec![payment_creation_request(PAYMENT_CREATION_REQUEST_BODY)],
        );

        // Execute once.
        let result = client
            .create_payment(
                &token,
                &device_id,
                &pay_plan()?,
                PaymentVerification::Unverified,
            )
            .await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);

        assert_eq!(observed, expected);
        assert!(!format!("{observed:?}").contains("synthetic-eligibility-token"));
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn request_creation_missing_or_more_public_audience_is_ambiguous() -> TestResult {
    let mut missing = created_payment_body_with_visibility(
        "request-1",
        "charge",
        "pending",
        "123",
        "456",
        Visibility::Friends,
    );
    missing["data"]["payment"]["audience"] = Value::Null;
    let more_public = created_payment_body_with_visibility(
        "request-1",
        "charge",
        "pending",
        "123",
        "456",
        Visibility::Public,
    );

    for body in [missing, more_public] {
        let response = scripted_json_response(200, body)?;
        let (token, device_id) = test_session()?;
        let (client, transport) = scripted_client([Ok(response)])?;
        let expected = ScriptedObservation::expected(
            Err(ApiErrorSnapshot::financial_unknown(
                REQUEST_CREATION_OPERATION,
            )),
            vec![payment_creation_request(
                REQUEST_CREATION_FRIENDS_REQUEST_BODY,
            )],
        );

        let result = client
            .create_request(
                &token,
                &device_id,
                &request_plan_with_visibility(Visibility::Friends)?,
            )
            .await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);

        assert_eq!(observed, expected);
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
        .create_payment(
            &token,
            &device_id,
            &pay_plan()?,
            PaymentVerification::Unverified,
        )
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
    for code in ["13006"] {
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
            .create_payment(
                &token,
                &device_id,
                &pay_plan()?,
                PaymentVerification::Unverified,
            )
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
        Err(ApiErrorSnapshot::financial_http_unknown(
            REQUEST_CREATION_OPERATION,
            400,
            Some("13006"),
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

#[tokio::test(flavor = "current_thread")]
async fn duplicate_and_temporary_payment_rejections_have_specific_safe_messages() -> TestResult {
    for (code, expected_error) in [
        ("1360", ApiErrorSnapshot::duplicate_payment_rejected()),
        ("10100", ApiErrorSnapshot::temporary_payment_rejected()),
    ] {
        let response = scripted_json_response(403, serde_json::json!({"error": {"code": code}}))?;
        let (token, device_id) = test_session()?;
        let (client, transport) = scripted_client([Ok(response)])?;
        let expected = ScriptedObservation::expected(
            Err(expected_error),
            vec![payment_creation_request(PAYMENT_CREATION_REQUEST_BODY)],
        );

        let result = client
            .create_payment(
                &token,
                &device_id,
                &pay_plan()?,
                PaymentVerification::Unverified,
            )
            .await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);

        assert_eq!(observed, expected);
    }

    for (status, body, code) in [
        (400, serde_json::json!({"error": {"code": "1360"}}), "1360"),
        (
            400,
            serde_json::json!({"error": {"code": "10100"}}),
            "10100",
        ),
        (403, serde_json::json!({"error_code": "1360"}), "1360"),
        (403, serde_json::json!({"error_code": "10100"}), "10100"),
    ] {
        let response = scripted_json_response(status, body)?;
        let (token, device_id) = test_session()?;
        let (client, transport) = scripted_client([Ok(response)])?;
        let expected = ScriptedObservation::expected(
            Err(ApiErrorSnapshot::financial_http_unknown(
                PAYMENT_CREATION_OPERATION,
                status,
                Some(code),
            )),
            vec![payment_creation_request(PAYMENT_CREATION_REQUEST_BODY)],
        );

        let result = client
            .create_payment(
                &token,
                &device_id,
                &pay_plan()?,
                PaymentVerification::Unverified,
            )
            .await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);

        assert_eq!(observed, expected);
    }
    Ok(())
}

use super::*;

#[tokio::test(flavor = "current_thread")]
async fn pending_requests_map_both_directions_and_validate_detail_ids() -> TestResult {
    let server = MockServer::start().await;
    let next = format!(
        "{}/v1/payments?action=charge&before=request-3&limit=2&status=pending%2Cheld",
        server.uri()
    );
    let outgoing = request_body("request-1", "123", "456", "pending");
    let incoming = request_body("request-2", "789", "123", "held");
    Mock::given(method("GET"))
        .and(path("/v1/payments"))
        .and(query_param("action", "charge"))
        .and(query_param("status", "pending,held"))
        .and(query_param("limit", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data":[outgoing.clone(),incoming],"pagination":{"next":next}
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/payments/request-1"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"data":outgoing})),
        )
        .mount(&server)
        .await;
    let client = test_client(&server)?;
    let (token, device_id) = test_session()?;
    let user_id = UserId::from_str("123")?;
    let request_id = RequestId::from_str("request-1")?;
    let size = Limit::try_from(2)?;
    let page = client
        .pending_requests(
            &token,
            &device_id,
            &user_id,
            PendingRequestsPageRequest::new(size, None),
        )
        .await?;
    let (requests, next) = page.into_parts();
    let detail = client
        .request_by_id(&token, &device_id, &user_id, &request_id)
        .await?;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].direction(), RequestDirection::Outgoing);
    assert_eq!(requests[1].direction(), RequestDirection::Incoming);
    assert_eq!(detail.id(), &request_id);
    assert_eq!(next.as_ref().map(RequestsBefore::as_str), Some("request-3"));
    assert_request_count(&server, 2).await;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn pending_requests_reject_non_charge_or_non_pending_records() -> TestResult {
    for (action, status) in [("pay", "pending"), ("charge", "settled")] {
        // Setup.
        let mut body = request_body("request-1", "123", "456", status);
        if let Some(object) = body.as_object_mut() {
            object.insert("action".to_owned(), Value::String(action.to_owned()));
        }
        let response = scripted_json_response(
            200,
            serde_json::json!({"data":[body],"pagination":{"next":null}}),
        )?;
        let (token, device_id) = test_session()?;
        let user_id = UserId::from_str("123")?;

        // Immutable initial script/state.
        let script = [Ok(response)];
        let (client, transport) = scripted_client(script)?;

        // Complete expected observation.
        let expected = ScriptedObservation::expected(
            Err(ApiErrorSnapshot::contract(REQUEST_LIST_OPERATION)),
            vec![authenticated_read_request(
                "/payments",
                &["payments"],
                &[
                    ("action", "charge"),
                    ("status", "pending,held"),
                    ("limit", "1"),
                ],
            )],
        );

        // Execute once.
        let result = client
            .pending_requests(
                &token,
                &device_id,
                &user_id,
                PendingRequestsPageRequest::new(Limit::MIN, None),
            )
            .await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn pending_requests_reject_malformed_dtos_and_duplicate_continuations_without_a_socket()
-> TestResult {
    for body in [
        serde_json::json!({"data": {}, "pagination": {"next": null}}),
        serde_json::json!({
            "data": [{
                "id": "request-1",
                "status": "pending",
                "action": "charge",
                "amount": "0.01"
            }],
            "pagination": {"next": null}
        }),
    ] {
        // Setup.
        let response = scripted_json_response(200, body)?;
        let (token, device_id) = test_session()?;

        // Immutable initial script/state.
        let script = [Ok(response)];
        let (client, transport) = scripted_client(script)?;

        // Complete expected observation.
        let expected = ScriptedObservation::expected(
            Err(ApiErrorSnapshot::contract(REQUEST_LIST_OPERATION)),
            vec![authenticated_read_request(
                "/payments",
                &["payments"],
                &[
                    ("action", "charge"),
                    ("status", "pending,held"),
                    ("limit", "1"),
                ],
            )],
        );

        // Execute once.
        let result = client
            .pending_requests(
                &token,
                &device_id,
                &UserId::from_str("123")?,
                PendingRequestsPageRequest::new(Limit::MIN, None),
            )
            .await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);

        assert_eq!(observed, expected);
    }

    for next in [
        "https://api.venmo.com/v1/payments?action=charge&before=request-2&before=request-3&limit=1&status=pending%2Cheld",
        "https://api.venmo.com/v1/payments?action=charge&action=charge&before=request-2&limit=1&status=pending%2Cheld",
    ] {
        // Setup.
        let response = scripted_json_response(
            200,
            serde_json::json!({"data": [], "pagination": {"next": next}}),
        )?;
        let (token, device_id) = test_session()?;

        // Immutable initial script/state.
        let script = [Ok(response)];
        let (client, transport) = scripted_client(script)?;

        // Complete expected observation.
        let expected = ScriptedObservation::expected(
            Err(ApiErrorSnapshot::contract(REQUEST_LIST_OPERATION)),
            vec![authenticated_read_request(
                "/payments",
                &["payments"],
                &[
                    ("action", "charge"),
                    ("status", "pending,held"),
                    ("limit", "1"),
                ],
            )],
        );

        // Execute once.
        let result = client
            .pending_requests(
                &token,
                &device_id,
                &UserId::from_str("123")?,
                PendingRequestsPageRequest::new(Limit::MIN, None),
            )
            .await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn request_detail_preserves_terminal_state_for_mutation_preflight() -> TestResult {
    for (action, status, actor, target, direction) in [
        (
            "charge",
            "cancelled",
            "456",
            "123",
            RequestDirection::Incoming,
        ),
        ("pay", "settled", "123", "456", RequestDirection::Outgoing),
    ] {
        let server = MockServer::start().await;
        let mut body = request_body("request-1", actor, target, status);
        body["action"] = Value::String(action.to_owned());
        Mock::given(method("GET"))
            .and(path("/v1/payments/request-1"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"data":body})),
            )
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;
        let user_id = UserId::from_str("123")?;
        let request_id = RequestId::from_str("request-1")?;
        let detail = client
            .request_by_id(&token, &device_id, &user_id, &request_id)
            .await?;
        assert_eq!(detail.status().as_str(), status);
        assert_eq!(detail.direction(), direction);
        assert_eq!(
            detail.action(),
            if action == "charge" {
                RequestAction::Charge
            } else {
                RequestAction::Pay
            }
        );
        assert_request_count(&server, 1).await;
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn request_acceptance_uses_exact_approve_update_and_validates_settlement() -> TestResult {
    for (action, actor_id, target_id) in [("charge", "456", "123"), ("pay", "123", "456")] {
        let server = MockServer::start().await;
        let mut response = updated_payment_body(action, "settled", actor_id, target_id);
        response["data"]["id"] = Value::String("payment-1".to_owned());
        response["data"]["date_created"] = Value::String("2026-07-14T23:50:08Z".to_owned());
        Mock::given(method("PUT"))
            .and(path("/v1/payments/request-1"))
            .and(header("accept", "application/json; charset=utf-8"))
            .and(header("content-type", "application/json"))
            .and(header("authorization", "Bearer synthetic-token"))
            .and(header("device-id", "synthetic-device"))
            .and(body_string(r#"{"action":"approve"}"#))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;
        let accepted = client
            .accept_request(&token, &device_id, &accept_plan()?)
            .await?;
        assert_eq!(
            accepted.payment_id().map(PaymentId::as_str),
            Some("payment-1")
        );
        assert_eq!(accepted.status(), Some(FinancialStatus::Settled));
        assert_request_count(&server, 1).await;
        assert_requests_have_no_query(&server).await;
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn request_approval_notification_resolves_the_unique_nested_payment_id() -> TestResult {
    let response = scripted_json_response(
        200,
        serde_json::json!({"data":[
            {"id":"notification-other"},
            {"id":"notification-1","payment":{"id":"request-1"}}
        ]}),
    )?;
    let (client, transport) = scripted_client([Ok(response)])?;
    let (token, device_id) = test_session()?;
    let request_id = RequestId::from_str("request-1")?;

    let result = client
        .request_approval_notification_id(&token, &device_id, &request_id)
        .await;
    let observed = ScriptedObservation::observed(
        project_result(result, |notification_id| {
            notification_id.as_str().to_owned()
        }),
        &transport,
    );
    let expected = ScriptedObservation::expected(
        Ok("notification-1".to_owned()),
        vec![authenticated_read_request(
            "/notifications",
            &["notifications"],
            &[("acknowledged", "false")],
        )],
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn request_approval_notification_rejects_missing_duplicate_or_invalid_matches() -> TestResult
{
    for body in [
        serde_json::json!({"data":[{"id":"notification-1","payment":{"id":"request-2"}}]}),
        serde_json::json!({"data":[
            {"id":"notification-1","payment":{"id":"request-1"}},
            {"id":"notification-2","payment":{"id":"request-1"}}
        ]}),
        serde_json::json!({"data":[{"id":"bad id","payment":{"id":"request-1"}}]}),
    ] {
        let response = scripted_json_response(200, body)?;
        let (client, transport) = scripted_client([Ok(response)])?;
        let (token, device_id) = test_session()?;
        let request_id = RequestId::from_str("request-1")?;

        let result = client
            .request_approval_notification_id(&token, &device_id, &request_id)
            .await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);
        let expected = ScriptedObservation::expected(
            Err(ApiErrorSnapshot::contract(
                REQUEST_APPROVAL_NOTIFICATION_OPERATION,
            )),
            vec![authenticated_read_request(
                "/notifications",
                &["notifications"],
                &[("acknowledged", "false")],
            )],
        );

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn request_approval_eligibility_uses_source_bound_form_and_preserves_empty_fees() -> TestResult
{
    let response = scripted_json_response(
        200,
        serde_json::json!({"data": {
            "eligible": true,
            "eligibility_token": "synthetic-approval-token",
            "fees": []
        }}),
    )?;
    let (client, transport) = scripted_client([Ok(response)])?;
    let (token, device_id) = test_session()?;
    let requester = financial_user("456", "requester")?;
    let funding = PeerFundingSource::external(zero_fee_peer_method()?);

    let result = client
        .request_approval_eligibility(
            &token,
            &device_id,
            &requester,
            125,
            "Dinner & café",
            &funding,
        )
        .await;
    let observed = ScriptedObservation::observed(
        project_result(result, |eligibility| {
            let (token, fees) = eligibility.into_parts();
            (
                token.expose().to_owned(),
                fees.entries().map(<[_]>::len),
                fees.total_cents(),
            )
        }),
        &transport,
    );
    let expected = ScriptedObservation::expected(
        Ok(("synthetic-approval-token".to_owned(), Some(0), 0)),
        vec![ScriptedRequest::for_test_form(
            ScriptedCredentials::authenticated_for_test(
                SYNTHETIC_ACCESS_TOKEN,
                SYNTHETIC_DEVICE_ID,
            ),
            Method::POST,
            "/protection/eligibility",
            &["protection", "eligibility"],
            &[],
            b"target_type=user_id&target_id=456&country_code=1&amount=125&note=Dinner+%26+caf%C3%A9&funding_source_id=bank-1",
            OperationClass::NonFinancialWrite,
        )],
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn request_approval_eligibility_accepts_and_totals_valid_fees() -> TestResult {
    let response = scripted_json_response(
        200,
        serde_json::json!({"data": {
            "eligible": true,
            "eligibility_token": "synthetic-approval-token",
            "fees": [
                {
                    "product_uri":"venmo://fees/one","applied_to":"transaction",
                    "fee_token":"fee-token-1","base_fee_amount":25,"fee_percentage":2.5,
                    "calculated_fee_amount_in_cents":25
                },
                {
                    "product_uri":"venmo://fees/two","applied_to":"transaction",
                    "fee_token":"fee-token-2","calculated_fee_amount_in_cents":10
                }
            ]
        }}),
    )?;
    let (client, transport) = scripted_client([Ok(response)])?;
    let (token, device_id) = test_session()?;
    let requester = financial_user("456", "requester")?;
    let funding = PeerFundingSource::external(zero_fee_peer_method()?);

    let result = client
        .request_approval_eligibility(&token, &device_id, &requester, 125, "Dinner", &funding)
        .await;
    let observed = ScriptedObservation::observed(
        project_result(result, |eligibility| {
            let (_, fees) = eligibility.into_parts();
            (fees.entries().map(<[_]>::len), fees.total_cents())
        }),
        &transport,
    );
    let expected = ScriptedObservation::expected(
        Ok((Some(2), 35)),
        vec![ScriptedRequest::for_test_form(
            ScriptedCredentials::authenticated_for_test(
                SYNTHETIC_ACCESS_TOKEN,
                SYNTHETIC_DEVICE_ID,
            ),
            Method::POST,
            "/protection/eligibility",
            &["protection", "eligibility"],
            &[],
            b"target_type=user_id&target_id=456&country_code=1&amount=125&note=Dinner&funding_source_id=bank-1",
            OperationClass::NonFinancialWrite,
        )],
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn request_approval_eligibility_rejects_denial_missing_token_and_invalid_fees() -> TestResult
{
    let cases = [
        (
            serde_json::json!({"data":{"eligible":false}}),
            ApiErrorSnapshot {
                kind: ApiFailureKind::Rejected,
                detail: ApiErrorDetail::RequestApprovalEligibilityDenied,
            },
        ),
        (
            serde_json::json!({"data":{"eligible":true,"fees":[]}}),
            ApiErrorSnapshot::contract(REQUEST_APPROVAL_ELIGIBILITY_OPERATION),
        ),
        (
            serde_json::json!({"data":{
                "eligible":true,
                "eligibility_token":"synthetic-approval-token",
                "fees":[{"fee_token":"unsupported"}]
            }}),
            ApiErrorSnapshot::contract(REQUEST_APPROVAL_ELIGIBILITY_OPERATION),
        ),
        (
            serde_json::json!({"data":{
                "eligible":true,
                "eligibility_token":"synthetic-approval-token",
                "fees":[
                    {"product_uri":"a","applied_to":"b","fee_token":"c","calculated_fee_amount_in_cents":18446744073709551615_u64},
                    {"product_uri":"d","applied_to":"e","fee_token":"f","calculated_fee_amount_in_cents":1}
                ]
            }}),
            ApiErrorSnapshot::contract(REQUEST_APPROVAL_ELIGIBILITY_OPERATION),
        ),
    ];
    let (token, device_id) = test_session()?;
    let requester = financial_user("456", "requester")?;
    let funding = PeerFundingSource::external(zero_fee_peer_method()?);

    for (body, expected_error) in cases {
        let response = scripted_json_response(200, body)?;
        let (client, transport) = scripted_client([Ok(response)])?;
        let result = client
            .request_approval_eligibility(&token, &device_id, &requester, 125, "Dinner", &funding)
            .await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);
        let expected = ScriptedObservation::expected(
            Err(expected_error),
            vec![ScriptedRequest::for_test_form(
                ScriptedCredentials::authenticated_for_test(
                    SYNTHETIC_ACCESS_TOKEN,
                    SYNTHETIC_DEVICE_ID,
                ),
                Method::POST,
                "/protection/eligibility",
                &["protection", "eligibility"],
                &[],
                b"target_type=user_id&target_id=456&country_code=1&amount=125&note=Dinner&funding_source_id=bank-1",
                OperationClass::NonFinancialWrite,
            )],
        );
        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn source_funded_request_acceptance_uses_modern_options_route() -> TestResult {
    let response = scripted_json_response(200, serde_json::json!({"data": {"url": null}}))?;
    let (client, transport) = scripted_client([Ok(response)])?;
    let (token, device_id) = test_session()?;

    let result = client
        .accept_request(
            &token,
            &device_id,
            &source_funded_accept_plan_with_fees(RequestApprovalFees::present(
                vec![synthetic_approval_fee(25)],
                25,
            ))?,
        )
        .await;
    let observed = ScriptedObservation::observed(
        project_result(result, |accepted| {
            (accepted.payment_id().is_none(), accepted.status().is_none())
        }),
        &transport,
    );
    let expected = ScriptedObservation::expected(
        Ok((true, true)),
        vec![authenticated_request(
            Method::PUT,
            "/requests/{request-id}",
            &["requests", "notification-1"],
            &[],
            Some(br#"{"funding_source_id":"bank-1","eligibility_token":"synthetic-approval-token","fees":[{"product_uri":"venmo://fees/request-approval","applied_to":"transaction","fee_token":"synthetic-fee-token","base_fee_amount":25,"fee_percentage":2.5,"calculated_fee_amount_in_cents":25}],"metadata":{"quasi_cash_disclaimer_viewed":false}}"#),
            OperationClass::FinancialWrite,
        )],
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn source_funded_acceptance_continuations_and_malformed_success_are_ambiguous() -> TestResult
{
    for body in [
        serde_json::json!({"data":{"url":"https://venmo.example/continue"}}),
        serde_json::json!([]),
    ] {
        let response = scripted_json_response(200, body)?;
        let (client, transport) = scripted_client([Ok(response)])?;
        let (token, device_id) = test_session()?;
        let result = client
            .accept_request(
                &token,
                &device_id,
                &source_funded_unprotected_accept_plan()?,
            )
            .await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);
        let expected = ScriptedObservation::expected(
            Err(ApiErrorSnapshot {
                kind: ApiFailureKind::AmbiguousWrite,
                detail: ApiErrorDetail::FinancialOutcomeUnknown {
                    operation: REQUEST_ACCEPTANCE_OPERATION,
                },
            }),
            vec![authenticated_request(
                Method::PUT,
                "/requests/{request-id}",
                &["requests", "notification-1"],
                &[],
                Some(br#"{"funding_source_id":"bank-1","eligibility_token":"synthetic-approval-token","metadata":{"quasi_cash_disclaimer_viewed":false}}"#),
                OperationClass::FinancialWrite,
            )],
        );
        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn request_decline_uses_deny_not_cancel_and_requires_terminal_response() -> TestResult {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/v1/payments/request-1"))
        .and(header("accept", "application/json; charset=utf-8"))
        .and(header("content-type", "application/json"))
        .and(header("authorization", "Bearer synthetic-token"))
        .and(header("device-id", "synthetic-device"))
        .and(body_string(r#"{"action":"deny"}"#))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(updated_payment_body(
                "charge",
                "cancelled",
                "456",
                "123",
            )),
        )
        .mount(&server)
        .await;
    let client = test_client(&server)?;
    let (token, device_id) = test_session()?;
    let declined = client
        .decline_request(&token, &device_id, &decline_plan()?)
        .await?;
    assert_eq!(declined.request_id().as_str(), "request-1");
    assert_eq!(declined.status().as_str(), "cancelled");
    assert_request_count(&server, 1).await;
    assert_requests_have_no_query(&server).await;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn request_update_mismatches_and_unverified_errors_are_ambiguous() -> TestResult {
    let mut invalid_payment_id = updated_payment_body("pay", "settled", "123", "456");
    invalid_payment_id["data"]["id"] = Value::String("bad id".to_owned());
    let mut predating_payment = updated_payment_body("pay", "settled", "123", "456");
    predating_payment["data"]["id"] = Value::String("payment-1".to_owned());
    predating_payment["data"]["date_created"] = Value::String("2026-07-11T11:59:59Z".to_owned());
    for (status, body) in [
        (200, updated_payment_body("pay", "settled", "456", "123")),
        (200, invalid_payment_id),
        (200, predating_payment),
        (
            200,
            serde_json::json!({"data":{"id":"request-1","status":"settled"}}),
        ),
        (400, serde_json::json!({"error":{"code":2901}})),
        (401, serde_json::json!({"error":{"code":"unauthorized"}})),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/v1/payments/request-1"))
            .respond_with(ResponseTemplate::new(status).set_body_json(body))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;
        let result = client
            .accept_request(&token, &device_id, &accept_plan()?)
            .await;
        assert!(if status >= 400 {
            matches!(
                result,
                Err(VenmoApiError::FinancialHttpOutcomeUnknown { .. })
            )
        } else {
            matches!(result, Err(VenmoApiError::FinancialOutcomeUnknown { .. }))
        });
        assert_eq!(
            result.as_ref().err().map(ApiFailure::kind),
            Some(ApiFailureKind::AmbiguousWrite)
        );
        assert_request_count(&server, 1).await;
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn decline_rejects_every_unproven_terminal_response_as_ambiguous() -> TestResult {
    let mut wrong_id = updated_payment_body("charge", "cancelled", "456", "123");
    wrong_id["data"]["id"] = Value::String("request-2".to_owned());
    let mut wrong_amount = updated_payment_body("charge", "cancelled", "456", "123");
    wrong_amount["data"]["amount"] = Value::String("0.02".to_owned());
    let mut wrong_note = updated_payment_body("charge", "cancelled", "456", "123");
    wrong_note["data"]["note"] = Value::String("Different note".to_owned());
    let mut wrong_audience = updated_payment_body("charge", "cancelled", "456", "123");
    wrong_audience["data"]["audience"] = Value::String("public".to_owned());
    let mut wrong_created_at = updated_payment_body("charge", "cancelled", "456", "123");
    wrong_created_at["data"]["date_created"] = Value::String("2026-07-11T12:00:01".to_owned());
    for (status, body) in [
        (200, updated_payment_body("charge", "pending", "456", "123")),
        (200, updated_payment_body("pay", "cancelled", "456", "123")),
        (
            200,
            updated_payment_body("charge", "cancelled", "123", "456"),
        ),
        (200, wrong_id),
        (200, wrong_amount),
        (200, wrong_note),
        (200, wrong_audience),
        (200, wrong_created_at),
        (400, serde_json::json!({"error":{"code":2901}})),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/v1/payments/request-1"))
            .respond_with(ResponseTemplate::new(status).set_body_json(body))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;
        let result = client
            .decline_request(&token, &device_id, &decline_plan()?)
            .await;
        assert!(if status >= 400 {
            matches!(
                result,
                Err(VenmoApiError::FinancialHttpOutcomeUnknown { .. })
            )
        } else {
            matches!(result, Err(VenmoApiError::FinancialOutcomeUnknown { .. }))
        });
        assert_eq!(
            result.as_ref().err().map(ApiFailure::kind),
            Some(ApiFailureKind::AmbiguousWrite)
        );
        assert_request_count(&server, 1).await;
    }
    Ok(())
}

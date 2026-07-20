use super::*;

#[tokio::test(flavor = "current_thread")]
async fn payment_methods_map_supported_envelopes_and_default_roles() -> TestResult {
    for (body, expected_methods) in [
        (
            r#"{"data":[{"id":"balance-1","type":"balance","name":"Venmo balance","last_four":null,"peer_payment_role":"default"},{"id":"bank-1","payment_method_type":"bank","display_name":"Bank","lastFour":1234,"isDefault":false}]}"#,
            vec![
                PaymentMethodSnapshot {
                    id: "balance-1".to_owned(),
                    name: Some("Venmo balance".to_owned()),
                    method_type: Some("balance".to_owned()),
                    last_four: None,
                    is_default: true,
                },
                PaymentMethodSnapshot {
                    id: "bank-1".to_owned(),
                    name: Some("Bank".to_owned()),
                    method_type: Some("bank".to_owned()),
                    last_four: Some("1234".to_owned()),
                    is_default: false,
                },
            ],
        ),
        (
            r#"{"data":{"payment_methods":[{"id":123,"label":"Card","type":"card","merchant_payment_role":"backup"}]}}"#,
            vec![PaymentMethodSnapshot {
                id: "123".to_owned(),
                name: Some("Card".to_owned()),
                method_type: Some("card".to_owned()),
                last_four: None,
                is_default: false,
            }],
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
            Ok(expected_methods),
            vec![authenticated_read_request(
                "/payment-methods",
                &["payment-methods"],
                &[],
            )],
        );

        // Execute once.
        let result = client.payment_methods(&token, &device_id).await;
        let observed = ScriptedObservation::observed(
            project_result(result, |methods| {
                methods
                    .iter()
                    .map(PaymentMethodSnapshot::from)
                    .collect::<Vec<_>>()
            }),
            &transport,
        );

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn payment_methods_reject_duplicate_or_invalid_ids() -> TestResult {
    for body in [
        r#"{"data":[{"id":"same"},{"id":"same"}]}"#,
        r#"{"data":[{"id":"bad id"}]}"#,
    ] {
        // Setup.
        let response = scripted_response(200, body.as_bytes().to_vec())?;
        let (token, device_id) = test_session()?;

        // Immutable initial script/state.
        let script = [Ok(response)];
        let (client, transport) = scripted_client(script)?;

        // Complete expected observation.
        let expected = ScriptedObservation::expected(
            Err(ApiErrorSnapshot::contract(PAYMENT_METHODS_OPERATION)),
            vec![authenticated_read_request(
                "/payment-methods",
                &["payment-methods"],
                &[],
            )],
        );

        // Execute once.
        let result = client.payment_methods(&token, &device_id).await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn peer_funding_uses_only_peer_roles_and_explicit_fee_evidence() -> TestResult {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/payment-methods"))
        .and(header("authorization", "Bearer synthetic-token"))
        .and(header("device-id", "synthetic-device"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                {"id":"balance-1","type":"balance","name":"Venmo balance","peer_payment_role":"default","fee":{"calculated_fee_amount_in_cents":0}},
                {"id":"bank-1","type":"bank","name":"Bank","peer_payment_role":"backup","merchant_payment_role":"none","fee":{"calculated_fee_amount_in_cents":0}},
                {"id":"card-1","type":"card","name":"Card","peer_payment_role":"backup","merchant_payment_role":"default","fee":{"calculated_fee_amount_in_cents":3}},
                {"id":"excluded-1","peer_payment_role":"none","merchant_payment_role":"default"}
            ]
        })))
        .mount(&server)
        .await;
    let client = test_client(&server)?;
    let (token, device_id) = test_session()?;
    let sources = client.peer_funding_sources(&token, &device_id).await?;
    let methods = sources.external();
    assert_eq!(
        sources.balance().map(|method| method.id().as_str()),
        Some("balance-1")
    );
    assert_eq!(methods.len(), 2);
    assert_eq!(methods[0].role(), PeerFundingRole::Backup);
    assert_eq!(methods[0].fee(), PeerFundingFee::ProvenZero);
    assert_eq!(methods[1].role(), PeerFundingRole::Backup);
    assert_eq!(methods[1].fee(), PeerFundingFee::from_cents(3));
    assert!(
        !methods
            .iter()
            .any(|method| matches!(method.method().id().as_str(), "balance-1" | "excluded-1"))
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn peer_funding_rejects_missing_unknown_or_duplicate_peer_contracts() -> TestResult {
    for body in [
        serde_json::json!({"data": [{"id": "one"}]}),
        serde_json::json!({"data": [{"id": "one", "peer_payment_role": "surprise"}]}),
        serde_json::json!({"data": [{"id": "one", "type": "mystery", "peer_payment_role": "backup"}]}),
        serde_json::json!({"data": [
            {"id": "one", "peer_payment_role": "none"},
            {"id": "one", "peer_payment_role": "backup"}
        ]}),
        serde_json::json!({"data": [
            {"id": "balance-1", "type": "balance", "peer_payment_role": "default"},
            {"id": "balance-2", "type": "balance", "peer_payment_role": "backup"}
        ]}),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/payment-methods"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;
        let result = client.peer_funding_sources(&token, &device_id).await;
        assert!(matches!(result, Err(VenmoApiError::Contract { .. })));
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn peer_funding_preserves_an_unrecognized_method_fee_as_unknown() -> TestResult {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/payment-methods"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [{"id":"bank-1","type":"bank","name":"Bank","peer_payment_role":"default","fee":0}]
        })))
        .mount(&server)
        .await;
    let client = test_client(&server)?;
    let (token, device_id) = test_session()?;
    let sources = client.peer_funding_sources(&token, &device_id).await?;
    let methods = sources.external();
    assert_eq!(methods.len(), 1);
    assert_eq!(methods[0].fee(), PeerFundingFee::Unknown);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn balance_maps_exact_signed_available_and_on_hold_fields() -> TestResult {
    // Setup.
    let response = scripted_json_response(
        200,
        serde_json::json!({"data":{"balance":"12.34","balance_on_hold":"-0.05","user":{"id":"123","username":"alice"}}}),
    )?;
    let (token, device_id) = test_session()?;

    // Immutable initial script/state.
    let script = [Ok(response)];
    let (client, transport) = scripted_client(script)?;

    // Complete expected observation.
    let expected = ScriptedObservation::expected(
        Ok((1_234, -5, "$12.34".to_owned())),
        vec![authenticated_read_request("/account", &["account"], &[])],
    );

    // Execute once.
    let result = client.balance(&token, &device_id).await;
    let observed = ScriptedObservation::observed(
        project_result(result, |balance| {
            (
                balance.available().cents(),
                balance.on_hold().cents(),
                balance.available().to_string(),
            )
        }),
        &transport,
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn balance_rejects_missing_or_lossy_values() -> TestResult {
    for body in [
        serde_json::json!({"data": {"balance": "1.00"}}),
        serde_json::json!({"data": {"balance": "1.001", "balance_on_hold": "0.00"}}),
    ] {
        // Setup.
        let response = scripted_json_response(200, body)?;
        let (token, device_id) = test_session()?;

        // Immutable initial script/state.
        let script = [Ok(response)];
        let (client, transport) = scripted_client(script)?;

        // Complete expected observation.
        let expected = ScriptedObservation::expected(
            Err(ApiErrorSnapshot::contract(BALANCE_OPERATION)),
            vec![authenticated_read_request("/account", &["account"], &[])],
        );

        // Execute once.
        let result = client.balance(&token, &device_id).await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);

        assert_eq!(observed, expected);
    }
    Ok(())
}

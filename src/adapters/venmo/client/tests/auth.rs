use super::*;

#[tokio::test(flavor = "current_thread")]
async fn password_login_serializes_the_exact_http_request() -> TestResult {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/oauth/access_token"))
        .and(header("device-id", "synthetic-device"))
        .and(body_string(PASSWORD_LOGIN_REQUEST_BODY))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "synthetic-issued-token"
        })))
        .mount(&server)
        .await;
    let client = test_client(&server)?;
    let identifier = LoginIdentifier::parse_owned("alice@example.com".to_owned())?;
    let password = AccountPassword::parse_owned("synthetic-password".to_owned())?;
    let device_id = DeviceId::from_str("synthetic-device")?;

    let result = client
        .begin_password_login(&identifier, &password, &device_id)
        .await?;

    assert!(matches!(result, PasswordLoginStart::Authenticated(_)));
    let requests = server.received_requests().await;
    assert!(requests.as_ref().is_some_and(|requests| {
        requests.len() == 1 && requests[0].headers.get("authorization").is_none()
    }));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn password_login_parses_direct_and_wrapped_tokens_without_a_socket() -> TestResult {
    for response_body in [
        r#"{"access_token":"synthetic-issued-token"}"#,
        r#"{"data":{"access_token":"synthetic-issued-token"}}"#,
    ] {
        // Setup.
        let response = scripted_response(200, response_body.as_bytes().to_vec())?;
        let identifier = LoginIdentifier::parse_owned("alice@example.com".to_owned())?;
        let password = AccountPassword::parse_owned("synthetic-password".to_owned())?;
        let device_id = DeviceId::from_str(SYNTHETIC_DEVICE_ID)?;

        // Immutable initial script/state.
        let script = [Ok(response)];
        let (client, transport) = scripted_client(script)?;

        // Complete expected observation.
        let expected = ScriptedObservation::expected(
            Ok(PasswordStartSnapshot::Authenticated(
                SecretSnapshot::IssuedToken,
            )),
            vec![password_login_request()],
        );

        // Execute once.
        let result = client
            .begin_password_login(&identifier, &password, &device_id)
            .await;
        let observed = ScriptedObservation::observed(
            project_result(result, PasswordStartSnapshot::from),
            &transport,
        );

        assert_eq!(observed, expected);
        let rendered = format!("{observed:?}");
        assert!(!rendered.contains("synthetic-password"));
        assert!(!rendered.contains(SYNTHETIC_DEVICE_ID));
        assert!(!rendered.contains(SYNTHETIC_ISSUED_TOKEN));
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn password_login_returns_a_redacted_scripted_otp_challenge() -> TestResult {
    // Setup.
    let response = scripted_json_response(
        401,
        serde_json::json!({
            "error": {"code": 81109, "message": "private remote message"}
        }),
    )?
    .with_otp_secret_for_test(SYNTHETIC_OTP_SECRET);
    let identifier = LoginIdentifier::parse_owned("alice@example.com".to_owned())?;
    let password = AccountPassword::parse_owned("synthetic-password".to_owned())?;
    let device_id = DeviceId::from_str(SYNTHETIC_DEVICE_ID)?;

    // Immutable initial script/state.
    let script = [Ok(response)];
    let (client, transport) = scripted_client(script)?;

    // Complete expected observation.
    let expected = ScriptedObservation::expected(
        Ok(PasswordStartSnapshot::OtpRequired(
            SecretSnapshot::OtpSecret,
        )),
        vec![password_login_request()],
    );

    // Execute once.
    let result = client
        .begin_password_login(&identifier, &password, &device_id)
        .await;
    let observed = ScriptedObservation::observed(
        project_result(result, PasswordStartSnapshot::from),
        &transport,
    );

    assert_eq!(observed, expected);
    let rendered = format!("{observed:?}");
    assert!(!rendered.contains(SYNTHETIC_OTP_SECRET));
    assert!(!rendered.contains("synthetic-password"));
    assert!(!rendered.contains(SYNTHETIC_DEVICE_ID));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn otp_completion_and_device_trust_use_exact_sensitive_headers() -> TestResult {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/account/two-factor/token"))
        .and(header("device-id", "synthetic-device"))
        .and(header("venmo-otp-secret", "synthetic-otp-secret"))
        .and(body_string(SMS_OTP_REQUEST_BODY))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/oauth/access_token"))
        .and(query_param("client_id", "1"))
        .and(header("device-id", "synthetic-device"))
        .and(header("venmo-otp-secret", "synthetic-otp-secret"))
        .and(header("venmo-otp", "123456"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "synthetic-issued-token"
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/users/devices"))
        .and(header("authorization", "Bearer synthetic-issued-token"))
        .and(header("device-id", "synthetic-device"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let client = test_client(&server)?;
    let device_id = DeviceId::from_str("synthetic-device")?;
    let otp_secret = OtpSecret::parse_owned("synthetic-otp-secret".to_owned())?;
    let otp_code = OtpCode::parse_owned("123456".to_owned())?;

    client.request_sms_otp(&otp_secret, &device_id).await?;
    let token = client
        .complete_otp_login(&otp_code, &otp_secret, &device_id)
        .await?;
    client.trust_device(&token, &device_id).await?;

    assert_eq!(token.expose_secret(), "synthetic-issued-token");
    assert_request_count(&server, 3).await;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn malformed_scripted_otp_challenge_fails_without_exposing_remote_values() -> TestResult {
    // Setup.
    let response = scripted_json_response(
        401,
        serde_json::json!({
            "error": {"code": 81109, "message": "private remote message"}
        }),
    )?;
    let identifier = LoginIdentifier::parse_owned("alice@example.com".to_owned())?;
    let password = AccountPassword::parse_owned("synthetic-password".to_owned())?;
    let device_id = DeviceId::from_str(SYNTHETIC_DEVICE_ID)?;

    // Immutable initial script/state.
    let script = [Ok(response)];
    let (client, transport) = scripted_client(script)?;

    // Complete expected observation.
    let expected = ScriptedObservation::expected(
        Err(ApiErrorSnapshot::contract(PASSWORD_LOGIN_OPERATION)),
        vec![password_login_request()],
    );

    // Execute once.
    let result = client
        .begin_password_login(&identifier, &password, &device_id)
        .await;
    let observed = ScriptedObservation::observed(
        project_result(result, PasswordStartSnapshot::from),
        &transport,
    );

    assert_eq!(observed, expected);
    let rendered = format!("{observed:?}");
    assert!(!rendered.contains("private remote message"));
    assert!(!rendered.contains("synthetic-password"));
    assert!(!rendered.contains(SYNTHETIC_DEVICE_ID));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn malformed_password_login_success_is_unknown_and_redacted() -> TestResult {
    for response_body in [
        r#"{"access_token":"synthetic-private-token""#,
        r#"{"data":{}}"#,
        r#"{"access_token":"synthetic private token"}"#,
    ] {
        // Setup.
        let response = scripted_response(200, response_body.as_bytes().to_vec())?;
        let identifier = LoginIdentifier::parse_owned("alice@example.com".to_owned())?;
        let password = AccountPassword::parse_owned("synthetic-password".to_owned())?;
        let device_id = DeviceId::from_str(SYNTHETIC_DEVICE_ID)?;

        // Immutable initial script/state.
        let script = [Ok(response)];
        let (client, transport) = scripted_client(script)?;

        // Complete expected observation.
        let expected = ScriptedObservation::expected(
            Err(ApiErrorSnapshot::authentication_unknown(
                PASSWORD_LOGIN_OPERATION,
            )),
            vec![password_login_request()],
        );

        // Execute once.
        let result = client
            .begin_password_login(&identifier, &password, &device_id)
            .await;
        let observed = ScriptedObservation::observed(
            project_result(result, PasswordStartSnapshot::from),
            &transport,
        );

        assert_eq!(observed, expected);
        let rendered = format!("{observed:?}");
        assert!(!rendered.contains("synthetic-private-token"));
        assert!(!rendered.contains("synthetic private token"));
        assert!(!rendered.contains("synthetic-password"));
        assert!(!rendered.contains(SYNTHETIC_DEVICE_ID));
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn current_account_maps_wrapped_and_direct_envelopes() -> TestResult {
    for body in [
        r#"{"data":{"user":{"id":123,"username":"alice","display_name":"Alice"}}}"#,
        r#"{"data":{"id":"123","username":"alice","displayName":"Alice"}}"#,
    ] {
        // Setup.
        let response = scripted_response(200, body.as_bytes().to_vec())?;
        let (token, device_id) = test_session()?;

        // Immutable initial script/state.
        let script = [Ok(response)];
        let (client, transport) = scripted_client(script)?;

        // Complete expected observation.
        let expected = ScriptedObservation::expected(
            Ok(AccountSnapshot {
                user_id: "123".to_owned(),
                username: "alice".to_owned(),
                display_name: Some("Alice".to_owned()),
            }),
            vec![authenticated_read_request("/account", &["account"], &[])],
        );

        // Execute once.
        let result = client.current_account(&token, &device_id).await;
        let observed = ScriptedObservation::observed(
            project_result(result, AccountSnapshot::from),
            &transport,
        );

        assert_eq!(observed, expected);
        let rendered = format!("{observed:?}");
        assert!(!rendered.contains(SYNTHETIC_ACCESS_TOKEN));
        assert!(!rendered.contains(SYNTHETIC_DEVICE_ID));
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn revocation_uses_delete_and_accepts_an_empty_success() -> TestResult {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/v1/oauth/access_token"))
        .and(header("authorization", "Bearer synthetic-token"))
        .and(header("device-id", "synthetic-device"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let client = test_client(&server)?;
    let (token, device_id) = test_session()?;
    client.revoke_access_token(&token, &device_id).await?;
    assert_request_count(&server, 1).await;
    Ok(())
}

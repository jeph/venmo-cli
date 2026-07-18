use super::*;

#[tokio::test(flavor = "current_thread")]
async fn user_search_maps_users_and_uses_bounded_offset_queries() -> TestResult {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/users"))
        .and(query_param("query", "alice"))
        .and(query_param("type", "username"))
        .and(query_param("limit", "2"))
        .and(query_param("offset", "50"))
        .and(header("authorization", "Bearer synthetic-token"))
        .and(header("device-id", "synthetic-device"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"data":{"users":[{"id":51,"username":"alice","display_name":"Alice"},{"id":"52","username":"@alice2","name":"Alice Two"}]}}"#,
            "application/json",
        ))
        .mount(&server)
        .await;
    let client = test_client(&server)?;
    let (token, device_id) = test_session()?;
    let page_size = Limit::try_from(2)?;

    for input in ["alice", "@alice"] {
        let query = UserSearchQuery::from_str(input)?;
        let page = client
            .search_users(
                &token,
                &device_id,
                &query,
                UserSearchPageRequest::new(page_size, Offset::new(50)),
            )
            .await?;
        let (users, next) = page.into_parts();
        assert_eq!(users.len(), 2);
        assert_eq!(
            users.first().and_then(User::username).map(Username::as_str),
            Some("alice")
        );
        assert_eq!(users.last().and_then(User::display_name), Some("Alice Two"));
        assert_eq!(next.map(Offset::get), Some(52));
    }
    assert_request_count(&server, 2).await;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn user_search_rejects_invalid_records_and_oversized_pages() -> TestResult {
    for body in [
        r#"{"data":[{"id":"not-numeric","username":"alice"}]}"#,
        r#"{"data":[{"id":"1"},{"id":"2"}]}"#,
    ] {
        // Setup.
        let response = scripted_response(200, body.as_bytes().to_vec())?;
        let (token, device_id) = test_session()?;
        let query = UserSearchQuery::from_str("Alice Example")?;
        let page_size = Limit::MIN;

        // Immutable initial script/state.
        let script = [Ok(response)];
        let (client, transport) = scripted_client(script)?;

        // Complete expected observation.
        let expected = ScriptedObservation::expected(
            Err(ApiErrorSnapshot::contract(USER_SEARCH_OPERATION)),
            vec![authenticated_read_request(
                "/users",
                &["users"],
                &[("query", "Alice Example"), ("limit", "1"), ("offset", "0")],
            )],
        );

        // Execute once.
        let result = client
            .search_users(
                &token,
                &device_id,
                &query,
                UserSearchPageRequest::new(page_size, Offset::default()),
            )
            .await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn user_lookup_maps_supported_envelopes_and_exact_id() -> TestResult {
    for body in [
        r#"{"data":{"user":{"id":123,"username":"alice","display_name":"Alice","identity_type":"personal","is_payable":true}}}"#,
        r#"{"data":{"id":"123","username":"@alice","name":"Alice","identity_type":"personal","is_payable":true}}"#,
    ] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/users/123"))
            .and(header("authorization", "Bearer synthetic-token"))
            .and(header("device-id", "synthetic-device"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
            .mount(&server)
            .await;
        let client = test_client(&server)?;
        let (token, device_id) = test_session()?;
        let user_id = UserId::from_str("123")?;
        let user = client.user_by_id(&token, &device_id, &user_id).await?;
        assert_eq!(user.user_id(), &user_id);
        assert_eq!(user.username().map(Username::as_str), Some("alice"));
        assert_eq!(user.display_name(), Some("Alice"));
        assert_eq!(user.profile_kind(), Some(UserProfileKind::Personal));
        assert_eq!(user.is_payable(), Some(true));
        assert_request_count(&server, 1).await;
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn user_lookup_rejects_mismatched_or_invalid_ids() -> TestResult {
    for body in [
        r#"{"data":{"user":{"id":"124","username":"alice"}}}"#,
        r#"{"data":{"id":"not-numeric","username":"alice"}}"#,
    ] {
        // Setup.
        let response = scripted_response(200, body.as_bytes().to_vec())?;
        let (token, device_id) = test_session()?;
        let user_id = UserId::from_str("123")?;

        // Immutable initial script/state.
        let script = [Ok(response)];
        let (client, transport) = scripted_client(script)?;

        // Complete expected observation.
        let expected = ScriptedObservation::expected(
            Err(ApiErrorSnapshot::contract(USER_LOOKUP_OPERATION)),
            vec![authenticated_read_request(
                "/users/{user-id}",
                &["users", "123"],
                &[],
            )],
        );

        // Execute once.
        let result = client.user_by_id(&token, &device_id, &user_id).await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn friends_map_records_and_validate_offset_continuations() -> TestResult {
    // Setup.
    let response = scripted_json_response(
        200,
        serde_json::json!({
            "data": [
                {"id":"40","username":"friend1","display_name":"Friend One"},
                {"id":41,"username":"@friend2","display_name":"Friend Two"}
            ],
            "pagination":{"next":"https://api.venmo.com/v1/users/123/friends?limit=2&offset=4","previous":null}
        }),
    )?;
    let (token, device_id) = test_session()?;
    let user_id = UserId::from_str("123")?;
    let size = Limit::try_from(2)?;

    // Immutable initial script/state.
    let script = [Ok(response)];
    let (client, transport) = scripted_client(script)?;

    // Complete expected observation.
    let expected = ScriptedObservation::expected(
        Ok((
            vec![
                UserSnapshot {
                    user_id: "40".to_owned(),
                    username: Some("friend1".to_owned()),
                    display_name: Some("Friend One".to_owned()),
                    profile_kind: None,
                    is_payable: None,
                },
                UserSnapshot {
                    user_id: "41".to_owned(),
                    username: Some("friend2".to_owned()),
                    display_name: Some("Friend Two".to_owned()),
                    profile_kind: None,
                    is_payable: None,
                },
            ],
            Some(4),
        )),
        vec![authenticated_read_request(
            "/users/{user-id}/friends",
            &["users", "123", "friends"],
            &[("limit", "2"), ("offset", "2")],
        )],
    );

    // Execute once.
    let result = client
        .friends(
            &token,
            &device_id,
            &user_id,
            FriendsPageRequest::new(size, Offset::new(2)),
        )
        .await;
    let observed = ScriptedObservation::observed(
        project_result(result, |page| {
            let (users, next) = page.into_parts();
            (
                users.iter().map(UserSnapshot::from).collect::<Vec<_>>(),
                next.map(Offset::get),
            )
        }),
        &transport,
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn friends_reject_untrusted_continuation_origins() -> TestResult {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/users/123/friends"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [],
            "pagination":{"next":"https://untrusted.example/v1/users/123/friends?limit=1&offset=1"}
        })))
        .mount(&server)
        .await;
    let client = test_client(&server)?;
    let (token, device_id) = test_session()?;
    let user_id = UserId::from_str("123")?;
    let result = client
        .friends(
            &token,
            &device_id,
            &user_id,
            FriendsPageRequest::new(Limit::MIN, Offset::default()),
        )
        .await;
    assert!(matches!(
        result,
        Err(VenmoApiError::Transport(
            TransportError::InvalidContinuationLink
        ))
    ));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn friends_reject_duplicate_or_unexpected_continuation_fields() -> TestResult {
    for next in [
        "https://api.venmo.com/v1/users/123/friends?limit=1&limit=1&offset=1",
        "https://api.venmo.com/v1/users/123/friends?limit=1&offset=1&cursor=opaque",
    ] {
        // Setup.
        let response = scripted_json_response(
            200,
            serde_json::json!({"data": [], "pagination": {"next": next}}),
        )?;
        let (token, device_id) = test_session()?;
        let user_id = UserId::from_str("123")?;

        // Immutable initial script/state.
        let script = [Ok(response)];
        let (client, transport) = scripted_client(script)?;

        // Complete expected observation.
        let expected = ScriptedObservation::expected(
            Err(ApiErrorSnapshot::contract(FRIENDS_OPERATION)),
            vec![authenticated_read_request(
                "/users/{user-id}/friends",
                &["users", "123", "friends"],
                &[("limit", "1"), ("offset", "0")],
            )],
        );

        // Execute once.
        let result = client
            .friends(
                &token,
                &device_id,
                &user_id,
                FriendsPageRequest::new(Limit::MIN, Offset::default()),
            )
            .await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);

        assert_eq!(observed, expected);
    }
    Ok(())
}

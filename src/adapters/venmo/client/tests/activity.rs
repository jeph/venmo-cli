use super::*;

fn current_activity_scope(user_id: &str) -> Result<ActivityFeedScope, Box<dyn Error>> {
    let user_id = UserId::from_str(user_id)?;
    Ok(ActivityFeedScope::new(
        user_id.clone(),
        user_id,
        ActivityFeedKind::CurrentUser,
    ))
}

#[tokio::test(flavor = "current_thread")]
async fn activity_list_and_detail_use_story_ids_and_verified_party_direction() -> TestResult {
    let server = MockServer::start().await;
    let next = format!(
        "{}/v1/stories/target-or-actor/123?before_id=story-2&limit=1&only_public_stories=False&social_only=False",
        server.uri()
    );
    let story = serde_json::json!({
        "id":"story-1","date_created":"2026-07-11T12:00:00","note":"Dinner","audience":"private",
        "payment":{"id":"payment-1","status":"settled","action":"pay","amount":1.25,
            "actor":{"id":"123","username":"alice"},
            "target":{"user":{"id":"456","username":"bob","display_name":"Bob"}},
            "audience":"private","date_created":"2026-07-11T12:00:00"}
    });
    Mock::given(method("GET"))
        .and(path("/v1/stories/target-or-actor/123"))
        .and(query_param("limit", "1"))
        .and(query_param("social_only", "false"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data":[story.clone()],"pagination":{"next":next}
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/stories/story-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data":story})))
        .mount(&server)
        .await;
    let client = test_client(&server)?;
    let (token, device_id) = test_session()?;
    let user_id = UserId::from_str("123")?;
    let scope = current_activity_scope("123")?;
    let activity_id = ActivityId::from_str("story-1")?;

    let page = client
        .activity(
            &token,
            &device_id,
            &scope,
            ActivityPageRequest::new(Limit::MIN, None),
        )
        .await?;
    let (activities, next) = page.into_parts();
    let detail = client
        .activity_by_id(&token, &device_id, &user_id, &activity_id)
        .await?;
    assert_eq!(activities.len(), 1);
    assert_eq!(detail.id(), &activity_id);
    assert_eq!(detail.amount().cents(), 125);
    let (actor, target) = detail
        .parties()
        .payment_parties()
        .ok_or_else(|| io::Error::other("missing absolute payment parties"))?;
    assert_eq!(actor.user_id().as_str(), "123");
    assert_eq!(target.user_id().as_str(), "456");
    assert_eq!(next.as_ref().map(ActivityBeforeId::as_str), Some("story-2"));
    assert_request_count(&server, 2).await;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn activity_detail_rejects_mismatched_story_ids() -> TestResult {
    // Setup.
    let response = scripted_json_response(200, activity_body("story-2"))?;
    let (token, device_id) = test_session()?;
    let user_id = UserId::from_str("123")?;
    let activity_id = ActivityId::from_str("story-1")?;

    // Immutable initial script/state.
    let script = [Ok(response)];
    let (client, transport) = scripted_client(script)?;

    // Complete expected observation.
    let expected = ScriptedObservation::expected(
        Err(ApiErrorSnapshot::contract(ACTIVITY_DETAIL_OPERATION)),
        vec![authenticated_read_request(
            "/stories/{story-id}",
            &["stories", "story-1"],
            &[],
        )],
    );

    // Execute once.
    let result = client
        .activity_by_id(&token, &device_id, &user_id, &activity_id)
        .await;
    let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn activity_rejects_representative_malformed_dto_branches_without_a_socket() -> TestResult {
    for body in [
        serde_json::json!({"data": {}, "pagination": {"next": null}}),
        serde_json::json!({
            "data": [{"id": "story-1", "date_created": "2026-07-11T12:00:00Z"}],
            "pagination": {"next": null}
        }),
        {
            let mut body = activity_body("story-1");
            body["data"]["payment"]["amount"] = Value::String("0.00".to_owned());
            serde_json::json!({"data": [body["data"].clone()], "pagination": {"next": null}})
        },
    ] {
        // Setup.
        let response = scripted_json_response(200, body)?;
        let (token, device_id) = test_session()?;

        // Immutable initial script/state.
        let script = [Ok(response)];
        let (client, transport) = scripted_client(script)?;
        let scope = current_activity_scope("123")?;

        // Complete expected observation.
        let expected = ScriptedObservation::expected(
            Err(ApiErrorSnapshot::contract(ACTIVITY_LIST_OPERATION)),
            vec![authenticated_read_request(
                "/stories/target-or-actor/{user-id}",
                &["stories", "target-or-actor", "123"],
                &[("limit", "1"), ("social_only", "false")],
            )],
        );

        // Execute once.
        let result = client
            .activity(
                &token,
                &device_id,
                &scope,
                ActivityPageRequest::new(Limit::MIN, None),
            )
            .await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn activity_rejects_duplicate_continuation_fields_without_a_socket() -> TestResult {
    for next in [
        "https://api.venmo.com/v1/stories/target-or-actor/123?before_id=story-2&before_id=story-3&limit=1&social_only=false",
        "https://api.venmo.com/v1/stories/target-or-actor/123?before_id=story-2&limit=1&limit=1&social_only=false",
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
        let scope = current_activity_scope("123")?;

        // Complete expected observation.
        let expected = ScriptedObservation::expected(
            Err(ApiErrorSnapshot::contract(ACTIVITY_LIST_OPERATION)),
            vec![authenticated_read_request(
                "/stories/target-or-actor/{user-id}",
                &["stories", "target-or-actor", "123"],
                &[("limit", "1"), ("social_only", "false")],
            )],
        );

        // Execute once.
        let result = client
            .activity(
                &token,
                &device_id,
                &scope,
                ActivityPageRequest::new(Limit::MIN, None),
            )
            .await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn activity_list_maps_external_transfer_records() -> TestResult {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/stories/target-or-actor/123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data":[
                {"id":"story-transfer","date_created":"2026-07-11T12:00:00","note":null,"audience":"private","payment":null,
                 "transfer":{"id":789,"status":"issued","type":"standard","amount":12.34,"date_requested":"2026-07-11T12:00:00",
                    "destination":{"name":"Synthetic bank","type":"bank","last_four":"1234"}}},
                {"id":"story-add-funds","date_created":"2026-07-11T13:00:00","note":null,"audience":"private","payment":null,
                 "transfer":{"id":790,"status":"complete","type":"add_funds","amount":"5.00","date_requested":"2026-07-11T13:00:00",
                    "source":{"name":"Synthetic source","type":"bank","last_four":5678}}},
                {"id":"story-authorization","date_created":"2026-07-11T14:00:00","note":"Synthetic purchase","audience":"private",
                 "payment":null,"transfer":null,
                 "authorization":{"id":"authorization-1","status":"captured","amount":"2.50","created_at":"2026-07-11T14:00:00",
                    "descriptor":"Synthetic descriptor","merchant":{"display_name":"Synthetic merchant"},
                    "user":{"id":"123","username":"alice"}}}
            ],
            "pagination":{"next":null}
        })))
        .mount(&server)
        .await;
    let client = test_client(&server)?;
    let (token, device_id) = test_session()?;
    let scope = current_activity_scope("123")?;
    let page_size = Limit::try_from(3)?;
    let page = client
        .activity(
            &token,
            &device_id,
            &scope,
            ActivityPageRequest::new(page_size, None),
        )
        .await?;
    let (activities, next) = page.into_parts();
    let activity = activities
        .first()
        .ok_or_else(|| io::Error::other("missing mapped transfer activity"))?;
    let add_funds = activities
        .get(1)
        .ok_or_else(|| io::Error::other("missing mapped add-funds activity"))?;
    let authorization = activities
        .last()
        .ok_or_else(|| io::Error::other("missing mapped authorization activity"))?;
    assert!(next.is_none());
    assert_eq!(activities.len(), 3);
    assert_eq!(activity.action().as_str(), "transfer:standard");
    assert_eq!(activity.status().as_str(), "issued");
    assert_eq!(activity.direction(), ActivityDirection::Outgoing);
    assert_eq!(activity.amount().cents(), 1_234);
    assert_eq!(
        activity.counterparty().external_parts(),
        Some(("Synthetic bank", "bank", Some("1234")))
    );
    assert_eq!(add_funds.action().as_str(), "transfer:add_funds");
    assert_eq!(add_funds.direction(), ActivityDirection::Incoming);
    assert_eq!(
        add_funds.counterparty().external_parts(),
        Some(("Synthetic source", "bank", Some("5678")))
    );
    assert_eq!(authorization.action().as_str(), "authorization");
    assert_eq!(authorization.status().as_str(), "captured");
    assert_eq!(authorization.direction(), ActivityDirection::Outgoing);
    assert_eq!(
        authorization.counterparty().external_parts(),
        Some(("Synthetic merchant", "merchant", None))
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn other_user_activity_uses_native_query_and_subject_relative_direction() -> TestResult {
    let next = "https://api.venmo.com/v1/stories/target-or-actor/456?before_id=story-next&limit=1";
    let body = serde_json::json!({
        "data": [{
            "id":"story-1","date_created":"2026-07-11T12:00:00","audience":"public",
            "payment":{"id":"payment-1","status":"settled","action":"pay","amount":"1.25",
                "actor":{"id":"456","username":"alice"},
                "target":{"user":{"id":"789","username":"bob"}},
                "audience":"public","date_created":"2026-07-11T12:00:00"}
        }],
        "pagination":{"next":next}
    });
    let response = scripted_json_response(200, body)?;
    let (client, transport) = scripted_client([Ok(response)])?;
    let (token, device_id) = test_session()?;
    let scope = ActivityFeedScope::new(
        UserId::from_str("123")?,
        UserId::from_str("456")?,
        ActivityFeedKind::OtherPersonalUser,
    );

    let result = client
        .activity(
            &token,
            &device_id,
            &scope,
            ActivityPageRequest::new(Limit::MIN, None),
        )
        .await;
    let observed = ScriptedObservation::observed(
        project_result(result, |page| {
            let (activities, next) = page.into_parts();
            let activity = &activities[0];
            (
                activity.direction(),
                activity
                    .counterparty()
                    .as_user()
                    .map(|user| user.user_id().as_str().to_owned()),
                next.map(|value| value.as_str().to_owned()),
            )
        }),
        &transport,
    );
    let expected = ScriptedObservation::expected(
        Ok((
            ActivityDirection::Outgoing,
            Some("789".to_owned()),
            Some("story-next".to_owned()),
        )),
        vec![authenticated_read_request(
            "/stories/target-or-actor/{user-id}",
            &["stories", "target-or-actor", "456"],
            &[("limit", "1")],
        )],
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn other_user_activity_enforces_visible_payment_privacy_and_record_type() -> TestResult {
    let (token, device_id) = test_session()?;
    let scope = ActivityFeedScope::new(
        UserId::from_str("123")?,
        UserId::from_str("456")?,
        ActivityFeedKind::OtherPersonalUser,
    );
    let private_between_others = serde_json::json!({
        "data": [{
            "id":"story-1","date_created":"2026-07-11T12:00:00","audience":"private",
            "payment":{"id":"payment-1","status":"settled","action":"pay","amount":"1.25",
                "actor":{"id":"456","username":"alice"},
                "target":{"user":{"id":"789","username":"bob"}},
                "audience":"private","date_created":"2026-07-11T12:00:00"}
        }],"pagination":{"next":null}
    });
    let nonpayment = serde_json::json!({
        "data": [{
            "id":"story-transfer","date_created":"2026-07-11T12:00:00","audience":"private",
            "transfer":{"status":"issued","type":"standard","amount":"1.25",
                "destination":{"name":"Bank","type":"bank","last_four":"1234"}}
        }],"pagination":{"next":null}
    });

    for body in [private_between_others, nonpayment] {
        let response = scripted_json_response(200, body)?;
        let (client, transport) = scripted_client([Ok(response)])?;
        let result = client
            .activity(
                &token,
                &device_id,
                &scope,
                ActivityPageRequest::new(Limit::MIN, None),
            )
            .await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);
        let expected = ScriptedObservation::expected(
            Err(ApiErrorSnapshot::contract(ACTIVITY_LIST_OPERATION)),
            vec![authenticated_read_request(
                "/stories/target-or-actor/{user-id}",
                &["stories", "target-or-actor", "456"],
                &[("limit", "1")],
            )],
        );
        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn other_user_activity_accepts_private_story_only_when_viewer_is_other_party() -> TestResult {
    let body = serde_json::json!({
        "data": [{
            "id":"story-1","date_created":"2026-07-11T12:00:00","audience":"private",
            "payment":{"id":"payment-1","status":"settled","action":"pay","amount":"1.25",
                "actor":{"id":"456","username":"alice"},
                "target":{"user":{"id":"123","username":"owner"}},
                "audience":"private","date_created":"2026-07-11T12:00:00"}
        }],"pagination":{"next":null}
    });
    let response = scripted_json_response(200, body)?;
    let (client, transport) = scripted_client([Ok(response)])?;
    let (token, device_id) = test_session()?;
    let scope = ActivityFeedScope::new(
        UserId::from_str("123")?,
        UserId::from_str("456")?,
        ActivityFeedKind::OtherPersonalUser,
    );

    let result = client
        .activity(
            &token,
            &device_id,
            &scope,
            ActivityPageRequest::new(Limit::MIN, None),
        )
        .await;
    let observed = ScriptedObservation::observed(
        project_result(result, |page| {
            let (activities, _) = page.into_parts();
            activities[0].direction()
        }),
        &transport,
    );
    let expected = ScriptedObservation::expected(
        Ok(ActivityDirection::Outgoing),
        vec![authenticated_read_request(
            "/stories/target-or-actor/{user-id}",
            &["stories", "target-or-actor", "456"],
            &[("limit", "1")],
        )],
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn unique_public_activity_detail_preserves_absolute_external_parties() -> TestResult {
    let body = serde_json::json!({"data": {
        "id":"story-1","date_created":"2026-07-11T12:00:00","audience":"friends",
        "payment":{"id":"payment-1","status":"settled","action":"pay","amount":"1.25",
            "actor":{"id":"456","username":"alice"},
            "target":{"user":{"id":"789","username":"bob"}},
            "audience":"friends","date_created":"2026-07-11T12:00:00"}
    }});
    let response = scripted_json_response(200, body)?;
    let (client, transport) = scripted_client([Ok(response)])?;
    let (token, device_id) = test_session()?;
    let current_user_id = UserId::from_str("123")?;
    let activity_id = ActivityId::from_str("story-1")?;

    let result = client
        .activity_by_id(&token, &device_id, &current_user_id, &activity_id)
        .await;
    let observed = ScriptedObservation::observed(
        project_result(result, |detail| {
            detail.parties().payment_parties().map(|(actor, target)| {
                (
                    actor.user_id().as_str().to_owned(),
                    target.user_id().as_str().to_owned(),
                )
            })
        }),
        &transport,
    );
    let expected = ScriptedObservation::expected(
        Ok(Some(("456".to_owned(), "789".to_owned()))),
        vec![authenticated_read_request(
            "/stories/{story-id}",
            &["stories", "story-1"],
            &[],
        )],
    );

    assert_eq!(observed, expected);
    Ok(())
}

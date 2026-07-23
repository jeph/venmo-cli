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
    assert_eq!(detail.amount().map(Money::cents), Some(125));
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
        {
            let mut body = activity_body("story-1");
            body["data"]["payment"]["amount"] = Value::Null;
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
    assert_eq!(
        activity.status().map(|status| status.as_str()),
        Some("issued")
    );
    assert_eq!(activity.direction(), ActivityDirection::Outgoing);
    assert_eq!(activity.amount().map(Money::cents), Some(1_234));
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
    assert_eq!(
        authorization.status().map(|status| status.as_str()),
        Some("captured")
    );
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
            "payment":{"id":"payment-1","status":"settled","action":"pay","amount":null,
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
                activity.amount(),
                next.map(|value| value.as_str().to_owned()),
            )
        }),
        &transport,
    );
    let expected = ScriptedObservation::expected(
        Ok((
            ActivityDirection::Outgoing,
            Some("789".to_owned()),
            None,
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
            (activities[0].direction(), activities[0].amount())
        }),
        &transport,
    );
    let expected = ScriptedObservation::expected(
        Ok((ActivityDirection::Outgoing, None)),
        vec![authenticated_read_request(
            "/stories/target-or-actor/{user-id}",
            &["stories", "target-or-actor", "456"],
            &[("limit", "1")],
        )],
    );

    assert_eq!(observed, expected);
    Ok(())
}

fn disbursement_story(owner_id: &str, audience: &str, rewards_earned: bool) -> Value {
    serde_json::json!({
        "id": "story-disbursement",
        "type": "disbursement",
        "date_created": "2026-07-11T12:00:00",
        "note": "Synthetic reward",
        "audience": audience,
        "payment": null,
        "transfer": null,
        "authorization": null,
        "disbursement": {
            "id": "disbursement-1",
            "date_created": "2026-07-11T12:00:00",
            "merchant": {"display_name": "Synthetic merchant"},
            "user": {"id": owner_id, "username": "alice"},
            "rewards_earned": rewards_earned,
            "rewards_partner_label": null,
            "type": null,
            "metadata": {}
        }
    })
}

#[tokio::test(flavor = "current_thread")]
async fn other_user_activity_maps_visible_disbursement_without_amount_or_status() -> TestResult {
    let next = "https://api.venmo.com/v1/stories/target-or-actor/456?before_id=story-next&limit=1";
    let response = scripted_json_response(
        200,
        serde_json::json!({
            "data": [disbursement_story("456", "friends", false)],
            "pagination": {"next": next}
        }),
    )?;
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
                activity.id().as_str().to_owned(),
                activity.action().as_str().to_owned(),
                activity.direction(),
                activity
                    .counterparty()
                    .external_parts()
                    .map(|(name, kind, last_four)| {
                        (
                            name.to_owned(),
                            kind.to_owned(),
                            last_four.map(str::to_owned),
                        )
                    }),
                activity.amount(),
                activity.status().map(|status| status.as_str().to_owned()),
                activity.note().map(str::to_owned),
                activity.audience().map(str::to_owned),
                next.map(|value| value.as_str().to_owned()),
            )
        }),
        &transport,
    );
    let expected = ScriptedObservation::expected(
        Ok((
            "story-disbursement".to_owned(),
            "disbursement".to_owned(),
            ActivityDirection::Incoming,
            Some(("Synthetic merchant".to_owned(), "merchant".to_owned(), None)),
            None,
            None,
            Some("Synthetic reward".to_owned()),
            Some("friends".to_owned()),
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
async fn activity_detail_maps_disbursement_with_absolute_owner() -> TestResult {
    let response = scripted_json_response(
        200,
        serde_json::json!({"data": disbursement_story("456", "public", true)}),
    )?;
    let (client, transport) = scripted_client([Ok(response)])?;
    let (token, device_id) = test_session()?;
    let current_user_id = UserId::from_str("123")?;
    let activity_id = ActivityId::from_str("story-disbursement")?;

    let result = client
        .activity_by_id(&token, &device_id, &current_user_id, &activity_id)
        .await;
    let observed = ScriptedObservation::observed(
        project_result(result, |detail| {
            (
                detail
                    .parties()
                    .account_parts()
                    .map(|(account, direction, counterparty)| {
                        (
                            account.user_id().as_str().to_owned(),
                            direction,
                            counterparty
                                .external_parts()
                                .map(|(name, kind, _)| (name.to_owned(), kind.to_owned())),
                        )
                    }),
                detail.amount(),
                detail.status().map(|status| status.as_str().to_owned()),
            )
        }),
        &transport,
    );
    let expected = ScriptedObservation::expected(
        Ok((
            Some((
                "456".to_owned(),
                ActivityDirection::Incoming,
                Some(("Synthetic merchant".to_owned(), "merchant".to_owned())),
            )),
            None,
            None,
        )),
        vec![authenticated_read_request(
            "/stories/{story-id}",
            &["stories", "story-disbursement"],
            &[],
        )],
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn disbursement_activity_rejects_unproven_or_ambiguous_shapes() -> TestResult {
    let (token, device_id) = test_session()?;
    let scope = ActivityFeedScope::new(
        UserId::from_str("123")?,
        UserId::from_str("456")?,
        ActivityFeedKind::OtherPersonalUser,
    );
    let wrong_owner = disbursement_story("789", "friends", true);
    let private = disbursement_story("456", "private", true);
    let mut wrong_type = disbursement_story("456", "friends", true);
    wrong_type["type"] = Value::String("reward".to_owned());
    let mut ambiguous = disbursement_story("456", "friends", true);
    ambiguous["payment"] = serde_json::json!({
        "id": "payment-1",
        "status": "settled",
        "action": "pay",
        "amount": null,
        "actor": {"id": "456", "username": "alice"},
        "target": {"user": {"id": "789", "username": "bob"}},
        "audience": "friends",
        "date_created": "2026-07-11T12:00:00"
    });

    for story in [wrong_owner, private, wrong_type, ambiguous] {
        let response = scripted_json_response(
            200,
            serde_json::json!({"data": [story], "pagination": {"next": null}}),
        )?;
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
            (
                detail.parties().payment_parties().map(|(actor, target)| {
                    (
                        actor.user_id().as_str().to_owned(),
                        target.user_id().as_str().to_owned(),
                    )
                }),
                detail.amount(),
            )
        }),
        &transport,
    );
    let expected = ScriptedObservation::expected(
        Ok((Some(("456".to_owned(), "789".to_owned())), None)),
        vec![authenticated_read_request(
            "/stories/{story-id}",
            &["stories", "story-1"],
            &[],
        )],
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn participant_activity_detail_requires_an_amount() -> TestResult {
    let body = serde_json::json!({"data": {
        "id":"story-1","date_created":"2026-07-11T12:00:00","audience":"private",
        "payment":{"id":"payment-1","status":"settled","action":"pay","amount":null,
            "actor":{"id":"123","username":"owner"},
            "target":{"user":{"id":"789","username":"bob"}},
            "audience":"private","date_created":"2026-07-11T12:00:00"}
    }});
    let response = scripted_json_response(200, body)?;
    let (client, transport) = scripted_client([Ok(response)])?;
    let (token, device_id) = test_session()?;
    let current_user_id = UserId::from_str("123")?;
    let activity_id = ActivityId::from_str("story-1")?;

    let result = client
        .activity_by_id(&token, &device_id, &current_user_id, &activity_id)
        .await;
    let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);
    let expected = ScriptedObservation::expected(
        Err(ApiErrorSnapshot::contract(ACTIVITY_DETAIL_OPERATION)),
        vec![authenticated_read_request(
            "/stories/{story-id}",
            &["stories", "story-1"],
            &[],
        )],
    );

    assert_eq!(observed, expected);
    Ok(())
}

fn social_activity_body(liked_by_owner: bool, include_comment: bool) -> Value {
    let mut body = activity_body("story-1");
    body["data"]["likes"] = if liked_by_owner {
        serde_json::json!({
            "count": 1,
            "data": [{"id":"123","username":"alice"}],
            "pagination": {"next": null}
        })
    } else {
        serde_json::json!({"count":0,"data":[],"pagination":{"next":null}})
    };
    body["data"]["comments"] = if include_comment {
        serde_json::json!({
            "count": 1,
            "data": [{
                "id":"comment-1",
                "user":{"id":"123","username":"alice"},
                "message":"Synthetic comment",
                "date_created":"2026-07-11T12:01:00Z"
            }],
            "pagination":{"next":null}
        })
    } else {
        serde_json::json!({"count":0,"data":[],"pagination":{"next":null}})
    };
    body
}

#[tokio::test(flavor = "current_thread")]
async fn activity_detail_maps_embedded_likes_and_comments_with_completeness() -> TestResult {
    let mut body = social_activity_body(true, true);
    body["data"]["likes"]["pagination"] = Value::Null;
    body["data"]["comments"]["pagination"] = Value::Null;
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
            detail
                .social()
                .likes()
                .zip(detail.social().comments())
                .map(|(likes, comments)| {
                    (
                        likes.count(),
                        likes.items()[0].user_id().as_str().to_owned(),
                        likes.is_complete(),
                        comments.count(),
                        comments.items()[0].id().as_str().to_owned(),
                        comments.items()[0].message().to_owned(),
                        comments.is_complete(),
                    )
                })
        }),
        &transport,
    );
    let expected = ScriptedObservation::expected(
        Ok(Some((
            1,
            "123".to_owned(),
            true,
            1,
            "comment-1".to_owned(),
            "Synthetic comment".to_owned(),
            true,
        ))),
        vec![authenticated_read_request(
            "/stories/{story-id}",
            &["stories", "story-1"],
            &[],
        )],
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn activity_detail_rejects_inconsistent_or_duplicate_embedded_social_data() -> TestResult {
    let duplicate_liker = serde_json::json!({
        "count":2,
        "data":[{"id":"123","username":"alice"},{"id":"123","username":"alice"}],
        "pagination":{"next":null}
    });
    let duplicate_comment = serde_json::json!({
        "count":2,
        "data":[
            {"id":"comment-1","user":{"id":"123"},"message":"one","date_created":"2026-07-11T12:01:00Z"},
            {"id":"comment-1","user":{"id":"123"},"message":"two","date_created":"2026-07-11T12:02:00Z"}
        ],
        "pagination":{"next":null}
    });
    for (field, collection) in [
        (
            "likes",
            serde_json::json!({
                "count":0,"data":[{"id":"123","username":"alice"}],"pagination":{"next":null}
            }),
        ),
        ("likes", duplicate_liker),
        (
            "comments",
            serde_json::json!({
                "count":0,
                "data":[{"id":"comment-1","user":{"id":"123"},"message":"one","date_created":"2026-07-11T12:01:00Z"}],
                "pagination":{"next":null}
            }),
        ),
        ("comments", duplicate_comment),
    ] {
        let mut body = activity_body("story-1");
        body["data"][field] = collection;
        let response = scripted_json_response(200, body)?;
        let (client, transport) = scripted_client([Ok(response)])?;
        let (token, device_id) = test_session()?;
        let current_user_id = UserId::from_str("123")?;
        let activity_id = ActivityId::from_str("story-1")?;

        let result = client
            .activity_by_id(&token, &device_id, &current_user_id, &activity_id)
            .await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);
        let expected = ScriptedObservation::expected(
            Err(ApiErrorSnapshot::contract(ACTIVITY_DETAIL_OPERATION)),
            vec![authenticated_read_request(
                "/stories/{story-id}",
                &["stories", "story-1"],
                &[],
            )],
        );

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn activity_detail_maps_aggregate_reactions_and_current_user_state() -> TestResult {
    let mut body = activity_body("story-1");
    body["data"]["reactions"] = serde_json::json!([
        {"emoji":"🔥","count":2,"reacted_by_user":true},
        {"emoji":":red_heart:","count":1,"reacted_by_user":false}
    ]);
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
            detail.social().reactions().map(|reactions| {
                (
                    reactions.total_count(),
                    reactions
                        .items()
                        .iter()
                        .map(|reaction| {
                            (
                                reaction.emoji().as_str().to_owned(),
                                reaction.count(),
                                reaction.reacted_by_current_user(),
                            )
                        })
                        .collect::<Vec<_>>(),
                )
            })
        }),
        &transport,
    );
    let expected = ScriptedObservation::expected(
        Ok(Some((
            3,
            vec![("🔥".to_owned(), 2, true), ("❤️".to_owned(), 1, false)],
        ))),
        vec![authenticated_read_request(
            "/stories/{story-id}",
            &["stories", "story-1"],
            &[],
        )],
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn activity_detail_rejects_invalid_or_duplicate_reaction_aggregates() -> TestResult {
    for reactions in [
        serde_json::json!([
            {"emoji":"🔥","count":1},
            {"emoji":"🔥","count":1}
        ]),
        serde_json::json!([{"emoji":"🔥","count":0,"reacted_by_user":true}]),
        serde_json::json!([{"emoji":"🔥❤️","count":1}]),
    ] {
        let mut body = activity_body("story-1");
        body["data"]["reactions"] = reactions;
        let response = scripted_json_response(200, body)?;
        let (client, transport) = scripted_client([Ok(response)])?;
        let (token, device_id) = test_session()?;
        let current_user_id = UserId::from_str("123")?;
        let activity_id = ActivityId::from_str("story-1")?;

        let result = client
            .activity_by_id(&token, &device_id, &current_user_id, &activity_id)
            .await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);
        let expected = ScriptedObservation::expected(
            Err(ApiErrorSnapshot::contract(ACTIVITY_DETAIL_OPERATION)),
            vec![authenticated_read_request(
                "/stories/{story-id}",
                &["stories", "story-1"],
                &[],
            )],
        );

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn activity_like_and_unlike_use_exact_bodyless_state_routes_then_reconcile() -> TestResult {
    for (like, response_body, method, operation, expected_likes) in [
        (
            true,
            social_activity_body(true, false),
            Method::POST,
            ACTIVITY_LIKE_OPERATION,
            1,
        ),
        (
            false,
            social_activity_body(false, false),
            Method::DELETE,
            ACTIVITY_UNLIKE_OPERATION,
            0,
        ),
    ] {
        let mutation = scripted_response(204, Vec::new())?;
        let refresh = scripted_json_response(200, response_body)?;
        let (client, transport) = scripted_client([Ok(mutation), Ok(refresh)])?;
        let (token, device_id) = test_session()?;
        let current_user_id = UserId::from_str("123")?;
        let activity_id = ActivityId::from_str("story-1")?;

        let result = if like {
            client
                .like_activity(&token, &device_id, &current_user_id, &activity_id)
                .await
        } else {
            client
                .unlike_activity(&token, &device_id, &current_user_id, &activity_id)
                .await
        };
        let observed = ScriptedObservation::observed(
            project_result(result, |detail| {
                detail.social().likes().map(|likes| likes.count())
            }),
            &transport,
        );
        let expected = ScriptedObservation::expected(
            Ok(Some(expected_likes)),
            vec![
                authenticated_request(
                    method,
                    "/stories/{story-id}/likes",
                    &["stories", "story-1", "likes"],
                    &[],
                    None,
                    OperationClass::StateWrite,
                ),
                authenticated_read_request("/stories/{story-id}", &["stories", "story-1"], &[]),
            ],
        );

        assert_eq!(observed, expected, "{operation}");
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn activity_reaction_add_and_remove_use_exact_json_state_routes_then_reconcile() -> TestResult
{
    for (add, method, operation, reacted, count) in [
        (true, Method::POST, ACTIVITY_REACTION_ADD_OPERATION, true, 2),
        (
            false,
            Method::DELETE,
            ACTIVITY_REACTION_REMOVE_OPERATION,
            false,
            1,
        ),
    ] {
        let mutation = scripted_response(204, Vec::new())?;
        let mut body = activity_body("story-1");
        body["data"]["reactions"] = serde_json::json!([
            {"emoji":"🔥","count":count,"reacted_by_user":reacted}
        ]);
        let refresh = scripted_json_response(200, body)?;
        let (client, transport) = scripted_client([Ok(mutation), Ok(refresh)])?;
        let (token, device_id) = test_session()?;
        let current_user_id = UserId::from_str("123")?;
        let activity_id = ActivityId::from_str("story-1")?;
        let emoji = ActivityReactionEmoji::from_str("🔥")?;

        let result = if add {
            client
                .add_activity_reaction(&token, &device_id, &current_user_id, &activity_id, &emoji)
                .await
        } else {
            client
                .remove_activity_reaction(
                    &token,
                    &device_id,
                    &current_user_id,
                    &activity_id,
                    &emoji,
                )
                .await
        };
        let observed = ScriptedObservation::observed(
            project_result(result, |detail| detail.social().reaction_state(&emoji)),
            &transport,
        );
        let expected_state = if add {
            ActivityReactionState::Present
        } else {
            ActivityReactionState::Absent
        };
        let expected = ScriptedObservation::expected(
            Ok(expected_state),
            vec![
                authenticated_request(
                    method,
                    "/stories/{story-id}/reactions",
                    &["stories", "story-1", "reactions"],
                    &[],
                    Some(r#"{"emoji":"🔥"}"#.as_bytes()),
                    OperationClass::StateWrite,
                ),
                authenticated_read_request("/stories/{story-id}", &["stories", "story-1"], &[]),
            ],
        );

        assert_eq!(observed, expected, "{operation}");
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn activity_red_heart_reaction_uses_the_native_wire_alias() -> TestResult {
    let mutation = scripted_response(204, Vec::new())?;
    let mut body = activity_body("story-1");
    body["data"]["reactions"] = serde_json::json!([
        {"emoji":":red_heart:","count":1,"reacted_by_user":true}
    ]);
    let refresh = scripted_json_response(200, body)?;
    let (client, transport) = scripted_client([Ok(mutation), Ok(refresh)])?;
    let (token, device_id) = test_session()?;
    let current_user_id = UserId::from_str("123")?;
    let activity_id = ActivityId::from_str("story-1")?;
    let emoji = ActivityReactionEmoji::from_str("❤")?;

    let result = client
        .add_activity_reaction(&token, &device_id, &current_user_id, &activity_id, &emoji)
        .await;
    let observed = ScriptedObservation::observed(
        project_result(result, |detail| detail.social().reaction_state(&emoji)),
        &transport,
    );
    let expected = ScriptedObservation::expected(
        Ok(ActivityReactionState::Present),
        vec![
            authenticated_request(
                Method::POST,
                "/stories/{story-id}/reactions",
                &["stories", "story-1", "reactions"],
                &[],
                Some(r#"{"emoji":":red_heart:"}"#.as_bytes()),
                OperationClass::StateWrite,
            ),
            authenticated_read_request("/stories/{story-id}", &["stories", "story-1"], &[]),
        ],
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn activity_comment_add_uses_exact_json_and_verifies_created_comment() -> TestResult {
    let created = scripted_json_response(
        200,
        serde_json::json!({"data":{
            "id":"comment-1",
            "user":{"id":"123","username":"alice"},
            "message":"Synthetic comment",
            "date_created":"2026-07-11T12:01:00Z"
        }}),
    )?;
    let refresh = scripted_json_response(200, social_activity_body(false, true))?;
    let (client, transport) = scripted_client([Ok(created), Ok(refresh)])?;
    let (token, device_id) = test_session()?;
    let current_user_id = UserId::from_str("123")?;
    let activity_id = ActivityId::from_str("story-1")?;
    let message = ActivityCommentMessage::from_str("Synthetic comment")?;

    let result = client
        .add_activity_comment(&token, &device_id, &current_user_id, &activity_id, &message)
        .await;
    let observed = ScriptedObservation::observed(
        project_result(result, |detail| {
            detail.social().comments().map(|comments| comments.count())
        }),
        &transport,
    );
    let expected = ScriptedObservation::expected(
        Ok(Some(1)),
        vec![
            authenticated_request(
                Method::POST,
                "/stories/{story-id}/comments",
                &["stories", "story-1", "comments"],
                &[],
                Some(br#"{"message":"Synthetic comment"}"#),
                OperationClass::StateWrite,
            ),
            authenticated_read_request("/stories/{story-id}", &["stories", "story-1"], &[]),
        ],
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn activity_comment_remove_uses_only_the_comment_id_route() -> TestResult {
    let mutation = scripted_response(204, Vec::new())?;
    let (client, transport) = scripted_client([Ok(mutation)])?;
    let (token, device_id) = test_session()?;
    let comment_id = ActivityCommentId::from_str("comment-1")?;

    let result = client
        .remove_activity_comment(&token, &device_id, &comment_id)
        .await;
    let observed = ScriptedObservation::observed(result, &transport);
    let expected = ScriptedObservation::expected(
        Ok(()),
        vec![authenticated_request(
            Method::DELETE,
            "/comments/{comment-id}",
            &["comments", "comment-1"],
            &[],
            None,
            OperationClass::StateWrite,
        )],
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn activity_comment_remove_explains_that_the_comment_could_not_be_found() -> TestResult {
    let response = scripted_response(404, Vec::new())?;
    let (client, transport) = scripted_client([Ok(response)])?;
    let (token, device_id) = test_session()?;
    let comment_id = ActivityCommentId::from_str("comment-1")?;

    let result = client
        .remove_activity_comment(&token, &device_id, &comment_id)
        .await;
    let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);
    let expected = ScriptedObservation::expected(
        Err(ApiErrorSnapshot {
            kind: ApiFailureKind::Rejected,
            detail: ApiErrorDetail::ActivityCommentNotFound {
                rendered: "Comment not found.".to_owned(),
            },
        }),
        vec![authenticated_request(
            Method::DELETE,
            "/comments/{comment-id}",
            &["comments", "comment-1"],
            &[],
            None,
            OperationClass::StateWrite,
        )],
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn social_mutation_reconciliation_mismatches_are_ambiguous() -> TestResult {
    let mutation = scripted_response(204, Vec::new())?;
    let refresh = scripted_json_response(200, social_activity_body(false, false))?;
    let (client, transport) = scripted_client([Ok(mutation), Ok(refresh)])?;
    let (token, device_id) = test_session()?;
    let current_user_id = UserId::from_str("123")?;
    let activity_id = ActivityId::from_str("story-1")?;

    let result = client
        .like_activity(&token, &device_id, &current_user_id, &activity_id)
        .await;
    let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);
    let expected = ScriptedObservation::expected(
        Err(ApiErrorSnapshot {
            kind: ApiFailureKind::AmbiguousWrite,
            detail: ApiErrorDetail::StateMutationOutcomeUnknown {
                operation: ACTIVITY_LIKE_OPERATION,
            },
        }),
        vec![
            authenticated_request(
                Method::POST,
                "/stories/{story-id}/likes",
                &["stories", "story-1", "likes"],
                &[],
                None,
                OperationClass::StateWrite,
            ),
            authenticated_read_request("/stories/{story-id}", &["stories", "story-1"], &[]),
        ],
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn reaction_mutation_reconciliation_mismatches_are_ambiguous() -> TestResult {
    let mutation = scripted_response(204, Vec::new())?;
    let mut body = activity_body("story-1");
    body["data"]["reactions"] = serde_json::json!([]);
    let refresh = scripted_json_response(200, body)?;
    let (client, transport) = scripted_client([Ok(mutation), Ok(refresh)])?;
    let (token, device_id) = test_session()?;
    let current_user_id = UserId::from_str("123")?;
    let activity_id = ActivityId::from_str("story-1")?;
    let emoji = ActivityReactionEmoji::from_str("🔥")?;

    let result = client
        .add_activity_reaction(&token, &device_id, &current_user_id, &activity_id, &emoji)
        .await;
    let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);
    let expected = ScriptedObservation::expected(
        Err(ApiErrorSnapshot {
            kind: ApiFailureKind::AmbiguousWrite,
            detail: ApiErrorDetail::StateMutationOutcomeUnknown {
                operation: ACTIVITY_REACTION_ADD_OPERATION,
            },
        }),
        vec![
            authenticated_request(
                Method::POST,
                "/stories/{story-id}/reactions",
                &["stories", "story-1", "reactions"],
                &[],
                Some(r#"{"emoji":"🔥"}"#.as_bytes()),
                OperationClass::StateWrite,
            ),
            authenticated_read_request("/stories/{story-id}", &["stories", "story-1"], &[]),
        ],
    );

    assert_eq!(observed, expected);
    Ok(())
}

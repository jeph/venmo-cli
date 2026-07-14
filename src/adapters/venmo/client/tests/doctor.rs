use super::*;
use crate::features::doctor::{RequiredShape, ShapeProbeOutcome};

#[tokio::test(flavor = "current_thread")]
async fn doctor_connectivity_probe_is_read_only_and_unauthenticated() -> TestResult {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/account"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let client = test_client(&server)?;
    client.connectivity().await?;
    let requests = server.received_requests().await;
    assert!(requests.as_ref().is_some_and(|requests| {
        requests.len() == 1
            && requests[0].headers.get("authorization").is_none()
            && requests[0].headers.get("device-id").is_none()
    }));
    Ok(())
}

fn valid_required_shape_bodies() -> [&'static str; 5] {
    [
        r#"{"data":{"balance":"0.00","balance_on_hold":"0.00"}}"#,
        r#"{"data":[]}"#,
        r#"{"data":[],"pagination":{"next":null}}"#,
        r#"{"data":[],"pagination":{"next":null}}"#,
        r#"{"data":[],"pagination":{"next":null}}"#,
    ]
}

fn required_shape_responses(
    malformed_index: Option<usize>,
) -> Result<Vec<Result<HttpResponse, TransportError>>, Box<dyn Error>> {
    valid_required_shape_bodies()
        .into_iter()
        .enumerate()
        .map(|(index, body)| {
            scripted_response(
                200,
                if malformed_index == Some(index) {
                    b"not-json".to_vec()
                } else {
                    body.as_bytes().to_vec()
                },
            )
            .map(Ok)
        })
        .collect()
}

fn required_shape_requests() -> Vec<ScriptedRequest> {
    vec![
        authenticated_read_request("/account", &["account"], &[]),
        authenticated_read_request("/payment-methods", &["payment-methods"], &[]),
        authenticated_read_request(
            "/users/{user-id}/friends",
            &["users", "123", "friends"],
            &[("limit", "1"), ("offset", "0")],
        ),
        authenticated_read_request(
            "/stories/target-or-actor/{user-id}",
            &["stories", "target-or-actor", "123"],
            &[("limit", "1"), ("social_only", "false")],
        ),
        authenticated_read_request(
            "/payments",
            &["payments"],
            &[
                ("action", "charge"),
                ("status", "pending,held"),
                ("limit", "1"),
            ],
        ),
    ]
}

#[tokio::test(flavor = "current_thread")]
async fn doctor_required_shapes_probe_every_exact_read_in_stable_order() -> TestResult {
    // Setup.
    let (token, device_id) = test_session()?;
    let current_user_id = UserId::from_str("123")?;

    // Immutable initial script/state.
    let script = required_shape_responses(None)?;
    let (client, transport) = scripted_client(script)?;

    // Complete expected observation.
    let expected = ScriptedObservation::expected(
        RequiredShape::ALL
            .map(ShapeProbeOutcome::passed)
            .into_iter()
            .collect::<Vec<_>>(),
        required_shape_requests(),
    );

    // Execute once.
    let outcomes = client
        .required_shapes(&token, &device_id, &current_user_id)
        .await;
    let observed = ScriptedObservation::observed(outcomes, &transport);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn each_malformed_required_shape_is_reported_and_does_not_skip_later_probes() -> TestResult {
    // Setup.
    let (token, device_id) = test_session()?;
    let current_user_id = UserId::from_str("123")?;
    for malformed_index in 0..RequiredShape::ALL.len() {
        // Immutable initial script/state.
        let script = required_shape_responses(Some(malformed_index))?;
        let (client, transport) = scripted_client(script)?;

        // Complete expected observation.
        let expected_outcomes = RequiredShape::ALL
            .into_iter()
            .enumerate()
            .map(|(index, shape)| {
                if index == malformed_index {
                    ShapeProbeOutcome::failed(shape, ApiFailureKind::Contract)
                } else {
                    ShapeProbeOutcome::passed(shape)
                }
            })
            .collect::<Vec<_>>();
        let expected = ScriptedObservation::expected(expected_outcomes, required_shape_requests());

        // Execute once.
        let outcomes = client
            .required_shapes(&token, &device_id, &current_user_id)
            .await;
        let observed = ScriptedObservation::observed(outcomes, &transport);

        assert_eq!(observed, expected);
    }
    Ok(())
}

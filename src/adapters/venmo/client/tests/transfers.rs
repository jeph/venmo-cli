use super::*;
use time::format_description::well_known::Rfc3339;

const TRANSFER_CREATION_REQUEST_BODY: &str = concat!(
    r#"{"amount":1234,"destination_id":"bank-out","final_amount":1234,"#,
    r#""transfer_type":"standard"}"#,
);
#[derive(Debug, Eq, PartialEq)]
struct OptionsSnapshot {
    preferred_out: Option<TransferSpeed>,
    standard_destinations: Vec<InstrumentSnapshot>,
    standard_fee_empty: bool,
    standard_estimate: String,
    instant_destination_count: usize,
    instant_fee: (Option<String>, Option<String>, Option<String>, bool),
}

#[derive(Debug, Eq, PartialEq)]
struct InstrumentSnapshot {
    id: String,
    name: String,
    asset_name: String,
    instrument_type: String,
    last_four: String,
    is_default: bool,
    estimate: String,
}

impl From<&TransferInstrument> for InstrumentSnapshot {
    fn from(value: &TransferInstrument) -> Self {
        Self {
            id: value.id().as_str().to_owned(),
            name: value.name().to_owned(),
            asset_name: value.asset_name().to_owned(),
            instrument_type: value.instrument_type().to_owned(),
            last_four: value.last_four().to_owned(),
            is_default: value.is_default(),
            estimate: value.transfer_to_estimate().to_owned(),
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn transfer_options_map_current_direction_speed_and_fee_structure() -> TestResult {
    // Setup.
    let response = scripted_json_response(
        200,
        serde_json::json!({
            "data": {
                "preferred_transfer_type": {"in": null, "out": "standard"},
                "standard": {
                    "eligible_destinations": [{
                        "id": "bank-out", "name": "Outbound bank", "asset_name": "Savings",
                        "type": "bank", "last_four": "2222", "is_default": true,
                        "transfer_to_estimate": "1-3 business days"
                    }],
                    "fee": {
                        "minimum_amount": null, "maximum_amount": null,
                        "variable_percentage": null, "fixed_amount": null
                    },
                    "transfer_to_estimate": "1-3 business days"
                },
                "instant": {
                    "eligible_destinations": [],
                    "fee": {
                        "minimum_amount": 25, "maximum_amount": 2500,
                        "variable_percentage": 1.75, "fixed_amount": 10
                    },
                    "transfer_to_estimate": "Usually within minutes",
                    "supported": false
                }
            }
        }),
    )?;
    let (token, device_id) = test_session()?;

    // Immutable initial script/state.
    let (client, transport) = scripted_client([Ok(response)])?;

    // Complete expected observation.
    let expected = ScriptedObservation::expected(
        Ok(OptionsSnapshot {
            preferred_out: Some(TransferSpeed::Standard),
            standard_destinations: vec![InstrumentSnapshot {
                id: "bank-out".to_owned(),
                name: "Outbound bank".to_owned(),
                asset_name: "Savings".to_owned(),
                instrument_type: "bank".to_owned(),
                last_four: "2222".to_owned(),
                is_default: true,
                estimate: "1-3 business days".to_owned(),
            }],
            standard_fee_empty: true,
            standard_estimate: "1-3 business days".to_owned(),
            instant_destination_count: 0,
            instant_fee: (
                Some("25".to_owned()),
                Some("2500".to_owned()),
                Some("1.75".to_owned()),
                true,
            ),
        }),
        vec![authenticated_read_request(
            "/transfers/options",
            &["transfers", "options"],
            &[],
        )],
    );

    // Execute once.
    let result = client.transfer_options(&token, &device_id).await;
    let observed = ScriptedObservation::observed(
        project_result(result, |options| OptionsSnapshot {
            preferred_out: options.preferred_out(),
            standard_destinations: options
                .standard()
                .eligible_destinations()
                .iter()
                .map(InstrumentSnapshot::from)
                .collect(),
            standard_fee_empty: options.standard().fee().is_empty(),
            standard_estimate: options.standard().transfer_to_estimate().to_owned(),
            instant_destination_count: options.instant().eligible_destinations().len(),
            instant_fee: (
                options.instant().fee().minimum_amount().map(str::to_owned),
                options.instant().fee().maximum_amount().map(str::to_owned),
                options
                    .instant()
                    .fee()
                    .variable_percentage()
                    .map(str::to_owned),
                options.instant().fee().has_additional_non_null_fields(),
            ),
        }),
        &transport,
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn transfer_options_reject_unknown_preference_invalid_ids_and_oversized_branches()
-> TestResult {
    let too_many = (0..=100)
        .map(|index| {
            serde_json::json!({
                "id": format!("bank-{index}"), "name": "Bank", "asset_name": "Checking",
                "type": "bank", "last_four": "1234", "is_default": false,
                "transfer_to_estimate": "Soon"
            })
        })
        .collect::<Vec<_>>();
    for body in [
        serde_json::json!({
            "data": {
                "preferred_transfer_type": {"in": null},
                "standard": empty_mode(), "instant": empty_mode()
            }
        }),
        serde_json::json!({
            "data": {
                "preferred_transfer_type": {"in": null, "out": "overnight"},
                "standard": empty_mode(), "instant": empty_mode()
            }
        }),
        serde_json::json!({
            "data": {
                "preferred_transfer_type": {"in": null, "out": "standard"},
                "standard": mode_with_destinations(vec![serde_json::json!({
                    "id": "bad id", "name": "Bank", "asset_name": "Checking", "type": "bank",
                    "last_four": "1234", "is_default": true, "transfer_to_estimate": "Soon"
                })]),
                "instant": empty_mode()
            }
        }),
        serde_json::json!({
            "data": {
                "preferred_transfer_type": {"in": null, "out": "standard"},
                "standard": mode_with_destinations(vec![synthetic_destination_with_suffix(
                    "short", "123"
                )]),
                "instant": empty_mode()
            }
        }),
        serde_json::json!({
            "data": {
                "preferred_transfer_type": {"in": null, "out": "standard"},
                "standard": mode_with_destinations(vec![synthetic_destination_with_suffix(
                    "overlong", "12345"
                )]),
                "instant": empty_mode()
            }
        }),
        serde_json::json!({
            "data": {
                "preferred_transfer_type": {"in": null, "out": "standard"},
                "standard": mode_with_destinations(vec![synthetic_destination_with_suffix(
                    "full", "1234567890123456"
                )]),
                "instant": empty_mode()
            }
        }),
        serde_json::json!({
            "data": {
                "preferred_transfer_type": {"in": null, "out": "standard"},
                "standard": mode_with_destinations(too_many), "instant": empty_mode()
            }
        }),
        serde_json::json!({
            "data": {
                "preferred_transfer_type": {"in": null, "out": "standard"},
                "standard": mode_with_destinations(vec![
                    synthetic_destination("same"), synthetic_destination("same")
                ]),
                "instant": empty_mode()
            }
        }),
    ] {
        // Setup.
        let response = scripted_json_response(200, body)?;
        let (token, device_id) = test_session()?;

        // Immutable initial script/state.
        let (client, transport) = scripted_client([Ok(response)])?;

        // Complete expected observation.
        let expected = ScriptedObservation::expected(
            Err(ApiErrorSnapshot::contract(TRANSFER_OPTIONS_OPERATION)),
            vec![authenticated_read_request(
                "/transfers/options",
                &["transfers", "options"],
                &[],
            )],
        );

        // Execute once.
        let result = client.transfer_options(&token, &device_id).await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn transfer_creation_sends_exact_standard_body_and_validates_reconciled_success() -> TestResult
{
    // Setup.
    let response = scripted_json_response(201, created_transfer_body())?;
    let (token, device_id) = test_session()?;
    let plan = transfer_plan()?;

    // Immutable initial script/state.
    let (client, transport) = scripted_client([Ok(response)])?;

    // Complete expected observation.
    let expected = ScriptedObservation::expected(
        Ok((
            "789".to_owned(),
            "pending".to_owned(),
            1_234_u64,
            0_u64,
            time::OffsetDateTime::parse("2026-07-17T05:24:56Z", &Rfc3339)?,
        )),
        vec![authenticated_request(
            Method::POST,
            "/transfers",
            &["transfers"],
            &[],
            Some(TRANSFER_CREATION_REQUEST_BODY.as_bytes()),
            OperationClass::FinancialWrite,
        )],
    );

    // Execute once.
    let result = client.create_transfer_out(&token, &device_id, &plan).await;
    let observed = ScriptedObservation::observed(
        project_result(result, |created| {
            (
                created.id().as_str().to_owned(),
                created.status().to_owned(),
                created.net_amount().cents(),
                created.fee_cents(),
                created.requested_at(),
            )
        }),
        &transport,
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn all_available_selection_uses_only_the_resolved_plan_cents_on_the_wire() -> TestResult {
    let response = scripted_json_response(201, created_transfer_body())?;
    let (token, device_id) = test_session()?;
    let plan = transfer_all_plan()?;
    let (client, transport) = scripted_client([Ok(response)])?;

    let created = client
        .create_transfer_out(&token, &device_id, &plan)
        .await?;

    assert_eq!(created.net_amount().cents(), 1_234);
    assert_eq!(
        transport.snapshot(),
        ScriptedTransportSnapshot::for_test(
            vec![authenticated_request(
                Method::POST,
                "/transfers",
                &["transfers"],
                &[],
                Some(TRANSFER_CREATION_REQUEST_BODY.as_bytes()),
                OperationClass::FinancialWrite,
            )],
            Vec::new(),
            false,
        )
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn transfer_creation_treats_empty_or_non_success_responses_as_ambiguous() -> TestResult {
    for response in [
        scripted_response(201, Vec::new())?,
        scripted_json_response(400, serde_json::json!({"error": {"code": 1396}}))?,
        scripted_json_response(200, created_transfer_body())?,
        scripted_json_response(
            201,
            serde_json::json!({
                "data": {
                    "id": 789,
                    "status": "settled",
                    "type": "standard",
                    "amount": 12.34,
                    "amount_cents": 1234,
                    "amount_fee_cents": 0,
                    "amount_requested_cents": 1234,
                    "date_requested": "2026-07-17T05:24:56Z",
                    "destination": {
                        "id": "bank-out", "name": "Bank", "asset_name": "Checking",
                        "type": "bank", "last_four": "2222", "is_default": true,
                        "transfer_to_estimate": "1-3 business days"
                    }
                }
            }),
        )?,
    ] {
        // Setup.
        let status = response.status().as_u16();
        let (token, device_id) = test_session()?;
        let plan = transfer_plan()?;

        // Immutable initial script/state.
        let (client, transport) = scripted_client([Ok(response)])?;

        // Complete expected observation.
        let expected_error = if status >= 400 {
            ApiErrorSnapshot::financial_http_unknown(
                TRANSFER_OUT_CREATION_OPERATION,
                status,
                Some("1396"),
            )
        } else {
            ApiErrorSnapshot::financial_unknown(TRANSFER_OUT_CREATION_OPERATION)
        };
        let expected = ScriptedObservation::expected(
            Err(expected_error),
            vec![authenticated_request(
                Method::POST,
                "/transfers",
                &["transfers"],
                &[],
                Some(TRANSFER_CREATION_REQUEST_BODY.as_bytes()),
                OperationClass::FinancialWrite,
            )],
        );

        // Execute once.
        let result = client.create_transfer_out(&token, &device_id, &plan).await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn transfer_creation_rejects_every_relied_on_success_mismatch_as_ambiguous() -> TestResult {
    let mut wrong_requested = created_transfer_body();
    wrong_requested["data"]["amount_requested_cents"] = serde_json::json!(1235);
    let mut wrong_arithmetic = created_transfer_body();
    wrong_arithmetic["data"]["amount_fee_cents"] = serde_json::json!(1);
    let mut wrong_dollars = created_transfer_body();
    wrong_dollars["data"]["amount"] = serde_json::json!(12.33);
    let mut wrong_speed = created_transfer_body();
    wrong_speed["data"]["type"] = serde_json::json!("instant");
    let mut wrong_destination = created_transfer_body();
    wrong_destination["data"]["destination"]["id"] = serde_json::json!("other-bank");
    let mut wrong_suffix = created_transfer_body();
    wrong_suffix["data"]["destination"]["last_four"] = serde_json::json!("9999");
    let mut invalid_id = created_transfer_body();
    invalid_id["data"]["id"] = serde_json::json!("bad id");
    let mut missing_time = created_transfer_body();
    if let Some(data) = missing_time
        .get_mut("data")
        .and_then(serde_json::Value::as_object_mut)
    {
        data.remove("date_requested");
    }

    for body in [
        wrong_requested,
        wrong_arithmetic,
        wrong_dollars,
        wrong_speed,
        wrong_destination,
        wrong_suffix,
        invalid_id,
        missing_time,
    ] {
        let response = scripted_json_response(201, body)?;
        let (token, device_id) = test_session()?;
        let plan = transfer_plan()?;
        let (client, transport) = scripted_client([Ok(response)])?;
        let expected = ScriptedObservation::expected(
            Err(ApiErrorSnapshot::financial_unknown(
                TRANSFER_OUT_CREATION_OPERATION,
            )),
            vec![authenticated_request(
                Method::POST,
                "/transfers",
                &["transfers"],
                &[],
                Some(TRANSFER_CREATION_REQUEST_BODY.as_bytes()),
                OperationClass::FinancialWrite,
            )],
        );

        let result = client.create_transfer_out(&token, &device_id, &plan).await;
        let observed = ScriptedObservation::observed(project_result(result, |_| ()), &transport);

        assert_eq!(observed, expected);
    }
    Ok(())
}

fn created_transfer_body() -> serde_json::Value {
    serde_json::json!({
        "data": {
            "id": 789,
            "status": "pending",
            "type": "standard",
            "amount": 12.34,
            "amount_cents": 1234,
            "amount_fee_cents": 0,
            "amount_requested_cents": 1234,
            "date_requested": "2026-07-17T05:24:56Z",
            "destination": {
                "id": "bank-out",
                "name": "Bank",
                "asset_name": "Checking",
                "type": "bank",
                "last_four": "2222",
                "is_default": true,
                "transfer_to_estimate": "1-3 business days",
                "account_status": "verified",
                "bank_account": null,
                "card": null
            }
        }
    })
}

fn empty_mode() -> serde_json::Value {
    mode_with_destinations(Vec::new())
}

fn mode_with_destinations(destinations: Vec<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({
        "eligible_destinations": destinations,
        "fee": {
            "minimum_amount": null,
            "maximum_amount": null,
            "variable_percentage": null
        },
        "transfer_to_estimate": "Soon"
    })
}

fn synthetic_destination(id: &str) -> serde_json::Value {
    synthetic_destination_with_suffix(id, "1234")
}

fn synthetic_destination_with_suffix(id: &str, last_four: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "name": "Bank",
        "asset_name": "Checking",
        "type": "bank",
        "last_four": last_four,
        "is_default": false,
        "transfer_to_estimate": "Soon"
    })
}

fn transfer_plan() -> Result<TransferOutPlan, Box<dyn Error>> {
    Ok(TransferOutPlan::new(
        Account::new(
            UserId::from_str("123")?,
            Username::from_bare("alice")?,
            Some("Alice".to_owned()),
        ),
        Balance::new(
            SignedUsdAmount::from_cents(10_000),
            SignedUsdAmount::from_cents(0),
        ),
        TransferOutAmount::Exact(Money::from_cents(1_234)?),
        Money::from_cents(1_234)?,
        TransferSpeed::Standard,
        TransferInstrument::new(
            TransferInstrumentId::from_str("bank-out")?,
            "Bank".to_owned(),
            "Checking".to_owned(),
            "bank".to_owned(),
            TransferInstrumentSuffix::from_str("2222")?,
            true,
            "1-3 business days".to_owned(),
        ),
    ))
}

fn transfer_all_plan() -> Result<TransferOutPlan, Box<dyn Error>> {
    Ok(TransferOutPlan::new(
        Account::new(
            UserId::from_str("123")?,
            Username::from_bare("alice")?,
            Some("Alice".to_owned()),
        ),
        Balance::new(
            SignedUsdAmount::from_cents(1_234),
            SignedUsdAmount::from_cents(500),
        ),
        TransferOutAmount::AllAvailable,
        Money::from_cents(1_234)?,
        TransferSpeed::Standard,
        TransferInstrument::new(
            TransferInstrumentId::from_str("bank-out")?,
            "Bank".to_owned(),
            "Checking".to_owned(),
            "bank".to_owned(),
            TransferInstrumentSuffix::from_str("2222")?,
            true,
            "1-3 business days".to_owned(),
        ),
    ))
}

use super::*;

#[tokio::test(flavor = "current_thread")]
async fn payment_methods_handler_has_exact_typed_call_output_and_zero_flushes() -> TestResult {
    // Setup.
    let response = vec![PaymentMethod::new(
        PaymentMethodId::from_str("method-1")?,
        Some("Bank\nname".to_owned()),
        Some("bank".to_owned()),
        Some("1234".to_owned()),
        true,
    )];

    // Immutable initial script/state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::standard(Rc::clone(&transcript));
    let api = PaymentMethodsFake {
        responses: ResponseQueue::successful(response),
        transcript: Rc::clone(&transcript),
    };
    let mut stdout = RecordingWriter::new(
        Stream::Stdout,
        WriterState::default(),
        Rc::clone(&transcript),
    );
    let expected = Observed::new(
        ResultSnapshot::Success,
        ReadState {
            calls: vec![
                ReadCall::ReadCredential,
                ReadCall::PaymentMethods {
                    session: fixture_session(),
                },
                ReadCall::StdoutWrite,
            ],
            remaining_credentials: vec![ResponseId::UnexpectedSecond],
            api: vec![ResponseId::UnexpectedSecond],
            stdout: writer_state(PAYMENT_METHODS_OUTPUT),
            stderr: WriterState::default(),
        },
    );

    // Execute once.
    let mut stderr = Vec::new();
    let mut output = human_output(CommandId::PayOptions, &mut stdout, &mut stderr);
    let result = run_payment_methods(&reader, &api, &mut output).await;
    let observed = Observed::new(
        snapshot_result(result),
        ReadState {
            calls: transcript.borrow().clone(),
            remaining_credentials: reader.remaining(),
            api: api.responses.remaining(),
            stdout: stdout.state,
            stderr: WriterState::default(),
        },
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn balance_handler_has_exact_typed_call_output_and_zero_flushes() -> TestResult {
    // Setup.
    let response = Balance::new(
        SignedUsdAmount::from_cents(1_234),
        SignedUsdAmount::from_cents(-5),
    );

    // Immutable initial script/state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::standard(Rc::clone(&transcript));
    let api = BalanceFake {
        responses: ResponseQueue::successful(response),
        transcript: Rc::clone(&transcript),
    };
    let mut stdout = writer(Stream::Stdout, Rc::clone(&transcript));
    let expected = Observed::new(
        ResultSnapshot::Success,
        ReadState {
            calls: vec![
                ReadCall::ReadCredential,
                ReadCall::Balance {
                    session: fixture_session(),
                },
                ReadCall::StdoutWrite,
            ],
            remaining_credentials: vec![ResponseId::UnexpectedSecond],
            api: vec![ResponseId::UnexpectedSecond],
            stdout: writer_state("Available: $12.34\nOn hold: -$0.05\n"),
            stderr: WriterState::default(),
        },
    );

    // Execute once.
    let mut stderr = Vec::new();
    let mut output = human_output(CommandId::Balance, &mut stdout, &mut stderr);
    let result = run_balance(&reader, &api, &mut output).await;
    let observed = Observed::new(
        snapshot_result(result),
        ReadState {
            calls: transcript.borrow().clone(),
            remaining_credentials: reader.remaining(),
            api: api.responses.remaining(),
            stdout: stdout.state,
            stderr: WriterState::default(),
        },
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn balance_json_is_exact_valid_and_contains_no_credentials() -> TestResult {
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::standard(Rc::clone(&transcript));
    let api = BalanceFake {
        responses: ResponseQueue::successful(Balance::new(
            SignedUsdAmount::from_cents(1_234),
            SignedUsdAmount::from_cents(-5),
        )),
        transcript: Rc::clone(&transcript),
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut output = OutputSession::new(
        OutputFormat::Json,
        CommandId::Balance,
        false,
        &mut stdout,
        &mut stderr,
    );

    run_balance(&reader, &api, &mut output).await?;

    assert!(stderr.is_empty());
    assert_eq!(stdout.iter().filter(|byte| **byte == b'\n').count(), 1);
    let text = String::from_utf8(stdout.clone())?;
    assert!(!text.contains(TOKEN));
    assert!(!text.contains(DEVICE));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&stdout)?,
        serde_json::json!({
            "command": "balance",
            "ok": true,
            "data": {
                "balance": {
                    "available": { "amount": "12.34", "currency": "USD" },
                    "on_hold": { "amount": "-0.05", "currency": "USD" },
                }
            }
        })
    );
    Ok(())
}

use super::*;

#[tokio::test(flavor = "current_thread")]
async fn accept_handler_orders_default_no_authorization_one_write_output_and_flush() -> TestResult {
    // Setup.
    let script = AcceptScript::successful();

    // Immutable initial script/state.
    let initial = AcceptState::default();
    let expected = Observed::new(
        ResultSnapshot::Success,
        AcceptState {
            calls: successful_calls(),
            remaining: Some(RemainingAcceptScript::after_success()),
            stdout: writer_state(ACCEPT_RESULT, 1),
            stderr: writer_state(ACCEPT_DETAILS, 1),
        },
    );
    let mut harness = AcceptHarness::new(script, initial)?;

    // Execute once.
    let result = harness.execute().await;
    let observed = harness.observed(result);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn accept_default_no_decline_stops_before_interrupt_installation_and_write() -> TestResult {
    // Setup.
    let script =
        AcceptScript::successful().with_first_confirmation(ConfirmationStep::Answer(false));

    // Immutable initial script/state.
    let initial = AcceptState::default();
    let expected = Observed::new(
        failure_snapshot(
            ErrorVariant::Accept,
            ErrorCategory::Cancelled,
            "request acceptance cancelled",
        ),
        AcceptState {
            calls: preparation_and_authorization_calls(),
            remaining: Some(RemainingAcceptScript {
                interruptions: 2,
                acceptances: 2,
                ..RemainingAcceptScript::after_success()
            }),
            stderr: writer_state(ACCEPT_DETAILS, 1),
            ..AcceptState::default()
        },
    );
    let mut harness = AcceptHarness::new(script, initial)?;

    // Execute once.
    let result = harness.execute().await;
    let observed = harness.observed(result);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn accept_preflight_failure_stops_before_output_authorization_and_interruption() -> TestResult
{
    // Setup.
    let script = AcceptScript::successful().with_first_credential(CredentialStep::Missing);

    // Immutable initial script/state.
    let initial = AcceptState::default();
    let expected = Observed::new(
        failure_snapshot(
            ErrorVariant::Accept,
            ErrorCategory::Credential,
            "no Venmo credential is stored; run `venmo auth login`",
        ),
        AcceptState {
            calls: vec![AcceptCall::ReadCredential],
            remaining: Some(RemainingAcceptScript {
                credentials: 1,
                accounts: 2,
                requests: 2,
                users: 2,
                balances: 2,
                confirmations: 2,
                interruptions: 2,
                acceptances: 2,
            }),
            ..AcceptState::default()
        },
    );
    let mut harness = AcceptHarness::new(script, initial)?;

    // Execute once.
    let result = harness.execute().await;
    let observed = harness.observed(result);

    assert_eq!(observed, expected);
    Ok(())
}

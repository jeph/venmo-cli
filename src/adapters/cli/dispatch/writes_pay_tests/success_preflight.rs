use super::*;

#[tokio::test(flavor = "current_thread")]
async fn pay_handler_orders_full_preflight_confirmation_write_output_and_flush() -> TestResult {
    // Setup.
    let script = PayScript::successful();

    // Immutable initial script/state.
    let initial = PayState::default();
    let expected = Observed::new(
        ResultSnapshot::Success,
        PayState {
            calls: successful_calls(),
            remaining: Some(RemainingPayScript::after_success()),
            stdout: writer_state(PAY_RESULT, 1),
            stderr: writer_state(PAY_DETAILS, 1),
        },
    );
    let mut harness = PayHarness::new(script, initial)?;

    // Execute once.
    let result = harness.execute().await;
    let observed = harness.observed(result);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn pay_dry_run_keeps_full_preflight_but_skips_prompt_signal_otp_and_write() -> TestResult {
    let mut calls = preparation_calls();
    calls.extend([
        PayCall::StderrWrite,
        PayCall::StderrFlush,
        PayCall::StdoutWrite,
        PayCall::StdoutFlush,
    ]);
    let expected = Observed::new(
        ResultSnapshot::Success,
        PayState {
            calls,
            remaining: Some(RemainingPayScript {
                credentials: 1,
                accounts: 1,
                users: 1,
                balances: 1,
                funding_methods: 1,
                eligibility: 1,
                request_ids: 1,
                confirmations: 2,
                interruptions: 2,
                creations: 2,
            }),
            stdout: writer_state("Dry run complete; no changes made.\n", 1),
            stderr: writer_state(PAY_DETAILS, 1),
        },
    );
    let mut harness = PayHarness::new(PayScript::successful(), PayState::default())?;
    harness.args.yes = false;
    harness.args.dry_run = true;

    let result = harness.execute().await;
    let observed = harness.observed(result);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn pay_handler_propagates_explicit_visibility_to_plan_and_output() -> TestResult {
    let script = PayScript::successful();
    let expected = Observed::new(
        ResultSnapshot::Success,
        PayState {
            calls: successful_calls_with_visibility(Visibility::Public),
            remaining: Some(RemainingPayScript::after_success()),
            stdout: writer_state(&PAY_RESULT.replace("private", "public"), 1),
            stderr: writer_state(&PAY_DETAILS.replace("private", "public"), 1),
        },
    );
    let mut harness = PayHarness::new(script, PayState::default())?;
    harness.args.visibility = crate::adapters::cli::args::VisibilityArg::Public;

    let result = harness.execute().await;
    let observed = harness.observed(result);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn pay_handler_propagates_purchase_protection_through_preflight_and_output() -> TestResult {
    let expected = Observed::new(
        ResultSnapshot::Success,
        PayState {
            calls: protected_successful_calls(),
            remaining: Some(RemainingPayScript::after_success()),
            stdout: writer_state(PROTECTED_PAY_RESULT, 1),
            stderr: writer_state(PROTECTED_PAY_DETAILS, 1),
        },
    );
    let mut harness = PayHarness::new(PayScript::successful(), PayState::default())?;
    harness.args.protect = true;

    let result = harness.execute().await;
    let observed = harness.observed(result);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn pay_preflight_failure_precedes_output_confirmation_and_interruption() -> TestResult {
    // Setup.
    let script = PayScript::successful().with_first_credential(CredentialStep::Missing);

    // Immutable initial script/state.
    let initial = PayState::default();
    let expected = Observed::new(
        failure_snapshot(
            ErrorVariant::Pay,
            ErrorCategory::Credential,
            "no Venmo credential is stored; run `venmo auth login`",
        ),
        PayState {
            calls: vec![PayCall::ReadCredential],
            remaining: Some(RemainingPayScript {
                credentials: 1,
                accounts: 2,
                users: 2,
                balances: 2,
                funding_methods: 2,
                eligibility: 2,
                request_ids: 2,
                confirmations: 2,
                interruptions: 2,
                creations: 2,
            }),
            ..PayState::default()
        },
    );
    let mut harness = PayHarness::new(script, initial)?;

    // Execute once.
    let result = harness.execute().await;
    let observed = harness.observed(result);

    assert_eq!(observed, expected);
    Ok(())
}

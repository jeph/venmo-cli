use super::*;

#[tokio::test(flavor = "current_thread")]
async fn pay_interruption_tie_and_signal_failures_have_exact_write_boundaries() -> TestResult {
    for (script, expected_result, write_started) in [
        (
            PayScript::successful()
                .with_first_creation(CreationStep::Pending)
                .with_first_interruption(InterruptionStep::PendingThenReady),
            failure_snapshot(
                ErrorVariant::FinancialWriteInterruptedUnknown,
                ErrorCategory::AmbiguousWrite,
                "the financial operation was interrupted after transmission may have begun; do not retry until its status is verified independently",
            ),
            true,
        ),
        (
            PayScript::successful().with_first_interruption(InterruptionStep::Ready),
            failure_snapshot(
                ErrorVariant::FinancialWriteInterruptedUnknown,
                ErrorCategory::AmbiguousWrite,
                "the financial operation was interrupted after transmission may have begun; do not retry until its status is verified independently",
            ),
            false,
        ),
        (
            PayScript::successful().with_first_interruption(InterruptionStep::StreamFailure),
            failure_snapshot(
                ErrorVariant::SignalInitialization,
                ErrorCategory::Internal,
                "failed to install financial-write interrupt protection",
            ),
            false,
        ),
        (
            PayScript::successful().with_first_interruption(InterruptionStep::InstallationFailure),
            failure_snapshot(
                ErrorVariant::SignalInitialization,
                ErrorCategory::Internal,
                "failed to install financial-write interrupt protection",
            ),
            false,
        ),
    ] {
        // Immutable initial script/state.
        let initial = PayState::default();
        let mut calls = preparation_and_authorization_calls();
        calls.push(PayCall::InstallInterruption);
        if write_started {
            calls.push(create_payment_call());
        }
        let expected = Observed::new(
            expected_result,
            PayState {
                calls,
                remaining: Some(RemainingPayScript {
                    creations: if write_started { 1 } else { 2 },
                    ..RemainingPayScript::after_success()
                }),
                stderr: writer_state(PAY_DETAILS, 1),
                ..PayState::default()
            },
        );
        let mut harness = PayHarness::new(script, initial)?;

        // Execute once.
        let result = harness.execute().await;
        let observed = harness.observed(result);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn pay_post_write_output_failures_keep_the_specialized_ambiguous_outcome() -> TestResult {
    for initial_stdout in [
        WriterState {
            fail_write: true,
            ..WriterState::default()
        },
        WriterState {
            fail_flush: true,
            ..WriterState::default()
        },
    ] {
        // Setup.
        let script = PayScript::successful();

        // Immutable initial script/state.
        let initial = PayState {
            stdout: initial_stdout.clone(),
            ..PayState::default()
        };
        let expected_stdout = if initial_stdout.fail_write {
            initial_stdout
        } else {
            WriterState {
                text: PAY_RESULT.to_owned(),
                flush_count: 1,
                fail_flush: true,
                ..WriterState::default()
            }
        };
        let expected = Observed::new(
            failure_snapshot(
                ErrorVariant::FinancialResultOutput,
                ErrorCategory::AmbiguousWrite,
                "the financial operation succeeded, but its result could not be written; do not retry it and verify the result through activity or requests and the official Venmo app",
            ),
            PayState {
                calls: if expected_stdout.fail_write {
                    successful_calls_without_stdout_flush()
                } else {
                    successful_calls()
                },
                remaining: Some(RemainingPayScript::after_success()),
                stdout: expected_stdout,
                stderr: writer_state(PAY_DETAILS, 1),
            },
        );
        let mut harness = PayHarness::new(script, initial)?;

        // Execute once.
        let result = harness.execute().await;
        let observed = harness.observed(result);

        assert_eq!(observed, expected);
    }
    Ok(())
}

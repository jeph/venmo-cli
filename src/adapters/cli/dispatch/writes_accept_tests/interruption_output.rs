use super::*;

#[tokio::test(flavor = "current_thread")]
async fn accept_interruption_tie_and_signal_failure_preserve_exact_ambiguity_boundaries()
-> TestResult {
    for (script, expected_result, write_started) in [
        (
            AcceptScript::successful()
                .with_first_acceptance(WriteStep::Pending)
                .with_first_interruption(InterruptionStep::PendingThenReady),
            failure_snapshot(
                ErrorVariant::FinancialWriteInterruptedUnknown,
                ErrorCategory::AmbiguousWrite,
                "the financial operation was interrupted after transmission may have begun; do not retry until its status is verified independently",
            ),
            true,
        ),
        (
            AcceptScript::successful().with_first_interruption(InterruptionStep::Ready),
            failure_snapshot(
                ErrorVariant::FinancialWriteInterruptedUnknown,
                ErrorCategory::AmbiguousWrite,
                "the financial operation was interrupted after transmission may have begun; do not retry until its status is verified independently",
            ),
            false,
        ),
        (
            AcceptScript::successful().with_first_interruption(InterruptionStep::StreamFailure),
            failure_snapshot(
                ErrorVariant::SignalInitialization,
                ErrorCategory::Internal,
                "failed to install financial-write interrupt protection",
            ),
            false,
        ),
    ] {
        // Immutable initial script/state.
        let initial = AcceptState::default();
        let mut calls = preparation_and_authorization_calls();
        calls.push(AcceptCall::InstallInterruption);
        if write_started {
            calls.push(accept_call());
        }
        let expected = Observed::new(
            expected_result,
            AcceptState {
                calls,
                remaining: Some(RemainingAcceptScript {
                    acceptances: if write_started { 1 } else { 2 },
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
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn accept_post_write_output_failures_are_specialized_ambiguous_outcomes() -> TestResult {
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
        let script = AcceptScript::successful();

        // Immutable initial script/state.
        let initial = AcceptState {
            stdout: initial_stdout.clone(),
            ..AcceptState::default()
        };
        let expected_stdout = if initial_stdout.fail_write {
            initial_stdout
        } else {
            WriterState {
                text: ACCEPT_RESULT.to_owned(),
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
            AcceptState {
                calls: if expected_stdout.fail_write {
                    successful_calls_without_stdout_flush()
                } else {
                    successful_calls()
                },
                remaining: Some(RemainingAcceptScript::after_success()),
                stdout: expected_stdout,
                stderr: writer_state(ACCEPT_DETAILS, 1),
            },
        );
        let mut harness = AcceptHarness::new(script, initial)?;

        // Execute once.
        let result = harness.execute().await;
        let observed = harness.observed(result);

        assert_eq!(observed, expected);
    }
    Ok(())
}

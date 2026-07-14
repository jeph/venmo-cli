use super::*;

#[derive(Clone, Copy)]
enum FundingPromptCase {
    Cancelled,
    Interaction,
    InvalidChoice,
}

#[tokio::test(flavor = "current_thread")]
async fn funding_prompt_cancel_error_and_invalid_choice_stop_before_eligibility() -> TestResult {
    for case in [
        FundingPromptCase::Cancelled,
        FundingPromptCase::Interaction,
        FundingPromptCase::InvalidChoice,
    ] {
        // Setup.
        let amount = Money::from_cents(1)?;
        let note = Note::from_str("Synthetic note")?;
        let methods = vec![
            peer_method(
                "bank-1",
                PeerFundingRole::Backup,
                PeerFundingFee::ProvenZero,
            )?,
            peer_method(
                "bank-2",
                PeerFundingRole::Backup,
                PeerFundingFee::ProvenZero,
            )?,
        ];
        let (selection, expected_failure) = match case {
            FundingPromptCase::Cancelled => (
                SelectionScript::Cancelled,
                FundingFailure::Prompt(PromptFailure::Cancelled),
            ),
            FundingPromptCase::Interaction => (
                SelectionScript::Interaction,
                FundingFailure::Prompt(PromptFailure::Interaction),
            ),
            FundingPromptCase::InvalidChoice => (
                SelectionScript::Index(2),
                FundingFailure::Prompt(PromptFailure::InvalidSelection {
                    index: 2,
                    choice_count: 2,
                }),
            ),
        };

        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader::new(ReaderScript::Present, Rc::clone(&transcript));
        let api = FakeApi::new(
            PayScript::successful()?.with_methods(Ok(methods.clone())),
            Rc::clone(&transcript),
        );
        let generator = FixedGenerator::new(Rc::clone(&transcript));
        let prompt = FakePrompt::new(
            true,
            ConfirmationScript::Answer(true),
            selection,
            Rc::clone(&transcript),
        );

        // Complete expected final outcome and state.
        let mut expected_calls = successful_calls(amount, note.clone())?
            .into_iter()
            .take(5)
            .collect::<Vec<_>>();
        expected_calls.extend([
            Call::PromptAvailability,
            Call::SelectFundingChoice {
                prompt: "Choose a payment method".to_owned(),
                choices: methods
                    .iter()
                    .map(|method| method.method().clone())
                    .collect(),
            },
        ]);
        let expected = Observation::new(
            PayOutcome::Failure(PayFailure::FundingSelection(expected_failure)),
            expected_calls,
        );

        // Execute once.
        let result = run_pay(&reader, &api, &generator, &prompt, amount, note, None, true).await;
        let observed = Observation::new(pay_outcome(result), transcript.borrow().clone());

        assert_eq!(observed, expected);
    }
    Ok(())
}

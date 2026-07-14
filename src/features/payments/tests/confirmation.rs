use super::*;

#[derive(Clone, Copy)]
enum ConfirmationCase {
    Required,
    Declined,
    Cancelled,
    Interaction,
    Confirmed,
}

#[tokio::test(flavor = "current_thread")]
async fn confirmation_required_declined_cancelled_failed_and_confirmed_are_distinct() -> TestResult
{
    for case in [
        ConfirmationCase::Required,
        ConfirmationCase::Declined,
        ConfirmationCase::Cancelled,
        ConfirmationCase::Interaction,
        ConfirmationCase::Confirmed,
    ] {
        // Setup.
        let amount = Money::from_cents(1)?;
        let note = Note::from_str("Synthetic note")?;
        let (interactive, confirmation, expected_outcome, prompt_calls) =
            confirmation_case(case, amount, note.clone())?;

        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader::new(ReaderScript::Present, Rc::clone(&transcript));
        let api = FakeApi::new(PayScript::successful()?, Rc::clone(&transcript));
        let generator = FixedGenerator::new(Rc::clone(&transcript));
        let prompt = FakePrompt::new(
            interactive,
            confirmation,
            SelectionScript::Index(0),
            Rc::clone(&transcript),
        );

        // Complete expected final outcome and state.
        let mut expected_calls = successful_calls(amount, note.clone())?
            .into_iter()
            .take(7)
            .collect::<Vec<_>>();
        expected_calls.extend(prompt_calls);
        if matches!(case, ConfirmationCase::Confirmed) {
            let create = successful_calls(amount, note.clone())?
                .into_iter()
                .nth(7)
                .ok_or_else(|| io::Error::other("missing synthetic create call"))?;
            expected_calls.push(create);
        }
        let expected = Observation::new(expected_outcome, expected_calls);

        // Execute once.
        let result = run_pay(
            &reader, &api, &generator, &prompt, amount, note, None, false,
        )
        .await;
        let observed = Observation::new(pay_outcome(result), transcript.borrow().clone());

        assert_eq!(observed, expected);
    }
    Ok(())
}

type ConfirmationExpectation = (bool, ConfirmationScript, PayOutcome, Vec<Call>);

fn confirmation_case(
    case: ConfirmationCase,
    amount: Money,
    note: Note,
) -> Result<ConfirmationExpectation, Box<dyn Error>> {
    Ok(match case {
        ConfirmationCase::Required => (
            false,
            ConfirmationScript::Answer(false),
            PayOutcome::Failure(PayFailure::ConfirmationRequired),
            vec![Call::PromptAvailability],
        ),
        ConfirmationCase::Declined => (
            true,
            ConfirmationScript::Answer(false),
            PayOutcome::Failure(PayFailure::ConfirmationDeclined),
            confirmation_calls(),
        ),
        ConfirmationCase::Cancelled => (
            true,
            ConfirmationScript::Cancelled,
            PayOutcome::Failure(PayFailure::Confirmation(PromptFailure::Cancelled)),
            confirmation_calls(),
        ),
        ConfirmationCase::Interaction => (
            true,
            ConfirmationScript::Interaction,
            PayOutcome::Failure(PayFailure::Confirmation(PromptFailure::Interaction)),
            confirmation_calls(),
        ),
        ConfirmationCase::Confirmed => (
            true,
            ConfirmationScript::Answer(true),
            PayOutcome::Success(Box::new(PayResult::new(
                pay_plan(
                    test_account()?,
                    financial_user()?,
                    amount,
                    note,
                    test_balance(),
                    peer_method(
                        "bank-1",
                        PeerFundingRole::Default,
                        PeerFundingFee::ProvenZero,
                    )?,
                )?,
                created_payment()?,
            ))),
            confirmation_calls(),
        ),
    })
}

fn confirmation_calls() -> Vec<Call> {
    vec![
        Call::PromptAvailability,
        Call::ConfirmDefaultNo {
            prompt: "Send this payment?".to_owned(),
        },
    ]
}

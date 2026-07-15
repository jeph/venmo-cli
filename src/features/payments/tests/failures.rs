use super::*;

#[derive(Clone, Copy)]
enum FailureStage {
    MissingCredential,
    CredentialRead,
    CurrentAccount,
    UserLookup,
    Balance,
    FundingMethods,
    Eligibility,
    Create,
}

#[tokio::test(flavor = "current_thread")]
async fn credential_and_api_failures_preserve_categories_and_stop_at_the_failing_call() -> TestResult
{
    for stage in [
        FailureStage::MissingCredential,
        FailureStage::CredentialRead,
        FailureStage::CurrentAccount,
        FailureStage::UserLookup,
        FailureStage::Balance,
        FailureStage::FundingMethods,
        FailureStage::Eligibility,
        FailureStage::Create,
    ] {
        // Setup.
        let amount = Money::from_cents(1)?;
        let note = Note::from_str("Synthetic note")?;
        let full_calls = successful_calls(amount, note.clone())?;
        let (reader_script, api_script, expected_failure, prefix_len) = failure_case(stage)?;

        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader::new(reader_script, Rc::clone(&transcript));
        let api = FakeApi::new(api_script, Rc::clone(&transcript));
        let generator = FixedGenerator::new(Rc::clone(&transcript));
        let prompt = FakePrompt::new(
            false,
            ConfirmationScript::Answer(false),
            SelectionScript::Index(0),
            Rc::clone(&transcript),
        );

        // Complete expected final outcome and state.
        let expected = Observation::new(
            PayOutcome::Failure(expected_failure),
            full_calls.into_iter().take(prefix_len).collect(),
        );

        // Execute once.
        let result = run_pay(&reader, &api, &generator, &prompt, amount, note, None, true).await;
        let observed = Observation::new(pay_outcome(result), transcript.borrow().clone());

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn recipient_and_funding_contract_failures_never_reach_eligibility_or_write() -> TestResult {
    for (script, expected_failure) in [
        (
            PayScript::successful()?.with_user(Ok(User::new(
                UserId::from_str("999")?,
                Some(Username::from_bare("different")?),
                Some("Different user".to_owned()),
            )
            .with_financial_attributes(UserProfileKind::Personal, true))),
            PayFailure::Recipient(RecipientResolutionFailureKind::ApiContract),
        ),
        (
            PayScript::successful()?.with_methods(Ok(vec![
                peer_method(
                    "duplicate",
                    PeerFundingRole::Backup,
                    PeerFundingFee::ProvenZero,
                )?,
                peer_method(
                    "duplicate",
                    PeerFundingRole::Backup,
                    PeerFundingFee::ProvenZero,
                )?,
            ])),
            PayFailure::FundingSelection(FundingFailure::DuplicateMethodIds),
        ),
    ] {
        // Setup.
        let amount = Money::from_cents(1)?;
        let note = Note::from_str("Synthetic note")?;
        let prefix_len = if matches!(expected_failure, PayFailure::Recipient(_)) {
            3
        } else {
            5
        };

        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader::new(ReaderScript::Present, Rc::clone(&transcript));
        let api = FakeApi::new(script, Rc::clone(&transcript));
        let generator = FixedGenerator::new(Rc::clone(&transcript));
        let prompt = FakePrompt::new(
            false,
            ConfirmationScript::Answer(false),
            SelectionScript::Index(0),
            Rc::clone(&transcript),
        );

        // Complete expected final outcome and state.
        let expected = Observation::new(
            PayOutcome::Failure(expected_failure),
            successful_calls(amount, note.clone())?
                .into_iter()
                .take(prefix_len)
                .collect(),
        );

        // Execute once.
        let result = run_pay(&reader, &api, &generator, &prompt, amount, note, None, true).await;
        let observed = Observation::new(pay_outcome(result), transcript.borrow().clone());

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn nonzero_transaction_fee_is_retained_and_payment_proceeds() -> TestResult {
    // Setup.
    let amount = Money::from_cents(1)?;
    let note = Note::from_str("Synthetic note")?;
    let script = PayScript::successful()?.with_eligibility(Ok(blank_eligibility(1)?));

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::new(ReaderScript::Present, Rc::clone(&transcript));
    let api = FakeApi::new(script, Rc::clone(&transcript));
    let generator = FixedGenerator::new(Rc::clone(&transcript));
    let prompt = FakePrompt::new(
        false,
        ConfirmationScript::Answer(false),
        SelectionScript::Index(0),
        Rc::clone(&transcript),
    );

    // Complete expected final outcome and state.
    let expected_plan = pay_plan_with_fee(
        test_account()?,
        financial_user()?,
        amount,
        note.clone(),
        test_balance(),
        peer_method(
            "bank-1",
            PeerFundingRole::Default,
            PeerFundingFee::ProvenZero,
        )?,
        1,
    )?;
    let expected = Observation::new(
        PayOutcome::Success(Box::new(PayResult::new(expected_plan, created_payment()?))),
        successful_calls_with_fee(amount, note.clone(), 1)?,
    );

    // Execute once.
    let result = run_pay(&reader, &api, &generator, &prompt, amount, note, None, true).await;
    let observed = Observation::new(pay_outcome(result), transcript.borrow().clone());

    assert_eq!(observed, expected);
    Ok(())
}

fn failure_case(
    stage: FailureStage,
) -> Result<(ReaderScript, PayScript, PayFailure, usize), Box<dyn Error>> {
    let script = PayScript::successful()?;
    Ok(match stage {
        FailureStage::MissingCredential => (
            ReaderScript::Missing,
            script,
            PayFailure::CredentialMissing,
            1,
        ),
        FailureStage::CredentialRead => {
            (ReaderScript::Failure, script, PayFailure::CredentialRead, 1)
        }
        FailureStage::CurrentAccount => (
            ReaderScript::Present,
            script.with_account(Err(FakeApiError(ApiFailureKind::Rejected))),
            PayFailure::CurrentAccount(ApiFailureKind::Rejected),
            2,
        ),
        FailureStage::UserLookup => (
            ReaderScript::Present,
            script.with_user(Err(FakeApiError(ApiFailureKind::Timeout))),
            PayFailure::Recipient(RecipientResolutionFailureKind::Api(ApiFailureKind::Timeout)),
            3,
        ),
        FailureStage::Balance => (
            ReaderScript::Present,
            script.with_balance(Err(FakeApiError(ApiFailureKind::Network))),
            PayFailure::Balance(ApiFailureKind::Network),
            4,
        ),
        FailureStage::FundingMethods => (
            ReaderScript::Present,
            script.with_methods(Err(FakeApiError(ApiFailureKind::Contract))),
            PayFailure::FundingMethods(ApiFailureKind::Contract),
            5,
        ),
        FailureStage::Eligibility => (
            ReaderScript::Present,
            script.with_eligibility(Err(FakeApiError(ApiFailureKind::Contract))),
            PayFailure::Eligibility(ApiFailureKind::Contract),
            6,
        ),
        FailureStage::Create => (
            ReaderScript::Present,
            script.with_creation(Err(FakeApiError(ApiFailureKind::AmbiguousWrite))),
            PayFailure::Create(ApiFailureKind::AmbiguousWrite),
            8,
        ),
    })
}

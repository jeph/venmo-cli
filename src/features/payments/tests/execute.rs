use super::*;

#[tokio::test(flavor = "current_thread")]
async fn complete_preflight_then_execute_has_one_ordered_write_with_complete_arguments()
-> TestResult {
    // Setup.
    let amount = Money::from_cents(125)?;
    let note = Note::from_str("Synthetic payment note")?;
    let account = test_account()?;
    let recipient = financial_user()?;
    let balance = test_balance();
    let method = peer_method(
        "bank-1",
        PeerFundingRole::Default,
        PeerFundingFee::ProvenZero,
    )?;
    let created = created_payment()?;

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::new(ReaderScript::Present, Rc::clone(&transcript));
    let api = FakeApi::new(PayScript::successful()?, Rc::clone(&transcript));
    let generator = FixedGenerator::new(Rc::clone(&transcript));
    let prompt = FakePrompt::new(
        false,
        ConfirmationScript::Answer(false),
        SelectionScript::Index(0),
        Rc::clone(&transcript),
    );

    // Complete expected final outcome and state.
    let plan = pay_plan(
        account.clone(),
        recipient.clone(),
        amount,
        note.clone(),
        balance.clone(),
        method.clone(),
    )?;
    let expected = Observation::new(
        PayOutcome::Success(Box::new(PayResult::new(plan, created))),
        vec![
            Call::ReadCredential,
            current_account_call(),
            user_lookup_call("456")?,
            balance_call(),
            funding_methods_call(),
            Call::Eligibility {
                session: RedactedSecret::Redacted,
                recipient,
                amount,
                note,
            },
            Call::GenerateClientRequestId {
                request_id: fixed_request_id(),
            },
            Call::CreatePayment {
                session: RedactedSecret::Redacted,
                plan: Box::new(PayPlanCall {
                    request_id: fixed_request_id(),
                    account,
                    recipient: financial_user()?,
                    amount,
                    note: Note::from_str("Synthetic payment note")?,
                    balance,
                    backup_method: method,
                    eligibility_token: RedactedSecret::Redacted,
                }),
            },
        ],
    );

    // Execute once.
    let result = run_pay(
        &reader,
        &api,
        &generator,
        &prompt,
        amount,
        Note::from_str("Synthetic payment note")?,
        None,
        true,
    )
    .await;
    let observed = Observation::new(pay_outcome(result), transcript.borrow().clone());

    assert_eq!(observed, expected);
    let rendered = format!("{:?}", observed.transcript);
    assert!(!rendered.contains(ACCESS_TOKEN));
    assert!(!rendered.contains(DEVICE_ID));
    assert!(!rendered.contains(ELIGIBILITY_TOKEN));
    Ok(())
}

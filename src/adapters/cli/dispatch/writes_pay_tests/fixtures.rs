use super::*;

pub(super) fn pay_args() -> TestResult<PayArgs> {
    let cli = Cli::try_parse_from(["venmo", "pay", "456", "0.01", "--note", "Synthetic payment"])?;
    match cli.command {
        Command::Pay(args) => Ok(args),
        _ => Err(io::Error::other("pay arguments parsed as another command").into()),
    }
}

pub(super) fn credential() -> Result<LoadedCredential, FakeCredentialError> {
    Ok(LoadedCredential {
        envelope: CredentialEnvelope::new(
            AccessToken::from_str("synthetic-pay-token").map_err(|_| FakeCredentialError)?,
            DeviceId::from_str("synthetic-pay-device").map_err(|_| FakeCredentialError)?,
            UserId::from_str("123").map_err(|_| FakeCredentialError)?,
            Username::from_bare("owner").map_err(|_| FakeCredentialError)?,
            Some("Synthetic owner".to_owned()),
            time::OffsetDateTime::UNIX_EPOCH,
        ),
        format: CredentialFormat::Version1,
    })
}

pub(super) fn account() -> TestResult<Account> {
    Ok(Account::new(
        UserId::from_str("123")?,
        Username::from_bare("owner")?,
        Some("Synthetic owner".to_owned()),
    ))
}

pub(super) fn recipient() -> TestResult<User> {
    Ok(User::new(
        UserId::from_str("456")?,
        Some(Username::from_bare("bob")?),
        Some("Synthetic recipient".to_owned()),
    )
    .with_financial_attributes(UserProfileKind::Personal, true))
}

pub(super) fn balance() -> Balance {
    Balance::new(
        SignedUsdAmount::from_cents(300),
        SignedUsdAmount::from_cents(25),
    )
}

pub(super) fn funding_method() -> TestResult<PeerFundingMethod> {
    Ok(PeerFundingMethod::new(
        PaymentMethod::new(
            PaymentMethodId::from_str("bank-1")?,
            Some("Synthetic bank".to_owned()),
            Some("bank".to_owned()),
            Some("1234".to_owned()),
            true,
        ),
        PeerFundingRole::Default,
        PeerFundingFee::Unknown,
    ))
}

pub(super) fn fixed_request_id() -> ClientRequestId {
    match ClientRequestId::from_str(REQUEST_ID) {
        Ok(request_id) => request_id,
        Err(_) => ClientRequestId::generate(),
    }
}

pub(super) fn created_payment() -> TestResult<CreatedPayment> {
    Ok(CreatedPayment::new(
        PaymentId::from_str("payment-1")?,
        FinancialStatus::Settled,
    ))
}

pub(super) const PAY_PREFLIGHT: &str = concat!(
    "Payment preflight:\n",
    "  From account: @owner (ID 123)\n",
    "  Recipient: @bob (Synthetic recipient) (ID 456)\n",
    "  Amount: $0.01\n",
    "  Note: Synthetic payment\n",
    "  Requested audience: private\n",
    "  Available Venmo balance: $3.00\n",
    "  Submitted backup method: Synthetic bank (bank ending 1234, ID bank-1)\n",
    "  Submitted method fee: unknown\n",
    "  Eligibility-reported fee: $0.00\n",
    "  Eligibility-reported total: $0.01\n",
    "  Warning: Venmo may use available balance before the submitted backup method.\n",
    "  Warning: eligibility is not bound to the submitted backup method; the final fee may differ.\n",
    "  Warning: Venmo may apply a more restrictive audience based on participant privacy settings.\n",
);

pub(super) const PAY_RESULT: &str = concat!(
    "Payment ID: payment-1\n",
    "Status: settled\n",
    "Recipient: @bob (Synthetic recipient)\n",
    "Amount: $0.01\n",
    "Requested audience: private\n",
    "Eligibility-reported fee: $0.00\n",
    "Submitted backup method ID: bank-1\n",
    "The response does not prove the final funding source or fee; Venmo may have used available balance before the submitted backup method.\n",
);

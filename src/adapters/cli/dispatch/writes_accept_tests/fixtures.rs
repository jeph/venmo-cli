use super::*;

pub(super) fn accept_args() -> TestResult<AcceptArgs> {
    let cli = Cli::try_parse_from(["venmo", "accept", "request-1"])?;
    match cli.command {
        Command::Accept(args) => Ok(args),
        _ => Err(io::Error::other("accept arguments parsed as another command").into()),
    }
}

pub(super) fn credential() -> Result<LoadedCredential, FakeCredentialError> {
    Ok(LoadedCredential {
        envelope: CredentialEnvelope::new(
            AccessToken::from_str("synthetic-accept-token").map_err(|_| FakeCredentialError)?,
            DeviceId::from_str("synthetic-accept-device").map_err(|_| FakeCredentialError)?,
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

pub(super) fn requester() -> TestResult<User> {
    Ok(User::new(
        UserId::from_str("456")?,
        Some(Username::from_bare("requester")?),
        Some("Synthetic requester".to_owned()),
    )
    .with_financial_attributes(UserProfileKind::Personal, true))
}

pub(super) fn request_record() -> TestResult<RequestRecord> {
    Ok(RequestRecord::new(
        RequestId::from_str("request-1")?,
        RequestAction::Charge,
        RequestDirection::Incoming,
        User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("requester")?),
            Some("Synthetic requester".to_owned()),
        ),
        Money::from_cents(1)?,
        Some("Synthetic request".to_owned()),
        Some(time::OffsetDateTime::UNIX_EPOCH),
        RequestStatus::from_str("pending")?,
    )
    .with_audience(Some("private".to_owned())))
}

pub(super) fn balance() -> Balance {
    Balance::new(
        SignedUsdAmount::from_cents(1),
        SignedUsdAmount::from_cents(0),
    )
}

pub(super) fn accepted_request() -> TestResult<AcceptedRequest> {
    Ok(AcceptedRequest::new(
        PaymentId::from_str("payment-1")?,
        FinancialStatus::Settled,
    ))
}

pub(super) const ACCEPT_PREFLIGHT: &str = concat!(
    "Request acceptance preflight:\n",
    "  Paying account: @owner (ID 123)\n",
    "  Request ID: request-1\n",
    "  Requester: @requester (Synthetic requester) (ID 456)\n",
    "  Amount: $0.01\n",
    "  Note: Synthetic request\n",
    "  Audience: private\n",
    "  Current request status: pending\n",
    "  Created: 1970-01-01T00:00:00Z\n",
    "  Fee/source proof: unavailable in the request-update contract\n",
    "  Available Venmo balance: $0.01\n",
    "  Funding guard: available balance covers the request and no external funding method will be submitted.\n",
    "  Warning: the update does not bind that balance snapshot or prove the final fee/source; accepting pays the requester and settles this exact request.\n",
);

pub(super) const ACCEPT_RESULT: &str = concat!(
    "Accepted request ID: request-1\n",
    "Payment ID: payment-1\n",
    "Status: settled\n",
    "Paid requester: @requester (Synthetic requester)\n",
    "Amount: $0.01\n",
    "Preflight required full available-balance coverage and submitted no external funding method; the response did not prove the actual source or fee.\n",
);

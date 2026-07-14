use super::*;

pub(super) fn session_call(access_token: &AccessToken, device_id: &DeviceId) -> SessionCall {
    SessionCall {
        access_token: if access_token.expose_secret() == TOKEN {
            SensitiveArgument::Fixture
        } else {
            SensitiveArgument::Other
        },
        device_id: if device_id.as_str() == DEVICE {
            SensitiveArgument::Fixture
        } else {
            SensitiveArgument::Other
        },
    }
}

pub(super) const fn fixture_session() -> SessionCall {
    SessionCall {
        access_token: SensitiveArgument::Fixture,
        device_id: SensitiveArgument::Fixture,
    }
}

pub(super) fn credential() -> Result<LoadedCredential, FakeCredentialError> {
    Ok(LoadedCredential {
        envelope: CredentialEnvelope::new(
            AccessToken::from_str(TOKEN).map_err(|_| FakeCredentialError)?,
            DeviceId::from_str(DEVICE).map_err(|_| FakeCredentialError)?,
            UserId::from_str("1000").map_err(|_| FakeCredentialError)?,
            Username::from_bare("owner").map_err(|_| FakeCredentialError)?,
            Some("Synthetic owner".to_owned()),
            time::OffsetDateTime::UNIX_EPOCH,
        ),
        format: CredentialFormat::Version1,
    })
}

pub(super) fn synthetic_activity() -> TestResult<Activity> {
    Ok(Activity::new(
        ActivityId::from_str("story-1")?,
        time::OffsetDateTime::UNIX_EPOCH,
        ActivityAction::from_str("pay")?,
        ActivityDirection::Outgoing,
        ActivityCounterparty::user(User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("bob")?),
            Some("Synthetic User".to_owned()),
        )),
        Money::from_str("1.25")?,
        ActivityStatus::from_str("failed")?,
        Some("note\n\u{1b}[31mline".to_owned()),
        Some("private".to_owned()),
    ))
}

pub(super) fn synthetic_transfer() -> TestResult<Activity> {
    Ok(Activity::new(
        ActivityId::from_str("story-transfer")?,
        time::OffsetDateTime::UNIX_EPOCH,
        ActivityAction::from_str("transfer:standard")?,
        ActivityDirection::Outgoing,
        ActivityCounterparty::external(
            "Bank\nname".to_owned(),
            "bank".to_owned(),
            Some("1234".to_owned()),
        ),
        Money::from_str("12.34")?,
        ActivityStatus::from_str("issued")?,
        None,
        Some("private".to_owned()),
    ))
}

pub(super) fn synthetic_request() -> TestResult<RequestRecord> {
    Ok(RequestRecord::new(
        RequestId::from_str("request-1")?,
        RequestAction::Charge,
        RequestDirection::Incoming,
        User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("bob")?),
            Some("Synthetic User".to_owned()),
        ),
        Money::from_str("0.01")?,
        Some("request\u{202e}note".to_owned()),
        Some(time::OffsetDateTime::UNIX_EPOCH),
        RequestStatus::from_str("pending")?,
    ))
}

pub(super) fn payment_methods_args() -> TestResult<PaymentMethodsArgs> {
    match Cli::try_parse_from(["venmo", "payment-methods", "list"])?.command {
        Command::PaymentMethods(args) => Ok(args),
        _ => Err(io::Error::other("payment-method arguments parsed as another command").into()),
    }
}

pub(super) fn users_args() -> TestResult<UsersArgs> {
    match Cli::try_parse_from([
        "venmo", "users", "search", "alice", "--limit", "1", "--offset", "10",
    ])?
    .command
    {
        Command::Users(args) => Ok(args),
        _ => Err(io::Error::other("user arguments parsed as another command").into()),
    }
}

pub(super) fn friends_args() -> TestResult<FriendsArgs> {
    match Cli::try_parse_from(["venmo", "friends", "list", "--limit", "1", "--offset", "20"])?
        .command
    {
        Command::Friends(args) => Ok(args),
        _ => Err(io::Error::other("friend arguments parsed as another command").into()),
    }
}

pub(super) fn activity_list_args() -> TestResult<ActivityArgs> {
    match Cli::try_parse_from([
        "venmo",
        "activity",
        "list",
        "--limit",
        "2",
        "--before-id",
        "story-current",
    ])?
    .command
    {
        Command::Activity(args) => Ok(args),
        _ => Err(io::Error::other("activity-list arguments parsed as another command").into()),
    }
}

pub(super) fn activity_show_args() -> TestResult<ActivityArgs> {
    match Cli::try_parse_from(["venmo", "activity", "show", "story-1"])?.command {
        Command::Activity(args) => Ok(args),
        _ => Err(io::Error::other("activity-show arguments parsed as another command").into()),
    }
}

pub(super) fn requests_args() -> TestResult<RequestsArgs> {
    match Cli::try_parse_from([
        "venmo",
        "requests",
        "list",
        "--direction",
        "incoming",
        "--limit",
        "1",
        "--before",
        "request-current",
    ])?
    .command
    {
        Command::Requests(args) => Ok(args),
        _ => Err(io::Error::other("request-list arguments parsed as another command").into()),
    }
}

pub(super) const PAYMENT_METHODS_OUTPUT: &str = concat!(
    " ID       | NAME       | TYPE | LAST4 | DEFAULT\n",
    "----------+------------+------+-------+---------\n",
    " method-1 | Bank\\nname | bank | 1234  | yes\n",
);

pub(super) const USERS_OUTPUT: &str = concat!(
    " ID  | USERNAME | NAME\n",
    "-----+----------+-------------------\n",
    " 123 | @alice   | Alice\\u{001B}[31m\n",
);

pub(super) const FRIENDS_OUTPUT: &str = concat!(
    " ID  | USERNAME | NAME\n",
    "-----+----------+-----------\n",
    " 456 | @bob     | Bob\\nName\n",
);

pub(super) const ACTIVITY_LIST_OUTPUT: &str = concat!(
    " ID             | TIME                 | ACTION            | DIRECTION | COUNTERPARTY               | AMOUNT | STATUS | NOTE\n",
    "----------------+----------------------+-------------------+-----------+----------------------------+--------+--------+------------------------\n",
    " story-1        | 1970-01-01T00:00:00Z | pay               | outgoing  | @bob                       | $1.25  | failed | note\\n\\u{001B}[31mline\n",
    " story-transfer | 1970-01-01T00:00:00Z | transfer:standard | outgoing  | Bank\\nname (bank ••••1234) | $12.34 | issued |\n",
);

pub(super) const ACTIVITY_SHOW_OUTPUT: &str = concat!(
    "Activity ID: story-1\n",
    "Time: 1970-01-01T00:00:00Z\n",
    "Action: pay\n",
    "Direction: outgoing\n",
    "Counterparty: @bob\n",
    "Amount: $1.25\n",
    "Status: failed\n",
    "Note: note\\n\\u{001B}[31mline\n",
    "Audience: private\n",
);

pub(super) const REQUESTS_OUTPUT: &str = concat!(
    " ID        | DIRECTION | COUNTERPARTY | AMOUNT | CREATED              | STATUS  | NOTE\n",
    "-----------+-----------+--------------+--------+----------------------+---------+---------------------\n",
    " request-1 | incoming  | @bob         | $0.01  | 1970-01-01T00:00:00Z | pending | request\\u{202E}note\n",
);

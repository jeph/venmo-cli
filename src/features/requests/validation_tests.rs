use std::error::Error;
use std::str::FromStr;

use time::OffsetDateTime;

use super::*;
use crate::features::people::User;
use crate::features::requests::RequestStatus;
use crate::shared::{Money, UserId, Username};

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn complete_request_validation_table_covers_every_contract_variant() -> TestResult {
    let cases = [
        (
            request(
                "request-1",
                RequestAction::Charge,
                Some("note"),
                Some("private"),
                Some("requester"),
                Some(OffsetDateTime::UNIX_EPOCH),
            )?,
            Ok(()),
        ),
        (
            request(
                "request-1",
                RequestAction::Pay,
                Some("note"),
                Some("private"),
                Some("requester"),
                Some(OffsetDateTime::UNIX_EPOCH),
            )?,
            Err(RequestMutationValidationError::NotChargeRequest),
        ),
        (
            request(
                "request-1",
                RequestAction::Charge,
                None,
                Some("private"),
                Some("requester"),
                Some(OffsetDateTime::UNIX_EPOCH),
            )?,
            Err(RequestMutationValidationError::InvalidNote),
        ),
        (
            request(
                "request-1",
                RequestAction::Charge,
                Some(" \t "),
                Some("private"),
                Some("requester"),
                Some(OffsetDateTime::UNIX_EPOCH),
            )?,
            Err(RequestMutationValidationError::InvalidNote),
        ),
        (
            request(
                "request-1",
                RequestAction::Charge,
                Some("note"),
                None,
                Some("requester"),
                Some(OffsetDateTime::UNIX_EPOCH),
            )?,
            Err(RequestMutationValidationError::UnsupportedAudience),
        ),
        (
            request(
                "request-1",
                RequestAction::Charge,
                Some("note"),
                Some("unsupported"),
                Some("requester"),
                Some(OffsetDateTime::UNIX_EPOCH),
            )?,
            Err(RequestMutationValidationError::UnsupportedAudience),
        ),
        (
            request(
                "request-1",
                RequestAction::Charge,
                Some("note"),
                Some("private"),
                None,
                Some(OffsetDateTime::UNIX_EPOCH),
            )?,
            Err(RequestMutationValidationError::IncompleteRequester),
        ),
        (
            request(
                "request-1",
                RequestAction::Charge,
                Some("note"),
                Some("private"),
                Some("requester"),
                None,
            )?,
            Err(RequestMutationValidationError::MissingCreationTime),
        ),
    ];

    for (request, expected) in cases {
        let observed = validate_complete_request(&request);
        assert_eq!(observed, expected);
    }
    Ok(())
}

#[test]
fn id_direction_state_and_private_audience_tables_compare_whole_results() -> TestResult {
    let expected_id = RequestId::from_str("request-1")?;
    for (record_id, expected) in [
        ("request-1", Ok(())),
        (
            "different",
            Err(RequestMutationValidationError::MismatchedId),
        ),
    ] {
        let record = complete_request(record_id, RequestDirection::Incoming, "pending", "private")?;
        assert_eq!(validate_request_id(&record, &expected_id), expected);
    }

    for (direction, status, expected) in [
        (RequestDirection::Incoming, "pending", Ok(())),
        (
            RequestDirection::Outgoing,
            "pending",
            Err(RequestMutationValidationError::NotIncoming),
        ),
        (
            RequestDirection::Incoming,
            "held",
            Err(RequestMutationValidationError::NotPending),
        ),
        (
            RequestDirection::Incoming,
            "cancelled",
            Err(RequestMutationValidationError::NotPending),
        ),
        (
            RequestDirection::Incoming,
            "Pending",
            Err(RequestMutationValidationError::NotPending),
        ),
    ] {
        let record = complete_request("request-1", direction, status, "private")?;
        assert_eq!(validate_incoming_pending(&record), expected);
    }

    for (direction, status, expected) in [
        (RequestDirection::Outgoing, "pending", Ok(())),
        (RequestDirection::Outgoing, "held", Ok(())),
        (
            RequestDirection::Incoming,
            "pending",
            Err(RequestMutationValidationError::NotOutgoing),
        ),
        (
            RequestDirection::Outgoing,
            "cancelled",
            Err(RequestMutationValidationError::NotOpen),
        ),
        (
            RequestDirection::Outgoing,
            "settled",
            Err(RequestMutationValidationError::NotOpen),
        ),
    ] {
        let record = complete_request("request-1", direction, status, "private")?;
        assert_eq!(validate_outgoing_open(&record), expected);
    }

    for (audience, expected) in [
        ("private", Ok(())),
        (
            "friends",
            Err(RequestMutationValidationError::UnverifiedAudience),
        ),
        (
            "public",
            Err(RequestMutationValidationError::UnverifiedAudience),
        ),
    ] {
        let record =
            complete_request("request-1", RequestDirection::Incoming, "pending", audience)?;
        assert_eq!(validate_private_audience(&record), expected);
    }
    let missing_audience = request(
        "request-1",
        RequestAction::Charge,
        Some("note"),
        None,
        Some("requester"),
        Some(OffsetDateTime::UNIX_EPOCH),
    )?;
    assert_eq!(
        validate_private_audience(&missing_audience),
        Err(RequestMutationValidationError::UnverifiedAudience)
    );
    Ok(())
}

fn complete_request(
    id: &str,
    direction: RequestDirection,
    status: &str,
    audience: &str,
) -> Result<RequestRecord, Box<dyn Error>> {
    Ok(RequestRecord::new(
        RequestId::from_str(id)?,
        RequestAction::Charge,
        direction,
        user(Some("requester"))?,
        Money::from_cents(1)?,
        Some("note".to_owned()),
        Some(OffsetDateTime::UNIX_EPOCH),
        RequestStatus::from_str(status)?,
    )
    .with_audience(Some(audience.to_owned())))
}

fn request(
    id: &str,
    action: RequestAction,
    note: Option<&str>,
    audience: Option<&str>,
    username: Option<&str>,
    created_at: Option<OffsetDateTime>,
) -> Result<RequestRecord, Box<dyn Error>> {
    Ok(RequestRecord::new(
        RequestId::from_str(id)?,
        action,
        RequestDirection::Incoming,
        user(username)?,
        Money::from_cents(1)?,
        note.map(str::to_owned),
        created_at,
        RequestStatus::from_str("pending")?,
    )
    .with_audience(audience.map(str::to_owned)))
}

fn user(username: Option<&str>) -> Result<User, Box<dyn Error>> {
    Ok(User::new(
        UserId::from_str("456")?,
        username.map(Username::from_bare).transpose()?,
        Some("Synthetic requester".to_owned()),
    ))
}

use std::fmt;
use std::str::FromStr;

use thiserror::Error;
use time::OffsetDateTime;

use crate::features::people::User;
use crate::shared::Money;
use crate::shared::opaque_id::opaque_id;

opaque_id!(RequestId, "request ID");

const MAX_REQUEST_STATUS_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestDirection {
    Incoming,
    Outgoing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestAction {
    Charge,
    Pay,
}

impl fmt::Display for RequestDirection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Incoming => "incoming",
            Self::Outgoing => "outgoing",
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RequestDirectionFilter {
    #[default]
    All,
    Incoming,
    Outgoing,
}

impl RequestDirectionFilter {
    #[must_use]
    pub const fn matches(self, direction: RequestDirection) -> bool {
        matches!(self, Self::All)
            || matches!(
                (self, direction),
                (Self::Incoming, RequestDirection::Incoming)
                    | (Self::Outgoing, RequestDirection::Outgoing)
            )
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RequestStatus(String);

impl RequestStatus {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn is_pending_or_held(&self) -> bool {
        matches!(self.0.as_str(), "pending" | "held")
    }
}

impl fmt::Display for RequestStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for RequestStatus {
    type Err = RequestStatusParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() {
            return Err(RequestStatusParseError::Empty);
        }
        if value.len() > MAX_REQUEST_STATUS_BYTES {
            return Err(RequestStatusParseError::TooLong {
                maximum_bytes: MAX_REQUEST_STATUS_BYTES,
            });
        }
        if value.chars().any(char::is_control) {
            return Err(RequestStatusParseError::ControlCharacter);
        }
        Ok(Self(value.to_owned()))
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum RequestStatusParseError {
    #[error("request status must not be empty")]
    Empty,

    #[error("request status must not exceed {maximum_bytes} bytes")]
    TooLong { maximum_bytes: usize },

    #[error("request status must not contain control characters")]
    ControlCharacter,
}

#[derive(Clone, Eq, PartialEq)]
pub struct RequestRecord {
    id: RequestId,
    action: RequestAction,
    direction: RequestDirection,
    counterparty: User,
    amount: Money,
    note: Option<String>,
    audience: Option<String>,
    created_at: Option<OffsetDateTime>,
    status: RequestStatus,
}

impl RequestRecord {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        id: RequestId,
        action: RequestAction,
        direction: RequestDirection,
        counterparty: User,
        amount: Money,
        note: Option<String>,
        created_at: Option<OffsetDateTime>,
        status: RequestStatus,
    ) -> Self {
        Self {
            id,
            action,
            direction,
            counterparty,
            amount,
            note,
            audience: None,
            created_at,
            status,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &RequestId {
        &self.id
    }

    #[must_use]
    pub const fn action(&self) -> RequestAction {
        self.action
    }

    #[must_use]
    pub const fn direction(&self) -> RequestDirection {
        self.direction
    }

    #[must_use]
    pub const fn counterparty(&self) -> &User {
        &self.counterparty
    }

    #[must_use]
    pub(crate) fn with_counterparty(self, counterparty: User) -> Self {
        Self {
            counterparty,
            ..self
        }
    }

    #[must_use]
    pub const fn amount(&self) -> Money {
        self.amount
    }

    #[must_use]
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    #[must_use]
    pub(crate) fn with_audience(self, audience: Option<String>) -> Self {
        Self { audience, ..self }
    }

    #[must_use]
    pub fn audience(&self) -> Option<&str> {
        self.audience.as_deref()
    }

    #[must_use]
    pub const fn created_at(&self) -> Option<OffsetDateTime> {
        self.created_at
    }

    #[must_use]
    pub const fn status(&self) -> &RequestStatus {
        &self.status
    }
}

impl fmt::Debug for RequestRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RequestRecord")
            .field("id", &"[REDACTED]")
            .field("action", &self.action)
            .field("direction", &self.direction)
            .field("counterparty", &"[REDACTED]")
            .field("amount", &"[REDACTED]")
            .field("note", &"[REDACTED]")
            .field("audience", &"[REDACTED]")
            .field("created_at", &"[REDACTED]")
            .field("status", &self.status)
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct CreatedRequest {
    id: RequestId,
    status: RequestStatus,
}

impl CreatedRequest {
    #[must_use]
    pub(crate) const fn new(id: RequestId, status: RequestStatus) -> Self {
        Self { id, status }
    }

    #[must_use]
    pub const fn id(&self) -> &RequestId {
        &self.id
    }

    #[must_use]
    pub const fn status(&self) -> &RequestStatus {
        &self.status
    }
}

impl fmt::Debug for CreatedRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CreatedRequest")
            .field("id", &"[REDACTED]")
            .field("status", &self.status)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::{UserId, Username};

    #[test]
    fn request_status_has_exact_byte_unicode_control_and_state_boundaries()
    -> Result<(), Box<dyn std::error::Error>> {
        for value in [
            "x".repeat(MAX_REQUEST_STATUS_BYTES),
            "é".repeat(MAX_REQUEST_STATUS_BYTES / 2),
        ] {
            assert_eq!(value.len(), MAX_REQUEST_STATUS_BYTES);
            assert_eq!(RequestStatus::from_str(&value)?.as_str(), value);
        }
        for value in [
            "x".repeat(MAX_REQUEST_STATUS_BYTES + 1),
            format!("{}a", "é".repeat(MAX_REQUEST_STATUS_BYTES / 2)),
        ] {
            assert_eq!(
                RequestStatus::from_str(&value),
                Err(RequestStatusParseError::TooLong {
                    maximum_bytes: MAX_REQUEST_STATUS_BYTES
                })
            );
        }
        assert_eq!(
            RequestStatus::from_str(""),
            Err(RequestStatusParseError::Empty)
        );
        for value in ["line\nbreak", "zero\u{0}byte"] {
            assert_eq!(
                RequestStatus::from_str(value),
                Err(RequestStatusParseError::ControlCharacter)
            );
        }
        assert!(RequestStatus::from_str("pending")?.is_pending_or_held());
        assert!(RequestStatus::from_str("held")?.is_pending_or_held());
        assert!(!RequestStatus::from_str("Pending")?.is_pending_or_held());
        assert!(!RequestStatus::from_str("settled")?.is_pending_or_held());
        Ok(())
    }

    #[test]
    fn request_record_constructor_preserves_action_direction_identity_and_optional_fields()
    -> Result<(), Box<dyn std::error::Error>> {
        let counterparty = User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("requester")?),
            Some("Requester".to_owned()),
        );
        let record = RequestRecord::new(
            RequestId::from_str("request-1")?,
            RequestAction::Charge,
            RequestDirection::Incoming,
            counterparty.clone(),
            Money::from_cents(1)?,
            Some("private note".to_owned()),
            Some(OffsetDateTime::UNIX_EPOCH),
            RequestStatus::from_str("pending")?,
        )
        .with_audience(Some("private".to_owned()));
        assert_eq!(record.id().as_str(), "request-1");
        assert_eq!(record.action(), RequestAction::Charge);
        assert_eq!(record.direction(), RequestDirection::Incoming);
        assert_eq!(record.counterparty(), &counterparty);
        assert_eq!(record.amount().cents(), 1);
        assert_eq!(record.note(), Some("private note"));
        assert_eq!(record.audience(), Some("private"));
        assert_eq!(record.created_at(), Some(OffsetDateTime::UNIX_EPOCH));
        assert_eq!(record.status().as_str(), "pending");
        let rendered = format!("{record:?}");
        assert!(!rendered.contains("request-1"));
        assert!(!rendered.contains("private note"));

        let minimal = RequestRecord::new(
            RequestId::from_str("request-2")?,
            RequestAction::Pay,
            RequestDirection::Outgoing,
            counterparty,
            Money::from_cents(2)?,
            None,
            None,
            RequestStatus::from_str("settled")?,
        );
        assert_eq!(minimal.action(), RequestAction::Pay);
        assert_eq!(minimal.direction(), RequestDirection::Outgoing);
        assert!(minimal.note().is_none());
        assert!(minimal.audience().is_none());
        assert!(minimal.created_at().is_none());
        Ok(())
    }

    #[test]
    fn created_request_is_a_request_owned_whole_value() -> Result<(), Box<dyn std::error::Error>> {
        let created = CreatedRequest::new(
            RequestId::from_str("request-123")?,
            RequestStatus::from_str("pending")?,
        );

        assert_eq!(created.id().as_str(), "request-123");
        assert_eq!(created.status().as_str(), "pending");
        assert_eq!(
            format!("{created:?}"),
            "CreatedRequest { id: \"[REDACTED]\", status: RequestStatus(\"pending\") }"
        );
        Ok(())
    }
}

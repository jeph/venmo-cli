use std::fmt;
use std::str::FromStr;

use thiserror::Error;
use time::OffsetDateTime;

use super::{Money, RequestId, User};

const MAX_REQUEST_STATUS_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestDirection {
    Incoming,
    Outgoing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PendingRequestAction {
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
    pub fn is_pending_record(&self) -> bool {
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
pub struct PendingRequest {
    id: RequestId,
    action: PendingRequestAction,
    direction: RequestDirection,
    counterparty: User,
    amount: Money,
    note: Option<String>,
    audience: Option<String>,
    created_at: Option<OffsetDateTime>,
    status: RequestStatus,
}

impl PendingRequest {
    #[must_use]
    pub fn new(
        id: RequestId,
        direction: RequestDirection,
        counterparty: User,
        amount: Money,
        note: Option<String>,
        created_at: Option<OffsetDateTime>,
        status: RequestStatus,
    ) -> Self {
        Self {
            id,
            action: PendingRequestAction::Charge,
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
    pub(crate) fn with_action(mut self, action: PendingRequestAction) -> Self {
        self.action = action;
        self
    }

    #[must_use]
    pub const fn action(&self) -> PendingRequestAction {
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
    pub(crate) fn with_counterparty(mut self, counterparty: User) -> Self {
        self.counterparty = counterparty;
        self
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
    #[cfg(test)]
    pub(crate) fn with_note(mut self, note: Option<String>) -> Self {
        self.note = note;
        self
    }

    #[must_use]
    pub(crate) fn with_audience(mut self, audience: Option<String>) -> Self {
        self.audience = audience;
        self
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

impl fmt::Debug for PendingRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PendingRequest")
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

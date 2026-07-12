use std::fmt;
use std::str::FromStr;

use thiserror::Error;
use time::OffsetDateTime;

use super::{ActivityId, Money, User};

const MAX_ACTIVITY_LABEL_BYTES: usize = 64;

macro_rules! activity_label {
    ($name:ident, $kind:literal) => {
        #[derive(Clone, Debug, Eq, Hash, PartialEq)]
        pub struct $name(String);

        impl $name {
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = ActivityLabelParseError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                validate_activity_label(value, $kind)?;
                Ok(Self(value.to_owned()))
            }
        }
    };
}

fn validate_activity_label(value: &str, kind: &'static str) -> Result<(), ActivityLabelParseError> {
    if value.is_empty() {
        return Err(ActivityLabelParseError::Empty { kind });
    }
    if value.len() > MAX_ACTIVITY_LABEL_BYTES {
        return Err(ActivityLabelParseError::TooLong {
            kind,
            maximum_bytes: MAX_ACTIVITY_LABEL_BYTES,
        });
    }
    if value.chars().any(char::is_control) {
        return Err(ActivityLabelParseError::ControlCharacter { kind });
    }
    Ok(())
}

activity_label!(ActivityAction, "activity action");
activity_label!(ActivityStatus, "activity status");

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ActivityLabelParseError {
    #[error("{kind} must not be empty")]
    Empty { kind: &'static str },

    #[error("{kind} must not exceed {maximum_bytes} bytes")]
    TooLong {
        kind: &'static str,
        maximum_bytes: usize,
    },

    #[error("{kind} must not contain control characters")]
    ControlCharacter { kind: &'static str },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivityDirection {
    Incoming,
    Outgoing,
}

#[derive(Clone, Eq, PartialEq)]
pub enum ActivityCounterparty {
    User(User),
    External {
        name: String,
        kind: String,
        last_four: Option<String>,
    },
}

impl ActivityCounterparty {
    #[must_use]
    pub const fn user(user: User) -> Self {
        Self::User(user)
    }

    #[must_use]
    pub fn external(name: String, kind: String, last_four: Option<String>) -> Self {
        Self::External {
            name,
            kind,
            last_four,
        }
    }

    #[must_use]
    pub const fn as_user(&self) -> Option<&User> {
        match self {
            Self::User(user) => Some(user),
            Self::External { .. } => None,
        }
    }

    #[must_use]
    pub fn external_parts(&self) -> Option<(&str, &str, Option<&str>)> {
        match self {
            Self::User(_) => None,
            Self::External {
                name,
                kind,
                last_four,
            } => Some((name, kind, last_four.as_deref())),
        }
    }
}

impl fmt::Debug for ActivityCounterparty {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::User(_) => "ActivityCounterparty::User([REDACTED])",
            Self::External { .. } => "ActivityCounterparty::External([REDACTED])",
        })
    }
}

impl fmt::Display for ActivityDirection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Incoming => "incoming",
            Self::Outgoing => "outgoing",
        })
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct Activity {
    id: ActivityId,
    occurred_at: OffsetDateTime,
    action: ActivityAction,
    direction: ActivityDirection,
    counterparty: ActivityCounterparty,
    amount: Money,
    status: ActivityStatus,
    note: Option<String>,
    audience: Option<String>,
}

impl Activity {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        id: ActivityId,
        occurred_at: OffsetDateTime,
        action: ActivityAction,
        direction: ActivityDirection,
        counterparty: ActivityCounterparty,
        amount: Money,
        status: ActivityStatus,
        note: Option<String>,
        audience: Option<String>,
    ) -> Self {
        Self {
            id,
            occurred_at,
            action,
            direction,
            counterparty,
            amount,
            status,
            note,
            audience,
        }
    }

    #[must_use]
    pub fn id(&self) -> &ActivityId {
        &self.id
    }

    #[must_use]
    pub const fn occurred_at(&self) -> OffsetDateTime {
        self.occurred_at
    }

    #[must_use]
    pub fn action(&self) -> &ActivityAction {
        &self.action
    }

    #[must_use]
    pub const fn direction(&self) -> ActivityDirection {
        self.direction
    }

    #[must_use]
    pub const fn counterparty(&self) -> &ActivityCounterparty {
        &self.counterparty
    }

    #[must_use]
    pub const fn amount(&self) -> Money {
        self.amount
    }

    #[must_use]
    pub const fn status(&self) -> &ActivityStatus {
        &self.status
    }

    #[must_use]
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    #[must_use]
    pub fn audience(&self) -> Option<&str> {
        self.audience.as_deref()
    }
}

impl fmt::Debug for Activity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Activity")
            .field("id", &"[REDACTED]")
            .field("occurred_at", &"[REDACTED]")
            .field("action", &self.action)
            .field("direction", &self.direction)
            .field("counterparty", &"[REDACTED]")
            .field("amount", &"[REDACTED]")
            .field("status", &self.status)
            .field("note", &"[REDACTED]")
            .field("audience", &self.audience)
            .finish()
    }
}

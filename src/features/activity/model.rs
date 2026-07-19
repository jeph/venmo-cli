use std::fmt;
use std::str::FromStr;

use thiserror::Error;
use time::OffsetDateTime;

use crate::features::people::User;
use crate::shared::opaque_id::opaque_id;
use crate::shared::{Money, UserId, Username};

opaque_id!(ActivityId, "activity ID");

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivityFeedKind {
    CurrentUser,
    OtherPersonalUser,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityFeedScope {
    viewer_user_id: UserId,
    subject_user_id: UserId,
    kind: ActivityFeedKind,
}

impl ActivityFeedScope {
    #[must_use]
    pub const fn new(
        viewer_user_id: UserId,
        subject_user_id: UserId,
        kind: ActivityFeedKind,
    ) -> Self {
        Self {
            viewer_user_id,
            subject_user_id,
            kind,
        }
    }

    #[must_use]
    pub const fn viewer_user_id(&self) -> &UserId {
        &self.viewer_user_id
    }

    #[must_use]
    pub const fn subject_user_id(&self) -> &UserId {
        &self.subject_user_id
    }

    #[must_use]
    pub const fn kind(&self) -> ActivityFeedKind {
        self.kind
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivitySubject {
    user_id: UserId,
    username: Username,
    kind: ActivityFeedKind,
}

impl ActivitySubject {
    #[must_use]
    pub const fn new(user_id: UserId, username: Username, kind: ActivityFeedKind) -> Self {
        Self {
            user_id,
            username,
            kind,
        }
    }

    #[must_use]
    pub const fn user_id(&self) -> &UserId {
        &self.user_id
    }

    #[must_use]
    pub const fn username(&self) -> &Username {
        &self.username
    }

    #[must_use]
    pub const fn kind(&self) -> ActivityFeedKind {
        self.kind
    }
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
    amount: Option<Money>,
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
        amount: Option<Money>,
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
    pub const fn amount(&self) -> Option<Money> {
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
            .field("audience", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub enum ActivityDetailParties {
    Payment {
        actor: User,
        target: User,
    },
    Relative {
        direction: ActivityDirection,
        counterparty: ActivityCounterparty,
    },
}

impl ActivityDetailParties {
    #[must_use]
    pub const fn payment_parties(&self) -> Option<(&User, &User)> {
        match self {
            Self::Payment { actor, target } => Some((actor, target)),
            Self::Relative { .. } => None,
        }
    }

    #[must_use]
    pub const fn relative_parts(&self) -> Option<(ActivityDirection, &ActivityCounterparty)> {
        match self {
            Self::Payment { .. } => None,
            Self::Relative {
                direction,
                counterparty,
            } => Some((*direction, counterparty)),
        }
    }
}

impl fmt::Debug for ActivityDetailParties {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Payment { .. } => "ActivityDetailParties::Payment([REDACTED])",
            Self::Relative { .. } => "ActivityDetailParties::Relative([REDACTED])",
        })
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct ActivityDetail {
    id: ActivityId,
    occurred_at: OffsetDateTime,
    action: ActivityAction,
    parties: ActivityDetailParties,
    amount: Option<Money>,
    status: ActivityStatus,
    note: Option<String>,
    audience: Option<String>,
}

impl ActivityDetail {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn payment(
        id: ActivityId,
        occurred_at: OffsetDateTime,
        action: ActivityAction,
        actor: User,
        target: User,
        amount: Option<Money>,
        status: ActivityStatus,
        note: Option<String>,
        audience: Option<String>,
    ) -> Self {
        Self {
            id,
            occurred_at,
            action,
            parties: ActivityDetailParties::Payment { actor, target },
            amount,
            status,
            note,
            audience,
        }
    }

    #[must_use]
    pub fn relative(activity: Activity) -> Self {
        Self {
            id: activity.id,
            occurred_at: activity.occurred_at,
            action: activity.action,
            parties: ActivityDetailParties::Relative {
                direction: activity.direction,
                counterparty: activity.counterparty,
            },
            amount: activity.amount,
            status: activity.status,
            note: activity.note,
            audience: activity.audience,
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
    pub const fn action(&self) -> &ActivityAction {
        &self.action
    }

    #[must_use]
    pub const fn parties(&self) -> &ActivityDetailParties {
        &self.parties
    }

    #[must_use]
    pub const fn amount(&self) -> Option<Money> {
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

impl fmt::Debug for ActivityDetail {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ActivityDetail")
            .field("id", &"[REDACTED]")
            .field("occurred_at", &"[REDACTED]")
            .field("action", &self.action)
            .field("parties", &self.parties)
            .field("amount", &"[REDACTED]")
            .field("status", &self.status)
            .field("note", &"[REDACTED]")
            .field("audience", &"[REDACTED]")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;
    use crate::shared::{UserId, Username};

    #[test]
    fn activity_action_and_status_have_exact_byte_unicode_and_control_boundaries()
    -> Result<(), Box<dyn Error>> {
        for value in [
            "x".repeat(MAX_ACTIVITY_LABEL_BYTES),
            "é".repeat(MAX_ACTIVITY_LABEL_BYTES / 2),
        ] {
            assert_eq!(value.len(), MAX_ACTIVITY_LABEL_BYTES);
            assert_eq!(ActivityAction::from_str(&value)?.as_str(), value);
            assert_eq!(ActivityStatus::from_str(&value)?.as_str(), value);
        }
        for value in [
            "x".repeat(MAX_ACTIVITY_LABEL_BYTES + 1),
            format!("{}a", "é".repeat(MAX_ACTIVITY_LABEL_BYTES / 2)),
        ] {
            assert!(matches!(
                ActivityAction::from_str(&value),
                Err(ActivityLabelParseError::TooLong {
                    maximum_bytes: MAX_ACTIVITY_LABEL_BYTES,
                    ..
                })
            ));
            assert!(matches!(
                ActivityStatus::from_str(&value),
                Err(ActivityLabelParseError::TooLong {
                    maximum_bytes: MAX_ACTIVITY_LABEL_BYTES,
                    ..
                })
            ));
        }
        for value in ["", "line\nbreak", "zero\u{0}byte"] {
            assert!(ActivityAction::from_str(value).is_err());
            assert!(ActivityStatus::from_str(value).is_err());
        }
        Ok(())
    }

    #[test]
    fn activity_and_counterparty_constructors_preserve_every_typed_field()
    -> Result<(), Box<dyn Error>> {
        let user = User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("bob")?),
            Some("Bob".to_owned()),
        );
        let activity = Activity::new(
            ActivityId::from_str("story-1")?,
            OffsetDateTime::UNIX_EPOCH,
            ActivityAction::from_str("pay")?,
            ActivityDirection::Outgoing,
            ActivityCounterparty::user(user.clone()),
            Some(Money::from_cents(125)?),
            ActivityStatus::from_str("settled")?,
            Some("private note".to_owned()),
            Some("private".to_owned()),
        );
        assert_eq!(activity.id().as_str(), "story-1");
        assert_eq!(activity.occurred_at(), OffsetDateTime::UNIX_EPOCH);
        assert_eq!(activity.action().as_str(), "pay");
        assert_eq!(activity.direction(), ActivityDirection::Outgoing);
        assert_eq!(activity.counterparty().as_user(), Some(&user));
        assert_eq!(activity.amount().map(Money::cents), Some(125));
        assert_eq!(activity.status().as_str(), "settled");
        assert_eq!(activity.note(), Some("private note"));
        assert_eq!(activity.audience(), Some("private"));
        let rendered = format!("{activity:?}");
        assert!(!rendered.contains("story-1"));
        assert!(!rendered.contains("private note"));

        let external = ActivityCounterparty::external(
            "Synthetic bank".to_owned(),
            "bank".to_owned(),
            Some("1234".to_owned()),
        );
        assert_eq!(
            external.external_parts(),
            Some(("Synthetic bank", "bank", Some("1234")))
        );
        assert!(external.as_user().is_none());
        assert!(!format!("{external:?}").contains("Synthetic bank"));
        Ok(())
    }
}

use std::fmt;
use std::str::FromStr;

use thiserror::Error;
use time::OffsetDateTime;
use unicode_properties::UnicodeEmoji;
use unicode_segmentation::UnicodeSegmentation;

use crate::features::people::User;
use crate::shared::opaque_id::opaque_id;
use crate::shared::{Money, UserId, Username};

opaque_id!(ActivityId, "activity ID");
opaque_id!(ActivityCommentId, "activity comment ID");

const MAX_ACTIVITY_LABEL_BYTES: usize = 64;
const MAX_COMMENT_CHARACTERS: usize = 2_000;
const MAX_COMMENT_BYTES: usize = MAX_COMMENT_CHARACTERS * 4;
pub const MAX_REACTION_EMOJI_BYTES: usize = 128;

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

#[derive(Clone, Eq, PartialEq)]
pub struct ActivityCommentMessage(String);

impl ActivityCommentMessage {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ActivityCommentMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ActivityCommentMessage([REDACTED])")
    }
}

impl FromStr for ActivityCommentMessage {
    type Err = ActivityCommentMessageParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if !value.chars().any(|character| !character.is_whitespace()) {
            return Err(ActivityCommentMessageParseError::Empty);
        }
        if value.len() > MAX_COMMENT_BYTES || value.chars().count() > MAX_COMMENT_CHARACTERS {
            return Err(ActivityCommentMessageParseError::TooLong {
                maximum_characters: MAX_COMMENT_CHARACTERS,
            });
        }
        Ok(Self(value.to_owned()))
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ActivityCommentMessageParseError {
    #[error("comment must contain non-whitespace text")]
    Empty,

    #[error("comment must not exceed {maximum_characters} characters")]
    TooLong { maximum_characters: usize },
}

#[derive(Clone, Eq, Hash, PartialEq)]
pub struct ActivityReactionEmoji(String);

impl ActivityReactionEmoji {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ActivityReactionEmoji {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl fmt::Debug for ActivityReactionEmoji {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ActivityReactionEmoji([REDACTED])")
    }
}

impl FromStr for ActivityReactionEmoji {
    type Err = ActivityReactionEmojiParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() {
            return Err(ActivityReactionEmojiParseError::Empty);
        }
        if value.len() > MAX_REACTION_EMOJI_BYTES {
            return Err(ActivityReactionEmojiParseError::TooLong {
                maximum_bytes: MAX_REACTION_EMOJI_BYTES,
            });
        }
        if value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
        {
            return Err(ActivityReactionEmojiParseError::WhitespaceOrControl);
        }
        let mut graphemes = value.graphemes(true);
        if graphemes.next() != Some(value) || graphemes.next().is_some() {
            return Err(ActivityReactionEmojiParseError::MultipleGraphemes);
        }
        if !value.chars().any(|character| character.is_emoji_char()) {
            return Err(ActivityReactionEmojiParseError::NotEmoji);
        }
        let value = if value == "❤" { "❤️" } else { value };
        Ok(Self(value.to_owned()))
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ActivityReactionEmojiParseError {
    #[error("reaction emoji must not be empty")]
    Empty,

    #[error("reaction emoji must be one Unicode grapheme cluster")]
    MultipleGraphemes,

    #[error("reaction must contain a Unicode emoji character")]
    NotEmoji,

    #[error("reaction emoji must not contain whitespace or control characters")]
    WhitespaceOrControl,

    #[error("reaction emoji must not exceed {maximum_bytes} bytes")]
    TooLong { maximum_bytes: usize },
}

#[derive(Clone, Eq, PartialEq)]
pub struct ActivityReaction {
    emoji: ActivityReactionEmoji,
    count: u64,
    reacted_by_current_user: bool,
}

impl ActivityReaction {
    #[must_use]
    pub const fn new(
        emoji: ActivityReactionEmoji,
        count: u64,
        reacted_by_current_user: bool,
    ) -> Self {
        Self {
            emoji,
            count,
            reacted_by_current_user,
        }
    }

    #[must_use]
    pub const fn emoji(&self) -> &ActivityReactionEmoji {
        &self.emoji
    }

    #[must_use]
    pub const fn count(&self) -> u64 {
        self.count
    }

    #[must_use]
    pub const fn reacted_by_current_user(&self) -> bool {
        self.reacted_by_current_user
    }
}

impl fmt::Debug for ActivityReaction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ActivityReaction")
            .field("emoji", &"[REDACTED]")
            .field("count", &self.count)
            .field("reacted_by_current_user", &self.reacted_by_current_user)
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct ActivityReactions {
    total_count: u64,
    items: Vec<ActivityReaction>,
}

impl ActivityReactions {
    pub fn try_new(items: Vec<ActivityReaction>) -> Result<Self, ActivityReactionsError> {
        let total_count = items.iter().try_fold(0_u64, |total, reaction| {
            total
                .checked_add(reaction.count())
                .ok_or(ActivityReactionsError::CountOverflow)
        })?;
        Ok(Self { total_count, items })
    }

    #[must_use]
    pub const fn total_count(&self) -> u64 {
        self.total_count
    }

    #[must_use]
    pub fn items(&self) -> &[ActivityReaction] {
        &self.items
    }

    #[must_use]
    pub fn state(&self, emoji: &ActivityReactionEmoji) -> ActivityReactionState {
        self.items
            .iter()
            .find(|reaction| reaction.emoji() == emoji)
            .map_or(ActivityReactionState::Absent, |reaction| {
                if reaction.reacted_by_current_user() {
                    ActivityReactionState::Present
                } else {
                    ActivityReactionState::Absent
                }
            })
    }
}

impl fmt::Debug for ActivityReactions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ActivityReactions")
            .field("total_count", &self.total_count)
            .field(
                "items",
                &format_args!("{} redacted item(s)", self.items.len()),
            )
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ActivityReactionsError {
    #[error("reaction counts exceed the supported aggregate range")]
    CountOverflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivityReactionState {
    Present,
    Absent,
    Unknown,
}

#[derive(Clone, Eq, PartialEq)]
pub struct ActivityComment {
    id: ActivityCommentId,
    author: User,
    message: String,
    created_at: OffsetDateTime,
}

impl ActivityComment {
    #[must_use]
    pub fn new(
        id: ActivityCommentId,
        author: User,
        message: String,
        created_at: OffsetDateTime,
    ) -> Self {
        Self {
            id,
            author,
            message,
            created_at,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &ActivityCommentId {
        &self.id
    }

    #[must_use]
    pub const fn author(&self) -> &User {
        &self.author
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    #[must_use]
    pub const fn created_at(&self) -> OffsetDateTime {
        self.created_at
    }
}

impl fmt::Debug for ActivityComment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ActivityComment")
            .field("id", &"[REDACTED]")
            .field("author", &"[REDACTED]")
            .field("message", &"[REDACTED]")
            .field("created_at", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct ActivitySocialCollection<T> {
    count: u64,
    items: Vec<T>,
    complete: bool,
}

impl<T> ActivitySocialCollection<T> {
    #[must_use]
    pub fn new(count: u64, items: Vec<T>, complete: bool) -> Self {
        Self {
            count,
            items,
            complete,
        }
    }

    #[must_use]
    pub const fn count(&self) -> u64 {
        self.count
    }

    #[must_use]
    pub fn items(&self) -> &[T] {
        &self.items
    }

    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.complete
    }
}

impl<T> fmt::Debug for ActivitySocialCollection<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ActivitySocialCollection")
            .field("count", &self.count)
            .field(
                "items",
                &format_args!("{} redacted item(s)", self.items.len()),
            )
            .field("complete", &self.complete)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivityLikeState {
    Liked,
    NotLiked,
    Unknown,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ActivitySocial {
    likes: Option<ActivitySocialCollection<User>>,
    comments: Option<ActivitySocialCollection<ActivityComment>>,
    reactions: Option<ActivityReactions>,
}

impl ActivitySocial {
    #[must_use]
    pub fn new(
        likes: Option<ActivitySocialCollection<User>>,
        comments: Option<ActivitySocialCollection<ActivityComment>>,
    ) -> Self {
        Self {
            likes,
            comments,
            reactions: None,
        }
    }

    #[must_use]
    pub fn with_reactions(self, reactions: Option<ActivityReactions>) -> Self {
        Self { reactions, ..self }
    }

    #[must_use]
    pub const fn likes(&self) -> Option<&ActivitySocialCollection<User>> {
        self.likes.as_ref()
    }

    #[must_use]
    pub const fn comments(&self) -> Option<&ActivitySocialCollection<ActivityComment>> {
        self.comments.as_ref()
    }

    #[must_use]
    pub const fn reactions(&self) -> Option<&ActivityReactions> {
        self.reactions.as_ref()
    }

    #[must_use]
    pub fn reaction_state(&self, emoji: &ActivityReactionEmoji) -> ActivityReactionState {
        self.reactions
            .as_ref()
            .map_or(ActivityReactionState::Unknown, |reactions| {
                reactions.state(emoji)
            })
    }

    #[must_use]
    pub fn like_state(&self, current_user_id: &UserId) -> ActivityLikeState {
        let Some(likes) = &self.likes else {
            return ActivityLikeState::Unknown;
        };
        if likes
            .items()
            .iter()
            .any(|user| user.user_id() == current_user_id)
        {
            ActivityLikeState::Liked
        } else if likes.is_complete() {
            ActivityLikeState::NotLiked
        } else {
            ActivityLikeState::Unknown
        }
    }
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
    status: Option<ActivityStatus>,
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
        status: Option<ActivityStatus>,
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
    pub const fn status(&self) -> Option<&ActivityStatus> {
        self.status.as_ref()
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
    Account {
        account: User,
        direction: ActivityDirection,
        counterparty: ActivityCounterparty,
    },
}

impl ActivityDetailParties {
    #[must_use]
    pub const fn payment_parties(&self) -> Option<(&User, &User)> {
        match self {
            Self::Payment { actor, target } => Some((actor, target)),
            Self::Relative { .. } | Self::Account { .. } => None,
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
            Self::Account { .. } => None,
        }
    }

    #[must_use]
    pub const fn account_parts(&self) -> Option<(&User, ActivityDirection, &ActivityCounterparty)> {
        match self {
            Self::Account {
                account,
                direction,
                counterparty,
            } => Some((account, *direction, counterparty)),
            Self::Payment { .. } | Self::Relative { .. } => None,
        }
    }
}

impl fmt::Debug for ActivityDetailParties {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Payment { .. } => "ActivityDetailParties::Payment([REDACTED])",
            Self::Relative { .. } => "ActivityDetailParties::Relative([REDACTED])",
            Self::Account { .. } => "ActivityDetailParties::Account([REDACTED])",
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
    status: Option<ActivityStatus>,
    note: Option<String>,
    audience: Option<String>,
    social: ActivitySocial,
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
        status: Option<ActivityStatus>,
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
            social: ActivitySocial::default(),
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
            social: ActivitySocial::default(),
        }
    }

    #[must_use]
    pub fn account(activity: Activity, account: User) -> Self {
        Self {
            id: activity.id,
            occurred_at: activity.occurred_at,
            action: activity.action,
            parties: ActivityDetailParties::Account {
                account,
                direction: activity.direction,
                counterparty: activity.counterparty,
            },
            amount: activity.amount,
            status: activity.status,
            note: activity.note,
            audience: activity.audience,
            social: ActivitySocial::default(),
        }
    }

    #[must_use]
    pub fn with_social(self, social: ActivitySocial) -> Self {
        Self { social, ..self }
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
    pub const fn status(&self) -> Option<&ActivityStatus> {
        self.status.as_ref()
    }

    #[must_use]
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    #[must_use]
    pub fn audience(&self) -> Option<&str> {
        self.audience.as_deref()
    }

    #[must_use]
    pub const fn social(&self) -> &ActivitySocial {
        &self.social
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
            .field("social", &self.social)
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
            Some(ActivityStatus::from_str("settled")?),
            Some("private note".to_owned()),
            Some("private".to_owned()),
        );
        assert_eq!(activity.id().as_str(), "story-1");
        assert_eq!(activity.occurred_at(), OffsetDateTime::UNIX_EPOCH);
        assert_eq!(activity.action().as_str(), "pay");
        assert_eq!(activity.direction(), ActivityDirection::Outgoing);
        assert_eq!(activity.counterparty().as_user(), Some(&user));
        assert_eq!(activity.amount().map(Money::cents), Some(125));
        assert_eq!(
            activity.status().map(ActivityStatus::as_str),
            Some("settled")
        );
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

    #[test]
    fn comment_messages_enforce_native_nonblank_two_thousand_character_limit() {
        for value in ["", " \t\n"] {
            assert_eq!(
                ActivityCommentMessage::from_str(value),
                Err(ActivityCommentMessageParseError::Empty)
            );
        }
        for value in ["hello".to_owned(), "🍜".repeat(MAX_COMMENT_CHARACTERS)] {
            assert_eq!(
                ActivityCommentMessage::from_str(&value).map(|message| message.as_str().to_owned()),
                Ok(value)
            );
        }
        assert!(matches!(
            ActivityCommentMessage::from_str(&"x".repeat(MAX_COMMENT_CHARACTERS + 1)),
            Err(ActivityCommentMessageParseError::TooLong {
                maximum_characters: MAX_COMMENT_CHARACTERS
            })
        ));
    }

    #[test]
    fn reaction_emoji_preserves_one_bounded_unicode_grapheme_and_redacts_debug()
    -> Result<(), Box<dyn Error>> {
        for value in ["🔥", "❤️", "👨🏽‍💻", "🇺🇸", "1️⃣"] {
            let emoji = ActivityReactionEmoji::from_str(value)?;
            assert_eq!(emoji.as_str(), value);
            assert!(!format!("{emoji:?}").contains(value));
        }
        assert_eq!(ActivityReactionEmoji::from_str("❤")?.as_str(), "❤️");
        assert_eq!(
            ActivityReactionEmoji::from_str(""),
            Err(ActivityReactionEmojiParseError::Empty)
        );
        assert_eq!(
            ActivityReactionEmoji::from_str("🔥❤️"),
            Err(ActivityReactionEmojiParseError::MultipleGraphemes)
        );
        assert_eq!(
            ActivityReactionEmoji::from_str("A"),
            Err(ActivityReactionEmojiParseError::NotEmoji)
        );
        assert_eq!(
            ActivityReactionEmoji::from_str("🔥 ❤️"),
            Err(ActivityReactionEmojiParseError::WhitespaceOrControl)
        );
        assert_eq!(
            ActivityReactionEmoji::from_str("🔥\n"),
            Err(ActivityReactionEmojiParseError::WhitespaceOrControl)
        );
        assert!(matches!(
            ActivityReactionEmoji::from_str(&format!("🔥{}", "\u{fe0f}".repeat(128))),
            Err(ActivityReactionEmojiParseError::TooLong {
                maximum_bytes: MAX_REACTION_EMOJI_BYTES
            })
        ));
        Ok(())
    }

    #[test]
    fn reaction_aggregates_preserve_counts_and_evidence_sensitive_state()
    -> Result<(), Box<dyn Error>> {
        let fire = ActivityReactionEmoji::from_str("🔥")?;
        let heart = ActivityReactionEmoji::from_str("❤️")?;
        let reactions = ActivityReactions::try_new(vec![
            ActivityReaction::new(fire.clone(), 2, true),
            ActivityReaction::new(heart.clone(), 1, false),
        ])?;
        assert_eq!(reactions.total_count(), 3);
        assert_eq!(reactions.state(&fire), ActivityReactionState::Present);
        assert_eq!(reactions.state(&heart), ActivityReactionState::Absent);
        assert_eq!(
            reactions.state(&ActivityReactionEmoji::from_str("👍")?),
            ActivityReactionState::Absent
        );
        assert_eq!(
            ActivitySocial::default().reaction_state(&fire),
            ActivityReactionState::Unknown
        );
        assert_eq!(
            ActivitySocial::new(None, None)
                .with_reactions(Some(reactions))
                .reaction_state(&fire),
            ActivityReactionState::Present
        );
        assert_eq!(
            ActivityReactions::try_new(vec![
                ActivityReaction::new(fire, u64::MAX, false),
                ActivityReaction::new(heart, 1, false),
            ]),
            Err(ActivityReactionsError::CountOverflow)
        );
        Ok(())
    }

    #[test]
    fn like_state_is_evidence_sensitive() -> Result<(), Box<dyn Error>> {
        let current_user_id = UserId::from_str("123")?;
        let owner = User::new(current_user_id.clone(), None, None);
        let other = User::new(UserId::from_str("456")?, None, None);
        let unavailable = ActivitySocial::default();
        let partial = ActivitySocial::new(
            Some(ActivitySocialCollection::new(2, vec![other.clone()], false)),
            None,
        );
        let complete = ActivitySocial::new(
            Some(ActivitySocialCollection::new(1, vec![other], true)),
            None,
        );
        let liked = ActivitySocial::new(
            Some(ActivitySocialCollection::new(1, vec![owner], true)),
            None,
        );

        assert_eq!(
            unavailable.like_state(&current_user_id),
            ActivityLikeState::Unknown
        );
        assert_eq!(
            partial.like_state(&current_user_id),
            ActivityLikeState::Unknown
        );
        assert_eq!(
            complete.like_state(&current_user_id),
            ActivityLikeState::NotLiked
        );
        assert_eq!(liked.like_state(&current_user_id), ActivityLikeState::Liked);
        Ok(())
    }
}

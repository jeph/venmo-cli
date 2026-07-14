use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use super::User;
use crate::shared::{UserId, UserIdParseError, Username, UsernameParseError};

const REDACTED: &str = "[REDACTED]";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecipientInput {
    Username(Username),
    UserId(UserId),
}

#[derive(Clone, Eq, PartialEq)]
pub struct ResolvedRecipient {
    user: User,
}

impl ResolvedRecipient {
    #[must_use]
    pub(crate) fn new(user: User) -> Self {
        Self { user }
    }

    #[must_use]
    pub fn into_user(self) -> User {
        self.user
    }
}

impl fmt::Debug for ResolvedRecipient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResolvedRecipient")
            .field("user", &REDACTED)
            .finish()
    }
}

impl fmt::Display for RecipientInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Username(username) => username.fmt(formatter),
            Self::UserId(user_id) => user_id.fmt(formatter),
        }
    }
}

impl FromStr for RecipientInput {
    type Err = RecipientParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.starts_with('@') {
            return value
                .parse::<Username>()
                .map(Self::Username)
                .map_err(RecipientParseError::Username);
        }

        value
            .parse::<UserId>()
            .map(Self::UserId)
            .map_err(RecipientParseError::UserId)
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum RecipientParseError {
    #[error("invalid username recipient: {0}")]
    Username(UsernameParseError),

    #[error("recipient must be an exact @username or positive numeric Venmo user ID: {0}")]
    UserId(UserIdParseError),
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;
    use crate::features::people::UserProfileKind;

    #[test]
    fn recipient_parser_covers_exact_username_byte_unicode_control_and_numeric_boundaries()
    -> Result<(), Box<dyn Error>> {
        let exact_ascii = format!("@{}", "a".repeat(1023));
        let exact_unicode = format!("@{}a", "é".repeat(511));
        assert_eq!(exact_ascii.len(), 1024);
        assert_eq!(exact_unicode.len(), 1024);
        for value in [exact_ascii.as_str(), exact_unicode.as_str(), "@élise"] {
            assert!(matches!(
                RecipientInput::from_str(value),
                Ok(RecipientInput::Username(_))
            ));
        }

        for value in [
            format!("@{}", "a".repeat(1024)),
            format!("@{}", "é".repeat(512)),
            "@white space".to_owned(),
            "@line\nbreak".to_owned(),
        ] {
            assert!(matches!(
                RecipientInput::from_str(&value),
                Err(RecipientParseError::Username(_))
            ));
        }

        for value in ["1", "0001"] {
            let recipient = RecipientInput::from_str(value)?;
            assert!(matches!(
                recipient,
                RecipientInput::UserId(ref id) if id.as_str() == value
            ));
        }
        for value in ["", "alice", "0", "１２", "+1", "1\u{0}2"] {
            assert!(matches!(
                RecipientInput::from_str(value),
                Err(RecipientParseError::UserId(_))
            ));
        }
        Ok(())
    }

    #[test]
    fn resolved_recipient_constructor_preserves_the_authoritative_whole_user()
    -> Result<(), Box<dyn Error>> {
        let user = User::new(
            UserId::from_str("123")?,
            Some(Username::from_bare("alice")?),
            Some("Alice".to_owned()),
        )
        .with_financial_attributes(UserProfileKind::Personal, true);
        let expected = user.clone();
        let resolved = ResolvedRecipient::new(user);
        assert_eq!(resolved.clone().into_user(), expected);
        assert!(!format!("{resolved:?}").contains("Alice"));
        Ok(())
    }
}

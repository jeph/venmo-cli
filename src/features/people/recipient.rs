use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use super::User;
use crate::shared::{Username, UsernameParseError};

const REDACTED: &str = "[REDACTED]";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecipientInput(Username);

impl RecipientInput {
    #[must_use]
    pub const fn username(&self) -> &Username {
        &self.0
    }
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
        self.0.fmt(formatter)
    }
}

impl FromStr for RecipientInput {
    type Err = RecipientParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Username::from_optional_prefix(value)
            .map(Self)
            .map_err(RecipientParseError::Username)
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum RecipientParseError {
    #[error("invalid username recipient: {0}")]
    Username(UsernameParseError),
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;
    use crate::features::people::UserProfileKind;
    use crate::shared::UserId;

    #[test]
    fn recipient_parser_normalizes_optional_at_and_rejects_invalid_usernames()
    -> Result<(), Box<dyn Error>> {
        let bare = RecipientInput::from_str("alice")?;
        let prefixed = RecipientInput::from_str("@alice")?;
        assert_eq!(bare, prefixed);
        assert_eq!(bare.username().as_str(), "alice");
        assert_eq!(bare.to_string(), "@alice");

        let exact_ascii = "a".repeat(1023);
        let exact_unicode = format!("{}a", "é".repeat(511));
        for value in [exact_ascii.as_str(), exact_unicode.as_str(), "élise"] {
            assert!(RecipientInput::from_str(value).is_ok());
        }
        for value in [
            "a".repeat(1024),
            "é".repeat(512),
            "white space".to_owned(),
            "line\nbreak".to_owned(),
        ] {
            assert!(matches!(
                RecipientInput::from_str(&value),
                Err(RecipientParseError::Username(_))
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

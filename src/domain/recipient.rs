use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use super::identifiers::{UserId, UserIdParseError};
use super::user::User;

const MAX_USERNAME_BYTES: usize = 1023;
const REDACTED: &str = "[REDACTED]";

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Username(String);

impl Username {
    pub fn from_bare(value: impl Into<String>) -> Result<Self, UsernameParseError> {
        let value = value.into();
        validate_bare_username(&value)?;
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Username {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "@{}", self.0)
    }
}

impl FromStr for Username {
    type Err = UsernameParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some(username) = value.strip_prefix('@') else {
            return Err(UsernameParseError::MissingPrefix);
        };
        Self::from_bare(username)
    }
}

fn validate_bare_username(username: &str) -> Result<(), UsernameParseError> {
    if username.is_empty() {
        return Err(UsernameParseError::Empty);
    }
    if username.len() > MAX_USERNAME_BYTES {
        return Err(UsernameParseError::TooLong {
            maximum_bytes: MAX_USERNAME_BYTES,
        });
    }
    if username
        .chars()
        .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(UsernameParseError::InvalidCharacter);
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum UsernameParseError {
    #[error("username recipient must begin with @")]
    MissingPrefix,

    #[error("username cannot be empty")]
    Empty,

    #[error("username cannot exceed {maximum_bytes} bytes")]
    TooLong { maximum_bytes: usize },

    #[error("username cannot contain whitespace or control characters")]
    InvalidCharacter,
}

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
    pub fn user(&self) -> &User {
        &self.user
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

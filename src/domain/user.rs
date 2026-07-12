use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use super::{UserId, Username};

const MAX_QUERY_BYTES: usize = 1024;
const REDACTED: &str = "[REDACTED]";

#[derive(Clone, Eq, PartialEq)]
pub struct User {
    user_id: UserId,
    username: Option<Username>,
    display_name: Option<String>,
}

impl User {
    #[must_use]
    pub fn new(user_id: UserId, username: Option<Username>, display_name: Option<String>) -> Self {
        Self {
            user_id,
            username,
            display_name,
        }
    }

    #[must_use]
    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }

    #[must_use]
    pub fn username(&self) -> Option<&Username> {
        self.username.as_ref()
    }

    #[must_use]
    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }
}

impl fmt::Debug for User {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("User")
            .field("user_id", &REDACTED)
            .field("username", &REDACTED)
            .field("display_name", &REDACTED)
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct UserSearchQuery(String);

impl UserSearchQuery {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn username_query(&self) -> Option<&str> {
        self.0.strip_prefix('@')
    }
}

impl fmt::Debug for UserSearchQuery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserSearchQuery([REDACTED])")
    }
}

impl FromStr for UserSearchQuery {
    type Err = UserSearchQueryParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.trim().is_empty() {
            return Err(UserSearchQueryParseError::Empty);
        }
        if value.len() > MAX_QUERY_BYTES {
            return Err(UserSearchQueryParseError::TooLong {
                maximum_bytes: MAX_QUERY_BYTES,
            });
        }
        if value.chars().any(char::is_control) {
            return Err(UserSearchQueryParseError::ControlCharacter);
        }
        if value == "@" {
            return Err(UserSearchQueryParseError::EmptyUsername);
        }
        Ok(Self(value.to_owned()))
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum UserSearchQueryParseError {
    #[error("search query must contain non-whitespace text")]
    Empty,

    #[error("search query must not exceed {maximum_bytes} bytes")]
    TooLong { maximum_bytes: usize },

    #[error("search query must not contain control characters")]
    ControlCharacter,

    #[error("username search must include text after `@`")]
    EmptyUsername,
}

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use super::{UserId, Username};

const MAX_QUERY_BYTES: usize = 1024;
const REDACTED: &str = "[REDACTED]";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserProfileKind {
    Personal,
    Business,
    Charity,
    Unknown,
}

#[derive(Clone, Eq, PartialEq)]
pub struct User {
    user_id: UserId,
    username: Option<Username>,
    display_name: Option<String>,
    profile_kind: Option<UserProfileKind>,
    is_payable: Option<bool>,
}

impl User {
    #[must_use]
    pub fn new(user_id: UserId, username: Option<Username>, display_name: Option<String>) -> Self {
        Self {
            user_id,
            username,
            display_name,
            profile_kind: None,
            is_payable: None,
        }
    }

    #[must_use]
    pub fn with_financial_attributes(
        mut self,
        profile_kind: UserProfileKind,
        is_payable: bool,
    ) -> Self {
        self.profile_kind = Some(profile_kind);
        self.is_payable = Some(is_payable);
        self
    }

    #[must_use]
    pub(crate) fn with_optional_financial_attributes(
        mut self,
        profile_kind: Option<UserProfileKind>,
        is_payable: Option<bool>,
    ) -> Self {
        self.profile_kind = profile_kind;
        self.is_payable = is_payable;
        self
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

    #[must_use]
    pub const fn profile_kind(&self) -> Option<UserProfileKind> {
        self.profile_kind
    }

    #[must_use]
    pub const fn is_payable(&self) -> Option<bool> {
        self.is_payable
    }
}

impl fmt::Debug for User {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("User")
            .field("user_id", &REDACTED)
            .field("username", &REDACTED)
            .field("display_name", &REDACTED)
            .field("profile_kind", &self.profile_kind)
            .field("is_payable", &self.is_payable)
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

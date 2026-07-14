use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use crate::shared::{UserId, Username};

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
        self,
        profile_kind: UserProfileKind,
        is_payable: bool,
    ) -> Self {
        Self {
            profile_kind: Some(profile_kind),
            is_payable: Some(is_payable),
            ..self
        }
    }

    #[must_use]
    pub(crate) fn with_optional_financial_attributes(
        self,
        profile_kind: Option<UserProfileKind>,
        is_payable: Option<bool>,
    ) -> Self {
        Self {
            profile_kind,
            is_payable,
            ..self
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

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    #[test]
    fn user_constructor_preserves_optional_identity_and_financial_evidence()
    -> Result<(), Box<dyn Error>> {
        let minimal = User::new(UserId::from_str("123")?, None, None);
        assert_eq!(minimal.user_id().as_str(), "123");
        assert!(minimal.username().is_none());
        assert!(minimal.display_name().is_none());
        assert_eq!(minimal.profile_kind(), None);
        assert_eq!(minimal.is_payable(), None);

        let complete = User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("élise")?),
            Some("Élise Example".to_owned()),
        )
        .with_financial_attributes(UserProfileKind::Personal, true);
        assert_eq!(complete.username().map(Username::as_str), Some("élise"));
        assert_eq!(complete.display_name(), Some("Élise Example"));
        assert_eq!(complete.profile_kind(), Some(UserProfileKind::Personal));
        assert_eq!(complete.is_payable(), Some(true));
        let rendered = format!("{complete:?}");
        assert!(!rendered.contains("456"));
        assert!(!rendered.contains("élise"));
        assert!(!rendered.contains("Élise Example"));
        Ok(())
    }

    #[test]
    fn user_search_query_limit_counts_bytes_and_handles_unicode_and_controls()
    -> Result<(), Box<dyn Error>> {
        for value in ["x".repeat(MAX_QUERY_BYTES), "é".repeat(MAX_QUERY_BYTES / 2)] {
            assert_eq!(value.len(), MAX_QUERY_BYTES);
            assert_eq!(UserSearchQuery::from_str(&value)?.as_str(), value);
        }
        for value in [
            "x".repeat(MAX_QUERY_BYTES + 1),
            "é".repeat(MAX_QUERY_BYTES / 2 + 1),
        ] {
            assert_eq!(
                UserSearchQuery::from_str(&value),
                Err(UserSearchQueryParseError::TooLong {
                    maximum_bytes: MAX_QUERY_BYTES
                })
            );
        }

        assert_eq!(
            UserSearchQuery::from_str(" \t "),
            Err(UserSearchQueryParseError::Empty)
        );
        assert_eq!(
            UserSearchQuery::from_str("@"),
            Err(UserSearchQueryParseError::EmptyUsername)
        );
        for value in ["line\nbreak", "zero\u{0}byte", "tab\tquery"] {
            assert_eq!(
                UserSearchQuery::from_str(value),
                Err(UserSearchQueryParseError::ControlCharacter)
            );
        }
        assert_eq!(
            UserSearchQuery::from_str("Élise Smith")?.as_str(),
            "Élise Smith"
        );
        assert_eq!(
            UserSearchQuery::from_str("@élise")?.username_query(),
            Some("élise")
        );
        Ok(())
    }
}

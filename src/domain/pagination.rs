use std::fmt;
use std::num::NonZeroU32;
use std::str::FromStr;

use thiserror::Error;

pub const DEFAULT_LIST_LIMIT: u32 = 10;
pub const MAX_LIST_LIMIT: u32 = 50;
pub const MAX_EXHAUSTIVE_USER_SEARCH_RESULTS: u32 = 200;
pub const MAX_CONTINUATION_TOKEN_BYTES: usize = 1024;

const REDACTED: &str = "[REDACTED]";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limit(NonZeroU32);

impl Limit {
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }

    #[must_use]
    pub(crate) const fn as_nonzero(self) -> NonZeroU32 {
        self.0
    }
}

impl TryFrom<u32> for Limit {
    type Error = LimitParseError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        let nonzero = NonZeroU32::new(value).ok_or(LimitParseError::Zero)?;
        if value > MAX_LIST_LIMIT {
            return Err(LimitParseError::TooLarge {
                maximum: MAX_LIST_LIMIT,
            });
        }
        Ok(Self(nonzero))
    }
}

impl fmt::Display for Limit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for Limit {
    type Err = LimitParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let parsed = value.parse::<u32>().map_err(|_| LimitParseError::Invalid)?;
        Self::try_from(parsed)
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum LimitParseError {
    #[error("limit must be a positive integer that fits in 32 bits")]
    Invalid,

    #[error("limit must be greater than zero")]
    Zero,

    #[error("limit must not exceed {maximum}")]
    TooLarge { maximum: u32 },
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Offset(u32);

impl Offset {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for Offset {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for Offset {
    type Err = OffsetParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(OffsetParseError);
        }
        value.parse::<u32>().map(Self).map_err(|_| OffsetParseError)
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("offset must be a nonnegative integer that fits in 32 bits")]
pub struct OffsetParseError;

macro_rules! continuation_token {
    ($name:ident) => {
        #[derive(Clone, Eq, Hash, PartialEq)]
        pub struct $name(String);

        impl $name {
            #[must_use]
            pub(crate) fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_tuple(stringify!($name))
                    .field(&REDACTED)
                    .finish()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(REDACTED)
            }
        }

        impl FromStr for $name {
            type Err = ContinuationTokenParseError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                validate_continuation_token(value)?;
                Ok(Self(value.to_owned()))
            }
        }
    };
}

continuation_token!(ActivityBeforeId);
continuation_token!(RequestsBefore);

fn validate_continuation_token(value: &str) -> Result<(), ContinuationTokenParseError> {
    if value.is_empty() {
        return Err(ContinuationTokenParseError::Empty);
    }
    if value.len() > MAX_CONTINUATION_TOKEN_BYTES {
        return Err(ContinuationTokenParseError::TooLong {
            maximum_bytes: MAX_CONTINUATION_TOKEN_BYTES,
        });
    }
    if value.chars().any(is_disallowed_token_character) {
        return Err(ContinuationTokenParseError::InvalidCharacter);
    }
    Ok(())
}

fn is_disallowed_token_character(character: char) -> bool {
    character.is_whitespace()
        || character.is_control()
        || matches!(
            character,
            '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{206F}' | '\u{FEFF}'
        )
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ContinuationTokenParseError {
    #[error("continuation token is required")]
    Empty,

    #[error("continuation token must not contain whitespace or control characters")]
    InvalidCharacter,

    #[error("continuation token exceeds the {maximum_bytes}-byte safety limit")]
    TooLong { maximum_bytes: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_are_typed_nonnegative_u32_values() {
        assert_eq!(Offset::from_str("0").map(Offset::get), Ok(0));
        assert_eq!(
            Offset::from_str("4294967295").map(Offset::get),
            Ok(u32::MAX)
        );
        for value in ["", "-1", "+1", "4294967296", "1.0"] {
            assert!(Offset::from_str(value).is_err(), "accepted {value:?}");
        }
    }

    #[test]
    fn continuation_tokens_are_bounded_strict_and_redacted() {
        let raw = "sensitive-token_123";
        let activity = ActivityBeforeId::from_str(raw);
        assert!(activity.is_ok());
        if let Ok(activity) = activity {
            assert_eq!(activity.to_string(), REDACTED);
            assert!(!format!("{activity:?}").contains(raw));
        }

        for value in ["", "has space", "has\ttab", "has\nline", "bidi\u{202E}"] {
            assert!(
                RequestsBefore::from_str(value).is_err(),
                "accepted {value:?}"
            );
        }
        let oversized = "a".repeat(MAX_CONTINUATION_TOKEN_BYTES + 1);
        assert!(matches!(
            ActivityBeforeId::from_str(&oversized),
            Err(ContinuationTokenParseError::TooLong { .. })
        ));
    }
}

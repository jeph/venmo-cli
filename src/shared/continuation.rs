use thiserror::Error;

pub const MAX_CONTINUATION_TOKEN_BYTES: usize = 1024;
pub(crate) const REDACTED: &str = "[REDACTED]";

pub(crate) fn validate_continuation_token(value: &str) -> Result<(), ContinuationTokenParseError> {
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

        impl std::fmt::Debug for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter
                    .debug_tuple(stringify!($name))
                    .field(&$crate::shared::continuation::REDACTED)
                    .finish()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str($crate::shared::continuation::REDACTED)
            }
        }

        impl std::str::FromStr for $name {
            type Err = $crate::shared::ContinuationTokenParseError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                $crate::shared::continuation::validate_continuation_token(value)?;
                Ok(Self(value.to_owned()))
            }
        }
    };
}

pub(crate) use continuation_token;

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    continuation_token!(TestContinuation);

    #[test]
    fn continuation_tokens_are_strict_bounded_and_redacted()
    -> Result<(), Box<dyn std::error::Error>> {
        let token = TestContinuation::from_str("opaque-token")?;
        assert_eq!(token.as_str(), "opaque-token");
        assert_eq!(token.to_string(), REDACTED);
        assert_eq!(format!("{token:?}"), "TestContinuation(\"[REDACTED]\")");

        let exact_ascii = "x".repeat(MAX_CONTINUATION_TOKEN_BYTES);
        let exact_unicode = "é".repeat(MAX_CONTINUATION_TOKEN_BYTES / 2);
        assert_eq!(
            TestContinuation::from_str(&exact_ascii)?.as_str().len(),
            MAX_CONTINUATION_TOKEN_BYTES
        );
        assert_eq!(
            TestContinuation::from_str(&exact_unicode)?.as_str().len(),
            MAX_CONTINUATION_TOKEN_BYTES
        );

        assert!(TestContinuation::from_str("").is_err());
        assert!(TestContinuation::from_str("has space").is_err());
        assert!(TestContinuation::from_str("line\nbreak").is_err());
        assert!(TestContinuation::from_str("hidden\u{200B}control").is_err());
        assert!(TestContinuation::from_str(&"x".repeat(MAX_CONTINUATION_TOKEN_BYTES + 1)).is_err());
        assert!(
            TestContinuation::from_str(&"é".repeat(MAX_CONTINUATION_TOKEN_BYTES / 2 + 1)).is_err()
        );
        Ok(())
    }
}

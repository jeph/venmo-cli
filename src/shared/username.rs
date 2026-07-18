use std::fmt;
use std::str::FromStr;

use thiserror::Error;

const MAX_USERNAME_BYTES: usize = 1023;

#[derive(Clone, Eq, Hash, PartialEq)]
pub struct Username(String);

impl Username {
    pub fn from_bare(value: impl Into<String>) -> Result<Self, UsernameParseError> {
        let value = value.into();
        validate_bare_username(&value)?;
        Ok(Self(value))
    }

    pub fn from_optional_prefix(value: impl Into<String>) -> Result<Self, UsernameParseError> {
        let value = value.into();
        let bare = value.strip_prefix('@').unwrap_or(&value);
        Self::from_bare(bare.to_owned())
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

impl fmt::Debug for Username {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Username([REDACTED])")
    }
}

impl FromStr for Username {
    type Err = UsernameParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::from_optional_prefix(value)
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
    #[error("username cannot be empty")]
    Empty,

    #[error("username cannot exceed {maximum_bytes} bytes")]
    TooLong { maximum_bytes: usize },

    #[error("username cannot contain whitespace or control characters")]
    InvalidCharacter,
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    #[test]
    fn username_limit_is_exactly_bytes_and_preserves_evidence_gated_unicode()
    -> Result<(), Box<dyn Error>> {
        let exact_ascii = "a".repeat(MAX_USERNAME_BYTES);
        let exact_unicode = format!("{}a", "é".repeat((MAX_USERNAME_BYTES - 1) / 2));
        assert_eq!(exact_unicode.len(), MAX_USERNAME_BYTES);
        for value in [&exact_ascii, &exact_unicode] {
            let username = Username::from_bare(value.to_owned())?;
            assert_eq!(username.as_str().len(), MAX_USERNAME_BYTES);
        }

        for value in [
            "a".repeat(MAX_USERNAME_BYTES + 1),
            "é".repeat(MAX_USERNAME_BYTES.div_ceil(2)),
        ] {
            assert_eq!(
                Username::from_bare(value),
                Err(UsernameParseError::TooLong {
                    maximum_bytes: MAX_USERNAME_BYTES
                })
            );
        }
        Ok(())
    }

    #[test]
    fn username_prefix_unicode_and_control_boundaries_are_explicit() -> Result<(), Box<dyn Error>> {
        let username = Username::from_str("@élise")?;
        assert_eq!(username.as_str(), "élise");
        assert_eq!(username.to_string(), "@élise");
        assert_eq!(format!("{username:?}"), "Username([REDACTED])");

        assert_eq!(Username::from_str("alice")?, Username::from_str("@alice")?);
        assert_eq!(Username::from_str("@"), Err(UsernameParseError::Empty));
        for value in [
            "white space",
            "line\nbreak",
            "zero\u{0}byte",
            "nonbreaking\u{a0}space",
        ] {
            assert_eq!(
                Username::from_bare(value),
                Err(UsernameParseError::InvalidCharacter)
            );
        }
        Ok(())
    }

    #[test]
    fn optional_prefix_normalization_produces_one_canonical_username() -> Result<(), Box<dyn Error>>
    {
        let bare = Username::from_optional_prefix("élise")?;
        let prefixed = Username::from_optional_prefix("@élise")?;

        assert_eq!(bare, prefixed);
        assert_eq!(bare.as_str(), "élise");
        assert_eq!(bare.to_string(), "@élise");
        assert_eq!(
            Username::from_optional_prefix("@"),
            Err(UsernameParseError::Empty)
        );
        Ok(())
    }
}

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

const MAX_OPAQUE_ID_BYTES: usize = 512;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error(
    "{kind} must be non-empty, at most 512 bytes, and contain no whitespace or control characters"
)]
pub struct OpaqueIdParseError {
    kind: &'static str,
}

fn validate_opaque_id(value: &str, kind: &'static str) -> Result<(), OpaqueIdParseError> {
    if value.is_empty()
        || value.len() > MAX_OPAQUE_ID_BYTES
        || value.chars().any(|character| {
            character.is_whitespace()
                || character.is_control()
                || is_invisible_format_control(character)
        })
    {
        return Err(OpaqueIdParseError { kind });
    }
    Ok(())
}

fn is_invisible_format_control(character: char) -> bool {
    matches!(
        character,
        '\u{00AD}'
            | '\u{061C}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{FEFF}'
    )
}

macro_rules! opaque_id {
    ($name:ident, $kind:literal) => {
        #[derive(Clone, Eq, Hash, PartialEq)]
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

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(concat!(stringify!($name), "([REDACTED])"))
            }
        }

        impl FromStr for $name {
            type Err = OpaqueIdParseError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                validate_opaque_id(value, $kind)?;
                Ok(Self(value.to_owned()))
            }
        }
    };
}

opaque_id!(ActivityId, "activity ID");
opaque_id!(PaymentId, "payment ID");
opaque_id!(PaymentMethodId, "payment-method ID");
opaque_id!(RequestId, "request ID");

#[derive(Clone, Eq, Hash, PartialEq)]
pub struct UserId(String);

impl UserId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl fmt::Debug for UserId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserId([REDACTED])")
    }
}

impl FromStr for UserId {
    type Err = UserIdParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(UserIdParseError::NotNumeric);
        }
        if !value.bytes().any(|byte| byte != b'0') {
            return Err(UserIdParseError::NotPositive);
        }
        Ok(Self(value.to_owned()))
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum UserIdParseError {
    #[error("Venmo user ID must contain only ASCII decimal digits")]
    NotNumeric,

    #[error("Venmo user ID must be positive")]
    NotPositive,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_ids_are_bounded_and_reject_invisible_controls() {
        assert!(RequestId::from_str("request-1").is_ok());
        assert!(RequestId::from_str(&"x".repeat(MAX_OPAQUE_ID_BYTES + 1)).is_err());
        assert!(RequestId::from_str("request\u{202E}1").is_err());
        assert!(RequestId::from_str("request\u{200B}1").is_err());
        assert!(RequestId::from_str("request\u{061C}1").is_err());
    }
}

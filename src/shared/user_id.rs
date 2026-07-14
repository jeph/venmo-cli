use std::fmt;
use std::str::FromStr;

use thiserror::Error;

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
    use std::error::Error;

    use super::*;

    #[test]
    fn user_ids_preserve_the_existing_numeric_representation_without_recanonicalizing()
    -> Result<(), Box<dyn Error>> {
        for value in ["1", "0001", "18446744073709551616"] {
            let id = UserId::from_str(value)?;
            assert_eq!(id.as_str(), value);
            assert_eq!(id.to_string(), value);
            assert_eq!(format!("{id:?}"), "UserId([REDACTED])");
        }
        Ok(())
    }

    #[test]
    fn user_ids_reject_zero_non_ascii_digits_signs_whitespace_and_controls() {
        for value in ["0", "00", "000"] {
            assert_eq!(UserId::from_str(value), Err(UserIdParseError::NotPositive));
        }
        for value in ["", "+1", "-1", "1.0", "１", "1 2", "1\n2", "abc"] {
            assert_eq!(UserId::from_str(value), Err(UserIdParseError::NotNumeric));
        }
    }
}

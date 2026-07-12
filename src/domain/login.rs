use std::fmt;

use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use super::AccessToken;

const MAX_IDENTIFIER_BYTES: usize = 1024;
const MAX_PASSWORD_BYTES: usize = 64 * 1024;
const MAX_OTP_SECRET_BYTES: usize = 4096;
const OTP_DIGITS: usize = 6;
const REDACTED: &str = "[REDACTED]";

macro_rules! redacted_format {
    ($name:ident) => {
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
    };
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct LoginIdentifier(String);

impl LoginIdentifier {
    pub(crate) fn parse_owned(value: String) -> Result<Self, LoginIdentifierParseError> {
        let mut value = Zeroizing::new(value);
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(LoginIdentifierParseError::Empty);
        }
        if trimmed.len() > MAX_IDENTIFIER_BYTES {
            return Err(LoginIdentifierParseError::TooLong {
                maximum_bytes: MAX_IDENTIFIER_BYTES,
            });
        }
        if trimmed
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
        {
            return Err(LoginIdentifierParseError::InvalidCharacter);
        }
        let normalized = trimmed.to_owned();
        value.zeroize();
        Ok(Self(normalized))
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

redacted_format!(LoginIdentifier);

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum LoginIdentifierParseError {
    #[error("Venmo email, phone, or username is required")]
    Empty,

    #[error("Venmo email, phone, or username cannot contain whitespace or control characters")]
    InvalidCharacter,

    #[error("Venmo email, phone, or username exceeds the {maximum_bytes}-byte safety limit")]
    TooLong { maximum_bytes: usize },
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct AccountPassword(String);

impl AccountPassword {
    pub(crate) fn parse_owned(value: String) -> Result<Self, AccountPasswordParseError> {
        let mut value = Zeroizing::new(value);
        if value.is_empty() {
            return Err(AccountPasswordParseError::Empty);
        }
        if value.len() > MAX_PASSWORD_BYTES {
            return Err(AccountPasswordParseError::TooLong {
                maximum_bytes: MAX_PASSWORD_BYTES,
            });
        }
        if value.chars().any(char::is_control) {
            return Err(AccountPasswordParseError::ControlCharacter);
        }
        Ok(Self(std::mem::take(&mut *value)))
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

redacted_format!(AccountPassword);

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum AccountPasswordParseError {
    #[error("Venmo password is required")]
    Empty,

    #[error("Venmo password cannot contain control characters")]
    ControlCharacter,

    #[error("Venmo password exceeds the {maximum_bytes}-byte safety limit")]
    TooLong { maximum_bytes: usize },
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct OtpCode(String);

impl OtpCode {
    pub(crate) fn parse_owned(value: String) -> Result<Self, OtpCodeParseError> {
        let mut value = Zeroizing::new(value);
        let trimmed = value.trim();
        if trimmed.len() != OTP_DIGITS || !trimmed.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(OtpCodeParseError::Invalid);
        }
        let normalized = trimmed.to_owned();
        value.zeroize();
        Ok(Self(normalized))
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

redacted_format!(OtpCode);

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum OtpCodeParseError {
    #[error("Venmo SMS code must contain exactly six ASCII digits")]
    Invalid,
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct OtpSecret(String);

impl OtpSecret {
    pub(crate) fn parse_owned(value: String) -> Result<Self, OtpSecretParseError> {
        let mut value = Zeroizing::new(value);
        if value.is_empty() {
            return Err(OtpSecretParseError::Empty);
        }
        if value.len() > MAX_OTP_SECRET_BYTES {
            return Err(OtpSecretParseError::TooLong {
                maximum_bytes: MAX_OTP_SECRET_BYTES,
            });
        }
        if value
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
        {
            return Err(OtpSecretParseError::InvalidCharacter);
        }
        Ok(Self(std::mem::take(&mut *value)))
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

redacted_format!(OtpSecret);

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum OtpSecretParseError {
    #[error("Venmo OTP secret is missing")]
    Empty,

    #[error("Venmo OTP secret cannot contain whitespace or control characters")]
    InvalidCharacter,

    #[error("Venmo OTP secret exceeds the {maximum_bytes}-byte safety limit")]
    TooLong { maximum_bytes: usize },
}

pub enum PasswordLoginStart {
    Authenticated(AccessToken),
    OtpRequired(OtpSecret),
}

impl fmt::Debug for PasswordLoginStart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authenticated(_) => formatter.write_str("Authenticated([REDACTED])"),
            Self::OtpRequired(_) => formatter.write_str("OtpRequired([REDACTED])"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_secrets_validate_and_redact() {
        let identifier = LoginIdentifier::parse_owned(" alice@example.com ".to_owned());
        assert!(identifier.is_ok());
        if let Ok(identifier) = identifier {
            assert_eq!(identifier.expose(), "alice@example.com");
            assert_eq!(identifier.to_string(), REDACTED);
            assert!(!format!("{identifier:?}").contains("alice"));
        }

        let password = AccountPassword::parse_owned("synthetic password".to_owned());
        assert!(password.is_ok());
        if let Ok(password) = password {
            assert_eq!(password.expose(), "synthetic password");
            assert_eq!(password.to_string(), REDACTED);
            assert!(!format!("{password:?}").contains("synthetic"));
        }

        let otp = OtpCode::parse_owned(" 123456 ".to_owned());
        assert!(otp.is_ok());
        if let Ok(otp) = otp {
            assert_eq!(otp.expose(), "123456");
            assert_eq!(otp.to_string(), REDACTED);
        }
    }

    #[test]
    fn login_secrets_reject_invalid_values() {
        assert!(LoginIdentifier::parse_owned(" ".to_owned()).is_err());
        assert!(LoginIdentifier::parse_owned("bad identifier".to_owned()).is_err());
        assert!(AccountPassword::parse_owned(String::new()).is_err());
        assert!(AccountPassword::parse_owned("bad\npassword".to_owned()).is_err());
        assert!(OtpCode::parse_owned("12345".to_owned()).is_err());
        assert!(OtpCode::parse_owned("12345a".to_owned()).is_err());
        assert!(OtpSecret::parse_owned("secret value".to_owned()).is_err());
    }
}

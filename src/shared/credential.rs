use std::fmt;
use std::str::FromStr;

use thiserror::Error;
use time::OffsetDateTime;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use super::{UserId, Username};

const MAX_ACCESS_TOKEN_BYTES: usize = 64 * 1024;
const MAX_DEVICE_ID_BYTES: usize = 1024;
const REDACTED: &str = "[REDACTED]";

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct AccessToken(String);

impl AccessToken {
    pub(crate) fn parse_owned(value: String) -> Result<Self, AccessTokenParseError> {
        let value = Zeroizing::new(value);
        if value.chars().any(char::is_control) {
            return Err(AccessTokenParseError::ControlCharacter);
        }

        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(AccessTokenParseError::Empty);
        }

        let token = match strip_prefix_ignore_ascii_case(trimmed, "Authorization:") {
            Some(rest) => parse_bearer_value(rest.trim_start())?,
            None => match strip_bearer_scheme(trimmed) {
                Some(token) => token,
                None if trimmed.eq_ignore_ascii_case("Bearer") => {
                    return Err(AccessTokenParseError::Empty);
                }
                None if starts_with_ignore_ascii_case(trimmed, "Authorization") => {
                    return Err(AccessTokenParseError::MalformedAuthorizationHeader);
                }
                None => trimmed,
            },
        };

        Self::from_normalized_owned(token.to_owned())
    }

    pub(crate) fn from_normalized_owned(value: String) -> Result<Self, AccessTokenParseError> {
        let mut value = Zeroizing::new(value);
        if value.is_empty() {
            return Err(AccessTokenParseError::Empty);
        }
        if value.len() > MAX_ACCESS_TOKEN_BYTES {
            return Err(AccessTokenParseError::TooLong {
                maximum_bytes: MAX_ACCESS_TOKEN_BYTES,
            });
        }
        if value.chars().any(char::is_control) {
            return Err(AccessTokenParseError::ControlCharacter);
        }
        if value.chars().any(char::is_whitespace) {
            return Err(AccessTokenParseError::Whitespace);
        }

        Ok(Self(std::mem::take(&mut *value)))
    }

    pub(crate) fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl FromStr for AccessToken {
    type Err = AccessTokenParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse_owned(value.to_owned())
    }
}

impl fmt::Debug for AccessToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("AccessToken")
            .field(&REDACTED)
            .finish()
    }
}

impl fmt::Display for AccessToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum AccessTokenParseError {
    #[error("bearer token is required")]
    Empty,

    #[error("bearer token cannot contain control characters")]
    ControlCharacter,

    #[error("bearer token cannot contain whitespace")]
    Whitespace,

    #[error("Authorization header must have the form `Authorization: Bearer <token>`")]
    MalformedAuthorizationHeader,

    #[error("bearer token exceeds the {maximum_bytes}-byte safety limit")]
    TooLong { maximum_bytes: usize },
}

fn parse_bearer_value(value: &str) -> Result<&str, AccessTokenParseError> {
    strip_bearer_scheme(value).ok_or(AccessTokenParseError::MalformedAuthorizationHeader)
}

fn strip_bearer_scheme(value: &str) -> Option<&str> {
    let rest = strip_prefix_ignore_ascii_case(value, "Bearer")?;
    let first = rest.chars().next()?;
    if !first.is_whitespace() {
        return None;
    }
    let token = rest.trim_start();
    (!token.is_empty()).then_some(token)
}

fn strip_prefix_ignore_ascii_case<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    let head = value.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| value.get(prefix.len()..))?
}

fn starts_with_ignore_ascii_case(value: &str, prefix: &str) -> bool {
    value
        .get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

#[derive(Clone, Eq, Hash, PartialEq, Zeroize, ZeroizeOnDrop)]
pub struct DeviceId(String);

impl DeviceId {
    pub fn from_owned(value: String) -> Result<Self, DeviceIdParseError> {
        let mut value = Zeroizing::new(value);
        if value.is_empty() {
            return Err(DeviceIdParseError::Empty);
        }
        if value.len() > MAX_DEVICE_ID_BYTES {
            return Err(DeviceIdParseError::TooLong {
                maximum_bytes: MAX_DEVICE_ID_BYTES,
            });
        }
        if value
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
        {
            return Err(DeviceIdParseError::InvalidCharacter);
        }
        Ok(Self(std::mem::take(&mut *value)))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for DeviceId {
    type Err = DeviceIdParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::from_owned(value.to_owned())
    }
}

impl fmt::Debug for DeviceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("DeviceId").field(&REDACTED).finish()
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum DeviceIdParseError {
    #[error("device ID is required")]
    Empty,

    #[error("device ID cannot contain whitespace or control characters")]
    InvalidCharacter,

    #[error("device ID exceeds the {maximum_bytes}-byte safety limit")]
    TooLong { maximum_bytes: usize },
}

pub struct CredentialEnvelope {
    access_token: AccessToken,
    device_id: DeviceId,
    user_id: UserId,
    username: Username,
    display_name: Option<String>,
    saved_at: OffsetDateTime,
}

impl CredentialEnvelope {
    pub fn new(
        access_token: AccessToken,
        device_id: DeviceId,
        user_id: UserId,
        username: Username,
        display_name: Option<String>,
        saved_at: OffsetDateTime,
    ) -> Self {
        Self {
            access_token,
            device_id,
            user_id,
            username,
            display_name,
            saved_at,
        }
    }

    #[must_use]
    pub fn access_token(&self) -> &AccessToken {
        &self.access_token
    }

    #[must_use]
    pub fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    #[must_use]
    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }

    #[must_use]
    pub fn username(&self) -> &Username {
        &self.username
    }

    #[must_use]
    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }

    #[must_use]
    pub const fn saved_at(&self) -> OffsetDateTime {
        self.saved_at
    }

    #[must_use]
    pub(crate) fn storage_equivalent(&self, other: &Self) -> bool {
        self.access_token.expose_secret() == other.access_token.expose_secret()
            && self.device_id == other.device_id
            && self.user_id == other.user_id
            && self.username == other.username
            && self.display_name == other.display_name
            && self.saved_at == other.saved_at
    }
}

impl fmt::Debug for CredentialEnvelope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialEnvelope")
            .field("access_token", &REDACTED)
            .field("device_id", &REDACTED)
            .field("user_id", &REDACTED)
            .field("username", &REDACTED)
            .field("display_name", &REDACTED)
            .field("saved_at", &self.saved_at)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_input_normalizes_supported_forms_and_redacts_formatting() {
        for source in [
            "secret-value",
            "Bearer secret-value",
            "authorization: bearer secret-value",
        ] {
            let token = AccessToken::from_str(source);
            assert!(token.is_ok(), "rejected {source:?}");
            if let Ok(token) = token {
                assert_eq!(token.expose_secret(), "secret-value");
                assert_eq!(token.to_string(), REDACTED);
                assert!(!format!("{token:?}").contains("secret-value"));
            }
        }
    }

    #[test]
    fn token_input_rejects_unsafe_or_malformed_values() {
        for source in [
            "",
            "Bearer ",
            "Authorization: Basic secret",
            "Authorization Bearer secret",
            "secret value",
            "secret\nvalue",
        ] {
            assert!(
                AccessToken::from_str(source).is_err(),
                "accepted {source:?}"
            );
        }
    }

    #[test]
    fn access_token_byte_limit_is_exact_for_ascii_and_unicode() {
        for value in [
            "x".repeat(MAX_ACCESS_TOKEN_BYTES),
            "é".repeat(MAX_ACCESS_TOKEN_BYTES / 2),
        ] {
            assert_eq!(value.len(), MAX_ACCESS_TOKEN_BYTES);
            let token = AccessToken::from_normalized_owned(value);
            assert!(token.is_ok());
        }
        assert!(matches!(
            AccessToken::from_normalized_owned("x".repeat(MAX_ACCESS_TOKEN_BYTES + 1)),
            Err(AccessTokenParseError::TooLong {
                maximum_bytes: MAX_ACCESS_TOKEN_BYTES
            })
        ));
        assert!(matches!(
            AccessToken::from_normalized_owned("é".repeat(MAX_ACCESS_TOKEN_BYTES / 2 + 1)),
            Err(AccessTokenParseError::TooLong {
                maximum_bytes: MAX_ACCESS_TOKEN_BYTES
            })
        ));
    }

    #[test]
    fn device_id_byte_unicode_whitespace_and_control_boundaries_are_exact() {
        for value in [
            "x".repeat(MAX_DEVICE_ID_BYTES),
            "é".repeat(MAX_DEVICE_ID_BYTES / 2),
        ] {
            assert_eq!(value.len(), MAX_DEVICE_ID_BYTES);
            let device_id = DeviceId::from_owned(value);
            assert!(device_id.is_ok());
        }
        for value in [
            "x".repeat(MAX_DEVICE_ID_BYTES + 1),
            "é".repeat(MAX_DEVICE_ID_BYTES / 2 + 1),
        ] {
            assert!(matches!(
                DeviceId::from_owned(value),
                Err(DeviceIdParseError::TooLong {
                    maximum_bytes: MAX_DEVICE_ID_BYTES
                })
            ));
        }
        for value in ["device id", "device\nid", "device\u{0}id", "device\u{a0}id"] {
            assert_eq!(
                DeviceId::from_owned(value.to_owned()),
                Err(DeviceIdParseError::InvalidCharacter)
            );
        }
    }
}

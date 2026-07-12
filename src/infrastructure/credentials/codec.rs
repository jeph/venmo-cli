use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use zeroize::Zeroizing;

use crate::application::ports::{CredentialFormat, LoadedCredential};
use crate::domain::{
    AccessToken, AccessTokenParseError, CredentialEnvelope, DeviceId, DeviceIdParseError, UserId,
    UserIdParseError, Username, UsernameParseError,
};

const CURRENT_SCHEMA_VERSION: u64 = 1;
const MAX_ENCODED_CREDENTIAL_BYTES: usize = 128 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CredentialWire {
    schema_version: Option<u64>,
    access_token: SecretString,
    device_id: String,
    user_id: String,
    username: String,
    display_name: Option<String>,
    created_at: String,
}

struct SecretString(Zeroizing<String>);

impl SecretString {
    fn take(&mut self) -> String {
        std::mem::take(&mut *self.0)
    }
}

impl<'de> Deserialize<'de> for SecretString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)
            .map(Zeroizing::new)
            .map(Self)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CredentialWireRef<'a> {
    schema_version: u64,
    access_token: &'a str,
    device_id: &'a str,
    user_id: &'a str,
    username: &'a str,
    display_name: Option<&'a str>,
    created_at: &'a str,
}

pub(super) struct EncodedCredential(Zeroizing<String>);

impl EncodedCredential {
    pub(super) fn as_str(&self) -> &str {
        &self.0
    }
}

pub(super) fn encode(
    credential: &CredentialEnvelope,
) -> Result<EncodedCredential, CredentialCodecError> {
    let created_at = credential
        .created_at()
        .format(&Rfc3339)
        .map_err(|source| CredentialCodecError::TimestampEncoding { source })?;
    let wire = CredentialWireRef {
        schema_version: CURRENT_SCHEMA_VERSION,
        access_token: credential.access_token().expose_secret(),
        device_id: credential.device_id().as_str(),
        user_id: credential.user_id().as_str(),
        username: credential.username().as_str(),
        display_name: credential.display_name(),
        created_at: &created_at,
    };
    let encoded = Zeroizing::new(
        serde_json::to_string(&wire).map_err(|source| CredentialCodecError::Encoding { source })?,
    );
    if encoded.len() > MAX_ENCODED_CREDENTIAL_BYTES {
        return Err(CredentialCodecError::TooLarge {
            maximum_bytes: MAX_ENCODED_CREDENTIAL_BYTES,
        });
    }
    Ok(EncodedCredential(encoded))
}

pub(super) fn decode(raw: &str) -> Result<LoadedCredential, CredentialCodecError> {
    if raw.len() > MAX_ENCODED_CREDENTIAL_BYTES {
        return Err(CredentialCodecError::TooLarge {
            maximum_bytes: MAX_ENCODED_CREDENTIAL_BYTES,
        });
    }

    let mut wire: CredentialWire = serde_json::from_str(raw).map_err(classify_json_error)?;
    let format = match wire.schema_version {
        None => CredentialFormat::LegacyTypeScript,
        Some(CURRENT_SCHEMA_VERSION) => CredentialFormat::Version1,
        Some(version) => return Err(CredentialCodecError::UnsupportedVersion { version }),
    };

    let access_token = AccessToken::from_normalized_owned(wire.access_token.take())
        .map_err(|source| CredentialCodecError::InvalidAccessToken { source })?;
    let device_id = DeviceId::from_owned(wire.device_id)
        .map_err(|source| CredentialCodecError::InvalidDeviceId { source })?;
    let user_id = UserId::from_str(&wire.user_id)
        .map_err(|source| CredentialCodecError::InvalidUserId { source })?;
    let bare_username = wire.username.strip_prefix('@').unwrap_or(&wire.username);
    let username = Username::from_bare(bare_username.to_owned())
        .map_err(|source| CredentialCodecError::InvalidUsername { source })?;
    let created_at = OffsetDateTime::parse(&wire.created_at, &Rfc3339)
        .map_err(|source| CredentialCodecError::InvalidCreationTime { source })?;
    let envelope = CredentialEnvelope::new(
        access_token,
        device_id,
        user_id,
        username,
        wire.display_name,
        created_at,
    );

    Ok(LoadedCredential { envelope, format })
}

fn classify_json_error(source: serde_json::Error) -> CredentialCodecError {
    match source.classify() {
        serde_json::error::Category::Data => CredentialCodecError::InvalidShape { source },
        serde_json::error::Category::Io
        | serde_json::error::Category::Syntax
        | serde_json::error::Category::Eof => CredentialCodecError::MalformedJson { source },
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialCodecErrorKind {
    Corrupt,
    Invalid,
    UnsupportedVersion,
    TooLarge,
    Internal,
}

#[derive(Debug, Error)]
pub enum CredentialCodecError {
    #[error("stored credential is not valid JSON")]
    MalformedJson {
        #[source]
        source: serde_json::Error,
    },

    #[error("stored credential JSON has an invalid shape")]
    InvalidShape {
        #[source]
        source: serde_json::Error,
    },

    #[error("stored credential uses unsupported schema version {version}")]
    UnsupportedVersion { version: u64 },

    #[error("stored credential has an invalid bearer token")]
    InvalidAccessToken {
        #[source]
        source: AccessTokenParseError,
    },

    #[error("stored credential has an invalid device ID")]
    InvalidDeviceId {
        #[source]
        source: DeviceIdParseError,
    },

    #[error("stored credential has an invalid user ID")]
    InvalidUserId {
        #[source]
        source: UserIdParseError,
    },

    #[error("stored credential has an invalid username")]
    InvalidUsername {
        #[source]
        source: UsernameParseError,
    },

    #[error("stored credential has an invalid creation time")]
    InvalidCreationTime {
        #[source]
        source: time::error::Parse,
    },

    #[error("stored credential exceeds the {maximum_bytes}-byte safety limit")]
    TooLarge { maximum_bytes: usize },

    #[error("credential JSON encoding failed")]
    Encoding {
        #[source]
        source: serde_json::Error,
    },

    #[error("credential timestamp encoding failed")]
    TimestampEncoding {
        #[source]
        source: time::error::Format,
    },
}

impl CredentialCodecError {
    #[must_use]
    pub const fn kind(&self) -> CredentialCodecErrorKind {
        match self {
            Self::MalformedJson { .. } => CredentialCodecErrorKind::Corrupt,
            Self::InvalidShape { .. }
            | Self::InvalidAccessToken { .. }
            | Self::InvalidDeviceId { .. }
            | Self::InvalidUserId { .. }
            | Self::InvalidUsername { .. }
            | Self::InvalidCreationTime { .. } => CredentialCodecErrorKind::Invalid,
            Self::UnsupportedVersion { .. } => CredentialCodecErrorKind::UnsupportedVersion,
            Self::TooLarge { .. } => CredentialCodecErrorKind::TooLarge,
            Self::Encoding { .. } | Self::TimestampEncoding { .. } => {
                CredentialCodecErrorKind::Internal
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    type TestResult = Result<(), Box<dyn Error>>;

    fn fixture() -> Result<CredentialEnvelope, Box<dyn Error>> {
        let access_token = AccessToken::from_normalized_owned("token-secret".to_owned());
        let device_id = DeviceId::from_owned("device-id".to_owned());
        let user_id = UserId::from_str("123");
        let username = Username::from_bare("alice");

        Ok(CredentialEnvelope::new(
            access_token?,
            device_id?,
            user_id?,
            username?,
            Some("Alice Example".to_owned()),
            OffsetDateTime::UNIX_EPOCH,
        ))
    }

    #[test]
    fn version_one_round_trips_without_exposing_the_secret() -> TestResult {
        let credential = fixture()?;
        let encoded = encode(&credential)?;
        assert!(encoded.as_str().contains("\"schemaVersion\":1"));
        assert!(encoded.as_str().contains("token-secret"));

        let decoded = decode(encoded.as_str())?;
        assert_eq!(decoded.format, CredentialFormat::Version1);
        assert_eq!(decoded.envelope.user_id().as_str(), "123");
        assert_eq!(decoded.envelope.username().as_str(), "alice");
        assert!(!format!("{:?}", decoded.envelope).contains("token-secret"));
        Ok(())
    }

    #[test]
    fn unversioned_typescript_envelope_is_recognized() -> TestResult {
        let raw = r#"{"accessToken":"legacy-secret","deviceId":"legacy-device","userId":"123","username":"alice","displayName":null,"createdAt":"2026-07-10T00:00:00.000Z"}"#;
        let decoded = decode(raw)?;
        assert_eq!(decoded.format, CredentialFormat::LegacyTypeScript);
        assert_eq!(decoded.envelope.username().as_str(), "alice");
        Ok(())
    }

    #[test]
    fn codec_distinguishes_corrupt_invalid_and_future_data() {
        let corrupt = decode("{");
        assert!(matches!(
            corrupt,
            Err(ref error) if error.kind() == CredentialCodecErrorKind::Corrupt
        ));

        let invalid = decode(r#"{"schemaVersion":1}"#);
        assert!(matches!(
            invalid,
            Err(ref error) if error.kind() == CredentialCodecErrorKind::Invalid
        ));

        let future = decode(
            r#"{"schemaVersion":2,"accessToken":"secret","deviceId":"device","userId":"123","username":"alice","displayName":null,"createdAt":"now"}"#,
        );
        assert!(matches!(
            future,
            Err(ref error) if error.kind() == CredentialCodecErrorKind::UnsupportedVersion
        ));
    }
}

use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use zeroize::Zeroizing;

use crate::shared::{
    AccessToken, AccessTokenParseError, CredentialEnvelope, CredentialFormat, DeviceId,
    DeviceIdParseError, LoadedCredential, UserId, UserIdParseError, Username, UsernameParseError,
};

const CURRENT_SCHEMA_VERSION: u64 = 1;
const MAX_ENCODED_CREDENTIAL_BYTES: usize = 128 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CredentialWire {
    #[serde(default)]
    schema_version: SchemaVersionWire,
    access_token: SecretString,
    device_id: String,
    user_id: String,
    username: String,
    display_name: Option<String>,
    #[serde(rename = "createdAt")]
    saved_at: String,
}

#[derive(Default)]
enum SchemaVersionWire {
    #[default]
    Missing,
    Null,
    Version(u64),
}

impl<'de> Deserialize<'de> for SchemaVersionWire {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<u64>::deserialize(deserializer).map(|version| match version {
            Some(version) => Self::Version(version),
            None => Self::Null,
        })
    }
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
    #[serde(rename = "createdAt")]
    saved_at: &'a str,
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
    if credential.username().as_str().starts_with('@') {
        return Err(CredentialCodecError::NonCanonicalUsername);
    }
    let saved_at = credential
        .saved_at()
        .format(&Rfc3339)
        .map_err(|source| CredentialCodecError::TimestampEncoding { source })?;
    let wire = CredentialWireRef {
        schema_version: CURRENT_SCHEMA_VERSION,
        access_token: credential.access_token().expose_secret(),
        device_id: credential.device_id().as_str(),
        user_id: credential.user_id().as_str(),
        username: credential.username().as_str(),
        display_name: credential.display_name(),
        saved_at: &saved_at,
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
        SchemaVersionWire::Missing => CredentialFormat::LegacyTypeScript,
        SchemaVersionWire::Null => return Err(CredentialCodecError::InvalidSchemaVersion),
        SchemaVersionWire::Version(CURRENT_SCHEMA_VERSION) => CredentialFormat::Version1,
        SchemaVersionWire::Version(version) => {
            return Err(CredentialCodecError::UnsupportedVersion { version });
        }
    };

    let access_token = AccessToken::from_normalized_owned(wire.access_token.take())
        .map_err(|source| CredentialCodecError::InvalidAccessToken { source })?;
    let device_id = DeviceId::from_owned(wire.device_id)
        .map_err(|source| CredentialCodecError::InvalidDeviceId { source })?;
    let user_id = UserId::from_str(&wire.user_id)
        .map_err(|source| CredentialCodecError::InvalidUserId { source })?;
    let bare_username = match format {
        CredentialFormat::LegacyTypeScript => match wire.username.strip_prefix('@') {
            Some(username) if username.starts_with('@') => {
                return Err(CredentialCodecError::NonCanonicalUsername);
            }
            Some(username) => username,
            None => &wire.username,
        },
        CredentialFormat::Version1 if wire.username.starts_with('@') => {
            return Err(CredentialCodecError::NonCanonicalUsername);
        }
        CredentialFormat::Version1 => &wire.username,
    };
    let username = Username::from_bare(bare_username.to_owned())
        .map_err(|source| CredentialCodecError::InvalidUsername { source })?;
    let saved_at = OffsetDateTime::parse(&wire.saved_at, &Rfc3339)
        .map_err(|source| CredentialCodecError::InvalidSavedTime { source })?;
    let envelope = CredentialEnvelope::new(
        access_token,
        device_id,
        user_id,
        username,
        wire.display_name,
        saved_at,
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

    #[error("stored credential schema version must be an integer when present")]
    InvalidSchemaVersion,

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

    #[error("stored credential username is not canonical for its schema")]
    NonCanonicalUsername,

    #[error("stored credential has an invalid saved time")]
    InvalidSavedTime {
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
            | Self::InvalidSchemaVersion
            | Self::InvalidAccessToken { .. }
            | Self::InvalidDeviceId { .. }
            | Self::InvalidUserId { .. }
            | Self::InvalidUsername { .. }
            | Self::NonCanonicalUsername
            | Self::InvalidSavedTime { .. } => CredentialCodecErrorKind::Invalid,
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
    use std::io;

    use super::*;

    type TestResult = Result<(), Box<dyn Error>>;

    const V1_FIXTURE: &str = concat!(
        r#"{"schemaVersion":1,"accessToken":"token-secret","deviceId":"device-id","#,
        r#""userId":"123","username":"alice","displayName":"Alice Example","#,
        r#""createdAt":"1970-01-01T00:00:00Z"}"#,
    );

    fn fixture() -> Result<CredentialEnvelope, Box<dyn Error>> {
        fixture_with(Some("Alice Example".to_owned()), "alice")
    }

    fn fixture_with(
        display_name: Option<String>,
        username: &str,
    ) -> Result<CredentialEnvelope, Box<dyn Error>> {
        let access_token = AccessToken::from_normalized_owned("token-secret".to_owned());
        let device_id = DeviceId::from_owned("device-id".to_owned());
        let user_id = UserId::from_str("123");
        let username = Username::from_bare(username);

        Ok(CredentialEnvelope::new(
            access_token?,
            device_id?,
            user_id?,
            username?,
            display_name,
            OffsetDateTime::UNIX_EPOCH,
        ))
    }

    fn v1_value() -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": 1,
            "accessToken": "token-secret",
            "deviceId": "device-id",
            "userId": "123",
            "username": "alice",
            "displayName": "Alice Example",
            "createdAt": "1970-01-01T00:00:00Z"
        })
    }

    fn encoded_value(value: &serde_json::Value) -> Result<String, serde_json::Error> {
        serde_json::to_string(value)
    }

    fn require_codec_error(
        result: Result<LoadedCredential, CredentialCodecError>,
    ) -> Result<CredentialCodecError, Box<dyn Error>> {
        match result {
            Ok(_) => Err(io::Error::other("credential unexpectedly decoded").into()),
            Err(error) => Ok(error),
        }
    }

    #[test]
    fn version_one_round_trips_without_exposing_the_secret() -> TestResult {
        let credential = fixture()?;
        let encoded = encode(&credential)?;
        assert_eq!(encoded.as_str(), V1_FIXTURE);
        assert!(!encoded.as_str().contains("savedAt"));

        let decoded = decode(encoded.as_str())?;
        assert_eq!(decoded.format, CredentialFormat::Version1);
        assert_eq!(decoded.envelope.user_id().as_str(), "123");
        assert_eq!(decoded.envelope.username().as_str(), "alice");
        assert_eq!(decoded.envelope.saved_at(), OffsetDateTime::UNIX_EPOCH);
        let debug = format!("{:?}", decoded.envelope);
        assert!(debug.contains("saved_at"));
        assert!(!debug.contains("created_at"));
        assert!(!debug.contains("token-secret"));
        Ok(())
    }

    #[test]
    fn unversioned_typescript_envelope_is_recognized() -> TestResult {
        for username in ["alice", "@alice"] {
            let raw = format!(
                r#"{{"accessToken":"legacy-secret","deviceId":"legacy-device","userId":"123","username":"{username}","displayName":null,"createdAt":"2026-07-10T00:00:00.000Z"}}"#
            );
            let decoded = decode(&raw)?;
            assert_eq!(decoded.format, CredentialFormat::LegacyTypeScript);
            assert_eq!(decoded.envelope.username().as_str(), "alice");
        }
        Ok(())
    }

    #[test]
    fn missing_schema_is_legacy_but_explicit_null_is_invalid() -> TestResult {
        let mut legacy = v1_value();
        let Some(object) = legacy.as_object_mut() else {
            return Err(io::Error::other("test fixture was not an object").into());
        };
        object.remove("schemaVersion");
        let decoded = decode(&encoded_value(&legacy)?)?;
        assert_eq!(decoded.format, CredentialFormat::LegacyTypeScript);

        let mut explicit_null = v1_value();
        explicit_null["schemaVersion"] = serde_json::Value::Null;
        let error = require_codec_error(decode(&encoded_value(&explicit_null)?))?;
        assert!(matches!(&error, CredentialCodecError::InvalidSchemaVersion));
        assert_eq!(error.kind(), CredentialCodecErrorKind::Invalid);
        Ok(())
    }

    #[test]
    fn version_one_requires_a_canonical_bare_username_on_decode_and_encode() -> TestResult {
        let mut prefixed = v1_value();
        prefixed["username"] = serde_json::Value::String("@alice".to_owned());
        let error = require_codec_error(decode(&encoded_value(&prefixed)?))?;
        assert!(matches!(error, CredentialCodecError::NonCanonicalUsername));

        let noncanonical = fixture_with(None, "@alice")?;
        assert!(matches!(
            encode(&noncanonical),
            Err(CredentialCodecError::NonCanonicalUsername)
        ));

        let legacy_double_prefix = r#"{"accessToken":"legacy-secret","deviceId":"legacy-device","userId":"123","username":"@@alice","displayName":null,"createdAt":"2026-07-10T00:00:00Z"}"#;
        assert!(matches!(
            decode(legacy_double_prefix),
            Err(CredentialCodecError::NonCanonicalUsername)
        ));
        Ok(())
    }

    #[test]
    fn malformed_timestamps_are_rejected_as_invalid_data() -> TestResult {
        for timestamp in [
            "",
            "now",
            "1970-01-01",
            "1970-01-01T00:00:00",
            "1970-13-01T00:00:00Z",
            "1970-01-01T00:00:00+99:00",
        ] {
            let mut value = v1_value();
            value["createdAt"] = serde_json::Value::String(timestamp.to_owned());
            let error = require_codec_error(decode(&encoded_value(&value)?))?;
            assert!(
                matches!(&error, CredentialCodecError::InvalidSavedTime { .. }),
                "accepted or misclassified {timestamp:?}"
            );
            assert_eq!(error.kind(), CredentialCodecErrorKind::Invalid);
        }
        Ok(())
    }

    #[test]
    fn malformed_and_invalid_field_table_has_exact_classifications() -> TestResult {
        for malformed in ["", "{", "[}", r#"{"accessToken":"secret""#] {
            let error = require_codec_error(decode(malformed))?;
            assert_eq!(error.kind(), CredentialCodecErrorKind::Corrupt);
            assert!(matches!(error, CredentialCodecError::MalformedJson { .. }));
        }

        let cases = [
            ("accessToken", serde_json::Value::String(String::new())),
            (
                "accessToken",
                serde_json::Value::String("token with space".to_owned()),
            ),
            ("deviceId", serde_json::Value::String(String::new())),
            (
                "deviceId",
                serde_json::Value::String("device\nvalue".to_owned()),
            ),
            ("userId", serde_json::Value::String("0".to_owned())),
            (
                "userId",
                serde_json::Value::String("not-numeric".to_owned()),
            ),
            ("username", serde_json::Value::String(String::new())),
            (
                "username",
                serde_json::Value::String("bad\nusername".to_owned()),
            ),
        ];
        for (field, invalid_value) in cases {
            let mut value = v1_value();
            value[field] = invalid_value;
            let error = require_codec_error(decode(&encoded_value(&value)?))?;
            assert_eq!(
                error.kind(),
                CredentialCodecErrorKind::Invalid,
                "field {field}"
            );
        }

        for (field, invalid_value) in [
            ("accessToken", serde_json::Value::Null),
            ("deviceId", serde_json::json!(7)),
            ("userId", serde_json::Value::Null),
            ("username", serde_json::json!(["alice"])),
            ("displayName", serde_json::json!(true)),
            ("createdAt", serde_json::Value::Null),
        ] {
            let mut value = v1_value();
            value[field] = invalid_value;
            let error = require_codec_error(decode(&encoded_value(&value)?))?;
            assert!(matches!(&error, CredentialCodecError::InvalidShape { .. }));
            assert_eq!(error.kind(), CredentialCodecErrorKind::Invalid);
        }
        Ok(())
    }

    #[test]
    fn unsupported_schema_versions_are_distinct_from_invalid_shapes() -> TestResult {
        for version in [0_u64, 2, u64::MAX] {
            let mut value = v1_value();
            value["schemaVersion"] = serde_json::json!(version);
            let error = require_codec_error(decode(&encoded_value(&value)?))?;
            assert!(matches!(
                &error,
                CredentialCodecError::UnsupportedVersion { version: observed }
                    if *observed == version
            ));
            assert_eq!(error.kind(), CredentialCodecErrorKind::UnsupportedVersion);
        }

        for schema in [
            serde_json::json!(-1),
            serde_json::json!(1.5),
            serde_json::json!("1"),
        ] {
            let mut value = v1_value();
            value["schemaVersion"] = schema;
            let error = require_codec_error(decode(&encoded_value(&value)?))?;
            assert!(matches!(error, CredentialCodecError::InvalidShape { .. }));
        }
        Ok(())
    }

    #[test]
    fn encoded_input_limit_accepts_the_exact_boundary_and_rejects_one_more_byte() -> TestResult {
        let padding = MAX_ENCODED_CREDENTIAL_BYTES
            .checked_sub(V1_FIXTURE.len())
            .ok_or_else(|| io::Error::other("fixture exceeded codec input limit"))?;
        let exact = format!("{V1_FIXTURE}{}", " ".repeat(padding));
        assert_eq!(exact.len(), MAX_ENCODED_CREDENTIAL_BYTES);
        assert_eq!(decode(&exact)?.format, CredentialFormat::Version1);

        let oversized = format!("{exact} ");
        assert!(matches!(
            decode(&oversized),
            Err(CredentialCodecError::TooLarge {
                maximum_bytes: MAX_ENCODED_CREDENTIAL_BYTES
            })
        ));
        Ok(())
    }

    #[test]
    fn encoded_output_limit_accepts_the_exact_boundary_and_rejects_one_more_byte() -> TestResult {
        let empty_display = fixture_with(Some(String::new()), "alice")?;
        let fixed_bytes = encode(&empty_display)?.as_str().len();
        let display_bytes = MAX_ENCODED_CREDENTIAL_BYTES
            .checked_sub(fixed_bytes)
            .ok_or_else(|| io::Error::other("fixture exceeded codec output limit"))?;

        let exact = fixture_with(Some("x".repeat(display_bytes)), "alice")?;
        let encoded = encode(&exact)?;
        assert_eq!(encoded.as_str().len(), MAX_ENCODED_CREDENTIAL_BYTES);

        let oversized = fixture_with(Some("x".repeat(display_bytes + 1)), "alice")?;
        assert!(matches!(
            encode(&oversized),
            Err(CredentialCodecError::TooLarge {
                maximum_bytes: MAX_ENCODED_CREDENTIAL_BYTES
            })
        ));
        Ok(())
    }

    #[test]
    fn codec_errors_and_debug_output_redact_every_sensitive_field() -> TestResult {
        const SECRET: &str = "synthetic-super-secret-marker";
        let mut value = v1_value();
        value["accessToken"] = serde_json::Value::String(SECRET.to_owned());
        value["deviceId"] = serde_json::Value::String("device secret".to_owned());
        let error = require_codec_error(decode(&encoded_value(&value)?))?;
        let rendered = format!("{error} {error:?}");
        assert!(!rendered.contains(SECRET));
        assert!(!rendered.contains("device secret"));

        let credential = fixture()?;
        let rendered = format!("{credential:?}");
        for sensitive in ["token-secret", "device-id", "123", "alice", "Alice Example"] {
            assert!(!rendered.contains(sensitive));
        }
        Ok(())
    }
}

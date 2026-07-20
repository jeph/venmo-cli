use std::fmt;

use reqwest::StatusCode;
use serde_json::Value;
use zeroize::Zeroizing;

use super::error::PostSendFailure;

#[derive(Eq, PartialEq)]
pub(in crate::adapters::venmo) struct HttpResponse {
    pub(super) status: StatusCode,
    pub(super) body: Vec<u8>,
    pub(super) otp_secret: Option<Zeroizing<Vec<u8>>>,
}

impl HttpResponse {
    #[must_use]
    pub(in crate::adapters::venmo) const fn status(&self) -> StatusCode {
        self.status
    }

    #[must_use]
    pub(in crate::adapters::venmo) fn body(&self) -> &[u8] {
        &self.body
    }

    pub(in crate::adapters::venmo) fn otp_secret(&self) -> Option<&[u8]> {
        self.otp_secret.as_ref().map(|value| value.as_slice())
    }

    #[cfg(test)]
    #[must_use]
    pub(in crate::adapters::venmo) fn for_test(
        status: StatusCode,
        body: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            status,
            body: body.into(),
            otp_secret: None,
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(in crate::adapters::venmo) fn with_otp_secret_for_test(mut self, otp_secret: &str) -> Self {
        self.otp_secret = Some(Zeroizing::new(otp_secret.as_bytes().to_vec()));
        self
    }
}

impl fmt::Debug for HttpResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpResponse")
            .field("status", &self.status)
            .field("body_bytes", &self.body.len())
            .field("has_otp_secret", &self.otp_secret.is_some())
            .finish()
    }
}

const MAX_DEBUG_CODE_BYTES: usize = 64;
const MAX_DEBUG_TITLE_BYTES: usize = 128;
const MAX_DEBUG_MESSAGE_BYTES: usize = 512;
const REDACTED: &str = "[REDACTED]";

#[derive(Debug, Eq, PartialEq)]
pub(super) struct ResponseDiagnostics {
    pub(super) body_kind: &'static str,
    pub(super) error_code: Option<String>,
    pub(super) error_title: Option<String>,
    pub(super) error_message: Option<String>,
}

impl ResponseDiagnostics {
    pub(super) fn from_body(body: &[u8], redactions: &[Option<&str>]) -> Self {
        if body.is_empty() {
            return Self {
                body_kind: "empty",
                error_code: None,
                error_title: None,
                error_message: None,
            };
        }
        let Ok(value) = serde_json::from_slice::<Value>(body) else {
            return Self {
                body_kind: "non-json",
                error_code: None,
                error_title: None,
                error_message: None,
            };
        };
        let root_error = value.get("error").filter(|error| error.is_object());
        let graph_ql_error = value
            .get("errors")
            .and_then(Value::as_array)
            .and_then(|errors| errors.first())
            .filter(|error| error.is_object());
        let code = root_error
            .and_then(|error| error.get("code"))
            .or_else(|| value.get("error_code"))
            .or_else(|| {
                graph_ql_error
                    .and_then(|error| error.get("extensions"))
                    .and_then(|extensions| extensions.get("code"))
            })
            .and_then(json_scalar_text)
            .and_then(|code| sanitize_debug_code(&code));
        let title = root_error
            .and_then(|error| error.get("title"))
            .or_else(|| {
                graph_ql_error
                    .and_then(|error| error.get("extensions"))
                    .and_then(|extensions| extensions.get("title"))
            })
            .and_then(Value::as_str)
            .and_then(|title| bounded_debug_text(title, MAX_DEBUG_TITLE_BYTES, redactions));
        let message = root_error
            .and_then(|error| error.get("message"))
            .or_else(|| graph_ql_error.and_then(|error| error.get("message")))
            .and_then(Value::as_str)
            .and_then(|message| bounded_debug_text(message, MAX_DEBUG_MESSAGE_BYTES, redactions));
        Self {
            body_kind: "json",
            error_code: code,
            error_title: title,
            error_message: message,
        }
    }
}

fn json_scalar_text(value: &Value) -> Option<String> {
    if let Some(value) = value.as_str() {
        Some(value.to_owned())
    } else if value.is_number() {
        Some(value.to_string())
    } else {
        None
    }
}

fn sanitize_debug_code(value: &str) -> Option<String> {
    if value.is_empty()
        || value.len() > MAX_DEBUG_CODE_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
    {
        return None;
    }
    Some(value.to_owned())
}

fn bounded_debug_text(
    value: &str,
    maximum_bytes: usize,
    redactions: &[Option<&str>],
) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    let mut redacted = value.to_owned();
    for secret in redactions
        .iter()
        .flatten()
        .filter(|secret| !secret.is_empty())
    {
        if redacted.contains(secret) {
            redacted = redacted.replace(secret, REDACTED);
        }
    }
    redacted = redact_sensitive_numeric_runs(&redacted);

    let mut sanitized = String::with_capacity(redacted.len().min(maximum_bytes));
    let mut truncated = false;
    for character in redacted.chars() {
        let escaped = escaped_debug_character(character);
        if sanitized.len() + escaped.len() > maximum_bytes {
            truncated = true;
            break;
        }
        sanitized.push_str(&escaped);
    }
    if truncated {
        const ELLIPSIS: &str = "...";
        while sanitized.len() + ELLIPSIS.len() > maximum_bytes {
            let Some((index, _)) = sanitized.char_indices().next_back() else {
                break;
            };
            sanitized.truncate(index);
        }
        sanitized.push_str(ELLIPSIS);
    }
    (!sanitized.is_empty()).then_some(sanitized)
}

fn redact_sensitive_numeric_runs(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut index = 0;
    while index < value.len() {
        let Some(character) = value[index..].chars().next() else {
            break;
        };
        if !character.is_ascii_digit() {
            output.push(character);
            index += character.len_utf8();
            continue;
        }
        let start = index;
        while index < value.len() && value.as_bytes()[index].is_ascii_digit() {
            index += 1;
        }
        let length = index - start;
        if length == 6 || length >= 12 {
            output.push_str(REDACTED);
        } else {
            output.push_str(&value[start..index]);
        }
    }
    output
}

fn escaped_debug_character(character: char) -> String {
    match character {
        '\n' => "\\n".to_owned(),
        '\r' => "\\r".to_owned(),
        '\t' => "\\t".to_owned(),
        character if character.is_control() || is_invisible_format_control(character) => {
            format!("\\u{{{:04X}}}", u32::from(character))
        }
        character => character.to_string(),
    }
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

pub(super) async fn read_bounded_body(
    mut response: reqwest::Response,
    maximum_bytes: usize,
) -> Result<Vec<u8>, PostSendFailure> {
    if response
        .content_length()
        .is_some_and(|length| length > maximum_bytes as u64)
    {
        return Err(PostSendFailure::ResponseTooLarge);
    }

    let initial_capacity = response
        .content_length()
        .and_then(|length| usize::try_from(length).ok())
        .unwrap_or(0)
        .min(maximum_bytes);
    let mut body = Vec::new();
    body.try_reserve(initial_capacity)
        .map_err(|_| PostSendFailure::ResourceExhaustion)?;

    loop {
        let chunk = response
            .chunk()
            .await
            .map_err(|_| PostSendFailure::ResponseRead)?;
        let Some(chunk) = chunk else {
            break;
        };
        reserve_body_chunk(&mut body, chunk.len(), maximum_bytes)?;
        body.extend_from_slice(&chunk);
    }

    Ok(body)
}

pub(super) fn reserve_body_chunk(
    body: &mut Vec<u8>,
    chunk_length: usize,
    maximum_bytes: usize,
) -> Result<(), PostSendFailure> {
    if body
        .len()
        .checked_add(chunk_length)
        .is_none_or(|length| length > maximum_bytes)
    {
        return Err(PostSendFailure::ResponseTooLarge);
    }

    // Reserve relative to length so the immediately following extension cannot
    // allocate through an infallible API.
    body.try_reserve(chunk_length)
        .map_err(|_| PostSendFailure::ResourceExhaustion)
}

#[cfg(test)]
mod diagnostics_tests {
    use std::error::Error;
    use std::io;

    use super::*;

    type TestResult = Result<(), Box<dyn Error>>;

    #[test]
    fn extracts_only_bounded_terminal_safe_error_diagnostics() -> TestResult {
        let long_message = format!(
            "private-token\n{}private-device",
            "x".repeat(MAX_DEBUG_MESSAGE_BYTES)
        );
        let body = serde_json::to_vec(&serde_json::json!({
            "error": {
                "code": 10100,
                "title": "Blocked\u{200B} payment",
                "message": long_message,
                "access_token": "nested-private-token"
            },
            "data": {
                "note": "private-note",
                "funding_source_id": "private-source"
            }
        }))?;

        let diagnostics = ResponseDiagnostics::from_body(
            &body,
            &[Some("private-token"), Some("private-device"), None],
        );

        assert_eq!(diagnostics.body_kind, "json");
        assert_eq!(diagnostics.error_code.as_deref(), Some("10100"));
        assert_eq!(
            diagnostics.error_title.as_deref(),
            Some("Blocked\\u{200B} payment")
        );
        let message = diagnostics
            .error_message
            .as_deref()
            .ok_or_else(|| io::Error::other("missing diagnostic message"))?;
        assert!(message.starts_with("[REDACTED]\\n"));
        assert!(message.ends_with("..."));
        assert!(message.len() <= MAX_DEBUG_MESSAGE_BYTES);
        for private in [
            "private-token",
            "private-device",
            "nested-private-token",
            "private-note",
            "private-source",
        ] {
            assert!(!format!("{diagnostics:?}{message}").contains(private));
        }
        Ok(())
    }

    #[test]
    fn supports_graphql_errors_without_logging_the_envelope() -> TestResult {
        let body = serde_json::to_vec(&serde_json::json!({
            "errors": [{
                "message": "GraphQL failure",
                "extensions": {
                    "code": "OTP_FAILED",
                    "title": "Verification failed",
                    "private": "must-not-appear"
                }
            }],
            "data": {"private": "also-must-not-appear"}
        }))?;

        let diagnostics = ResponseDiagnostics::from_body(&body, &[]);

        assert_eq!(diagnostics.error_code.as_deref(), Some("OTP_FAILED"));
        assert_eq!(
            diagnostics.error_title.as_deref(),
            Some("Verification failed")
        );
        assert_eq!(
            diagnostics.error_message.as_deref(),
            Some("GraphQL failure")
        );
        let rendered = format!("{diagnostics:?}");
        assert!(!rendered.contains("must-not-appear"));
        Ok(())
    }

    #[test]
    fn classifies_empty_non_json_and_invalid_code_without_exposing_body_bytes() -> TestResult {
        assert_eq!(
            ResponseDiagnostics::from_body(&[], &[]),
            ResponseDiagnostics {
                body_kind: "empty",
                error_code: None,
                error_title: None,
                error_message: None,
            }
        );
        assert_eq!(
            ResponseDiagnostics::from_body(b"private non-json body", &[]),
            ResponseDiagnostics {
                body_kind: "non-json",
                error_code: None,
                error_title: None,
                error_message: None,
            }
        );
        let body = serde_json::to_vec(&serde_json::json!({
            "error": {"code": "unsafe code", "message": "safe message"}
        }))?;
        let diagnostics = ResponseDiagnostics::from_body(&body, &[]);
        assert_eq!(diagnostics.error_code, None);
        assert_eq!(diagnostics.error_message.as_deref(), Some("safe message"));
        Ok(())
    }
}

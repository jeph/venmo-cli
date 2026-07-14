use std::str::FromStr;
use std::time::Duration;

use reqwest::Url;
use time::OffsetDateTime;
use wiremock::MockServer;

use super::*;
use crate::shared::{AccessToken, CredentialEnvelope, DeviceId, UserId, Username};

const TEST_TIMEOUT: Duration = Duration::from_secs(2);
const TEST_RESPONSE_LIMIT: usize = 1024;

mod client;
mod errors;

fn test_transport(
    server: &MockServer,
    request_timeout: Duration,
    response_limit: usize,
) -> Result<VenmoHttpTransport, TransportBuildError> {
    let base_url = Url::parse(&format!("{}/v1/", server.uri()))
        .map_err(|_| TransportBuildError::InvalidProductionOrigin)?;
    VenmoHttpTransport::for_test(base_url, request_timeout, response_limit)
}

fn test_credential() -> Result<CredentialEnvelope, Box<dyn std::error::Error>> {
    Ok(CredentialEnvelope::new(
        AccessToken::from_str("secret-value")?,
        DeviceId::from_str("device-value")?,
        UserId::from_str("123")?,
        Username::from_str("@alice")?,
        Some("Alice".to_owned()),
        OffsetDateTime::UNIX_EPOCH,
    ))
}

fn empty_json_body() -> Result<JsonBody, serde_json::Error> {
    JsonBody::encode(&serde_json::json!({}))
}

async fn assert_request_count(server: &MockServer, expected: usize) {
    let requests = server.received_requests().await;
    assert!(
        requests
            .as_ref()
            .is_some_and(|requests| requests.len() == expected),
        "expected {expected} captured request(s), got {}",
        requests.as_ref().map_or(0, Vec::len)
    );
}

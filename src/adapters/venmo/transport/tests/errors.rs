use std::io::{self, Read, Write};
use std::net::TcpListener;
use std::thread::{self, JoinHandle};

use reqwest::Url;
use reqwest::header::{HeaderMap, HeaderValue};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::adapters::venmo::transport::client::capture_otp_secret;
use crate::adapters::venmo::transport::error::{PostSendFailure, classify_post_send_failure};

type RawServer = JoinHandle<io::Result<()>>;
type RawTransportResult = Result<(VenmoHttpTransport, RawServer), Box<dyn std::error::Error>>;

fn raw_response_transport(response: &'static [u8], response_limit: usize) -> RawTransportResult {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let base_url = Url::parse(&format!("http://{address}/v1/"))?;
    let transport = VenmoHttpTransport::for_test(base_url, TEST_TIMEOUT, response_limit)?;
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept()?;
        stream.set_read_timeout(Some(TEST_TIMEOUT))?;
        stream.set_write_timeout(Some(TEST_TIMEOUT))?;
        let mut request = [0_u8; 4096];
        let _bytes_read = stream.read(&mut request)?;
        stream.write_all(response)?;
        stream.flush()
    });
    Ok((transport, server))
}

fn join_raw_server(server: RawServer) -> Result<(), Box<dyn std::error::Error>> {
    let result = server
        .join()
        .map_err(|_| io::Error::other("raw HTTP fixture thread terminated unexpectedly"))?;
    result.map_err(Into::into)
}

#[tokio::test(flavor = "current_thread")]
async fn classifies_token_issuance_timeout_as_unknown() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/oauth/access_token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(200))
                .set_body_bytes(b"{}"),
        )
        .mount(&server)
        .await;
    let transport = test_transport(&server, Duration::from_millis(50), 1024)?;
    let credential = test_credential()?;
    let result = transport
        .send_with_device_id(
            credential.device_id(),
            HttpRequest::password_login_json_post(
                "/oauth/access_token",
                &["oauth", "access_token"],
                &[],
                empty_json_body()?,
            ),
        )
        .await;
    assert_eq!(
        result,
        Err(TransportError::AuthenticationOutcomeUnknown {
            cause: AmbiguousWriteCause::Timeout,
        })
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn classifies_invalid_otp_response_header_after_authentication_write_as_unknown()
-> Result<(), Box<dyn std::error::Error>> {
    const SENSITIVE_MARKER: &str = "synthetic-private-otp-secret";
    let server = MockServer::start().await;
    let oversized_header = SENSITIVE_MARKER.repeat(
        MAX_OTP_SECRET_HEADER_BYTES
            .div_ceil(SENSITIVE_MARKER.len())
            .saturating_add(1),
    );
    Mock::given(method("POST"))
        .and(path("/v1/oauth/access_token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("venmo-otp-secret", oversized_header.as_str())
                .set_body_bytes(b"{}"),
        )
        .mount(&server)
        .await;
    let transport = test_transport(&server, TEST_TIMEOUT, TEST_RESPONSE_LIMIT)?;
    let credential = test_credential()?;
    let result = transport
        .send_with_device_id(
            credential.device_id(),
            HttpRequest::password_login_json_post(
                "/oauth/access_token",
                &["oauth", "access_token"],
                &[],
                empty_json_body()?,
            ),
        )
        .await;
    let rendered = match &result {
        Ok(response) => format!("{response:?}"),
        Err(error) => format!("{error} {error:?}"),
    };
    assert_eq!(
        result,
        Err(TransportError::AuthenticationOutcomeUnknown {
            cause: AmbiguousWriteCause::ResponseCapture,
        })
    );
    assert!(!rendered.contains(SENSITIVE_MARKER));
    assert_request_count(&server, 1).await;
    Ok(())
}

#[test]
fn otp_secret_response_header_enforces_missing_empty_exact_duplicate_and_oversized_boundaries()
-> Result<(), Box<dyn std::error::Error>> {
    let missing = HeaderMap::new();
    let missing_capture = capture_otp_secret(&missing).map_err(|failure| {
        std::io::Error::other(format!("unexpected capture failure: {failure:?}"))
    })?;
    assert!(missing_capture.is_none());

    for length in [1, MAX_OTP_SECRET_HEADER_BYTES] {
        let mut headers = HeaderMap::new();
        headers.insert(
            OTP_SECRET_HEADER,
            HeaderValue::from_str(&"x".repeat(length))?,
        );
        let captured = capture_otp_secret(&headers).map_err(|failure| {
            std::io::Error::other(format!("unexpected capture failure: {failure:?}"))
        })?;
        assert_eq!(captured.as_ref().map(|value| value.len()), Some(length));
    }

    for length in [0, MAX_OTP_SECRET_HEADER_BYTES + 1] {
        let mut headers = HeaderMap::new();
        headers.insert(
            OTP_SECRET_HEADER,
            HeaderValue::from_bytes(&vec![b'x'; length])?,
        );
        assert_eq!(
            capture_otp_secret(&headers),
            Err(PostSendFailure::ResponseCapture)
        );
    }

    let mut duplicate = HeaderMap::new();
    duplicate.append(OTP_SECRET_HEADER, HeaderValue::from_static("first"));
    duplicate.append(OTP_SECRET_HEADER, HeaderValue::from_static("second"));
    assert_eq!(
        capture_otp_secret(&duplicate),
        Err(PostSendFailure::ResponseCapture)
    );
    Ok(())
}

#[test]
fn classifies_every_post_send_failure_exactly_by_operation() {
    for (failure, cause, ordinary) in [
        (
            PostSendFailure::Redirect,
            AmbiguousWriteCause::Redirect,
            TransportError::UnexpectedRedirect,
        ),
        (
            PostSendFailure::ResponseCapture,
            AmbiguousWriteCause::ResponseCapture,
            TransportError::InvalidAuthenticationResponseHeader,
        ),
        (
            PostSendFailure::ResponseRead,
            AmbiguousWriteCause::ResponseRead,
            TransportError::ResponseRead,
        ),
        (
            PostSendFailure::ResponseTooLarge,
            AmbiguousWriteCause::ResponseTooLarge,
            TransportError::ResponseTooLarge {
                maximum_bytes: TEST_RESPONSE_LIMIT,
            },
        ),
        (
            PostSendFailure::ResourceExhaustion,
            AmbiguousWriteCause::ResourceExhaustion,
            TransportError::ResourceExhaustion,
        ),
    ] {
        for operation in [OperationClass::Read, OperationClass::NonFinancialWrite] {
            assert_eq!(
                classify_post_send_failure(operation, failure, TEST_RESPONSE_LIMIT),
                ordinary
            );
        }
        assert_eq!(
            classify_post_send_failure(
                OperationClass::AuthenticationWrite,
                failure,
                TEST_RESPONSE_LIMIT,
            ),
            TransportError::AuthenticationOutcomeUnknown { cause }
        );
        assert_eq!(
            classify_post_send_failure(
                OperationClass::FinancialWrite,
                failure,
                TEST_RESPONSE_LIMIT,
            ),
            TransportError::FinancialWriteOutcomeUnknown { cause }
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn classifies_uncertain_financial_write_failures_as_ambiguous()
-> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/payments"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"too-large"))
        .mount(&server)
        .await;
    let transport = test_transport(&server, TEST_TIMEOUT, 4)?;
    let credential = test_credential()?;
    let result = transport
        .send_authenticated(
            ApiSession::from(&credential),
            HttpRequest::financial_json_post("/payments", &["payments"], &[], empty_json_body()?),
        )
        .await;
    assert_eq!(
        result,
        Err(TransportError::FinancialWriteOutcomeUnknown {
            cause: AmbiguousWriteCause::ResponseTooLarge,
        })
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn chunked_response_crossing_the_limit_is_rejected_without_content_length()
-> Result<(), Box<dyn std::error::Error>> {
    const RESPONSE: &[u8] = concat!(
        "HTTP/1.1 200 OK\r\n",
        "Transfer-Encoding: chunked\r\n",
        "Connection: close\r\n\r\n",
        "3\r\nabc\r\n",
        "3\r\ndef\r\n",
        "0\r\n\r\n"
    )
    .as_bytes();
    let (transport, server) = raw_response_transport(RESPONSE, 4)?;
    let credential = test_credential()?;
    let result = transport
        .send_authenticated(
            ApiSession::from(&credential),
            HttpRequest::read("/chunked", &["chunked"], &[]),
        )
        .await;
    join_raw_server(server)?;
    assert_eq!(
        result,
        Err(TransportError::ResponseTooLarge { maximum_bytes: 4 })
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn truncated_chunked_response_is_a_deterministic_read_failure()
-> Result<(), Box<dyn std::error::Error>> {
    const RESPONSE: &[u8] = concat!(
        "HTTP/1.1 200 OK\r\n",
        "Transfer-Encoding: chunked\r\n",
        "Connection: close\r\n\r\n",
        "5\r\nabc"
    )
    .as_bytes();
    let (transport, server) = raw_response_transport(RESPONSE, TEST_RESPONSE_LIMIT)?;
    let credential = test_credential()?;
    let result = transport
        .send_authenticated(
            ApiSession::from(&credential),
            HttpRequest::read("/truncated", &["truncated"], &[]),
        )
        .await;
    join_raw_server(server)?;
    assert_eq!(result, Err(TransportError::ResponseRead));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn classifies_financial_write_timeout_as_ambiguous() -> Result<(), Box<dyn std::error::Error>>
{
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/payments"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(200))
                .set_body_bytes(b"{}"),
        )
        .mount(&server)
        .await;
    let transport = test_transport(&server, Duration::from_millis(50), 1024)?;
    let credential = test_credential()?;
    let result = transport
        .send_authenticated(
            ApiSession::from(&credential),
            HttpRequest::financial_json_post("/payments", &["payments"], &[], empty_json_body()?),
        )
        .await;
    assert_eq!(
        result,
        Err(TransportError::FinancialWriteOutcomeUnknown {
            cause: AmbiguousWriteCause::Timeout,
        })
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn classifies_pretransmission_connect_failure_as_network()
-> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    drop(listener);
    let base_url = Url::parse(&format!("http://{address}/v1/"))?;
    let transport = VenmoHttpTransport::for_test(base_url, TEST_TIMEOUT, 1024)?;
    let credential = test_credential()?;
    let result = transport
        .send_authenticated(
            ApiSession::from(&credential),
            HttpRequest::financial_json_post("/payments", &["payments"], &[], empty_json_body()?),
        )
        .await;
    assert_eq!(result, Err(TransportError::Network));
    Ok(())
}

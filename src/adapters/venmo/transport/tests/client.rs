use reqwest::StatusCode;
use wiremock::matchers::{header, header_exists, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::adapters::venmo::transport::client::parse_trusted_next_link_at_origin;
use crate::adapters::venmo::transport::response::reserve_body_chunk;

#[tokio::test(flavor = "current_thread")]
async fn sends_only_the_expected_authenticated_request() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/users/123"))
        .and(query_param("limit", "10"))
        .and(header("accept", "application/json; charset=utf-8"))
        .and(header("accept-language", "en-US;q=1.0"))
        .and(header("authorization", "Bearer secret-value"))
        .and(header("device-id", "device-value"))
        .and(header("user-agent", COMPATIBILITY_USER_AGENT))
        .and(header_exists("x-session-id"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"{}"))
        .mount(&server)
        .await;

    let transport = test_transport(&server, TEST_TIMEOUT, TEST_RESPONSE_LIMIT)?;
    let credential = test_credential()?;
    let response = transport
        .send_authenticated(
            ApiSession::from(&credential),
            HttpRequest::read("/users/{user-id}", &["users", "123"], &[("limit", "10")]),
        )
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.body(), b"{}");
    assert!(!format!("{response:?}").contains("{}"));
    assert_request_count(&server, 1).await;
    Ok(())
}

#[test]
fn json_body_encodes_exact_bytes_without_debug_exposure() -> Result<(), Box<dyn std::error::Error>>
{
    const SENSITIVE_MARKER: &str = "synthetic-private-password";
    let body = JsonBody::encode(&serde_json::json!({"password": SENSITIVE_MARKER}))?;
    assert_eq!(
        body.bytes.as_slice(),
        br#"{"password":"synthetic-private-password"}"#
    );
    assert!(!format!("{body:?}").contains(SENSITIVE_MARKER));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn rejects_redirects_without_following_them() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/start"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/v1/destination"))
        .mount(&server)
        .await;
    let transport = test_transport(&server, TEST_TIMEOUT, TEST_RESPONSE_LIMIT)?;
    let credential = test_credential()?;
    let result = transport
        .send_authenticated(
            ApiSession::from(&credential),
            HttpRequest::read("/start", &["start"], &[]),
        )
        .await;
    assert_eq!(result, Err(TransportError::UnexpectedRedirect));
    assert_request_count(&server, 1).await;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn bounds_response_bodies_before_buffering_them() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/large"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"too-large"))
        .mount(&server)
        .await;
    let transport = test_transport(&server, TEST_TIMEOUT, 4)?;
    let credential = test_credential()?;
    let result = transport
        .send_authenticated(
            ApiSession::from(&credential),
            HttpRequest::read("/large", &["large"], &[]),
        )
        .await;
    assert_eq!(
        result,
        Err(TransportError::ResponseTooLarge { maximum_bytes: 4 })
    );
    Ok(())
}

#[test]
fn reserves_each_untrusted_body_chunk_relative_to_length() {
    let mut body = Vec::with_capacity(8);
    let original_capacity = body.capacity();
    body.resize(original_capacity - 1, b'a');
    let chunk = b"bc";
    let expected_length = body.len() + chunk.len();
    assert!(expected_length > original_capacity);
    assert_eq!(
        reserve_body_chunk(&mut body, chunk.len(), expected_length),
        Ok(())
    );
    let reserved_capacity = body.capacity();
    assert!(reserved_capacity >= expected_length);
    body.extend_from_slice(chunk);
    assert_eq!(body.len(), expected_length);
    assert_eq!(body.capacity(), reserved_capacity);
}

#[test]
fn chunk_reservation_accepts_the_exact_limit_and_rejects_overflow_or_one_byte_more() {
    let mut exact = b"abc".to_vec();
    assert_eq!(reserve_body_chunk(&mut exact, 1, 4), Ok(()));

    let mut over = b"abc".to_vec();
    assert_eq!(
        reserve_body_chunk(&mut over, 2, 4),
        Err(crate::adapters::venmo::transport::error::PostSendFailure::ResponseTooLarge)
    );

    let mut overflow = vec![b'x'];
    assert_eq!(
        reserve_body_chunk(&mut overflow, usize::MAX, usize::MAX),
        Err(crate::adapters::venmo::transport::error::PostSendFailure::ResponseTooLarge)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn never_retries_at_the_http_client_layer() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/transient"))
        .respond_with(ResponseTemplate::new(503).set_body_bytes(b"unavailable"))
        .mount(&server)
        .await;
    let transport = test_transport(&server, TEST_TIMEOUT, TEST_RESPONSE_LIMIT)?;
    let credential = test_credential()?;
    let response = transport
        .send_authenticated(
            ApiSession::from(&credential),
            HttpRequest::read("/transient", &["transient"], &[]),
        )
        .await?;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_request_count(&server, 1).await;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn rejects_untrusted_route_segments_before_network_access()
-> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    let transport = test_transport(&server, TEST_TIMEOUT, TEST_RESPONSE_LIMIT)?;
    let credential = test_credential()?;
    for segment in ["", ".", "..", "users/123", "users\\123", "bad\nvalue"] {
        let result = transport
            .send_authenticated(
                ApiSession::from(&credential),
                HttpRequest::read("/{segment}", &[segment], &[]),
            )
            .await;
        assert_eq!(result, Err(TransportError::InvalidRoute));
    }
    assert_request_count(&server, 0).await;
    Ok(())
}

#[test]
fn base_url_construction_matrix_accepts_only_safe_origins_and_positive_limits()
-> Result<(), Box<dyn std::error::Error>> {
    for origin in [
        "https://api.example.test/v1/",
        "http://127.0.0.1:8080/v1/",
        "http://[::1]:8080/v1/",
        "http://localhost:8080/v1/",
    ] {
        let result = VenmoHttpTransport::for_test(
            reqwest::Url::parse(origin)?,
            TEST_TIMEOUT,
            TEST_RESPONSE_LIMIT,
        )
        .map(|_| ());
        assert_eq!(result, Ok(()), "rejected safe origin {origin}");
    }

    for (origin, expected) in [
        (
            "http://api.example.test/v1/",
            TransportBuildError::InsecureOrigin,
        ),
        ("ftp://localhost/v1/", TransportBuildError::InsecureOrigin),
        (
            "https://user@api.example.test/v1/",
            TransportBuildError::UnsafeOrigin,
        ),
        (
            "https://api.example.test/v1/?query=value",
            TransportBuildError::UnsafeOrigin,
        ),
        (
            "https://api.example.test/v1/#fragment",
            TransportBuildError::UnsafeOrigin,
        ),
    ] {
        let result = VenmoHttpTransport::for_test(
            reqwest::Url::parse(origin)?,
            TEST_TIMEOUT,
            TEST_RESPONSE_LIMIT,
        )
        .map(|_| ());
        assert_eq!(result, Err(expected), "accepted unsafe origin {origin}");
    }

    let zero_limit = VenmoHttpTransport::for_test(
        reqwest::Url::parse("https://api.example.test/v1/")?,
        TEST_TIMEOUT,
        0,
    )
    .map(|_| ());
    assert_eq!(zero_limit, Err(TransportBuildError::InvalidResponseLimit));

    assert!(VenmoHttpTransport::production().is_ok());
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn invalid_query_names_fail_before_network_access() -> Result<(), Box<dyn std::error::Error>>
{
    let server = MockServer::start().await;
    let transport = test_transport(&server, TEST_TIMEOUT, TEST_RESPONSE_LIMIT)?;
    let credential = test_credential()?;
    for name in ["", "bad\nname", "bad\u{0}name"] {
        let query = [(name, "value")];
        let result = transport
            .send_authenticated(
                ApiSession::from(&credential),
                HttpRequest::read("/query", &["query"], &query),
            )
            .await;
        assert_eq!(result, Err(TransportError::InvalidQuery));
    }
    assert_request_count(&server, 0).await;
    Ok(())
}

#[test]
fn continuation_links_are_bound_to_the_exact_origin_path_and_authority()
-> Result<(), Box<dyn std::error::Error>> {
    let base = reqwest::Url::parse("https://api.venmo.com/v1/")?;
    let expected_path = ["users", "123", "friends"];
    let valid = parse_trusted_next_link_at_origin(
        &base,
        "https://api.venmo.com/v1/users/123/friends?limit=1&offset=1",
        &expected_path,
    )?;
    assert_eq!(
        valid,
        [
            ("limit".to_owned(), "1".to_owned()),
            ("offset".to_owned(), "1".to_owned())
        ]
    );

    for link in [
        "/v1/users/123/friends?limit=1&offset=1",
        "http://api.venmo.com/v1/users/123/friends?limit=1&offset=1",
        "https://untrusted.example/v1/users/123/friends?limit=1&offset=1",
        "https://api.venmo.com:444/v1/users/123/friends?limit=1&offset=1",
        "https://user@api.venmo.com/v1/users/123/friends?limit=1&offset=1",
        "https://api.venmo.com/v1/users/999/friends?limit=1&offset=1",
        "https://api.venmo.com/v1/users/123/friends?limit=1&offset=1#fragment",
    ] {
        assert_eq!(
            parse_trusted_next_link_at_origin(&base, link, &expected_path),
            Err(TransportError::InvalidContinuationLink),
            "accepted continuation {link}"
        );
    }
    Ok(())
}

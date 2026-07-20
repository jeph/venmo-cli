use std::time::{Duration, Instant};

use reqwest::header::{
    ACCEPT, ACCEPT_LANGUAGE, AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue,
};
use reqwest::{Url, redirect};
use zeroize::Zeroizing;

use crate::features::auth::{OtpCode, OtpSecret};
use crate::shared::{AccessToken, DeviceId};

use super::error::{
    PostSendFailure, TransportBuildError, TransportError, classify_post_send_failure,
    classify_send_error,
};
use super::request::{ApiEndpoint, HttpRequest, RequestCredentials, ResponseCapture};
use super::response::{HttpResponse, ResponseDiagnostics, read_bounded_body};
use super::{
    ApiTransport, CONNECT_TIMEOUT, DEVICE_ID_HEADER, ENGLISH_ACCEPT_LANGUAGE, FORM_CONTENT_TYPE,
    JSON_ACCEPT, JSON_CONTENT_TYPE, MAX_OTP_SECRET_HEADER_BYTES, MAX_RESPONSE_BYTES,
    OTP_CODE_HEADER, OTP_SECRET_HEADER, PRODUCTION_API_BASE, READ_TIMEOUT, REQUEST_TIMEOUT,
    X_SESSION_ID_HEADER,
};

pub struct VenmoHttpTransport {
    client: reqwest::Client,
    base_url: Url,
    response_limit: usize,
}

impl VenmoHttpTransport {
    pub(in crate::adapters::venmo) fn production() -> Result<Self, TransportBuildError> {
        let base_url = Url::parse(PRODUCTION_API_BASE)
            .map_err(|_| TransportBuildError::InvalidProductionOrigin)?;
        Self::build(
            base_url,
            CONNECT_TIMEOUT,
            READ_TIMEOUT,
            REQUEST_TIMEOUT,
            MAX_RESPONSE_BYTES,
            false,
        )
    }

    fn build(
        base_url: Url,
        connect_timeout: Duration,
        read_timeout: Duration,
        request_timeout: Duration,
        response_limit: usize,
        allow_insecure_loopback: bool,
    ) -> Result<Self, TransportBuildError> {
        validate_base_url(&base_url, allow_insecure_loopback)?;
        if response_limit == 0 {
            return Err(TransportBuildError::InvalidResponseLimit);
        }

        let session_id = uuid::Uuid::new_v4().as_u128().to_string();
        let session_id = HeaderValue::from_str(&session_id)
            .map_err(|_| TransportBuildError::ClientInitialization)?;
        let mut default_headers = HeaderMap::new();
        default_headers.insert(ACCEPT_LANGUAGE, ENGLISH_ACCEPT_LANGUAGE);
        default_headers.insert(X_SESSION_ID_HEADER, session_id);

        let client = reqwest::Client::builder()
            .use_rustls_tls()
            .connect_timeout(connect_timeout)
            .read_timeout(read_timeout)
            .timeout(request_timeout)
            .redirect(redirect::Policy::none())
            .referer(false)
            .no_proxy()
            .no_gzip()
            .no_brotli()
            .no_zstd()
            .no_deflate()
            .retry(reqwest::retry::never())
            .user_agent(super::COMPATIBILITY_USER_AGENT)
            .default_headers(default_headers)
            .build()
            .map_err(|_| TransportBuildError::ClientInitialization)?;

        Ok(Self {
            client,
            base_url,
            response_limit,
        })
    }

    pub(super) fn parse_trusted_next_link(
        &self,
        raw: &str,
        expected_path_segments: &[&str],
    ) -> Result<Vec<(String, String)>, TransportError> {
        parse_trusted_next_link_at_origin(&self.base_url, raw, expected_path_segments)
    }

    async fn send(
        &self,
        credentials: RequestCredentials<'_>,
        request: HttpRequest<'_>,
    ) -> Result<HttpResponse, TransportError> {
        let started_at = Instant::now();
        let url = self.build_url(request.endpoint, request.path_segments, request.query)?;
        let mut builder = self
            .client
            .request(request.method.clone(), url)
            .header(ACCEPT, JSON_ACCEPT);
        match credentials {
            RequestCredentials::Authenticated(session) => {
                builder = builder
                    .header(AUTHORIZATION, authorization_header(session.access_token)?)
                    .header(DEVICE_ID_HEADER, device_id_header(session.device_id)?);
            }
            RequestCredentials::Device(device_id) => {
                builder = builder.header(DEVICE_ID_HEADER, device_id_header(device_id)?);
            }
            RequestCredentials::OtpSecret {
                device_id,
                otp_secret,
            } => {
                builder = builder
                    .header(DEVICE_ID_HEADER, device_id_header(device_id)?)
                    .header(OTP_SECRET_HEADER, otp_secret_header(otp_secret)?);
            }
            RequestCredentials::OtpCode {
                device_id,
                otp_secret,
                otp_code,
            } => {
                builder = builder
                    .header(DEVICE_ID_HEADER, device_id_header(device_id)?)
                    .header(OTP_SECRET_HEADER, otp_secret_header(otp_secret)?)
                    .header(OTP_CODE_HEADER, otp_code_header(otp_code)?);
            }
        }

        if let Some(json_body) = request.json_body {
            let reqwest_body = json_body
                .try_into_reqwest_copy()
                .map_err(|_| TransportError::RequestConstruction)?;
            builder = builder
                .header(CONTENT_TYPE, JSON_CONTENT_TYPE)
                .body(reqwest_body);
        } else if let Some(form_body) = request.form_body {
            let reqwest_body = form_body
                .try_into_reqwest_copy()
                .map_err(|_| TransportError::RequestConstruction)?;
            builder = builder
                .header(CONTENT_TYPE, FORM_CONTENT_TYPE)
                .body(reqwest_body);
        }

        let built = builder
            .build()
            .map_err(|_| TransportError::RequestConstruction)?;
        let response = match self.client.execute(built).await {
            Ok(response) => response,
            Err(error) => {
                trace_failure(request.method.as_str(), request.route_template, started_at);
                return Err(classify_send_error(request.operation, &error));
            }
        };

        let status = response.status();
        let otp_secret = match request.response_capture {
            ResponseCapture::None => None,
            ResponseCapture::OtpSecret => match capture_otp_secret(response.headers()) {
                Ok(otp_secret) => otp_secret,
                Err(failure) => {
                    trace_response_failure(
                        request.method.as_str(),
                        request.route_template,
                        status.as_u16(),
                        started_at,
                        "response-headers",
                    );
                    return Err(classify_post_send_failure(
                        request.operation,
                        failure,
                        self.response_limit,
                    ));
                }
            },
        };
        if status.is_redirection() {
            trace_response_failure(
                request.method.as_str(),
                request.route_template,
                status.as_u16(),
                started_at,
                "redirect",
            );
            return Err(classify_post_send_failure(
                request.operation,
                PostSendFailure::Redirect,
                self.response_limit,
            ));
        }

        let body = match read_bounded_body(response, self.response_limit).await {
            Ok(body) => body,
            Err(failure) => {
                trace_response_failure(
                    request.method.as_str(),
                    request.route_template,
                    status.as_u16(),
                    started_at,
                    "body-read",
                );
                return Err(classify_post_send_failure(
                    request.operation,
                    failure,
                    self.response_limit,
                ));
            }
        };
        trace_response(
            request.method.as_str(),
            request.route_template,
            status.as_u16(),
            started_at,
            &body,
            credentials,
        );

        Ok(HttpResponse {
            status,
            body,
            otp_secret,
        })
    }

    fn build_url(
        &self,
        endpoint: ApiEndpoint,
        path_segments: &[&str],
        query: &[(&str, &str)],
    ) -> Result<Url, TransportError> {
        build_url_at_endpoint(&self.base_url, endpoint, path_segments, query)
    }

    #[cfg(test)]
    pub(in crate::adapters::venmo) fn for_test(
        base_url: Url,
        request_timeout: Duration,
        response_limit: usize,
    ) -> Result<Self, TransportBuildError> {
        Self::build(
            base_url,
            request_timeout,
            request_timeout,
            request_timeout,
            response_limit,
            true,
        )
    }
}

fn build_url_at_endpoint(
    base_url: &Url,
    endpoint: ApiEndpoint,
    path_segments: &[&str],
    query: &[(&str, &str)],
) -> Result<Url, TransportError> {
    if endpoint == ApiEndpoint::V1 {
        return build_url_at_origin(base_url, path_segments, query);
    }

    let mut root = base_url.clone();
    let last_segment = root
        .path_segments()
        .ok_or(TransportError::InvalidRoute)?
        .rfind(|segment| !segment.is_empty());
    if last_segment != Some("v1") {
        return Err(TransportError::InvalidRoute);
    }
    root.path_segments_mut()
        .map_err(|()| TransportError::InvalidRoute)?
        .pop_if_empty()
        .pop();
    build_url_at_origin(&root, path_segments, query)
}

impl ApiTransport for VenmoHttpTransport {
    async fn execute<'a>(
        &'a self,
        credentials: RequestCredentials<'a>,
        request: HttpRequest<'a>,
    ) -> Result<HttpResponse, TransportError> {
        self.send(credentials, request).await
    }

    fn parse_trusted_next_link(
        &self,
        raw: &str,
        expected_path_segments: &[&str],
    ) -> Result<Vec<(String, String)>, TransportError> {
        VenmoHttpTransport::parse_trusted_next_link(self, raw, expected_path_segments)
    }
}

pub(super) fn parse_trusted_next_link_at_origin(
    base_url: &Url,
    raw: &str,
    expected_path_segments: &[&str],
) -> Result<Vec<(String, String)>, TransportError> {
    let url = Url::parse(raw).map_err(|_| TransportError::InvalidContinuationLink)?;
    let expected = build_url_at_origin(base_url, expected_path_segments, &[])?;
    if url.scheme() != expected.scheme()
        || url.host_str() != expected.host_str()
        || url.port_or_known_default() != expected.port_or_known_default()
        || url.path() != expected.path()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(TransportError::InvalidContinuationLink);
    }
    Ok(url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect())
}

fn build_url_at_origin(
    base_url: &Url,
    path_segments: &[&str],
    query: &[(&str, &str)],
) -> Result<Url, TransportError> {
    if path_segments.is_empty() {
        return Err(TransportError::InvalidRoute);
    }

    let mut url = base_url.clone();
    {
        let mut target = url
            .path_segments_mut()
            .map_err(|()| TransportError::InvalidRoute)?;
        target.pop_if_empty();
        for segment in path_segments {
            validate_path_segment(segment)?;
            target.push(segment);
        }
    }

    if !query.is_empty() {
        let mut pairs = url.query_pairs_mut();
        for (name, value) in query {
            if name.is_empty() || name.chars().any(char::is_control) {
                return Err(TransportError::InvalidQuery);
            }
            pairs.append_pair(name, value);
        }
    }

    Ok(url)
}

fn validate_base_url(
    base_url: &Url,
    allow_insecure_loopback: bool,
) -> Result<(), TransportBuildError> {
    let secure = base_url.scheme() == "https";
    let allowed_test_origin = allow_insecure_loopback
        && base_url.scheme() == "http"
        && base_url.host_str().is_some_and(is_loopback_host);
    if !secure && !allowed_test_origin {
        return Err(TransportBuildError::InsecureOrigin);
    }
    if base_url.host_str().is_none()
        || !base_url.username().is_empty()
        || base_url.password().is_some()
        || base_url.query().is_some()
        || base_url.fragment().is_some()
    {
        return Err(TransportBuildError::UnsafeOrigin);
    }
    Ok(())
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "::1" | "[::1]" | "localhost")
}

fn validate_path_segment(segment: &str) -> Result<(), TransportError> {
    if segment.is_empty()
        || matches!(segment, "." | "..")
        || segment
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\'))
    {
        return Err(TransportError::InvalidRoute);
    }
    Ok(())
}

fn authorization_header(access_token: &AccessToken) -> Result<HeaderValue, TransportError> {
    let mut bytes = Zeroizing::new(Vec::with_capacity(
        "Bearer ".len() + access_token.expose_secret().len(),
    ));
    bytes.extend_from_slice(b"Bearer ");
    bytes.extend_from_slice(access_token.expose_secret().as_bytes());
    let mut value = HeaderValue::from_bytes(bytes.as_slice())
        .map_err(|_| TransportError::InvalidAuthenticationHeader)?;
    value.set_sensitive(true);
    Ok(value)
}

fn device_id_header(device_id: &DeviceId) -> Result<HeaderValue, TransportError> {
    sensitive_request_header(device_id.as_str())
}

fn otp_secret_header(otp_secret: &OtpSecret) -> Result<HeaderValue, TransportError> {
    sensitive_request_header(otp_secret.expose())
}

fn otp_code_header(otp_code: &OtpCode) -> Result<HeaderValue, TransportError> {
    sensitive_request_header(otp_code.expose())
}

fn sensitive_request_header(secret: &str) -> Result<HeaderValue, TransportError> {
    let mut value =
        HeaderValue::from_str(secret).map_err(|_| TransportError::InvalidAuthenticationHeader)?;
    value.set_sensitive(true);
    Ok(value)
}

pub(super) fn capture_otp_secret(
    headers: &HeaderMap,
) -> Result<Option<Zeroizing<Vec<u8>>>, PostSendFailure> {
    let mut values = headers.get_all(&OTP_SECRET_HEADER).iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(PostSendFailure::ResponseCapture);
    }
    let raw = value.as_bytes();
    if raw.is_empty() || raw.len() > MAX_OTP_SECRET_HEADER_BYTES {
        return Err(PostSendFailure::ResponseCapture);
    }
    let mut captured = Zeroizing::new(Vec::new());
    captured
        .try_reserve_exact(raw.len())
        .map_err(|_| PostSendFailure::ResourceExhaustion)?;
    captured.extend_from_slice(raw);
    Ok(Some(captured))
}

fn trace_failure(method: &str, route_template: &str, started_at: Instant) {
    tracing::debug!(
        http.method = method,
        http.route = route_template,
        http.duration_ms = duration_millis(started_at.elapsed()),
        http.retry_count = 0_u8,
        "Venmo API transport failure"
    );
}

fn trace_response(
    method: &str,
    route_template: &str,
    status: u16,
    started_at: Instant,
    body: &[u8],
    credentials: RequestCredentials<'_>,
) {
    if !tracing::enabled!(tracing::Level::DEBUG) {
        return;
    }
    let redactions = credential_redactions(credentials);
    let diagnostics = ResponseDiagnostics::from_body(body, &redactions);
    tracing::debug!(
        http.method = method,
        http.route = route_template,
        http.status = status,
        http.duration_ms = duration_millis(started_at.elapsed()),
        http.retry_count = 0_u8,
        http.response_bytes = body.len(),
        http.body_kind = diagnostics.body_kind,
        venmo.error_code = ?diagnostics.error_code,
        venmo.error_title = ?diagnostics.error_title,
        venmo.error_message = ?diagnostics.error_message,
        "Venmo API response"
    );
}

fn trace_response_failure(
    method: &str,
    route_template: &str,
    status: u16,
    started_at: Instant,
    stage: &'static str,
) {
    tracing::debug!(
        http.method = method,
        http.route = route_template,
        http.status = status,
        http.duration_ms = duration_millis(started_at.elapsed()),
        http.retry_count = 0_u8,
        failure.stage = stage,
        "Venmo API response failure"
    );
}

fn credential_redactions(credentials: RequestCredentials<'_>) -> [Option<&str>; 3] {
    match credentials {
        RequestCredentials::Authenticated(session) => [
            Some(session.access_token.expose_secret()),
            Some(session.device_id.as_str()),
            None,
        ],
        RequestCredentials::Device(device_id) => [Some(device_id.as_str()), None, None],
        RequestCredentials::OtpSecret {
            device_id,
            otp_secret,
        } => [Some(device_id.as_str()), Some(otp_secret.expose()), None],
        RequestCredentials::OtpCode {
            device_id,
            otp_secret,
            otp_code,
        } => [
            Some(device_id.as_str()),
            Some(otp_secret.expose()),
            Some(otp_code.expose()),
        ],
    }
}

fn duration_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod debug_logging_tests {
    use std::error::Error;
    use std::io::{self, Write};
    use std::sync::{Arc, Mutex};

    use tracing::Level;
    use tracing_subscriber::fmt::MakeWriter;

    use super::*;
    use crate::adapters::venmo::transport::request::ApiSession;
    use crate::shared::{AccessToken, DeviceId};

    type TestResult = Result<(), Box<dyn Error>>;

    #[derive(Clone, Default)]
    struct CapturedWriter(Arc<Mutex<Vec<u8>>>);

    struct CapturedWriterGuard(Arc<Mutex<Vec<u8>>>);

    impl CapturedWriter {
        fn rendered(&self) -> Result<String, Box<dyn Error>> {
            let bytes = self
                .0
                .lock()
                .map_err(|_| io::Error::other("captured debug output lock was poisoned"))?
                .clone();
            Ok(String::from_utf8(bytes)?)
        }
    }

    impl<'a> MakeWriter<'a> for CapturedWriter {
        type Writer = CapturedWriterGuard;

        fn make_writer(&'a self) -> Self::Writer {
            CapturedWriterGuard(Arc::clone(&self.0))
        }
    }

    impl Write for CapturedWriterGuard {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.0
                .lock()
                .map_err(|_| io::Error::other("captured debug output lock was poisoned"))?
                .extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn response_debug_event_exposes_only_bounded_safe_fields() -> TestResult {
        let token = AccessToken::from_normalized_owned("private-token".to_owned())?;
        let device_id = DeviceId::from_owned("private-device".to_owned())?;
        let body = serde_json::to_vec(&serde_json::json!({
            "error": {
                "code": "10100",
                "title": "Payment blocked",
                "message": "private-token and private-device rejected OTP 123456 and source 123456789012"
            },
            "data": {
                "note": "private-note",
                "funding_source_id": "private-source",
                "otp": "123456"
            }
        }))?;
        let captured = CapturedWriter::default();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(Level::DEBUG)
            .with_writer(captured.clone())
            .with_ansi(false)
            .without_time()
            .with_target(false)
            .compact()
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            trace_response(
                "POST",
                "/payments",
                403,
                Instant::now(),
                &body,
                RequestCredentials::Authenticated(ApiSession::new(&token, &device_id)),
            );
        });
        let rendered = captured.rendered()?;

        for expected in [
            "http.method=\"POST\"",
            "http.route=\"/payments\"",
            "http.status=403",
            "http.retry_count=0",
            "http.body_kind=\"json\"",
            "10100",
            "Payment blocked",
            "[REDACTED] and [REDACTED] rejected OTP [REDACTED] and source [REDACTED]",
        ] {
            assert!(
                rendered.contains(expected),
                "missing {expected:?}: {rendered}"
            );
        }
        for private in [
            "private-token",
            "private-device",
            "private-note",
            "private-source",
            "123456",
        ] {
            assert!(
                !rendered.contains(private),
                "exposed {private:?}: {rendered}"
            );
        }
        Ok(())
    }
}

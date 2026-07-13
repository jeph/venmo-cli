use std::fmt;
use std::time::{Duration, Instant};

use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use reqwest::{Method, StatusCode, Url, redirect};
use thiserror::Error;
use zeroize::Zeroizing;

use crate::domain::{AccessToken, CredentialEnvelope, DeviceId, OtpCode, OtpSecret};

const PRODUCTION_API_BASE: &str = "https://api.venmo.com/v1/";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(15);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const COMPATIBILITY_USER_AGENT: &str = "Venmo/7.44.0 (iPhone; iOS 13.0; Scale/2.0)";
const DEVICE_ID_HEADER: HeaderName = HeaderName::from_static("device-id");
const OTP_CODE_HEADER: HeaderName = HeaderName::from_static("venmo-otp");
const OTP_SECRET_HEADER: HeaderName = HeaderName::from_static("venmo-otp-secret");
const MAX_OTP_SECRET_HEADER_BYTES: usize = 4096;
const JSON_ACCEPT: HeaderValue = HeaderValue::from_static("application/json");
const JSON_CONTENT_TYPE: HeaderValue = HeaderValue::from_static("application/json");

#[derive(Clone, Copy)]
pub(super) struct ApiSession<'a> {
    access_token: &'a AccessToken,
    device_id: &'a DeviceId,
}

impl<'a> ApiSession<'a> {
    #[must_use]
    pub(super) const fn new(access_token: &'a AccessToken, device_id: &'a DeviceId) -> Self {
        Self {
            access_token,
            device_id,
        }
    }
}

impl<'a> From<&'a CredentialEnvelope> for ApiSession<'a> {
    fn from(credential: &'a CredentialEnvelope) -> Self {
        Self::new(credential.access_token(), credential.device_id())
    }
}

impl fmt::Debug for ApiSession<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ApiSession")
            .field("access_token", &"[REDACTED]")
            .field("device_id", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Copy)]
enum RequestCredentials<'a> {
    None,
    Authenticated(ApiSession<'a>),
    Device(&'a DeviceId),
    OtpSecret {
        device_id: &'a DeviceId,
        otp_secret: &'a OtpSecret,
    },
    OtpCode {
        device_id: &'a DeviceId,
        otp_secret: &'a OtpSecret,
        otp_code: &'a OtpCode,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OperationClass {
    Read,
    NonFinancialWrite,
    AuthenticationWrite,
    #[allow(dead_code)]
    FinancialWrite,
}

enum RequestBody {
    Json(Vec<u8>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResponseCapture {
    None,
    OtpSecret,
}

pub(super) struct HttpRequest<'a> {
    method: Method,
    route_template: &'static str,
    path_segments: &'a [&'a str],
    query: &'a [(&'a str, &'a str)],
    body: Option<RequestBody>,
    operation: OperationClass,
    response_capture: ResponseCapture,
}

impl<'a> HttpRequest<'a> {
    #[must_use]
    pub(super) const fn read(
        route_template: &'static str,
        path_segments: &'a [&'a str],
        query: &'a [(&'a str, &'a str)],
    ) -> Self {
        Self {
            method: Method::GET,
            route_template,
            path_segments,
            query,
            body: None,
            operation: OperationClass::Read,
            response_capture: ResponseCapture::None,
        }
    }

    #[must_use]
    pub(super) const fn non_financial_delete(
        route_template: &'static str,
        path_segments: &'a [&'a str],
        query: &'a [(&'a str, &'a str)],
    ) -> Self {
        Self {
            method: Method::DELETE,
            route_template,
            path_segments,
            query,
            body: None,
            operation: OperationClass::NonFinancialWrite,
            response_capture: ResponseCapture::None,
        }
    }

    #[must_use]
    #[allow(dead_code)]
    pub(super) fn financial_json_post(
        route_template: &'static str,
        path_segments: &'a [&'a str],
        query: &'a [(&'a str, &'a str)],
        body: Vec<u8>,
    ) -> Self {
        Self {
            method: Method::POST,
            route_template,
            path_segments,
            query,
            body: Some(RequestBody::Json(body)),
            operation: OperationClass::FinancialWrite,
            response_capture: ResponseCapture::None,
        }
    }

    #[must_use]
    pub(super) fn financial_json_put(
        route_template: &'static str,
        path_segments: &'a [&'a str],
        query: &'a [(&'a str, &'a str)],
        body: Vec<u8>,
    ) -> Self {
        Self {
            method: Method::PUT,
            route_template,
            path_segments,
            query,
            body: Some(RequestBody::Json(body)),
            operation: OperationClass::FinancialWrite,
            response_capture: ResponseCapture::None,
        }
    }

    pub(super) fn password_login_json_post(
        route_template: &'static str,
        path_segments: &'a [&'a str],
        query: &'a [(&'a str, &'a str)],
        body: Vec<u8>,
    ) -> Self {
        Self {
            method: Method::POST,
            route_template,
            path_segments,
            query,
            body: Some(RequestBody::Json(body)),
            operation: OperationClass::AuthenticationWrite,
            response_capture: ResponseCapture::OtpSecret,
        }
    }

    pub(super) fn non_financial_json_post(
        route_template: &'static str,
        path_segments: &'a [&'a str],
        query: &'a [(&'a str, &'a str)],
        body: Vec<u8>,
    ) -> Self {
        Self {
            method: Method::POST,
            route_template,
            path_segments,
            query,
            body: Some(RequestBody::Json(body)),
            operation: OperationClass::NonFinancialWrite,
            response_capture: ResponseCapture::None,
        }
    }

    pub(super) const fn non_financial_post(
        route_template: &'static str,
        path_segments: &'a [&'a str],
        query: &'a [(&'a str, &'a str)],
    ) -> Self {
        Self {
            method: Method::POST,
            route_template,
            path_segments,
            query,
            body: None,
            operation: OperationClass::NonFinancialWrite,
            response_capture: ResponseCapture::None,
        }
    }

    pub(super) const fn authentication_post(
        route_template: &'static str,
        path_segments: &'a [&'a str],
        query: &'a [(&'a str, &'a str)],
    ) -> Self {
        Self {
            method: Method::POST,
            route_template,
            path_segments,
            query,
            body: None,
            operation: OperationClass::AuthenticationWrite,
            response_capture: ResponseCapture::None,
        }
    }
}

#[derive(Eq, PartialEq)]
pub(super) struct HttpResponse {
    status: StatusCode,
    body: Vec<u8>,
    otp_secret: Option<Zeroizing<Vec<u8>>>,
}

impl HttpResponse {
    #[must_use]
    pub(super) const fn status(&self) -> StatusCode {
        self.status
    }

    #[must_use]
    pub(super) fn body(&self) -> &[u8] {
        &self.body
    }

    pub(super) fn otp_secret(&self) -> Option<&[u8]> {
        self.otp_secret.as_ref().map(|value| value.as_slice())
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

pub(super) struct VenmoHttpTransport {
    client: reqwest::Client,
    base_url: Url,
    response_limit: usize,
}

impl VenmoHttpTransport {
    pub(super) fn production() -> Result<Self, TransportBuildError> {
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
            .user_agent(COMPATIBILITY_USER_AGENT)
            .build()
            .map_err(|_| TransportBuildError::ClientInitialization)?;

        Ok(Self {
            client,
            base_url,
            response_limit,
        })
    }

    pub(super) async fn send_authenticated(
        &self,
        session: ApiSession<'_>,
        request: HttpRequest<'_>,
    ) -> Result<HttpResponse, TransportError> {
        self.send(RequestCredentials::Authenticated(session), request)
            .await
    }

    pub(super) async fn send_unauthenticated(
        &self,
        request: HttpRequest<'_>,
    ) -> Result<HttpResponse, TransportError> {
        self.send(RequestCredentials::None, request).await
    }

    pub(super) fn parse_trusted_next_link(
        &self,
        raw: &str,
        expected_path_segments: &[&str],
    ) -> Result<Vec<(String, String)>, TransportError> {
        let url = Url::parse(raw).map_err(|_| TransportError::InvalidContinuationLink)?;
        let expected = self.build_url(expected_path_segments, &[])?;
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

    pub(super) async fn send_with_device_id(
        &self,
        device_id: &DeviceId,
        request: HttpRequest<'_>,
    ) -> Result<HttpResponse, TransportError> {
        self.send(RequestCredentials::Device(device_id), request)
            .await
    }

    pub(super) async fn send_with_otp_secret(
        &self,
        device_id: &DeviceId,
        otp_secret: &OtpSecret,
        request: HttpRequest<'_>,
    ) -> Result<HttpResponse, TransportError> {
        self.send(
            RequestCredentials::OtpSecret {
                device_id,
                otp_secret,
            },
            request,
        )
        .await
    }

    pub(super) async fn send_with_otp_code(
        &self,
        device_id: &DeviceId,
        otp_secret: &OtpSecret,
        otp_code: &OtpCode,
        request: HttpRequest<'_>,
    ) -> Result<HttpResponse, TransportError> {
        self.send(
            RequestCredentials::OtpCode {
                device_id,
                otp_secret,
                otp_code,
            },
            request,
        )
        .await
    }

    async fn send(
        &self,
        credentials: RequestCredentials<'_>,
        request: HttpRequest<'_>,
    ) -> Result<HttpResponse, TransportError> {
        let started_at = Instant::now();
        let url = self.build_url(request.path_segments, request.query)?;
        let mut builder = self
            .client
            .request(request.method.clone(), url)
            .header(ACCEPT, JSON_ACCEPT);
        match credentials {
            RequestCredentials::None => {}
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

        if let Some(body) = request.body {
            match body {
                RequestBody::Json(body) => {
                    builder = builder.header(CONTENT_TYPE, JSON_CONTENT_TYPE).body(body);
                }
            }
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
            ResponseCapture::OtpSecret => capture_otp_secret(response.headers())?,
        };
        tracing::debug!(
            http.method = request.method.as_str(),
            http.route = request.route_template,
            http.status = status.as_u16(),
            http.duration_ms = duration_millis(started_at.elapsed()),
            http.retry_count = 0_u8,
            "Venmo API response"
        );

        if status.is_redirection() {
            return Err(classify_post_send_failure(
                request.operation,
                PostSendFailure::Redirect,
                self.response_limit,
            ));
        }

        let body = read_bounded_body(response, self.response_limit)
            .await
            .map_err(|failure| {
                classify_post_send_failure(request.operation, failure, self.response_limit)
            })?;

        Ok(HttpResponse {
            status,
            body,
            otp_secret,
        })
    }

    fn build_url(
        &self,
        path_segments: &[&str],
        query: &[(&str, &str)],
    ) -> Result<Url, TransportError> {
        if path_segments.is_empty() {
            return Err(TransportError::InvalidRoute);
        }

        let mut url = self.base_url.clone();
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

    #[cfg(test)]
    pub(super) fn for_test(
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AmbiguousWriteCause {
    Timeout,
    Network,
    Redirect,
    ResponseRead,
    ResponseTooLarge,
    ResourceExhaustion,
}

impl fmt::Display for AmbiguousWriteCause {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Timeout => "the request timed out after transmission may have begun",
            Self::Network => "the connection failed after transmission may have begun",
            Self::Redirect => "the server returned an unexpected redirect",
            Self::ResponseRead => "the response ended before it could be read completely",
            Self::ResponseTooLarge => "the response exceeded the configured safety limit",
            Self::ResourceExhaustion => "the response could not be buffered safely",
        })
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum TransportBuildError {
    #[error("the production Venmo API origin is invalid")]
    InvalidProductionOrigin,

    #[error("the HTTP transport requires a fixed HTTPS origin")]
    InsecureOrigin,

    #[error("the HTTP transport origin cannot contain credentials, query data, or a fragment")]
    UnsafeOrigin,

    #[error("the HTTP response size limit must be positive")]
    InvalidResponseLimit,

    #[error("failed to initialize the HTTPS client")]
    ClientInitialization,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum TransportError {
    #[error("the API route is invalid")]
    InvalidRoute,

    #[error("the API query is invalid")]
    InvalidQuery,

    #[error("the API returned an untrusted or malformed continuation link")]
    InvalidContinuationLink,

    #[error("failed to construct a sensitive API header")]
    InvalidAuthenticationHeader,

    #[error("the API returned an invalid authentication response header")]
    InvalidAuthenticationResponseHeader,

    #[error("failed to construct the API request")]
    RequestConstruction,

    #[error("the API request timed out")]
    Timeout,

    #[error("the API request failed before a usable response was received")]
    Network,

    #[error("the API returned an unexpected redirect")]
    UnexpectedRedirect,

    #[error("the API response exceeded the {maximum_bytes}-byte safety limit")]
    ResponseTooLarge { maximum_bytes: usize },

    #[error("the API response ended before it could be read completely")]
    ResponseRead,

    #[error("the API response could not be buffered safely")]
    ResourceExhaustion,

    #[error(
        "financial write outcome is unknown: {cause}; do not retry until activity or requests and the official app verify the result"
    )]
    FinancialWriteOutcomeUnknown { cause: AmbiguousWriteCause },

    #[error("authentication outcome is unknown: {cause}; a remote token may have been issued")]
    AuthenticationOutcomeUnknown { cause: AmbiguousWriteCause },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PostSendFailure {
    Redirect,
    ResponseRead,
    ResponseTooLarge,
    ResourceExhaustion,
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
    matches!(host, "127.0.0.1" | "::1" | "localhost")
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

fn capture_otp_secret(headers: &HeaderMap) -> Result<Option<Zeroizing<Vec<u8>>>, TransportError> {
    let Some(value) = headers.get(&OTP_SECRET_HEADER) else {
        return Ok(None);
    };
    let raw = value.as_bytes();
    if raw.is_empty() || raw.len() > MAX_OTP_SECRET_HEADER_BYTES {
        return Err(TransportError::InvalidAuthenticationResponseHeader);
    }
    let mut captured = Zeroizing::new(Vec::new());
    captured
        .try_reserve_exact(raw.len())
        .map_err(|_| TransportError::ResourceExhaustion)?;
    captured.extend_from_slice(raw);
    Ok(Some(captured))
}

fn classify_send_error(operation: OperationClass, error: &reqwest::Error) -> TransportError {
    if error.is_connect() {
        return if error.is_timeout() {
            TransportError::Timeout
        } else {
            TransportError::Network
        };
    }
    let cause = if error.is_timeout() {
        AmbiguousWriteCause::Timeout
    } else {
        AmbiguousWriteCause::Network
    };
    match operation {
        OperationClass::FinancialWrite => {
            return TransportError::FinancialWriteOutcomeUnknown { cause };
        }
        OperationClass::AuthenticationWrite => {
            return TransportError::AuthenticationOutcomeUnknown { cause };
        }
        OperationClass::Read | OperationClass::NonFinancialWrite => {}
    }

    if error.is_timeout() {
        TransportError::Timeout
    } else {
        TransportError::Network
    }
}

fn classify_post_send_failure(
    operation: OperationClass,
    failure: PostSendFailure,
    maximum_bytes: usize,
) -> TransportError {
    let cause = match failure {
        PostSendFailure::Redirect => AmbiguousWriteCause::Redirect,
        PostSendFailure::ResponseRead => AmbiguousWriteCause::ResponseRead,
        PostSendFailure::ResponseTooLarge => AmbiguousWriteCause::ResponseTooLarge,
        PostSendFailure::ResourceExhaustion => AmbiguousWriteCause::ResourceExhaustion,
    };
    match operation {
        OperationClass::FinancialWrite => {
            return TransportError::FinancialWriteOutcomeUnknown { cause };
        }
        OperationClass::AuthenticationWrite => {
            return TransportError::AuthenticationOutcomeUnknown { cause };
        }
        OperationClass::Read | OperationClass::NonFinancialWrite => {}
    }

    match failure {
        PostSendFailure::Redirect => TransportError::UnexpectedRedirect,
        PostSendFailure::ResponseRead => TransportError::ResponseRead,
        PostSendFailure::ResponseTooLarge => TransportError::ResponseTooLarge { maximum_bytes },
        PostSendFailure::ResourceExhaustion => TransportError::ResourceExhaustion,
    }
}

async fn read_bounded_body(
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
        let new_length = body
            .len()
            .checked_add(chunk.len())
            .filter(|length| *length <= maximum_bytes)
            .ok_or(PostSendFailure::ResponseTooLarge)?;
        body.try_reserve(new_length.saturating_sub(body.capacity()))
            .map_err(|_| PostSendFailure::ResourceExhaustion)?;
        body.extend_from_slice(&chunk);
    }

    Ok(body)
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

fn duration_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::str::FromStr;

    use time::OffsetDateTime;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::domain::{AccessToken, DeviceId, UserId, Username};

    const TEST_TIMEOUT: Duration = Duration::from_secs(2);
    const TEST_RESPONSE_LIMIT: usize = 1024;

    #[tokio::test(flavor = "current_thread")]
    async fn sends_only_the_expected_authenticated_request()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/users/123"))
            .and(query_param("limit", "10"))
            .and(header("accept", "application/json"))
            .and(header("authorization", "Bearer secret-value"))
            .and(header("device-id", "device-value"))
            .and(header("user-agent", COMPATIBILITY_USER_AGENT))
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
    async fn bounds_response_bodies_before_buffering_them() -> Result<(), Box<dyn std::error::Error>>
    {
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
    async fn classifies_token_issuance_timeout_as_unknown() -> Result<(), Box<dyn std::error::Error>>
    {
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
                    b"{}".to_vec(),
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
                HttpRequest::financial_json_post("/payments", &["payments"], &[], b"{}".to_vec()),
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
    async fn classifies_financial_write_timeout_as_ambiguous()
    -> Result<(), Box<dyn std::error::Error>> {
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
                HttpRequest::financial_json_post("/payments", &["payments"], &[], b"{}".to_vec()),
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
                HttpRequest::financial_json_post("/payments", &["payments"], &[], b"{}".to_vec()),
            )
            .await;

        assert_eq!(result, Err(TransportError::Network));
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
    fn production_transport_has_a_valid_fixed_origin() {
        assert!(VenmoHttpTransport::production().is_ok());
    }

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
        let token = AccessToken::from_str("secret-value")?;
        let device_id = DeviceId::from_str("device-value")?;
        let user_id = UserId::from_str("123")?;
        let username = Username::from_str("@alice")?;
        Ok(CredentialEnvelope::new(
            token,
            device_id,
            user_id,
            username,
            Some("Alice".to_owned()),
            OffsetDateTime::UNIX_EPOCH,
        ))
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
}

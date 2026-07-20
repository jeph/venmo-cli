use std::future::Future;
use std::time::Duration;

use reqwest::header::{HeaderName, HeaderValue};

mod client;
mod error;
mod request;
mod response;
#[cfg(test)]
mod scripted;

pub(super) use client::VenmoHttpTransport;
#[cfg(test)]
pub(super) use error::AmbiguousWriteCause;
pub(crate) use error::{TransportBuildError, TransportError};
pub(super) use request::{ApiSession, FormBody, HttpRequest, JsonBody};
#[cfg(test)]
pub(super) use request::{OperationClass, ResponseCapture};
pub(super) use response::HttpResponse;
#[cfg(test)]
pub(super) use scripted::{
    ScriptedCredentials, ScriptedRequest, ScriptedResultSnapshot, ScriptedTransport,
    ScriptedTransportSnapshot,
};

use request::RequestCredentials;

const PRODUCTION_API_BASE: &str = "https://api.venmo.com/v1/";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(15);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const COMPATIBILITY_USER_AGENT: &str = "Venmo/26.13.0 (iPhone; iOS 18.6.2; Scale/3.0)";
const DEVICE_ID_HEADER: HeaderName = HeaderName::from_static("device-id");
const X_SESSION_ID_HEADER: HeaderName = HeaderName::from_static("x-session-id");
const OTP_CODE_HEADER: HeaderName = HeaderName::from_static("venmo-otp");
const OTP_SECRET_HEADER: HeaderName = HeaderName::from_static("venmo-otp-secret");
const MAX_OTP_SECRET_HEADER_BYTES: usize = 4096;
const JSON_ACCEPT: HeaderValue = HeaderValue::from_static("application/json; charset=utf-8");
const ENGLISH_ACCEPT_LANGUAGE: HeaderValue = HeaderValue::from_static("en-US;q=1.0");
const JSON_CONTENT_TYPE: HeaderValue = HeaderValue::from_static("application/json");
const FORM_CONTENT_TYPE: HeaderValue =
    HeaderValue::from_static("application/x-www-form-urlencoded");

pub(super) trait ApiTransport: Sync {
    fn execute<'a>(
        &'a self,
        credentials: RequestCredentials<'a>,
        request: HttpRequest<'a>,
    ) -> impl Future<Output = Result<HttpResponse, TransportError>> + Send + 'a;

    fn parse_trusted_next_link(
        &self,
        raw: &str,
        expected_path_segments: &[&str],
    ) -> Result<Vec<(String, String)>, TransportError>;

    fn send_authenticated<'a>(
        &'a self,
        session: ApiSession<'a>,
        request: HttpRequest<'a>,
    ) -> impl Future<Output = Result<HttpResponse, TransportError>> + Send + 'a {
        self.execute(RequestCredentials::Authenticated(session), request)
    }

    fn send_with_device_id<'a>(
        &'a self,
        device_id: &'a crate::shared::DeviceId,
        request: HttpRequest<'a>,
    ) -> impl Future<Output = Result<HttpResponse, TransportError>> + Send + 'a {
        self.execute(RequestCredentials::Device(device_id), request)
    }

    fn send_with_otp_secret<'a>(
        &'a self,
        device_id: &'a crate::shared::DeviceId,
        otp_secret: &'a crate::features::auth::OtpSecret,
        request: HttpRequest<'a>,
    ) -> impl Future<Output = Result<HttpResponse, TransportError>> + Send + 'a {
        self.execute(
            RequestCredentials::OtpSecret {
                device_id,
                otp_secret,
            },
            request,
        )
    }

    fn send_with_otp_code<'a>(
        &'a self,
        device_id: &'a crate::shared::DeviceId,
        otp_secret: &'a crate::features::auth::OtpSecret,
        otp_code: &'a crate::features::auth::OtpCode,
        request: HttpRequest<'a>,
    ) -> impl Future<Output = Result<HttpResponse, TransportError>> + Send + 'a {
        self.execute(
            RequestCredentials::OtpCode {
                device_id,
                otp_secret,
                otp_code,
            },
            request,
        )
    }
}

#[cfg(test)]
mod tests;

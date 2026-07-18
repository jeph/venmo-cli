use std::fmt;

use reqwest::Method;
use serde::Serialize;
use zeroize::Zeroizing;

use crate::features::auth::{OtpCode, OtpSecret};
use crate::shared::{AccessToken, CredentialEnvelope, DeviceId};

#[derive(Clone, Copy)]
pub(in crate::adapters::venmo) struct ApiSession<'a> {
    pub(super) access_token: &'a AccessToken,
    pub(super) device_id: &'a DeviceId,
}

impl<'a> ApiSession<'a> {
    #[must_use]
    pub(in crate::adapters::venmo) const fn new(
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> Self {
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
pub(in crate::adapters::venmo) enum RequestCredentials<'a> {
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
pub(in crate::adapters::venmo) enum OperationClass {
    Read,
    NonFinancialWrite,
    AuthenticationWrite,
    FinancialWrite,
}

pub(in crate::adapters::venmo) struct JsonBody {
    pub(super) bytes: Zeroizing<Vec<u8>>,
}

impl JsonBody {
    pub(in crate::adapters::venmo) fn encode<T>(value: &T) -> Result<Self, serde_json::Error>
    where
        T: Serialize + ?Sized,
    {
        let mut bytes = Zeroizing::new(Vec::new());
        serde_json::to_writer(&mut *bytes, value)?;
        Ok(Self { bytes })
    }

    pub(super) fn try_into_reqwest_copy(
        self,
    ) -> Result<Vec<u8>, std::collections::TryReserveError> {
        // reqwest cannot own the Zeroizing wrapper. Keep its unavoidable,
        // short-lived request-body copy at this transport boundary.
        let mut copy = Vec::new();
        copy.try_reserve_exact(self.bytes.len())?;
        copy.extend_from_slice(self.bytes.as_slice());
        Ok(copy)
    }
}

impl fmt::Debug for JsonBody {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JsonBody")
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::adapters::venmo) enum ResponseCapture {
    None,
    OtpSecret,
}

pub(in crate::adapters::venmo) struct HttpRequest<'a> {
    pub(super) method: Method,
    pub(super) route_template: &'static str,
    pub(super) path_segments: &'a [&'a str],
    pub(super) query: &'a [(&'a str, &'a str)],
    pub(super) json_body: Option<JsonBody>,
    pub(super) operation: OperationClass,
    pub(super) response_capture: ResponseCapture,
}

impl<'a> HttpRequest<'a> {
    #[must_use]
    pub(in crate::adapters::venmo) const fn read(
        route_template: &'static str,
        path_segments: &'a [&'a str],
        query: &'a [(&'a str, &'a str)],
    ) -> Self {
        Self::new(
            Method::GET,
            route_template,
            path_segments,
            query,
            None,
            OperationClass::Read,
            ResponseCapture::None,
        )
    }

    #[must_use]
    pub(in crate::adapters::venmo) fn financial_json_post(
        route_template: &'static str,
        path_segments: &'a [&'a str],
        query: &'a [(&'a str, &'a str)],
        json_body: JsonBody,
    ) -> Self {
        Self::new(
            Method::POST,
            route_template,
            path_segments,
            query,
            Some(json_body),
            OperationClass::FinancialWrite,
            ResponseCapture::None,
        )
    }

    #[must_use]
    pub(in crate::adapters::venmo) fn financial_json_put(
        route_template: &'static str,
        path_segments: &'a [&'a str],
        query: &'a [(&'a str, &'a str)],
        json_body: JsonBody,
    ) -> Self {
        Self::new(
            Method::PUT,
            route_template,
            path_segments,
            query,
            Some(json_body),
            OperationClass::FinancialWrite,
            ResponseCapture::None,
        )
    }

    pub(in crate::adapters::venmo) fn password_login_json_post(
        route_template: &'static str,
        path_segments: &'a [&'a str],
        query: &'a [(&'a str, &'a str)],
        json_body: JsonBody,
    ) -> Self {
        Self::new(
            Method::POST,
            route_template,
            path_segments,
            query,
            Some(json_body),
            OperationClass::AuthenticationWrite,
            ResponseCapture::OtpSecret,
        )
    }

    pub(in crate::adapters::venmo) fn non_financial_json_post(
        route_template: &'static str,
        path_segments: &'a [&'a str],
        query: &'a [(&'a str, &'a str)],
        json_body: JsonBody,
    ) -> Self {
        Self::new(
            Method::POST,
            route_template,
            path_segments,
            query,
            Some(json_body),
            OperationClass::NonFinancialWrite,
            ResponseCapture::None,
        )
    }

    pub(in crate::adapters::venmo) const fn non_financial_post(
        route_template: &'static str,
        path_segments: &'a [&'a str],
        query: &'a [(&'a str, &'a str)],
    ) -> Self {
        Self::new(
            Method::POST,
            route_template,
            path_segments,
            query,
            None,
            OperationClass::NonFinancialWrite,
            ResponseCapture::None,
        )
    }

    pub(in crate::adapters::venmo) const fn authentication_post(
        route_template: &'static str,
        path_segments: &'a [&'a str],
        query: &'a [(&'a str, &'a str)],
    ) -> Self {
        Self::new(
            Method::POST,
            route_template,
            path_segments,
            query,
            None,
            OperationClass::AuthenticationWrite,
            ResponseCapture::None,
        )
    }

    const fn new(
        method: Method,
        route_template: &'static str,
        path_segments: &'a [&'a str],
        query: &'a [(&'a str, &'a str)],
        json_body: Option<JsonBody>,
        operation: OperationClass,
        response_capture: ResponseCapture,
    ) -> Self {
        Self {
            method,
            route_template,
            path_segments,
            query,
            json_body,
            operation,
            response_capture,
        }
    }
}

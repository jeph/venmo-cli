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
    StateWrite,
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

pub(in crate::adapters::venmo) struct FormBody {
    pub(super) bytes: Zeroizing<Vec<u8>>,
}

impl FormBody {
    pub(in crate::adapters::venmo) fn user_id(user_id: &crate::shared::UserId) -> Self {
        let mut bytes = Zeroizing::new(Vec::with_capacity(
            "user_id=".len() + user_id.as_str().len(),
        ));
        bytes.extend_from_slice(b"user_id=");
        bytes.extend_from_slice(user_id.as_str().as_bytes());
        Self { bytes }
    }

    pub(super) fn try_into_reqwest_copy(
        self,
    ) -> Result<Vec<u8>, std::collections::TryReserveError> {
        let mut copy = Vec::new();
        copy.try_reserve_exact(self.bytes.len())?;
        copy.extend_from_slice(self.bytes.as_slice());
        Ok(copy)
    }
}

impl fmt::Debug for FormBody {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FormBody")
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
    pub(super) form_body: Option<FormBody>,
    pub(super) operation: OperationClass,
    pub(super) response_capture: ResponseCapture,
}

struct RequestBodies {
    json: Option<JsonBody>,
    form: Option<FormBody>,
}

impl RequestBodies {
    const NONE: Self = Self {
        json: None,
        form: None,
    };

    const fn json(body: JsonBody) -> Self {
        Self {
            json: Some(body),
            form: None,
        }
    }

    const fn form(body: FormBody) -> Self {
        Self {
            json: None,
            form: Some(body),
        }
    }
}

impl<'a> HttpRequest<'a> {
    #[must_use]
    pub(in crate::adapters::venmo) fn read(
        route_template: &'static str,
        path_segments: &'a [&'a str],
        query: &'a [(&'a str, &'a str)],
    ) -> Self {
        Self::new(
            Method::GET,
            route_template,
            path_segments,
            query,
            RequestBodies::NONE,
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
            RequestBodies::json(json_body),
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
            RequestBodies::json(json_body),
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
            RequestBodies::json(json_body),
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
            RequestBodies::json(json_body),
            OperationClass::NonFinancialWrite,
            ResponseCapture::None,
        )
    }

    pub(in crate::adapters::venmo) fn non_financial_post(
        route_template: &'static str,
        path_segments: &'a [&'a str],
        query: &'a [(&'a str, &'a str)],
    ) -> Self {
        Self::new(
            Method::POST,
            route_template,
            path_segments,
            query,
            RequestBodies::NONE,
            OperationClass::NonFinancialWrite,
            ResponseCapture::None,
        )
    }

    pub(in crate::adapters::venmo) fn authentication_post(
        route_template: &'static str,
        path_segments: &'a [&'a str],
        query: &'a [(&'a str, &'a str)],
    ) -> Self {
        Self::new(
            Method::POST,
            route_template,
            path_segments,
            query,
            RequestBodies::NONE,
            OperationClass::AuthenticationWrite,
            ResponseCapture::None,
        )
    }

    #[must_use]
    pub(in crate::adapters::venmo) fn state_form_post(
        route_template: &'static str,
        path_segments: &'a [&'a str],
        query: &'a [(&'a str, &'a str)],
        form_body: FormBody,
    ) -> Self {
        Self::new(
            Method::POST,
            route_template,
            path_segments,
            query,
            RequestBodies::form(form_body),
            OperationClass::StateWrite,
            ResponseCapture::None,
        )
    }

    #[must_use]
    pub(in crate::adapters::venmo) fn state_delete(
        route_template: &'static str,
        path_segments: &'a [&'a str],
        query: &'a [(&'a str, &'a str)],
    ) -> Self {
        Self::new(
            Method::DELETE,
            route_template,
            path_segments,
            query,
            RequestBodies::NONE,
            OperationClass::StateWrite,
            ResponseCapture::None,
        )
    }

    fn new(
        method: Method,
        route_template: &'static str,
        path_segments: &'a [&'a str],
        query: &'a [(&'a str, &'a str)],
        bodies: RequestBodies,
        operation: OperationClass,
        response_capture: ResponseCapture,
    ) -> Self {
        Self {
            method,
            route_template,
            path_segments,
            query,
            json_body: bodies.json,
            form_body: bodies.form,
            operation,
            response_capture,
        }
    }
}

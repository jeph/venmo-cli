use std::future::Future;
use std::str::FromStr;

use serde_json::Value;

use crate::features::auth::{
    AccountPassword, CurrentAccountApi, LoginIdentifier, OtpCode, OtpSecret, PasswordLoginApi,
    PasswordLoginStart,
};
use crate::shared::{AccessToken, Account, DeviceId, UserId, Username};

use super::super::dto::{AccountEnvelope, PasswordLoginRequest, SmsOtpRequest};
use super::super::transport::{ApiSession, ApiTransport, HttpRequest, HttpResponse, JsonBody};
use super::error::VenmoApiError;
use super::response::{
    decode_success, extract_error_code, parse_response_value, require_authenticated_success,
    require_success, require_success_parsed,
};
use super::{
    CURRENT_ACCOUNT_OPERATION, DEVICE_TRUST_OPERATION, OTP_COMPLETION_OPERATION,
    OTP_REQUEST_OPERATION, PASSWORD_LOGIN_OPERATION, VenmoApiClient,
};

impl<T: ApiTransport> VenmoApiClient<T> {
    async fn start_password_login(
        &self,
        identifier: &LoginIdentifier,
        password: &AccountPassword,
        device_id: &DeviceId,
    ) -> Result<PasswordLoginStart, VenmoApiError> {
        let body = JsonBody::encode(&PasswordLoginRequest {
            phone_email_or_username: identifier.expose(),
            client_id: "1",
            password: password.expose(),
        })
        .map_err(|_| VenmoApiError::RequestEncoding {
            operation: PASSWORD_LOGIN_OPERATION,
        })?;
        let response = self
            .transport
            .send_with_device_id(
                device_id,
                HttpRequest::password_login_json_post(
                    "/oauth/access_token",
                    &["oauth", "access_token"],
                    &[],
                    body,
                ),
            )
            .await?;
        parse_password_login_response(response)
    }

    async fn send_sms_otp(
        &self,
        otp_secret: &OtpSecret,
        device_id: &DeviceId,
    ) -> Result<(), VenmoApiError> {
        let body = JsonBody::encode(&SmsOtpRequest { via: "sms" }).map_err(|_| {
            VenmoApiError::RequestEncoding {
                operation: OTP_REQUEST_OPERATION,
            }
        })?;
        let response = self
            .transport
            .send_with_otp_secret(
                device_id,
                otp_secret,
                HttpRequest::non_financial_json_post(
                    "/account/two-factor/token",
                    &["account", "two-factor", "token"],
                    &[],
                    body,
                ),
            )
            .await?;
        require_success(OTP_REQUEST_OPERATION, response)
    }

    async fn finish_otp_login(
        &self,
        otp_code: &OtpCode,
        otp_secret: &OtpSecret,
        device_id: &DeviceId,
    ) -> Result<AccessToken, VenmoApiError> {
        let response = self
            .transport
            .send_with_otp_code(
                device_id,
                otp_secret,
                otp_code,
                HttpRequest::authentication_post(
                    "/oauth/access_token",
                    &["oauth", "access_token"],
                    &[("client_id", "1")],
                ),
            )
            .await?;
        let value = require_issued_token_json(OTP_COMPLETION_OPERATION, response)?;
        extract_access_token(OTP_COMPLETION_OPERATION, value)
    }

    async fn mark_device_trusted(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
    ) -> Result<(), VenmoApiError> {
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::non_financial_post("/users/devices", &["users", "devices"], &[]),
            )
            .await?;
        require_authenticated_success(DEVICE_TRUST_OPERATION, response)
    }

    pub(super) async fn fetch_current_account(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
    ) -> Result<Account, VenmoApiError> {
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::read("/account", &["account"], &[]),
            )
            .await?;
        let envelope: AccountEnvelope = decode_success(
            CURRENT_ACCOUNT_OPERATION,
            response,
            "the account response did not match the supported envelope",
        )?;
        let user = envelope.data.into_user();
        let user_id =
            UserId::from_str(&user.id.into_string()).map_err(|_| VenmoApiError::Contract {
                operation: CURRENT_ACCOUNT_OPERATION,
                problem: "the account response contained an invalid user ID",
            })?;
        let raw_username = user.username.ok_or(VenmoApiError::Contract {
            operation: CURRENT_ACCOUNT_OPERATION,
            problem: "the account response omitted the username",
        })?;
        let username = match raw_username.strip_prefix('@') {
            Some(bare) => bare.to_owned(),
            None => raw_username,
        };
        let username = Username::from_bare(username).map_err(|_| VenmoApiError::Contract {
            operation: CURRENT_ACCOUNT_OPERATION,
            problem: "the account response contained an invalid username",
        })?;
        Ok(Account::new(user_id, username, user.display_name))
    }
}

impl<T: ApiTransport> CurrentAccountApi for VenmoApiClient<T> {
    type Error = VenmoApiError;

    fn current_account<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a {
        self.fetch_current_account(access_token, device_id)
    }
}

impl<T: ApiTransport> PasswordLoginApi for VenmoApiClient<T> {
    type Error = VenmoApiError;

    fn begin_password_login<'a>(
        &'a self,
        identifier: &'a LoginIdentifier,
        password: &'a AccountPassword,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<PasswordLoginStart, Self::Error>> + Send + 'a {
        self.start_password_login(identifier, password, device_id)
    }

    fn request_sms_otp<'a>(
        &'a self,
        otp_secret: &'a OtpSecret,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
        self.send_sms_otp(otp_secret, device_id)
    }

    fn complete_otp_login<'a>(
        &'a self,
        otp_code: &'a OtpCode,
        otp_secret: &'a OtpSecret,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<AccessToken, Self::Error>> + Send + 'a {
        self.finish_otp_login(otp_code, otp_secret, device_id)
    }

    fn trust_device<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
        self.mark_device_trusted(access_token, device_id)
    }
}

fn parse_password_login_response(
    response: HttpResponse,
) -> Result<PasswordLoginStart, VenmoApiError> {
    let status = response.status();
    let value = parse_response_value(PASSWORD_LOGIN_OPERATION, status, response.body())
        .map_err(|error| issued_token_error(PASSWORD_LOGIN_OPERATION, error))?;
    let error_code = value.as_ref().and_then(extract_error_code);
    if error_code.as_deref() == Some("81109") {
        let raw_secret = response.otp_secret().ok_or(VenmoApiError::Contract {
            operation: PASSWORD_LOGIN_OPERATION,
            problem: "the OTP challenge omitted the required OTP secret",
        })?;
        let secret = std::str::from_utf8(raw_secret).map_err(|_| VenmoApiError::Contract {
            operation: PASSWORD_LOGIN_OPERATION,
            problem: "the OTP challenge returned an invalid OTP secret",
        })?;
        let secret =
            OtpSecret::parse_owned(secret.to_owned()).map_err(|_| VenmoApiError::Contract {
                operation: PASSWORD_LOGIN_OPERATION,
                problem: "the OTP challenge returned an invalid OTP secret",
            })?;
        return Ok(PasswordLoginStart::OtpRequired(secret));
    }
    let value = require_issued_token_value(PASSWORD_LOGIN_OPERATION, status, value)?;
    extract_access_token(PASSWORD_LOGIN_OPERATION, value).map(PasswordLoginStart::Authenticated)
}

fn require_issued_token_json(
    operation: &'static str,
    response: HttpResponse,
) -> Result<Value, VenmoApiError> {
    let status = response.status();
    let value = parse_response_value(operation, status, response.body())
        .map_err(|error| issued_token_error(operation, error))?;
    require_issued_token_value(operation, status, value)
}

fn require_issued_token_value(
    operation: &'static str,
    status: reqwest::StatusCode,
    value: Option<Value>,
) -> Result<Value, VenmoApiError> {
    require_success_parsed(operation, status, value)
        .and_then(|value| {
            value.ok_or(VenmoApiError::Contract {
                operation,
                problem: "the response body was empty",
            })
        })
        .map_err(|error| issued_token_error(operation, error))
}

fn issued_token_error(operation: &'static str, error: VenmoApiError) -> VenmoApiError {
    match error {
        VenmoApiError::MalformedJson { .. } | VenmoApiError::Contract { .. } => {
            VenmoApiError::AuthenticationOutcomeUnknown {
                operation,
                problem: "the successful response did not contain a usable JSON token payload",
            }
        }
        other => other,
    }
}

fn extract_access_token(
    operation: &'static str,
    mut value: Value,
) -> Result<AccessToken, VenmoApiError> {
    let token = if let Some(token) = value.get_mut("access_token") {
        token.take()
    } else if let Some(token) = value
        .get_mut("data")
        .and_then(|data| data.get_mut("access_token"))
    {
        token.take()
    } else {
        return Err(VenmoApiError::AuthenticationOutcomeUnknown {
            operation,
            problem: "the response omitted the access token",
        });
    };
    let Value::String(token) = token else {
        return Err(VenmoApiError::AuthenticationOutcomeUnknown {
            operation,
            problem: "the response omitted the access token",
        });
    };
    AccessToken::from_normalized_owned(token).map_err(|_| {
        VenmoApiError::AuthenticationOutcomeUnknown {
            operation,
            problem: "the response contained an invalid access token",
        }
    })
}

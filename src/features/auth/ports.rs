use std::future::Future;
use std::io;

use thiserror::Error;

use super::{
    AccountPassword, AccountPasswordParseError, LoginIdentifier, LoginIdentifierParseError,
    OtpCode, OtpCodeParseError, OtpSecret, PasswordLoginStart,
};
use crate::shared::{
    AccessToken, AccessTokenParseError, Account, ApiFailure, ApplicationFailureKind, DeviceId,
    DeviceIdParseError,
};

pub trait PromptAvailability {
    fn can_prompt(&self) -> bool;
}

/// Authentication-only input, including secret-bearing fields that implementations must not echo.
pub trait AuthenticationInput: PromptAvailability {
    fn read_login_identifier(&self, prompt: &str) -> Result<LoginIdentifier, PromptError>;
    fn read_account_password(&self, prompt: &str) -> Result<AccountPassword, PromptError>;
    fn read_otp_code(&self, prompt: &str) -> Result<OtpCode, PromptError>;
    fn read_access_token(&self, prompt: &str) -> Result<AccessToken, PromptError>;
    fn read_device_id(&self, prompt: &str) -> Result<DeviceId, PromptError>;
}

pub trait CurrentAccountApi {
    type Error: ApiFailure;

    fn current_account<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a;
}

pub trait TokenRevocationApi {
    type Error: ApiFailure;

    fn revoke_access_token<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;
}

pub trait PasswordLoginApi {
    type Error: ApiFailure;

    fn begin_password_login<'a>(
        &'a self,
        identifier: &'a LoginIdentifier,
        password: &'a AccountPassword,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<PasswordLoginStart, Self::Error>> + Send + 'a;

    fn request_sms_otp<'a>(
        &'a self,
        otp_secret: &'a OtpSecret,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;

    fn complete_otp_login<'a>(
        &'a self,
        otp_code: &'a OtpCode,
        otp_secret: &'a OtpSecret,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<AccessToken, Self::Error>> + Send + 'a;

    fn trust_device<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;
}

#[derive(Debug, Error)]
pub enum PromptError {
    #[error("cancelled")]
    Cancelled,

    #[error("an interactive terminal is required")]
    NotInteractive,

    #[error("the bearer token is invalid: {source}")]
    InvalidAccessToken {
        #[source]
        source: AccessTokenParseError,
    },

    #[error("the Venmo device ID is invalid: {source}")]
    InvalidDeviceId {
        #[source]
        source: DeviceIdParseError,
    },

    #[error("the Venmo account identifier is invalid: {source}")]
    InvalidLoginIdentifier {
        #[source]
        source: LoginIdentifierParseError,
    },

    #[error("the Venmo password is invalid: {source}")]
    InvalidAccountPassword {
        #[source]
        source: AccountPasswordParseError,
    },

    #[error("the Venmo SMS code is invalid: {source}")]
    InvalidOtpCode {
        #[source]
        source: OtpCodeParseError,
    },

    #[error("terminal interaction failed")]
    Interaction {
        #[source]
        source: io::Error,
    },
}

pub(crate) const fn prompt_failure_kind(error: &PromptError) -> ApplicationFailureKind {
    match error {
        PromptError::Cancelled => ApplicationFailureKind::Cancelled,
        PromptError::NotInteractive
        | PromptError::InvalidAccessToken { .. }
        | PromptError::InvalidDeviceId { .. }
        | PromptError::InvalidLoginIdentifier { .. }
        | PromptError::InvalidAccountPassword { .. }
        | PromptError::InvalidOtpCode { .. } => ApplicationFailureKind::Usage,
        PromptError::Interaction { .. } => ApplicationFailureKind::Internal,
    }
}

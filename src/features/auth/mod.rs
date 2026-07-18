mod login;
mod logout;
mod model;
mod persistence;
mod ports;
mod status;
#[cfg(test)]
mod tests;

pub(crate) use login::{
    DeviceTrustOutcome, LoginDisposition, LoginError, LoginResult, PasswordLoginReport,
    login_with_password,
};
pub(crate) use logout::{LocalDeletionOutcome, LogoutReport, logout_local};
pub(crate) use model::{
    AccountPassword, AccountPasswordParseError, LoginIdentifier, LoginIdentifierParseError,
    OtpCode, OtpCodeParseError, OtpSecret, PasswordLoginStart,
};
pub(crate) use ports::prompt_failure_kind;
pub(crate) use ports::{
    AuthenticationInput, CurrentAccountApi, PasswordLoginApi, PromptAvailability, PromptError,
};
pub(crate) use status::{AuthStatus, AuthStatusError, status};

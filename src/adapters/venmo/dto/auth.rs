use serde::{Deserialize, Serialize};

use super::people::UserData;

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct PasswordLoginRequest<'a> {
    pub phone_email_or_username: &'a str,
    pub client_id: &'static str,
    pub password: &'a str,
}

#[derive(Serialize)]
pub(crate) struct SmsOtpRequest {
    pub via: &'static str,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AccountEnvelope {
    pub data: UserData,
}

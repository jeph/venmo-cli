use super::*;

pub(super) const REDACTED: &str = "[REDACTED]";
pub(super) const PRIMARY_USER_ID: &str = "100";
pub(super) const SECONDARY_USER_ID: &str = "200";
pub(super) const USERNAME: &str = "alice";
pub(super) const DISPLAY_NAME: &str = "Alice";
pub(super) const STORED_TOKEN: &str = "synthetic-stored-access-token";
pub(super) const IMPORTED_TOKEN: &str = "synthetic-imported-access-token";
pub(super) const ISSUED_TOKEN: &str = "synthetic-issued-access-token";
pub(super) const MISMATCHED_TOKEN: &str = "synthetic-mismatched-access-token";
pub(super) const STORED_DEVICE: &str = "synthetic-stored-device";
pub(super) const PROMPTED_DEVICE: &str = "synthetic-prompted-device";
pub(super) const LOGIN_IDENTIFIER: &str = "alice@example.com";
pub(super) const ACCOUNT_PASSWORD: &str = "synthetic-password";
pub(super) const OTP_CODE: &str = "123456";
pub(super) const OTP_SECRET: &str = "synthetic-otp-secret";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AccountIdentity {
    Primary,
    Secondary,
    Unknown,
}

impl AccountIdentity {
    pub(super) const fn user_id(self) -> &'static str {
        match self {
            Self::Primary => PRIMARY_USER_ID,
            Self::Secondary => SECONDARY_USER_ID,
            Self::Unknown => "300",
        }
    }

    pub(super) fn classify(user_id: &str) -> Self {
        match user_id {
            PRIMARY_USER_ID => Self::Primary,
            SECONDARY_USER_ID => Self::Secondary,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(super) struct AccountSnapshot {
    pub(super) identity: AccountIdentity,
    pub(super) username: String,
    pub(super) display_name: Option<String>,
}

impl AccountSnapshot {
    pub(super) fn synthetic(identity: AccountIdentity) -> Self {
        Self {
            identity,
            username: USERNAME.to_owned(),
            display_name: Some(DISPLAY_NAME.to_owned()),
        }
    }

    pub(super) fn from_account(account: &Account) -> Self {
        Self {
            identity: AccountIdentity::classify(account.user_id().as_str()),
            username: account.username().as_str().to_owned(),
            display_name: account.display_name().map(str::to_owned),
        }
    }
}

impl fmt::Debug for AccountSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AccountSnapshot")
            .field("identity", &self.identity)
            .field("username", &REDACTED)
            .field("display_name", &REDACTED)
            .finish()
    }
}

#[derive(Clone, Copy)]
pub(super) enum TokenMaterial {
    Stored,
    Imported,
    Issued,
    Mismatched,
}

impl TokenMaterial {
    pub(super) const fn value(self) -> &'static str {
        match self {
            Self::Stored => STORED_TOKEN,
            Self::Imported => IMPORTED_TOKEN,
            Self::Issued => ISSUED_TOKEN,
            Self::Mismatched => MISMATCHED_TOKEN,
        }
    }

    pub(super) const fn snapshot(self) -> SecretSnapshot {
        match self {
            Self::Stored => SecretSnapshot::Stored,
            Self::Imported => SecretSnapshot::Imported,
            Self::Issued => SecretSnapshot::Issued,
            Self::Mismatched => SecretSnapshot::Mismatched,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum DeviceMaterial {
    Stored,
    Prompted,
}

impl DeviceMaterial {
    pub(super) const fn value(self) -> &'static str {
        match self {
            Self::Stored => STORED_DEVICE,
            Self::Prompted => PROMPTED_DEVICE,
        }
    }

    pub(super) const fn snapshot(self) -> SecretSnapshot {
        match self {
            Self::Stored => SecretSnapshot::Stored,
            Self::Prompted => SecretSnapshot::Prompted,
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum SecretSnapshot {
    Stored,
    Imported,
    Issued,
    Prompted,
    Mismatched,
    Unknown,
}

impl fmt::Debug for SecretSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

pub(super) fn token_snapshot(value: &str) -> SecretSnapshot {
    match value {
        STORED_TOKEN => SecretSnapshot::Stored,
        IMPORTED_TOKEN => SecretSnapshot::Imported,
        ISSUED_TOKEN => SecretSnapshot::Issued,
        MISMATCHED_TOKEN => SecretSnapshot::Mismatched,
        _ => SecretSnapshot::Unknown,
    }
}

pub(super) fn device_snapshot(value: &str) -> SecretSnapshot {
    match value {
        STORED_DEVICE => SecretSnapshot::Stored,
        PROMPTED_DEVICE => SecretSnapshot::Prompted,
        _ => SecretSnapshot::Unknown,
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(super) struct CredentialSnapshot {
    pub(super) access_token: SecretSnapshot,
    pub(super) device_id: SecretSnapshot,
    pub(super) account: AccountSnapshot,
    pub(super) saved_at: OffsetDateTime,
    pub(super) format: CredentialFormat,
}

impl CredentialSnapshot {
    pub(super) fn synthetic(
        token: TokenMaterial,
        device: DeviceMaterial,
        account: AccountIdentity,
        saved_at: OffsetDateTime,
    ) -> Self {
        Self {
            access_token: token.snapshot(),
            device_id: device.snapshot(),
            account: AccountSnapshot::synthetic(account),
            saved_at,
            format: CredentialFormat::Version1,
        }
    }

    pub(super) fn from_envelope(envelope: &CredentialEnvelope, format: CredentialFormat) -> Self {
        Self {
            access_token: token_snapshot(envelope.access_token().expose_secret()),
            device_id: device_snapshot(envelope.device_id().as_str()),
            account: AccountSnapshot {
                identity: AccountIdentity::classify(envelope.user_id().as_str()),
                username: envelope.username().as_str().to_owned(),
                display_name: envelope.display_name().map(str::to_owned),
            },
            saved_at: envelope.saved_at(),
            format,
        }
    }
}

impl fmt::Debug for CredentialSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialSnapshot")
            .field("access_token", &self.access_token)
            .field("device_id", &self.device_id)
            .field("account", &self.account)
            .field("saved_at", &self.saved_at)
            .field("format", &self.format)
            .finish()
    }
}

#[derive(Clone)]
pub(super) struct CredentialFixture {
    pub(super) token: String,
    pub(super) device_id: String,
    pub(super) user_id: String,
    pub(super) username: String,
    pub(super) display_name: Option<String>,
    pub(super) saved_at: OffsetDateTime,
    pub(super) format: CredentialFormat,
}

impl CredentialFixture {
    pub(super) fn synthetic(
        account: AccountIdentity,
        token: TokenMaterial,
        device: DeviceMaterial,
        saved_at: OffsetDateTime,
    ) -> Self {
        Self {
            token: token.value().to_owned(),
            device_id: device.value().to_owned(),
            user_id: account.user_id().to_owned(),
            username: USERNAME.to_owned(),
            display_name: Some(DISPLAY_NAME.to_owned()),
            saved_at,
            format: CredentialFormat::Version1,
        }
    }

    pub(super) fn stored(account: AccountIdentity) -> Self {
        Self::synthetic(
            account,
            TokenMaterial::Stored,
            DeviceMaterial::Stored,
            OffsetDateTime::UNIX_EPOCH,
        )
    }

    pub(super) fn from_credential(credential: &CredentialEnvelope) -> Self {
        Self {
            token: credential.access_token().expose_secret().to_owned(),
            device_id: credential.device_id().as_str().to_owned(),
            user_id: credential.user_id().as_str().to_owned(),
            username: credential.username().as_str().to_owned(),
            display_name: credential.display_name().map(str::to_owned),
            saved_at: credential.saved_at(),
            format: CredentialFormat::Version1,
        }
    }

    pub(super) fn load(&self) -> Result<LoadedCredential, FakeCredentialError> {
        let envelope = CredentialEnvelope::new(
            AccessToken::from_normalized_owned(self.token.clone())
                .map_err(|_| FakeCredentialError::internal())?,
            DeviceId::from_owned(self.device_id.clone())
                .map_err(|_| FakeCredentialError::internal())?,
            UserId::from_str(&self.user_id).map_err(|_| FakeCredentialError::internal())?,
            Username::from_bare(self.username.clone())
                .map_err(|_| FakeCredentialError::internal())?,
            self.display_name.clone(),
            self.saved_at,
        );
        Ok(LoadedCredential {
            envelope,
            format: self.format,
        })
    }

    pub(super) fn snapshot(&self) -> CredentialSnapshot {
        CredentialSnapshot {
            access_token: token_snapshot(&self.token),
            device_id: device_snapshot(&self.device_id),
            account: AccountSnapshot {
                identity: AccountIdentity::classify(&self.user_id),
                username: self.username.clone(),
                display_name: self.display_name.clone(),
            },
            saved_at: self.saved_at,
            format: self.format,
        }
    }
}

#[derive(Clone, Copy, Debug, Error)]
#[error("synthetic credential-store failure")]
pub(super) struct FakeCredentialError {
    pub(super) kind: CredentialFailureKind,
}

impl FakeCredentialError {
    pub(super) const fn internal() -> Self {
        Self {
            kind: CredentialFailureKind::Internal,
        }
    }

    pub(super) const fn platform() -> Self {
        Self {
            kind: CredentialFailureKind::Platform,
        }
    }
}

impl CredentialStoreFailure for FakeCredentialError {
    fn kind(&self) -> CredentialFailureKind {
        self.kind
    }
}

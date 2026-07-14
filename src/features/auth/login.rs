use std::fmt;

use thiserror::Error;
use time::OffsetDateTime;

use super::persistence::{CredentialOrigin, ExistingCredential, persist_validated_login};
use super::{
    AccountPassword, AuthenticationInput, CurrentAccountApi, LoginIdentifier, PasswordLoginApi,
    PasswordLoginStart, PromptError, prompt_failure_kind,
};
use crate::shared::{
    AccessToken, Account, ApiOperationFailure, ApplicationFailureKind, Clock, CredentialEnvelope,
    CredentialReader, CredentialStoreFailure, CredentialWriter, DeviceId, OperationFailure,
};

const ACCESS_TOKEN_PROMPT: &str = "Bearer token";
const IMPORT_DEVICE_ID_PROMPT: &str = "Matching Venmo v_id/device ID";
pub(super) const TRUSTED_DEVICE_ID_PROMPT: &str = "Trusted Venmo v_id/device ID";
pub(super) const ACCOUNT_IDENTIFIER_PROMPT: &str = "Venmo email, phone, or username";
pub(super) const ACCOUNT_PASSWORD_PROMPT: &str = "Venmo password";
const OTP_CODE_PROMPT: &str = "SMS verification code";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoginDisposition {
    Created,
    ReplacedForSameAccount,
    RecoveredUnusableEntry,
}

#[derive(Eq, PartialEq)]
pub struct LoginResult {
    account: Account,
    saved_at: OffsetDateTime,
    disposition: LoginDisposition,
}

impl fmt::Debug for LoginResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoginResult")
            .field("account", self.account())
            .field("saved_at", &self.saved_at())
            .field("disposition", &self.disposition())
            .finish()
    }
}

pub(super) enum IssuedPasswordLogin {
    AlreadyTrusted(AccessToken),
    RequiresDeviceTrust(AccessToken),
}

impl IssuedPasswordLogin {
    #[must_use]
    pub(super) fn access_token(&self) -> &AccessToken {
        match self {
            Self::AlreadyTrusted(access_token) | Self::RequiresDeviceTrust(access_token) => {
                access_token
            }
        }
    }
}

#[derive(Debug)]
pub enum DeviceTrustOutcome {
    NotNeeded,
    Trusted,
    Failed(ApiOperationFailure),
}

impl DeviceTrustOutcome {
    #[cfg(test)]
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(self, Self::NotNeeded | Self::Trusted)
    }
}

#[derive(Debug)]
pub struct PasswordLoginReport {
    login: LoginResult,
    device_trust: DeviceTrustOutcome,
}

impl PasswordLoginReport {
    #[must_use]
    pub(crate) fn new(login: LoginResult, device_trust: DeviceTrustOutcome) -> Self {
        Self {
            login,
            device_trust,
        }
    }

    #[must_use]
    pub const fn login(&self) -> &LoginResult {
        &self.login
    }

    #[must_use]
    pub const fn device_trust(&self) -> &DeviceTrustOutcome {
        &self.device_trust
    }

    #[cfg(test)]
    #[must_use]
    pub const fn is_complete_success(&self) -> bool {
        self.device_trust.is_complete()
    }
}

impl LoginResult {
    #[must_use]
    pub(crate) fn new(
        account: Account,
        saved_at: OffsetDateTime,
        disposition: LoginDisposition,
    ) -> Self {
        Self {
            account,
            saved_at,
            disposition,
        }
    }

    #[must_use]
    pub fn account(&self) -> &Account {
        &self.account
    }

    #[must_use]
    pub const fn saved_at(&self) -> OffsetDateTime {
        self.saved_at
    }

    #[must_use]
    pub const fn disposition(&self) -> LoginDisposition {
        self.disposition
    }
}

#[derive(Debug, Error)]
pub enum LoginError {
    #[error(transparent)]
    Prompt(#[from] PromptError),

    #[error("failed to read the existing OS credential entry: {source}")]
    CredentialLoad {
        #[source]
        source: OperationFailure,
    },

    #[error(
        "a readable Venmo credential is already stored; run `venmo auth logout` before password login"
    )]
    CredentialAlreadyStored,

    #[error(
        "no readable Venmo credential is stored; run `venmo auth login` before reauthenticating"
    )]
    ReauthenticationCredentialMissing,

    #[error("the bearer token could not be validated: {source}")]
    TokenValidation {
        #[source]
        source: ApiOperationFailure,
    },

    #[error(
        "Venmo issued a token, but it could not be validated and was not stored; the remote token may remain active, so review official Venmo session controls: {source}"
    )]
    IssuedTokenValidation {
        #[source]
        source: ApiOperationFailure,
    },

    #[error("Venmo password authentication failed: {source}")]
    PasswordAuthentication {
        #[source]
        source: ApiOperationFailure,
    },

    #[error("failed to request a Venmo SMS verification code: {source}")]
    OtpRequest {
        #[source]
        source: ApiOperationFailure,
    },

    #[error("Venmo SMS verification failed: {source}")]
    OtpCompletion {
        #[source]
        source: ApiOperationFailure,
    },

    #[error(
        "the bearer token belongs to a different account; run `venmo auth logout` before switching accounts"
    )]
    DifferentAccount,

    #[error(
        "Venmo issued a token for a different account, so the stored credential was left unchanged; the remote token may remain active, so review official Venmo session controls"
    )]
    IssuedTokenDifferentAccount,

    #[error(
        "the OS credential update could not be verified; credential storage state is unknown, so run `venmo auth status`"
    )]
    CredentialStorageStateUnknown {
        #[source]
        source: Option<OperationFailure>,
    },

    #[error(
        "the issued token's OS credential update could not be verified; credential storage state is unknown and the remote token may remain active, so run `venmo auth status` and review official Venmo session controls"
    )]
    IssuedCredentialStorageStateUnknown {
        #[source]
        source: Option<OperationFailure>,
    },
}

impl LoginError {
    #[must_use]
    pub const fn failure_kind(&self) -> ApplicationFailureKind {
        match self {
            Self::Prompt(source) => prompt_failure_kind(source),
            Self::CredentialLoad { .. }
            | Self::CredentialAlreadyStored
            | Self::ReauthenticationCredentialMissing
            | Self::DifferentAccount
            | Self::IssuedTokenDifferentAccount
            | Self::CredentialStorageStateUnknown { .. }
            | Self::IssuedCredentialStorageStateUnknown { .. } => {
                ApplicationFailureKind::Credential
            }
            Self::TokenValidation { source }
            | Self::IssuedTokenValidation { source }
            | Self::PasswordAuthentication { source }
            | Self::OtpRequest { source }
            | Self::OtpCompletion { source } => ApplicationFailureKind::Api(source.kind()),
        }
    }
}

pub async fn login_with_token<S, P, A, C>(
    store: &S,
    prompt: &P,
    api: &A,
    clock: &C,
) -> Result<LoginResult, LoginError>
where
    S: CredentialReader + CredentialWriter,
    P: AuthenticationInput,
    A: CurrentAccountApi,
    C: Clock,
{
    if !prompt.can_prompt() {
        return Err(LoginError::Prompt(PromptError::NotInteractive));
    }

    let existing = match store.read_credential() {
        Ok(Some(loaded)) => ExistingCredential::Loaded(loaded),
        Ok(None) => ExistingCredential::Missing,
        Err(source) if source.kind().permits_validated_replacement() => {
            ExistingCredential::Replaceable
        }
        Err(source) => {
            return Err(LoginError::CredentialLoad {
                source: OperationFailure::new(source),
            });
        }
    };

    let access_token = prompt.read_access_token(ACCESS_TOKEN_PROMPT)?;
    let device_id = match &existing {
        ExistingCredential::Loaded(loaded) => loaded.envelope.device_id().clone(),
        ExistingCredential::Missing | ExistingCredential::Replaceable => {
            prompt.read_device_id(IMPORT_DEVICE_ID_PROMPT)?
        }
    };

    let account = api
        .current_account(&access_token, &device_id)
        .await
        .map_err(|source| LoginError::TokenValidation {
            source: ApiOperationFailure::new(source),
        })?;

    if let ExistingCredential::Loaded(loaded) = &existing
        && loaded.envelope.user_id() != account.user_id()
    {
        return Err(LoginError::DifferentAccount);
    }

    let (result, _credential) = persist_validated_login(
        store,
        clock,
        existing,
        account,
        access_token,
        device_id,
        CredentialOrigin::Imported,
    )?;
    Ok(result)
}

pub async fn login_with_password<S, P, A, C>(
    store: &S,
    prompt: &P,
    api: &A,
    clock: &C,
) -> Result<PasswordLoginReport, LoginError>
where
    S: CredentialReader + CredentialWriter,
    P: AuthenticationInput,
    A: CurrentAccountApi + PasswordLoginApi,
    C: Clock,
{
    if !prompt.can_prompt() {
        return Err(LoginError::Prompt(PromptError::NotInteractive));
    }

    let existing = match store.read_credential() {
        Ok(Some(_)) => return Err(LoginError::CredentialAlreadyStored),
        Ok(None) => ExistingCredential::Missing,
        Err(source) if source.kind().permits_validated_replacement() => {
            ExistingCredential::Replaceable
        }
        Err(source) => {
            return Err(LoginError::CredentialLoad {
                source: OperationFailure::new(source),
            });
        }
    };
    let identifier = prompt.read_login_identifier(ACCOUNT_IDENTIFIER_PROMPT)?;
    let password = prompt.read_account_password(ACCOUNT_PASSWORD_PROMPT)?;
    let device_id = prompt.read_device_id(TRUSTED_DEVICE_ID_PROMPT)?;
    let issued_login = issue_password_login(prompt, api, &device_id, identifier, password).await?;
    let account = api
        .current_account(issued_login.access_token(), &device_id)
        .await
        .map_err(|source| LoginError::IssuedTokenValidation {
            source: ApiOperationFailure::new(source),
        })?;
    persist_issued_password_login(
        store,
        api,
        clock,
        existing,
        account,
        issued_login,
        device_id,
    )
    .await
}

pub(super) async fn issue_password_login<P, A>(
    prompt: &P,
    api: &A,
    device_id: &DeviceId,
    identifier: LoginIdentifier,
    password: AccountPassword,
) -> Result<IssuedPasswordLogin, LoginError>
where
    P: AuthenticationInput,
    A: PasswordLoginApi,
{
    let start = api
        .begin_password_login(&identifier, &password, device_id)
        .await
        .map_err(|source| LoginError::PasswordAuthentication {
            source: ApiOperationFailure::new(source),
        })?;
    drop(password);
    drop(identifier);

    match start {
        PasswordLoginStart::Authenticated(access_token) => {
            Ok(IssuedPasswordLogin::AlreadyTrusted(access_token))
        }
        PasswordLoginStart::OtpRequired(otp_secret) => {
            api.request_sms_otp(&otp_secret, device_id)
                .await
                .map_err(|source| LoginError::OtpRequest {
                    source: ApiOperationFailure::new(source),
                })?;
            let otp_code = prompt.read_otp_code(OTP_CODE_PROMPT)?;
            let access_token = api
                .complete_otp_login(&otp_code, &otp_secret, device_id)
                .await
                .map_err(|source| LoginError::OtpCompletion {
                    source: ApiOperationFailure::new(source),
                })?;
            Ok(IssuedPasswordLogin::RequiresDeviceTrust(access_token))
        }
    }
}

pub(super) async fn persist_issued_password_login<S, A, C>(
    store: &S,
    api: &A,
    clock: &C,
    existing: ExistingCredential,
    account: Account,
    issued_login: IssuedPasswordLogin,
    device_id: DeviceId,
) -> Result<PasswordLoginReport, LoginError>
where
    S: CredentialReader + CredentialWriter,
    A: PasswordLoginApi,
    C: Clock,
{
    match issued_login {
        IssuedPasswordLogin::AlreadyTrusted(access_token) => {
            let (login, _) = persist_issued_access_token(
                store,
                clock,
                existing,
                account,
                access_token,
                device_id,
            )?;
            Ok(PasswordLoginReport::new(
                login,
                DeviceTrustOutcome::NotNeeded,
            ))
        }
        IssuedPasswordLogin::RequiresDeviceTrust(access_token) => {
            let (login, credential) = persist_issued_access_token(
                store,
                clock,
                existing,
                account,
                access_token,
                device_id,
            )?;
            let device_trust = match api
                .trust_device(credential.access_token(), credential.device_id())
                .await
            {
                Ok(()) => DeviceTrustOutcome::Trusted,
                Err(source) => DeviceTrustOutcome::Failed(ApiOperationFailure::new(source)),
            };
            Ok(PasswordLoginReport::new(login, device_trust))
        }
    }
}

fn persist_issued_access_token<S, C>(
    store: &S,
    clock: &C,
    existing: ExistingCredential,
    account: Account,
    access_token: AccessToken,
    device_id: DeviceId,
) -> Result<(LoginResult, CredentialEnvelope), LoginError>
where
    S: CredentialReader + CredentialWriter,
    C: Clock,
{
    persist_validated_login(
        store,
        clock,
        existing,
        account,
        access_token,
        device_id,
        CredentialOrigin::Issued,
    )
}

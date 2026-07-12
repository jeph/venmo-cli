use std::error::Error;
use std::fmt;

use thiserror::Error;
use time::OffsetDateTime;

use super::ports::{
    AuthenticationApi, Clock, CredentialDeleteOutcome, CredentialFormat, CredentialStore,
    CredentialStoreFailure, LoadedCredential, PasswordLoginApi, PromptError, PromptPort,
};
use crate::domain::{
    Account, AccountPassword, CredentialEnvelope, LoginIdentifier, PasswordLoginStart,
};

const ACCESS_TOKEN_PROMPT: &str = "Bearer token";
const IMPORT_DEVICE_ID_PROMPT: &str = "Matching Venmo v_id/device ID";
const TRUSTED_DEVICE_ID_PROMPT: &str = "Trusted Venmo v_id/device ID";
const ACCOUNT_IDENTIFIER_PROMPT: &str = "Venmo email, phone, or username";
const ACCOUNT_PASSWORD_PROMPT: &str = "Venmo password";
const OTP_CODE_PROMPT: &str = "SMS verification code";
const REDACTED: &str = "[REDACTED]";

type BoxError = Box<dyn Error + Send + Sync>;

pub struct OperationFailure {
    source: BoxError,
}

impl OperationFailure {
    pub(crate) fn new(source: impl Error + Send + Sync + 'static) -> Self {
        Self {
            source: Box::new(source),
        }
    }

    #[must_use]
    pub fn source_error(&self) -> &(dyn Error + Send + Sync + 'static) {
        self.source.as_ref()
    }
}

impl fmt::Debug for OperationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OperationFailure")
            .field("source", &REDACTED)
            .finish()
    }
}

impl fmt::Display for OperationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.source.fmt(formatter)
    }
}

impl Error for OperationFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoginDisposition {
    Created,
    ReplacedForSameAccount,
    RecoveredUnusableEntry,
}

#[derive(Debug)]
pub struct LoginResult {
    account: Account,
    saved_at: OffsetDateTime,
    disposition: LoginDisposition,
}

#[derive(Debug)]
pub enum DeviceTrustOutcome {
    NotNeeded,
    Trusted,
    Failed(OperationFailure),
}

impl DeviceTrustOutcome {
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
        source: OperationFailure,
    },

    #[error(
        "Venmo issued a token, but it could not be validated and was not stored; the remote token may remain active, so review official Venmo session controls: {source}"
    )]
    IssuedTokenValidation {
        #[source]
        source: OperationFailure,
    },

    #[error("Venmo password authentication failed: {source}")]
    PasswordAuthentication {
        #[source]
        source: OperationFailure,
    },

    #[error("failed to request a Venmo SMS verification code: {source}")]
    OtpRequest {
        #[source]
        source: OperationFailure,
    },

    #[error("Venmo SMS verification failed: {source}")]
    OtpCompletion {
        #[source]
        source: OperationFailure,
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

#[derive(Clone, Copy)]
enum CredentialOrigin {
    Imported,
    Issued,
}

enum ExistingCredential {
    Missing,
    Loaded(LoadedCredential),
    Replaceable,
}

pub async fn login_with_token<S, P, A, C>(
    store: &S,
    prompt: &P,
    api: &A,
    clock: &C,
) -> Result<LoginResult, LoginError>
where
    S: CredentialStore,
    P: PromptPort,
    A: AuthenticationApi,
    C: Clock,
{
    if !prompt.can_prompt() {
        return Err(LoginError::Prompt(PromptError::NotInteractive));
    }

    let existing = match store.load() {
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
            source: OperationFailure::new(source),
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
    S: CredentialStore,
    P: PromptPort,
    A: AuthenticationApi + PasswordLoginApi,
    C: Clock,
{
    if !prompt.can_prompt() {
        return Err(LoginError::Prompt(PromptError::NotInteractive));
    }

    let existing = match store.load() {
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
    let (access_token, requires_device_trust) =
        issue_password_login(prompt, api, &device_id, identifier, password).await?;
    let account = api
        .current_account(&access_token, &device_id)
        .await
        .map_err(|source| LoginError::IssuedTokenValidation {
            source: OperationFailure::new(source),
        })?;
    persist_issued_password_login(
        store,
        api,
        clock,
        existing,
        account,
        access_token,
        device_id,
        requires_device_trust,
    )
    .await
}

/// Issues a replacement token while preserving the readable stored credential's device identity.
///
/// The stored access token is deliberately not validated: this operation exists so an expired
/// token can be replaced without losing the device ID that Venmo already recognizes.
pub async fn reauthenticate<S, P, A, C>(
    store: &S,
    prompt: &P,
    api: &A,
    clock: &C,
) -> Result<PasswordLoginReport, LoginError>
where
    S: CredentialStore,
    P: PromptPort,
    A: AuthenticationApi + PasswordLoginApi,
    C: Clock,
{
    if !prompt.can_prompt() {
        return Err(LoginError::Prompt(PromptError::NotInteractive));
    }

    let loaded = store
        .load()
        .map_err(|source| LoginError::CredentialLoad {
            source: OperationFailure::new(source),
        })?
        .ok_or(LoginError::ReauthenticationCredentialMissing)?;
    let expected_user_id = loaded.envelope.user_id().clone();
    let device_id = loaded.envelope.device_id().clone();

    let identifier = prompt.read_login_identifier(ACCOUNT_IDENTIFIER_PROMPT)?;
    let password = prompt.read_account_password(ACCOUNT_PASSWORD_PROMPT)?;
    let (access_token, requires_device_trust) =
        issue_password_login(prompt, api, &device_id, identifier, password).await?;
    let account = api
        .current_account(&access_token, &device_id)
        .await
        .map_err(|source| LoginError::IssuedTokenValidation {
            source: OperationFailure::new(source),
        })?;
    if account.user_id() != &expected_user_id {
        return Err(LoginError::IssuedTokenDifferentAccount);
    }

    persist_issued_password_login(
        store,
        api,
        clock,
        ExistingCredential::Loaded(loaded),
        account,
        access_token,
        device_id,
        requires_device_trust,
    )
    .await
}

async fn issue_password_login<P, A>(
    prompt: &P,
    api: &A,
    device_id: &crate::domain::DeviceId,
    identifier: LoginIdentifier,
    password: AccountPassword,
) -> Result<(crate::domain::AccessToken, bool), LoginError>
where
    P: PromptPort,
    A: PasswordLoginApi,
{
    let start = api
        .begin_password_login(&identifier, &password, device_id)
        .await
        .map_err(|source| LoginError::PasswordAuthentication {
            source: OperationFailure::new(source),
        })?;
    drop(password);
    drop(identifier);

    let (access_token, requires_device_trust) = match start {
        PasswordLoginStart::Authenticated(access_token) => (access_token, false),
        PasswordLoginStart::OtpRequired(otp_secret) => {
            api.request_sms_otp(&otp_secret, device_id)
                .await
                .map_err(|source| LoginError::OtpRequest {
                    source: OperationFailure::new(source),
                })?;
            let otp_code = prompt.read_otp_code(OTP_CODE_PROMPT)?;
            let access_token = api
                .complete_otp_login(&otp_code, &otp_secret, device_id)
                .await
                .map_err(|source| LoginError::OtpCompletion {
                    source: OperationFailure::new(source),
                })?;
            (access_token, true)
        }
    };
    Ok((access_token, requires_device_trust))
}

#[allow(clippy::too_many_arguments)]
async fn persist_issued_password_login<S, A, C>(
    store: &S,
    api: &A,
    clock: &C,
    existing: ExistingCredential,
    account: Account,
    access_token: crate::domain::AccessToken,
    device_id: crate::domain::DeviceId,
    requires_device_trust: bool,
) -> Result<PasswordLoginReport, LoginError>
where
    S: CredentialStore,
    A: PasswordLoginApi,
    C: Clock,
{
    let (login, credential) = persist_validated_login(
        store,
        clock,
        existing,
        account,
        access_token,
        device_id,
        CredentialOrigin::Issued,
    )?;
    let device_trust = if requires_device_trust {
        match api
            .trust_device(credential.access_token(), credential.device_id())
            .await
        {
            Ok(()) => DeviceTrustOutcome::Trusted,
            Err(source) => DeviceTrustOutcome::Failed(OperationFailure::new(source)),
        }
    } else {
        DeviceTrustOutcome::NotNeeded
    };

    Ok(PasswordLoginReport::new(login, device_trust))
}

fn persist_validated_login<S, C>(
    store: &S,
    clock: &C,
    existing: ExistingCredential,
    account: Account,
    access_token: crate::domain::AccessToken,
    device_id: crate::domain::DeviceId,
    origin: CredentialOrigin,
) -> Result<(LoginResult, CredentialEnvelope), LoginError>
where
    S: CredentialStore,
    C: Clock,
{
    let disposition = match existing {
        ExistingCredential::Missing => LoginDisposition::Created,
        ExistingCredential::Loaded(_) => LoginDisposition::ReplacedForSameAccount,
        ExistingCredential::Replaceable => LoginDisposition::RecoveredUnusableEntry,
    };
    let saved_at = clock.now_utc();
    let credential = CredentialEnvelope::new(
        access_token,
        device_id,
        account.user_id().clone(),
        account.username().clone(),
        account.display_name().map(str::to_owned),
        saved_at,
    );

    store
        .save(&credential)
        .map_err(|source| storage_state_unknown(origin, Some(OperationFailure::new(source))))?;
    let verification = store
        .load()
        .map_err(|source| storage_state_unknown(origin, Some(OperationFailure::new(source))))?;
    let Some(verification) = verification else {
        return Err(storage_state_unknown(origin, None));
    };
    if verification.format != CredentialFormat::Version1
        || !credential.storage_equivalent(&verification.envelope)
    {
        return Err(storage_state_unknown(origin, None));
    }

    Ok((LoginResult::new(account, saved_at, disposition), credential))
}

fn storage_state_unknown(origin: CredentialOrigin, source: Option<OperationFailure>) -> LoginError {
    match origin {
        CredentialOrigin::Imported => LoginError::CredentialStorageStateUnknown { source },
        CredentialOrigin::Issued => LoginError::IssuedCredentialStorageStateUnknown { source },
    }
}

#[derive(Debug)]
pub struct AuthStatus {
    account: Account,
    saved_at: OffsetDateTime,
    credential_format: CredentialFormat,
}

impl AuthStatus {
    #[must_use]
    pub fn account(&self) -> &Account {
        &self.account
    }

    #[must_use]
    pub const fn saved_at(&self) -> OffsetDateTime {
        self.saved_at
    }

    #[must_use]
    pub const fn credential_format(&self) -> CredentialFormat {
        self.credential_format
    }
}

#[derive(Debug, Error)]
pub enum AuthStatusError {
    #[error("no Venmo credential is stored; run `venmo auth login`")]
    MissingCredential,

    #[error("failed to read the OS credential entry: {source}")]
    CredentialLoad {
        #[source]
        source: OperationFailure,
    },

    #[error("the stored bearer token could not be validated: {source}")]
    TokenValidation {
        #[source]
        source: OperationFailure,
    },

    #[error("the live Venmo account does not match the account stored with this credential")]
    AccountMismatch,
}

pub async fn status<S, A>(store: &S, api: &A) -> Result<AuthStatus, AuthStatusError>
where
    S: CredentialStore,
    A: AuthenticationApi,
{
    let loaded = store
        .load()
        .map_err(|source| AuthStatusError::CredentialLoad {
            source: OperationFailure::new(source),
        })?
        .ok_or(AuthStatusError::MissingCredential)?;
    let account = api
        .current_account(loaded.envelope.access_token(), loaded.envelope.device_id())
        .await
        .map_err(|source| AuthStatusError::TokenValidation {
            source: OperationFailure::new(source),
        })?;
    if account.user_id() != loaded.envelope.user_id() {
        return Err(AuthStatusError::AccountMismatch);
    }

    Ok(AuthStatus {
        account,
        saved_at: loaded.envelope.created_at(),
        credential_format: loaded.format,
    })
}

#[derive(Debug)]
pub enum RemoteRevocationOutcome {
    NotRequested,
    NotNeeded,
    Revoked,
    Failed(OperationFailure),
    NotAttempted(OperationFailure),
}

impl RemoteRevocationOutcome {
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(self, Self::NotRequested | Self::NotNeeded | Self::Revoked)
    }
}

#[derive(Debug)]
pub enum LocalDeletionOutcome {
    Deleted,
    Missing,
    Failed(OperationFailure),
}

impl LocalDeletionOutcome {
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(self, Self::Deleted | Self::Missing)
    }
}

#[derive(Debug)]
pub struct LogoutReport {
    remote: RemoteRevocationOutcome,
    local: LocalDeletionOutcome,
}

impl LogoutReport {
    #[must_use]
    pub const fn remote(&self) -> &RemoteRevocationOutcome {
        &self.remote
    }

    #[must_use]
    pub const fn local(&self) -> &LocalDeletionOutcome {
        &self.local
    }

    #[must_use]
    pub const fn is_complete_success(&self) -> bool {
        self.remote.is_complete() && self.local.is_complete()
    }
}

pub async fn logout<S, A>(store: &S, api: &A, revoke: bool) -> LogoutReport
where
    S: CredentialStore,
    A: AuthenticationApi,
{
    if !revoke {
        return logout_local(store);
    }

    let remote = {
        match store.load() {
            Ok(Some(loaded)) => match api
                .revoke_access_token(loaded.envelope.access_token(), loaded.envelope.device_id())
                .await
            {
                Ok(()) => RemoteRevocationOutcome::Revoked,
                Err(source) => RemoteRevocationOutcome::Failed(OperationFailure::new(source)),
            },
            Ok(None) => RemoteRevocationOutcome::NotNeeded,
            Err(source) => RemoteRevocationOutcome::NotAttempted(OperationFailure::new(source)),
        }
    };

    let local = delete_local(store);

    LogoutReport { remote, local }
}

pub fn logout_local<S>(store: &S) -> LogoutReport
where
    S: CredentialStore,
{
    LogoutReport {
        remote: RemoteRevocationOutcome::NotRequested,
        local: delete_local(store),
    }
}

fn delete_local<S>(store: &S) -> LocalDeletionOutcome
where
    S: CredentialStore,
{
    match store.delete() {
        Ok(CredentialDeleteOutcome::Deleted) => LocalDeletionOutcome::Deleted,
        Ok(CredentialDeleteOutcome::Missing) => LocalDeletionOutcome::Missing,
        Err(source) => LocalDeletionOutcome::Failed(OperationFailure::new(source)),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::future::{Future, ready};
    use std::str::FromStr;

    use super::*;
    use crate::application::ports::{CredentialFailureKind, CredentialStoreFailure};
    use crate::domain::{
        AccessToken, AccountPassword, DeviceId, LoginIdentifier, OtpCode, OtpSecret, UserId,
        Username,
    };

    type TestResult = Result<(), Box<dyn Error>>;

    #[derive(Clone)]
    struct CredentialFixture {
        token: String,
        device_id: String,
        user_id: String,
        username: String,
        display_name: Option<String>,
        saved_at: OffsetDateTime,
        format: CredentialFormat,
    }

    impl CredentialFixture {
        fn standard(user_id: &str, token: &str, device_id: &str) -> Self {
            Self {
                token: token.to_owned(),
                device_id: device_id.to_owned(),
                user_id: user_id.to_owned(),
                username: "alice".to_owned(),
                display_name: Some("Alice".to_owned()),
                saved_at: OffsetDateTime::UNIX_EPOCH,
                format: CredentialFormat::Version1,
            }
        }

        fn from_credential(credential: &CredentialEnvelope) -> Self {
            Self {
                token: credential.access_token().expose_secret().to_owned(),
                device_id: credential.device_id().as_str().to_owned(),
                user_id: credential.user_id().as_str().to_owned(),
                username: credential.username().as_str().to_owned(),
                display_name: credential.display_name().map(str::to_owned),
                saved_at: credential.created_at(),
                format: CredentialFormat::Version1,
            }
        }

        fn load(&self) -> Result<LoadedCredential, FakeCredentialError> {
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
    }

    #[derive(Clone)]
    enum FakeStoreState {
        Missing,
        Present(CredentialFixture),
        Failure(CredentialFailureKind),
    }

    #[derive(Clone, Copy)]
    enum SaveBehavior {
        Normal,
        FailAfterWrite,
        ReadBackFailure,
        ReadBackMissing,
        StoreMismatch,
    }

    struct FakeStore {
        state: RefCell<FakeStoreState>,
        save_behavior: Cell<SaveBehavior>,
        delete_fails: Cell<bool>,
        load_count: Cell<u32>,
        save_count: Cell<u32>,
        delete_count: Cell<u32>,
    }

    impl FakeStore {
        fn new(state: FakeStoreState) -> Self {
            Self {
                state: RefCell::new(state),
                save_behavior: Cell::new(SaveBehavior::Normal),
                delete_fails: Cell::new(false),
                load_count: Cell::new(0),
                save_count: Cell::new(0),
                delete_count: Cell::new(0),
            }
        }

        fn stored_fixture(&self) -> Option<CredentialFixture> {
            match &*self.state.borrow() {
                FakeStoreState::Present(fixture) => Some(fixture.clone()),
                FakeStoreState::Missing | FakeStoreState::Failure(_) => None,
            }
        }
    }

    impl CredentialStore for FakeStore {
        type Error = FakeCredentialError;

        fn load(&self) -> Result<Option<LoadedCredential>, Self::Error> {
            self.load_count.set(self.load_count.get() + 1);
            match &*self.state.borrow() {
                FakeStoreState::Missing => Ok(None),
                FakeStoreState::Present(fixture) => fixture.load().map(Some),
                FakeStoreState::Failure(kind) => Err(FakeCredentialError { kind: *kind }),
            }
        }

        fn save(&self, credential: &CredentialEnvelope) -> Result<(), Self::Error> {
            self.save_count.set(self.save_count.get() + 1);
            let mut fixture = CredentialFixture::from_credential(credential);
            match self.save_behavior.get() {
                SaveBehavior::Normal => {
                    *self.state.borrow_mut() = FakeStoreState::Present(fixture);
                    Ok(())
                }
                SaveBehavior::FailAfterWrite => {
                    *self.state.borrow_mut() = FakeStoreState::Present(fixture);
                    Err(FakeCredentialError::platform())
                }
                SaveBehavior::ReadBackFailure => {
                    *self.state.borrow_mut() =
                        FakeStoreState::Failure(CredentialFailureKind::Platform);
                    Ok(())
                }
                SaveBehavior::ReadBackMissing => {
                    *self.state.borrow_mut() = FakeStoreState::Missing;
                    Ok(())
                }
                SaveBehavior::StoreMismatch => {
                    fixture.token = "different-token".to_owned();
                    *self.state.borrow_mut() = FakeStoreState::Present(fixture);
                    Ok(())
                }
            }
        }

        fn delete(&self) -> Result<CredentialDeleteOutcome, Self::Error> {
            self.delete_count.set(self.delete_count.get() + 1);
            if self.delete_fails.get() {
                return Err(FakeCredentialError::platform());
            }
            let outcome = if matches!(&*self.state.borrow(), FakeStoreState::Missing) {
                CredentialDeleteOutcome::Missing
            } else {
                CredentialDeleteOutcome::Deleted
            };
            *self.state.borrow_mut() = FakeStoreState::Missing;
            Ok(outcome)
        }
    }

    #[derive(Clone, Copy, Debug, Error)]
    #[error("synthetic credential-store failure")]
    struct FakeCredentialError {
        kind: CredentialFailureKind,
    }

    impl FakeCredentialError {
        const fn internal() -> Self {
            Self {
                kind: CredentialFailureKind::Internal,
            }
        }

        const fn platform() -> Self {
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

    struct FakePrompt {
        interactive: bool,
        token: &'static str,
        device_id: &'static str,
        identifier: &'static str,
        password: &'static str,
        otp_code: &'static str,
        read_count: Cell<u32>,
        device_id_read_count: Cell<u32>,
        identifier_read_count: Cell<u32>,
        password_read_count: Cell<u32>,
        otp_read_count: Cell<u32>,
        prompt_order: RefCell<Vec<&'static str>>,
    }

    impl FakePrompt {
        fn interactive(token: &'static str) -> Self {
            Self {
                interactive: true,
                token,
                device_id: "matching-device",
                identifier: "alice@example.com",
                password: "synthetic-password",
                otp_code: "123456",
                read_count: Cell::new(0),
                device_id_read_count: Cell::new(0),
                identifier_read_count: Cell::new(0),
                password_read_count: Cell::new(0),
                otp_read_count: Cell::new(0),
                prompt_order: RefCell::new(Vec::new()),
            }
        }

        fn password_login() -> Self {
            Self::interactive("unused-token-import")
        }

        fn noninteractive(token: &'static str) -> Self {
            let mut prompt = Self::interactive(token);
            prompt.interactive = false;
            prompt
        }
    }

    impl PromptPort for FakePrompt {
        fn can_prompt(&self) -> bool {
            self.interactive
        }

        fn read_login_identifier(&self, _prompt: &str) -> Result<LoginIdentifier, PromptError> {
            self.prompt_order.borrow_mut().push("identifier");
            self.identifier_read_count
                .set(self.identifier_read_count.get() + 1);
            LoginIdentifier::parse_owned(self.identifier.to_owned())
                .map_err(|source| PromptError::InvalidLoginIdentifier { source })
        }

        fn read_account_password(&self, _prompt: &str) -> Result<AccountPassword, PromptError> {
            self.prompt_order.borrow_mut().push("password");
            self.password_read_count
                .set(self.password_read_count.get() + 1);
            AccountPassword::parse_owned(self.password.to_owned())
                .map_err(|source| PromptError::InvalidAccountPassword { source })
        }

        fn read_otp_code(&self, _prompt: &str) -> Result<OtpCode, PromptError> {
            self.prompt_order.borrow_mut().push("otp");
            self.otp_read_count.set(self.otp_read_count.get() + 1);
            OtpCode::parse_owned(self.otp_code.to_owned())
                .map_err(|source| PromptError::InvalidOtpCode { source })
        }

        fn read_access_token(&self, _prompt: &str) -> Result<AccessToken, PromptError> {
            self.prompt_order.borrow_mut().push("token");
            self.read_count.set(self.read_count.get() + 1);
            AccessToken::from_str(self.token)
                .map_err(|source| PromptError::InvalidAccessToken { source })
        }

        fn read_device_id(&self, _prompt: &str) -> Result<DeviceId, PromptError> {
            self.prompt_order.borrow_mut().push("device");
            self.device_id_read_count
                .set(self.device_id_read_count.get() + 1);
            DeviceId::from_str(self.device_id)
                .map_err(|source| PromptError::InvalidDeviceId { source })
        }

        fn confirm_default_no(&self, _prompt: &str) -> Result<bool, PromptError> {
            Err(PromptError::Cancelled)
        }

        fn select(&self, _prompt: &str, _sanitized_items: &[String]) -> Result<usize, PromptError> {
            Err(PromptError::Cancelled)
        }
    }

    #[derive(Clone, Copy, Debug, Error)]
    #[error("synthetic API failure")]
    struct FakeApiError;

    struct FakeApi {
        account: Account,
        current_account_fails: bool,
        revoke_fails: bool,
        password_start: FakePasswordStart,
        otp_request_fails: bool,
        otp_completion_fails: bool,
        trust_fails: bool,
        current_account_count: Cell<u32>,
        revoke_count: Cell<u32>,
        password_start_count: Cell<u32>,
        otp_request_count: Cell<u32>,
        otp_completion_count: Cell<u32>,
        trust_count: Cell<u32>,
        current_account_token: RefCell<Option<String>>,
        current_account_device: RefCell<Option<String>>,
        password_start_device: RefCell<Option<String>>,
        otp_request_device: RefCell<Option<String>>,
        otp_completion_device: RefCell<Option<String>>,
        trust_device: RefCell<Option<String>>,
    }

    #[derive(Clone, Copy)]
    enum FakePasswordStart {
        Token,
        Otp,
        Fail,
    }

    impl FakeApi {
        fn new(account: Account) -> Self {
            Self {
                account,
                current_account_fails: false,
                revoke_fails: false,
                password_start: FakePasswordStart::Token,
                otp_request_fails: false,
                otp_completion_fails: false,
                trust_fails: false,
                current_account_count: Cell::new(0),
                revoke_count: Cell::new(0),
                password_start_count: Cell::new(0),
                otp_request_count: Cell::new(0),
                otp_completion_count: Cell::new(0),
                trust_count: Cell::new(0),
                current_account_token: RefCell::new(None),
                current_account_device: RefCell::new(None),
                password_start_device: RefCell::new(None),
                otp_request_device: RefCell::new(None),
                otp_completion_device: RefCell::new(None),
                trust_device: RefCell::new(None),
            }
        }
    }

    impl AuthenticationApi for FakeApi {
        type Error = FakeApiError;

        fn current_account<'a>(
            &'a self,
            access_token: &'a AccessToken,
            device_id: &'a DeviceId,
        ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a {
            self.current_account_count
                .set(self.current_account_count.get() + 1);
            *self.current_account_token.borrow_mut() =
                Some(access_token.expose_secret().to_owned());
            *self.current_account_device.borrow_mut() = Some(device_id.as_str().to_owned());
            ready(if self.current_account_fails {
                Err(FakeApiError)
            } else {
                Ok(self.account.clone())
            })
        }

        fn revoke_access_token<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
        ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
            self.revoke_count.set(self.revoke_count.get() + 1);
            ready(if self.revoke_fails {
                Err(FakeApiError)
            } else {
                Ok(())
            })
        }
    }

    impl PasswordLoginApi for FakeApi {
        type Error = FakeApiError;

        fn begin_password_login<'a>(
            &'a self,
            _identifier: &'a LoginIdentifier,
            _password: &'a AccountPassword,
            device_id: &'a DeviceId,
        ) -> impl Future<Output = Result<PasswordLoginStart, Self::Error>> + Send + 'a {
            self.password_start_count
                .set(self.password_start_count.get() + 1);
            *self.password_start_device.borrow_mut() = Some(device_id.as_str().to_owned());
            let result = match self.password_start {
                FakePasswordStart::Token => AccessToken::from_str("issued-token")
                    .map(PasswordLoginStart::Authenticated)
                    .map_err(|_| FakeApiError),
                FakePasswordStart::Otp => OtpSecret::parse_owned("synthetic-otp-secret".to_owned())
                    .map(PasswordLoginStart::OtpRequired)
                    .map_err(|_| FakeApiError),
                FakePasswordStart::Fail => Err(FakeApiError),
            };
            ready(result)
        }

        fn request_sms_otp<'a>(
            &'a self,
            _otp_secret: &'a OtpSecret,
            device_id: &'a DeviceId,
        ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
            self.otp_request_count.set(self.otp_request_count.get() + 1);
            *self.otp_request_device.borrow_mut() = Some(device_id.as_str().to_owned());
            ready(if self.otp_request_fails {
                Err(FakeApiError)
            } else {
                Ok(())
            })
        }

        fn complete_otp_login<'a>(
            &'a self,
            _otp_code: &'a OtpCode,
            _otp_secret: &'a OtpSecret,
            device_id: &'a DeviceId,
        ) -> impl Future<Output = Result<AccessToken, Self::Error>> + Send + 'a {
            self.otp_completion_count
                .set(self.otp_completion_count.get() + 1);
            *self.otp_completion_device.borrow_mut() = Some(device_id.as_str().to_owned());
            let result = if self.otp_completion_fails {
                Err(FakeApiError)
            } else {
                AccessToken::from_str("issued-token").map_err(|_| FakeApiError)
            };
            ready(result)
        }

        fn trust_device<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            device_id: &'a DeviceId,
        ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
            self.trust_count.set(self.trust_count.get() + 1);
            *self.trust_device.borrow_mut() = Some(device_id.as_str().to_owned());
            ready(if self.trust_fails {
                Err(FakeApiError)
            } else {
                Ok(())
            })
        }
    }

    struct FixedClock(OffsetDateTime);

    impl Clock for FixedClock {
        fn now_utc(&self) -> OffsetDateTime {
            self.0
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn token_import_requires_the_matching_device_and_verifies_storage() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Missing);
        let prompt = FakePrompt::interactive("new-token");
        let api = FakeApi::new(test_account("100")?);
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(5));

        let result = login_with_token(&store, &prompt, &api, &clock).await?;

        assert_eq!(result.disposition(), LoginDisposition::Created);
        assert_eq!(result.account().user_id().as_str(), "100");
        assert_eq!(result.saved_at(), clock.0);
        assert_eq!(store.save_count.get(), 1);
        assert_eq!(store.load_count.get(), 2);
        assert_eq!(api.current_account_count.get(), 1);
        assert_eq!(prompt.device_id_read_count.get(), 1);
        let saved = store.stored_fixture();
        assert!(saved.as_ref().is_some_and(|saved| {
            saved.token == "new-token"
                && saved.device_id == "matching-device"
                && saved.user_id == "100"
        }));
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn invalid_imported_device_stops_before_api_or_storage() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Missing);
        let mut prompt = FakePrompt::interactive("new-token");
        prompt.device_id = "invalid device";
        let api = FakeApi::new(test_account("100")?);
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

        let result = login_with_token(&store, &prompt, &api, &clock).await;

        assert!(matches!(
            result,
            Err(LoginError::Prompt(PromptError::InvalidDeviceId { .. }))
        ));
        assert_eq!(prompt.read_count.get(), 1);
        assert_eq!(prompt.device_id_read_count.get(), 1);
        assert_eq!(api.current_account_count.get(), 0);
        assert_eq!(store.save_count.get(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn noninteractive_token_login_touches_no_keyring_or_api() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Missing);
        let prompt = FakePrompt::noninteractive("new-token");
        let api = FakeApi::new(test_account("100")?);
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

        let result = login_with_token(&store, &prompt, &api, &clock).await;

        assert!(matches!(
            result,
            Err(LoginError::Prompt(PromptError::NotInteractive))
        ));
        assert_eq!(store.load_count.get(), 0);
        assert_eq!(store.save_count.get(), 0);
        assert_eq!(prompt.read_count.get(), 0);
        assert_eq!(api.current_account_count.get(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn noninteractive_password_login_touches_no_keyring_prompts_or_api() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Missing);
        let prompt = FakePrompt::noninteractive("unused-token");
        let api = FakeApi::new(test_account("100")?);
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

        let result = login_with_password(&store, &prompt, &api, &clock).await;

        assert!(matches!(
            result,
            Err(LoginError::Prompt(PromptError::NotInteractive))
        ));
        assert_eq!(store.load_count.get(), 0);
        assert_eq!(store.save_count.get(), 0);
        assert_eq!(prompt.device_id_read_count.get(), 0);
        assert_eq!(prompt.identifier_read_count.get(), 0);
        assert_eq!(prompt.password_read_count.get(), 0);
        assert_eq!(api.password_start_count.get(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn login_reuses_a_device_but_blocks_a_different_account() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Present(CredentialFixture::standard(
            "100",
            "old-token",
            "old-device",
        )));
        let prompt = FakePrompt::interactive("new-token");
        let api = FakeApi::new(test_account("200")?);
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

        let result = login_with_token(&store, &prompt, &api, &clock).await;

        assert!(matches!(result, Err(LoginError::DifferentAccount)));
        assert_eq!(store.save_count.get(), 0);
        assert_eq!(prompt.device_id_read_count.get(), 0);
        let stored = store.stored_fixture();
        assert!(stored.as_ref().is_some_and(|stored| {
            stored.token == "old-token" && stored.device_id == "old-device"
        }));
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn validated_login_replaces_every_targetable_unusable_entry() -> TestResult {
        for kind in [
            CredentialFailureKind::Corrupt,
            CredentialFailureKind::Invalid,
            CredentialFailureKind::UnsupportedVersion,
            CredentialFailureKind::TooLarge,
        ] {
            let store = FakeStore::new(FakeStoreState::Failure(kind));
            let prompt = FakePrompt::interactive("new-token");
            let api = FakeApi::new(test_account("100")?);
            let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

            let result = login_with_token(&store, &prompt, &api, &clock).await?;

            assert_eq!(
                result.disposition(),
                LoginDisposition::RecoveredUnusableEntry
            );
            assert_eq!(prompt.device_id_read_count.get(), 1);
            assert_eq!(store.save_count.get(), 1);
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn login_does_not_prompt_when_the_store_cannot_target_one_entry() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Failure(CredentialFailureKind::Ambiguous));
        let prompt = FakePrompt::interactive("new-token");
        let api = FakeApi::new(test_account("100")?);
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

        let result = login_with_token(&store, &prompt, &api, &clock).await;

        assert!(matches!(result, Err(LoginError::CredentialLoad { .. })));
        assert_eq!(prompt.read_count.get(), 0);
        assert_eq!(api.current_account_count.get(), 0);
        assert_eq!(store.save_count.get(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn failed_token_validation_never_saves() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Missing);
        let prompt = FakePrompt::interactive("new-token");
        let mut api = FakeApi::new(test_account("100")?);
        api.current_account_fails = true;
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

        let result = login_with_token(&store, &prompt, &api, &clock).await;

        assert!(matches!(result, Err(LoginError::TokenValidation { .. })));
        assert_eq!(store.save_count.get(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn failed_or_mismatched_save_reports_unknown_storage_state() -> TestResult {
        for behavior in [SaveBehavior::FailAfterWrite, SaveBehavior::StoreMismatch] {
            let store = FakeStore::new(FakeStoreState::Missing);
            store.save_behavior.set(behavior);
            let prompt = FakePrompt::interactive("new-token");
            let api = FakeApi::new(test_account("100")?);
            let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

            let result = login_with_token(&store, &prompt, &api, &clock).await;

            assert!(matches!(
                result,
                Err(LoginError::CredentialStorageStateUnknown { .. })
            ));
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn direct_password_login_preserves_the_already_trusted_device() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Missing);
        let prompt = FakePrompt::password_login();
        let api = FakeApi::new(test_account("100")?);
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

        let report = login_with_password(&store, &prompt, &api, &clock).await?;

        assert!(report.is_complete_success());
        assert_eq!(report.login().disposition(), LoginDisposition::Created);
        assert!(matches!(
            report.device_trust(),
            DeviceTrustOutcome::NotNeeded
        ));
        assert_eq!(prompt.device_id_read_count.get(), 1);
        assert_eq!(prompt.identifier_read_count.get(), 1);
        assert_eq!(prompt.password_read_count.get(), 1);
        assert_eq!(prompt.otp_read_count.get(), 0);
        assert_eq!(
            prompt.prompt_order.borrow().as_slice(),
            ["identifier", "password", "device"]
        );
        assert_eq!(api.password_start_count.get(), 1);
        assert_eq!(api.current_account_count.get(), 1);
        assert_eq!(api.trust_count.get(), 0);
        let saved = store.stored_fixture();
        assert!(saved.as_ref().is_some_and(|saved| {
            saved.token == "issued-token" && saved.device_id == "matching-device"
        }));
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn direct_reauthentication_reuses_the_exact_stored_device_without_trust() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Present(CredentialFixture::standard(
            "100",
            "expired-old-token",
            "stored-trusted-device",
        )));
        let prompt = FakePrompt::password_login();
        let api = FakeApi::new(test_account("100")?);
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(9));

        let report = reauthenticate(&store, &prompt, &api, &clock).await?;

        assert!(report.is_complete_success());
        assert_eq!(
            report.login().disposition(),
            LoginDisposition::ReplacedForSameAccount
        );
        assert!(matches!(
            report.device_trust(),
            DeviceTrustOutcome::NotNeeded
        ));
        assert_eq!(prompt.device_id_read_count.get(), 0);
        assert_eq!(prompt.identifier_read_count.get(), 1);
        assert_eq!(prompt.password_read_count.get(), 1);
        assert_eq!(prompt.otp_read_count.get(), 0);
        assert_eq!(
            prompt.prompt_order.borrow().as_slice(),
            ["identifier", "password"]
        );
        assert_eq!(api.password_start_count.get(), 1);
        assert_eq!(api.current_account_count.get(), 1);
        assert_eq!(api.trust_count.get(), 0);
        assert_eq!(
            api.password_start_device.borrow().as_deref(),
            Some("stored-trusted-device")
        );
        assert_eq!(
            api.current_account_device.borrow().as_deref(),
            Some("stored-trusted-device")
        );
        assert_eq!(
            api.current_account_token.borrow().as_deref(),
            Some("issued-token")
        );
        let saved = store.stored_fixture();
        assert!(saved.as_ref().is_some_and(|saved| {
            saved.token == "issued-token"
                && saved.device_id == "stored-trusted-device"
                && saved.user_id == "100"
                && saved.saved_at == clock.0
        }));
        assert_eq!(store.save_count.get(), 1);
        assert_eq!(store.load_count.get(), 2);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reauthentication_completes_otp_with_the_stored_device_then_trusts_it() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Present(CredentialFixture::standard(
            "100",
            "expired-old-token",
            "stored-trusted-device",
        )));
        let prompt = FakePrompt::password_login();
        let mut api = FakeApi::new(test_account("100")?);
        api.password_start = FakePasswordStart::Otp;
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

        let report = reauthenticate(&store, &prompt, &api, &clock).await?;

        assert!(report.is_complete_success());
        assert!(matches!(report.device_trust(), DeviceTrustOutcome::Trusted));
        assert_eq!(prompt.device_id_read_count.get(), 0);
        assert_eq!(api.password_start_count.get(), 1);
        assert_eq!(api.otp_request_count.get(), 1);
        assert_eq!(prompt.otp_read_count.get(), 1);
        assert_eq!(api.otp_completion_count.get(), 1);
        assert_eq!(api.current_account_count.get(), 1);
        assert_eq!(store.save_count.get(), 1);
        assert_eq!(api.trust_count.get(), 1);
        for observed in [
            &api.password_start_device,
            &api.otp_request_device,
            &api.otp_completion_device,
            &api.current_account_device,
            &api.trust_device,
        ] {
            assert_eq!(observed.borrow().as_deref(), Some("stored-trusted-device"));
        }
        let saved = store.stored_fixture();
        assert!(saved.as_ref().is_some_and(|saved| {
            saved.token == "issued-token" && saved.device_id == "stored-trusted-device"
        }));
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reauthentication_otp_trust_failure_keeps_the_verified_replacement() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Present(CredentialFixture::standard(
            "100",
            "old-token",
            "stored-device",
        )));
        let prompt = FakePrompt::password_login();
        let mut api = FakeApi::new(test_account("100")?);
        api.password_start = FakePasswordStart::Otp;
        api.trust_fails = true;
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

        let report = reauthenticate(&store, &prompt, &api, &clock).await?;

        assert!(!report.is_complete_success());
        assert!(matches!(
            report.device_trust(),
            DeviceTrustOutcome::Failed(_)
        ));
        assert_eq!(store.save_count.get(), 1);
        assert_eq!(store.load_count.get(), 2);
        assert_eq!(api.trust_count.get(), 1);
        let saved = store.stored_fixture();
        assert!(saved.as_ref().is_some_and(|saved| {
            saved.token == "issued-token" && saved.device_id == "stored-device"
        }));
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reauthentication_requires_one_readable_stored_credential() -> TestResult {
        let missing_store = FakeStore::new(FakeStoreState::Missing);
        let missing_prompt = FakePrompt::password_login();
        let missing_api = FakeApi::new(test_account("100")?);
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

        let missing = reauthenticate(&missing_store, &missing_prompt, &missing_api, &clock).await;
        assert!(matches!(
            missing,
            Err(LoginError::ReauthenticationCredentialMissing)
        ));
        assert_eq!(missing_prompt.identifier_read_count.get(), 0);
        assert_eq!(missing_api.password_start_count.get(), 0);
        assert_eq!(missing_store.save_count.get(), 0);

        for kind in [
            CredentialFailureKind::Unavailable,
            CredentialFailureKind::Corrupt,
            CredentialFailureKind::Invalid,
            CredentialFailureKind::UnsupportedVersion,
            CredentialFailureKind::TooLarge,
            CredentialFailureKind::Ambiguous,
            CredentialFailureKind::Platform,
            CredentialFailureKind::Internal,
        ] {
            let store = FakeStore::new(FakeStoreState::Failure(kind));
            let prompt = FakePrompt::password_login();
            let api = FakeApi::new(test_account("100")?);

            let result = reauthenticate(&store, &prompt, &api, &clock).await;

            assert!(matches!(result, Err(LoginError::CredentialLoad { .. })));
            assert_eq!(prompt.identifier_read_count.get(), 0);
            assert_eq!(prompt.password_read_count.get(), 0);
            assert_eq!(api.password_start_count.get(), 0);
            assert_eq!(store.save_count.get(), 0);
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn invalid_reauthentication_prompt_input_preserves_the_old_credential() -> TestResult {
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

        let identifier_store = FakeStore::new(FakeStoreState::Present(
            CredentialFixture::standard("100", "old-token", "stored-device"),
        ));
        let mut identifier_prompt = FakePrompt::password_login();
        identifier_prompt.identifier = "invalid identifier";
        let identifier_api = FakeApi::new(test_account("100")?);
        let identifier_result = reauthenticate(
            &identifier_store,
            &identifier_prompt,
            &identifier_api,
            &clock,
        )
        .await;
        assert!(matches!(
            identifier_result,
            Err(LoginError::Prompt(
                PromptError::InvalidLoginIdentifier { .. }
            ))
        ));
        assert_eq!(identifier_prompt.password_read_count.get(), 0);
        assert_eq!(identifier_api.password_start_count.get(), 0);
        assert_old_credential(&identifier_store, "old-token", "stored-device");

        let password_store = FakeStore::new(FakeStoreState::Present(CredentialFixture::standard(
            "100",
            "old-token",
            "stored-device",
        )));
        let mut password_prompt = FakePrompt::password_login();
        password_prompt.password = "";
        let password_api = FakeApi::new(test_account("100")?);
        let password_result =
            reauthenticate(&password_store, &password_prompt, &password_api, &clock).await;
        assert!(matches!(
            password_result,
            Err(LoginError::Prompt(
                PromptError::InvalidAccountPassword { .. }
            ))
        ));
        assert_eq!(password_api.password_start_count.get(), 0);
        assert_old_credential(&password_store, "old-token", "stored-device");

        let otp_store = FakeStore::new(FakeStoreState::Present(CredentialFixture::standard(
            "100",
            "old-token",
            "stored-device",
        )));
        let mut otp_prompt = FakePrompt::password_login();
        otp_prompt.otp_code = "not-six-digits";
        let mut otp_api = FakeApi::new(test_account("100")?);
        otp_api.password_start = FakePasswordStart::Otp;
        let otp_result = reauthenticate(&otp_store, &otp_prompt, &otp_api, &clock).await;
        assert!(matches!(
            otp_result,
            Err(LoginError::Prompt(PromptError::InvalidOtpCode { .. }))
        ));
        assert_eq!(otp_api.password_start_count.get(), 1);
        assert_eq!(otp_api.otp_request_count.get(), 1);
        assert_eq!(otp_api.otp_completion_count.get(), 0);
        assert_eq!(otp_store.save_count.get(), 0);
        assert_old_credential(&otp_store, "old-token", "stored-device");
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reauthentication_account_mismatch_never_replaces_the_old_credential() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Present(CredentialFixture::standard(
            "100",
            "old-token",
            "stored-device",
        )));
        let prompt = FakePrompt::password_login();
        let api = FakeApi::new(test_account("200")?);
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

        let result = reauthenticate(&store, &prompt, &api, &clock).await;

        assert!(matches!(
            &result,
            Err(LoginError::IssuedTokenDifferentAccount)
        ));
        if let Err(error) = result {
            let rendered = error.to_string();
            assert!(rendered.contains("stored credential was left unchanged"));
            assert!(rendered.contains("remote token may remain active"));
            assert!(!rendered.contains("old-token"));
            assert!(!rendered.contains("issued-token"));
            assert!(!rendered.contains("stored-device"));
        }
        assert_eq!(api.password_start_count.get(), 1);
        assert_eq!(api.current_account_count.get(), 1);
        assert_eq!(api.trust_count.get(), 0);
        assert_eq!(store.save_count.get(), 0);
        assert_old_credential(&store, "old-token", "stored-device");
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reauthentication_validation_failure_never_replaces_the_old_credential() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Present(CredentialFixture::standard(
            "100",
            "old-token",
            "stored-device",
        )));
        let prompt = FakePrompt::password_login();
        let mut api = FakeApi::new(test_account("100")?);
        api.current_account_fails = true;
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

        let result = reauthenticate(&store, &prompt, &api, &clock).await;

        assert!(matches!(
            result,
            Err(LoginError::IssuedTokenValidation { .. })
        ));
        assert_eq!(api.password_start_count.get(), 1);
        assert_eq!(api.current_account_count.get(), 1);
        assert_eq!(api.trust_count.get(), 0);
        assert_eq!(store.save_count.get(), 0);
        assert_old_credential(&store, "old-token", "stored-device");
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reauthentication_password_failure_is_never_retried_or_stored() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Present(CredentialFixture::standard(
            "100",
            "old-token",
            "stored-device",
        )));
        let prompt = FakePrompt::password_login();
        let mut api = FakeApi::new(test_account("100")?);
        api.password_start = FakePasswordStart::Fail;
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

        let result = reauthenticate(&store, &prompt, &api, &clock).await;

        assert!(matches!(
            result,
            Err(LoginError::PasswordAuthentication { .. })
        ));
        assert_eq!(api.password_start_count.get(), 1);
        assert_eq!(api.current_account_count.get(), 0);
        assert_eq!(api.trust_count.get(), 0);
        assert_old_credential(&store, "old-token", "stored-device");
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reauthentication_save_and_readback_failures_report_unknown_state() -> TestResult {
        for behavior in [
            SaveBehavior::FailAfterWrite,
            SaveBehavior::ReadBackFailure,
            SaveBehavior::ReadBackMissing,
            SaveBehavior::StoreMismatch,
        ] {
            let store = FakeStore::new(FakeStoreState::Present(CredentialFixture::standard(
                "100",
                "old-token",
                "stored-device",
            )));
            store.save_behavior.set(behavior);
            let prompt = FakePrompt::password_login();
            let mut api = FakeApi::new(test_account("100")?);
            api.password_start = FakePasswordStart::Otp;
            let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

            let result = reauthenticate(&store, &prompt, &api, &clock).await;

            assert!(matches!(
                result,
                Err(LoginError::IssuedCredentialStorageStateUnknown { .. })
            ));
            assert_eq!(api.password_start_count.get(), 1);
            assert_eq!(api.otp_completion_count.get(), 1);
            assert_eq!(api.current_account_count.get(), 1);
            assert_eq!(store.save_count.get(), 1);
            assert_eq!(api.trust_count.get(), 0);
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn noninteractive_reauthentication_touches_no_store_prompts_or_api() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Present(CredentialFixture::standard(
            "100",
            "old-token",
            "stored-device",
        )));
        let prompt = FakePrompt::noninteractive("unused-token");
        let api = FakeApi::new(test_account("100")?);
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

        let result = reauthenticate(&store, &prompt, &api, &clock).await;

        assert!(matches!(
            result,
            Err(LoginError::Prompt(PromptError::NotInteractive))
        ));
        assert_eq!(store.load_count.get(), 0);
        assert_eq!(store.save_count.get(), 0);
        assert_eq!(prompt.identifier_read_count.get(), 0);
        assert_eq!(prompt.password_read_count.get(), 0);
        assert_eq!(api.password_start_count.get(), 0);
        assert_eq!(api.current_account_count.get(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn invalid_trusted_device_stops_after_credentials_but_before_api_or_storage() -> TestResult
    {
        let store = FakeStore::new(FakeStoreState::Missing);
        let mut prompt = FakePrompt::password_login();
        prompt.device_id = "invalid device";
        let api = FakeApi::new(test_account("100")?);
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

        let result = login_with_password(&store, &prompt, &api, &clock).await;

        assert!(matches!(
            result,
            Err(LoginError::Prompt(PromptError::InvalidDeviceId { .. }))
        ));
        assert_eq!(prompt.device_id_read_count.get(), 1);
        assert_eq!(prompt.identifier_read_count.get(), 1);
        assert_eq!(prompt.password_read_count.get(), 1);
        assert_eq!(
            prompt.prompt_order.borrow().as_slice(),
            ["identifier", "password", "device"]
        );
        assert_eq!(api.password_start_count.get(), 0);
        assert_eq!(store.save_count.get(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn password_login_completes_the_sms_otp_flow() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Missing);
        let prompt = FakePrompt::password_login();
        let mut api = FakeApi::new(test_account("100")?);
        api.password_start = FakePasswordStart::Otp;
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

        let report = login_with_password(&store, &prompt, &api, &clock).await?;

        assert!(report.is_complete_success());
        assert_eq!(api.otp_request_count.get(), 1);
        assert_eq!(prompt.otp_read_count.get(), 1);
        assert_eq!(api.otp_completion_count.get(), 1);
        assert_eq!(api.trust_count.get(), 1);
        assert_eq!(
            prompt.prompt_order.borrow().as_slice(),
            ["identifier", "password", "device", "otp"]
        );
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn password_login_blocks_an_existing_valid_credential_before_prompts() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Present(CredentialFixture::standard(
            "100",
            "old-token",
            "old-device",
        )));
        let prompt = FakePrompt::password_login();
        let api = FakeApi::new(test_account("100")?);
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

        let result = login_with_password(&store, &prompt, &api, &clock).await;

        assert!(matches!(result, Err(LoginError::CredentialAlreadyStored)));
        assert_eq!(prompt.device_id_read_count.get(), 0);
        assert_eq!(prompt.identifier_read_count.get(), 0);
        assert_eq!(prompt.password_read_count.get(), 0);
        assert_eq!(api.password_start_count.get(), 0);
        assert_eq!(store.save_count.get(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn trust_failure_keeps_the_verified_credential_and_reports_incomplete() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Missing);
        let prompt = FakePrompt::password_login();
        let mut api = FakeApi::new(test_account("100")?);
        api.password_start = FakePasswordStart::Otp;
        api.trust_fails = true;
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

        let report = login_with_password(&store, &prompt, &api, &clock).await?;

        assert!(!report.is_complete_success());
        assert!(matches!(
            report.device_trust(),
            DeviceTrustOutcome::Failed(_)
        ));
        assert_eq!(store.save_count.get(), 1);
        assert!(store.stored_fixture().is_some());
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn issued_token_validation_failure_is_not_stored_or_trusted() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Missing);
        let prompt = FakePrompt::password_login();
        let mut api = FakeApi::new(test_account("100")?);
        api.current_account_fails = true;
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

        let result = login_with_password(&store, &prompt, &api, &clock).await;

        assert!(matches!(
            result,
            Err(LoginError::IssuedTokenValidation { .. })
        ));
        assert_eq!(store.save_count.get(), 0);
        assert_eq!(api.trust_count.get(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn issued_token_storage_failure_never_attempts_device_trust() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Missing);
        store.save_behavior.set(SaveBehavior::FailAfterWrite);
        let prompt = FakePrompt::password_login();
        let api = FakeApi::new(test_account("100")?);
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

        let result = login_with_password(&store, &prompt, &api, &clock).await;

        assert!(matches!(
            result,
            Err(LoginError::IssuedCredentialStorageStateUnknown { .. })
        ));
        assert_eq!(api.trust_count.get(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn password_and_otp_failures_never_store_a_credential() -> TestResult {
        let password_store = FakeStore::new(FakeStoreState::Missing);
        let password_prompt = FakePrompt::password_login();
        let mut password_api = FakeApi::new(test_account("100")?);
        password_api.password_start = FakePasswordStart::Fail;
        let clock = FixedClock(OffsetDateTime::UNIX_EPOCH);

        let password_result =
            login_with_password(&password_store, &password_prompt, &password_api, &clock).await;
        assert!(matches!(
            password_result,
            Err(LoginError::PasswordAuthentication { .. })
        ));
        assert_eq!(password_store.save_count.get(), 0);

        let otp_store = FakeStore::new(FakeStoreState::Missing);
        let otp_prompt = FakePrompt::password_login();
        let mut otp_api = FakeApi::new(test_account("100")?);
        otp_api.password_start = FakePasswordStart::Otp;
        otp_api.otp_request_fails = true;
        let otp_result = login_with_password(&otp_store, &otp_prompt, &otp_api, &clock).await;
        assert!(matches!(otp_result, Err(LoginError::OtpRequest { .. })));
        assert_eq!(otp_store.save_count.get(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn status_is_read_only_and_rejects_an_account_mismatch() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Present(CredentialFixture::standard(
            "100", "token", "device",
        )));
        let matching_api = FakeApi::new(test_account("100")?);

        let result = status(&store, &matching_api).await?;
        assert_eq!(result.account().user_id().as_str(), "100");
        assert_eq!(result.credential_format(), CredentialFormat::Version1);
        assert_eq!(store.save_count.get(), 0);

        let different_api = FakeApi::new(test_account("200")?);
        let mismatch = status(&store, &different_api).await;
        assert!(matches!(mismatch, Err(AuthStatusError::AccountMismatch)));
        assert_eq!(store.save_count.get(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn logout_reports_remote_failure_after_local_deletion() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Present(CredentialFixture::standard(
            "100", "token", "device",
        )));
        let mut api = FakeApi::new(test_account("100")?);
        api.revoke_fails = true;

        let report = logout(&store, &api, true).await;

        assert!(matches!(
            report.remote(),
            RemoteRevocationOutcome::Failed(_)
        ));
        assert!(matches!(report.local(), LocalDeletionOutcome::Deleted));
        assert!(!report.is_complete_success());
        assert!(matches!(&*store.state.borrow(), FakeStoreState::Missing));
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn logout_reports_local_failure_after_remote_revocation() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Present(CredentialFixture::standard(
            "100", "token", "device",
        )));
        store.delete_fails.set(true);
        let api = FakeApi::new(test_account("100")?);

        let report = logout(&store, &api, true).await;

        assert!(matches!(report.remote(), RemoteRevocationOutcome::Revoked));
        assert!(matches!(report.local(), LocalDeletionOutcome::Failed(_)));
        assert!(!report.is_complete_success());
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn local_only_logout_never_loads_or_calls_the_api() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Failure(CredentialFailureKind::Corrupt));
        let api = FakeApi::new(test_account("100")?);

        let report = logout(&store, &api, false).await;

        assert!(matches!(
            report.remote(),
            RemoteRevocationOutcome::NotRequested
        ));
        assert!(matches!(report.local(), LocalDeletionOutcome::Deleted));
        assert!(report.is_complete_success());
        assert_eq!(store.load_count.get(), 0);
        assert_eq!(api.revoke_count.get(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn revoke_request_with_unreadable_credential_still_deletes_locally() -> TestResult {
        let store = FakeStore::new(FakeStoreState::Failure(CredentialFailureKind::Corrupt));
        let api = FakeApi::new(test_account("100")?);

        let report = logout(&store, &api, true).await;

        assert!(matches!(
            report.remote(),
            RemoteRevocationOutcome::NotAttempted(_)
        ));
        assert!(matches!(report.local(), LocalDeletionOutcome::Deleted));
        assert!(!report.is_complete_success());
        assert_eq!(api.revoke_count.get(), 0);
        Ok(())
    }

    fn test_account(user_id: &str) -> Result<Account, Box<dyn Error>> {
        Ok(Account::new(
            UserId::from_str(user_id)?,
            Username::from_bare("alice")?,
            Some("Alice".to_owned()),
        ))
    }

    fn assert_old_credential(store: &FakeStore, token: &str, device_id: &str) {
        let stored = store.stored_fixture();
        assert!(
            stored
                .as_ref()
                .is_some_and(|stored| { stored.token == token && stored.device_id == device_id })
        );
        assert_eq!(store.save_count.get(), 0);
    }
}

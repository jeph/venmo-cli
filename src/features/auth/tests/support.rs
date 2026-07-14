use super::*;

pub(super) type Transcript = Rc<RefCell<Vec<AuthCall>>>;

#[derive(Clone)]
pub(super) enum FakeStoreState {
    Missing,
    Present(CredentialFixture),
    Failure(CredentialFailureKind),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum CredentialStateSnapshot {
    Missing,
    Present(CredentialSnapshot),
    Failure(CredentialFailureKind),
}

impl FakeStoreState {
    pub(super) fn snapshot(&self) -> CredentialStateSnapshot {
        match self {
            Self::Missing => CredentialStateSnapshot::Missing,
            Self::Present(credential) => CredentialStateSnapshot::Present(credential.snapshot()),
            Self::Failure(kind) => CredentialStateSnapshot::Failure(*kind),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum SaveScript {
    Normal,
    FailAfterWrite,
    ReadBackFailure,
    ReadBackMissing,
    StoreMismatch,
}

#[derive(Clone, Copy)]
pub(super) enum DeleteScript {
    Normal,
    ReportDeleted,
    ReportMissing,
    Fail,
}

#[derive(Clone, Copy)]
pub(super) struct StoreScript {
    pub(super) save: SaveScript,
    pub(super) delete: DeleteScript,
}

impl StoreScript {
    pub(super) const NORMAL: Self = Self {
        save: SaveScript::Normal,
        delete: DeleteScript::Normal,
    };

    pub(super) const fn with_save(save: SaveScript) -> Self {
        Self {
            save,
            ..Self::NORMAL
        }
    }

    pub(super) const fn with_delete(delete: DeleteScript) -> Self {
        Self {
            delete,
            ..Self::NORMAL
        }
    }
}

pub(super) struct FakeStore {
    state: RefCell<FakeStoreState>,
    script: StoreScript,
    calls: Transcript,
}

impl FakeStore {
    pub(super) fn new(state: FakeStoreState, script: StoreScript, calls: Transcript) -> Self {
        Self {
            state: RefCell::new(state),
            script,
            calls,
        }
    }

    pub(super) fn snapshot(&self) -> CredentialStateSnapshot {
        self.state.borrow().snapshot()
    }
}

impl CredentialCapability for FakeStore {
    type Error = FakeCredentialError;
}

impl CredentialReader for FakeStore {
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        self.calls.borrow_mut().push(AuthCall::LoadCredential);
        match &*self.state.borrow() {
            FakeStoreState::Missing => Ok(None),
            FakeStoreState::Present(fixture) => fixture.load().map(Some),
            FakeStoreState::Failure(kind) => Err(FakeCredentialError { kind: *kind }),
        }
    }
}

impl CredentialWriter for FakeStore {
    fn save_credential(&self, credential: &CredentialEnvelope) -> Result<(), Self::Error> {
        self.calls
            .borrow_mut()
            .push(AuthCall::SaveCredential(CredentialSnapshot::from_envelope(
                credential,
                CredentialFormat::Version1,
            )));
        let mut fixture = CredentialFixture::from_credential(credential);
        match self.script.save {
            SaveScript::Normal => {
                *self.state.borrow_mut() = FakeStoreState::Present(fixture);
                Ok(())
            }
            SaveScript::FailAfterWrite => {
                *self.state.borrow_mut() = FakeStoreState::Present(fixture);
                Err(FakeCredentialError::platform())
            }
            SaveScript::ReadBackFailure => {
                *self.state.borrow_mut() = FakeStoreState::Failure(CredentialFailureKind::Platform);
                Ok(())
            }
            SaveScript::ReadBackMissing => {
                *self.state.borrow_mut() = FakeStoreState::Missing;
                Ok(())
            }
            SaveScript::StoreMismatch => {
                fixture.token = MISMATCHED_TOKEN.to_owned();
                *self.state.borrow_mut() = FakeStoreState::Present(fixture);
                Ok(())
            }
        }
    }
}

impl CredentialDeleter for FakeStore {
    fn delete_credential(&self) -> Result<CredentialDeleteOutcome, Self::Error> {
        delete_fake_credential(&self.state, self.script.delete, &self.calls)
    }
}

pub(super) struct DeleteOnlyFake {
    state: RefCell<FakeStoreState>,
    script: DeleteScript,
    calls: Transcript,
}

impl DeleteOnlyFake {
    pub(super) fn new(state: FakeStoreState, script: StoreScript, calls: Transcript) -> Self {
        Self {
            state: RefCell::new(state),
            script: script.delete,
            calls,
        }
    }

    pub(super) fn snapshot(&self) -> CredentialStateSnapshot {
        self.state.borrow().snapshot()
    }
}

impl CredentialCapability for DeleteOnlyFake {
    type Error = FakeCredentialError;
}

impl CredentialDeleter for DeleteOnlyFake {
    fn delete_credential(&self) -> Result<CredentialDeleteOutcome, Self::Error> {
        delete_fake_credential(&self.state, self.script, &self.calls)
    }
}

fn delete_fake_credential(
    state: &RefCell<FakeStoreState>,
    script: DeleteScript,
    calls: &Transcript,
) -> Result<CredentialDeleteOutcome, FakeCredentialError> {
    calls.borrow_mut().push(AuthCall::DeleteCredential);
    let outcome = match script {
        DeleteScript::Fail => return Err(FakeCredentialError::platform()),
        DeleteScript::ReportDeleted => CredentialDeleteOutcome::Deleted,
        DeleteScript::ReportMissing => CredentialDeleteOutcome::Missing,
        DeleteScript::Normal => {
            if matches!(&*state.borrow(), FakeStoreState::Missing) {
                CredentialDeleteOutcome::Missing
            } else {
                CredentialDeleteOutcome::Deleted
            }
        }
    };
    *state.borrow_mut() = FakeStoreState::Missing;
    Ok(outcome)
}

pub(super) struct FakeReader {
    state: FakeStoreState,
    calls: Transcript,
}

impl FakeReader {
    pub(super) fn new(state: FakeStoreState, calls: Transcript) -> Self {
        Self { state, calls }
    }

    pub(super) fn snapshot(&self) -> CredentialStateSnapshot {
        self.state.snapshot()
    }
}

impl CredentialCapability for FakeReader {
    type Error = FakeCredentialError;
}

impl CredentialReader for FakeReader {
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        self.calls.borrow_mut().push(AuthCall::ReadCredential);
        match &self.state {
            FakeStoreState::Missing => Ok(None),
            FakeStoreState::Present(fixture) => fixture.load().map(Some),
            FakeStoreState::Failure(kind) => Err(FakeCredentialError { kind: *kind }),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum PromptAvailabilityScript {
    Interactive,
    NonInteractive,
}

#[derive(Clone, Copy)]
pub(super) enum DeviceInputScript {
    Valid(DeviceMaterial),
    Invalid,
}

#[derive(Clone, Copy)]
pub(super) enum IdentifierInputScript {
    Valid,
    Invalid,
}

#[derive(Clone, Copy)]
pub(super) enum PasswordInputScript {
    Valid,
    Invalid,
}

#[derive(Clone, Copy)]
pub(super) enum OtpInputScript {
    Valid,
    Invalid,
}

#[derive(Clone, Copy)]
pub(super) struct PromptScript {
    pub(super) availability: PromptAvailabilityScript,
    pub(super) token: TokenMaterial,
    pub(super) device: DeviceInputScript,
    pub(super) identifier: IdentifierInputScript,
    pub(super) password: PasswordInputScript,
    pub(super) otp: OtpInputScript,
}

impl PromptScript {
    pub(super) const fn token_login() -> Self {
        Self {
            availability: PromptAvailabilityScript::Interactive,
            token: TokenMaterial::Imported,
            device: DeviceInputScript::Valid(DeviceMaterial::Prompted),
            identifier: IdentifierInputScript::Valid,
            password: PasswordInputScript::Valid,
            otp: OtpInputScript::Valid,
        }
    }

    pub(super) const fn password_login() -> Self {
        Self::token_login()
    }

    pub(super) const fn noninteractive() -> Self {
        Self {
            availability: PromptAvailabilityScript::NonInteractive,
            ..Self::token_login()
        }
    }
}

pub(super) struct FakePrompt {
    script: PromptScript,
    calls: Transcript,
}

impl FakePrompt {
    pub(super) fn new(script: PromptScript, calls: Transcript) -> Self {
        Self { script, calls }
    }
}

impl PromptAvailability for FakePrompt {
    fn can_prompt(&self) -> bool {
        self.calls
            .borrow_mut()
            .push(AuthCall::CheckPromptAvailability);
        matches!(
            self.script.availability,
            PromptAvailabilityScript::Interactive
        )
    }
}

impl AuthenticationInput for FakePrompt {
    fn read_login_identifier(&self, _prompt: &str) -> Result<LoginIdentifier, PromptError> {
        self.calls.borrow_mut().push(AuthCall::ReadLoginIdentifier);
        let value = match self.script.identifier {
            IdentifierInputScript::Valid => LOGIN_IDENTIFIER,
            IdentifierInputScript::Invalid => "invalid identifier",
        };
        LoginIdentifier::parse_owned(value.to_owned())
            .map_err(|source| PromptError::InvalidLoginIdentifier { source })
    }

    fn read_account_password(&self, _prompt: &str) -> Result<AccountPassword, PromptError> {
        self.calls.borrow_mut().push(AuthCall::ReadAccountPassword);
        let value = match self.script.password {
            PasswordInputScript::Valid => ACCOUNT_PASSWORD,
            PasswordInputScript::Invalid => "",
        };
        AccountPassword::parse_owned(value.to_owned())
            .map_err(|source| PromptError::InvalidAccountPassword { source })
    }

    fn read_otp_code(&self, _prompt: &str) -> Result<OtpCode, PromptError> {
        self.calls.borrow_mut().push(AuthCall::ReadOtpCode);
        let value = match self.script.otp {
            OtpInputScript::Valid => OTP_CODE,
            OtpInputScript::Invalid => "not-six-digits",
        };
        OtpCode::parse_owned(value.to_owned())
            .map_err(|source| PromptError::InvalidOtpCode { source })
    }

    fn read_access_token(&self, _prompt: &str) -> Result<AccessToken, PromptError> {
        self.calls.borrow_mut().push(AuthCall::ReadAccessToken);
        AccessToken::from_str(self.script.token.value())
            .map_err(|source| PromptError::InvalidAccessToken { source })
    }

    fn read_device_id(&self, _prompt: &str) -> Result<DeviceId, PromptError> {
        self.calls.borrow_mut().push(AuthCall::ReadDeviceId);
        let value = match self.script.device {
            DeviceInputScript::Valid(device) => device.value(),
            DeviceInputScript::Invalid => "invalid device",
        };
        DeviceId::from_str(value).map_err(|source| PromptError::InvalidDeviceId { source })
    }
}

#[derive(Clone, Copy, Debug, Error)]
#[error("synthetic API failure")]
pub(super) struct FakeApiError(pub(super) ApiFailureKind);

impl ApiFailure for FakeApiError {
    fn kind(&self) -> ApiFailureKind {
        self.0
    }
}

#[derive(Clone)]
pub(super) enum PasswordStartScript {
    Authenticated(TokenMaterial),
    OtpRequired,
    Failure(ApiFailureKind),
}

#[derive(Clone)]
pub(super) enum OtpCompletionScript {
    Authenticated(TokenMaterial),
    Failure(ApiFailureKind),
}

#[derive(Clone)]
pub(super) struct ApiScript {
    pub(super) current_account: Result<Account, ApiFailureKind>,
    pub(super) revoke: Result<(), ApiFailureKind>,
    pub(super) password_start: PasswordStartScript,
    pub(super) otp_request: Result<(), ApiFailureKind>,
    pub(super) otp_completion: OtpCompletionScript,
    pub(super) trust: Result<(), ApiFailureKind>,
}

impl ApiScript {
    pub(super) fn successful(account: Account) -> Self {
        Self {
            current_account: Ok(account),
            revoke: Ok(()),
            password_start: PasswordStartScript::Authenticated(TokenMaterial::Issued),
            otp_request: Ok(()),
            otp_completion: OtpCompletionScript::Authenticated(TokenMaterial::Issued),
            trust: Ok(()),
        }
    }
}

pub(super) struct FakeApi {
    script: ApiScript,
    calls: Transcript,
}

impl FakeApi {
    pub(super) fn new(script: ApiScript, calls: Transcript) -> Self {
        Self { script, calls }
    }
}

impl CurrentAccountApi for FakeApi {
    type Error = FakeApiError;

    fn current_account<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a {
        self.calls.borrow_mut().push(AuthCall::CurrentAccount(
            AuthenticationSnapshot::from_values(access_token, device_id),
        ));
        ready(self.script.current_account.clone().map_err(FakeApiError))
    }
}

impl TokenRevocationApi for FakeApi {
    type Error = FakeApiError;

    fn revoke_access_token<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
        self.calls.borrow_mut().push(AuthCall::RevokeAccessToken(
            AuthenticationSnapshot::from_values(access_token, device_id),
        ));
        ready(self.script.revoke.map_err(FakeApiError))
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
        self.calls.borrow_mut().push(AuthCall::BeginPasswordLogin {
            identifier: RedactedValue,
            password: RedactedValue,
            device_id: device_snapshot(device_id.as_str()),
        });
        let result = match self.script.password_start {
            PasswordStartScript::Authenticated(token) => AccessToken::from_str(token.value())
                .map(PasswordLoginStart::Authenticated)
                .map_err(|_| FakeApiError(ApiFailureKind::Internal)),
            PasswordStartScript::OtpRequired => OtpSecret::parse_owned(OTP_SECRET.to_owned())
                .map(PasswordLoginStart::OtpRequired)
                .map_err(|_| FakeApiError(ApiFailureKind::Internal)),
            PasswordStartScript::Failure(kind) => Err(FakeApiError(kind)),
        };
        ready(result)
    }

    fn request_sms_otp<'a>(
        &'a self,
        _otp_secret: &'a OtpSecret,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
        self.calls.borrow_mut().push(AuthCall::RequestSmsOtp {
            otp_secret: RedactedValue,
            device_id: device_snapshot(device_id.as_str()),
        });
        ready(self.script.otp_request.map_err(FakeApiError))
    }

    fn complete_otp_login<'a>(
        &'a self,
        _otp_code: &'a OtpCode,
        _otp_secret: &'a OtpSecret,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<AccessToken, Self::Error>> + Send + 'a {
        self.calls.borrow_mut().push(AuthCall::CompleteOtpLogin {
            otp_code: RedactedValue,
            otp_secret: RedactedValue,
            device_id: device_snapshot(device_id.as_str()),
        });
        let result = match self.script.otp_completion {
            OtpCompletionScript::Authenticated(token) => AccessToken::from_str(token.value())
                .map_err(|_| FakeApiError(ApiFailureKind::Internal)),
            OtpCompletionScript::Failure(kind) => Err(FakeApiError(kind)),
        };
        ready(result)
    }

    fn trust_device<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
        self.calls
            .borrow_mut()
            .push(AuthCall::TrustDevice(AuthenticationSnapshot::from_values(
                access_token,
                device_id,
            )));
        ready(self.script.trust.map_err(FakeApiError))
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) struct RedactedValue;

impl fmt::Debug for RedactedValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct AuthenticationSnapshot {
    pub(super) access_token: SecretSnapshot,
    pub(super) device_id: SecretSnapshot,
}

impl AuthenticationSnapshot {
    pub(super) const fn synthetic(token: TokenMaterial, device: DeviceMaterial) -> Self {
        Self {
            access_token: token.snapshot(),
            device_id: device.snapshot(),
        }
    }

    fn from_values(access_token: &AccessToken, device_id: &DeviceId) -> Self {
        Self {
            access_token: token_snapshot(access_token.expose_secret()),
            device_id: device_snapshot(device_id.as_str()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum AuthCall {
    CheckPromptAvailability,
    LoadCredential,
    ReadCredential,
    SaveCredential(CredentialSnapshot),
    DeleteCredential,
    ReadAccessToken,
    ReadDeviceId,
    ReadLoginIdentifier,
    ReadAccountPassword,
    ReadOtpCode,
    CurrentAccount(AuthenticationSnapshot),
    RevokeAccessToken(AuthenticationSnapshot),
    BeginPasswordLogin {
        identifier: RedactedValue,
        password: RedactedValue,
        device_id: SecretSnapshot,
    },
    RequestSmsOtp {
        otp_secret: RedactedValue,
        device_id: SecretSnapshot,
    },
    CompleteOtpLogin {
        otp_code: RedactedValue,
        otp_secret: RedactedValue,
        device_id: SecretSnapshot,
    },
    TrustDevice(AuthenticationSnapshot),
}

pub(super) fn transcript() -> Transcript {
    Rc::new(RefCell::new(Vec::new()))
}

pub(super) fn current_account_call(token: TokenMaterial, device: DeviceMaterial) -> AuthCall {
    AuthCall::CurrentAccount(AuthenticationSnapshot::synthetic(token, device))
}

pub(super) fn revoke_call(token: TokenMaterial, device: DeviceMaterial) -> AuthCall {
    AuthCall::RevokeAccessToken(AuthenticationSnapshot::synthetic(token, device))
}

pub(super) fn begin_password_call(device: DeviceMaterial) -> AuthCall {
    AuthCall::BeginPasswordLogin {
        identifier: RedactedValue,
        password: RedactedValue,
        device_id: device.snapshot(),
    }
}

pub(super) fn request_otp_call(device: DeviceMaterial) -> AuthCall {
    AuthCall::RequestSmsOtp {
        otp_secret: RedactedValue,
        device_id: device.snapshot(),
    }
}

pub(super) fn complete_otp_call(device: DeviceMaterial) -> AuthCall {
    AuthCall::CompleteOtpLogin {
        otp_code: RedactedValue,
        otp_secret: RedactedValue,
        device_id: device.snapshot(),
    }
}

pub(super) fn trust_call(token: TokenMaterial, device: DeviceMaterial) -> AuthCall {
    AuthCall::TrustDevice(AuthenticationSnapshot::synthetic(token, device))
}

pub(super) struct FixedClock(pub(super) OffsetDateTime);

impl Clock for FixedClock {
    fn now_utc(&self) -> OffsetDateTime {
        self.0
    }
}

pub(super) fn test_account(identity: AccountIdentity) -> Result<Account, Box<dyn StdError>> {
    Ok(Account::new(
        UserId::from_str(identity.user_id())?,
        Username::from_bare(USERNAME)?,
        Some(DISPLAY_NAME.to_owned()),
    ))
}

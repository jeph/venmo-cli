use super::*;

#[derive(Debug, Eq, PartialEq)]
pub(super) struct AuthObservation<Outcome> {
    pub(super) outcome: Outcome,
    pub(super) credential_state: CredentialStateSnapshot,
    pub(super) calls: Vec<AuthCall>,
}

impl<Outcome> AuthObservation<Outcome> {
    pub(super) fn new(
        outcome: Outcome,
        credential_state: CredentialStateSnapshot,
        calls: Vec<AuthCall>,
    ) -> Self {
        Self {
            outcome,
            credential_state,
            calls,
        }
    }
}

pub(super) fn observe_store<Outcome>(
    outcome: Outcome,
    store: &FakeStore,
    calls: &Transcript,
) -> AuthObservation<Outcome> {
    AuthObservation::new(outcome, store.snapshot(), calls.borrow().clone())
}

pub(super) fn observe_deleter<Outcome>(
    outcome: Outcome,
    deleter: &DeleteOnlyFake,
    calls: &Transcript,
) -> AuthObservation<Outcome> {
    AuthObservation::new(outcome, deleter.snapshot(), calls.borrow().clone())
}

pub(super) fn observe_reader<Outcome>(
    outcome: Outcome,
    reader: &FakeReader,
    calls: &Transcript,
) -> AuthObservation<Outcome> {
    AuthObservation::new(outcome, reader.snapshot(), calls.borrow().clone())
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct LoginSnapshot {
    pub(super) account: AccountSnapshot,
    pub(super) saved_at: OffsetDateTime,
    pub(super) disposition: LoginDisposition,
}

impl LoginSnapshot {
    pub(super) fn synthetic(
        account: AccountIdentity,
        saved_at: OffsetDateTime,
        disposition: LoginDisposition,
    ) -> Self {
        Self {
            account: AccountSnapshot::synthetic(account),
            saved_at,
            disposition,
        }
    }

    fn from_result(result: &LoginResult) -> Self {
        Self {
            account: AccountSnapshot::from_account(result.account()),
            saved_at: result.saved_at(),
            disposition: result.disposition(),
        }
    }
}

#[test]
fn login_result_compares_all_fields_as_one_redacted_value() -> TestResult {
    let saved_at = OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(5);
    let primary = test_account(AccountIdentity::Primary)?;
    let actual = LoginResult::new(primary.clone(), saved_at, LoginDisposition::Created);
    let equal = LoginResult::new(primary.clone(), saved_at, LoginDisposition::Created);
    let different_account = LoginResult::new(
        test_account(AccountIdentity::Secondary)?,
        saved_at,
        LoginDisposition::Created,
    );
    let different_time = LoginResult::new(
        primary.clone(),
        OffsetDateTime::UNIX_EPOCH,
        LoginDisposition::Created,
    );
    let different_disposition = LoginResult::new(
        primary.clone(),
        saved_at,
        LoginDisposition::ReplacedExistingCredential,
    );

    assert_eq!(actual, equal);
    assert_ne!(actual, different_account);
    assert_ne!(actual, different_time);
    assert_ne!(actual, different_disposition);
    assert_eq!(actual.account(), &primary);
    assert_eq!(actual.saved_at(), saved_at);
    assert_eq!(actual.disposition(), LoginDisposition::Created);

    let rendered = format!("{actual:?}");
    assert!(rendered.contains(REDACTED));
    assert!(!rendered.contains(USERNAME));
    assert!(!rendered.contains(DISPLAY_NAME));
    Ok(())
}

#[derive(Debug, Eq, PartialEq)]
pub(super) enum DeviceTrustSnapshot {
    NotNeeded,
    Trusted,
    Failed(ApiFailureKind),
}

impl DeviceTrustSnapshot {
    fn from_outcome(outcome: &DeviceTrustOutcome) -> Self {
        match outcome {
            DeviceTrustOutcome::NotNeeded => Self::NotNeeded,
            DeviceTrustOutcome::Trusted => Self::Trusted,
            DeviceTrustOutcome::Failed(source) => Self::Failed(source.kind()),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct PasswordLoginSnapshot {
    pub(super) login: LoginSnapshot,
    pub(super) device_trust: DeviceTrustSnapshot,
    pub(super) complete: bool,
}

impl PasswordLoginSnapshot {
    pub(super) fn synthetic(login: LoginSnapshot, device_trust: DeviceTrustSnapshot) -> Self {
        let complete = matches!(
            device_trust,
            DeviceTrustSnapshot::NotNeeded | DeviceTrustSnapshot::Trusted
        );
        Self {
            login,
            device_trust,
            complete,
        }
    }

    fn from_report(report: &PasswordLoginReport) -> Self {
        Self {
            login: LoginSnapshot::from_result(report.login()),
            device_trust: DeviceTrustSnapshot::from_outcome(report.device_trust()),
            complete: report.is_complete_success(),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(super) enum PromptFailureSnapshot {
    Cancelled,
    NotInteractive,
    InvalidDeviceId,
    InvalidLoginIdentifier,
    InvalidAccountPassword,
    InvalidOtpCode,
    Interaction,
}

impl PromptFailureSnapshot {
    fn from_error(error: &PromptError) -> Self {
        match error {
            PromptError::Cancelled => Self::Cancelled,
            PromptError::NotInteractive => Self::NotInteractive,
            PromptError::InvalidDeviceId { .. } => Self::InvalidDeviceId,
            PromptError::InvalidLoginIdentifier { .. } => Self::InvalidLoginIdentifier,
            PromptError::InvalidAccountPassword { .. } => Self::InvalidAccountPassword,
            PromptError::InvalidOtpCode { .. } => Self::InvalidOtpCode,
            PromptError::Interaction { .. } => Self::Interaction,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(super) enum StorageFailureSnapshot {
    Operation,
    MissingOrMismatch,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) enum LoginFailure {
    Prompt(PromptFailureSnapshot),
    CredentialLoad,
    IssuedTokenValidation(ApiFailureKind),
    PasswordAuthentication(ApiFailureKind),
    OtpRequest(ApiFailureKind),
    OtpCompletion(ApiFailureKind),
    IssuedCredentialStorageStateUnknown(StorageFailureSnapshot),
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct LoginFailureSnapshot {
    pub(super) kind: ApplicationFailureKind,
    pub(super) failure: LoginFailure,
}

impl LoginFailureSnapshot {
    fn from_error(error: &LoginError) -> Self {
        let failure = match error {
            LoginError::Prompt(source) => {
                LoginFailure::Prompt(PromptFailureSnapshot::from_error(source))
            }
            LoginError::CredentialLoad { .. } => LoginFailure::CredentialLoad,
            LoginError::IssuedTokenValidation { source } => {
                LoginFailure::IssuedTokenValidation(source.kind())
            }
            LoginError::PasswordAuthentication { source } => {
                LoginFailure::PasswordAuthentication(source.kind())
            }
            LoginError::OtpRequest { source } => LoginFailure::OtpRequest(source.kind()),
            LoginError::OtpCompletion { source } => LoginFailure::OtpCompletion(source.kind()),
            LoginError::IssuedCredentialStorageStateUnknown { source } => {
                LoginFailure::IssuedCredentialStorageStateUnknown(if source.is_some() {
                    StorageFailureSnapshot::Operation
                } else {
                    StorageFailureSnapshot::MissingOrMismatch
                })
            }
        };
        Self {
            kind: error.failure_kind(),
            failure,
        }
    }

    pub(super) const fn synthetic(kind: ApplicationFailureKind, failure: LoginFailure) -> Self {
        Self { kind, failure }
    }
}

pub(super) fn password_outcome(
    result: &Result<PasswordLoginReport, LoginError>,
) -> Result<PasswordLoginSnapshot, LoginFailureSnapshot> {
    match result {
        Ok(report) => Ok(PasswordLoginSnapshot::from_report(report)),
        Err(error) => Err(LoginFailureSnapshot::from_error(error)),
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct AuthStatusSnapshot {
    pub(super) account: AccountSnapshot,
    pub(super) saved_at: OffsetDateTime,
    pub(super) credential_format: CredentialFormat,
}

impl AuthStatusSnapshot {
    pub(super) fn synthetic(
        account: AccountIdentity,
        saved_at: OffsetDateTime,
        credential_format: CredentialFormat,
    ) -> Self {
        Self {
            account: AccountSnapshot::synthetic(account),
            saved_at,
            credential_format,
        }
    }

    fn from_status(status: &AuthStatus) -> Self {
        Self {
            account: AccountSnapshot::from_account(status.account()),
            saved_at: status.saved_at(),
            credential_format: status.credential_format(),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(super) enum AuthStatusFailure {
    CredentialMissing,
    CredentialRead,
    TokenValidation(ApiFailureKind),
    AccountMismatch,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct AuthStatusFailureSnapshot {
    pub(super) kind: ApplicationFailureKind,
    pub(super) failure: AuthStatusFailure,
}

impl AuthStatusFailureSnapshot {
    fn from_error(error: &AuthStatusError) -> Self {
        let failure = match error {
            AuthStatusError::Credential(crate::shared::CredentialAccessError::Missing) => {
                AuthStatusFailure::CredentialMissing
            }
            AuthStatusError::Credential(crate::shared::CredentialAccessError::Read { .. }) => {
                AuthStatusFailure::CredentialRead
            }
            AuthStatusError::TokenValidation { source } => {
                AuthStatusFailure::TokenValidation(source.kind())
            }
            AuthStatusError::AccountMismatch => AuthStatusFailure::AccountMismatch,
        };
        Self {
            kind: error.failure_kind(),
            failure,
        }
    }

    pub(super) const fn synthetic(
        kind: ApplicationFailureKind,
        failure: AuthStatusFailure,
    ) -> Self {
        Self { kind, failure }
    }
}

pub(super) fn status_outcome(
    result: &Result<AuthStatus, AuthStatusError>,
) -> Result<AuthStatusSnapshot, AuthStatusFailureSnapshot> {
    match result {
        Ok(status) => Ok(AuthStatusSnapshot::from_status(status)),
        Err(error) => Err(AuthStatusFailureSnapshot::from_error(error)),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LocalDeletionSnapshot {
    Deleted,
    Missing,
    Failed,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct LogoutSnapshot {
    pub(super) local: LocalDeletionSnapshot,
    pub(super) complete: bool,
    pub(super) failure_kind: Option<ApplicationFailureKind>,
}

impl LogoutSnapshot {
    pub(super) const fn synthetic(
        local: LocalDeletionSnapshot,
        complete: bool,
        failure_kind: Option<ApplicationFailureKind>,
    ) -> Self {
        Self {
            local,
            complete,
            failure_kind,
        }
    }

    pub(super) fn from_report(report: &LogoutReport) -> Self {
        let local = match report.local() {
            LocalDeletionOutcome::Deleted => LocalDeletionSnapshot::Deleted,
            LocalDeletionOutcome::Missing => LocalDeletionSnapshot::Missing,
            LocalDeletionOutcome::Failed(_) => LocalDeletionSnapshot::Failed,
        };
        Self {
            local,
            complete: report.is_complete_success(),
            failure_kind: report.failure_kind(),
        }
    }
}

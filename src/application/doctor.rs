use std::collections::HashSet;

use super::ports::{
    ApiFailure, ApiFailureKind, CredentialReader, DoctorApi, LoadedCredential, RequiredShape,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DoctorCheckStatus {
    Pass,
    Fail,
    Skipped,
}

impl DoctorCheckStatus {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pass => "Pass",
            Self::Fail => "Fail",
            Self::Skipped => "Skipped",
        }
    }
}

#[derive(Debug)]
pub struct DoctorCheck {
    name: &'static str,
    status: DoctorCheckStatus,
    detail: String,
    remediation: Option<&'static str>,
}

impl DoctorCheck {
    #[must_use]
    pub fn new(
        name: &'static str,
        status: DoctorCheckStatus,
        detail: impl Into<String>,
        remediation: Option<&'static str>,
    ) -> Self {
        Self {
            name,
            status,
            detail: detail.into(),
            remediation,
        }
    }

    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    #[must_use]
    pub const fn status(&self) -> DoctorCheckStatus {
        self.status
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }

    #[must_use]
    pub const fn remediation(&self) -> Option<&'static str> {
        self.remediation
    }
}

#[derive(Debug)]
pub struct DoctorReport {
    checks: Vec<DoctorCheck>,
}

impl DoctorReport {
    #[must_use]
    pub fn new(checks: Vec<DoctorCheck>) -> Self {
        Self { checks }
    }

    #[must_use]
    pub fn checks(&self) -> &[DoctorCheck] {
        &self.checks
    }

    #[must_use]
    pub fn is_healthy(&self) -> bool {
        self.checks
            .iter()
            .all(|check| check.status() == DoctorCheckStatus::Pass)
    }
}

pub async fn diagnose<S, A>(
    store: &S,
    api: Option<&A>,
    api_initialization_failed: bool,
) -> DoctorReport
where
    S: CredentialReader,
    A: DoctorApi,
{
    let mut checks = Vec::with_capacity(6);
    checks.push(DoctorCheck::new(
        "Build and platform",
        DoctorCheckStatus::Pass,
        format!(
            "venmo {} on {}/{}",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
        None,
    ));

    let credential = match store.read_credential() {
        Ok(credential) => {
            checks.push(DoctorCheck::new(
                "Credential store",
                DoctorCheckStatus::Pass,
                "the native credential store is readable",
                None,
            ));
            match credential {
                Some(loaded) => {
                    checks.push(DoctorCheck::new(
                        "Credential presence and schema",
                        DoctorCheckStatus::Pass,
                        "a supported credential envelope is present",
                        None,
                    ));
                    Some(loaded)
                }
                None => {
                    checks.push(DoctorCheck::new(
                        "Credential presence and schema",
                        DoctorCheckStatus::Fail,
                        "no credential is stored",
                        Some("Run `venmo auth login` from an interactive terminal."),
                    ));
                    None
                }
            }
        }
        Err(_) => {
            checks.push(DoctorCheck::new(
                "Credential store",
                DoctorCheckStatus::Fail,
                "the native credential store could not be read",
                Some(
                    "Unlock or configure the OS credential store; Linux requires a user-session Secret Service.",
                ),
            ));
            checks.push(DoctorCheck::new(
                "Credential presence and schema",
                DoctorCheckStatus::Skipped,
                "credential inspection depends on a readable credential store",
                None,
            ));
            None
        }
    };

    match api {
        Some(api) => match api.connectivity().await {
            Ok(()) => checks.push(DoctorCheck::new(
                "Connectivity and TLS",
                DoctorCheckStatus::Pass,
                "the fixed Venmo API origin returned an HTTPS response",
                None,
            )),
            Err(error) => checks.push(DoctorCheck::new(
                "Connectivity and TLS",
                DoctorCheckStatus::Fail,
                api_failure_detail(error.kind()),
                Some("Check DNS, HTTPS connectivity, system time, and TLS trust settings."),
            )),
        },
        None => checks.push(DoctorCheck::new(
            "Connectivity and TLS",
            DoctorCheckStatus::Fail,
            if api_initialization_failed {
                "the HTTPS client could not be initialized"
            } else {
                "the Venmo API client is unavailable"
            },
            Some("Review the local TLS/runtime installation and retry."),
        )),
    }

    let authenticated = authenticate_for_doctor(api, credential.as_ref()).await;
    match &authenticated {
        AuthenticationDiagnostic::Passed => checks.push(DoctorCheck::new(
            "Current-account authentication",
            DoctorCheckStatus::Pass,
            "the stored credential matches the authenticated account",
            None,
        )),
        AuthenticationDiagnostic::Failed(kind) => checks.push(DoctorCheck::new(
            "Current-account authentication",
            DoctorCheckStatus::Fail,
            api_failure_detail(*kind),
            Some(
                "Run `venmo auth reauthenticate`; use `venmo auth login --token` only to import a replacement bearer.",
            ),
        )),
        AuthenticationDiagnostic::AccountMismatch => checks.push(DoctorCheck::new(
            "Current-account authentication",
            DoctorCheckStatus::Fail,
            "the live account does not match the stored account identity",
            Some("Run `venmo auth logout`, then authenticate the intended account."),
        )),
        AuthenticationDiagnostic::Skipped => checks.push(DoctorCheck::new(
            "Current-account authentication",
            DoctorCheckStatus::Skipped,
            "authentication depends on an available API client and usable credential",
            None,
        )),
    }

    match (api, credential.as_ref(), authenticated) {
        (Some(api), Some(credential), AuthenticationDiagnostic::Passed) => {
            let outcomes = api
                .required_shapes(
                    credential.envelope.access_token(),
                    credential.envelope.device_id(),
                    credential.envelope.user_id(),
                )
                .await;
            checks.push(required_shapes_check(&outcomes));
        }
        _ => checks.push(DoctorCheck::new(
            "Required private-API shapes",
            DoctorCheckStatus::Skipped,
            "shape checks depend on successful current-account authentication",
            None,
        )),
    }

    DoctorReport::new(checks)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AuthenticationDiagnostic {
    Passed,
    Failed(ApiFailureKind),
    AccountMismatch,
    Skipped,
}

async fn authenticate_for_doctor<A: DoctorApi>(
    api: Option<&A>,
    credential: Option<&LoadedCredential>,
) -> AuthenticationDiagnostic {
    let (Some(api), Some(credential)) = (api, credential) else {
        return AuthenticationDiagnostic::Skipped;
    };
    match api
        .diagnostic_current_account(
            credential.envelope.access_token(),
            credential.envelope.device_id(),
        )
        .await
    {
        Ok(account) if account.user_id() == credential.envelope.user_id() => {
            AuthenticationDiagnostic::Passed
        }
        Ok(_) => AuthenticationDiagnostic::AccountMismatch,
        Err(error) => AuthenticationDiagnostic::Failed(error.kind()),
    }
}

fn required_shapes_check(outcomes: &[super::ports::ShapeProbeOutcome]) -> DoctorCheck {
    const REQUIRED: [RequiredShape; 5] = [
        RequiredShape::Balance,
        RequiredShape::PaymentMethods,
        RequiredShape::Friends,
        RequiredShape::Activity,
        RequiredShape::PendingRequests,
    ];
    let mut seen = HashSet::with_capacity(outcomes.len());
    let mut failures = Vec::new();
    for outcome in outcomes {
        if !seen.insert(outcome.shape()) {
            failures.push("duplicate diagnostic result".to_owned());
        } else if let Some(kind) = outcome.failure() {
            failures.push(format!(
                "{} ({})",
                outcome.shape().label(),
                api_failure_detail(kind)
            ));
        }
    }
    for required in REQUIRED {
        if !seen.contains(&required) {
            failures.push(format!("{} (missing diagnostic result)", required.label()));
        }
    }
    if failures.is_empty() {
        DoctorCheck::new(
            "Required private-API shapes",
            DoctorCheckStatus::Pass,
            "all required read-only response shapes were recognized",
            None,
        )
    } else {
        DoctorCheck::new(
            "Required private-API shapes",
            DoctorCheckStatus::Fail,
            failures.join(", "),
            Some("The private Venmo API may have changed; update the CLI before relying on it."),
        )
    }
}

fn api_failure_detail(kind: ApiFailureKind) -> &'static str {
    match kind {
        ApiFailureKind::Network => "the network request failed",
        ApiFailureKind::Timeout => "the network request timed out",
        ApiFailureKind::Rejected => "Venmo rejected the request",
        ApiFailureKind::Contract => "the private API response shape was not recognized",
        ApiFailureKind::AmbiguousWrite => "an unexpected ambiguous-write classification occurred",
        ApiFailureKind::Internal => "an internal API-client failure occurred",
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::error::Error;
    use std::future::{Future, ready};
    use std::str::FromStr;

    use time::OffsetDateTime;

    use super::*;
    use crate::application::ports::{
        CredentialFailureKind, CredentialFormat, CredentialStoreFailure, ShapeProbeOutcome,
    };
    use crate::domain::{AccessToken, Account, CredentialEnvelope, DeviceId, UserId, Username};

    type TestResult = Result<(), Box<dyn Error>>;

    #[derive(Clone, Copy)]
    enum ReaderState {
        Present,
        Missing,
        Unreadable,
    }

    // This fake intentionally exposes no save or delete capability. Compiling calls to
    // `diagnose` with it proves that diagnostics are constructed from a read-only port.
    struct ReadOnlyCredentialReader {
        state: ReaderState,
        read_calls: Cell<usize>,
    }

    impl ReadOnlyCredentialReader {
        fn new(state: ReaderState) -> Self {
            Self {
                state,
                read_calls: Cell::new(0),
            }
        }
    }

    #[derive(Debug, thiserror::Error)]
    #[error("fake credential reader failure")]
    struct FakeCredentialError;

    impl CredentialStoreFailure for FakeCredentialError {
        fn kind(&self) -> CredentialFailureKind {
            CredentialFailureKind::Unavailable
        }
    }

    impl CredentialReader for ReadOnlyCredentialReader {
        type Error = FakeCredentialError;

        fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
            self.read_calls.set(self.read_calls.get() + 1);
            match self.state {
                ReaderState::Present => Ok(Some(loaded_credential()?)),
                ReaderState::Missing => Ok(None),
                ReaderState::Unreadable => Err(FakeCredentialError),
            }
        }
    }

    #[derive(Clone, Copy, Debug, thiserror::Error)]
    #[error("fake API failure")]
    struct FakeApiError(ApiFailureKind);

    impl ApiFailure for FakeApiError {
        fn kind(&self) -> ApiFailureKind {
            self.0
        }
    }

    #[derive(Clone, Copy)]
    enum AccountOutcome {
        Matching,
        Mismatched,
        Failure(ApiFailureKind),
    }

    struct FakeDoctorApi {
        connectivity_result: Result<(), FakeApiError>,
        account_outcome: AccountOutcome,
        shape_outcomes: Vec<ShapeProbeOutcome>,
        connectivity_calls: Cell<usize>,
        account_calls: Cell<usize>,
        shape_calls: Cell<usize>,
    }

    impl FakeDoctorApi {
        fn healthy() -> Self {
            Self {
                connectivity_result: Ok(()),
                account_outcome: AccountOutcome::Matching,
                shape_outcomes: all_shapes_pass(),
                connectivity_calls: Cell::new(0),
                account_calls: Cell::new(0),
                shape_calls: Cell::new(0),
            }
        }

        fn with_account_outcome(account_outcome: AccountOutcome) -> Self {
            Self {
                account_outcome,
                ..Self::healthy()
            }
        }

        fn with_shapes(shape_outcomes: Vec<ShapeProbeOutcome>) -> Self {
            Self {
                shape_outcomes,
                ..Self::healthy()
            }
        }

        fn with_connectivity_failure(kind: ApiFailureKind) -> Self {
            Self {
                connectivity_result: Err(FakeApiError(kind)),
                ..Self::healthy()
            }
        }
    }

    impl DoctorApi for FakeDoctorApi {
        type Error = FakeApiError;

        fn connectivity(&self) -> impl Future<Output = Result<(), Self::Error>> + Send + '_ {
            self.connectivity_calls
                .set(self.connectivity_calls.get() + 1);
            ready(self.connectivity_result)
        }

        fn diagnostic_current_account<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
        ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a {
            self.account_calls.set(self.account_calls.get() + 1);
            let result = match self.account_outcome {
                AccountOutcome::Matching => account("1000"),
                AccountOutcome::Mismatched => account("2000"),
                AccountOutcome::Failure(kind) => Err(FakeApiError(kind)),
            };
            ready(result)
        }

        fn required_shapes<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            _current_user_id: &'a UserId,
        ) -> impl Future<Output = Vec<ShapeProbeOutcome>> + Send + 'a {
            self.shape_calls.set(self.shape_calls.get() + 1);
            ready(self.shape_outcomes.clone())
        }
    }

    fn loaded_credential() -> Result<LoadedCredential, FakeCredentialError> {
        Ok(LoadedCredential {
            envelope: CredentialEnvelope::new(
                AccessToken::from_str("synthetic-token").map_err(|_| FakeCredentialError)?,
                DeviceId::from_str("synthetic-device").map_err(|_| FakeCredentialError)?,
                UserId::from_str("1000").map_err(|_| FakeCredentialError)?,
                Username::from_bare("tester".to_owned()).map_err(|_| FakeCredentialError)?,
                Some("Test User".to_owned()),
                OffsetDateTime::UNIX_EPOCH,
            ),
            format: CredentialFormat::Version1,
        })
    }

    fn account(user_id: &str) -> Result<Account, FakeApiError> {
        Ok(Account::new(
            UserId::from_str(user_id).map_err(|_| FakeApiError(ApiFailureKind::Internal))?,
            Username::from_bare("api-user".to_owned())
                .map_err(|_| FakeApiError(ApiFailureKind::Internal))?,
            Some("API User".to_owned()),
        ))
    }

    fn all_shapes_pass() -> Vec<ShapeProbeOutcome> {
        vec![
            ShapeProbeOutcome::passed(RequiredShape::Balance),
            ShapeProbeOutcome::passed(RequiredShape::PaymentMethods),
            ShapeProbeOutcome::passed(RequiredShape::Friends),
            ShapeProbeOutcome::passed(RequiredShape::Activity),
            ShapeProbeOutcome::passed(RequiredShape::PendingRequests),
        ]
    }

    fn statuses(report: &DoctorReport) -> Vec<DoctorCheckStatus> {
        report.checks().iter().map(DoctorCheck::status).collect()
    }

    fn check<'a>(report: &'a DoctorReport, name: &str) -> Option<&'a DoctorCheck> {
        report.checks().iter().find(|check| check.name() == name)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn all_pass_report_uses_only_the_read_only_credential_capability() -> TestResult {
        let reader = ReadOnlyCredentialReader::new(ReaderState::Present);
        let api = FakeDoctorApi::healthy();

        let report = diagnose(&reader, Some(&api), false).await;

        assert_eq!(report.checks().len(), 6);
        assert_eq!(statuses(&report), vec![DoctorCheckStatus::Pass; 6]);
        assert!(report.is_healthy());
        assert_eq!(reader.read_calls.get(), 1);
        assert_eq!(api.connectivity_calls.get(), 1);
        assert_eq!(api.account_calls.get(), 1);
        assert_eq!(api.shape_calls.get(), 1);
        assert_eq!(
            check(&report, "Required private-API shapes").map(DoctorCheck::detail),
            Some("all required read-only response shapes were recognized")
        );
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn account_mismatch_produces_mixed_pass_fail_and_skipped_checks() -> TestResult {
        let reader = ReadOnlyCredentialReader::new(ReaderState::Present);
        let api = FakeDoctorApi::with_account_outcome(AccountOutcome::Mismatched);

        let report = diagnose(&reader, Some(&api), false).await;

        assert_eq!(
            statuses(&report),
            vec![
                DoctorCheckStatus::Pass,
                DoctorCheckStatus::Pass,
                DoctorCheckStatus::Pass,
                DoctorCheckStatus::Pass,
                DoctorCheckStatus::Fail,
                DoctorCheckStatus::Skipped,
            ]
        );
        assert!(!report.is_healthy());
        assert_eq!(
            check(&report, "Current-account authentication").map(DoctorCheck::detail),
            Some("the live account does not match the stored account identity")
        );
        assert_eq!(api.account_calls.get(), 1);
        assert_eq!(api.shape_calls.get(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn missing_credential_skips_authenticated_checks_but_not_connectivity() -> TestResult {
        let reader = ReadOnlyCredentialReader::new(ReaderState::Missing);
        let api = FakeDoctorApi::healthy();

        let report = diagnose(&reader, Some(&api), false).await;

        assert_eq!(
            statuses(&report),
            vec![
                DoctorCheckStatus::Pass,
                DoctorCheckStatus::Pass,
                DoctorCheckStatus::Fail,
                DoctorCheckStatus::Pass,
                DoctorCheckStatus::Skipped,
                DoctorCheckStatus::Skipped,
            ]
        );
        assert_eq!(
            check(&report, "Credential presence and schema").map(DoctorCheck::detail),
            Some("no credential is stored")
        );
        assert_eq!(api.connectivity_calls.get(), 1);
        assert_eq!(api.account_calls.get(), 0);
        assert_eq!(api.shape_calls.get(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn unreadable_credential_store_skips_dependent_checks() -> TestResult {
        let reader = ReadOnlyCredentialReader::new(ReaderState::Unreadable);
        let api = FakeDoctorApi::healthy();

        let report = diagnose(&reader, Some(&api), false).await;

        assert_eq!(
            statuses(&report),
            vec![
                DoctorCheckStatus::Pass,
                DoctorCheckStatus::Fail,
                DoctorCheckStatus::Skipped,
                DoctorCheckStatus::Pass,
                DoctorCheckStatus::Skipped,
                DoctorCheckStatus::Skipped,
            ]
        );
        assert_eq!(
            check(&report, "Credential store").map(DoctorCheck::detail),
            Some("the native credential store could not be read")
        );
        assert_eq!(reader.read_calls.get(), 1);
        assert_eq!(api.connectivity_calls.get(), 1);
        assert_eq!(api.account_calls.get(), 0);
        assert_eq!(api.shape_calls.get(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn shape_failures_duplicates_and_missing_results_are_aggregated() -> TestResult {
        let reader = ReadOnlyCredentialReader::new(ReaderState::Present);
        let api = FakeDoctorApi::with_shapes(vec![
            ShapeProbeOutcome::passed(RequiredShape::Balance),
            ShapeProbeOutcome::passed(RequiredShape::Balance),
            ShapeProbeOutcome::failed(RequiredShape::Friends, ApiFailureKind::Timeout),
            ShapeProbeOutcome::passed(RequiredShape::Activity),
        ]);

        let report = diagnose(&reader, Some(&api), false).await;

        assert_eq!(
            statuses(&report),
            vec![
                DoctorCheckStatus::Pass,
                DoctorCheckStatus::Pass,
                DoctorCheckStatus::Pass,
                DoctorCheckStatus::Pass,
                DoctorCheckStatus::Pass,
                DoctorCheckStatus::Fail,
            ]
        );
        assert_eq!(
            check(&report, "Required private-API shapes").map(DoctorCheck::detail),
            Some(
                "duplicate diagnostic result, friends (the network request timed out), payment methods (missing diagnostic result), pending requests (missing diagnostic result)"
            )
        );
        assert_eq!(api.shape_calls.get(), 1);
        assert!(!report.is_healthy());
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn connectivity_failure_preserves_the_failure_category_detail() -> TestResult {
        let reader = ReadOnlyCredentialReader::new(ReaderState::Present);
        let api = FakeDoctorApi::with_connectivity_failure(ApiFailureKind::Network);

        let report = diagnose(&reader, Some(&api), false).await;

        assert_eq!(
            check(&report, "Connectivity and TLS").map(DoctorCheck::status),
            Some(DoctorCheckStatus::Fail)
        );
        assert_eq!(
            check(&report, "Connectivity and TLS").map(DoctorCheck::detail),
            Some("the network request failed")
        );
        assert_eq!(api.account_calls.get(), 1);
        assert_eq!(api.shape_calls.get(), 1);
        assert!(!report.is_healthy());
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn account_api_failure_skips_shape_probes() -> TestResult {
        let reader = ReadOnlyCredentialReader::new(ReaderState::Present);
        let api =
            FakeDoctorApi::with_account_outcome(AccountOutcome::Failure(ApiFailureKind::Rejected));

        let report = diagnose(&reader, Some(&api), false).await;

        assert_eq!(
            check(&report, "Current-account authentication").map(DoctorCheck::detail),
            Some("Venmo rejected the request")
        );
        assert_eq!(
            check(&report, "Required private-API shapes").map(DoctorCheck::status),
            Some(DoctorCheckStatus::Skipped)
        );
        assert_eq!(api.shape_calls.get(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn unavailable_api_reports_client_initialization_failure() -> TestResult {
        let reader = ReadOnlyCredentialReader::new(ReaderState::Present);

        let report = diagnose::<_, FakeDoctorApi>(&reader, None, true).await;

        assert_eq!(
            statuses(&report),
            vec![
                DoctorCheckStatus::Pass,
                DoctorCheckStatus::Pass,
                DoctorCheckStatus::Pass,
                DoctorCheckStatus::Fail,
                DoctorCheckStatus::Skipped,
                DoctorCheckStatus::Skipped,
            ]
        );
        assert_eq!(
            check(&report, "Connectivity and TLS").map(DoctorCheck::detail),
            Some("the HTTPS client could not be initialized")
        );
        assert!(!report.is_healthy());
        Ok(())
    }
}

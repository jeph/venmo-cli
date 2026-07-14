use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error;
use std::future::{Future, ready};
use std::rc::Rc;
use std::str::FromStr;

use time::OffsetDateTime;

use super::*;
use crate::shared::{
    AccessToken, Account, ApiFailure, ApiFailureKind, CredentialCapability, CredentialEnvelope,
    CredentialFailureKind, CredentialFormat, CredentialReader, CredentialStoreFailure, DeviceId,
    LoadedCredential, UserId, Username,
};

type TestResult = Result<(), Box<dyn Error>>;
type Transcript = Rc<RefCell<Vec<Call>>>;

const TEST_BUILD_INFO: BuildInfo = BuildInfo::new("1.2.3-test", "test-os", "test-arch");
const FIXTURE_TOKEN: &str = "synthetic-token";
const FIXTURE_DEVICE_ID: &str = "synthetic-device";
const FIXTURE_USER_ID: &str = "1000";
const API_FAILURE_KINDS: [ApiFailureKind; 6] = [
    ApiFailureKind::Network,
    ApiFailureKind::Timeout,
    ApiFailureKind::Rejected,
    ApiFailureKind::Contract,
    ApiFailureKind::AmbiguousWrite,
    ApiFailureKind::Internal,
];
const CREDENTIAL_FAILURE_KINDS: [CredentialFailureKind; 8] = [
    CredentialFailureKind::Unavailable,
    CredentialFailureKind::Corrupt,
    CredentialFailureKind::Invalid,
    CredentialFailureKind::UnsupportedVersion,
    CredentialFailureKind::TooLarge,
    CredentialFailureKind::Ambiguous,
    CredentialFailureKind::Platform,
    CredentialFailureKind::Internal,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SensitiveArgument {
    Fixture,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SessionCall {
    access_token: SensitiveArgument,
    device_id: SensitiveArgument,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Call {
    ReadCredential,
    Connectivity,
    CurrentAccount {
        session: SessionCall,
    },
    RequiredShapes {
        session: SessionCall,
        current_user_id: UserId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReaderOutcome {
    Present,
    Missing,
    Failure(CredentialFailureKind),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResponseId {
    Primary,
    UnexpectedSecond,
}

struct ConnectivityResponse {
    id: ResponseId,
    result: Result<(), FakeApiError>,
}

struct AccountResponse {
    id: ResponseId,
    result: Result<Account, FakeApiError>,
}

struct ShapesResponse {
    id: ResponseId,
    outcomes: Vec<ShapeProbeOutcome>,
}

#[derive(Debug, Eq, PartialEq)]
struct ReportSnapshot {
    report: DoctorReport,
    healthy: bool,
}

impl From<DoctorReport> for ReportSnapshot {
    fn from(report: DoctorReport) -> Self {
        let healthy = report.is_healthy();
        Self { report, healthy }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct FakeState {
    reader: ReaderOutcome,
    remaining_connectivity: Vec<ResponseId>,
    remaining_accounts: Vec<ResponseId>,
    remaining_shapes: Vec<ResponseId>,
    transcript: Vec<Call>,
}

#[derive(Debug, Eq, PartialEq)]
struct Snapshot {
    outcome: ReportSnapshot,
    state: FakeState,
}

// This fake intentionally exposes no save or delete capability. Compiling calls to
// `diagnose` with it proves that diagnostics use only the read-only port.
struct ReadOnlyCredentialReader {
    outcome: ReaderOutcome,
    transcript: Transcript,
}

impl ReadOnlyCredentialReader {
    fn new(outcome: ReaderOutcome, transcript: Transcript) -> Self {
        Self {
            outcome,
            transcript,
        }
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error, Eq, PartialEq)]
#[error("fake credential reader failure")]
struct FakeCredentialError(CredentialFailureKind);

impl CredentialStoreFailure for FakeCredentialError {
    fn kind(&self) -> CredentialFailureKind {
        self.0
    }
}

impl CredentialCapability for ReadOnlyCredentialReader {
    type Error = FakeCredentialError;
}

impl CredentialReader for ReadOnlyCredentialReader {
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        self.transcript.borrow_mut().push(Call::ReadCredential);
        match self.outcome {
            ReaderOutcome::Present => loaded_credential().map(Some),
            ReaderOutcome::Missing => Ok(None),
            ReaderOutcome::Failure(kind) => Err(FakeCredentialError(kind)),
        }
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error, Eq, PartialEq)]
#[error("fake API failure")]
struct FakeApiError(ApiFailureKind);

impl ApiFailure for FakeApiError {
    fn kind(&self) -> ApiFailureKind {
        self.0
    }
}

struct FakeDoctorApi {
    connectivity: RefCell<VecDeque<ConnectivityResponse>>,
    accounts: RefCell<VecDeque<AccountResponse>>,
    shapes: RefCell<VecDeque<ShapesResponse>>,
    transcript: Transcript,
}

impl FakeDoctorApi {
    fn new(
        connectivity: Vec<ConnectivityResponse>,
        accounts: Vec<AccountResponse>,
        shapes: Vec<ShapesResponse>,
        transcript: Transcript,
    ) -> Self {
        Self {
            connectivity: RefCell::new(connectivity.into()),
            accounts: RefCell::new(accounts.into()),
            shapes: RefCell::new(shapes.into()),
            transcript,
        }
    }

    fn healthy(transcript: Transcript) -> Result<Self, FakeApiError> {
        Ok(Self::new(
            vec![connectivity_response(ResponseId::Primary, Ok(()))],
            vec![account_response(
                ResponseId::Primary,
                account(FIXTURE_USER_ID),
            )],
            vec![shapes_response(ResponseId::Primary, all_shapes_pass())],
            transcript,
        ))
    }

    fn remaining_connectivity(&self) -> Vec<ResponseId> {
        self.connectivity
            .borrow()
            .iter()
            .map(|response| response.id)
            .collect()
    }

    fn remaining_accounts(&self) -> Vec<ResponseId> {
        self.accounts
            .borrow()
            .iter()
            .map(|response| response.id)
            .collect()
    }

    fn remaining_shapes(&self) -> Vec<ResponseId> {
        self.shapes
            .borrow()
            .iter()
            .map(|response| response.id)
            .collect()
    }
}

impl DoctorApi for FakeDoctorApi {
    type Error = FakeApiError;

    fn connectivity(&self) -> impl Future<Output = Result<(), Self::Error>> + Send + '_ {
        self.transcript.borrow_mut().push(Call::Connectivity);
        let result = self
            .connectivity
            .borrow_mut()
            .pop_front()
            .map_or(Err(FakeApiError(ApiFailureKind::Internal)), |response| {
                response.result
            });
        ready(result)
    }

    fn diagnostic_current_account<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::CurrentAccount {
            session: session_call(access_token, device_id),
        });
        let result = self
            .accounts
            .borrow_mut()
            .pop_front()
            .map_or(Err(FakeApiError(ApiFailureKind::Internal)), |response| {
                response.result
            });
        ready(result)
    }

    fn required_shapes<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
    ) -> impl Future<Output = Vec<ShapeProbeOutcome>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::RequiredShapes {
            session: session_call(access_token, device_id),
            current_user_id: current_user_id.clone(),
        });
        let outcomes = self
            .shapes
            .borrow_mut()
            .pop_front()
            .map_or_else(Vec::new, |response| response.outcomes);
        ready(outcomes)
    }
}

fn session_call(access_token: &AccessToken, device_id: &DeviceId) -> SessionCall {
    SessionCall {
        access_token: if access_token.expose_secret() == FIXTURE_TOKEN {
            SensitiveArgument::Fixture
        } else {
            SensitiveArgument::Other
        },
        device_id: if device_id.as_str() == FIXTURE_DEVICE_ID {
            SensitiveArgument::Fixture
        } else {
            SensitiveArgument::Other
        },
    }
}

const fn fixture_session() -> SessionCall {
    SessionCall {
        access_token: SensitiveArgument::Fixture,
        device_id: SensitiveArgument::Fixture,
    }
}

const fn connectivity_response(
    id: ResponseId,
    result: Result<(), FakeApiError>,
) -> ConnectivityResponse {
    ConnectivityResponse { id, result }
}

fn account_response(id: ResponseId, result: Result<Account, FakeApiError>) -> AccountResponse {
    AccountResponse { id, result }
}

fn shapes_response(id: ResponseId, outcomes: Vec<ShapeProbeOutcome>) -> ShapesResponse {
    ShapesResponse { id, outcomes }
}

fn snapshot(
    report: DoctorReport,
    reader: &ReadOnlyCredentialReader,
    api: &FakeDoctorApi,
    transcript: &Transcript,
) -> Snapshot {
    Snapshot {
        outcome: ReportSnapshot::from(report),
        state: FakeState {
            reader: reader.outcome,
            remaining_connectivity: api.remaining_connectivity(),
            remaining_accounts: api.remaining_accounts(),
            remaining_shapes: api.remaining_shapes(),
            transcript: transcript.borrow().clone(),
        },
    }
}

fn expected_calls_through_shapes() -> Result<Vec<Call>, Box<dyn Error>> {
    Ok(vec![
        Call::ReadCredential,
        Call::Connectivity,
        Call::CurrentAccount {
            session: fixture_session(),
        },
        Call::RequiredShapes {
            session: fixture_session(),
            current_user_id: UserId::from_str(FIXTURE_USER_ID)?,
        },
    ])
}

#[tokio::test(flavor = "current_thread")]
async fn all_pass_report_is_complete_and_uses_read_only_capabilities_once() -> TestResult {
    // Setup.
    let expected_report = healthy_report();

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = ReadOnlyCredentialReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
    let api = FakeDoctorApi::new(
        vec![
            connectivity_response(ResponseId::Primary, Ok(())),
            connectivity_response(
                ResponseId::UnexpectedSecond,
                Err(FakeApiError(ApiFailureKind::Internal)),
            ),
        ],
        vec![
            account_response(ResponseId::Primary, account(FIXTURE_USER_ID)),
            account_response(
                ResponseId::UnexpectedSecond,
                Err(FakeApiError(ApiFailureKind::Internal)),
            ),
        ],
        vec![
            shapes_response(ResponseId::Primary, all_shapes_pass()),
            shapes_response(ResponseId::UnexpectedSecond, Vec::new()),
        ],
        Rc::clone(&transcript),
    );

    // Complete expected outcome and final fake state.
    let expected = Snapshot {
        outcome: ReportSnapshot {
            report: expected_report,
            healthy: true,
        },
        state: FakeState {
            reader: ReaderOutcome::Present,
            remaining_connectivity: vec![ResponseId::UnexpectedSecond],
            remaining_accounts: vec![ResponseId::UnexpectedSecond],
            remaining_shapes: vec![ResponseId::UnexpectedSecond],
            transcript: expected_calls_through_shapes()?,
        },
    };

    // Execute once.
    let report = diagnose(&reader, ApiAvailability::Ready(&api), TEST_BUILD_INFO).await;
    let observed = snapshot(report, &reader, &api, &transcript);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn credential_load_matrix_produces_complete_reports_and_skips_dependents() -> TestResult {
    // Setup.
    let cases = std::iter::once((
        ReaderOutcome::Missing,
        expected_report(
            pass(
                "Credential store",
                "the native credential store is readable",
            ),
            fail(
                "Credential presence and schema",
                "no credential is stored",
                "Run `venmo auth login` from an interactive terminal.",
            ),
            connectivity_pass(),
            authentication_skipped(),
            shapes_skipped(),
        ),
    ))
    .chain(CREDENTIAL_FAILURE_KINDS.into_iter().map(|kind| {
        (
            ReaderOutcome::Failure(kind),
            expected_report(
                fail(
                    "Credential store",
                    "the native credential store could not be read",
                    "Unlock or configure the OS credential store; Linux requires a user-session Secret Service.",
                ),
                skipped(
                    "Credential presence and schema",
                    "credential inspection depends on a readable credential store",
                ),
                connectivity_pass(),
                authentication_skipped(),
                shapes_skipped(),
            ),
        )
    }));

    for (reader_outcome, report) in cases {
        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = ReadOnlyCredentialReader::new(reader_outcome, Rc::clone(&transcript));
        let api = FakeDoctorApi::healthy(Rc::clone(&transcript))?;

        // Complete expected outcome and final fake state.
        let expected = Snapshot {
            outcome: ReportSnapshot {
                report,
                healthy: false,
            },
            state: FakeState {
                reader: reader_outcome,
                remaining_connectivity: Vec::new(),
                remaining_accounts: vec![ResponseId::Primary],
                remaining_shapes: vec![ResponseId::Primary],
                transcript: vec![Call::ReadCredential, Call::Connectivity],
            },
        };

        // Execute once.
        let report = diagnose(&reader, ApiAvailability::Ready(&api), TEST_BUILD_INFO).await;
        let observed = snapshot(report, &reader, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn api_unavailable_state_matrix_is_deterministic_and_makes_no_api_call() -> TestResult {
    // Setup.
    let cases = [
        (
            ReaderOutcome::Present,
            expected_report(
                credential_store_pass(),
                credential_schema_pass(),
                initialization_failed(),
                authentication_skipped(),
                shapes_skipped(),
            ),
        ),
        (
            ReaderOutcome::Missing,
            expected_report(
                credential_store_pass(),
                fail(
                    "Credential presence and schema",
                    "no credential is stored",
                    "Run `venmo auth login` from an interactive terminal.",
                ),
                initialization_failed(),
                authentication_skipped(),
                shapes_skipped(),
            ),
        ),
        (
            ReaderOutcome::Failure(CredentialFailureKind::Unavailable),
            expected_report(
                fail(
                    "Credential store",
                    "the native credential store could not be read",
                    "Unlock or configure the OS credential store; Linux requires a user-session Secret Service.",
                ),
                skipped(
                    "Credential presence and schema",
                    "credential inspection depends on a readable credential store",
                ),
                initialization_failed(),
                authentication_skipped(),
                shapes_skipped(),
            ),
        ),
    ];

    for (reader_outcome, report) in cases {
        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = ReadOnlyCredentialReader::new(reader_outcome, Rc::clone(&transcript));
        let api = FakeDoctorApi::healthy(Rc::clone(&transcript))?;

        // Complete expected outcome and final fake state.
        let expected = Snapshot {
            outcome: ReportSnapshot {
                report,
                healthy: false,
            },
            state: FakeState {
                reader: reader_outcome,
                remaining_connectivity: vec![ResponseId::Primary],
                remaining_accounts: vec![ResponseId::Primary],
                remaining_shapes: vec![ResponseId::Primary],
                transcript: vec![Call::ReadCredential],
            },
        };

        // Execute once.
        let report = diagnose::<_, FakeDoctorApi>(
            &reader,
            ApiAvailability::InitializationFailed,
            TEST_BUILD_INFO,
        )
        .await;
        let observed = snapshot(report, &reader, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn account_mismatch_produces_a_complete_mixed_report() -> TestResult {
    // Setup.
    let report = expected_report(
        credential_store_pass(),
        credential_schema_pass(),
        connectivity_pass(),
        fail(
            "Current-account authentication",
            "the live account does not match the stored account identity",
            "Run `venmo auth logout`, then authenticate the intended account.",
        ),
        shapes_skipped(),
    );

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = ReadOnlyCredentialReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
    let api = FakeDoctorApi::new(
        vec![connectivity_response(ResponseId::Primary, Ok(()))],
        vec![account_response(ResponseId::Primary, account("2000"))],
        vec![shapes_response(ResponseId::Primary, all_shapes_pass())],
        Rc::clone(&transcript),
    );

    // Complete expected outcome and final fake state.
    let expected = Snapshot {
        outcome: ReportSnapshot {
            report,
            healthy: false,
        },
        state: FakeState {
            reader: ReaderOutcome::Present,
            remaining_connectivity: Vec::new(),
            remaining_accounts: Vec::new(),
            remaining_shapes: vec![ResponseId::Primary],
            transcript: vec![
                Call::ReadCredential,
                Call::Connectivity,
                Call::CurrentAccount {
                    session: fixture_session(),
                },
            ],
        },
    };

    // Execute once.
    let report = diagnose(&reader, ApiAvailability::Ready(&api), TEST_BUILD_INFO).await;
    let observed = snapshot(report, &reader, &api, &transcript);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn connectivity_api_failure_kind_matrix_has_deterministic_details() -> TestResult {
    // Setup.
    for kind in API_FAILURE_KINDS {
        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = ReadOnlyCredentialReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
        let api = FakeDoctorApi::new(
            vec![connectivity_response(
                ResponseId::Primary,
                Err(FakeApiError(kind)),
            )],
            vec![account_response(
                ResponseId::Primary,
                account(FIXTURE_USER_ID),
            )],
            vec![shapes_response(ResponseId::Primary, all_shapes_pass())],
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Snapshot {
            outcome: ReportSnapshot {
                report: expected_report(
                    credential_store_pass(),
                    credential_schema_pass(),
                    fail(
                        "Connectivity and TLS",
                        api_failure_detail(kind),
                        "Check DNS, HTTPS connectivity, system time, and TLS trust settings.",
                    ),
                    authentication_pass(),
                    shapes_pass(),
                ),
                healthy: false,
            },
            state: FakeState {
                reader: ReaderOutcome::Present,
                remaining_connectivity: Vec::new(),
                remaining_accounts: Vec::new(),
                remaining_shapes: Vec::new(),
                transcript: expected_calls_through_shapes()?,
            },
        };

        // Execute once.
        let report = diagnose(&reader, ApiAvailability::Ready(&api), TEST_BUILD_INFO).await;
        let observed = snapshot(report, &reader, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn current_account_api_failure_kind_matrix_skips_shapes() -> TestResult {
    // Setup.
    for kind in API_FAILURE_KINDS {
        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = ReadOnlyCredentialReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
        let api = FakeDoctorApi::new(
            vec![connectivity_response(ResponseId::Primary, Ok(()))],
            vec![account_response(
                ResponseId::Primary,
                Err(FakeApiError(kind)),
            )],
            vec![shapes_response(ResponseId::Primary, all_shapes_pass())],
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Snapshot {
            outcome: ReportSnapshot {
                report: expected_report(
                    credential_store_pass(),
                    credential_schema_pass(),
                    connectivity_pass(),
                    fail(
                        "Current-account authentication",
                        api_failure_detail(kind),
                        "Run `venmo auth reauthenticate`; use `venmo auth login --token` only to import a replacement bearer.",
                    ),
                    shapes_skipped(),
                ),
                healthy: false,
            },
            state: FakeState {
                reader: ReaderOutcome::Present,
                remaining_connectivity: Vec::new(),
                remaining_accounts: Vec::new(),
                remaining_shapes: vec![ResponseId::Primary],
                transcript: vec![
                    Call::ReadCredential,
                    Call::Connectivity,
                    Call::CurrentAccount {
                        session: fixture_session(),
                    },
                ],
            },
        };

        // Execute once.
        let report = diagnose(&reader, ApiAvailability::Ready(&api), TEST_BUILD_INFO).await;
        let observed = snapshot(report, &reader, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn shape_api_failure_kind_matrix_has_deterministic_complete_reports() -> TestResult {
    // Setup.
    for kind in API_FAILURE_KINDS {
        let outcomes = RequiredShape::ALL
            .into_iter()
            .map(|shape| {
                if shape == RequiredShape::Balance {
                    ShapeProbeOutcome::failed(shape, kind)
                } else {
                    ShapeProbeOutcome::passed(shape)
                }
            })
            .collect();

        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = ReadOnlyCredentialReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
        let api = FakeDoctorApi::new(
            vec![connectivity_response(ResponseId::Primary, Ok(()))],
            vec![account_response(
                ResponseId::Primary,
                account(FIXTURE_USER_ID),
            )],
            vec![shapes_response(ResponseId::Primary, outcomes)],
            Rc::clone(&transcript),
        );

        // Complete expected outcome and final fake state.
        let expected = Snapshot {
            outcome: ReportSnapshot {
                report: expected_report(
                    credential_store_pass(),
                    credential_schema_pass(),
                    connectivity_pass(),
                    authentication_pass(),
                    fail(
                        "Required private-API shapes",
                        format!("balance ({})", api_failure_detail(kind)),
                        "The private Venmo API may have changed; update the CLI before relying on it.",
                    ),
                ),
                healthy: false,
            },
            state: FakeState {
                reader: ReaderOutcome::Present,
                remaining_connectivity: Vec::new(),
                remaining_accounts: Vec::new(),
                remaining_shapes: Vec::new(),
                transcript: expected_calls_through_shapes()?,
            },
        };

        // Execute once.
        let report = diagnose(&reader, ApiAvailability::Ready(&api), TEST_BUILD_INFO).await;
        let observed = snapshot(report, &reader, &api, &transcript);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn duplicate_failed_and_missing_shape_results_are_aggregated_deterministically() -> TestResult
{
    // Setup.
    let outcomes = vec![
        ShapeProbeOutcome::passed(RequiredShape::Balance),
        ShapeProbeOutcome::passed(RequiredShape::Balance),
        ShapeProbeOutcome::failed(RequiredShape::Friends, ApiFailureKind::Timeout),
        ShapeProbeOutcome::passed(RequiredShape::Activity),
    ];

    // Immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = ReadOnlyCredentialReader::new(ReaderOutcome::Present, Rc::clone(&transcript));
    let api = FakeDoctorApi::new(
        vec![connectivity_response(ResponseId::Primary, Ok(()))],
        vec![account_response(
            ResponseId::Primary,
            account(FIXTURE_USER_ID),
        )],
        vec![shapes_response(ResponseId::Primary, outcomes)],
        Rc::clone(&transcript),
    );

    // Complete expected outcome and final fake state.
    let expected = Snapshot {
        outcome: ReportSnapshot {
            report: expected_report(
                credential_store_pass(),
                credential_schema_pass(),
                connectivity_pass(),
                authentication_pass(),
                fail(
                    "Required private-API shapes",
                    "duplicate diagnostic result, friends (the network request timed out), payment methods (missing diagnostic result), pending requests (missing diagnostic result)",
                    "The private Venmo API may have changed; update the CLI before relying on it.",
                ),
            ),
            healthy: false,
        },
        state: FakeState {
            reader: ReaderOutcome::Present,
            remaining_connectivity: Vec::new(),
            remaining_accounts: Vec::new(),
            remaining_shapes: Vec::new(),
            transcript: expected_calls_through_shapes()?,
        },
    };

    // Execute once.
    let report = diagnose(&reader, ApiAvailability::Ready(&api), TEST_BUILD_INFO).await;
    let observed = snapshot(report, &reader, &api, &transcript);

    assert_eq!(observed, expected);
    Ok(())
}

fn loaded_credential() -> Result<LoadedCredential, FakeCredentialError> {
    let internal = || FakeCredentialError(CredentialFailureKind::Internal);
    Ok(LoadedCredential {
        envelope: CredentialEnvelope::new(
            AccessToken::from_str(FIXTURE_TOKEN).map_err(|_| internal())?,
            DeviceId::from_str(FIXTURE_DEVICE_ID).map_err(|_| internal())?,
            UserId::from_str(FIXTURE_USER_ID).map_err(|_| internal())?,
            Username::from_bare("tester".to_owned()).map_err(|_| internal())?,
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
    RequiredShape::ALL
        .into_iter()
        .map(ShapeProbeOutcome::passed)
        .collect()
}

fn healthy_report() -> DoctorReport {
    expected_report(
        credential_store_pass(),
        credential_schema_pass(),
        connectivity_pass(),
        authentication_pass(),
        shapes_pass(),
    )
}

fn pass(name: &'static str, detail: impl Into<String>) -> DoctorCheck {
    DoctorCheck::new(name, DoctorCheckStatus::Pass, detail, None)
}

fn fail(name: &'static str, detail: impl Into<String>, remediation: &'static str) -> DoctorCheck {
    DoctorCheck::new(name, DoctorCheckStatus::Fail, detail, Some(remediation))
}

fn skipped(name: &'static str, detail: impl Into<String>) -> DoctorCheck {
    DoctorCheck::new(name, DoctorCheckStatus::Skipped, detail, None)
}

fn expected_report(
    credential_store: DoctorCheck,
    credential_schema: DoctorCheck,
    connectivity: DoctorCheck,
    authentication: DoctorCheck,
    required_shapes: DoctorCheck,
) -> DoctorReport {
    DoctorReport::new(vec![
        pass(
            "Build and platform",
            "venmo 1.2.3-test on test-os/test-arch",
        ),
        credential_store,
        credential_schema,
        connectivity,
        authentication,
        required_shapes,
    ])
}

fn credential_store_pass() -> DoctorCheck {
    pass(
        "Credential store",
        "the native credential store is readable",
    )
}

fn credential_schema_pass() -> DoctorCheck {
    pass(
        "Credential presence and schema",
        "a supported credential envelope is present",
    )
}

fn connectivity_pass() -> DoctorCheck {
    pass(
        "Connectivity and TLS",
        "the fixed Venmo API origin returned an HTTPS response",
    )
}

fn initialization_failed() -> DoctorCheck {
    fail(
        "Connectivity and TLS",
        "the HTTPS client could not be initialized",
        "Review the local TLS/runtime installation and retry.",
    )
}

fn authentication_pass() -> DoctorCheck {
    pass(
        "Current-account authentication",
        "the stored credential matches the authenticated account",
    )
}

fn authentication_skipped() -> DoctorCheck {
    skipped(
        "Current-account authentication",
        "authentication depends on an available API client and usable credential",
    )
}

fn shapes_pass() -> DoctorCheck {
    pass(
        "Required private-API shapes",
        "all required read-only response shapes were recognized",
    )
}

fn shapes_skipped() -> DoctorCheck {
    skipped(
        "Required private-API shapes",
        "shape checks depend on successful current-account authentication",
    )
}

const fn api_failure_detail(kind: ApiFailureKind) -> &'static str {
    match kind {
        ApiFailureKind::Network => "the network request failed",
        ApiFailureKind::Timeout => "the network request timed out",
        ApiFailureKind::Rejected => "Venmo rejected the request",
        ApiFailureKind::Contract => "the private API response shape was not recognized",
        ApiFailureKind::AmbiguousWrite => "an unexpected ambiguous-write classification occurred",
        ApiFailureKind::Internal => "an internal API-client failure occurred",
    }
}

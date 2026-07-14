use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error;
use std::future::{Future, ready};
use std::io::{self, Write};
use std::rc::Rc;
use std::str::FromStr;

use super::*;
use crate::adapters::cli::error::ErrorCategory;
use crate::features::doctor::{RequiredShape, ShapeProbeOutcome};
use crate::shared::test_support::Observed;
use crate::shared::{
    AccessToken, Account, ApiFailure, ApiFailureKind, CredentialCapability, CredentialEnvelope,
    CredentialFailureKind, CredentialFormat, CredentialStoreFailure, DeviceId, LoadedCredential,
    UserId, Username,
};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;
type Transcript = Rc<RefCell<Vec<DoctorCall>>>;

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
enum DoctorCall {
    ReadCredential,
    Connectivity,
    CurrentAccount {
        session: SessionCall,
    },
    RequiredShapes {
        session: SessionCall,
        current_user_id: UserId,
    },
    StdoutWrite,
    StdoutFlush,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResponseId {
    Primary,
    UnexpectedSecond,
}

struct ScriptedResponse<T> {
    id: ResponseId,
    result: Result<T, FakeApiError>,
}

struct ResponseQueue<T> {
    responses: RefCell<VecDeque<ScriptedResponse<T>>>,
}

impl<T> ResponseQueue<T> {
    fn new(primary: Result<T, FakeApiError>, unexpected: Result<T, FakeApiError>) -> Self {
        Self {
            responses: RefCell::new(
                vec![
                    ScriptedResponse {
                        id: ResponseId::Primary,
                        result: primary,
                    },
                    ScriptedResponse {
                        id: ResponseId::UnexpectedSecond,
                        result: unexpected,
                    },
                ]
                .into(),
            ),
        }
    }

    fn take(&self) -> Result<T, FakeApiError> {
        self.responses
            .borrow_mut()
            .pop_front()
            .map_or(Err(FakeApiError(ApiFailureKind::Internal)), |response| {
                response.result
            })
    }

    fn remaining(&self) -> Vec<ResponseId> {
        self.responses
            .borrow()
            .iter()
            .map(|response| response.id)
            .collect()
    }
}

struct ShapeResponse {
    id: ResponseId,
    value: Vec<ShapeProbeOutcome>,
}

struct ShapeQueue {
    responses: RefCell<VecDeque<ShapeResponse>>,
}

impl ShapeQueue {
    fn healthy() -> Self {
        Self {
            responses: RefCell::new(
                vec![
                    ShapeResponse {
                        id: ResponseId::Primary,
                        value: required_shape_successes(),
                    },
                    ShapeResponse {
                        id: ResponseId::UnexpectedSecond,
                        value: Vec::new(),
                    },
                ]
                .into(),
            ),
        }
    }

    fn take(&self) -> Vec<ShapeProbeOutcome> {
        self.responses
            .borrow_mut()
            .pop_front()
            .map_or_else(Vec::new, |response| response.value)
    }

    fn remaining(&self) -> Vec<ResponseId> {
        self.responses
            .borrow()
            .iter()
            .map(|response| response.id)
            .collect()
    }
}

struct FakeReader {
    responses: RefCell<VecDeque<ResponseId>>,
    transcript: Transcript,
}

impl FakeReader {
    fn standard(transcript: Transcript) -> Self {
        Self {
            responses: RefCell::new(vec![ResponseId::Primary, ResponseId::UnexpectedSecond].into()),
            transcript,
        }
    }

    fn remaining(&self) -> Vec<ResponseId> {
        self.responses.borrow().iter().copied().collect()
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("synthetic credential failure")]
struct FakeCredentialError;

impl CredentialStoreFailure for FakeCredentialError {
    fn kind(&self) -> CredentialFailureKind {
        CredentialFailureKind::Internal
    }
}

impl CredentialCapability for FakeReader {
    type Error = FakeCredentialError;
}

impl CredentialReader for FakeReader {
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        self.transcript
            .borrow_mut()
            .push(DoctorCall::ReadCredential);
        let response = self.responses.borrow_mut().pop_front();
        if response.is_some() {
            credential().map(Some)
        } else {
            Ok(None)
        }
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("synthetic doctor API failure")]
struct FakeApiError(ApiFailureKind);

impl ApiFailure for FakeApiError {
    fn kind(&self) -> ApiFailureKind {
        self.0
    }
}

struct FakeDoctorApi {
    connectivity: ResponseQueue<()>,
    accounts: ResponseQueue<Account>,
    shapes: ShapeQueue,
    transcript: Transcript,
}

impl FakeDoctorApi {
    fn healthy(transcript: Transcript) -> TestResult<Self> {
        Ok(Self {
            connectivity: ResponseQueue::new(Ok(()), Err(FakeApiError(ApiFailureKind::Internal))),
            accounts: ResponseQueue::new(
                Ok(account()?),
                Err(FakeApiError(ApiFailureKind::Internal)),
            ),
            shapes: ShapeQueue::healthy(),
            transcript,
        })
    }

    fn with_connectivity_failure(mut self) -> Self {
        self.connectivity = ResponseQueue::new(
            Err(FakeApiError(ApiFailureKind::Timeout)),
            Err(FakeApiError(ApiFailureKind::Internal)),
        );
        self
    }

    fn remaining(&self) -> DoctorApiState {
        DoctorApiState {
            connectivity: self.connectivity.remaining(),
            accounts: self.accounts.remaining(),
            shapes: self.shapes.remaining(),
        }
    }
}

impl DoctorApi for FakeDoctorApi {
    type Error = FakeApiError;

    fn connectivity(&self) -> impl Future<Output = Result<(), Self::Error>> + Send + '_ {
        self.transcript.borrow_mut().push(DoctorCall::Connectivity);
        ready(self.connectivity.take())
    }

    fn diagnostic_current_account<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a {
        self.transcript
            .borrow_mut()
            .push(DoctorCall::CurrentAccount {
                session: session_call(access_token, device_id),
            });
        ready(self.accounts.take())
    }

    fn required_shapes<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
    ) -> impl Future<Output = Vec<ShapeProbeOutcome>> + Send + 'a {
        self.transcript
            .borrow_mut()
            .push(DoctorCall::RequiredShapes {
                session: session_call(access_token, device_id),
                current_user_id: current_user_id.clone(),
            });
        ready(self.shapes.take())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct WriterState {
    text: String,
    flush_count: u32,
    fail_write: bool,
}

struct RecordingWriter {
    state: WriterState,
    transcript: Transcript,
    recorded_write: bool,
}

impl RecordingWriter {
    fn new(state: WriterState, transcript: Transcript) -> Self {
        Self {
            state,
            transcript,
            recorded_write: false,
        }
    }
}

impl Write for RecordingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if !self.recorded_write {
            self.recorded_write = true;
            self.transcript.borrow_mut().push(DoctorCall::StdoutWrite);
        }
        if self.state.fail_write {
            return Err(io::Error::other("synthetic doctor output failure"));
        }
        self.state
            .text
            .push_str(std::str::from_utf8(buffer).map_err(io::Error::other)?);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.transcript.borrow_mut().push(DoctorCall::StdoutFlush);
        self.state.flush_count += 1;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ErrorVariant {
    DoctorIncomplete,
    CommandOutput,
    Unexpected,
}

#[derive(Debug, Eq, PartialEq)]
enum ResultSnapshot {
    Success,
    Failure {
        variant: ErrorVariant,
        category: ErrorCategory,
        exit_code: u8,
        message: String,
    },
}

#[derive(Debug, Eq, PartialEq)]
struct DoctorApiState {
    connectivity: Vec<ResponseId>,
    accounts: Vec<ResponseId>,
    shapes: Vec<ResponseId>,
}

#[derive(Debug, Eq, PartialEq)]
struct DoctorState {
    calls: Vec<DoctorCall>,
    remaining_credentials: Vec<ResponseId>,
    api: DoctorApiState,
    stdout: WriterState,
}

#[tokio::test(flavor = "current_thread")]
async fn doctor_handler_writes_exact_healthy_report_after_all_independent_checks() -> TestResult {
    // Setup.
    let build_info = doctor::BuildInfo::new("9.9.9", "testos", "testarch");

    // Immutable initial script/state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::standard(Rc::clone(&transcript));
    let api = FakeDoctorApi::healthy(Rc::clone(&transcript))?;
    let mut stdout = RecordingWriter::new(WriterState::default(), Rc::clone(&transcript));
    let expected = Observed::new(
        ResultSnapshot::Success,
        DoctorState {
            calls: successful_calls()?,
            remaining_credentials: vec![ResponseId::UnexpectedSecond],
            api: consumed_api_state(),
            stdout: writer_state(HEALTHY_OUTPUT),
        },
    );

    // Execute once.
    let result = run_with(
        &reader,
        doctor::ApiAvailability::Ready(&api),
        build_info,
        &mut stdout,
    )
    .await;
    let observed = observation(result, &transcript, &reader, &api, stdout.state);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn doctor_handler_writes_complete_unhealthy_report_before_exact_failure() -> TestResult {
    // Setup.
    let build_info = doctor::BuildInfo::new("9.9.9", "testos", "testarch");

    // Immutable initial script/state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::standard(Rc::clone(&transcript));
    let api = FakeDoctorApi::healthy(Rc::clone(&transcript))?.with_connectivity_failure();
    let mut stdout = RecordingWriter::new(WriterState::default(), Rc::clone(&transcript));
    let expected = Observed::new(
        failure_snapshot(
            ErrorVariant::DoctorIncomplete,
            ErrorCategory::Api,
            "doctor found one or more required failures; review the report",
        ),
        DoctorState {
            calls: successful_calls()?,
            remaining_credentials: vec![ResponseId::UnexpectedSecond],
            api: consumed_api_state(),
            stdout: writer_state(INCOMPLETE_OUTPUT),
        },
    );

    // Execute once.
    let result = run_with(
        &reader,
        doctor::ApiAvailability::Ready(&api),
        build_info,
        &mut stdout,
    )
    .await;
    let observed = observation(result, &transcript, &reader, &api, stdout.state);

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn doctor_output_failure_occurs_after_checks_without_repeating_any_api_call() -> TestResult {
    // Setup.
    let build_info = doctor::BuildInfo::new("9.9.9", "testos", "testarch");

    // Immutable initial script/state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::standard(Rc::clone(&transcript));
    let api = FakeDoctorApi::healthy(Rc::clone(&transcript))?;
    let mut stdout = RecordingWriter::new(
        WriterState {
            fail_write: true,
            ..WriterState::default()
        },
        Rc::clone(&transcript),
    );
    let expected = Observed::new(
        failure_snapshot(
            ErrorVariant::CommandOutput,
            ErrorCategory::Internal,
            "failed to write command output",
        ),
        DoctorState {
            calls: successful_calls()?,
            remaining_credentials: vec![ResponseId::UnexpectedSecond],
            api: consumed_api_state(),
            stdout: WriterState {
                fail_write: true,
                ..WriterState::default()
            },
        },
    );

    // Execute once.
    let result = run_with(
        &reader,
        doctor::ApiAvailability::Ready(&api),
        build_info,
        &mut stdout,
    )
    .await;
    let observed = observation(result, &transcript, &reader, &api, stdout.state);

    assert_eq!(observed, expected);
    Ok(())
}

fn observation(
    result: Result<(), AppError>,
    transcript: &Transcript,
    reader: &FakeReader,
    api: &FakeDoctorApi,
    stdout: WriterState,
) -> Observed<ResultSnapshot, DoctorState> {
    Observed::new(
        snapshot_result(result),
        DoctorState {
            calls: transcript.borrow().clone(),
            remaining_credentials: reader.remaining(),
            api: api.remaining(),
            stdout,
        },
    )
}

fn snapshot_result(result: Result<(), AppError>) -> ResultSnapshot {
    match result {
        Ok(()) => ResultSnapshot::Success,
        Err(error) => ResultSnapshot::Failure {
            variant: match error {
                AppError::DoctorIncomplete => ErrorVariant::DoctorIncomplete,
                AppError::CommandOutput { .. } => ErrorVariant::CommandOutput,
                _ => ErrorVariant::Unexpected,
            },
            category: error.category(),
            exit_code: error.exit_code(),
            message: error.to_string(),
        },
    }
}

fn failure_snapshot(
    variant: ErrorVariant,
    category: ErrorCategory,
    message: &str,
) -> ResultSnapshot {
    ResultSnapshot::Failure {
        variant,
        category,
        exit_code: category.exit_code(),
        message: message.to_owned(),
    }
}

fn successful_calls() -> TestResult<Vec<DoctorCall>> {
    Ok(vec![
        DoctorCall::ReadCredential,
        DoctorCall::Connectivity,
        DoctorCall::CurrentAccount {
            session: fixture_session(),
        },
        DoctorCall::RequiredShapes {
            session: fixture_session(),
            current_user_id: UserId::from_str("1000")?,
        },
        DoctorCall::StdoutWrite,
    ])
}

fn consumed_api_state() -> DoctorApiState {
    DoctorApiState {
        connectivity: vec![ResponseId::UnexpectedSecond],
        accounts: vec![ResponseId::UnexpectedSecond],
        shapes: vec![ResponseId::UnexpectedSecond],
    }
}

fn writer_state(text: &str) -> WriterState {
    WriterState {
        text: text.to_owned(),
        ..WriterState::default()
    }
}

fn required_shape_successes() -> Vec<ShapeProbeOutcome> {
    RequiredShape::ALL
        .into_iter()
        .map(ShapeProbeOutcome::passed)
        .collect()
}

fn session_call(access_token: &AccessToken, device_id: &DeviceId) -> SessionCall {
    SessionCall {
        access_token: if access_token.expose_secret() == "synthetic-doctor-token" {
            SensitiveArgument::Fixture
        } else {
            SensitiveArgument::Other
        },
        device_id: if device_id.as_str() == "synthetic-doctor-device" {
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

fn credential() -> Result<LoadedCredential, FakeCredentialError> {
    Ok(LoadedCredential {
        envelope: CredentialEnvelope::new(
            AccessToken::from_str("synthetic-doctor-token").map_err(|_| FakeCredentialError)?,
            DeviceId::from_str("synthetic-doctor-device").map_err(|_| FakeCredentialError)?,
            UserId::from_str("1000").map_err(|_| FakeCredentialError)?,
            Username::from_bare("owner").map_err(|_| FakeCredentialError)?,
            Some("Synthetic owner".to_owned()),
            time::OffsetDateTime::UNIX_EPOCH,
        ),
        format: CredentialFormat::Version1,
    })
}

fn account() -> TestResult<Account> {
    Ok(Account::new(
        UserId::from_str("1000")?,
        Username::from_bare("owner")?,
        Some("Synthetic owner".to_owned()),
    ))
}

const HEALTHY_OUTPUT: &str = concat!(
    " CHECK                          | STATUS | DETAIL                                                  | REMEDIATION\n",
    "--------------------------------+--------+---------------------------------------------------------+-------------\n",
    " Build and platform             | Pass   | venmo 9.9.9 on testos/testarch                          |\n",
    " Credential store               | Pass   | the native credential store is readable                 |\n",
    " Credential presence and schema | Pass   | a supported credential envelope is present              |\n",
    " Connectivity and TLS           | Pass   | the fixed Venmo API origin returned an HTTPS response   |\n",
    " Current-account authentication | Pass   | the stored credential matches the authenticated account |\n",
    " Required private-API shapes    | Pass   | all required read-only response shapes were recognized  |\n",
);

const INCOMPLETE_OUTPUT: &str = concat!(
    " CHECK                          | STATUS | DETAIL                                                  | REMEDIATION\n",
    "--------------------------------+--------+---------------------------------------------------------+---------------------------------------------------------------------\n",
    " Build and platform             | Pass   | venmo 9.9.9 on testos/testarch                          |\n",
    " Credential store               | Pass   | the native credential store is readable                 |\n",
    " Credential presence and schema | Pass   | a supported credential envelope is present              |\n",
    " Connectivity and TLS           | Fail   | the network request timed out                           | Check DNS, HTTPS connectivity, system time, and TLS trust settings.\n",
    " Current-account authentication | Pass   | the stored credential matches the authenticated account |\n",
    " Required private-API shapes    | Pass   | all required read-only response shapes were recognized  |\n",
);

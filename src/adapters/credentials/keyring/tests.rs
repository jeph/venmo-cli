use std::cell::RefCell;
use std::collections::VecDeque;
use std::error::Error;
use std::io;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::shared::{
    AccessToken, CredentialEnvelope, CredentialFormat, DeviceId, LoadedCredential, UserId, Username,
};

use super::*;

type TestResult = Result<(), Box<dyn Error>>;
const SENSITIVE_MARKER: &str = "scripted-sensitive-backend-detail";
const UNCHANGED_PASSWORD: &str = "unchanged-scripted-password";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScriptedFailure {
    NoEntry,
    NoStorageAccess,
    NoDefaultStore,
    NotSupported,
    Invalid,
    TooLong,
    BadEncoding,
    BadDataFormat,
    Ambiguous,
    Platform,
}

impl ScriptedFailure {
    fn into_keyring_error(self) -> KeyringError {
        match self {
            Self::NoEntry => KeyringError::NoEntry,
            Self::NoStorageAccess => {
                KeyringError::NoStorageAccess(Box::new(ScriptedPlatformFailure))
            }
            Self::NoDefaultStore => KeyringError::NoDefaultStore,
            Self::NotSupported => KeyringError::NotSupportedByStore(SENSITIVE_MARKER.to_owned()),
            Self::Invalid => {
                KeyringError::Invalid(SENSITIVE_MARKER.to_owned(), SENSITIVE_MARKER.to_owned())
            }
            Self::TooLong => KeyringError::TooLong(SENSITIVE_MARKER.to_owned(), 1),
            Self::BadEncoding => KeyringError::BadEncoding(SENSITIVE_MARKER.as_bytes().to_vec()),
            Self::BadDataFormat => KeyringError::BadDataFormat(
                SENSITIVE_MARKER.as_bytes().to_vec(),
                Box::new(ScriptedPlatformFailure),
            ),
            Self::Ambiguous => KeyringError::Ambiguous(Vec::new()),
            Self::Platform => KeyringError::PlatformFailure(Box::new(ScriptedPlatformFailure)),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{SENSITIVE_MARKER}")]
struct ScriptedPlatformFailure;

#[derive(Debug, thiserror::Error)]
#[error("scripted credential backend received an unexpected call")]
struct UnexpectedScriptCall;

enum ScriptedRead {
    Stored,
    Value(Zeroizing<String>),
    Failure(ScriptedFailure),
}

enum ScriptedStep {
    Create(Option<ScriptedFailure>),
    Get(ScriptedRead),
    Set(Option<ScriptedFailure>),
    Delete(Option<ScriptedFailure>),
}

impl ScriptedStep {
    const fn create_ok() -> Self {
        Self::Create(None)
    }

    const fn create_failure(failure: ScriptedFailure) -> Self {
        Self::Create(Some(failure))
    }

    const fn get_stored() -> Self {
        Self::Get(ScriptedRead::Stored)
    }

    fn get_value(value: impl Into<String>) -> Self {
        Self::Get(ScriptedRead::Value(Zeroizing::new(value.into())))
    }

    const fn get_failure(failure: ScriptedFailure) -> Self {
        Self::Get(ScriptedRead::Failure(failure))
    }

    const fn set_ok() -> Self {
        Self::Set(None)
    }

    const fn set_failure(failure: ScriptedFailure) -> Self {
        Self::Set(Some(failure))
    }

    const fn delete_ok() -> Self {
        Self::Delete(None)
    }

    const fn delete_failure(failure: ScriptedFailure) -> Self {
        Self::Delete(Some(failure))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum TranscriptCall {
    CreateEntry { service: String, account: String },
    GetPassword,
    SetPassword,
    DeleteCredential,
}

struct ScriptedState {
    steps: VecDeque<ScriptedStep>,
    transcript: Vec<TranscriptCall>,
    stored_password: Option<Zeroizing<String>>,
    unexpected_call: bool,
}

struct ScriptedBackend {
    state: RefCell<ScriptedState>,
}

impl ScriptedBackend {
    fn new(steps: impl IntoIterator<Item = ScriptedStep>, stored_password: Option<&str>) -> Self {
        Self {
            state: RefCell::new(ScriptedState {
                steps: steps.into_iter().collect(),
                transcript: Vec::new(),
                stored_password: stored_password
                    .map(|password| Zeroizing::new(password.to_owned())),
                unexpected_call: false,
            }),
        }
    }
}

struct ScriptedEntry<'state> {
    state: &'state RefCell<ScriptedState>,
}

impl CredentialEntryFactory for ScriptedBackend {
    type Entry<'entry> = ScriptedEntry<'entry>;

    fn entry(&self, service: &str, account: &str) -> Result<Self::Entry<'_>, KeyringError> {
        let result = {
            let mut state = self.state.borrow_mut();
            state.transcript.push(TranscriptCall::CreateEntry {
                service: service.to_owned(),
                account: account.to_owned(),
            });
            match state.steps.pop_front() {
                Some(ScriptedStep::Create(None)) => Ok(()),
                Some(ScriptedStep::Create(Some(failure))) => Err(failure.into_keyring_error()),
                Some(step) => {
                    state.steps.push_front(step);
                    state.unexpected_call = true;
                    Err(unexpected_script_error())
                }
                None => {
                    state.unexpected_call = true;
                    Err(unexpected_script_error())
                }
            }
        };
        result.map(|()| ScriptedEntry { state: &self.state })
    }
}

impl CredentialEntry for ScriptedEntry<'_> {
    fn get_password(&self) -> Result<String, KeyringError> {
        let mut state = self.state.borrow_mut();
        state.transcript.push(TranscriptCall::GetPassword);
        match state.steps.pop_front() {
            Some(ScriptedStep::Get(ScriptedRead::Stored)) => state
                .stored_password
                .as_ref()
                .map(|password| password.as_str().to_owned())
                .ok_or(KeyringError::NoEntry),
            Some(ScriptedStep::Get(ScriptedRead::Value(value))) => Ok(value.as_str().to_owned()),
            Some(ScriptedStep::Get(ScriptedRead::Failure(failure))) => {
                Err(failure.into_keyring_error())
            }
            Some(step) => {
                state.steps.push_front(step);
                state.unexpected_call = true;
                Err(unexpected_script_error())
            }
            None => {
                state.unexpected_call = true;
                Err(unexpected_script_error())
            }
        }
    }

    fn set_password(&self, password: &str) -> Result<(), KeyringError> {
        let mut state = self.state.borrow_mut();
        state.transcript.push(TranscriptCall::SetPassword);
        match state.steps.pop_front() {
            Some(ScriptedStep::Set(None)) => {
                state.stored_password = Some(Zeroizing::new(password.to_owned()));
                Ok(())
            }
            Some(ScriptedStep::Set(Some(failure))) => Err(failure.into_keyring_error()),
            Some(step) => {
                state.steps.push_front(step);
                state.unexpected_call = true;
                Err(unexpected_script_error())
            }
            None => {
                state.unexpected_call = true;
                Err(unexpected_script_error())
            }
        }
    }

    fn delete_credential(&self) -> Result<(), KeyringError> {
        let mut state = self.state.borrow_mut();
        state.transcript.push(TranscriptCall::DeleteCredential);
        match state.steps.pop_front() {
            Some(ScriptedStep::Delete(None)) => {
                state.stored_password = None;
                Ok(())
            }
            Some(ScriptedStep::Delete(Some(failure))) => Err(failure.into_keyring_error()),
            Some(step) => {
                state.steps.push_front(step);
                state.unexpected_call = true;
                Err(unexpected_script_error())
            }
            None => {
                state.unexpected_call = true;
                Err(unexpected_script_error())
            }
        }
    }
}

fn unexpected_script_error() -> KeyringError {
    KeyringError::PlatformFailure(Box::new(UnexpectedScriptCall))
}

fn scripted_store(
    steps: impl IntoIterator<Item = ScriptedStep>,
    stored_password: Option<&str>,
) -> CredentialStoreAdapter<ScriptedBackend> {
    CredentialStoreAdapter::new(
        VENMO_CREDENTIAL_SERVICE.to_owned(),
        VENMO_CREDENTIAL_ACCOUNT.to_owned(),
        ScriptedBackend::new(steps, stored_password),
    )
}

fn create_entry_call() -> TranscriptCall {
    TranscriptCall::CreateEntry {
        service: VENMO_CREDENTIAL_SERVICE.to_owned(),
        account: VENMO_CREDENTIAL_ACCOUNT.to_owned(),
    }
}

fn fixture() -> Result<CredentialEnvelope, Box<dyn Error>> {
    Ok(CredentialEnvelope::new(
        AccessToken::from_normalized_owned("scripted-token".to_owned())?,
        DeviceId::from_owned("scripted-device".to_owned())?,
        UserId::from_str("123")?,
        Username::from_bare("scripted-user")?,
        Some("Scripted User".to_owned()),
        time::OffsetDateTime::UNIX_EPOCH,
    ))
}

#[derive(Clone, Eq, PartialEq)]
struct RedactedString(Zeroizing<String>);

impl std::fmt::Debug for RedactedString {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[derive(Debug, Eq, PartialEq)]
enum ScriptedReadSnapshot {
    Stored,
    Value(RedactedString),
    Failure(ScriptedFailure),
}

#[derive(Debug, Eq, PartialEq)]
enum ScriptedStepSnapshot {
    Create(Option<ScriptedFailure>),
    Get(ScriptedReadSnapshot),
    Set(Option<ScriptedFailure>),
    Delete(Option<ScriptedFailure>),
}

impl From<&ScriptedStep> for ScriptedStepSnapshot {
    fn from(step: &ScriptedStep) -> Self {
        match step {
            ScriptedStep::Create(failure) => Self::Create(*failure),
            ScriptedStep::Get(ScriptedRead::Stored) => Self::Get(ScriptedReadSnapshot::Stored),
            ScriptedStep::Get(ScriptedRead::Value(value)) => {
                Self::Get(ScriptedReadSnapshot::Value(RedactedString(value.clone())))
            }
            ScriptedStep::Get(ScriptedRead::Failure(failure)) => {
                Self::Get(ScriptedReadSnapshot::Failure(*failure))
            }
            ScriptedStep::Set(failure) => Self::Set(*failure),
            ScriptedStep::Delete(failure) => Self::Delete(*failure),
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum SecretSnapshot {
    FixtureToken,
    FixtureDevice,
    Other,
}

impl std::fmt::Debug for SecretSnapshot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[derive(Debug, Eq, PartialEq)]
struct CredentialSnapshot {
    access_token: SecretSnapshot,
    device_id: SecretSnapshot,
    user_id: String,
    username: String,
    display_name: Option<String>,
    saved_at: time::OffsetDateTime,
    format: CredentialFormat,
}

impl CredentialSnapshot {
    fn fixture() -> Self {
        Self {
            access_token: SecretSnapshot::FixtureToken,
            device_id: SecretSnapshot::FixtureDevice,
            user_id: "123".to_owned(),
            username: "scripted-user".to_owned(),
            display_name: Some("Scripted User".to_owned()),
            saved_at: time::OffsetDateTime::UNIX_EPOCH,
            format: CredentialFormat::Version1,
        }
    }

    fn from_loaded(loaded: LoadedCredential) -> Self {
        let LoadedCredential { envelope, format } = loaded;
        Self {
            access_token: if envelope.access_token().expose_secret() == "scripted-token" {
                SecretSnapshot::FixtureToken
            } else {
                SecretSnapshot::Other
            },
            device_id: if envelope.device_id().as_str() == "scripted-device" {
                SecretSnapshot::FixtureDevice
            } else {
                SecretSnapshot::Other
            },
            user_id: envelope.user_id().as_str().to_owned(),
            username: envelope.username().as_str().to_owned(),
            display_name: envelope.display_name().map(str::to_owned),
            saved_at: envelope.saved_at(),
            format,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
enum StoredDataSnapshot {
    Missing,
    Unchanged,
    Credential(CredentialSnapshot),
    Other(RedactedString),
}

fn stored_data_snapshot(password: Option<&Zeroizing<String>>) -> StoredDataSnapshot {
    let Some(password) = password else {
        return StoredDataSnapshot::Missing;
    };
    if password.as_str() == UNCHANGED_PASSWORD {
        return StoredDataSnapshot::Unchanged;
    }
    match codec::decode(password.as_str()) {
        Ok(loaded) => StoredDataSnapshot::Credential(CredentialSnapshot::from_loaded(loaded)),
        Err(_) => StoredDataSnapshot::Other(RedactedString(password.clone())),
    }
}

#[derive(Debug, Eq, PartialEq)]
struct BackendStateSnapshot {
    transcript: Vec<TranscriptCall>,
    remaining_steps: Vec<ScriptedStepSnapshot>,
    stored_data: StoredDataSnapshot,
    unexpected_call: bool,
}

fn backend_snapshot(store: &CredentialStoreAdapter<ScriptedBackend>) -> BackendStateSnapshot {
    let state = store.entry_factory.state.borrow();
    BackendStateSnapshot {
        transcript: state.transcript.clone(),
        remaining_steps: state.steps.iter().map(ScriptedStepSnapshot::from).collect(),
        stored_data: stored_data_snapshot(state.stored_password.as_ref()),
        unexpected_call: state.unexpected_call,
    }
}

#[derive(Debug, Eq, PartialEq)]
struct CredentialErrorSnapshot {
    kind: CredentialStoreErrorKind,
    rendered: String,
}

impl CredentialErrorSnapshot {
    fn from_error(error: CredentialStoreError) -> Self {
        Self {
            kind: error.kind(),
            rendered: error.to_string(),
        }
    }

    fn backend(kind: CredentialStoreErrorKind, operation: &str) -> Self {
        let rendered = match kind {
            CredentialStoreErrorKind::Unavailable => format!(
                "cannot {operation} credentials because the OS credential store is unavailable"
            ),
            CredentialStoreErrorKind::Inaccessible => format!(
                "cannot {operation} credentials because the OS credential store is inaccessible"
            ),
            CredentialStoreErrorKind::Corrupt => {
                format!("cannot {operation} credentials because the OS credential entry is corrupt")
            }
            CredentialStoreErrorKind::Invalid => format!(
                "cannot {operation} credentials because a credential-store value is invalid"
            ),
            CredentialStoreErrorKind::TooLarge => format!(
                "cannot {operation} credentials because a credential-store limit was exceeded"
            ),
            CredentialStoreErrorKind::Ambiguous => format!(
                "cannot {operation} credentials because multiple OS credential entries matched"
            ),
            CredentialStoreErrorKind::Platform => format!(
                "the OS credential store failed while attempting to {operation} credentials"
            ),
            CredentialStoreErrorKind::UnsupportedVersion | CredentialStoreErrorKind::Internal => {
                "the stored credential envelope is unusable".to_owned()
            }
        };
        Self { kind, rendered }
    }

    fn codec(kind: CredentialStoreErrorKind) -> Self {
        Self {
            kind,
            rendered: "the stored credential envelope is unusable".to_owned(),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
enum StoreSuccessSnapshot {
    Read(Option<CredentialSnapshot>),
    Saved,
    Deleted(CredentialDeleteOutcome),
}

type StoreResultSnapshot = Result<StoreSuccessSnapshot, CredentialErrorSnapshot>;

#[derive(Debug, Eq, PartialEq)]
struct StoreObservation {
    result: StoreResultSnapshot,
    backend: BackendStateSnapshot,
}

impl StoreObservation {
    fn expected(result: StoreResultSnapshot, backend: BackendStateSnapshot) -> Self {
        Self { result, backend }
    }

    fn observed(
        result: Result<StoreSuccessSnapshot, CredentialStoreError>,
        store: &CredentialStoreAdapter<ScriptedBackend>,
    ) -> Self {
        Self {
            result: result.map_err(CredentialErrorSnapshot::from_error),
            backend: backend_snapshot(store),
        }
    }
}

fn expected_backend(
    transcript: Vec<TranscriptCall>,
    stored_data: StoredDataSnapshot,
) -> BackendStateSnapshot {
    BackendStateSnapshot {
        transcript,
        remaining_steps: Vec::new(),
        stored_data,
        unexpected_call: false,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StoreOperation {
    Load,
    Save,
    Delete,
}

impl StoreOperation {
    const ALL: [Self; 3] = [Self::Load, Self::Save, Self::Delete];

    const fn name(self) -> &'static str {
        match self {
            Self::Load => "read",
            Self::Save => "save",
            Self::Delete => "delete",
        }
    }

    fn invoke(
        self,
        store: &CredentialStoreAdapter<ScriptedBackend>,
        credential: &CredentialEnvelope,
    ) -> Result<StoreSuccessSnapshot, CredentialStoreError> {
        match self {
            Self::Load => store.read_credential().map(|loaded| {
                StoreSuccessSnapshot::Read(loaded.map(CredentialSnapshot::from_loaded))
            }),
            Self::Save => store
                .save_credential(credential)
                .map(|()| StoreSuccessSnapshot::Saved),
            Self::Delete => store.delete_credential().map(StoreSuccessSnapshot::Deleted),
        }
    }
}

fn entry_operation_failures() -> [(ScriptedFailure, CredentialStoreErrorKind); 7] {
    [
        (
            ScriptedFailure::NoStorageAccess,
            CredentialStoreErrorKind::Inaccessible,
        ),
        (ScriptedFailure::Invalid, CredentialStoreErrorKind::Invalid),
        (ScriptedFailure::TooLong, CredentialStoreErrorKind::TooLarge),
        (
            ScriptedFailure::BadEncoding,
            CredentialStoreErrorKind::Corrupt,
        ),
        (
            ScriptedFailure::BadDataFormat,
            CredentialStoreErrorKind::Corrupt,
        ),
        (
            ScriptedFailure::Ambiguous,
            CredentialStoreErrorKind::Ambiguous,
        ),
        (
            ScriptedFailure::Platform,
            CredentialStoreErrorKind::Platform,
        ),
    ]
}

#[test]
fn native_store_keeps_the_fixed_location_without_opening_an_entry() {
    let store = NativeCredentialStore::new();

    assert_eq!(
        (
            store.adapter.service.as_str(),
            store.adapter.account.as_str()
        ),
        (VENMO_CREDENTIAL_SERVICE, VENMO_CREDENTIAL_ACCOUNT)
    );
}

#[test]
fn entry_creation_failures_are_classified_for_every_operation() -> TestResult {
    // Setup.
    let credential = fixture()?;
    for operation in StoreOperation::ALL {
        for (failure, expected_kind) in [
            (
                ScriptedFailure::NoDefaultStore,
                CredentialStoreErrorKind::Platform,
            ),
            (
                ScriptedFailure::NotSupported,
                CredentialStoreErrorKind::Platform,
            ),
            (
                ScriptedFailure::NoStorageAccess,
                CredentialStoreErrorKind::Inaccessible,
            ),
            (
                ScriptedFailure::Platform,
                CredentialStoreErrorKind::Platform,
            ),
        ] {
            // Immutable initial script/state.
            let script = [ScriptedStep::create_failure(failure)];
            let store = scripted_store(script, Some(UNCHANGED_PASSWORD));

            // Complete expected observation.
            let expected = StoreObservation::expected(
                Err(CredentialErrorSnapshot::backend(
                    expected_kind,
                    operation.name(),
                )),
                expected_backend(vec![create_entry_call()], StoredDataSnapshot::Unchanged),
            );

            // Execute once.
            let result = operation.invoke(&store, &credential);
            let observed = StoreObservation::observed(result, &store);

            assert_eq!(observed, expected);
            assert!(!format!("{observed:?}").contains(SENSITIVE_MARKER));
        }
    }
    Ok(())
}

#[test]
fn missing_entries_are_normal_load_and_delete_outcomes() -> TestResult {
    // Setup.
    let credential = fixture()?;
    for (operation, script, expected_result, terminal_call) in [
        (
            StoreOperation::Load,
            vec![
                ScriptedStep::create_ok(),
                ScriptedStep::get_failure(ScriptedFailure::NoEntry),
            ],
            StoreSuccessSnapshot::Read(None),
            TranscriptCall::GetPassword,
        ),
        (
            StoreOperation::Delete,
            vec![
                ScriptedStep::create_ok(),
                ScriptedStep::delete_failure(ScriptedFailure::NoEntry),
            ],
            StoreSuccessSnapshot::Deleted(CredentialDeleteOutcome::Missing),
            TranscriptCall::DeleteCredential,
        ),
    ] {
        // Immutable initial script/state.
        let store = scripted_store(script, None);

        // Complete expected observation.
        let expected = StoreObservation::expected(
            Ok(expected_result),
            expected_backend(
                vec![create_entry_call(), terminal_call],
                StoredDataSnapshot::Missing,
            ),
        );

        // Execute once.
        let result = operation.invoke(&store, &credential);
        let observed = StoreObservation::observed(result, &store);

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[test]
fn get_failures_cover_the_typed_keyring_matrix() -> TestResult {
    // Setup.
    let credential = fixture()?;
    for (failure, expected_kind) in entry_operation_failures() {
        // Immutable initial script/state.
        let script = [
            ScriptedStep::create_ok(),
            ScriptedStep::get_failure(failure),
        ];
        let store = scripted_store(script, Some(UNCHANGED_PASSWORD));

        // Complete expected observation.
        let expected = StoreObservation::expected(
            Err(CredentialErrorSnapshot::backend(expected_kind, "read")),
            expected_backend(
                vec![create_entry_call(), TranscriptCall::GetPassword],
                StoredDataSnapshot::Unchanged,
            ),
        );

        // Execute once.
        let result = StoreOperation::Load.invoke(&store, &credential);
        let observed = StoreObservation::observed(result, &store);

        assert_eq!(observed, expected);
        assert!(!format!("{observed:?}").contains(SENSITIVE_MARKER));
    }
    Ok(())
}

#[test]
fn set_failures_cover_the_typed_keyring_matrix_without_replacing_data() -> TestResult {
    // Setup.
    let credential = fixture()?;
    for (failure, expected_kind) in entry_operation_failures() {
        // Immutable initial script/state.
        let script = [
            ScriptedStep::create_ok(),
            ScriptedStep::set_failure(failure),
        ];
        let store = scripted_store(script, Some(UNCHANGED_PASSWORD));

        // Complete expected observation.
        let expected = StoreObservation::expected(
            Err(CredentialErrorSnapshot::backend(expected_kind, "save")),
            expected_backend(
                vec![create_entry_call(), TranscriptCall::SetPassword],
                StoredDataSnapshot::Unchanged,
            ),
        );

        // Execute once.
        let result = StoreOperation::Save.invoke(&store, &credential);
        let observed = StoreObservation::observed(result, &store);

        assert_eq!(observed, expected);
        assert!(!format!("{observed:?}").contains(SENSITIVE_MARKER));
    }
    Ok(())
}

#[test]
fn delete_failures_cover_the_typed_keyring_matrix_without_deleting_data() -> TestResult {
    // Setup.
    let credential = fixture()?;
    for (failure, expected_kind) in entry_operation_failures() {
        // Immutable initial script/state.
        let script = [
            ScriptedStep::create_ok(),
            ScriptedStep::delete_failure(failure),
        ];
        let store = scripted_store(script, Some(UNCHANGED_PASSWORD));

        // Complete expected observation.
        let expected = StoreObservation::expected(
            Err(CredentialErrorSnapshot::backend(expected_kind, "delete")),
            expected_backend(
                vec![create_entry_call(), TranscriptCall::DeleteCredential],
                StoredDataSnapshot::Unchanged,
            ),
        );

        // Execute once.
        let result = StoreOperation::Delete.invoke(&store, &credential);
        let observed = StoreObservation::observed(result, &store);

        assert_eq!(observed, expected);
        assert!(!format!("{observed:?}").contains(SENSITIVE_MARKER));
    }
    Ok(())
}

#[test]
fn credential_codec_failures_are_classified_after_exactly_one_read() -> TestResult {
    // Setup.
    let malformed = format!("{{\"accessToken\":\"{SENSITIVE_MARKER}\"");
    let invalid = r#"{"schemaVersion":1,"accessToken":"","deviceId":"device","userId":"123","username":"alice","displayName":null,"createdAt":"1970-01-01T00:00:00Z"}"#.to_owned();
    let unsupported = format!(
        r#"{{"schemaVersion":2,"accessToken":"{SENSITIVE_MARKER}","deviceId":"device","userId":"123","username":"alice","displayName":null,"createdAt":"1970-01-01T00:00:00Z"}}"#
    );
    let oversized = "x".repeat(128 * 1024 + 1);
    let credential = fixture()?;
    for (raw, expected_kind) in [
        (malformed, CredentialStoreErrorKind::Corrupt),
        (invalid, CredentialStoreErrorKind::Invalid),
        (unsupported, CredentialStoreErrorKind::UnsupportedVersion),
        (oversized, CredentialStoreErrorKind::TooLarge),
    ] {
        // Immutable initial script/state.
        let script = [ScriptedStep::create_ok(), ScriptedStep::get_value(raw)];
        let store = scripted_store(script, None);

        // Complete expected observation.
        let expected = StoreObservation::expected(
            Err(CredentialErrorSnapshot::codec(expected_kind)),
            expected_backend(
                vec![create_entry_call(), TranscriptCall::GetPassword],
                StoredDataSnapshot::Missing,
            ),
        );

        // Execute once.
        let result = StoreOperation::Load.invoke(&store, &credential);
        let observed = StoreObservation::observed(result, &store);

        assert_eq!(observed, expected);
        assert!(!format!("{observed:?}").contains(SENSITIVE_MARKER));
    }
    Ok(())
}

#[test]
fn oversized_encoded_credentials_fail_before_opening_or_mutating_the_keyring() -> TestResult {
    // Setup.
    let credential = CredentialEnvelope::new(
        AccessToken::from_normalized_owned("scripted-token".to_owned())?,
        DeviceId::from_owned("scripted-device".to_owned())?,
        UserId::from_str("123")?,
        Username::from_bare("scripted-user")?,
        Some("x".repeat(128 * 1024)),
        time::OffsetDateTime::UNIX_EPOCH,
    );

    // Immutable initial script/state.
    let script: [ScriptedStep; 0] = [];
    let store = scripted_store(script, Some(UNCHANGED_PASSWORD));

    // Complete expected observation.
    let expected = StoreObservation::expected(
        Err(CredentialErrorSnapshot::codec(
            CredentialStoreErrorKind::TooLarge,
        )),
        expected_backend(Vec::new(), StoredDataSnapshot::Unchanged),
    );

    // Execute once.
    let result = StoreOperation::Save.invoke(&store, &credential);
    let observed = StoreObservation::observed(result, &store);

    assert_eq!(observed, expected);
    let rendered = format!("{observed:?}");
    assert!(!rendered.contains("scripted-token"));
    assert!(!rendered.contains("scripted-device"));
    Ok(())
}

#[test]
fn successful_operations_each_execute_and_observe_one_behavior() -> TestResult {
    // Setup.
    let credential = fixture()?;
    let encoded = codec::encode(&credential)?;
    for (operation, script, stored_password) in [
        (
            StoreOperation::Save,
            vec![ScriptedStep::create_ok(), ScriptedStep::set_ok()],
            None,
        ),
        (
            StoreOperation::Load,
            vec![ScriptedStep::create_ok(), ScriptedStep::get_stored()],
            Some(encoded.as_str()),
        ),
        (
            StoreOperation::Delete,
            vec![ScriptedStep::create_ok(), ScriptedStep::delete_ok()],
            Some(encoded.as_str()),
        ),
    ] {
        // Immutable initial script/state.
        let store = scripted_store(script, stored_password);

        // Complete expected observation.
        let (expected_result, terminal_call, stored_data) = match operation {
            StoreOperation::Save => (
                StoreSuccessSnapshot::Saved,
                TranscriptCall::SetPassword,
                StoredDataSnapshot::Credential(CredentialSnapshot::fixture()),
            ),
            StoreOperation::Load => (
                StoreSuccessSnapshot::Read(Some(CredentialSnapshot::fixture())),
                TranscriptCall::GetPassword,
                StoredDataSnapshot::Credential(CredentialSnapshot::fixture()),
            ),
            StoreOperation::Delete => (
                StoreSuccessSnapshot::Deleted(CredentialDeleteOutcome::Deleted),
                TranscriptCall::DeleteCredential,
                StoredDataSnapshot::Missing,
            ),
        };
        let expected = StoreObservation::expected(
            Ok(expected_result),
            expected_backend(vec![create_entry_call(), terminal_call], stored_data),
        );

        // Execute once.
        let result = operation.invoke(&store, &credential);
        let observed = StoreObservation::observed(result, &store);

        assert_eq!(observed, expected);
        let rendered = format!("{observed:?}");
        assert!(!rendered.contains("scripted-token"));
        assert!(!rendered.contains("scripted-device"));
    }

    Ok(())
}

#[test]
#[ignore = "manually exercises the native OS credential store"]
fn native_keyring_round_trip_uses_an_isolated_entry() -> TestResult {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let service = format!("venmo-cli-native-smoke-{}-{nonce}", std::process::id());
    let store = NativeCredentialStore::for_test_location(service, "synthetic".to_owned());
    let credential = CredentialEnvelope::new(
        AccessToken::from_normalized_owned("synthetic-token".to_owned())?,
        DeviceId::from_owned("synthetic-device".to_owned())?,
        UserId::from_str("123")?,
        Username::from_bare("synthetic-user")?,
        Some("Synthetic User".to_owned()),
        time::OffsetDateTime::UNIX_EPOCH,
    );

    let exercise_result = exercise_native_round_trip(&store, &credential);
    let cleanup_result = store.delete_credential();

    exercise_result?;
    match cleanup_result? {
        CredentialDeleteOutcome::Deleted => {}
        CredentialDeleteOutcome::Missing => {
            return Err(
                io::Error::other("native keyring test entry disappeared before cleanup").into(),
            );
        }
    }
    if store.read_credential()?.is_some() {
        return Err(io::Error::other("native keyring test entry remained after deletion").into());
    }
    Ok(())
}

fn exercise_native_round_trip(
    store: &NativeCredentialStore,
    credential: &CredentialEnvelope,
) -> TestResult {
    store.save_credential(credential)?;
    let Some(loaded) = store.read_credential()? else {
        return Err(io::Error::other("native keyring test entry was not readable").into());
    };
    if loaded.format != CredentialFormat::Version1 {
        return Err(io::Error::other("native keyring test entry used the wrong format").into());
    }
    if loaded.envelope.access_token().expose_secret() != "synthetic-token" {
        return Err(io::Error::other("native keyring returned a different secret").into());
    }
    if loaded.envelope.user_id().as_str() != "123" {
        return Err(io::Error::other("native keyring returned different metadata").into());
    }
    Ok(())
}

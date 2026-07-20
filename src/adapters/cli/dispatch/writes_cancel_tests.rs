use std::cell::RefCell;
use std::error::Error;
use std::future::{Future, pending, ready};
use std::rc::Rc;
use std::str::FromStr;

use time::OffsetDateTime;

use super::run_cancel_with;
use crate::adapters::cli::args::CancelArgs;
use crate::adapters::cli::output::TimestampFormatter;
use crate::features::auth::{CurrentAccountApi, PromptAvailability, PromptError};
use crate::features::payments::DefaultNoConfirmation;
use crate::features::people::User;
use crate::features::requests::{
    CancelRequestPlan, CancelledRequest, RequestAction, RequestCancellationApi, RequestDirection,
    RequestId, RequestLookupApi, RequestRecord, RequestStatus,
};
use crate::shared::{
    AccessToken, Account, ApiFailure, ApiFailureKind, CredentialCapability, CredentialEnvelope,
    CredentialFailureKind, CredentialFormat, CredentialReader, CredentialStoreFailure, DeviceId,
    LoadedCredential, Money, UserId, Username,
};

type TestResult = Result<(), Box<dyn Error>>;
type Transcript = Rc<RefCell<Vec<Call>>>;

#[derive(Clone, Debug, Eq, PartialEq)]
enum Call {
    Credential,
    Account,
    Request(RequestId),
    PromptAvailability,
    Confirm(String),
    Cancel(RequestId),
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("synthetic credential error")]
struct FakeCredentialError;

impl CredentialStoreFailure for FakeCredentialError {
    fn kind(&self) -> CredentialFailureKind {
        CredentialFailureKind::Unavailable
    }
}

struct FakeReader {
    transcript: Transcript,
}

impl CredentialCapability for FakeReader {
    type Error = FakeCredentialError;
}

impl CredentialReader for FakeReader {
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        self.transcript.borrow_mut().push(Call::Credential);
        Ok(Some(LoadedCredential {
            envelope: CredentialEnvelope::new(
                AccessToken::from_str("synthetic-secret-cancel-token")
                    .map_err(|_| FakeCredentialError)?,
                DeviceId::from_str("synthetic-secret-cancel-device")
                    .map_err(|_| FakeCredentialError)?,
                UserId::from_str("123").map_err(|_| FakeCredentialError)?,
                Username::from_bare("owner").map_err(|_| FakeCredentialError)?,
                Some("Synthetic owner".to_owned()),
                OffsetDateTime::UNIX_EPOCH,
            ),
            format: CredentialFormat::Version1,
        }))
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("synthetic API error")]
struct FakeApiError;

impl ApiFailure for FakeApiError {
    fn kind(&self) -> ApiFailureKind {
        ApiFailureKind::Contract
    }
}

struct FakeApi {
    transcript: Transcript,
    request: RequestRecord,
}

impl CurrentAccountApi for FakeApi {
    type Error = FakeApiError;

    fn current_account<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::Account);
        let result = (|| {
            Ok(Account::new(
                UserId::from_str("123").map_err(|_| FakeApiError)?,
                Username::from_bare("owner").map_err(|_| FakeApiError)?,
                Some("Synthetic owner".to_owned()),
            ))
        })();
        ready(result)
    }
}

impl RequestLookupApi for FakeApi {
    type Error = FakeApiError;

    fn request_by_id<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        _current_user_id: &'a UserId,
        request_id: &'a RequestId,
    ) -> impl Future<Output = Result<RequestRecord, Self::Error>> + Send + 'a {
        self.transcript
            .borrow_mut()
            .push(Call::Request(request_id.clone()));
        ready(Ok(self.request.clone()))
    }
}

impl RequestCancellationApi for FakeApi {
    type Error = FakeApiError;

    fn cancel_request<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        plan: &'a CancelRequestPlan,
    ) -> impl Future<Output = Result<CancelledRequest, Self::Error>> + Send + 'a {
        self.transcript
            .borrow_mut()
            .push(Call::Cancel(plan.request().id().clone()));
        let result = RequestStatus::from_str("cancelled")
            .map(|status| CancelledRequest::new(plan.request().id().clone(), status))
            .map_err(|_| FakeApiError);
        ready(result)
    }
}

struct FakePrompt {
    transcript: Transcript,
}

impl PromptAvailability for FakePrompt {
    fn can_prompt(&self) -> bool {
        self.transcript.borrow_mut().push(Call::PromptAvailability);
        true
    }
}

impl DefaultNoConfirmation for FakePrompt {
    fn confirm_default_no(&self, prompt: &str) -> Result<bool, PromptError> {
        self.transcript
            .borrow_mut()
            .push(Call::Confirm(prompt.to_owned()));
        Ok(true)
    }
}

#[tokio::test(flavor = "current_thread")]
async fn cancel_handler_orders_preflight_details_confirmation_one_write_and_result() -> TestResult {
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let request_id = RequestId::from_str("request-1")?;
    let reader = FakeReader {
        transcript: Rc::clone(&transcript),
    };
    let api = FakeApi {
        transcript: Rc::clone(&transcript),
        request: outgoing_request()?,
    };
    let prompt = FakePrompt {
        transcript: Rc::clone(&transcript),
    };
    let timestamps = TimestampFormatter::for_time_zone(jiff::tz::TimeZone::UTC);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    run_cancel_with(
        CancelArgs {
            request_id: request_id.clone(),
            yes: false,
        },
        &reader,
        &api,
        &prompt,
        &timestamps,
        &mut stdout,
        &mut stderr,
        || Ok(pending()),
    )
    .await?;

    assert_eq!(
        transcript.borrow().as_slice(),
        [
            Call::Credential,
            Call::Account,
            Call::Request(request_id.clone()),
            Call::PromptAvailability,
            Call::Confirm("Cancel this outgoing request?".to_owned()),
            Call::Cancel(request_id),
        ]
    );
    let stderr = String::from_utf8(stderr)?;
    let stdout = String::from_utf8(stdout)?;
    assert!(stderr.contains("Outgoing request cancellation details:"));
    assert!(stderr.contains("Current request status: held"));
    assert!(stdout.contains("Server status: cancelled"));
    assert!(stdout.contains("Money sent: no"));
    for secret in [
        "synthetic-secret-cancel-token",
        "synthetic-secret-cancel-device",
    ] {
        assert!(!stderr.contains(secret));
        assert!(!stdout.contains(secret));
    }
    Ok(())
}

fn outgoing_request() -> Result<RequestRecord, Box<dyn Error>> {
    Ok(RequestRecord::new(
        RequestId::from_str("request-1")?,
        RequestAction::Charge,
        RequestDirection::Outgoing,
        User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("recipient")?),
            Some("Synthetic recipient".to_owned()),
        ),
        Money::from_cents(1)?,
        Some("Synthetic request".to_owned()),
        Some(OffsetDateTime::UNIX_EPOCH),
        RequestStatus::from_str("held")?,
    )
    .with_audience(Some("private".to_owned())))
}

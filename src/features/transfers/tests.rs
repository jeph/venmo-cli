use std::cell::RefCell;
use std::error::Error;
use std::fmt;
use std::rc::Rc;
use std::str::FromStr;

use time::OffsetDateTime;

use super::model::TransferId;
use super::options;
use super::out::{self, TransferOutError};
use super::*;
use crate::features::auth::{CurrentAccountApi, PromptAvailability, PromptError};
use crate::features::payments::DefaultNoConfirmation;
use crate::features::wallet::{Balance, BalanceApi, SignedUsdAmount};
use crate::shared::{
    AccessToken, Account, ApiFailure, ApiFailureKind, CredentialCapability, CredentialEnvelope,
    CredentialFailureKind, CredentialFormat, CredentialReader, CredentialStoreFailure, DeviceId,
    LoadedCredential, Money, UserId, Username,
};

type TestResult = Result<(), Box<dyn Error>>;
type Transcript = Rc<RefCell<Vec<Call>>>;

#[derive(Clone, Debug, Eq, PartialEq)]
enum Call {
    ReadCredential,
    CurrentAccount,
    Balance,
    TransferOptions,
    PromptAvailability,
    Confirm {
        prompt: String,
    },
    Create {
        amount_cents: u64,
        speed: TransferSpeed,
        destination_id: String,
    },
}

struct FakeStore {
    transcript: Transcript,
    user_id: &'static str,
}

impl CredentialCapability for FakeStore {
    type Error = SyntheticCredentialError;
}

impl CredentialReader for FakeStore {
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        self.transcript.borrow_mut().push(Call::ReadCredential);
        Ok(Some(LoadedCredential {
            envelope: credential(self.user_id).map_err(|_| SyntheticCredentialError)?,
            format: CredentialFormat::Version1,
        }))
    }
}

struct FakeApi {
    transcript: Transcript,
    account_user_id: &'static str,
    available_cents: i64,
    options: TransferOptions,
}

impl CurrentAccountApi for FakeApi {
    type Error = SyntheticApiError;

    fn current_account<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::CurrentAccount);
        std::future::ready(account(self.account_user_id).map_err(|_| SyntheticApiError))
    }
}

impl BalanceApi for FakeApi {
    type Error = SyntheticApiError;

    fn balance<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Balance, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::Balance);
        std::future::ready(Ok(Balance::new(
            SignedUsdAmount::from_cents(self.available_cents),
            SignedUsdAmount::from_cents(0),
        )))
    }
}

impl TransferOptionsApi for FakeApi {
    type Error = SyntheticApiError;

    fn transfer_options<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<TransferOptions, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::TransferOptions);
        std::future::ready(Ok(self.options.clone()))
    }
}

impl TransferCreationApi for FakeApi {
    type Error = SyntheticApiError;

    fn create_transfer<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        plan: &'a TransferOutPlan,
    ) -> impl Future<Output = Result<CreatedTransfer, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::Create {
            amount_cents: plan.amount().cents(),
            speed: plan.speed(),
            destination_id: plan.destination().id().as_str().to_owned(),
        });
        std::future::ready(
            TransferId::from_str("transfer-1")
                .map(|id| {
                    CreatedTransfer::new(
                        id,
                        "pending".to_owned(),
                        OffsetDateTime::UNIX_EPOCH,
                        plan.amount(),
                        0,
                    )
                })
                .map_err(|_| SyntheticApiError),
        )
    }
}

struct FakePrompt {
    transcript: Transcript,
    answer: bool,
}

impl PromptAvailability for FakePrompt {
    fn can_prompt(&self) -> bool {
        self.transcript.borrow_mut().push(Call::PromptAvailability);
        true
    }
}

impl DefaultNoConfirmation for FakePrompt {
    fn confirm_default_no(&self, prompt: &str) -> Result<bool, PromptError> {
        self.transcript.borrow_mut().push(Call::Confirm {
            prompt: prompt.to_owned(),
        });
        Ok(self.answer)
    }
}

#[derive(Debug, Eq, PartialEq)]
struct SuccessObservation {
    transfer_id: String,
    amount_cents: u64,
    destination_id: String,
    transcript: Vec<Call>,
}

#[tokio::test(flavor = "current_thread")]
async fn standard_out_preserves_preflight_confirmation_and_one_write_order() -> TestResult {
    // Setup and immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let store = FakeStore {
        transcript: Rc::clone(&transcript),
        user_id: "123",
    };
    let api = FakeApi {
        transcript: Rc::clone(&transcript),
        account_user_id: "123",
        available_cents: 5_000,
        options: options_with_destinations(vec![instrument("bank-default", true)?]),
    };
    let prompt = FakePrompt {
        transcript: Rc::clone(&transcript),
        answer: true,
    };

    // Complete expected observation.
    let expected = SuccessObservation {
        transfer_id: "transfer-1".to_owned(),
        amount_cents: 1_234,
        destination_id: "bank-default".to_owned(),
        transcript: vec![
            Call::ReadCredential,
            Call::CurrentAccount,
            Call::Balance,
            Call::TransferOptions,
            Call::PromptAvailability,
            Call::Confirm {
                prompt: "Transfer this balance to the selected bank?".to_owned(),
            },
            Call::Create {
                amount_cents: 1_234,
                speed: TransferSpeed::Standard,
                destination_id: "bank-default".to_owned(),
            },
        ],
    };

    // Execute once.
    let prepared = out::prepare(
        &store,
        &api,
        Money::from_cents(1_234)?,
        TransferSpeed::Standard,
    )
    .await?;
    let authorized = out::authorize(&prompt, prepared, false)?;
    let result = out::execute(&api, authorized).await?;
    let observed = SuccessObservation {
        transfer_id: result.created().id().as_str().to_owned(),
        amount_cents: result.plan().amount().cents(),
        destination_id: result.plan().destination().id().as_str().to_owned(),
        transcript: transcript.borrow().clone(),
    };

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn standard_out_stops_before_later_reads_or_write_on_preflight_failures() -> TestResult {
    for (account_user_id, available_cents, destinations, expected_error, expected_calls) in [
        (
            "999",
            5_000,
            vec![instrument("bank-default", true)?],
            "account",
            vec![Call::ReadCredential, Call::CurrentAccount],
        ),
        (
            "123",
            50,
            vec![instrument("bank-default", true)?],
            "balance",
            vec![Call::ReadCredential, Call::CurrentAccount, Call::Balance],
        ),
        (
            "123",
            5_000,
            vec![instrument("one", false)?, instrument("two", false)?],
            "selection",
            vec![
                Call::ReadCredential,
                Call::CurrentAccount,
                Call::Balance,
                Call::TransferOptions,
            ],
        ),
    ] {
        // Setup and immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let store = FakeStore {
            transcript: Rc::clone(&transcript),
            user_id: "123",
        };
        let api = FakeApi {
            transcript: Rc::clone(&transcript),
            account_user_id,
            available_cents,
            options: options_with_destinations(destinations),
        };

        // Complete expected observation, execute once, and compare whole state.
        let result = out::prepare(
            &store,
            &api,
            Money::from_cents(100)?,
            TransferSpeed::Standard,
        )
        .await;
        let projected = match result {
            Err(TransferOutError::Validation(_)) => "account",
            Err(TransferOutError::InsufficientBalance) => "balance",
            Err(TransferOutError::Selection(_)) => "selection",
            Ok(_) | Err(_) => "unexpected",
        };
        assert_eq!(
            (projected, transcript.borrow().clone()),
            (expected_error, expected_calls)
        );
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn unsupported_speed_stops_before_credential_or_api_access() -> TestResult {
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let store = FakeStore {
        transcript: Rc::clone(&transcript),
        user_id: "123",
    };
    let api = FakeApi {
        transcript: Rc::clone(&transcript),
        account_user_id: "123",
        available_cents: 5_000,
        options: options_with_destinations(vec![instrument("bank-default", true)?]),
    };

    let result = out::prepare(
        &store,
        &api,
        Money::from_cents(100)?,
        TransferSpeed::Instant,
    )
    .await;

    assert!(matches!(result, Err(TransferOutError::UnsupportedSpeed)));
    assert!(transcript.borrow().is_empty());
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn options_read_uses_only_credential_and_options_capabilities() -> TestResult {
    // Setup and immutable initial state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let store = FakeStore {
        transcript: Rc::clone(&transcript),
        user_id: "123",
    };
    let api = FakeApi {
        transcript: Rc::clone(&transcript),
        account_user_id: "123",
        available_cents: 5_000,
        options: options_with_destinations(vec![instrument("bank-default", true)?]),
    };

    // Complete expected observation.
    let expected = (
        Some(TransferSpeed::Standard),
        1_usize,
        vec![Call::ReadCredential, Call::TransferOptions],
    );

    // Execute once.
    let result = options::get(&store, &api).await?;
    let observed = (
        result.options().preferred_out(),
        result.options().standard().eligible_destinations().len(),
        transcript.borrow().clone(),
    );

    assert_eq!(observed, expected);
    Ok(())
}

fn credential(user_id: &str) -> Result<CredentialEnvelope, Box<dyn Error>> {
    Ok(CredentialEnvelope::new(
        AccessToken::from_str("synthetic-token")?,
        DeviceId::from_str("synthetic-device")?,
        UserId::from_str(user_id)?,
        Username::from_bare("alice")?,
        Some("Alice".to_owned()),
        OffsetDateTime::UNIX_EPOCH,
    ))
}

fn account(user_id: &str) -> Result<Account, Box<dyn Error>> {
    Ok(Account::new(
        UserId::from_str(user_id)?,
        Username::from_bare("alice")?,
        Some("Alice".to_owned()),
    ))
}

fn instrument(id: &str, is_default: bool) -> Result<TransferInstrument, Box<dyn Error>> {
    Ok(TransferInstrument::new(
        TransferInstrumentId::from_str(id)?,
        "Synthetic bank".to_owned(),
        "Checking".to_owned(),
        "bank".to_owned(),
        TransferInstrumentSuffix::from_str("1234")?,
        is_default,
        "1-3 business days".to_owned(),
    ))
}

fn options_with_destinations(destinations: Vec<TransferInstrument>) -> TransferOptions {
    TransferOptions::new(
        None,
        Some(TransferSpeed::Standard),
        TransferModeOptions::new(
            Vec::new(),
            destinations,
            TransferFeeMetadata::default(),
            "1-3 business days".to_owned(),
        ),
        TransferModeOptions::new(
            Vec::new(),
            Vec::new(),
            TransferFeeMetadata::new(
                Some("25".to_owned()),
                Some("2500".to_owned()),
                Some("1.75".to_owned()),
                true,
            ),
            "Usually within minutes".to_owned(),
        ),
    )
}

#[derive(Clone, Copy, Debug)]
struct SyntheticApiError;

impl fmt::Display for SyntheticApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("synthetic API failure")
    }
}

impl Error for SyntheticApiError {}

impl ApiFailure for SyntheticApiError {
    fn kind(&self) -> ApiFailureKind {
        ApiFailureKind::Internal
    }
}

#[derive(Clone, Copy, Debug)]
struct SyntheticCredentialError;

impl fmt::Display for SyntheticCredentialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("synthetic credential failure")
    }
}

impl Error for SyntheticCredentialError {}

impl CredentialStoreFailure for SyntheticCredentialError {
    fn kind(&self) -> CredentialFailureKind {
        CredentialFailureKind::Internal
    }
}

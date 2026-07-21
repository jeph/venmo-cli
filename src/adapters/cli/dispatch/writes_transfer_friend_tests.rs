use std::cell::RefCell;
use std::error::Error;
use std::future::{Future, ready};
use std::rc::Rc;
use std::str::FromStr;

use super::{run_friend_add_with, run_friend_remove_with, run_transfer_out_with};
use crate::adapters::cli::args::{
    FriendAddArgs, FriendRemoveArgs, TransferAmountArg, TransferOutArgs, TransferSpeedArg,
};
use crate::adapters::cli::error::AppError;
use crate::adapters::cli::output::TimestampFormatter;
use crate::features::auth::{CurrentAccountApi, PromptAvailability, PromptError};
use crate::features::payments::DefaultNoConfirmation;
use crate::features::people::{
    FriendshipMutationApi, FriendshipStatus, User, UserLookupApi, UserProfileKind, UserSearchApi,
    UserSearchPage, UserSearchPageRequest, UserSearchQuery,
};
use crate::features::transfers::{
    CreatedTransfer, TransferFeeMetadata, TransferInstrument, TransferInstrumentId,
    TransferInstrumentSuffix, TransferModeOptions, TransferOptions, TransferOptionsApi,
    TransferOutCreationApi, TransferOutPlan, TransferSpeed,
};
use crate::features::wallet::{Balance, BalanceApi, SignedUsdAmount};
use crate::shared::{
    AccessToken, Account, ApiFailure, ApiFailureKind, CredentialCapability, CredentialEnvelope,
    CredentialFailureKind, CredentialFormat, CredentialReader, CredentialStoreFailure, DeviceId,
    LoadedCredential, Money, UserId, Username,
};

type TestResult = Result<(), Box<dyn Error>>;
type Transcript = Rc<RefCell<Vec<Call>>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Call {
    Credential,
    Account,
    Balance,
    TransferOptions,
    Search,
    User,
    Prompt,
    Mutation,
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("synthetic dry-run error")]
struct FakeError;

impl CredentialStoreFailure for FakeError {
    fn kind(&self) -> CredentialFailureKind {
        CredentialFailureKind::Internal
    }
}

impl ApiFailure for FakeError {
    fn kind(&self) -> ApiFailureKind {
        ApiFailureKind::Internal
    }
}

struct FakeReader {
    calls: Transcript,
}

impl CredentialCapability for FakeReader {
    type Error = FakeError;
}

impl CredentialReader for FakeReader {
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        self.calls.borrow_mut().push(Call::Credential);
        Ok(Some(LoadedCredential {
            envelope: credential()?,
            format: CredentialFormat::Version1,
        }))
    }
}

struct FakeApi {
    calls: Transcript,
    friendship: FriendshipStatus,
}

impl CurrentAccountApi for FakeApi {
    type Error = FakeError;

    fn current_account<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a {
        self.calls.borrow_mut().push(Call::Account);
        ready(account())
    }
}

impl BalanceApi for FakeApi {
    type Error = FakeError;

    fn balance<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Balance, Self::Error>> + Send + 'a {
        self.calls.borrow_mut().push(Call::Balance);
        ready(Ok(Balance::new(
            SignedUsdAmount::from_cents(500),
            SignedUsdAmount::from_cents(0),
        )))
    }
}

impl TransferOptionsApi for FakeApi {
    type Error = FakeError;

    fn transfer_options<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<TransferOptions, Self::Error>> + Send + 'a {
        self.calls.borrow_mut().push(Call::TransferOptions);
        ready(transfer_options())
    }
}

impl TransferOutCreationApi for FakeApi {
    type Error = FakeError;

    fn create_transfer_out<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        _plan: &'a TransferOutPlan,
    ) -> impl Future<Output = Result<CreatedTransfer, Self::Error>> + Send + 'a {
        self.calls.borrow_mut().push(Call::Mutation);
        ready(Err(FakeError))
    }
}

impl UserSearchApi for FakeApi {
    type Error = FakeError;

    fn search_users<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        _query: &'a UserSearchQuery,
        _page: UserSearchPageRequest,
    ) -> impl Future<Output = Result<UserSearchPage, Self::Error>> + Send + 'a {
        self.calls.borrow_mut().push(Call::Search);
        ready(target(self.friendship).map(|user| UserSearchPage::new(vec![user], None)))
    }
}

impl UserLookupApi for FakeApi {
    type Error = FakeError;

    fn user_by_id<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        _user_id: &'a UserId,
    ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a {
        self.calls.borrow_mut().push(Call::User);
        ready(target(self.friendship))
    }
}

impl FriendshipMutationApi for FakeApi {
    type Error = FakeError;

    fn add_or_accept_friend<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        _target_user_id: &'a UserId,
        _expected_status: FriendshipStatus,
    ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a {
        self.calls.borrow_mut().push(Call::Mutation);
        ready(Err(FakeError))
    }

    fn remove_or_cancel_friend<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        _self_user_id: &'a UserId,
        _target_user_id: &'a UserId,
    ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a {
        self.calls.borrow_mut().push(Call::Mutation);
        ready(Err(FakeError))
    }
}

struct FakePrompt {
    calls: Transcript,
}

impl PromptAvailability for FakePrompt {
    fn can_prompt(&self) -> bool {
        self.calls.borrow_mut().push(Call::Prompt);
        true
    }
}

impl DefaultNoConfirmation for FakePrompt {
    fn confirm_default_no(&self, _prompt: &str) -> Result<bool, PromptError> {
        self.calls.borrow_mut().push(Call::Prompt);
        Ok(true)
    }
}

#[tokio::test(flavor = "current_thread")]
async fn transfer_out_dry_run_stops_after_full_preflight_and_details() -> TestResult {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader {
        calls: Rc::clone(&calls),
    };
    let api = FakeApi {
        calls: Rc::clone(&calls),
        friendship: FriendshipStatus::NotFriend,
    };
    let prompt = FakePrompt {
        calls: Rc::clone(&calls),
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    run_transfer_out_with(
        TransferOutArgs {
            amount: TransferAmountArg::Exact(Money::from_cents(100)?),
            speed: TransferSpeedArg::Standard,
            yes: false,
            dry_run: true,
        },
        &reader,
        &api,
        &prompt,
        &TimestampFormatter::for_time_zone(jiff::tz::TimeZone::UTC),
        &mut stdout,
        &mut stderr,
        unexpected_interruption,
    )
    .await?;

    assert_eq!(
        calls.borrow().as_slice(),
        [
            Call::Credential,
            Call::Account,
            Call::Balance,
            Call::TransferOptions
        ]
    );
    assert!(String::from_utf8(stderr)?.contains("Transfer details:"));
    assert_eq!(String::from_utf8(stdout)?, dry_run_output());
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn friendship_dry_runs_stop_after_authoritative_state_preflight() -> TestResult {
    for (add, status) in [
        (true, FriendshipStatus::NotFriend),
        (false, FriendshipStatus::Friend),
    ] {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader {
            calls: Rc::clone(&calls),
        };
        let api = FakeApi {
            calls: Rc::clone(&calls),
            friendship: status,
        };
        let prompt = FakePrompt {
            calls: Rc::clone(&calls),
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let username = Username::from_bare("target")?;

        if add {
            run_friend_add_with(
                FriendAddArgs {
                    username,
                    yes: false,
                    dry_run: true,
                },
                &reader,
                &api,
                &prompt,
                &mut stdout,
                &mut stderr,
                unexpected_interruption,
            )
            .await?;
        } else {
            run_friend_remove_with(
                FriendRemoveArgs {
                    username,
                    yes: false,
                    dry_run: true,
                },
                &reader,
                &api,
                &prompt,
                &mut stdout,
                &mut stderr,
                unexpected_interruption,
            )
            .await?;
        }

        assert_eq!(
            calls.borrow().as_slice(),
            [Call::Credential, Call::Account, Call::Search, Call::User]
        );
        assert!(String::from_utf8(stderr)?.contains("Friendship details:"));
        assert_eq!(String::from_utf8(stdout)?, dry_run_output());
    }
    Ok(())
}

fn unexpected_interruption() -> Result<std::future::Pending<Result<(), AppError>>, AppError> {
    Err(AppError::SignalInitialization {
        source: std::io::Error::other("unexpected dry-run interruption installation"),
    })
}

const fn dry_run_output() -> &'static str {
    "Dry run complete; no changes made.\n"
}

fn credential() -> Result<CredentialEnvelope, FakeError> {
    Ok(CredentialEnvelope::new(
        AccessToken::from_str("synthetic-token").map_err(|_| FakeError)?,
        DeviceId::from_str("synthetic-device").map_err(|_| FakeError)?,
        UserId::from_str("123").map_err(|_| FakeError)?,
        Username::from_bare("owner").map_err(|_| FakeError)?,
        Some("Owner".to_owned()),
        time::OffsetDateTime::UNIX_EPOCH,
    ))
}

fn account() -> Result<Account, FakeError> {
    Ok(Account::new(
        UserId::from_str("123").map_err(|_| FakeError)?,
        Username::from_bare("owner").map_err(|_| FakeError)?,
        Some("Owner".to_owned()),
    ))
}

fn target(status: FriendshipStatus) -> Result<User, FakeError> {
    Ok(User::new(
        UserId::from_str("456").map_err(|_| FakeError)?,
        Some(Username::from_bare("target").map_err(|_| FakeError)?),
        Some("Target".to_owned()),
    )
    .with_financial_attributes(UserProfileKind::Personal, true)
    .with_friendship_status(status))
}

fn transfer_options() -> Result<TransferOptions, FakeError> {
    let destination = TransferInstrument::new(
        TransferInstrumentId::from_str("bank-1").map_err(|_| FakeError)?,
        "Synthetic bank".to_owned(),
        "Checking".to_owned(),
        "bank".to_owned(),
        TransferInstrumentSuffix::from_str("1234").map_err(|_| FakeError)?,
        true,
        "1-3 business days".to_owned(),
    );
    Ok(TransferOptions::new(
        Some(TransferSpeed::Standard),
        TransferModeOptions::new(
            vec![destination],
            TransferFeeMetadata::default(),
            "1-3 business days".to_owned(),
        ),
        TransferModeOptions::new(Vec::new(), TransferFeeMetadata::default(), String::new()),
    ))
}

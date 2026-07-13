use thiserror::Error;

use super::auth::OperationFailure;
use super::financial::{FinancialValidationError, validate_account, validate_recipient};
use super::funding::{self, FundingSelectionError};
use super::ports::{
    ApiFailure, ApiFailureKind, AuthenticationApi, BalanceApi, ClientRequestIdGenerator,
    CredentialReader, PaymentCreationApi, PaymentEligibilityApi, PeerFundingApi, PromptError,
    PromptPort, UsersApi,
};
use super::recipients::{self, RecipientResolutionError};
use crate::domain::{
    CreatedPayment, CredentialEnvelope, Money, Note, PayPlan, PaymentMethodId, RecipientInput,
};

#[derive(Debug)]
pub struct PreparedPay {
    credential: CredentialEnvelope,
    plan: PayPlan,
}

impl PreparedPay {
    #[must_use]
    pub(crate) const fn new(credential: CredentialEnvelope, plan: PayPlan) -> Self {
        Self { credential, plan }
    }

    #[must_use]
    pub const fn plan(&self) -> &PayPlan {
        &self.plan
    }
}

#[derive(Debug)]
pub struct PayResult {
    plan: PayPlan,
    created: CreatedPayment,
}

impl PayResult {
    #[must_use]
    pub(crate) const fn new(plan: PayPlan, created: CreatedPayment) -> Self {
        Self { plan, created }
    }

    #[must_use]
    pub const fn plan(&self) -> &PayPlan {
        &self.plan
    }

    #[must_use]
    pub const fn created(&self) -> &CreatedPayment {
        &self.created
    }
}

#[derive(Debug, Error)]
pub enum PayError {
    #[error("no Venmo credential is stored; run `venmo auth login`")]
    MissingCredential,
    #[error("failed to read the OS credential entry: {source}")]
    CredentialLoad {
        #[source]
        source: OperationFailure,
    },
    #[error("failed to verify the current Venmo account: {source}")]
    CurrentAccount {
        kind: ApiFailureKind,
        #[source]
        source: OperationFailure,
    },
    #[error(transparent)]
    Validation(#[from] FinancialValidationError),
    #[error(transparent)]
    Recipient(#[from] RecipientResolutionError),
    #[error("failed to read the Venmo wallet balance: {source}")]
    Balance {
        kind: ApiFailureKind,
        #[source]
        source: OperationFailure,
    },
    #[error("failed to obtain peer-payment funding methods: {source}")]
    FundingMethods {
        kind: ApiFailureKind,
        #[source]
        source: OperationFailure,
    },
    #[error(transparent)]
    FundingSelection(#[from] FundingSelectionError),
    #[error("failed to verify payment eligibility and fees: {source}")]
    Eligibility {
        kind: ApiFailureKind,
        #[source]
        source: OperationFailure,
    },
    #[error("the payment eligibility response reported a nonzero fee")]
    NonZeroEligibilityFee,
    #[error(
        "payment confirmation requires both stdin and stderr to be terminals; pass `--yes` to authorize non-interactively"
    )]
    ConfirmationRequired,
    #[error("payment cancelled")]
    ConfirmationDeclined,
    #[error("payment confirmation failed: {source}")]
    Confirmation {
        #[source]
        source: PromptError,
    },
    #[error("failed to create the Venmo payment: {source}")]
    Create {
        kind: ApiFailureKind,
        #[source]
        source: OperationFailure,
    },
}

impl PayError {
    #[must_use]
    pub const fn api_failure_kind(&self) -> Option<ApiFailureKind> {
        match self {
            Self::CurrentAccount { kind, .. }
            | Self::Balance { kind, .. }
            | Self::FundingMethods { kind, .. }
            | Self::Eligibility { kind, .. }
            | Self::Create { kind, .. } => Some(*kind),
            Self::Recipient(error) => match error.failure_kind() {
                recipients::RecipientResolutionFailureKind::Api(kind) => Some(kind),
                _ => None,
            },
            Self::MissingCredential
            | Self::CredentialLoad { .. }
            | Self::Validation(_)
            | Self::FundingSelection(_)
            | Self::NonZeroEligibilityFee
            | Self::ConfirmationRequired
            | Self::ConfirmationDeclined
            | Self::Confirmation { .. } => None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct AuthorizedPay(PreparedPay);

#[allow(clippy::too_many_arguments)]
pub(crate) async fn prepare<R, A, G, P>(
    credentials: &R,
    api: &A,
    generator: &G,
    prompt: &P,
    recipient: &RecipientInput,
    amount: Money,
    note: Note,
    requested_method: Option<&PaymentMethodId>,
) -> Result<PreparedPay, PayError>
where
    R: CredentialReader,
    A: AuthenticationApi + UsersApi + BalanceApi + PeerFundingApi + PaymentEligibilityApi,
    <A as AuthenticationApi>::Error: ApiFailure,
    G: ClientRequestIdGenerator,
    P: PromptPort,
{
    let loaded = credentials
        .read_credential()
        .map_err(|source| PayError::CredentialLoad {
            source: OperationFailure::new(source),
        })?
        .ok_or(PayError::MissingCredential)?;
    let credential = loaded.envelope;
    let account = api
        .current_account(credential.access_token(), credential.device_id())
        .await
        .map_err(|source| PayError::CurrentAccount {
            kind: source.kind(),
            source: OperationFailure::new(source),
        })?;
    validate_account(&credential, &account)?;
    let resolved = recipients::resolve_with_credential(&credential, api, recipient).await?;
    let recipient = resolved.into_user();
    validate_recipient(&account, &recipient)?;
    let balance = api
        .balance(credential.access_token(), credential.device_id())
        .await
        .map_err(|source| PayError::Balance {
            kind: source.kind(),
            source: OperationFailure::new(source),
        })?;
    let methods = api
        .peer_funding_methods(credential.access_token(), credential.device_id())
        .await
        .map_err(|source| PayError::FundingMethods {
            kind: source.kind(),
            source: OperationFailure::new(source),
        })?;
    let funding = funding::select(prompt, &methods, requested_method)?;
    let eligibility = api
        .payment_eligibility(
            credential.access_token(),
            credential.device_id(),
            &recipient,
            amount,
            &note,
        )
        .await
        .map_err(|source| PayError::Eligibility {
            kind: source.kind(),
            source: OperationFailure::new(source),
        })?;
    if eligibility.fee_cents() != 0 {
        return Err(PayError::NonZeroEligibilityFee);
    }
    let funding = funding.into_peer_method().with_proven_zero_fee();
    let plan = PayPlan::new(
        generator.generate(),
        account,
        recipient,
        amount,
        note,
        balance,
        funding,
        eligibility.into_token(),
    );
    Ok(PreparedPay::new(credential, plan))
}

pub(crate) fn authorize<P>(
    prompt: &P,
    prepared: PreparedPay,
    assume_yes: bool,
) -> Result<AuthorizedPay, PayError>
where
    P: PromptPort,
{
    if assume_yes {
        return Ok(AuthorizedPay(prepared));
    }
    if !prompt.can_prompt() {
        return Err(PayError::ConfirmationRequired);
    }
    match prompt
        .confirm_default_no("Send this payment?")
        .map_err(|source| PayError::Confirmation { source })?
    {
        true => Ok(AuthorizedPay(prepared)),
        false => Err(PayError::ConfirmationDeclined),
    }
}

pub(crate) async fn execute<A>(api: &A, authorized: AuthorizedPay) -> Result<PayResult, PayError>
where
    A: PaymentCreationApi,
{
    let AuthorizedPay(prepared) = authorized;
    let created = api
        .create_payment(
            prepared.credential.access_token(),
            prepared.credential.device_id(),
            &prepared.plan,
        )
        .await
        .map_err(|source| PayError::Create {
            kind: source.kind(),
            source: OperationFailure::new(source),
        })?;
    Ok(PayResult::new(prepared.plan, created))
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
        CredentialFailureKind, CredentialFormat, CredentialStoreFailure, LoadedCredential,
        PaymentEligibility, PromptError, UserSearchPage, UserSearchPageRequest,
    };
    use crate::domain::{
        AccessToken, Account, AccountPassword, Balance, ClientRequestId, DeviceId,
        EligibilityToken, FinancialStatus, LoginIdentifier, OtpCode, PaymentId, PaymentMethod,
        PeerFundingFee, PeerFundingMethod, PeerFundingRole, SignedUsdAmount, User, UserId,
        UserProfileKind, UserSearchQuery, Username,
    };

    type TestResult = Result<(), Box<dyn Error>>;

    #[derive(Clone, Debug, thiserror::Error)]
    #[error("synthetic API failure")]
    struct FakeApiError(ApiFailureKind);

    impl ApiFailure for FakeApiError {
        fn kind(&self) -> ApiFailureKind {
            self.0
        }
    }

    #[derive(Debug, thiserror::Error)]
    #[error("synthetic credential failure")]
    struct FakeCredentialError;

    impl CredentialStoreFailure for FakeCredentialError {
        fn kind(&self) -> CredentialFailureKind {
            CredentialFailureKind::Unavailable
        }
    }

    struct FakeReader {
        present: bool,
        reads: Cell<usize>,
    }

    impl CredentialReader for FakeReader {
        type Error = FakeCredentialError;

        fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
            self.reads.set(self.reads.get() + 1);
            if self.present {
                Ok(Some(loaded_credential()?))
            } else {
                Ok(None)
            }
        }
    }

    struct FakeApi {
        account: Result<Account, FakeApiError>,
        user: Result<User, FakeApiError>,
        balance: Result<Balance, FakeApiError>,
        methods: Result<Vec<PeerFundingMethod>, FakeApiError>,
        eligibility: Result<PaymentEligibility, FakeApiError>,
        creation: Result<CreatedPayment, FakeApiError>,
        account_calls: Cell<usize>,
        user_calls: Cell<usize>,
        balance_calls: Cell<usize>,
        funding_calls: Cell<usize>,
        eligibility_calls: Cell<usize>,
        creation_calls: Cell<usize>,
    }

    impl AuthenticationApi for FakeApi {
        type Error = FakeApiError;

        fn current_account<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
        ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a {
            self.account_calls.set(self.account_calls.get() + 1);
            ready(self.account.clone())
        }

        fn revoke_access_token<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
        ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
            ready(Err(FakeApiError(ApiFailureKind::Internal)))
        }
    }

    impl UsersApi for FakeApi {
        type Error = FakeApiError;

        fn user_by_id<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            _user_id: &'a UserId,
        ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a {
            self.user_calls.set(self.user_calls.get() + 1);
            ready(self.user.clone())
        }

        fn search_users<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            _query: &'a UserSearchQuery,
            _page: UserSearchPageRequest,
        ) -> impl Future<Output = Result<UserSearchPage, Self::Error>> + Send + 'a {
            ready(Err(FakeApiError(ApiFailureKind::Internal)))
        }
    }

    impl BalanceApi for FakeApi {
        type Error = FakeApiError;

        fn balance<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
        ) -> impl Future<Output = Result<Balance, Self::Error>> + Send + 'a {
            self.balance_calls.set(self.balance_calls.get() + 1);
            ready(self.balance.clone())
        }
    }

    impl PeerFundingApi for FakeApi {
        type Error = FakeApiError;

        fn peer_funding_methods<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
        ) -> impl Future<Output = Result<Vec<PeerFundingMethod>, Self::Error>> + Send + 'a {
            self.funding_calls.set(self.funding_calls.get() + 1);
            ready(self.methods.clone())
        }
    }

    impl PaymentEligibilityApi for FakeApi {
        type Error = FakeApiError;

        fn payment_eligibility<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            _recipient: &'a User,
            _amount: Money,
            _note: &'a Note,
        ) -> impl Future<Output = Result<PaymentEligibility, Self::Error>> + Send + 'a {
            self.eligibility_calls.set(self.eligibility_calls.get() + 1);
            ready(self.eligibility.clone())
        }
    }

    impl PaymentCreationApi for FakeApi {
        type Error = FakeApiError;

        fn create_payment<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            _plan: &'a PayPlan,
        ) -> impl Future<Output = Result<CreatedPayment, Self::Error>> + Send + 'a {
            self.creation_calls.set(self.creation_calls.get() + 1);
            ready(self.creation.clone())
        }
    }

    struct FixedGenerator;

    impl ClientRequestIdGenerator for FixedGenerator {
        fn generate(&self) -> ClientRequestId {
            match ClientRequestId::from_str("123e4567-e89b-12d3-a456-426614174000") {
                Ok(id) => id,
                Err(_) => ClientRequestId::generate(),
            }
        }
    }

    struct FakePrompt {
        interactive: bool,
        confirmed: bool,
    }

    const NONINTERACTIVE_PROMPT: FakePrompt = FakePrompt {
        interactive: false,
        confirmed: false,
    };

    impl PromptPort for FakePrompt {
        fn can_prompt(&self) -> bool {
            self.interactive
        }

        fn read_login_identifier(&self, _prompt: &str) -> Result<LoginIdentifier, PromptError> {
            Err(PromptError::Cancelled)
        }

        fn read_account_password(&self, _prompt: &str) -> Result<AccountPassword, PromptError> {
            Err(PromptError::Cancelled)
        }

        fn read_otp_code(&self, _prompt: &str) -> Result<OtpCode, PromptError> {
            Err(PromptError::Cancelled)
        }

        fn read_access_token(&self, _prompt: &str) -> Result<AccessToken, PromptError> {
            Err(PromptError::Cancelled)
        }

        fn read_device_id(&self, _prompt: &str) -> Result<DeviceId, PromptError> {
            Err(PromptError::Cancelled)
        }

        fn confirm_default_no(&self, _prompt: &str) -> Result<bool, PromptError> {
            Ok(self.confirmed)
        }

        fn select(&self, _prompt: &str, _items: &[String]) -> Result<usize, PromptError> {
            Err(PromptError::NotInteractive)
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn complete_preflight_then_execute_performs_exactly_one_write() -> TestResult {
        let reader = FakeReader {
            present: true,
            reads: Cell::new(0),
        };
        let api = successful_api()?;
        let recipient = RecipientInput::from_str("456")?;

        let prepared = prepare(
            &reader,
            &api,
            &FixedGenerator,
            &NONINTERACTIVE_PROMPT,
            &recipient,
            Money::from_cents(1)?,
            Note::from_str("Synthetic note")?,
            None,
        )
        .await?;

        assert_eq!(api.creation_calls.get(), 0);
        assert_eq!(prepared.plan().recipient().user_id().as_str(), "456");
        assert_eq!(
            prepared.plan().backup_method().method().id().as_str(),
            "bank-1"
        );
        let authorized = authorize(&NONINTERACTIVE_PROMPT, prepared, true)?;
        let result = execute(&api, authorized).await?;
        assert_eq!(result.created().id().as_str(), "payment-1");
        assert_eq!(reader.reads.get(), 1);
        assert_eq!(api.account_calls.get(), 1);
        assert_eq!(api.user_calls.get(), 1);
        assert_eq!(api.balance_calls.get(), 1);
        assert_eq!(api.funding_calls.get(), 1);
        assert_eq!(api.eligibility_calls.get(), 1);
        assert_eq!(api.creation_calls.get(), 1);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn execution_requires_yes_or_an_interactive_default_no_confirmation() -> TestResult {
        let noninteractive = authorize(
            &NONINTERACTIVE_PROMPT,
            prepared_pay(&successful_api()?).await?,
            false,
        );
        assert!(matches!(
            noninteractive,
            Err(PayError::ConfirmationRequired)
        ));

        let declined = FakePrompt {
            interactive: true,
            confirmed: false,
        };
        let declined = authorize(&declined, prepared_pay(&successful_api()?).await?, false);
        assert!(matches!(declined, Err(PayError::ConfirmationDeclined)));

        let confirmed = FakePrompt {
            interactive: true,
            confirmed: true,
        };
        assert!(authorize(&confirmed, prepared_pay(&successful_api()?).await?, false,).is_ok());
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn recipient_and_fee_failures_stop_before_any_write() -> TestResult {
        let reader = FakeReader {
            present: true,
            reads: Cell::new(0),
        };
        let recipient = RecipientInput::from_str("456")?;

        let mut missing_profile = successful_api()?;
        missing_profile.user = Ok(User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("bob")?),
            None,
        ));
        let result = prepare(
            &reader,
            &missing_profile,
            &FixedGenerator,
            &NONINTERACTIVE_PROMPT,
            &recipient,
            Money::from_cents(1)?,
            Note::from_str("Synthetic note")?,
            None,
        )
        .await;
        assert!(matches!(
            result,
            Err(PayError::Validation(
                FinancialValidationError::MissingProfileType
            ))
        ));
        assert_eq!(missing_profile.balance_calls.get(), 0);
        assert_eq!(missing_profile.creation_calls.get(), 0);

        let mut unknown_fee = successful_api()?;
        unknown_fee.methods = Ok(vec![peer_method(PeerFundingFee::Unknown)?]);
        let result = prepare(
            &reader,
            &unknown_fee,
            &FixedGenerator,
            &NONINTERACTIVE_PROMPT,
            &recipient,
            Money::from_cents(1)?,
            Note::from_str("Synthetic note")?,
            None,
        )
        .await?;
        assert_eq!(
            result.plan().backup_method().fee(),
            PeerFundingFee::ProvenZero
        );
        assert_eq!(unknown_fee.eligibility_calls.get(), 1);
        assert_eq!(unknown_fee.creation_calls.get(), 0);

        let mut eligibility_fee = successful_api()?;
        eligibility_fee.eligibility = Ok(PaymentEligibility::new(
            EligibilityToken::parse_owned("synthetic-eligibility".to_owned())?,
            1,
        ));
        let result = prepare(
            &reader,
            &eligibility_fee,
            &FixedGenerator,
            &NONINTERACTIVE_PROMPT,
            &recipient,
            Money::from_cents(1)?,
            Note::from_str("Synthetic note")?,
            None,
        )
        .await;
        assert!(matches!(result, Err(PayError::NonZeroEligibilityFee)));
        assert_eq!(eligibility_fee.creation_calls.get(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn missing_credentials_and_account_mismatch_stop_early() -> TestResult {
        let recipient = RecipientInput::from_str("456")?;
        let missing_reader = FakeReader {
            present: false,
            reads: Cell::new(0),
        };
        let api = successful_api()?;
        let missing = prepare(
            &missing_reader,
            &api,
            &FixedGenerator,
            &NONINTERACTIVE_PROMPT,
            &recipient,
            Money::from_cents(1)?,
            Note::from_str("Synthetic note")?,
            None,
        )
        .await;
        assert!(matches!(missing, Err(PayError::MissingCredential)));
        assert_eq!(api.account_calls.get(), 0);

        let reader = FakeReader {
            present: true,
            reads: Cell::new(0),
        };
        let mut mismatch = successful_api()?;
        mismatch.account = Ok(Account::new(
            UserId::from_str("999")?,
            Username::from_bare("other")?,
            None,
        ));
        let result = prepare(
            &reader,
            &mismatch,
            &FixedGenerator,
            &NONINTERACTIVE_PROMPT,
            &recipient,
            Money::from_cents(1)?,
            Note::from_str("Synthetic note")?,
            None,
        )
        .await;
        assert!(matches!(
            result,
            Err(PayError::Validation(
                FinancialValidationError::AccountMismatch
            ))
        ));
        assert_eq!(mismatch.user_calls.get(), 0);
        assert_eq!(mismatch.creation_calls.get(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn ambiguous_write_failure_is_preserved_and_never_retried() -> TestResult {
        let reader = FakeReader {
            present: true,
            reads: Cell::new(0),
        };
        let mut api = successful_api()?;
        api.creation = Err(FakeApiError(ApiFailureKind::AmbiguousWrite));
        let recipient = RecipientInput::from_str("456")?;
        let prepared = prepare(
            &reader,
            &api,
            &FixedGenerator,
            &NONINTERACTIVE_PROMPT,
            &recipient,
            Money::from_cents(1)?,
            Note::from_str("Synthetic note")?,
            None,
        )
        .await?;

        let authorized = authorize(&NONINTERACTIVE_PROMPT, prepared, true)?;
        let result = execute(&api, authorized).await;

        assert!(matches!(
            result,
            Err(PayError::Create {
                kind: ApiFailureKind::AmbiguousWrite,
                ..
            })
        ));
        assert_eq!(api.creation_calls.get(), 1);
        Ok(())
    }

    async fn prepared_pay(api: &FakeApi) -> Result<PreparedPay, Box<dyn Error>> {
        let reader = FakeReader {
            present: true,
            reads: Cell::new(0),
        };
        Ok(prepare(
            &reader,
            api,
            &FixedGenerator,
            &NONINTERACTIVE_PROMPT,
            &RecipientInput::from_str("456")?,
            Money::from_cents(1)?,
            Note::from_str("Synthetic note")?,
            None,
        )
        .await?)
    }

    fn successful_api() -> Result<FakeApi, Box<dyn Error>> {
        Ok(FakeApi {
            account: Ok(test_account()?),
            user: Ok(financial_user()?),
            balance: Ok(Balance::new(
                SignedUsdAmount::from_cents(0),
                SignedUsdAmount::from_cents(0),
            )),
            methods: Ok(vec![peer_method(PeerFundingFee::ProvenZero)?]),
            eligibility: Ok(PaymentEligibility::new(
                EligibilityToken::parse_owned("synthetic-eligibility".to_owned())?,
                0,
            )),
            creation: Ok(CreatedPayment::new(
                PaymentId::from_str("payment-1")?,
                FinancialStatus::Settled,
            )),
            account_calls: Cell::new(0),
            user_calls: Cell::new(0),
            balance_calls: Cell::new(0),
            funding_calls: Cell::new(0),
            eligibility_calls: Cell::new(0),
            creation_calls: Cell::new(0),
        })
    }

    fn loaded_credential() -> Result<LoadedCredential, FakeCredentialError> {
        Ok(LoadedCredential {
            envelope: CredentialEnvelope::new(
                AccessToken::from_str("synthetic-token").map_err(|_| FakeCredentialError)?,
                DeviceId::from_str("synthetic-device").map_err(|_| FakeCredentialError)?,
                UserId::from_str("123").map_err(|_| FakeCredentialError)?,
                Username::from_bare("owner").map_err(|_| FakeCredentialError)?,
                Some("Synthetic owner".to_owned()),
                OffsetDateTime::UNIX_EPOCH,
            ),
            format: CredentialFormat::Version1,
        })
    }

    fn test_account() -> Result<Account, Box<dyn Error>> {
        Ok(Account::new(
            UserId::from_str("123")?,
            Username::from_bare("owner")?,
            Some("Synthetic owner".to_owned()),
        ))
    }

    fn financial_user() -> Result<User, Box<dyn Error>> {
        Ok(User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("bob")?),
            Some("Synthetic recipient".to_owned()),
        )
        .with_financial_attributes(UserProfileKind::Personal, true))
    }

    fn peer_method(fee: PeerFundingFee) -> Result<PeerFundingMethod, Box<dyn Error>> {
        Ok(PeerFundingMethod::new(
            PaymentMethod::new(
                PaymentMethodId::from_str("bank-1")?,
                Some("Synthetic bank".to_owned()),
                Some("bank".to_owned()),
                Some("1234".to_owned()),
                true,
            ),
            PeerFundingRole::Default,
            fee,
        ))
    }
}

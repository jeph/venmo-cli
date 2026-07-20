use super::*;

pub(super) type TestResult = Result<(), Box<dyn Error>>;
pub(super) type Transcript = Rc<RefCell<Vec<Call>>>;

pub(super) const ACCESS_TOKEN: &str = "synthetic-secret-access-token";
pub(super) const DEVICE_ID: &str = "synthetic-secret-device-id";
pub(super) const ELIGIBILITY_TOKEN: &str = "synthetic-secret-eligibility-token";
pub(super) const REQUEST_UUID: &str = "123e4567-e89b-12d3-a456-426614174000";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RedactedSecret {
    Redacted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PayPlanCall {
    pub(super) request_id: ClientRequestId,
    pub(super) account: Account,
    pub(super) recipient: User,
    pub(super) amount: Money,
    pub(super) note: Note,
    pub(super) balance: Balance,
    pub(super) funding_source: PeerFundingSource,
    pub(super) funding_source_selection: PeerFundingSourceSelection,
    pub(super) eligibility_fee_cents: u64,
    pub(super) eligibility_token: RedactedSecret,
    pub(super) visibility: Visibility,
}

impl From<&PayPlan> for PayPlanCall {
    fn from(plan: &PayPlan) -> Self {
        Self {
            request_id: plan.request_id(),
            account: plan.account().clone(),
            recipient: plan.recipient().clone(),
            amount: plan.amount(),
            note: plan.note().clone(),
            balance: plan.balance().clone(),
            funding_source: plan.funding_source().clone(),
            funding_source_selection: plan.funding_source_selection(),
            eligibility_fee_cents: plan.eligibility_fee_cents(),
            eligibility_token: RedactedSecret::Redacted,
            visibility: plan.visibility(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Call {
    ReadCredential,
    CurrentAccount {
        session: RedactedSecret,
    },
    UserLookup {
        session: RedactedSecret,
        user_id: UserId,
    },
    UserSearch {
        session: RedactedSecret,
        query: UserSearchQuery,
        page: UserSearchPageRequest,
    },
    Balance {
        session: RedactedSecret,
    },
    FundingMethods {
        session: RedactedSecret,
    },
    Eligibility {
        session: RedactedSecret,
        recipient: User,
        amount: Money,
        note: Note,
    },
    GenerateClientRequestId {
        request_id: ClientRequestId,
    },
    PromptAvailability,
    ConfirmDefaultNo {
        prompt: String,
    },
    CreatePayment {
        session: RedactedSecret,
        plan: Box<PayPlanCall>,
        verification: PaymentVerification,
    },
    IssuePaymentOtp {
        session: RedactedSecret,
        request_id: ClientRequestId,
    },
    ReadPaymentOtp {
        prompt: String,
    },
    VerifyPaymentOtp {
        session: RedactedSecret,
        request_id: ClientRequestId,
        otp: RedactedSecret,
    },
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct Observation<Outcome> {
    pub(super) outcome: Outcome,
    pub(super) transcript: Vec<Call>,
}

impl<Outcome> Observation<Outcome> {
    pub(super) const fn new(outcome: Outcome, transcript: Vec<Call>) -> Self {
        Self {
            outcome,
            transcript,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum ReaderScript {
    Present,
    Missing,
    Failure,
}

pub(super) struct FakeReader {
    script: ReaderScript,
    transcript: Transcript,
}

impl FakeReader {
    pub(super) fn new(script: ReaderScript, transcript: Transcript) -> Self {
        Self { script, transcript }
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("synthetic credential failure")]
pub(super) struct FakeCredentialError;

impl CredentialStoreFailure for FakeCredentialError {
    fn kind(&self) -> CredentialFailureKind {
        CredentialFailureKind::Unavailable
    }
}

impl CredentialCapability for FakeReader {
    type Error = FakeCredentialError;
}

impl CredentialReader for FakeReader {
    fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
        self.transcript.borrow_mut().push(Call::ReadCredential);
        match self.script {
            ReaderScript::Present => loaded_credential().map(Some),
            ReaderScript::Missing => Ok(None),
            ReaderScript::Failure => Err(FakeCredentialError),
        }
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("synthetic API failure")]
pub(super) struct FakeApiError(pub(super) ApiFailureKind);

impl ApiFailure for FakeApiError {
    fn kind(&self) -> ApiFailureKind {
        self.0
    }
}

pub(super) struct PayScript {
    account: Result<Account, FakeApiError>,
    search_user: User,
    user: Result<User, FakeApiError>,
    balance: Result<Balance, FakeApiError>,
    sources: Result<PeerFundingSources, FakeApiError>,
    eligibility: Result<BlankSourceEligibility, FakeApiError>,
    creations: Vec<Result<PaymentCreationOutcome, FakeApiError>>,
    otp_issue: Result<(), FakeApiError>,
    otp_verification: Result<PaymentOtpVerification, FakeApiError>,
}

impl PayScript {
    pub(super) fn successful() -> Result<Self, Box<dyn Error>> {
        let user = financial_user()?;
        Ok(Self {
            account: Ok(test_account()?),
            search_user: user.clone(),
            user: Ok(user),
            balance: Ok(test_balance()),
            sources: Ok(peer_sources(
                Some(balance_method()?),
                vec![peer_method(
                    "bank-1",
                    PeerFundingRole::Default,
                    PeerFundingFee::ProvenZero,
                )?],
            )),
            eligibility: Ok(blank_eligibility(0)?),
            creations: vec![Ok(PaymentCreationOutcome::Created(created_payment()?))],
            otp_issue: Ok(()),
            otp_verification: Ok(PaymentOtpVerification::Verified),
        })
    }

    pub(super) fn with_account(self, account: Result<Account, FakeApiError>) -> Self {
        Self { account, ..self }
    }

    pub(super) fn with_user(self, user: Result<User, FakeApiError>) -> Self {
        Self { user, ..self }
    }

    pub(super) fn with_balance(self, balance: Result<Balance, FakeApiError>) -> Self {
        Self { balance, ..self }
    }

    pub(super) fn with_sources(self, sources: Result<PeerFundingSources, FakeApiError>) -> Self {
        Self { sources, ..self }
    }

    pub(super) fn with_eligibility(
        self,
        eligibility: Result<BlankSourceEligibility, FakeApiError>,
    ) -> Self {
        Self {
            eligibility,
            ..self
        }
    }

    pub(super) fn with_creation(self, creation: Result<CreatedPayment, FakeApiError>) -> Self {
        Self {
            creations: vec![creation.map(PaymentCreationOutcome::Created)],
            ..self
        }
    }

    pub(super) fn with_creation_sequence(
        self,
        creations: Vec<Result<PaymentCreationOutcome, FakeApiError>>,
    ) -> Self {
        Self { creations, ..self }
    }

    pub(super) fn with_otp_issue(self, otp_issue: Result<(), FakeApiError>) -> Self {
        Self { otp_issue, ..self }
    }

    pub(super) fn with_otp_verification(
        self,
        otp_verification: Result<PaymentOtpVerification, FakeApiError>,
    ) -> Self {
        Self {
            otp_verification,
            ..self
        }
    }
}

pub(super) struct FakeApi {
    account: Result<Account, FakeApiError>,
    search_user: User,
    user: Result<User, FakeApiError>,
    balance: Result<Balance, FakeApiError>,
    sources: Result<PeerFundingSources, FakeApiError>,
    eligibility: RefCell<Option<Result<BlankSourceEligibility, FakeApiError>>>,
    creations: RefCell<VecDeque<Result<PaymentCreationOutcome, FakeApiError>>>,
    otp_issue: Result<(), FakeApiError>,
    otp_verification: Result<PaymentOtpVerification, FakeApiError>,
    transcript: Transcript,
}

impl FakeApi {
    pub(super) fn new(script: PayScript, transcript: Transcript) -> Self {
        Self {
            account: script.account,
            search_user: script.search_user,
            user: script.user,
            balance: script.balance,
            sources: script.sources,
            eligibility: RefCell::new(Some(script.eligibility)),
            creations: RefCell::new(script.creations.into()),
            otp_issue: script.otp_issue,
            otp_verification: script.otp_verification,
            transcript,
        }
    }
}

impl CurrentAccountApi for FakeApi {
    type Error = FakeApiError;

    fn current_account<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::CurrentAccount {
            session: RedactedSecret::Redacted,
        });
        ready(self.account.clone())
    }
}

impl UserLookupApi for FakeApi {
    type Error = FakeApiError;

    fn user_by_id<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        user_id: &'a UserId,
    ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::UserLookup {
            session: RedactedSecret::Redacted,
            user_id: user_id.clone(),
        });
        ready(self.user.clone())
    }
}

impl UserSearchApi for FakeApi {
    type Error = FakeApiError;

    fn search_users<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        query: &'a UserSearchQuery,
        page: UserSearchPageRequest,
    ) -> impl Future<Output = Result<UserSearchPage, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::UserSearch {
            session: RedactedSecret::Redacted,
            query: query.clone(),
            page,
        });
        ready(Ok(UserSearchPage::new(
            vec![self.search_user.clone()],
            None,
        )))
    }
}

impl BalanceApi for FakeApi {
    type Error = FakeApiError;

    fn balance<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<Balance, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::Balance {
            session: RedactedSecret::Redacted,
        });
        ready(self.balance.clone())
    }
}

impl PeerFundingApi for FakeApi {
    type Error = FakeApiError;

    fn peer_funding_sources<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
    ) -> impl Future<Output = Result<PeerFundingSources, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::FundingMethods {
            session: RedactedSecret::Redacted,
        });
        ready(self.sources.clone())
    }
}

impl BlankSourceEligibilityApi for FakeApi {
    type Error = FakeApiError;

    fn blank_source_eligibility<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        recipient: &'a User,
        amount: Money,
        note: &'a Note,
    ) -> impl Future<Output = Result<BlankSourceEligibility, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::Eligibility {
            session: RedactedSecret::Redacted,
            recipient: recipient.clone(),
            amount,
            note: note.clone(),
        });
        let result = match self.eligibility.borrow_mut().take() {
            Some(result) => result,
            None => Err(FakeApiError(ApiFailureKind::Internal)),
        };
        ready(result)
    }
}

impl PaymentCreationApi for FakeApi {
    type Error = FakeApiError;

    fn create_payment<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        plan: &'a PayPlan,
        verification: PaymentVerification,
    ) -> impl Future<Output = Result<PaymentCreationOutcome, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::CreatePayment {
            session: RedactedSecret::Redacted,
            plan: Box::new(PayPlanCall::from(plan)),
            verification,
        });
        ready(
            self.creations
                .borrow_mut()
                .pop_front()
                .unwrap_or(Err(FakeApiError(ApiFailureKind::Internal))),
        )
    }
}

impl PaymentStepUpApi for FakeApi {
    type Error = FakeApiError;

    fn issue_payment_otp<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        request_id: &'a ClientRequestId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::IssuePaymentOtp {
            session: RedactedSecret::Redacted,
            request_id: *request_id,
        });
        ready(self.otp_issue)
    }

    fn verify_payment_otp<'a>(
        &'a self,
        _access_token: &'a AccessToken,
        _device_id: &'a DeviceId,
        request_id: &'a ClientRequestId,
        _otp: &'a crate::features::auth::OtpCode,
    ) -> impl Future<Output = Result<PaymentOtpVerification, Self::Error>> + Send + 'a {
        self.transcript.borrow_mut().push(Call::VerifyPaymentOtp {
            session: RedactedSecret::Redacted,
            request_id: *request_id,
            otp: RedactedSecret::Redacted,
        });
        ready(self.otp_verification)
    }
}

pub(super) struct FixedGenerator {
    transcript: Transcript,
}

impl FixedGenerator {
    pub(super) fn new(transcript: Transcript) -> Self {
        Self { transcript }
    }
}

impl ClientRequestIdGenerator for FixedGenerator {
    fn generate(&self) -> ClientRequestId {
        let request_id = fixed_request_id();
        self.transcript
            .borrow_mut()
            .push(Call::GenerateClientRequestId { request_id });
        request_id
    }
}

#[derive(Clone, Copy)]
pub(super) enum ConfirmationScript {
    Answer(bool),
    Cancelled,
    Interaction,
}

pub(super) struct FakePrompt {
    interactive: bool,
    confirmation: ConfirmationScript,
    transcript: Transcript,
}

impl FakePrompt {
    pub(super) fn new(
        interactive: bool,
        confirmation: ConfirmationScript,
        transcript: Transcript,
    ) -> Self {
        Self {
            interactive,
            confirmation,
            transcript,
        }
    }
}

impl PromptAvailability for FakePrompt {
    fn can_prompt(&self) -> bool {
        self.transcript.borrow_mut().push(Call::PromptAvailability);
        self.interactive
    }
}

impl DefaultNoConfirmation for FakePrompt {
    fn confirm_default_no(&self, prompt: &str) -> Result<bool, PromptError> {
        self.transcript.borrow_mut().push(Call::ConfirmDefaultNo {
            prompt: prompt.to_owned(),
        });
        match self.confirmation {
            ConfirmationScript::Answer(answer) => Ok(answer),
            ConfirmationScript::Cancelled => Err(PromptError::Cancelled),
            ConfirmationScript::Interaction => Err(PromptError::Interaction {
                source: io::Error::other("synthetic confirmation failure"),
            }),
        }
    }
}

impl PaymentStepUpInput for FakePrompt {
    fn read_payment_otp(
        &self,
        prompt: &str,
    ) -> Result<crate::features::auth::OtpCode, PromptError> {
        self.transcript.borrow_mut().push(Call::ReadPaymentOtp {
            prompt: prompt.to_owned(),
        });
        crate::features::auth::OtpCode::parse_owned("123456".to_owned())
            .map_err(|source| PromptError::InvalidOtpCode { source })
    }
}

pub(super) async fn run_pay(
    reader: &FakeReader,
    api: &FakeApi,
    generator: &FixedGenerator,
    prompt: &FakePrompt,
    amount: Money,
    note: Note,
    assume_yes: bool,
) -> Result<PayResult, PayError> {
    run_pay_with_visibility(
        reader,
        api,
        generator,
        prompt,
        amount,
        note,
        Visibility::Private,
        assume_yes,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_pay_with_visibility(
    reader: &FakeReader,
    api: &FakeApi,
    generator: &FixedGenerator,
    prompt: &FakePrompt,
    amount: Money,
    note: Note,
    visibility: Visibility,
    assume_yes: bool,
) -> Result<PayResult, PayError> {
    let recipient = RecipientInput::from_str("bob").map_err(|_| {
        PayError::Preflight(PeerPreflightError::Recipient(
            crate::features::people::recipients::RecipientResolutionError::Internal {
                problem: "synthetic recipient input was invalid",
            },
        ))
    })?;
    let prepared = prepare(
        reader,
        api,
        generator,
        &recipient,
        amount,
        note,
        PayOptions::new(None, visibility),
    )
    .await?;
    let authorized = authorize(prompt, prepared, assume_yes)?;
    execute(api, prompt, authorized).await
}

pub(super) fn successful_calls(amount: Money, note: Note) -> Result<Vec<Call>, Box<dyn Error>> {
    successful_calls_with_fee(amount, note, 0)
}

pub(super) fn successful_calls_with_fee(
    amount: Money,
    note: Note,
    eligibility_fee_cents: u64,
) -> Result<Vec<Call>, Box<dyn Error>> {
    let recipient = financial_user()?;
    let account = test_account()?;
    let balance = test_balance();
    let source = PeerFundingSource::balance(balance_method()?);
    Ok(vec![
        Call::ReadCredential,
        current_account_call(),
        user_search_call()?,
        user_lookup_call("456")?,
        balance_call(),
        funding_methods_call(),
        Call::Eligibility {
            session: RedactedSecret::Redacted,
            recipient: recipient.clone(),
            amount,
            note: note.clone(),
        },
        Call::GenerateClientRequestId {
            request_id: fixed_request_id(),
        },
        Call::CreatePayment {
            session: RedactedSecret::Redacted,
            plan: Box::new(PayPlanCall {
                request_id: fixed_request_id(),
                account,
                recipient,
                amount,
                note,
                balance,
                funding_source: source,
                funding_source_selection: PeerFundingSourceSelection::Automatic,
                eligibility_fee_cents,
                eligibility_token: RedactedSecret::Redacted,
                visibility: Visibility::Private,
            }),
            verification: PaymentVerification::Unverified,
        },
    ])
}

pub(super) const fn current_account_call() -> Call {
    Call::CurrentAccount {
        session: RedactedSecret::Redacted,
    }
}

pub(super) fn user_lookup_call(user_id: &str) -> Result<Call, Box<dyn Error>> {
    Ok(Call::UserLookup {
        session: RedactedSecret::Redacted,
        user_id: UserId::from_str(user_id)?,
    })
}

pub(super) fn user_search_call() -> Result<Call, Box<dyn Error>> {
    Ok(Call::UserSearch {
        session: RedactedSecret::Redacted,
        query: UserSearchQuery::from_str("bob")?,
        page: UserSearchPageRequest::new(Limit::try_from(50)?, Offset::default()),
    })
}

pub(super) const fn balance_call() -> Call {
    Call::Balance {
        session: RedactedSecret::Redacted,
    }
}

pub(super) const fn funding_methods_call() -> Call {
    Call::FundingMethods {
        session: RedactedSecret::Redacted,
    }
}

fn loaded_credential() -> Result<LoadedCredential, FakeCredentialError> {
    Ok(LoadedCredential {
        envelope: CredentialEnvelope::new(
            AccessToken::from_str(ACCESS_TOKEN).map_err(|_| FakeCredentialError)?,
            DeviceId::from_str(DEVICE_ID).map_err(|_| FakeCredentialError)?,
            UserId::from_str("123").map_err(|_| FakeCredentialError)?,
            Username::from_bare("owner").map_err(|_| FakeCredentialError)?,
            Some("Synthetic owner".to_owned()),
            OffsetDateTime::UNIX_EPOCH,
        ),
        format: CredentialFormat::Version1,
    })
}

pub(super) fn fixed_request_id() -> ClientRequestId {
    match ClientRequestId::from_str(REQUEST_UUID) {
        Ok(request_id) => request_id,
        Err(_) => ClientRequestId::generate(),
    }
}

pub(super) fn test_account() -> Result<Account, Box<dyn Error>> {
    Ok(Account::new(
        UserId::from_str("123")?,
        Username::from_bare("owner")?,
        Some("Synthetic owner".to_owned()),
    ))
}

pub(super) fn financial_user() -> Result<User, Box<dyn Error>> {
    Ok(User::new(
        UserId::from_str("456")?,
        Some(Username::from_bare("bob")?),
        Some("Synthetic recipient".to_owned()),
    )
    .with_financial_attributes(UserProfileKind::Personal, true))
}

pub(super) fn test_balance() -> Balance {
    Balance::new(
        SignedUsdAmount::from_cents(300),
        SignedUsdAmount::from_cents(25),
    )
}

pub(super) fn peer_method(
    id: &str,
    role: PeerFundingRole,
    fee: PeerFundingFee,
) -> Result<PeerFundingMethod, Box<dyn Error>> {
    Ok(PeerFundingMethod::new(
        PaymentMethod::new(
            PaymentMethodId::from_str(id)?,
            Some(format!("Synthetic {id}")),
            Some("bank".to_owned()),
            Some("1234".to_owned()),
            matches!(role, PeerFundingRole::Default),
        ),
        role,
        fee,
    ))
}

pub(super) fn balance_method() -> Result<PaymentMethod, Box<dyn Error>> {
    Ok(PaymentMethod::new(
        PaymentMethodId::from_str("balance-1")?,
        Some("Venmo balance".to_owned()),
        Some("balance".to_owned()),
        None,
        true,
    ))
}

pub(super) const fn peer_sources(
    balance: Option<PaymentMethod>,
    external: Vec<PeerFundingMethod>,
) -> PeerFundingSources {
    PeerFundingSources::new(balance, external)
}

pub(super) fn blank_eligibility(fee_cents: u64) -> Result<BlankSourceEligibility, Box<dyn Error>> {
    Ok(BlankSourceEligibility::new(
        EligibilityToken::parse_owned(ELIGIBILITY_TOKEN.to_owned())?,
        fee_cents,
    ))
}

pub(super) fn pay_plan(
    account: Account,
    recipient: User,
    amount: Money,
    note: Note,
    balance: Balance,
    method: PeerFundingMethod,
) -> Result<PayPlan, Box<dyn Error>> {
    pay_plan_with_fee_and_visibility(
        account,
        recipient,
        amount,
        note,
        balance,
        method,
        0,
        Visibility::Private,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn pay_plan_with_fee(
    account: Account,
    recipient: User,
    amount: Money,
    note: Note,
    balance: Balance,
    method: PeerFundingMethod,
    eligibility_fee_cents: u64,
) -> Result<PayPlan, Box<dyn Error>> {
    pay_plan_with_fee_and_visibility(
        account,
        recipient,
        amount,
        note,
        balance,
        method,
        eligibility_fee_cents,
        Visibility::Private,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn pay_plan_with_visibility(
    account: Account,
    recipient: User,
    amount: Money,
    note: Note,
    balance: Balance,
    method: PeerFundingMethod,
    visibility: Visibility,
) -> Result<PayPlan, Box<dyn Error>> {
    pay_plan_with_fee_and_visibility(
        account, recipient, amount, note, balance, method, 0, visibility,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn pay_plan_with_fee_and_visibility(
    account: Account,
    recipient: User,
    amount: Money,
    note: Note,
    balance: Balance,
    _method: PeerFundingMethod,
    eligibility_fee_cents: u64,
    visibility: Visibility,
) -> Result<PayPlan, Box<dyn Error>> {
    Ok(PayPlan::new(
        fixed_request_id(),
        account,
        recipient,
        amount,
        note,
        balance,
        PeerFundingSource::balance(balance_method()?),
        PeerFundingSourceSelection::Automatic,
        eligibility_fee_cents,
        EligibilityToken::parse_owned(ELIGIBILITY_TOKEN.to_owned())?,
        visibility,
    ))
}

pub(super) fn created_payment() -> Result<CreatedPayment, Box<dyn Error>> {
    Ok(CreatedPayment::new(
        PaymentId::from_str("payment-1")?,
        FinancialStatus::Settled,
    ))
}

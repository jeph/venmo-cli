use thiserror::Error;

use super::auth::OperationFailure;
use super::financial::{FinancialValidationError, validate_account, validate_recipient};
use super::ports::{
    ApiFailure, ApiFailureKind, AuthenticationApi, BalanceApi, CredentialReader, PromptError,
    PromptPort, RequestAcceptanceApi, RequestLookupApi, UsersApi,
};
use super::request_mutation::{
    RequestMutationValidationError, validate_complete_request, validate_incoming_pending,
    validate_private_audience, validate_request_id,
};
use crate::domain::{AcceptRequestPlan, AcceptedRequest, CredentialEnvelope, RequestId};

#[derive(Debug)]
pub struct PreparedAccept {
    credential: CredentialEnvelope,
    plan: AcceptRequestPlan,
}

impl PreparedAccept {
    #[must_use]
    pub(crate) const fn new(credential: CredentialEnvelope, plan: AcceptRequestPlan) -> Self {
        Self { credential, plan }
    }

    #[must_use]
    pub const fn plan(&self) -> &AcceptRequestPlan {
        &self.plan
    }
}

#[derive(Debug)]
pub struct AcceptResult {
    plan: AcceptRequestPlan,
    accepted: AcceptedRequest,
}

impl AcceptResult {
    #[must_use]
    pub(crate) const fn new(plan: AcceptRequestPlan, accepted: AcceptedRequest) -> Self {
        Self { plan, accepted }
    }

    #[must_use]
    pub const fn plan(&self) -> &AcceptRequestPlan {
        &self.plan
    }

    #[must_use]
    pub const fn accepted(&self) -> &AcceptedRequest {
        &self.accepted
    }
}

#[derive(Debug, Error)]
pub enum AcceptError {
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
    AccountOrRequester(#[from] FinancialValidationError),
    #[error("failed to load the authoritative incoming request: {source}")]
    RequestLookup {
        kind: ApiFailureKind,
        #[source]
        source: OperationFailure,
    },
    #[error("failed to load authoritative requester details: {source}")]
    RequesterLookup {
        kind: ApiFailureKind,
        #[source]
        source: OperationFailure,
    },
    #[error("the authoritative requester identity did not match the incoming request")]
    RequesterIdentityMismatch,
    #[error(transparent)]
    RequestValidation(#[from] RequestMutationValidationError),
    #[error("failed to read the Venmo wallet balance: {source}")]
    Balance {
        kind: ApiFailureKind,
        #[source]
        source: OperationFailure,
    },
    #[error(
        "acceptance requires available Venmo balance covering the full request; external funding is unavailable until its request-acceptance contract is verified"
    )]
    InsufficientWalletBalance,
    #[error(
        "request acceptance confirmation requires both stdin and stderr to be terminals; pass `--yes` to authorize non-interactively"
    )]
    ConfirmationRequired,
    #[error("request acceptance cancelled")]
    ConfirmationDeclined,
    #[error("request acceptance confirmation failed: {source}")]
    Confirmation {
        #[source]
        source: PromptError,
    },
    #[error("failed to accept the Venmo request: {source}")]
    Accept {
        kind: ApiFailureKind,
        #[source]
        source: OperationFailure,
    },
}

impl AcceptError {
    #[must_use]
    pub const fn api_failure_kind(&self) -> Option<ApiFailureKind> {
        match self {
            Self::CurrentAccount { kind, .. }
            | Self::RequestLookup { kind, .. }
            | Self::RequesterLookup { kind, .. }
            | Self::Balance { kind, .. }
            | Self::Accept { kind, .. } => Some(*kind),
            Self::MissingCredential
            | Self::CredentialLoad { .. }
            | Self::AccountOrRequester(_)
            | Self::RequesterIdentityMismatch
            | Self::RequestValidation(_)
            | Self::InsufficientWalletBalance
            | Self::ConfirmationRequired
            | Self::ConfirmationDeclined
            | Self::Confirmation { .. } => None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct AuthorizedAccept(PreparedAccept);

pub(crate) async fn prepare<R, A>(
    credentials: &R,
    api: &A,
    request_id: &RequestId,
) -> Result<PreparedAccept, AcceptError>
where
    R: CredentialReader,
    A: AuthenticationApi + RequestLookupApi + UsersApi + BalanceApi,
    <A as AuthenticationApi>::Error: ApiFailure,
{
    let loaded = credentials
        .read_credential()
        .map_err(|source| AcceptError::CredentialLoad {
            source: OperationFailure::new(source),
        })?
        .ok_or(AcceptError::MissingCredential)?;
    let credential = loaded.envelope;
    let account = api
        .current_account(credential.access_token(), credential.device_id())
        .await
        .map_err(|source| AcceptError::CurrentAccount {
            kind: source.kind(),
            source: OperationFailure::new(source),
        })?;
    validate_account(&credential, &account)?;
    let request = api
        .pending_request_by_id(
            credential.access_token(),
            credential.device_id(),
            account.user_id(),
            request_id,
        )
        .await
        .map_err(|source| AcceptError::RequestLookup {
            kind: source.kind(),
            source: OperationFailure::new(source),
        })?;
    validate_request_id(&request, request_id)?;
    validate_incoming_pending(&request)?;
    validate_complete_request(&request)?;
    validate_private_audience(&request)?;
    let requester = api
        .user_by_id(
            credential.access_token(),
            credential.device_id(),
            request.counterparty().user_id(),
        )
        .await
        .map_err(|source| AcceptError::RequesterLookup {
            kind: source.kind(),
            source: OperationFailure::new(source),
        })?;
    if requester.user_id() != request.counterparty().user_id() {
        return Err(AcceptError::RequesterIdentityMismatch);
    }
    validate_recipient(&account, &requester)?;
    let request = request.with_counterparty(requester);
    validate_complete_request(&request)?;
    let balance = api
        .balance(credential.access_token(), credential.device_id())
        .await
        .map_err(|source| AcceptError::Balance {
            kind: source.kind(),
            source: OperationFailure::new(source),
        })?;
    let available_cents = u64::try_from(balance.available().cents()).unwrap_or_default();
    if available_cents < request.amount().cents() {
        return Err(AcceptError::InsufficientWalletBalance);
    }
    Ok(PreparedAccept::new(
        credential,
        AcceptRequestPlan::new(account, request, balance),
    ))
}

pub(crate) fn authorize<P>(
    prompt: &P,
    prepared: PreparedAccept,
    assume_yes: bool,
) -> Result<AuthorizedAccept, AcceptError>
where
    P: PromptPort,
{
    if assume_yes {
        return Ok(AuthorizedAccept(prepared));
    }
    if !prompt.can_prompt() {
        return Err(AcceptError::ConfirmationRequired);
    }
    match prompt
        .confirm_default_no("Accept this request and pay its requester?")
        .map_err(|source| AcceptError::Confirmation { source })?
    {
        true => Ok(AuthorizedAccept(prepared)),
        false => Err(AcceptError::ConfirmationDeclined),
    }
}

pub(crate) async fn execute<A>(
    api: &A,
    authorized: AuthorizedAccept,
) -> Result<AcceptResult, AcceptError>
where
    A: RequestAcceptanceApi,
{
    let AuthorizedAccept(prepared) = authorized;
    let accepted = api
        .accept_request(
            prepared.credential.access_token(),
            prepared.credential.device_id(),
            &prepared.plan,
        )
        .await
        .map_err(|source| AcceptError::Accept {
            kind: source.kind(),
            source: OperationFailure::new(source),
        })?;
    Ok(AcceptResult::new(prepared.plan, accepted))
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
        UserSearchPage, UserSearchPageRequest,
    };
    use crate::domain::{
        AccessToken, Account, AccountPassword, Balance, DeviceId, FinancialStatus, LoginIdentifier,
        Money, OtpCode, PaymentId, PendingRequest, PendingRequestAction, RequestDirection,
        RequestStatus, SignedUsdAmount, User, UserId, UserProfileKind, UserSearchQuery, Username,
    };

    type TestResult = Result<(), Box<dyn Error>>;

    #[derive(Clone, Copy, Debug, thiserror::Error)]
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

    struct FakeReader;

    impl CredentialReader for FakeReader {
        type Error = FakeCredentialError;

        fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
            Ok(Some(LoadedCredential {
                envelope: credential()?,
                format: CredentialFormat::Version1,
            }))
        }
    }

    struct FakeApi {
        account: Account,
        user: User,
        request: PendingRequest,
        balance: Balance,
        accepted: Result<AcceptedRequest, FakeApiError>,
        account_calls: Cell<usize>,
        lookup_calls: Cell<usize>,
        user_calls: Cell<usize>,
        balance_calls: Cell<usize>,
        accept_calls: Cell<usize>,
        lookup_arguments_match: Cell<bool>,
        user_argument_matches: Cell<bool>,
        accept_plan_matches: Cell<bool>,
    }

    impl AuthenticationApi for FakeApi {
        type Error = FakeApiError;

        fn current_account<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
        ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a {
            self.account_calls.set(self.account_calls.get() + 1);
            ready(Ok(self.account.clone()))
        }

        fn revoke_access_token<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
        ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
            ready(Err(FakeApiError(ApiFailureKind::Internal)))
        }
    }

    impl RequestLookupApi for FakeApi {
        type Error = FakeApiError;

        fn pending_request_by_id<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            current_user_id: &'a UserId,
            request_id: &'a RequestId,
        ) -> impl Future<Output = Result<PendingRequest, Self::Error>> + Send + 'a {
            self.lookup_calls.set(self.lookup_calls.get() + 1);
            self.lookup_arguments_match
                .set(current_user_id == self.account.user_id() && request_id == self.request.id());
            ready(Ok(self.request.clone()))
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
            ready(Ok(self.balance.clone()))
        }
    }

    impl UsersApi for FakeApi {
        type Error = FakeApiError;

        fn user_by_id<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            user_id: &'a UserId,
        ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a {
            self.user_calls.set(self.user_calls.get() + 1);
            self.user_argument_matches
                .set(user_id == self.request.counterparty().user_id());
            ready(Ok(self.user.clone()))
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

    impl RequestAcceptanceApi for FakeApi {
        type Error = FakeApiError;

        fn accept_request<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            plan: &'a AcceptRequestPlan,
        ) -> impl Future<Output = Result<AcceptedRequest, Self::Error>> + Send + 'a {
            self.accept_calls.set(self.accept_calls.get() + 1);
            self.accept_plan_matches.set(
                plan.account().user_id() == self.account.user_id()
                    && plan.request().id() == self.request.id()
                    && plan.request().counterparty().user_id()
                        == self.request.counterparty().user_id(),
            );
            ready(self.accepted.clone())
        }
    }

    struct FakePrompt {
        interactive: bool,
        confirmed: bool,
    }

    impl PromptPort for FakePrompt {
        fn can_prompt(&self) -> bool {
            self.interactive
        }

        fn read_login_identifier(&self, _prompt: &str) -> Result<LoginIdentifier, PromptError> {
            Err(PromptError::NotInteractive)
        }

        fn read_account_password(&self, _prompt: &str) -> Result<AccountPassword, PromptError> {
            Err(PromptError::NotInteractive)
        }

        fn read_otp_code(&self, _prompt: &str) -> Result<OtpCode, PromptError> {
            Err(PromptError::NotInteractive)
        }

        fn read_access_token(&self, _prompt: &str) -> Result<AccessToken, PromptError> {
            Err(PromptError::NotInteractive)
        }

        fn read_device_id(&self, _prompt: &str) -> Result<DeviceId, PromptError> {
            Err(PromptError::NotInteractive)
        }

        fn confirm_default_no(&self, _prompt: &str) -> Result<bool, PromptError> {
            Ok(self.confirmed)
        }

        fn select(&self, _prompt: &str, _items: &[String]) -> Result<usize, PromptError> {
            Err(PromptError::NotInteractive)
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn prepare_authorize_execute_performs_exactly_one_acceptance() -> TestResult {
        let api = fake_api(incoming_request(
            RequestDirection::Incoming,
            "pending",
            "private",
        )?)?;
        let request_id = RequestId::from_str("request-1")?;
        let prepared = prepare(&FakeReader, &api, &request_id).await?;

        assert_eq!(api.accept_calls.get(), 0);
        let authorized = authorize(
            &FakePrompt {
                interactive: false,
                confirmed: false,
            },
            prepared,
            true,
        )?;
        let result = execute(&api, authorized).await?;

        assert_eq!(result.accepted().payment_id().as_str(), "request-1");
        assert_eq!(api.account_calls.get(), 1);
        assert_eq!(api.lookup_calls.get(), 1);
        assert_eq!(api.user_calls.get(), 1);
        assert_eq!(api.balance_calls.get(), 1);
        assert_eq!(api.accept_calls.get(), 1);
        assert!(api.lookup_arguments_match.get());
        assert!(api.user_argument_matches.get());
        assert!(api.accept_plan_matches.get());
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn unsafe_request_or_balance_stops_before_acceptance() -> TestResult {
        for (request, balance_cents) in [
            (
                incoming_request(RequestDirection::Outgoing, "pending", "private")?,
                1,
            ),
            (
                incoming_request(RequestDirection::Incoming, "held", "private")?,
                1,
            ),
            (
                incoming_request(RequestDirection::Incoming, "cancelled", "private")?,
                1,
            ),
            (
                incoming_request(RequestDirection::Incoming, "pending", "public")?,
                1,
            ),
            (
                incoming_request(RequestDirection::Incoming, "pending", "private")?,
                0,
            ),
            (
                incoming_request(RequestDirection::Incoming, "pending", "private")?.with_note(None),
                1,
            ),
            (
                incoming_request(RequestDirection::Incoming, "pending", "private")?
                    .with_audience(None),
                1,
            ),
            (
                incoming_request(RequestDirection::Incoming, "pending", "private")?
                    .with_action(PendingRequestAction::Pay),
                1,
            ),
        ] {
            let mut api = fake_api(request)?;
            api.balance = Balance::new(
                SignedUsdAmount::from_cents(balance_cents),
                SignedUsdAmount::from_cents(0),
            );
            let result = prepare(&FakeReader, &api, &RequestId::from_str("request-1")?).await;

            assert!(result.is_err());
            assert_eq!(api.accept_calls.get(), 0);
        }

        let mut mismatch = fake_api(incoming_request(
            RequestDirection::Incoming,
            "pending",
            "private",
        )?)?;
        mismatch.user = User::new(
            UserId::from_str("999")?,
            Some(Username::from_bare("requester")?),
            Some("Different identity".to_owned()),
        )
        .with_financial_attributes(UserProfileKind::Personal, true);
        let mismatch_result =
            prepare(&FakeReader, &mismatch, &RequestId::from_str("request-1")?).await;
        assert!(matches!(
            mismatch_result,
            Err(AcceptError::RequesterIdentityMismatch)
        ));
        assert_eq!(mismatch.accept_calls.get(), 0);

        let mut business = fake_api(incoming_request(
            RequestDirection::Incoming,
            "pending",
            "private",
        )?)?;
        business.user = User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("requester")?),
            Some("Synthetic business".to_owned()),
        )
        .with_financial_attributes(UserProfileKind::Business, true);
        let business_result =
            prepare(&FakeReader, &business, &RequestId::from_str("request-1")?).await;
        assert!(matches!(
            business_result,
            Err(AcceptError::AccountOrRequester(
                FinancialValidationError::UnsupportedProfileType
            ))
        ));
        assert_eq!(business.accept_calls.get(), 0);

        let mut incomplete = fake_api(incoming_request(
            RequestDirection::Incoming,
            "pending",
            "private",
        )?)?;
        incomplete.user = User::new(
            UserId::from_str("456")?,
            None,
            Some("Synthetic requester".to_owned()),
        )
        .with_financial_attributes(UserProfileKind::Personal, true);
        let incomplete_result =
            prepare(&FakeReader, &incomplete, &RequestId::from_str("request-1")?).await;
        assert!(matches!(
            incomplete_result,
            Err(AcceptError::RequestValidation(
                RequestMutationValidationError::IncompleteRequester
            ))
        ));
        assert_eq!(incomplete.accept_calls.get(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn confirmation_and_ambiguity_never_retry() -> TestResult {
        let api = fake_api(incoming_request(
            RequestDirection::Incoming,
            "pending",
            "private",
        )?)?;
        let request_id = RequestId::from_str("request-1")?;
        let prepared = prepare(&FakeReader, &api, &request_id).await?;
        let result = authorize(
            &FakePrompt {
                interactive: false,
                confirmed: false,
            },
            prepared,
            false,
        );
        assert!(matches!(result, Err(AcceptError::ConfirmationRequired)));
        assert_eq!(api.accept_calls.get(), 0);

        let mut ambiguous = fake_api(incoming_request(
            RequestDirection::Incoming,
            "pending",
            "private",
        )?)?;
        ambiguous.accepted = Err(FakeApiError(ApiFailureKind::AmbiguousWrite));
        let prepared = prepare(&FakeReader, &ambiguous, &request_id).await?;
        let authorized = authorize(
            &FakePrompt {
                interactive: true,
                confirmed: true,
            },
            prepared,
            false,
        )?;
        let result = execute(&ambiguous, authorized).await;
        assert!(matches!(
            result,
            Err(AcceptError::Accept {
                kind: ApiFailureKind::AmbiguousWrite,
                ..
            })
        ));
        assert_eq!(ambiguous.accept_calls.get(), 1);
        Ok(())
    }

    fn fake_api(request: PendingRequest) -> Result<FakeApi, Box<dyn Error>> {
        Ok(FakeApi {
            account: account()?,
            user: financial_requester()?,
            request,
            balance: Balance::new(
                SignedUsdAmount::from_cents(1),
                SignedUsdAmount::from_cents(0),
            ),
            accepted: Ok(AcceptedRequest::new(
                PaymentId::from_str("request-1")?,
                FinancialStatus::Settled,
            )),
            account_calls: Cell::new(0),
            lookup_calls: Cell::new(0),
            user_calls: Cell::new(0),
            balance_calls: Cell::new(0),
            accept_calls: Cell::new(0),
            lookup_arguments_match: Cell::new(false),
            user_argument_matches: Cell::new(false),
            accept_plan_matches: Cell::new(false),
        })
    }

    fn credential() -> Result<CredentialEnvelope, FakeCredentialError> {
        Ok(CredentialEnvelope::new(
            AccessToken::from_str("synthetic-token").map_err(|_| FakeCredentialError)?,
            DeviceId::from_str("synthetic-device").map_err(|_| FakeCredentialError)?,
            UserId::from_str("123").map_err(|_| FakeCredentialError)?,
            Username::from_bare("owner").map_err(|_| FakeCredentialError)?,
            Some("Synthetic owner".to_owned()),
            OffsetDateTime::UNIX_EPOCH,
        ))
    }

    fn account() -> Result<Account, Box<dyn Error>> {
        Ok(Account::new(
            UserId::from_str("123")?,
            Username::from_bare("owner")?,
            Some("Synthetic owner".to_owned()),
        ))
    }

    fn incoming_request(
        direction: RequestDirection,
        status: &str,
        audience: &str,
    ) -> Result<PendingRequest, Box<dyn Error>> {
        Ok(PendingRequest::new(
            RequestId::from_str("request-1")?,
            direction,
            financial_requester()?,
            Money::from_cents(1)?,
            Some("Synthetic request".to_owned()),
            Some(OffsetDateTime::UNIX_EPOCH),
            RequestStatus::from_str(status)?,
        )
        .with_audience(Some(audience.to_owned())))
    }

    fn financial_requester() -> Result<User, Box<dyn Error>> {
        Ok(User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("requester")?),
            Some("Synthetic requester".to_owned()),
        )
        .with_financial_attributes(UserProfileKind::Personal, true))
    }
}

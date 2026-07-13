use thiserror::Error;

use super::auth::OperationFailure;
use super::financial::{FinancialValidationError, validate_account, validate_recipient};
use super::ports::{
    ApiFailure, ApiFailureKind, AuthenticationApi, ClientRequestIdGenerator, CredentialReader,
    RequestCreationApi, UsersApi,
};
use super::recipients::{self, RecipientResolutionError};
use crate::domain::{
    CreateRequestPlan, CreatedPayment, CredentialEnvelope, Money, Note, RecipientInput,
};

#[derive(Debug)]
pub struct PreparedRequest {
    credential: CredentialEnvelope,
    plan: CreateRequestPlan,
}

impl PreparedRequest {
    #[must_use]
    pub(crate) const fn new(credential: CredentialEnvelope, plan: CreateRequestPlan) -> Self {
        Self { credential, plan }
    }

    #[must_use]
    pub const fn plan(&self) -> &CreateRequestPlan {
        &self.plan
    }
}

#[derive(Debug)]
pub struct RequestCreateResult {
    plan: CreateRequestPlan,
    created: CreatedPayment,
}

impl RequestCreateResult {
    #[must_use]
    pub(crate) const fn new(plan: CreateRequestPlan, created: CreatedPayment) -> Self {
        Self { plan, created }
    }

    #[must_use]
    pub const fn plan(&self) -> &CreateRequestPlan {
        &self.plan
    }

    #[must_use]
    pub const fn created(&self) -> &CreatedPayment {
        &self.created
    }
}

#[derive(Debug, Error)]
pub enum RequestCreateError {
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
    #[error("failed to create the Venmo request: {source}")]
    Create {
        kind: ApiFailureKind,
        #[source]
        source: OperationFailure,
    },
}

impl RequestCreateError {
    #[must_use]
    pub const fn api_failure_kind(&self) -> Option<ApiFailureKind> {
        match self {
            Self::CurrentAccount { kind, .. } | Self::Create { kind, .. } => Some(*kind),
            Self::Recipient(error) => match error.failure_kind() {
                recipients::RecipientResolutionFailureKind::Api(kind) => Some(kind),
                _ => None,
            },
            Self::MissingCredential | Self::CredentialLoad { .. } | Self::Validation(_) => None,
        }
    }
}

pub(crate) async fn prepare<R, A, G>(
    credentials: &R,
    api: &A,
    generator: &G,
    recipient: &RecipientInput,
    amount: Money,
    note: Note,
) -> Result<PreparedRequest, RequestCreateError>
where
    R: CredentialReader,
    A: AuthenticationApi + UsersApi,
    <A as AuthenticationApi>::Error: ApiFailure,
    G: ClientRequestIdGenerator,
{
    let loaded = credentials
        .read_credential()
        .map_err(|source| RequestCreateError::CredentialLoad {
            source: OperationFailure::new(source),
        })?
        .ok_or(RequestCreateError::MissingCredential)?;
    let credential = loaded.envelope;
    let account = api
        .current_account(credential.access_token(), credential.device_id())
        .await
        .map_err(|source| RequestCreateError::CurrentAccount {
            kind: source.kind(),
            source: OperationFailure::new(source),
        })?;
    validate_account(&credential, &account)?;
    let resolved = recipients::resolve_with_credential(&credential, api, recipient).await?;
    let recipient = resolved.into_user();
    validate_recipient(&account, &recipient)?;
    let plan = CreateRequestPlan::new(generator.generate(), account, recipient, amount, note);
    Ok(PreparedRequest::new(credential, plan))
}

pub(crate) async fn execute<A>(
    api: &A,
    prepared: PreparedRequest,
) -> Result<RequestCreateResult, RequestCreateError>
where
    A: RequestCreationApi,
{
    let created = api
        .create_request(
            prepared.credential.access_token(),
            prepared.credential.device_id(),
            &prepared.plan,
        )
        .await
        .map_err(|source| RequestCreateError::Create {
            kind: source.kind(),
            source: OperationFailure::new(source),
        })?;
    Ok(RequestCreateResult::new(prepared.plan, created))
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
        AccessToken, Account, ClientRequestId, DeviceId, FinancialStatus, PaymentId, User, UserId,
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

    struct FakeReader;

    impl CredentialReader for FakeReader {
        type Error = FakeCredentialError;

        fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
            Ok(Some(LoadedCredential {
                envelope: CredentialEnvelope::new(
                    AccessToken::from_str("synthetic-token").map_err(|_| FakeCredentialError)?,
                    DeviceId::from_str("synthetic-device").map_err(|_| FakeCredentialError)?,
                    UserId::from_str("123").map_err(|_| FakeCredentialError)?,
                    Username::from_bare("owner").map_err(|_| FakeCredentialError)?,
                    Some("Synthetic owner".to_owned()),
                    OffsetDateTime::UNIX_EPOCH,
                ),
                format: CredentialFormat::Version1,
            }))
        }
    }

    struct FakeApi {
        account: Account,
        user: User,
        creation: Result<CreatedPayment, FakeApiError>,
        account_calls: Cell<usize>,
        user_calls: Cell<usize>,
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

    impl UsersApi for FakeApi {
        type Error = FakeApiError;

        fn user_by_id<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            _user_id: &'a UserId,
        ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a {
            self.user_calls.set(self.user_calls.get() + 1);
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

    impl RequestCreationApi for FakeApi {
        type Error = FakeApiError;

        fn create_request<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            _plan: &'a CreateRequestPlan,
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

    #[tokio::test(flavor = "current_thread")]
    async fn prepare_then_execute_performs_one_request_write() -> TestResult {
        let api = fake_api(financial_user()?)?;
        let recipient = RecipientInput::from_str("456")?;
        let prepared = prepare(
            &FakeReader,
            &api,
            &FixedGenerator,
            &recipient,
            Money::from_cents(1)?,
            Note::from_str("Synthetic note")?,
        )
        .await?;

        assert_eq!(api.creation_calls.get(), 0);
        let result = execute(&api, prepared).await?;
        assert_eq!(result.created().id().as_str(), "request-1");
        assert_eq!(api.account_calls.get(), 1);
        assert_eq!(api.user_calls.get(), 1);
        assert_eq!(api.creation_calls.get(), 1);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn unsupported_recipient_stops_before_request_write() -> TestResult {
        let business = User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("business")?),
            None,
        )
        .with_financial_attributes(UserProfileKind::Business, true);
        let api = fake_api(business)?;
        let recipient = RecipientInput::from_str("456")?;

        let result = prepare(
            &FakeReader,
            &api,
            &FixedGenerator,
            &recipient,
            Money::from_cents(1)?,
            Note::from_str("Synthetic note")?,
        )
        .await;

        assert!(matches!(
            result,
            Err(RequestCreateError::Validation(
                FinancialValidationError::UnsupportedProfileType
            ))
        ));
        assert_eq!(api.creation_calls.get(), 0);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn ambiguous_request_write_is_preserved_without_retry() -> TestResult {
        let mut api = fake_api(financial_user()?)?;
        api.creation = Err(FakeApiError(ApiFailureKind::AmbiguousWrite));
        let recipient = RecipientInput::from_str("456")?;
        let prepared = prepare(
            &FakeReader,
            &api,
            &FixedGenerator,
            &recipient,
            Money::from_cents(1)?,
            Note::from_str("Synthetic note")?,
        )
        .await?;

        let result = execute(&api, prepared).await;

        assert!(matches!(
            result,
            Err(RequestCreateError::Create {
                kind: ApiFailureKind::AmbiguousWrite,
                ..
            })
        ));
        assert_eq!(api.creation_calls.get(), 1);
        Ok(())
    }

    fn fake_api(user: User) -> Result<FakeApi, Box<dyn Error>> {
        Ok(FakeApi {
            account: Account::new(
                UserId::from_str("123")?,
                Username::from_bare("owner")?,
                Some("Synthetic owner".to_owned()),
            ),
            user,
            creation: Ok(CreatedPayment::new(
                PaymentId::from_str("request-1")?,
                FinancialStatus::Pending,
            )),
            account_calls: Cell::new(0),
            user_calls: Cell::new(0),
            creation_calls: Cell::new(0),
        })
    }

    fn financial_user() -> Result<User, Box<dyn Error>> {
        Ok(User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("bob")?),
            Some("Synthetic recipient".to_owned()),
        )
        .with_financial_attributes(UserProfileKind::Personal, true))
    }
}

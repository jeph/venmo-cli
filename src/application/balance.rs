use thiserror::Error;

use super::auth::OperationFailure;
use super::ports::{ApiFailure, ApiFailureKind, BalanceApi, CredentialStore};
use super::read::ReadFailureKind;
use crate::domain::Balance;

#[derive(Debug)]
pub struct BalanceResult {
    balance: Balance,
}

impl BalanceResult {
    #[must_use]
    pub(crate) const fn new(balance: Balance) -> Self {
        Self { balance }
    }

    #[must_use]
    pub const fn balance(&self) -> &Balance {
        &self.balance
    }
}

#[derive(Debug, Error)]
pub enum BalanceError {
    #[error("no Venmo credential is stored; run `venmo auth login`")]
    MissingCredential,

    #[error("failed to read the OS credential entry: {source}")]
    CredentialLoad {
        #[source]
        source: OperationFailure,
    },

    #[error("failed to read the Venmo wallet balance: {source}")]
    Api {
        kind: ApiFailureKind,
        #[source]
        source: OperationFailure,
    },
}

impl BalanceError {
    #[must_use]
    pub const fn failure_kind(&self) -> ReadFailureKind {
        match self {
            Self::MissingCredential | Self::CredentialLoad { .. } => ReadFailureKind::Credential,
            Self::Api { kind, .. } => ReadFailureKind::Api(*kind),
        }
    }
}

pub async fn get<S, A>(store: &S, api: &A) -> Result<BalanceResult, BalanceError>
where
    S: CredentialStore,
    A: BalanceApi,
{
    let loaded = store
        .load()
        .map_err(|source| BalanceError::CredentialLoad {
            source: OperationFailure::new(source),
        })?
        .ok_or(BalanceError::MissingCredential)?;
    let balance = api
        .balance(loaded.envelope.access_token(), loaded.envelope.device_id())
        .await
        .map_err(|source| BalanceError::Api {
            kind: source.kind(),
            source: OperationFailure::new(source),
        })?;
    Ok(BalanceResult::new(balance))
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
        CredentialDeleteOutcome, CredentialFailureKind, CredentialFormat, CredentialStoreFailure,
        LoadedCredential,
    };
    use crate::domain::{
        AccessToken, CredentialEnvelope, DeviceId, SignedUsdAmount, UserId, Username,
    };

    type TestResult = Result<(), Box<dyn Error>>;

    #[derive(Clone, Copy)]
    enum StoreState {
        Present,
        Missing,
        Unreadable,
    }

    struct FakeCredentialStore {
        state: StoreState,
        load_calls: Cell<usize>,
    }

    impl FakeCredentialStore {
        fn new(state: StoreState) -> Self {
            Self {
                state,
                load_calls: Cell::new(0),
            }
        }
    }

    #[derive(Debug, thiserror::Error)]
    #[error("fake credential store failure")]
    struct FakeCredentialError;

    impl CredentialStoreFailure for FakeCredentialError {
        fn kind(&self) -> CredentialFailureKind {
            CredentialFailureKind::Unavailable
        }
    }

    impl CredentialStore for FakeCredentialStore {
        type Error = FakeCredentialError;

        fn load(&self) -> Result<Option<LoadedCredential>, Self::Error> {
            self.load_calls.set(self.load_calls.get() + 1);
            match self.state {
                StoreState::Present => Ok(Some(loaded_credential()?)),
                StoreState::Missing => Ok(None),
                StoreState::Unreadable => Err(FakeCredentialError),
            }
        }

        fn save(&self, _credential: &CredentialEnvelope) -> Result<(), Self::Error> {
            Err(FakeCredentialError)
        }

        fn delete(&self) -> Result<CredentialDeleteOutcome, Self::Error> {
            Err(FakeCredentialError)
        }
    }

    #[derive(Clone, Copy, Debug, thiserror::Error)]
    #[error("fake API failure")]
    struct FakeApiError(ApiFailureKind);

    impl ApiFailure for FakeApiError {
        fn kind(&self) -> ApiFailureKind {
            self.0
        }
    }

    struct FakeBalanceApi {
        result: Result<Balance, FakeApiError>,
        calls: Cell<usize>,
    }

    impl FakeBalanceApi {
        fn new(result: Result<Balance, FakeApiError>) -> Self {
            Self {
                result,
                calls: Cell::new(0),
            }
        }
    }

    impl BalanceApi for FakeBalanceApi {
        type Error = FakeApiError;

        fn balance<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
        ) -> impl Future<Output = Result<Balance, Self::Error>> + Send + 'a {
            self.calls.set(self.calls.get() + 1);
            ready(self.result.clone())
        }
    }

    fn loaded_credential() -> Result<LoadedCredential, FakeCredentialError> {
        Ok(LoadedCredential {
            envelope: CredentialEnvelope::new(
                AccessToken::from_str("synthetic-token").map_err(|_| FakeCredentialError)?,
                DeviceId::from_str("synthetic-device").map_err(|_| FakeCredentialError)?,
                UserId::from_str("1000").map_err(|_| FakeCredentialError)?,
                Username::from_bare("tester".to_owned()).map_err(|_| FakeCredentialError)?,
                Some("Test User".to_owned()),
                OffsetDateTime::UNIX_EPOCH,
            ),
            format: CredentialFormat::Version1,
        })
    }

    fn balance(available_cents: i64, on_hold_cents: i64) -> Balance {
        Balance::new(
            SignedUsdAmount::from_cents(available_cents),
            SignedUsdAmount::from_cents(on_hold_cents),
        )
    }

    #[tokio::test(flavor = "current_thread")]
    async fn get_returns_the_typed_balance() -> TestResult {
        let store = FakeCredentialStore::new(StoreState::Present);
        let api = FakeBalanceApi::new(Ok(balance(12_345, -67)));

        let result = get(&store, &api).await?;

        assert_eq!(result.balance().available().cents(), 12_345);
        assert_eq!(result.balance().on_hold().cents(), -67);
        assert_eq!(api.calls.get(), 1);
        assert_eq!(store.load_calls.get(), 1);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn missing_or_unreadable_credentials_make_no_api_call() -> TestResult {
        let missing_store = FakeCredentialStore::new(StoreState::Missing);
        let missing_api = FakeBalanceApi::new(Ok(balance(0, 0)));

        let missing = get(&missing_store, &missing_api).await;

        assert!(matches!(missing, Err(BalanceError::MissingCredential)));
        assert_eq!(missing_api.calls.get(), 0);
        assert_eq!(missing_store.load_calls.get(), 1);

        let unreadable_store = FakeCredentialStore::new(StoreState::Unreadable);
        let unreadable_api = FakeBalanceApi::new(Ok(balance(0, 0)));

        let unreadable = get(&unreadable_store, &unreadable_api).await;

        assert!(matches!(
            unreadable,
            Err(BalanceError::CredentialLoad { .. })
        ));
        assert_eq!(unreadable_api.calls.get(), 0);
        assert_eq!(unreadable_store.load_calls.get(), 1);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn api_failure_kind_is_preserved() -> TestResult {
        let store = FakeCredentialStore::new(StoreState::Present);
        let api = FakeBalanceApi::new(Err(FakeApiError(ApiFailureKind::Timeout)));

        let result = get(&store, &api).await;

        assert_eq!(
            result.as_ref().err().map(BalanceError::failure_kind),
            Some(ReadFailureKind::Api(ApiFailureKind::Timeout))
        );
        assert!(matches!(result, Err(BalanceError::Api { .. })));
        assert_eq!(api.calls.get(), 1);
        Ok(())
    }
}

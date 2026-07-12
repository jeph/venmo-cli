use thiserror::Error;

use super::auth::OperationFailure;
use super::ports::{ApiFailure, ApiFailureKind, CredentialStore, PaymentMethodsApi};
use crate::domain::PaymentMethod;

#[derive(Debug)]
pub struct PaymentMethodsResult {
    methods: Vec<PaymentMethod>,
}

impl PaymentMethodsResult {
    #[must_use]
    pub(crate) fn from_methods(methods: Vec<PaymentMethod>) -> Self {
        Self { methods }
    }

    #[must_use]
    pub fn methods(&self) -> &[PaymentMethod] {
        &self.methods
    }
}

#[derive(Debug, Error)]
pub enum PaymentMethodsError {
    #[error("no Venmo credential is stored; run `venmo auth login`")]
    MissingCredential,

    #[error("failed to read the OS credential entry: {source}")]
    CredentialLoad {
        #[source]
        source: OperationFailure,
    },

    #[error("failed to list Venmo payment methods: {source}")]
    Api {
        kind: ApiFailureKind,
        #[source]
        source: OperationFailure,
    },
}

impl PaymentMethodsError {
    #[must_use]
    pub const fn api_failure_kind(&self) -> Option<ApiFailureKind> {
        match self {
            Self::Api { kind, .. } => Some(*kind),
            Self::MissingCredential | Self::CredentialLoad { .. } => None,
        }
    }
}

pub async fn list<S, A>(store: &S, api: &A) -> Result<PaymentMethodsResult, PaymentMethodsError>
where
    S: CredentialStore,
    A: PaymentMethodsApi,
{
    let loaded = store
        .load()
        .map_err(|source| PaymentMethodsError::CredentialLoad {
            source: OperationFailure::new(source),
        })?
        .ok_or(PaymentMethodsError::MissingCredential)?;
    let methods = api
        .payment_methods(loaded.envelope.access_token(), loaded.envelope.device_id())
        .await
        .map_err(|source| PaymentMethodsError::Api {
            kind: source.kind(),
            source: OperationFailure::new(source),
        })?;

    Ok(PaymentMethodsResult::from_methods(methods))
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
        AccessToken, CredentialEnvelope, DeviceId, PaymentMethodId, UserId, Username,
    };

    type TestResult = Result<(), Box<dyn Error>>;

    struct FakeStore {
        has_credential: bool,
        fail_load: bool,
    }

    impl CredentialStore for FakeStore {
        type Error = FakeCredentialError;

        fn load(&self) -> Result<Option<LoadedCredential>, Self::Error> {
            if self.fail_load {
                return Err(FakeCredentialError);
            }
            if self.has_credential {
                loaded_credential().map(Some)
            } else {
                Ok(None)
            }
        }

        fn save(&self, _credential: &CredentialEnvelope) -> Result<(), Self::Error> {
            Err(FakeCredentialError)
        }

        fn delete(&self) -> Result<CredentialDeleteOutcome, Self::Error> {
            Err(FakeCredentialError)
        }
    }

    #[derive(Debug, Error)]
    #[error("synthetic credential failure")]
    struct FakeCredentialError;

    impl CredentialStoreFailure for FakeCredentialError {
        fn kind(&self) -> CredentialFailureKind {
            CredentialFailureKind::Internal
        }
    }

    struct FakeApi {
        calls: Cell<u32>,
        result: Result<Vec<PaymentMethod>, FakeApiError>,
    }

    impl PaymentMethodsApi for FakeApi {
        type Error = FakeApiError;

        fn payment_methods<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
        ) -> impl Future<Output = Result<Vec<PaymentMethod>, Self::Error>> + Send + 'a {
            self.calls.set(self.calls.get() + 1);
            ready(self.result.clone())
        }
    }

    #[derive(Clone, Debug, Error)]
    #[error("synthetic API failure")]
    struct FakeApiError(ApiFailureKind);

    impl ApiFailure for FakeApiError {
        fn kind(&self) -> ApiFailureKind {
            self.0
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn list_loads_the_credential_and_returns_typed_methods() -> TestResult {
        let api = FakeApi {
            calls: Cell::new(0),
            result: Ok(vec![payment_method("method-1")?]),
        };
        let store = FakeStore {
            has_credential: true,
            fail_load: false,
        };

        let result = list(&store, &api).await?;

        assert_eq!(result.methods().len(), 1);
        assert_eq!(
            result.methods().first().map(|method| method.id().as_str()),
            Some("method-1")
        );
        assert_eq!(api.calls.get(), 1);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn missing_or_unreadable_credentials_make_no_api_call() -> TestResult {
        for store in [
            FakeStore {
                has_credential: false,
                fail_load: false,
            },
            FakeStore {
                has_credential: false,
                fail_load: true,
            },
        ] {
            let api = FakeApi {
                calls: Cell::new(0),
                result: Ok(Vec::new()),
            };

            let result = list(&store, &api).await;

            assert!(result.is_err());
            assert_eq!(api.calls.get(), 0);
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn api_failure_kind_is_preserved() -> TestResult {
        let api = FakeApi {
            calls: Cell::new(0),
            result: Err(FakeApiError(ApiFailureKind::Timeout)),
        };
        let store = FakeStore {
            has_credential: true,
            fail_load: false,
        };

        let error = list(&store, &api).await;

        assert!(matches!(
            error,
            Err(PaymentMethodsError::Api {
                kind: ApiFailureKind::Timeout,
                ..
            })
        ));
        Ok(())
    }

    fn loaded_credential() -> Result<LoadedCredential, FakeCredentialError> {
        Ok(LoadedCredential {
            envelope: CredentialEnvelope::new(
                AccessToken::from_str("synthetic-token").map_err(|_| FakeCredentialError)?,
                DeviceId::from_str("synthetic-device").map_err(|_| FakeCredentialError)?,
                UserId::from_str("123").map_err(|_| FakeCredentialError)?,
                Username::from_bare("alice".to_owned()).map_err(|_| FakeCredentialError)?,
                Some("Alice".to_owned()),
                OffsetDateTime::UNIX_EPOCH,
            ),
            format: CredentialFormat::Version1,
        })
    }

    fn payment_method(id: &str) -> Result<PaymentMethod, Box<dyn Error>> {
        Ok(PaymentMethod::new(
            PaymentMethodId::from_str(id)?,
            Some("Synthetic method".to_owned()),
            Some("bank".to_owned()),
            Some("1234".to_owned()),
            true,
        ))
    }
}

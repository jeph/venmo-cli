use thiserror::Error;

use super::{PaymentMethod, PaymentMethodsApi};
use crate::shared::{
    ApiOperationFailure, ApplicationFailureKind, CredentialAccessError, CredentialReader,
    require_credential,
};

#[derive(Debug, Eq, PartialEq)]
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
    #[error(transparent)]
    Credential(#[from] CredentialAccessError),

    #[error("failed to list Venmo payment methods: {source}")]
    Api {
        #[source]
        source: ApiOperationFailure,
    },
}

impl PaymentMethodsError {
    #[must_use]
    pub const fn failure_kind(&self) -> ApplicationFailureKind {
        match self {
            Self::Credential(_) => ApplicationFailureKind::Credential,
            Self::Api { source } => ApplicationFailureKind::Api(source.kind()),
        }
    }
}

pub async fn list<R, A>(
    credentials: &R,
    api: &A,
) -> Result<PaymentMethodsResult, PaymentMethodsError>
where
    R: CredentialReader,
    A: PaymentMethodsApi,
{
    let loaded = require_credential(credentials)?;
    let methods = api
        .payment_methods(loaded.envelope.access_token(), loaded.envelope.device_id())
        .await
        .map_err(|source| PaymentMethodsError::Api {
            source: ApiOperationFailure::new(source),
        })?;

    Ok(PaymentMethodsResult::from_methods(methods))
}

#[cfg(test)]
#[path = "payment_methods_tests.rs"]
mod tests;

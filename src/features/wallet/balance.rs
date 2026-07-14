use thiserror::Error;

use super::{Balance, BalanceApi};
use crate::shared::{
    ApiOperationFailure, CredentialAccessError, CredentialReader, ReadFailureKind,
    require_credential,
};

#[derive(Debug, Eq, PartialEq)]
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
    #[error(transparent)]
    Credential(#[from] CredentialAccessError),

    #[error("failed to read the Venmo wallet balance: {source}")]
    Api {
        #[source]
        source: ApiOperationFailure,
    },
}

impl BalanceError {
    #[must_use]
    pub const fn failure_kind(&self) -> ReadFailureKind {
        match self {
            Self::Credential(_) => ReadFailureKind::Credential,
            Self::Api { source } => ReadFailureKind::Api(source.kind()),
        }
    }
}

pub async fn get<R, A>(credentials: &R, api: &A) -> Result<BalanceResult, BalanceError>
where
    R: CredentialReader,
    A: BalanceApi,
{
    let loaded = require_credential(credentials)?;
    let balance = api
        .balance(loaded.envelope.access_token(), loaded.envelope.device_id())
        .await
        .map_err(|source| BalanceError::Api {
            source: ApiOperationFailure::new(source),
        })?;
    Ok(BalanceResult::new(balance))
}

#[cfg(test)]
#[path = "balance_tests.rs"]
mod tests;

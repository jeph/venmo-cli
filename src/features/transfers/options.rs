use thiserror::Error;

use super::{TransferOptions, TransferOptionsApi};
use crate::shared::{
    ApiOperationFailure, CredentialAccessError, CredentialReader, ReadFailureKind,
    require_credential,
};

#[derive(Debug, Eq, PartialEq)]
pub struct TransferOptionsResult {
    options: TransferOptions,
}

impl TransferOptionsResult {
    #[must_use]
    pub(crate) const fn new(options: TransferOptions) -> Self {
        Self { options }
    }

    #[must_use]
    pub const fn options(&self) -> &TransferOptions {
        &self.options
    }
}

#[derive(Debug, Error)]
pub enum TransferOptionsError {
    #[error(transparent)]
    Credential(#[from] CredentialAccessError),

    #[error("failed to read Venmo transfer options: {source}")]
    Api {
        #[source]
        source: ApiOperationFailure,
    },
}

impl TransferOptionsError {
    #[must_use]
    pub const fn failure_kind(&self) -> ReadFailureKind {
        match self {
            Self::Credential(_) => ReadFailureKind::Credential,
            Self::Api { source } => ReadFailureKind::Api(source.kind()),
        }
    }
}

pub async fn get<R, A>(
    credentials: &R,
    api: &A,
) -> Result<TransferOptionsResult, TransferOptionsError>
where
    R: CredentialReader,
    A: TransferOptionsApi,
{
    let loaded = require_credential(credentials)?;
    let options = api
        .transfer_options(loaded.envelope.access_token(), loaded.envelope.device_id())
        .await
        .map_err(|source| TransferOptionsError::Api {
            source: ApiOperationFailure::new(source),
        })?;
    Ok(TransferOptionsResult::new(options))
}

use thiserror::Error;
use time::OffsetDateTime;

use super::CurrentAccountApi;
use crate::shared::{
    Account, ApiOperationFailure, ApplicationFailureKind, CredentialAccessError, CredentialBackend,
    CredentialFormat, CredentialReader, require_credential,
};

#[derive(Debug)]
pub struct AuthStatus {
    account: Account,
    saved_at: OffsetDateTime,
    credential_format: CredentialFormat,
    credential_backend: CredentialBackend,
}

impl AuthStatus {
    #[must_use]
    pub fn account(&self) -> &Account {
        &self.account
    }

    #[must_use]
    pub const fn saved_at(&self) -> OffsetDateTime {
        self.saved_at
    }

    #[must_use]
    pub const fn credential_format(&self) -> CredentialFormat {
        self.credential_format
    }

    #[must_use]
    pub const fn credential_backend(&self) -> CredentialBackend {
        self.credential_backend
    }
}

#[derive(Debug, Error)]
pub enum AuthStatusError {
    #[error(transparent)]
    Credential(#[from] CredentialAccessError),

    #[error("the stored bearer token could not be validated: {source}")]
    TokenValidation {
        #[source]
        source: ApiOperationFailure,
    },

    #[error("the live Venmo account does not match the account stored with this credential")]
    AccountMismatch,
}

impl AuthStatusError {
    #[must_use]
    pub const fn failure_kind(&self) -> ApplicationFailureKind {
        match self {
            Self::Credential(_) | Self::AccountMismatch => ApplicationFailureKind::Credential,
            Self::TokenValidation { source } => ApplicationFailureKind::Api(source.kind()),
        }
    }
}

pub async fn status<R, A>(credentials: &R, api: &A) -> Result<AuthStatus, AuthStatusError>
where
    R: CredentialReader,
    A: CurrentAccountApi,
{
    let loaded = require_credential(credentials)?;
    let account = api
        .current_account(loaded.envelope.access_token(), loaded.envelope.device_id())
        .await
        .map_err(|source| AuthStatusError::TokenValidation {
            source: ApiOperationFailure::new(source),
        })?;
    if account.user_id() != loaded.envelope.user_id() {
        return Err(AuthStatusError::AccountMismatch);
    }

    Ok(AuthStatus {
        account,
        saved_at: loaded.envelope.saved_at(),
        credential_format: loaded.format,
        credential_backend: credentials.credential_backend(),
    })
}

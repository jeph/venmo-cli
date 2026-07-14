use thiserror::Error;

use super::{FinancialValidationError, validate_account, validate_recipient};
use crate::features::auth::CurrentAccountApi;
use crate::features::people::recipients::{self, RecipientResolutionError};
use crate::features::people::{RecipientInput, User, UserLookupApi, UserSearchApi};
use crate::shared::{
    Account, ApiFailure, ApiOperationFailure, ApplicationFailureKind, CredentialAccessError,
    CredentialEnvelope, CredentialReader, require_credential,
};

pub(crate) struct PeerPreflight {
    credential: CredentialEnvelope,
    account: Account,
    recipient: User,
}

impl PeerPreflight {
    pub(crate) fn into_parts(self) -> (CredentialEnvelope, Account, User) {
        (self.credential, self.account, self.recipient)
    }
}

#[derive(Debug, Error)]
pub enum PeerPreflightError {
    #[error(transparent)]
    Credential(#[from] CredentialAccessError),

    #[error("failed to verify the current Venmo account: {source}")]
    CurrentAccount {
        #[source]
        source: ApiOperationFailure,
    },

    #[error(transparent)]
    Validation(#[from] FinancialValidationError),

    #[error(transparent)]
    Recipient(#[from] RecipientResolutionError),
}

impl PeerPreflightError {
    pub(crate) const fn failure_kind(&self) -> ApplicationFailureKind {
        match self {
            Self::Credential(_) => ApplicationFailureKind::Credential,
            Self::CurrentAccount { source } => ApplicationFailureKind::Api(source.kind()),
            Self::Validation(source) => source.failure_kind(),
            Self::Recipient(source) => source.application_failure_kind(),
        }
    }
}

pub(crate) async fn prepare<R, A>(
    credentials: &R,
    api: &A,
    recipient: &RecipientInput,
) -> Result<PeerPreflight, PeerPreflightError>
where
    R: CredentialReader,
    A: CurrentAccountApi + UserLookupApi + UserSearchApi,
    <A as CurrentAccountApi>::Error: ApiFailure,
{
    let credential = require_credential(credentials)?.envelope;
    let account = api
        .current_account(credential.access_token(), credential.device_id())
        .await
        .map_err(|source| PeerPreflightError::CurrentAccount {
            source: ApiOperationFailure::new(source),
        })?;
    validate_account(&credential, &account)?;
    let recipient = recipients::resolve_with_credential(&credential, api, recipient)
        .await?
        .into_user();
    validate_recipient(&account, &recipient)?;
    Ok(PeerPreflight {
        credential,
        account,
        recipient,
    })
}

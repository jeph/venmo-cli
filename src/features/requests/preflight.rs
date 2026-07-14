use thiserror::Error;

use super::validation::{
    RequestMutationValidationError, validate_complete_request, validate_incoming_pending,
    validate_request_id,
};
use super::{RequestId, RequestLookupApi, RequestRecord};
use crate::features::auth::CurrentAccountApi;
use crate::features::payments::{FinancialValidationError, validate_account};
use crate::shared::{
    Account, ApiFailure, ApiOperationFailure, ApplicationFailureKind, CredentialAccessError,
    CredentialEnvelope, CredentialReader, require_credential,
};

pub(crate) struct RequestMutationPreflight {
    credential: CredentialEnvelope,
    account: Account,
    request: RequestRecord,
}

impl RequestMutationPreflight {
    pub(crate) fn into_parts(self) -> (CredentialEnvelope, Account, RequestRecord) {
        (self.credential, self.account, self.request)
    }
}

#[derive(Debug, Error)]
pub enum RequestMutationPreflightError {
    #[error(transparent)]
    Credential(#[from] CredentialAccessError),

    #[error("failed to verify the current Venmo account: {source}")]
    CurrentAccount {
        #[source]
        source: ApiOperationFailure,
    },

    #[error(transparent)]
    Account(#[from] FinancialValidationError),

    #[error("failed to load the authoritative incoming request: {source}")]
    RequestLookup {
        #[source]
        source: ApiOperationFailure,
    },

    #[error(transparent)]
    RequestValidation(#[from] RequestMutationValidationError),
}

impl RequestMutationPreflightError {
    pub(crate) const fn failure_kind(&self) -> ApplicationFailureKind {
        match self {
            Self::Credential(_) => ApplicationFailureKind::Credential,
            Self::CurrentAccount { source } | Self::RequestLookup { source } => {
                ApplicationFailureKind::Api(source.kind())
            }
            Self::Account(source) => source.failure_kind(),
            Self::RequestValidation(source) => source.failure_kind(),
        }
    }
}

pub(crate) async fn prepare<R, A>(
    credentials: &R,
    api: &A,
    request_id: &RequestId,
) -> Result<RequestMutationPreflight, RequestMutationPreflightError>
where
    R: CredentialReader,
    A: CurrentAccountApi + RequestLookupApi,
    <A as CurrentAccountApi>::Error: ApiFailure,
{
    let credential = require_credential(credentials)?.envelope;
    let account = api
        .current_account(credential.access_token(), credential.device_id())
        .await
        .map_err(|source| RequestMutationPreflightError::CurrentAccount {
            source: ApiOperationFailure::new(source),
        })?;
    validate_account(&credential, &account)?;
    let request = api
        .request_by_id(
            credential.access_token(),
            credential.device_id(),
            account.user_id(),
            request_id,
        )
        .await
        .map_err(|source| RequestMutationPreflightError::RequestLookup {
            source: ApiOperationFailure::new(source),
        })?;
    validate_request_id(&request, request_id)?;
    validate_incoming_pending(&request)?;
    validate_complete_request(&request)?;
    Ok(RequestMutationPreflight {
        credential,
        account,
        request,
    })
}

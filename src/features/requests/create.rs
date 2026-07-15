use thiserror::Error;

use super::{CreateRequestPlan, CreatedRequest, RequestCreationApi};
use crate::features::auth::CurrentAccountApi;
use crate::features::payments::PeerPreflightError;
use crate::features::people::{RecipientInput, UserLookupApi, UserSearchApi};
use crate::shared::{
    ApiFailure, ApiOperationFailure, ApplicationFailureKind, ClientRequestIdGenerator,
    CredentialEnvelope, CredentialReader, Money, Note, Visibility,
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
}

#[derive(Debug, Eq, PartialEq)]
pub struct RequestCreateResult {
    plan: CreateRequestPlan,
    created: CreatedRequest,
}

impl RequestCreateResult {
    #[must_use]
    pub(crate) const fn new(plan: CreateRequestPlan, created: CreatedRequest) -> Self {
        Self { plan, created }
    }

    #[must_use]
    pub const fn plan(&self) -> &CreateRequestPlan {
        &self.plan
    }

    #[must_use]
    pub const fn created(&self) -> &CreatedRequest {
        &self.created
    }
}

#[derive(Debug, Error)]
pub enum RequestCreateError {
    #[error(transparent)]
    Preflight(#[from] PeerPreflightError),
    #[error("failed to create the Venmo request: {source}")]
    Create {
        #[source]
        source: ApiOperationFailure,
    },
}

impl RequestCreateError {
    #[must_use]
    pub const fn failure_kind(&self) -> ApplicationFailureKind {
        match self {
            Self::Preflight(source) => source.failure_kind(),
            Self::Create { source } => ApplicationFailureKind::Api(source.kind()),
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
    visibility: Visibility,
) -> Result<PreparedRequest, RequestCreateError>
where
    R: CredentialReader,
    A: CurrentAccountApi + UserLookupApi + UserSearchApi,
    <A as CurrentAccountApi>::Error: ApiFailure,
    G: ClientRequestIdGenerator,
{
    let (credential, account, recipient) =
        crate::features::payments::prepare_peer_preflight(credentials, api, recipient)
            .await?
            .into_parts();
    let plan = CreateRequestPlan::new(
        generator.generate(),
        account,
        recipient,
        amount,
        note,
        visibility,
    );
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
            source: ApiOperationFailure::new(source),
        })?;
    Ok(RequestCreateResult::new(prepared.plan, created))
}

#[cfg(test)]
#[path = "create_tests.rs"]
mod tests;

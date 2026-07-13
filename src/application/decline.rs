use thiserror::Error;

use super::auth::OperationFailure;
use super::financial::{FinancialValidationError, validate_account};
use super::ports::{
    ApiFailure, ApiFailureKind, AuthenticationApi, CredentialReader, RequestDeclineApi,
    RequestLookupApi,
};
use super::request_mutation::{
    RequestMutationValidationError, validate_complete_request, validate_incoming_pending,
    validate_request_id,
};
use crate::domain::{CredentialEnvelope, DeclineRequestPlan, DeclinedRequest, RequestId};

#[derive(Debug)]
pub struct PreparedDecline {
    credential: CredentialEnvelope,
    plan: DeclineRequestPlan,
}

impl PreparedDecline {
    #[must_use]
    pub(crate) const fn new(credential: CredentialEnvelope, plan: DeclineRequestPlan) -> Self {
        Self { credential, plan }
    }

    #[must_use]
    pub const fn plan(&self) -> &DeclineRequestPlan {
        &self.plan
    }
}

#[derive(Debug)]
pub struct DeclineResult {
    plan: DeclineRequestPlan,
    declined: DeclinedRequest,
}

impl DeclineResult {
    #[must_use]
    pub(crate) const fn new(plan: DeclineRequestPlan, declined: DeclinedRequest) -> Self {
        Self { plan, declined }
    }

    #[must_use]
    pub const fn plan(&self) -> &DeclineRequestPlan {
        &self.plan
    }

    #[must_use]
    pub const fn declined(&self) -> &DeclinedRequest {
        &self.declined
    }
}

#[derive(Debug, Error)]
pub enum DeclineError {
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
    Account(#[from] FinancialValidationError),
    #[error("failed to load the authoritative incoming request: {source}")]
    RequestLookup {
        kind: ApiFailureKind,
        #[source]
        source: OperationFailure,
    },
    #[error(transparent)]
    RequestValidation(#[from] RequestMutationValidationError),
    #[error("failed to decline the Venmo request: {source}")]
    Decline {
        kind: ApiFailureKind,
        #[source]
        source: OperationFailure,
    },
}

impl DeclineError {
    #[must_use]
    pub const fn api_failure_kind(&self) -> Option<ApiFailureKind> {
        match self {
            Self::CurrentAccount { kind, .. }
            | Self::RequestLookup { kind, .. }
            | Self::Decline { kind, .. } => Some(*kind),
            Self::MissingCredential
            | Self::CredentialLoad { .. }
            | Self::Account(_)
            | Self::RequestValidation(_) => None,
        }
    }
}

pub(crate) async fn prepare<R, A>(
    credentials: &R,
    api: &A,
    request_id: &RequestId,
) -> Result<PreparedDecline, DeclineError>
where
    R: CredentialReader,
    A: AuthenticationApi + RequestLookupApi,
    <A as AuthenticationApi>::Error: ApiFailure,
{
    let loaded = credentials
        .read_credential()
        .map_err(|source| DeclineError::CredentialLoad {
            source: OperationFailure::new(source),
        })?
        .ok_or(DeclineError::MissingCredential)?;
    let credential = loaded.envelope;
    let account = api
        .current_account(credential.access_token(), credential.device_id())
        .await
        .map_err(|source| DeclineError::CurrentAccount {
            kind: source.kind(),
            source: OperationFailure::new(source),
        })?;
    validate_account(&credential, &account)?;
    let request = api
        .pending_request_by_id(
            credential.access_token(),
            credential.device_id(),
            account.user_id(),
            request_id,
        )
        .await
        .map_err(|source| DeclineError::RequestLookup {
            kind: source.kind(),
            source: OperationFailure::new(source),
        })?;
    validate_request_id(&request, request_id)?;
    validate_incoming_pending(&request)?;
    validate_complete_request(&request)?;
    Ok(PreparedDecline::new(
        credential,
        DeclineRequestPlan::new(account, request),
    ))
}

pub(crate) async fn execute<A>(
    api: &A,
    prepared: PreparedDecline,
) -> Result<DeclineResult, DeclineError>
where
    A: RequestDeclineApi,
{
    let declined = api
        .decline_request(
            prepared.credential.access_token(),
            prepared.credential.device_id(),
            &prepared.plan,
        )
        .await
        .map_err(|source| DeclineError::Decline {
            kind: source.kind(),
            source: OperationFailure::new(source),
        })?;
    Ok(DeclineResult::new(prepared.plan, declined))
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
    };
    use crate::domain::{
        AccessToken, Account, DeviceId, Money, PendingRequest, PendingRequestAction,
        RequestDirection, RequestStatus, User, UserId, Username,
    };

    type TestResult = Result<(), Box<dyn Error>>;

    #[derive(Clone, Copy, Debug, thiserror::Error)]
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
        request: PendingRequest,
        declined: Result<DeclinedRequest, FakeApiError>,
        account_calls: Cell<usize>,
        lookup_calls: Cell<usize>,
        decline_calls: Cell<usize>,
        lookup_arguments_match: Cell<bool>,
        decline_plan_matches: Cell<bool>,
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

    impl RequestLookupApi for FakeApi {
        type Error = FakeApiError;

        fn pending_request_by_id<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            current_user_id: &'a UserId,
            request_id: &'a RequestId,
        ) -> impl Future<Output = Result<PendingRequest, Self::Error>> + Send + 'a {
            self.lookup_calls.set(self.lookup_calls.get() + 1);
            self.lookup_arguments_match
                .set(current_user_id == self.account.user_id() && request_id == self.request.id());
            ready(Ok(self.request.clone()))
        }
    }

    impl RequestDeclineApi for FakeApi {
        type Error = FakeApiError;

        fn decline_request<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            plan: &'a DeclineRequestPlan,
        ) -> impl Future<Output = Result<DeclinedRequest, Self::Error>> + Send + 'a {
            self.decline_calls.set(self.decline_calls.get() + 1);
            self.decline_plan_matches.set(
                plan.account().user_id() == self.account.user_id()
                    && plan.request().id() == self.request.id()
                    && plan.request().counterparty().user_id()
                        == self.request.counterparty().user_id(),
            );
            ready(self.declined.clone())
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn prepare_then_execute_performs_exactly_one_decline() -> TestResult {
        let api = fake_api(request(RequestDirection::Incoming, "pending")?)?;
        let prepared = prepare(&FakeReader, &api, &RequestId::from_str("request-1")?).await?;

        assert_eq!(api.decline_calls.get(), 0);
        let result = execute(&api, prepared).await?;

        assert_eq!(result.declined().status().as_str(), "cancelled");
        assert_eq!(api.account_calls.get(), 1);
        assert_eq!(api.lookup_calls.get(), 1);
        assert_eq!(api.decline_calls.get(), 1);
        assert!(api.lookup_arguments_match.get());
        assert!(api.decline_plan_matches.get());
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn wrong_direction_or_state_stops_before_decline() -> TestResult {
        for unsafe_request in [
            request(RequestDirection::Outgoing, "pending")?,
            request(RequestDirection::Incoming, "held")?,
            request(RequestDirection::Incoming, "cancelled")?,
            request(RequestDirection::Incoming, "pending")?.with_note(None),
            request(RequestDirection::Incoming, "pending")?.with_audience(None),
            request(RequestDirection::Incoming, "pending")?.with_action(PendingRequestAction::Pay),
        ] {
            let api = fake_api(unsafe_request)?;
            let result = prepare(&FakeReader, &api, &RequestId::from_str("request-1")?).await;

            assert!(result.is_err());
            assert_eq!(api.decline_calls.get(), 0);
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn ambiguous_decline_is_preserved_and_never_retried() -> TestResult {
        let mut api = fake_api(request(RequestDirection::Incoming, "pending")?)?;
        api.declined = Err(FakeApiError(ApiFailureKind::AmbiguousWrite));
        let prepared = prepare(&FakeReader, &api, &RequestId::from_str("request-1")?).await?;

        let result = execute(&api, prepared).await;

        assert!(matches!(
            result,
            Err(DeclineError::Decline {
                kind: ApiFailureKind::AmbiguousWrite,
                ..
            })
        ));
        assert_eq!(api.decline_calls.get(), 1);
        Ok(())
    }

    fn fake_api(request: PendingRequest) -> Result<FakeApi, Box<dyn Error>> {
        Ok(FakeApi {
            account: Account::new(
                UserId::from_str("123")?,
                Username::from_bare("owner")?,
                Some("Synthetic owner".to_owned()),
            ),
            request,
            declined: Ok(DeclinedRequest::new(
                RequestId::from_str("request-1")?,
                RequestStatus::from_str("cancelled")?,
            )),
            account_calls: Cell::new(0),
            lookup_calls: Cell::new(0),
            decline_calls: Cell::new(0),
            lookup_arguments_match: Cell::new(false),
            decline_plan_matches: Cell::new(false),
        })
    }

    fn request(
        direction: RequestDirection,
        status: &str,
    ) -> Result<PendingRequest, Box<dyn Error>> {
        Ok(PendingRequest::new(
            RequestId::from_str("request-1")?,
            direction,
            User::new(
                UserId::from_str("456")?,
                Some(Username::from_bare("requester")?),
                Some("Synthetic requester".to_owned()),
            ),
            Money::from_cents(1)?,
            Some("Synthetic request".to_owned()),
            Some(OffsetDateTime::UNIX_EPOCH),
            RequestStatus::from_str(status)?,
        )
        .with_audience(Some("private".to_owned())))
    }
}

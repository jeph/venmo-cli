use thiserror::Error;

use super::preflight;
use super::{
    CancelRequestPlan, CancelledRequest, RequestCancellationApi, RequestId, RequestLookupApi,
    RequestMutationPreflightError,
};
use crate::features::auth::{CurrentAccountApi, PromptError, prompt_failure_kind};
use crate::features::payments::DefaultNoConfirmation;
use crate::features::payments::confirmation::{self, DefaultNoConfirmationError};
use crate::shared::{
    ApiFailure, ApiOperationFailure, ApplicationFailureKind, CredentialEnvelope, CredentialReader,
};

#[derive(Debug)]
pub struct PreparedCancel {
    credential: CredentialEnvelope,
    plan: CancelRequestPlan,
}

impl PreparedCancel {
    #[must_use]
    pub(crate) const fn new(credential: CredentialEnvelope, plan: CancelRequestPlan) -> Self {
        Self { credential, plan }
    }

    #[must_use]
    pub const fn plan(&self) -> &CancelRequestPlan {
        &self.plan
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct CancelResult {
    plan: CancelRequestPlan,
    cancelled: CancelledRequest,
}

impl CancelResult {
    #[must_use]
    pub(crate) const fn new(plan: CancelRequestPlan, cancelled: CancelledRequest) -> Self {
        Self { plan, cancelled }
    }

    #[must_use]
    pub const fn plan(&self) -> &CancelRequestPlan {
        &self.plan
    }

    #[must_use]
    pub const fn cancelled(&self) -> &CancelledRequest {
        &self.cancelled
    }
}

#[derive(Debug, Error)]
pub enum CancelError {
    #[error(transparent)]
    Preflight(#[from] RequestMutationPreflightError),
    #[error(
        "request cancellation confirmation requires both stdin and stderr to be terminals; pass `--yes` to authorize non-interactively"
    )]
    ConfirmationRequired,
    #[error("request cancellation cancelled")]
    ConfirmationDeclined,
    #[error("request cancellation confirmation failed: {source}")]
    Confirmation {
        #[source]
        source: PromptError,
    },
    #[error("failed to cancel the outgoing Venmo request: {source}")]
    Cancel {
        #[source]
        source: ApiOperationFailure,
    },
}

impl CancelError {
    #[must_use]
    pub const fn failure_kind(&self) -> ApplicationFailureKind {
        match self {
            Self::Preflight(source) => source.failure_kind(),
            Self::ConfirmationRequired => ApplicationFailureKind::Usage,
            Self::ConfirmationDeclined => ApplicationFailureKind::Cancelled,
            Self::Confirmation { source } => prompt_failure_kind(source),
            Self::Cancel { source } => ApplicationFailureKind::Api(source.kind()),
        }
    }
}

#[derive(Debug)]
pub(crate) struct AuthorizedCancel(PreparedCancel);

pub(crate) async fn prepare<R, A>(
    credentials: &R,
    api: &A,
    request_id: &RequestId,
) -> Result<PreparedCancel, CancelError>
where
    R: CredentialReader,
    A: CurrentAccountApi + RequestLookupApi,
    <A as CurrentAccountApi>::Error: ApiFailure,
{
    let (credential, account, request) = preflight::prepare_outgoing(credentials, api, request_id)
        .await?
        .into_parts();
    Ok(PreparedCancel::new(
        credential,
        CancelRequestPlan::new(account, request),
    ))
}

pub(crate) fn authorize<P>(
    prompt: &P,
    prepared: PreparedCancel,
    assume_yes: bool,
) -> Result<AuthorizedCancel, CancelError>
where
    P: DefaultNoConfirmation,
{
    confirmation::authorize(prompt, assume_yes, "Cancel this outgoing request?").map_err(
        |error| match error {
            DefaultNoConfirmationError::Required => CancelError::ConfirmationRequired,
            DefaultNoConfirmationError::Declined => CancelError::ConfirmationDeclined,
            DefaultNoConfirmationError::Prompt(source) => CancelError::Confirmation { source },
        },
    )?;
    Ok(AuthorizedCancel(prepared))
}

pub(crate) async fn execute<A>(
    api: &A,
    authorized: AuthorizedCancel,
) -> Result<CancelResult, CancelError>
where
    A: RequestCancellationApi,
{
    let AuthorizedCancel(prepared) = authorized;
    let cancelled = api
        .cancel_request(
            prepared.credential.access_token(),
            prepared.credential.device_id(),
            &prepared.plan,
        )
        .await
        .map_err(|source| CancelError::Cancel {
            source: ApiOperationFailure::new(source),
        })?;
    Ok(CancelResult::new(prepared.plan, cancelled))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::error::Error;
    use std::future::{Future, ready};
    use std::str::FromStr;

    use time::OffsetDateTime;

    use super::*;
    use crate::features::auth::PromptAvailability;
    use crate::features::people::User;
    use crate::features::requests::{
        RequestAction, RequestDirection, RequestRecord, RequestStatus,
    };
    use crate::shared::{
        AccessToken, Account, ApiFailureKind, CredentialCapability, CredentialFailureKind,
        CredentialFormat, CredentialStoreFailure, DeviceId, LoadedCredential, Money, UserId,
        Username,
    };

    type TestResult = Result<(), Box<dyn Error>>;

    #[derive(Clone, Copy, Debug, thiserror::Error)]
    #[error("synthetic credential error")]
    struct FakeCredentialError;

    impl CredentialStoreFailure for FakeCredentialError {
        fn kind(&self) -> CredentialFailureKind {
            CredentialFailureKind::Unavailable
        }
    }

    struct FakeReader;

    impl CredentialCapability for FakeReader {
        type Error = FakeCredentialError;
    }

    impl CredentialReader for FakeReader {
        fn read_credential(&self) -> Result<Option<LoadedCredential>, Self::Error> {
            Ok(Some(LoadedCredential {
                envelope: CredentialEnvelope::new(
                    AccessToken::from_str("synthetic-secret-cancel-token")
                        .map_err(|_| FakeCredentialError)?,
                    DeviceId::from_str("synthetic-secret-cancel-device")
                        .map_err(|_| FakeCredentialError)?,
                    UserId::from_str("123").map_err(|_| FakeCredentialError)?,
                    Username::from_bare("owner").map_err(|_| FakeCredentialError)?,
                    Some("Synthetic owner".to_owned()),
                    OffsetDateTime::UNIX_EPOCH,
                ),
                format: CredentialFormat::Version1,
            }))
        }
    }

    #[derive(Clone, Copy, Debug, thiserror::Error)]
    #[error("synthetic API error")]
    struct FakeApiError;

    impl ApiFailure for FakeApiError {
        fn kind(&self) -> ApiFailureKind {
            ApiFailureKind::Contract
        }
    }

    struct FakeApi {
        request: RequestRecord,
        writes: Cell<usize>,
    }

    impl CurrentAccountApi for FakeApi {
        type Error = FakeApiError;

        fn current_account<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
        ) -> impl Future<Output = Result<Account, Self::Error>> + Send + 'a {
            ready(account())
        }
    }

    impl RequestLookupApi for FakeApi {
        type Error = FakeApiError;

        fn request_by_id<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            _current_user_id: &'a UserId,
            _request_id: &'a RequestId,
        ) -> impl Future<Output = Result<RequestRecord, Self::Error>> + Send + 'a {
            ready(Ok(self.request.clone()))
        }
    }

    impl RequestCancellationApi for FakeApi {
        type Error = FakeApiError;

        fn cancel_request<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            plan: &'a CancelRequestPlan,
        ) -> impl Future<Output = Result<CancelledRequest, Self::Error>> + Send + 'a {
            assert_eq!(plan.account().user_id().as_str(), "123");
            assert_eq!(plan.request(), &self.request);
            self.writes.set(self.writes.get() + 1);
            let result = RequestStatus::from_str("cancelled")
                .map(|status| CancelledRequest::new(plan.request().id().clone(), status))
                .map_err(|_| FakeApiError);
            ready(result)
        }
    }

    struct FakePrompt {
        interactive: bool,
        answer: bool,
    }

    impl PromptAvailability for FakePrompt {
        fn can_prompt(&self) -> bool {
            self.interactive
        }
    }

    impl DefaultNoConfirmation for FakePrompt {
        fn confirm_default_no(&self, prompt: &str) -> Result<bool, PromptError> {
            assert_eq!(prompt, "Cancel this outgoing request?");
            Ok(self.answer)
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn pending_and_held_outgoing_requests_cancel_once_with_the_authoritative_plan()
    -> TestResult {
        for status in ["pending", "held"] {
            let api = FakeApi {
                request: request(RequestDirection::Outgoing, status)?,
                writes: Cell::new(0),
            };
            let prepared = prepare(&FakeReader, &api, &RequestId::from_str("request-1")?).await?;
            let authorized = authorize(
                &FakePrompt {
                    interactive: false,
                    answer: false,
                },
                prepared,
                true,
            )?;
            let result = execute(&api, authorized).await?;
            assert_eq!(result.cancelled().status().as_str(), "cancelled");
            assert_eq!(api.writes.get(), 1);
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn wrong_direction_or_terminal_state_fails_before_authorization_or_write() -> TestResult {
        for (direction, status) in [
            (RequestDirection::Incoming, "pending"),
            (RequestDirection::Outgoing, "cancelled"),
            (RequestDirection::Outgoing, "settled"),
        ] {
            let api = FakeApi {
                request: request(direction, status)?,
                writes: Cell::new(0),
            };
            assert!(matches!(
                prepare(&FakeReader, &api, &RequestId::from_str("request-1")?).await,
                Err(CancelError::Preflight(
                    RequestMutationPreflightError::RequestValidation(_)
                ))
            ));
            assert_eq!(api.writes.get(), 0);
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn confirmation_defaults_no_and_noninteractive_requires_yes() -> TestResult {
        let api = FakeApi {
            request: request(RequestDirection::Outgoing, "pending")?,
            writes: Cell::new(0),
        };
        let request_id = RequestId::from_str("request-1")?;
        let prepared = prepare(&FakeReader, &api, &request_id).await?;
        assert!(matches!(
            authorize(
                &FakePrompt {
                    interactive: false,
                    answer: true,
                },
                prepared,
                false,
            ),
            Err(CancelError::ConfirmationRequired)
        ));
        let prepared = prepare(&FakeReader, &api, &request_id).await?;
        assert!(matches!(
            authorize(
                &FakePrompt {
                    interactive: true,
                    answer: false,
                },
                prepared,
                false,
            ),
            Err(CancelError::ConfirmationDeclined)
        ));
        assert_eq!(api.writes.get(), 0);
        Ok(())
    }

    fn account() -> Result<Account, FakeApiError> {
        Ok(Account::new(
            UserId::from_str("123").map_err(|_| FakeApiError)?,
            Username::from_bare("owner").map_err(|_| FakeApiError)?,
            Some("Synthetic owner".to_owned()),
        ))
    }

    fn request(direction: RequestDirection, status: &str) -> Result<RequestRecord, Box<dyn Error>> {
        Ok(RequestRecord::new(
            RequestId::from_str("request-1")?,
            RequestAction::Charge,
            direction,
            User::new(
                UserId::from_str("456")?,
                Some(Username::from_bare("recipient")?),
                Some("Synthetic recipient".to_owned()),
            ),
            Money::from_cents(1)?,
            Some("Synthetic request".to_owned()),
            Some(OffsetDateTime::UNIX_EPOCH),
            RequestStatus::from_str(status)?,
        )
        .with_audience(Some("private".to_owned())))
    }
}

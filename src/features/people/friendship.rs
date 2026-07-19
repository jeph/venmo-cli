use thiserror::Error;

use super::lookup::{self, UserLookupError};
use super::{
    FriendshipMutationApi, FriendshipStatus, User, UserLookupApi, UserProfileKind, UserSearchApi,
};
use crate::features::auth::{CurrentAccountApi, PromptError, prompt_failure_kind};
use crate::features::payments::DefaultNoConfirmation;
use crate::features::payments::confirmation::{self, DefaultNoConfirmationError};
use crate::shared::{
    Account, ApiFailure, ApiFailureKind, ApiOperationFailure, ApplicationFailureKind,
    CredentialAccessError, CredentialEnvelope, CredentialReader, Username, require_credential,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FriendshipIntent {
    Add,
    Remove,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FriendshipAction {
    SendRequest,
    AcceptRequest,
    Unfriend,
    CancelRequest,
}

impl FriendshipAction {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::SendRequest => "send friend request",
            Self::AcceptRequest => "accept incoming friend request",
            Self::Unfriend => "remove established friend",
            Self::CancelRequest => "cancel outgoing friend request",
        }
    }

    const fn expected_status(self) -> FriendshipStatus {
        match self {
            Self::SendRequest => FriendshipStatus::RequestSent,
            Self::AcceptRequest => FriendshipStatus::Friend,
            Self::Unfriend | Self::CancelRequest => FriendshipStatus::NotFriend,
        }
    }

    const fn confirmation(self) -> &'static str {
        match self {
            Self::SendRequest => "Send this friend request?",
            Self::AcceptRequest => "Accept this incoming friend request?",
            Self::Unfriend => "Remove this established friend?",
            Self::CancelRequest => "Cancel this outgoing friend request?",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct FriendshipPlan {
    account: Account,
    target: User,
    previous_status: FriendshipStatus,
    action: FriendshipAction,
}

impl FriendshipPlan {
    pub(crate) fn new(
        account: Account,
        target: User,
        previous_status: FriendshipStatus,
        action: FriendshipAction,
    ) -> Self {
        Self {
            account,
            target,
            previous_status,
            action,
        }
    }

    #[must_use]
    pub const fn account(&self) -> &Account {
        &self.account
    }

    #[must_use]
    pub const fn target(&self) -> &User {
        &self.target
    }

    #[must_use]
    pub const fn previous_status(&self) -> FriendshipStatus {
        self.previous_status
    }

    #[must_use]
    pub const fn action(&self) -> FriendshipAction {
        self.action
    }
}

#[derive(Debug)]
pub struct PreparedFriendshipMutation {
    credential: CredentialEnvelope,
    plan: FriendshipPlan,
}

impl PreparedFriendshipMutation {
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn new(credential: CredentialEnvelope, plan: FriendshipPlan) -> Self {
        Self { credential, plan }
    }

    #[must_use]
    pub const fn plan(&self) -> &FriendshipPlan {
        &self.plan
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct FriendshipMutationResult {
    plan: FriendshipPlan,
    status: FriendshipStatus,
}

impl FriendshipMutationResult {
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn new(plan: FriendshipPlan, status: FriendshipStatus) -> Self {
        Self { plan, status }
    }

    #[must_use]
    pub const fn plan(&self) -> &FriendshipPlan {
        &self.plan
    }

    #[must_use]
    pub const fn status(&self) -> FriendshipStatus {
        self.status
    }
}

#[derive(Debug, Error)]
pub enum FriendshipMutationError {
    #[error(transparent)]
    Credential(#[from] CredentialAccessError),

    #[error("failed to validate the current Venmo account: {source}")]
    CurrentAccount {
        #[source]
        source: ApiOperationFailure,
    },

    #[error("the current Venmo account does not match the stored credential")]
    AccountMismatch,

    #[error("failed to resolve the exact Venmo username: {source}")]
    TargetLookup {
        #[source]
        source: UserLookupError,
    },

    #[error("friendship cannot be changed for the authenticated account itself")]
    SelfTarget,

    #[error("the target response did not prove the profile type")]
    MissingProfileType,

    #[error("friendship mutations support personal Venmo profiles only")]
    UnsupportedProfileType,

    #[error("the target response did not contain a supported friendship status")]
    MissingFriendshipStatus,

    #[error("that Venmo user is already a friend")]
    AlreadyFriend,

    #[error("a friend request has already been sent to that Venmo user")]
    RequestAlreadySent,

    #[error("that Venmo user is not currently a friend and has no outgoing request")]
    NothingToRemove,

    #[error(
        "an incoming friend request must be accepted with `friends add` or declined separately"
    )]
    IncomingRequestRequiresSeparateAction,

    #[error(
        "friendship confirmation requires both stdin and stderr to be terminals; pass `--yes` to authorize non-interactively"
    )]
    ConfirmationRequired,

    #[error("friendship mutation cancelled")]
    ConfirmationDeclined,

    #[error("friendship confirmation failed: {source}")]
    Confirmation {
        #[source]
        source: PromptError,
    },

    #[error("failed to change the Venmo friendship: {source}")]
    Mutation {
        #[source]
        source: ApiOperationFailure,
    },

    #[error(
        "the friendship outcome is unknown and must be reconciled before retrying because {problem}"
    )]
    OutcomeUnknown { problem: &'static str },
}

impl FriendshipMutationError {
    #[must_use]
    pub const fn failure_kind(&self) -> ApplicationFailureKind {
        match self {
            Self::Credential(_) | Self::AccountMismatch => ApplicationFailureKind::Credential,
            Self::CurrentAccount { source } | Self::Mutation { source } => {
                ApplicationFailureKind::Api(source.kind())
            }
            Self::TargetLookup { source } => source.application_failure_kind(),
            Self::MissingProfileType | Self::MissingFriendshipStatus => {
                ApplicationFailureKind::ApiContract
            }
            Self::SelfTarget
            | Self::UnsupportedProfileType
            | Self::AlreadyFriend
            | Self::RequestAlreadySent
            | Self::NothingToRemove
            | Self::IncomingRequestRequiresSeparateAction
            | Self::ConfirmationRequired => ApplicationFailureKind::Usage,
            Self::ConfirmationDeclined => ApplicationFailureKind::Cancelled,
            Self::Confirmation { source } => prompt_failure_kind(source),
            Self::OutcomeUnknown { .. } => {
                ApplicationFailureKind::Api(ApiFailureKind::AmbiguousWrite)
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct AuthorizedFriendshipMutation(PreparedFriendshipMutation);

pub async fn prepare<R, A>(
    credentials: &R,
    api: &A,
    username: &Username,
    intent: FriendshipIntent,
) -> Result<PreparedFriendshipMutation, FriendshipMutationError>
where
    R: CredentialReader,
    A: CurrentAccountApi + UserLookupApi + UserSearchApi,
    <A as CurrentAccountApi>::Error: ApiFailure,
{
    let loaded = require_credential(credentials)?;
    let credential = loaded.envelope;
    let account = api
        .current_account(credential.access_token(), credential.device_id())
        .await
        .map_err(|source| FriendshipMutationError::CurrentAccount {
            source: ApiOperationFailure::new(source),
        })?;
    if account.user_id() != credential.user_id() {
        return Err(FriendshipMutationError::AccountMismatch);
    }
    let target = lookup::resolve_with_credential(&credential, api, username)
        .await
        .map_err(|source| FriendshipMutationError::TargetLookup { source })?;
    let plan = plan_for_target(account, target, intent)?;
    Ok(PreparedFriendshipMutation { credential, plan })
}

fn plan_for_target(
    account: Account,
    target: User,
    intent: FriendshipIntent,
) -> Result<FriendshipPlan, FriendshipMutationError> {
    if target.user_id() == account.user_id() {
        return Err(FriendshipMutationError::SelfTarget);
    }
    match target.profile_kind() {
        Some(UserProfileKind::Personal) => {}
        None => return Err(FriendshipMutationError::MissingProfileType),
        Some(UserProfileKind::Business | UserProfileKind::Charity | UserProfileKind::Unknown) => {
            return Err(FriendshipMutationError::UnsupportedProfileType);
        }
    }
    let previous_status = target
        .friendship_status()
        .ok_or(FriendshipMutationError::MissingFriendshipStatus)?;
    let action = select_action(intent, previous_status)?;
    Ok(FriendshipPlan::new(
        account,
        target,
        previous_status,
        action,
    ))
}

fn select_action(
    intent: FriendshipIntent,
    status: FriendshipStatus,
) -> Result<FriendshipAction, FriendshipMutationError> {
    match (intent, status) {
        (FriendshipIntent::Add, FriendshipStatus::NotFriend) => Ok(FriendshipAction::SendRequest),
        (FriendshipIntent::Add, FriendshipStatus::RequestReceived) => {
            Ok(FriendshipAction::AcceptRequest)
        }
        (FriendshipIntent::Add, FriendshipStatus::Friend) => {
            Err(FriendshipMutationError::AlreadyFriend)
        }
        (FriendshipIntent::Add, FriendshipStatus::RequestSent) => {
            Err(FriendshipMutationError::RequestAlreadySent)
        }
        (FriendshipIntent::Remove, FriendshipStatus::Friend) => Ok(FriendshipAction::Unfriend),
        (FriendshipIntent::Remove, FriendshipStatus::RequestSent) => {
            Ok(FriendshipAction::CancelRequest)
        }
        (FriendshipIntent::Remove, FriendshipStatus::NotFriend) => {
            Err(FriendshipMutationError::NothingToRemove)
        }
        (FriendshipIntent::Remove, FriendshipStatus::RequestReceived) => {
            Err(FriendshipMutationError::IncomingRequestRequiresSeparateAction)
        }
    }
}

pub(crate) fn authorize<P>(
    prompt: &P,
    prepared: PreparedFriendshipMutation,
    assume_yes: bool,
) -> Result<AuthorizedFriendshipMutation, FriendshipMutationError>
where
    P: DefaultNoConfirmation,
{
    confirmation::authorize(prompt, assume_yes, prepared.plan.action.confirmation()).map_err(
        |error| match error {
            DefaultNoConfirmationError::Required => FriendshipMutationError::ConfirmationRequired,
            DefaultNoConfirmationError::Declined => FriendshipMutationError::ConfirmationDeclined,
            DefaultNoConfirmationError::Prompt(source) => {
                FriendshipMutationError::Confirmation { source }
            }
        },
    )?;
    Ok(AuthorizedFriendshipMutation(prepared))
}

pub(crate) async fn execute<A>(
    api: &A,
    authorized: AuthorizedFriendshipMutation,
) -> Result<FriendshipMutationResult, FriendshipMutationError>
where
    A: FriendshipMutationApi,
{
    let AuthorizedFriendshipMutation(prepared) = authorized;
    let expected_status = prepared.plan.action.expected_status();
    let target_user_id = prepared.plan.target.user_id();
    let verified = match prepared.plan.action {
        FriendshipAction::SendRequest | FriendshipAction::AcceptRequest => {
            api.add_or_accept_friend(
                prepared.credential.access_token(),
                prepared.credential.device_id(),
                target_user_id,
                expected_status,
            )
            .await
        }
        FriendshipAction::Unfriend | FriendshipAction::CancelRequest => {
            api.remove_or_cancel_friend(
                prepared.credential.access_token(),
                prepared.credential.device_id(),
                prepared.plan.account.user_id(),
                target_user_id,
            )
            .await
        }
    }
    .map_err(|source| FriendshipMutationError::Mutation {
        source: ApiOperationFailure::new(source),
    })?;
    if verified.user_id() != target_user_id {
        return Err(FriendshipMutationError::OutcomeUnknown {
            problem: "the verified result identified a different user",
        });
    }
    if verified.friendship_status() != Some(expected_status) {
        return Err(FriendshipMutationError::OutcomeUnknown {
            problem: "the verified relationship did not match the requested action",
        });
    }
    Ok(FriendshipMutationResult {
        plan: prepared.plan,
        status: expected_status,
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::future::{Future, ready};
    use std::rc::Rc;
    use std::str::FromStr;

    use time::OffsetDateTime;

    use super::*;
    use crate::features::auth::PromptAvailability;
    use crate::shared::{AccessToken, DeviceId, UserId};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum PromptCall {
        Availability,
        Confirm(String),
    }

    struct FakePrompt {
        interactive: bool,
        answer: bool,
        calls: Rc<RefCell<Vec<PromptCall>>>,
    }

    impl PromptAvailability for FakePrompt {
        fn can_prompt(&self) -> bool {
            self.calls.borrow_mut().push(PromptCall::Availability);
            self.interactive
        }
    }

    impl DefaultNoConfirmation for FakePrompt {
        fn confirm_default_no(&self, prompt: &str) -> Result<bool, PromptError> {
            self.calls
                .borrow_mut()
                .push(PromptCall::Confirm(prompt.to_owned()));
            Ok(self.answer)
        }
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum MutationCall {
        Add {
            target: UserId,
            expected: FriendshipStatus,
        },
        Remove {
            account: UserId,
            target: UserId,
        },
    }

    #[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
    #[error("synthetic friendship API failure")]
    struct FakeApiError(ApiFailureKind);

    impl ApiFailure for FakeApiError {
        fn kind(&self) -> ApiFailureKind {
            self.0
        }
    }

    struct FakeMutationApi {
        responses: RefCell<VecDeque<Result<User, FakeApiError>>>,
        calls: Rc<RefCell<Vec<MutationCall>>>,
    }

    impl FriendshipMutationApi for FakeMutationApi {
        type Error = FakeApiError;

        fn add_or_accept_friend<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            target_user_id: &'a UserId,
            expected_status: FriendshipStatus,
        ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a {
            self.calls.borrow_mut().push(MutationCall::Add {
                target: target_user_id.clone(),
                expected: expected_status,
            });
            ready(self.next_response())
        }

        fn remove_or_cancel_friend<'a>(
            &'a self,
            _access_token: &'a AccessToken,
            _device_id: &'a DeviceId,
            self_user_id: &'a UserId,
            target_user_id: &'a UserId,
        ) -> impl Future<Output = Result<User, Self::Error>> + Send + 'a {
            self.calls.borrow_mut().push(MutationCall::Remove {
                account: self_user_id.clone(),
                target: target_user_id.clone(),
            });
            ready(self.next_response())
        }
    }

    impl FakeMutationApi {
        fn next_response(&self) -> Result<User, FakeApiError> {
            self.responses
                .borrow_mut()
                .pop_front()
                .unwrap_or(Err(FakeApiError(ApiFailureKind::Internal)))
        }
    }

    #[test]
    fn native_state_matrix_is_closed_and_action_specific() {
        assert_eq!(
            select_action(FriendshipIntent::Add, FriendshipStatus::NotFriend)
                .map_err(|error| error.to_string()),
            Ok(FriendshipAction::SendRequest)
        );
        assert_eq!(
            select_action(FriendshipIntent::Add, FriendshipStatus::RequestReceived)
                .map_err(|error| error.to_string()),
            Ok(FriendshipAction::AcceptRequest)
        );
        assert!(matches!(
            select_action(FriendshipIntent::Add, FriendshipStatus::Friend),
            Err(FriendshipMutationError::AlreadyFriend)
        ));
        assert!(matches!(
            select_action(FriendshipIntent::Add, FriendshipStatus::RequestSent),
            Err(FriendshipMutationError::RequestAlreadySent)
        ));
        assert_eq!(
            select_action(FriendshipIntent::Remove, FriendshipStatus::Friend)
                .map_err(|error| error.to_string()),
            Ok(FriendshipAction::Unfriend)
        );
        assert_eq!(
            select_action(FriendshipIntent::Remove, FriendshipStatus::RequestSent)
                .map_err(|error| error.to_string()),
            Ok(FriendshipAction::CancelRequest)
        );
        assert!(matches!(
            select_action(FriendshipIntent::Remove, FriendshipStatus::NotFriend),
            Err(FriendshipMutationError::NothingToRemove)
        ));
        assert!(matches!(
            select_action(FriendshipIntent::Remove, FriendshipStatus::RequestReceived),
            Err(FriendshipMutationError::IncomingRequestRequiresSeparateAction)
        ));
    }

    #[test]
    fn target_validation_fails_closed_before_action_selection() -> TestResult {
        let self_target = User::new(
            UserId::from_str("100")?,
            Some(Username::from_bare("owner")?),
            None,
        )
        .with_financial_attributes(UserProfileKind::Personal, true)
        .with_friendship_status(FriendshipStatus::NotFriend);
        assert!(matches!(
            plan_for_target(account()?, self_target, FriendshipIntent::Add),
            Err(FriendshipMutationError::SelfTarget)
        ));

        let missing_profile = User::new(
            UserId::from_str("200")?,
            Some(Username::from_bare("alice")?),
            None,
        )
        .with_friendship_status(FriendshipStatus::NotFriend);
        assert!(matches!(
            plan_for_target(account()?, missing_profile, FriendshipIntent::Add),
            Err(FriendshipMutationError::MissingProfileType)
        ));

        for kind in [
            UserProfileKind::Business,
            UserProfileKind::Charity,
            UserProfileKind::Unknown,
        ] {
            let unsupported = User::new(
                UserId::from_str("200")?,
                Some(Username::from_bare("alice")?),
                None,
            )
            .with_financial_attributes(kind, true)
            .with_friendship_status(FriendshipStatus::NotFriend);
            assert!(matches!(
                plan_for_target(account()?, unsupported, FriendshipIntent::Add),
                Err(FriendshipMutationError::UnsupportedProfileType)
            ));
        }

        let missing_status = User::new(
            UserId::from_str("200")?,
            Some(Username::from_bare("alice")?),
            None,
        )
        .with_financial_attributes(UserProfileKind::Personal, true);
        assert!(matches!(
            plan_for_target(account()?, missing_status, FriendshipIntent::Add),
            Err(FriendshipMutationError::MissingFriendshipStatus)
        ));
        Ok(())
    }

    #[test]
    fn authorization_is_default_no_action_specific_and_yes_skips_prompt() -> TestResult {
        for (action, previous, question) in [
            (
                FriendshipAction::SendRequest,
                FriendshipStatus::NotFriend,
                "Send this friend request?",
            ),
            (
                FriendshipAction::AcceptRequest,
                FriendshipStatus::RequestReceived,
                "Accept this incoming friend request?",
            ),
            (
                FriendshipAction::Unfriend,
                FriendshipStatus::Friend,
                "Remove this established friend?",
            ),
            (
                FriendshipAction::CancelRequest,
                FriendshipStatus::RequestSent,
                "Cancel this outgoing friend request?",
            ),
        ] {
            let skipped_calls = Rc::new(RefCell::new(Vec::new()));
            let skipped = authorize(
                &FakePrompt {
                    interactive: false,
                    answer: false,
                    calls: Rc::clone(&skipped_calls),
                },
                prepared(action, previous)?,
                true,
            );
            assert!(skipped.is_ok());
            assert!(skipped_calls.borrow().is_empty());

            let confirmed_calls = Rc::new(RefCell::new(Vec::new()));
            let confirmed = authorize(
                &FakePrompt {
                    interactive: true,
                    answer: true,
                    calls: Rc::clone(&confirmed_calls),
                },
                prepared(action, previous)?,
                false,
            );
            assert!(confirmed.is_ok());
            assert_eq!(
                *confirmed_calls.borrow(),
                vec![
                    PromptCall::Availability,
                    PromptCall::Confirm(question.to_owned())
                ]
            );
        }

        let unavailable_calls = Rc::new(RefCell::new(Vec::new()));
        let unavailable = authorize(
            &FakePrompt {
                interactive: false,
                answer: true,
                calls: Rc::clone(&unavailable_calls),
            },
            prepared(FriendshipAction::SendRequest, FriendshipStatus::NotFriend)?,
            false,
        );
        assert!(matches!(
            unavailable,
            Err(FriendshipMutationError::ConfirmationRequired)
        ));
        assert_eq!(*unavailable_calls.borrow(), vec![PromptCall::Availability]);

        let declined_calls = Rc::new(RefCell::new(Vec::new()));
        let declined = authorize(
            &FakePrompt {
                interactive: true,
                answer: false,
                calls: Rc::clone(&declined_calls),
            },
            prepared(FriendshipAction::Unfriend, FriendshipStatus::Friend)?,
            false,
        );
        assert!(matches!(
            declined,
            Err(FriendshipMutationError::ConfirmationDeclined)
        ));
        assert_eq!(declined_calls.borrow().len(), 2);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn execution_uses_exactly_one_native_write_for_each_action() -> TestResult {
        for (action, previous, expected_call) in [
            (
                FriendshipAction::SendRequest,
                FriendshipStatus::NotFriend,
                MutationCall::Add {
                    target: UserId::from_str("200")?,
                    expected: FriendshipStatus::RequestSent,
                },
            ),
            (
                FriendshipAction::AcceptRequest,
                FriendshipStatus::RequestReceived,
                MutationCall::Add {
                    target: UserId::from_str("200")?,
                    expected: FriendshipStatus::Friend,
                },
            ),
            (
                FriendshipAction::Unfriend,
                FriendshipStatus::Friend,
                MutationCall::Remove {
                    account: UserId::from_str("100")?,
                    target: UserId::from_str("200")?,
                },
            ),
            (
                FriendshipAction::CancelRequest,
                FriendshipStatus::RequestSent,
                MutationCall::Remove {
                    account: UserId::from_str("100")?,
                    target: UserId::from_str("200")?,
                },
            ),
        ] {
            let expected_status = action.expected_status();
            let calls = Rc::new(RefCell::new(Vec::new()));
            let api = FakeMutationApi {
                responses: RefCell::new(
                    vec![
                        Ok(target(expected_status)?),
                        Err(FakeApiError(ApiFailureKind::Internal)),
                    ]
                    .into(),
                ),
                calls: Rc::clone(&calls),
            };
            let authorized = AuthorizedFriendshipMutation(prepared(action, previous)?);

            let result = execute(&api, authorized).await?;

            assert_eq!(result.plan().action(), action);
            assert_eq!(result.status(), expected_status);
            assert_eq!(*calls.borrow(), vec![expected_call]);
            assert_eq!(
                api.responses.borrow().len(),
                1,
                "write retried unexpectedly"
            );
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn execution_fails_ambiguous_on_unverified_result_without_retrying() -> TestResult {
        for response in [
            Ok(User::new(
                UserId::from_str("999")?,
                Some(Username::from_bare("other")?),
                None,
            )
            .with_financial_attributes(UserProfileKind::Personal, true)
            .with_friendship_status(FriendshipStatus::RequestSent)),
            Ok(target(FriendshipStatus::NotFriend)?),
            Err(FakeApiError(ApiFailureKind::AmbiguousWrite)),
        ] {
            let calls = Rc::new(RefCell::new(Vec::new()));
            let api = FakeMutationApi {
                responses: RefCell::new(
                    vec![response, Err(FakeApiError(ApiFailureKind::Internal))].into(),
                ),
                calls: Rc::clone(&calls),
            };
            let result = execute(
                &api,
                AuthorizedFriendshipMutation(prepared(
                    FriendshipAction::SendRequest,
                    FriendshipStatus::NotFriend,
                )?),
            )
            .await;

            assert!(matches!(
                result,
                Err(FriendshipMutationError::OutcomeUnknown { .. })
                    | Err(FriendshipMutationError::Mutation { .. })
            ));
            assert_eq!(calls.borrow().len(), 1);
            assert_eq!(
                api.responses.borrow().len(),
                1,
                "write retried unexpectedly"
            );
            if let Err(error) = result {
                assert_eq!(
                    error.failure_kind(),
                    ApplicationFailureKind::Api(ApiFailureKind::AmbiguousWrite)
                );
            }
        }
        Ok(())
    }

    fn prepared(
        action: FriendshipAction,
        previous: FriendshipStatus,
    ) -> Result<PreparedFriendshipMutation, Box<dyn std::error::Error>> {
        Ok(PreparedFriendshipMutation {
            credential: credential()?,
            plan: FriendshipPlan::new(account()?, target(previous)?, previous, action),
        })
    }

    fn credential() -> Result<CredentialEnvelope, Box<dyn std::error::Error>> {
        Ok(CredentialEnvelope::new(
            AccessToken::from_str("synthetic-token")?,
            DeviceId::from_str("synthetic-device")?,
            UserId::from_str("100")?,
            Username::from_bare("owner")?,
            Some("Owner".to_owned()),
            OffsetDateTime::UNIX_EPOCH,
        ))
    }

    fn account() -> Result<Account, Box<dyn std::error::Error>> {
        Ok(Account::new(
            UserId::from_str("100")?,
            Username::from_bare("owner")?,
            Some("Owner".to_owned()),
        ))
    }

    fn target(status: FriendshipStatus) -> Result<User, Box<dyn std::error::Error>> {
        Ok(User::new(
            UserId::from_str("200")?,
            Some(Username::from_bare("alice")?),
            Some("Alice".to_owned()),
        )
        .with_financial_attributes(UserProfileKind::Personal, true)
        .with_friendship_status(status))
    }
}

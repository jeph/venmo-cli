use thiserror::Error;

use super::{
    ActivityCommentMessage, ActivityDetail, ActivityDetailApi, ActivityId, ActivityLikeState,
    ActivitySocialMutationApi,
};
use crate::features::auth::{PromptError, prompt_failure_kind};
use crate::features::payments::DefaultNoConfirmation;
use crate::features::payments::confirmation::{self, DefaultNoConfirmationError};
use crate::shared::{
    ApiFailureKind, ApiOperationFailure, ApplicationFailureKind, CredentialAccessError,
    CredentialEnvelope, CredentialReader, require_credential,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActivitySocialIntent {
    AddComment(ActivityCommentMessage),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActivitySocialAction {
    AddComment(ActivityCommentMessage),
}

impl ActivitySocialAction {
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::AddComment(_) => "add activity comment",
        }
    }

    #[must_use]
    pub const fn result_label(&self) -> &'static str {
        match self {
            Self::AddComment(_) => "Comment added",
        }
    }

    const fn confirmation(&self) -> &'static str {
        match self {
            Self::AddComment(_) => "Add this comment?",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivitySocialPlan {
    activity: ActivityDetail,
    action: ActivitySocialAction,
    previous_like_state: ActivityLikeState,
}

impl ActivitySocialPlan {
    #[must_use]
    pub const fn activity(&self) -> &ActivityDetail {
        &self.activity
    }

    #[must_use]
    pub const fn action(&self) -> &ActivitySocialAction {
        &self.action
    }

    #[must_use]
    pub const fn previous_like_state(&self) -> ActivityLikeState {
        self.previous_like_state
    }
}

#[derive(Debug)]
pub struct PreparedActivitySocialMutation {
    credential: CredentialEnvelope,
    plan: ActivitySocialPlan,
}

impl PreparedActivitySocialMutation {
    #[must_use]
    pub const fn plan(&self) -> &ActivitySocialPlan {
        &self.plan
    }
}

#[derive(Debug)]
pub(crate) struct AuthorizedActivitySocialMutation(PreparedActivitySocialMutation);

#[derive(Debug, Eq, PartialEq)]
pub struct ActivitySocialMutationResult {
    plan: ActivitySocialPlan,
    activity: ActivityDetail,
}

impl ActivitySocialMutationResult {
    #[must_use]
    pub const fn plan(&self) -> &ActivitySocialPlan {
        &self.plan
    }

    #[must_use]
    pub const fn activity(&self) -> &ActivityDetail {
        &self.activity
    }
}

#[derive(Debug, Error)]
pub enum ActivitySocialMutationError {
    #[error(transparent)]
    Credential(#[from] CredentialAccessError),

    #[error("failed to read the Venmo activity before changing it: {source}")]
    Preflight {
        #[source]
        source: ApiOperationFailure,
    },

    #[error("the Venmo activity preflight violates its contract because {problem}")]
    ResponseContract { problem: &'static str },

    #[error(
        "activity social confirmation requires both stdin and stderr to be terminals; pass `--yes` to authorize non-interactively"
    )]
    ConfirmationRequired,

    #[error("activity social mutation cancelled")]
    ConfirmationDeclined,

    #[error("activity social confirmation failed: {source}")]
    Confirmation {
        #[source]
        source: PromptError,
    },

    #[error("failed to change Venmo activity social state: {source}")]
    Mutation {
        #[source]
        source: ApiOperationFailure,
    },

    #[error(
        "the activity social outcome is unknown and must be verified before retrying because {problem}"
    )]
    OutcomeUnknown { problem: &'static str },
}

impl ActivitySocialMutationError {
    #[must_use]
    pub const fn failure_kind(&self) -> ApplicationFailureKind {
        match self {
            Self::Credential(_) => ApplicationFailureKind::Credential,
            Self::Preflight { source } | Self::Mutation { source } => {
                ApplicationFailureKind::Api(source.kind())
            }
            Self::ConfirmationRequired => ApplicationFailureKind::Usage,
            Self::ResponseContract { .. } => ApplicationFailureKind::ApiContract,
            Self::ConfirmationDeclined => ApplicationFailureKind::Cancelled,
            Self::Confirmation { source } => prompt_failure_kind(source),
            Self::OutcomeUnknown { .. } => {
                ApplicationFailureKind::Api(ApiFailureKind::AmbiguousWrite)
            }
        }
    }
}

pub async fn prepare<R, A>(
    credentials: &R,
    api: &A,
    activity_id: &ActivityId,
    intent: ActivitySocialIntent,
) -> Result<PreparedActivitySocialMutation, ActivitySocialMutationError>
where
    R: CredentialReader,
    A: ActivityDetailApi,
{
    let credential = require_credential(credentials)?.envelope;
    let activity = api
        .activity_by_id(
            credential.access_token(),
            credential.device_id(),
            credential.user_id(),
            activity_id,
        )
        .await
        .map_err(|source| ActivitySocialMutationError::Preflight {
            source: ApiOperationFailure::new(source),
        })?;
    if activity.id() != activity_id {
        return Err(ActivitySocialMutationError::ResponseContract {
            problem: "the preflight returned a different activity ID",
        });
    }
    let previous_like_state = activity.social().like_state(credential.user_id());
    let action = select_action(intent);
    Ok(PreparedActivitySocialMutation {
        credential,
        plan: ActivitySocialPlan {
            activity,
            action,
            previous_like_state,
        },
    })
}

fn select_action(intent: ActivitySocialIntent) -> ActivitySocialAction {
    match intent {
        ActivitySocialIntent::AddComment(message) => ActivitySocialAction::AddComment(message),
    }
}

pub(crate) fn authorize<P>(
    prompt: &P,
    prepared: PreparedActivitySocialMutation,
    assume_yes: bool,
) -> Result<AuthorizedActivitySocialMutation, ActivitySocialMutationError>
where
    P: DefaultNoConfirmation,
{
    confirmation::authorize(prompt, assume_yes, prepared.plan.action.confirmation()).map_err(
        |error| match error {
            DefaultNoConfirmationError::Required => {
                ActivitySocialMutationError::ConfirmationRequired
            }
            DefaultNoConfirmationError::Declined => {
                ActivitySocialMutationError::ConfirmationDeclined
            }
            DefaultNoConfirmationError::Prompt(source) => {
                ActivitySocialMutationError::Confirmation { source }
            }
        },
    )?;
    Ok(AuthorizedActivitySocialMutation(prepared))
}

pub(crate) async fn execute<A>(
    api: &A,
    authorized: AuthorizedActivitySocialMutation,
) -> Result<ActivitySocialMutationResult, ActivitySocialMutationError>
where
    A: ActivitySocialMutationApi,
{
    let AuthorizedActivitySocialMutation(prepared) = authorized;
    let credential = &prepared.credential;
    let activity_id = prepared.plan.activity.id();
    let updated = match &prepared.plan.action {
        ActivitySocialAction::AddComment(message) => {
            api.add_activity_comment(
                credential.access_token(),
                credential.device_id(),
                credential.user_id(),
                activity_id,
                message,
            )
            .await
        }
    }
    .map_err(|source| ActivitySocialMutationError::Mutation {
        source: ApiOperationFailure::new(source),
    })?;
    if updated.id() != activity_id {
        return Err(ActivitySocialMutationError::OutcomeUnknown {
            problem: "the reconciled response identified a different activity",
        });
    }
    Ok(ActivitySocialMutationResult {
        plan: prepared.plan,
        activity: updated,
    })
}

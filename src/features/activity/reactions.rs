use thiserror::Error;

use super::{
    ActivityDetail, ActivityDetailApi, ActivityError, ActivityId, ActivityReaction,
    ActivityReactionEmoji, ActivityReactionMutationApi, ActivityReactionState, ActivityReactions,
};
use crate::features::auth::{PromptError, prompt_failure_kind};
use crate::features::payments::DefaultNoConfirmation;
use crate::features::payments::confirmation::{self, DefaultNoConfirmationError};
use crate::shared::{
    ApiFailureKind, ApiOperationFailure, ApplicationFailureKind, CredentialAccessError,
    CredentialEnvelope, CredentialReader, require_credential,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityReactionListResult {
    activity_id: ActivityId,
    reactions: ActivityReactions,
}

impl ActivityReactionListResult {
    #[must_use]
    pub const fn new(activity_id: ActivityId, reactions: ActivityReactions) -> Self {
        Self {
            activity_id,
            reactions,
        }
    }

    #[must_use]
    pub const fn activity_id(&self) -> &ActivityId {
        &self.activity_id
    }

    #[must_use]
    pub const fn reactions(&self) -> &ActivityReactions {
        &self.reactions
    }
}

pub async fn list<R, A>(
    credentials: &R,
    api: &A,
    activity_id: &ActivityId,
) -> Result<ActivityReactionListResult, ActivityError>
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
        .map_err(|source| ActivityError::Api {
            source: ApiOperationFailure::new(source),
        })?;
    if activity.id() != activity_id {
        return Err(ActivityError::ResponseContract {
            problem: "the reaction-list response returned a different activity ID",
        });
    }
    let reactions = activity
        .social()
        .reactions()
        .cloned()
        .ok_or(ActivityError::ReactionsUnavailable)?;
    Ok(ActivityReactionListResult::new(
        activity.id().clone(),
        reactions,
    ))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActivityReactionIntent {
    Add(ActivityReactionEmoji),
    Remove(ActivityReactionEmoji),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActivityReactionAction {
    Add(ActivityReactionEmoji),
    Remove(ActivityReactionEmoji),
}

impl ActivityReactionAction {
    #[must_use]
    pub const fn emoji(&self) -> &ActivityReactionEmoji {
        match self {
            Self::Add(emoji) | Self::Remove(emoji) => emoji,
        }
    }

    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Add(_) => "add activity reaction",
            Self::Remove(_) => "remove activity reaction",
        }
    }

    #[must_use]
    pub const fn result_label(&self) -> &'static str {
        match self {
            Self::Add(_) => "Reaction added",
            Self::Remove(_) => "Reaction removed",
        }
    }

    const fn confirmation(&self) -> &'static str {
        match self {
            Self::Add(_) => "Add this reaction?",
            Self::Remove(_) => "Remove this reaction?",
        }
    }

    #[must_use]
    pub const fn expected_state(&self) -> ActivityReactionState {
        match self {
            Self::Add(_) => ActivityReactionState::Present,
            Self::Remove(_) => ActivityReactionState::Absent,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityReactionPlan {
    activity: ActivityDetail,
    action: ActivityReactionAction,
    previous_state: ActivityReactionState,
}

impl ActivityReactionPlan {
    #[must_use]
    pub const fn activity(&self) -> &ActivityDetail {
        &self.activity
    }

    #[must_use]
    pub const fn action(&self) -> &ActivityReactionAction {
        &self.action
    }

    #[must_use]
    pub const fn previous_state(&self) -> ActivityReactionState {
        self.previous_state
    }
}

#[derive(Debug)]
pub struct PreparedActivityReactionMutation {
    credential: CredentialEnvelope,
    plan: ActivityReactionPlan,
}

impl PreparedActivityReactionMutation {
    #[must_use]
    pub const fn plan(&self) -> &ActivityReactionPlan {
        &self.plan
    }
}

#[derive(Debug)]
pub(crate) struct AuthorizedActivityReactionMutation(PreparedActivityReactionMutation);

#[derive(Debug, Eq, PartialEq)]
pub struct ActivityReactionMutationResult {
    plan: ActivityReactionPlan,
    activity: ActivityDetail,
}

impl ActivityReactionMutationResult {
    #[must_use]
    pub const fn plan(&self) -> &ActivityReactionPlan {
        &self.plan
    }

    #[must_use]
    pub const fn activity(&self) -> &ActivityDetail {
        &self.activity
    }

    #[must_use]
    pub fn reconciled_reaction(&self) -> Option<&ActivityReaction> {
        let emoji = self.plan.action().emoji();
        self.activity.social().reactions().and_then(|reactions| {
            reactions
                .items()
                .iter()
                .find(|reaction| reaction.emoji() == emoji)
        })
    }
}

#[derive(Debug, Error)]
pub enum ActivityReactionMutationError {
    #[error(transparent)]
    Credential(#[from] CredentialAccessError),

    #[error("failed to read the Venmo activity before changing its reaction: {source}")]
    Preflight {
        #[source]
        source: ApiOperationFailure,
    },

    #[error("the Venmo reaction preflight violates its contract because {problem}")]
    ResponseContract { problem: &'static str },

    #[error("the authenticated account already has this reaction on the activity")]
    AlreadyReacted,

    #[error("the authenticated account does not have this reaction on the activity")]
    NotReacted,

    #[error("the activity response cannot prove the authenticated account's reaction state")]
    ReactionStateUnavailable,

    #[error(
        "activity reaction confirmation requires both stdin and stderr to be terminals; pass `--yes` to authorize non-interactively"
    )]
    ConfirmationRequired,

    #[error("activity reaction mutation cancelled")]
    ConfirmationDeclined,

    #[error("activity reaction confirmation failed: {source}")]
    Confirmation {
        #[source]
        source: PromptError,
    },

    #[error("failed to change the Venmo activity reaction: {source}")]
    Mutation {
        #[source]
        source: ApiOperationFailure,
    },

    #[error(
        "the activity reaction outcome is unknown and must be verified before retrying because {problem}"
    )]
    OutcomeUnknown { problem: &'static str },
}

impl ActivityReactionMutationError {
    #[must_use]
    pub const fn failure_kind(&self) -> ApplicationFailureKind {
        match self {
            Self::Credential(_) => ApplicationFailureKind::Credential,
            Self::Preflight { source } | Self::Mutation { source } => {
                ApplicationFailureKind::Api(source.kind())
            }
            Self::AlreadyReacted
            | Self::NotReacted
            | Self::ReactionStateUnavailable
            | Self::ConfirmationRequired => ApplicationFailureKind::Usage,
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
    intent: ActivityReactionIntent,
) -> Result<PreparedActivityReactionMutation, ActivityReactionMutationError>
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
        .map_err(|source| ActivityReactionMutationError::Preflight {
            source: ApiOperationFailure::new(source),
        })?;
    if activity.id() != activity_id {
        return Err(ActivityReactionMutationError::ResponseContract {
            problem: "the preflight returned a different activity ID",
        });
    }
    let emoji = match &intent {
        ActivityReactionIntent::Add(emoji) | ActivityReactionIntent::Remove(emoji) => emoji,
    };
    let previous_state = activity.social().reaction_state(emoji);
    let action = select_action(intent, previous_state)?;
    Ok(PreparedActivityReactionMutation {
        credential,
        plan: ActivityReactionPlan {
            activity,
            action,
            previous_state,
        },
    })
}

fn select_action(
    intent: ActivityReactionIntent,
    previous_state: ActivityReactionState,
) -> Result<ActivityReactionAction, ActivityReactionMutationError> {
    match (intent, previous_state) {
        (ActivityReactionIntent::Add(emoji), ActivityReactionState::Absent) => {
            Ok(ActivityReactionAction::Add(emoji))
        }
        (ActivityReactionIntent::Add(_), ActivityReactionState::Present) => {
            Err(ActivityReactionMutationError::AlreadyReacted)
        }
        (ActivityReactionIntent::Remove(emoji), ActivityReactionState::Present) => {
            Ok(ActivityReactionAction::Remove(emoji))
        }
        (ActivityReactionIntent::Remove(_), ActivityReactionState::Absent) => {
            Err(ActivityReactionMutationError::NotReacted)
        }
        (
            ActivityReactionIntent::Add(_) | ActivityReactionIntent::Remove(_),
            ActivityReactionState::Unknown,
        ) => Err(ActivityReactionMutationError::ReactionStateUnavailable),
    }
}

pub(crate) fn authorize<P>(
    prompt: &P,
    prepared: PreparedActivityReactionMutation,
    assume_yes: bool,
) -> Result<AuthorizedActivityReactionMutation, ActivityReactionMutationError>
where
    P: DefaultNoConfirmation,
{
    confirmation::authorize(prompt, assume_yes, prepared.plan.action.confirmation()).map_err(
        |error| match error {
            DefaultNoConfirmationError::Required => {
                ActivityReactionMutationError::ConfirmationRequired
            }
            DefaultNoConfirmationError::Declined => {
                ActivityReactionMutationError::ConfirmationDeclined
            }
            DefaultNoConfirmationError::Prompt(source) => {
                ActivityReactionMutationError::Confirmation { source }
            }
        },
    )?;
    Ok(AuthorizedActivityReactionMutation(prepared))
}

pub(crate) async fn execute<A>(
    api: &A,
    authorized: AuthorizedActivityReactionMutation,
) -> Result<ActivityReactionMutationResult, ActivityReactionMutationError>
where
    A: ActivityReactionMutationApi,
{
    let AuthorizedActivityReactionMutation(prepared) = authorized;
    let credential = &prepared.credential;
    let activity_id = prepared.plan.activity.id();
    let updated = match &prepared.plan.action {
        ActivityReactionAction::Add(emoji) => {
            api.add_activity_reaction(
                credential.access_token(),
                credential.device_id(),
                credential.user_id(),
                activity_id,
                emoji,
            )
            .await
        }
        ActivityReactionAction::Remove(emoji) => {
            api.remove_activity_reaction(
                credential.access_token(),
                credential.device_id(),
                credential.user_id(),
                activity_id,
                emoji,
            )
            .await
        }
    }
    .map_err(|source| ActivityReactionMutationError::Mutation {
        source: ApiOperationFailure::new(source),
    })?;
    if updated.id() != activity_id {
        return Err(ActivityReactionMutationError::OutcomeUnknown {
            problem: "the reconciled response identified a different activity",
        });
    }
    Ok(ActivityReactionMutationResult {
        plan: prepared.plan,
        activity: updated,
    })
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::str::FromStr;

    use super::*;

    #[test]
    fn explicit_reaction_intents_require_authoritative_current_state() -> Result<(), Box<dyn Error>>
    {
        let emoji = ActivityReactionEmoji::from_str("🔥")?;
        assert!(matches!(
            select_action(
                ActivityReactionIntent::Add(emoji.clone()),
                ActivityReactionState::Present
            ),
            Err(ActivityReactionMutationError::AlreadyReacted)
        ));
        assert!(matches!(
            select_action(
                ActivityReactionIntent::Remove(emoji.clone()),
                ActivityReactionState::Absent
            ),
            Err(ActivityReactionMutationError::NotReacted)
        ));
        assert!(matches!(
            select_action(
                ActivityReactionIntent::Add(emoji),
                ActivityReactionState::Unknown
            ),
            Err(ActivityReactionMutationError::ReactionStateUnavailable)
        ));
        Ok(())
    }
}

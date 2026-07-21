use super::{ActivityCommentId, ActivityCommentRemovalApi};
use crate::features::activity::social::ActivitySocialMutationError;
use crate::features::payments::DefaultNoConfirmation;
use crate::features::payments::confirmation::{self, DefaultNoConfirmationError};
use crate::shared::{CredentialEnvelope, CredentialReader, require_credential};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityCommentRemovalPlan {
    comment_id: ActivityCommentId,
}

impl ActivityCommentRemovalPlan {
    #[must_use]
    pub const fn comment_id(&self) -> &ActivityCommentId {
        &self.comment_id
    }
}

#[derive(Debug)]
pub struct PreparedActivityCommentRemoval {
    credential: CredentialEnvelope,
    plan: ActivityCommentRemovalPlan,
}

impl PreparedActivityCommentRemoval {
    #[must_use]
    pub const fn plan(&self) -> &ActivityCommentRemovalPlan {
        &self.plan
    }
}

#[derive(Debug)]
pub(crate) struct AuthorizedActivityCommentRemoval(PreparedActivityCommentRemoval);

#[derive(Debug, Eq, PartialEq)]
pub struct ActivityCommentRemovalResult {
    plan: ActivityCommentRemovalPlan,
}

impl ActivityCommentRemovalResult {
    #[must_use]
    pub const fn plan(&self) -> &ActivityCommentRemovalPlan {
        &self.plan
    }
}

pub fn prepare<R>(
    credentials: &R,
    comment_id: ActivityCommentId,
) -> Result<PreparedActivityCommentRemoval, ActivitySocialMutationError>
where
    R: CredentialReader,
{
    let credential = require_credential(credentials)?.envelope;
    Ok(PreparedActivityCommentRemoval {
        credential,
        plan: ActivityCommentRemovalPlan { comment_id },
    })
}

pub(crate) fn authorize<P>(
    prompt: &P,
    prepared: PreparedActivityCommentRemoval,
    assume_yes: bool,
) -> Result<AuthorizedActivityCommentRemoval, ActivitySocialMutationError>
where
    P: DefaultNoConfirmation,
{
    confirmation::authorize(prompt, assume_yes, "Remove this comment?").map_err(
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
    Ok(AuthorizedActivityCommentRemoval(prepared))
}

pub(crate) async fn execute<A>(
    api: &A,
    authorized: AuthorizedActivityCommentRemoval,
) -> Result<ActivityCommentRemovalResult, ActivitySocialMutationError>
where
    A: ActivityCommentRemovalApi,
{
    let AuthorizedActivityCommentRemoval(prepared) = authorized;
    api.remove_activity_comment(
        prepared.credential.access_token(),
        prepared.credential.device_id(),
        prepared.plan.comment_id(),
    )
    .await
    .map_err(|source| ActivitySocialMutationError::Mutation {
        source: crate::shared::ApiOperationFailure::new(source),
    })?;
    Ok(ActivityCommentRemovalResult {
        plan: prepared.plan,
    })
}

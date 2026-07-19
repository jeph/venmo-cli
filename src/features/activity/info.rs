use super::{ActivityDetail, ActivityDetailApi, ActivityError, ActivityId};
use crate::shared::{ApiOperationFailure, CredentialReader, require_credential};

#[derive(Debug, Eq, PartialEq)]
pub struct ActivityInfoResult {
    activity: ActivityDetail,
}

impl ActivityInfoResult {
    #[must_use]
    pub(crate) const fn new(activity: ActivityDetail) -> Self {
        Self { activity }
    }

    #[must_use]
    pub const fn activity(&self) -> &ActivityDetail {
        &self.activity
    }
}

pub async fn info<R, A>(
    credentials: &R,
    api: &A,
    activity_id: &ActivityId,
) -> Result<ActivityInfoResult, ActivityError>
where
    R: CredentialReader,
    A: ActivityDetailApi,
{
    let loaded = require_credential(credentials)?;
    let activity = api
        .activity_by_id(
            loaded.envelope.access_token(),
            loaded.envelope.device_id(),
            loaded.envelope.user_id(),
            activity_id,
        )
        .await
        .map_err(|source| ActivityError::Api {
            source: ApiOperationFailure::new(source),
        })?;
    if activity.id() != activity_id {
        return Err(ActivityError::ResponseContract {
            problem: "the API returned a different activity ID",
        });
    }
    Ok(ActivityInfoResult::new(activity))
}

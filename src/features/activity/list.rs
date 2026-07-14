use super::{Activity, ActivityBeforeId, ActivityError, ActivityListApi, ActivityPageRequest};
use crate::shared::{ApiOperationFailure, CredentialReader, FirstSeen, FirstSeenResult, Limit};

#[derive(Debug, Eq, PartialEq)]
pub struct ActivityListResult {
    activities: Vec<Activity>,
    next_before_id: Option<ActivityBeforeId>,
}

impl ActivityListResult {
    #[must_use]
    pub(crate) fn new(activities: Vec<Activity>, next_before_id: Option<ActivityBeforeId>) -> Self {
        Self {
            activities,
            next_before_id,
        }
    }

    #[must_use]
    pub fn activities(&self) -> &[Activity] {
        &self.activities
    }

    #[must_use]
    pub const fn next_before_id(&self) -> Option<&ActivityBeforeId> {
        self.next_before_id.as_ref()
    }
}

pub async fn list<R, A>(
    credentials: &R,
    api: &A,
    limit: Limit,
    before_id: Option<&ActivityBeforeId>,
) -> Result<ActivityListResult, ActivityError>
where
    R: CredentialReader,
    A: ActivityListApi,
{
    let loaded = crate::shared::require_credential(credentials)?;
    let current = before_id.cloned();
    let page = api
        .activity(
            loaded.envelope.access_token(),
            loaded.envelope.device_id(),
            loaded.envelope.user_id(),
            ActivityPageRequest::new(limit, current.clone()),
        )
        .await
        .map_err(|source| ActivityError::Api {
            source: ApiOperationFailure::new(source),
        })?;
    let (page_activities, next_token) = page.into_parts();
    validate_page_len(page_activities.len(), limit)?;
    validate_next_token(current.as_ref(), next_token.as_ref())?;
    if page_activities.is_empty() && next_token.is_some() {
        return Err(ActivityError::ResponseContract {
            problem: "the API returned an empty page with a before-id continuation",
        });
    }
    let activities = buffer_activities(page_activities)?;
    Ok(ActivityListResult::new(activities, next_token))
}

fn buffer_activities(page: Vec<Activity>) -> Result<Vec<Activity>, ActivityError> {
    let mut activities = Vec::with_capacity(page.len());
    let mut seen = FirstSeen::with_capacity(page.len());
    for activity in page {
        match seen.push(&mut activities, activity.id().clone(), activity) {
            FirstSeenResult::First | FirstSeenResult::Duplicate => {}
            FirstSeenResult::Conflicting => {
                return Err(ActivityError::ResponseContract {
                    problem: "the API returned conflicting records for one activity ID",
                });
            }
        }
    }
    Ok(activities)
}

fn validate_next_token(
    current: Option<&ActivityBeforeId>,
    next: Option<&ActivityBeforeId>,
) -> Result<(), ActivityError> {
    if current == next && next.is_some() {
        return Err(ActivityError::ResponseContract {
            problem: "the API returned a repeated or non-progressing before-id continuation",
        });
    }
    Ok(())
}

fn validate_page_len(actual: usize, requested: Limit) -> Result<(), ActivityError> {
    if actual > requested.get() as usize {
        return Err(ActivityError::ResponseContract {
            problem: "the API returned more activity records than requested",
        });
    }
    Ok(())
}

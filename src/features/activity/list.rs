use super::{
    Activity, ActivityBeforeId, ActivityError, ActivityFeedKind, ActivityFeedScope,
    ActivityListApi, ActivityPageRequest, ActivitySubject,
};
use crate::features::people::{UserLookupApi, UserProfileKind, UserSearchApi, lookup};
use crate::shared::{
    ApiOperationFailure, CredentialReader, FirstSeen, FirstSeenResult, Limit, Username,
    require_credential,
};

#[derive(Debug, Eq, PartialEq)]
pub struct ActivityListResult {
    subject: Option<ActivitySubject>,
    activities: Vec<Activity>,
    next_before_id: Option<ActivityBeforeId>,
}

impl ActivityListResult {
    #[must_use]
    pub(crate) fn new(activities: Vec<Activity>, next_before_id: Option<ActivityBeforeId>) -> Self {
        Self {
            subject: None,
            activities,
            next_before_id,
        }
    }

    #[must_use]
    pub(crate) fn for_subject(
        subject: ActivitySubject,
        activities: Vec<Activity>,
        next_before_id: Option<ActivityBeforeId>,
    ) -> Self {
        Self {
            subject: Some(subject),
            activities,
            next_before_id,
        }
    }

    #[must_use]
    pub const fn subject(&self) -> Option<&ActivitySubject> {
        self.subject.as_ref()
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
    let loaded = require_credential(credentials)?;
    let scope = ActivityFeedScope::new(
        loaded.envelope.user_id().clone(),
        loaded.envelope.user_id().clone(),
        ActivityFeedKind::CurrentUser,
    );
    list_page(&loaded.envelope, api, &scope, None, limit, before_id).await
}

pub async fn list_for_user<R, A>(
    credentials: &R,
    api: &A,
    username: &Username,
    limit: Limit,
    before_id: Option<&ActivityBeforeId>,
) -> Result<ActivityListResult, ActivityError>
where
    R: CredentialReader,
    A: ActivityListApi + UserLookupApi + UserSearchApi,
{
    let loaded = require_credential(credentials)?;
    let (scope, subject) = resolve_subject(&loaded.envelope, api, username).await?;
    list_page(&loaded.envelope, api, &scope, subject, limit, before_id).await
}

async fn list_page<A>(
    credential: &crate::shared::CredentialEnvelope,
    api: &A,
    scope: &ActivityFeedScope,
    subject: Option<ActivitySubject>,
    limit: Limit,
    before_id: Option<&ActivityBeforeId>,
) -> Result<ActivityListResult, ActivityError>
where
    A: ActivityListApi,
{
    let scope = ActivityFeedScope::new(
        scope.viewer_user_id().clone(),
        scope.subject_user_id().clone(),
        scope.kind(),
    );
    let current = before_id.cloned();
    let page = api
        .activity(
            credential.access_token(),
            credential.device_id(),
            &scope,
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
    Ok(match subject {
        Some(subject) => ActivityListResult::for_subject(subject, activities, next_token),
        None => ActivityListResult::new(activities, next_token),
    })
}

async fn resolve_subject<A>(
    credential: &crate::shared::CredentialEnvelope,
    api: &A,
    username: &Username,
) -> Result<(ActivityFeedScope, Option<ActivitySubject>), ActivityError>
where
    A: UserLookupApi + UserSearchApi,
{
    if username.as_str().to_lowercase() == credential.username().as_str().to_lowercase() {
        return Ok((
            ActivityFeedScope::new(
                credential.user_id().clone(),
                credential.user_id().clone(),
                ActivityFeedKind::CurrentUser,
            ),
            None,
        ));
    }
    let user = lookup::resolve_with_credential(credential, api, username)
        .await
        .map_err(|source| ActivityError::UserLookup { source })?;
    let subject = subject_from_user(user)?;
    Ok((
        ActivityFeedScope::new(
            credential.user_id().clone(),
            subject.user_id().clone(),
            ActivityFeedKind::OtherPersonalUser,
        ),
        Some(subject),
    ))
}

pub(super) fn subject_from_user(
    user: crate::features::people::User,
) -> Result<ActivitySubject, ActivityError> {
    match user.profile_kind() {
        Some(UserProfileKind::Personal) => {}
        Some(UserProfileKind::Business | UserProfileKind::Charity | UserProfileKind::Unknown) => {
            return Err(ActivityError::UnsupportedProfileType);
        }
        None => return Err(ActivityError::MissingProfileType),
    }
    let resolved_username = user
        .username()
        .cloned()
        .ok_or(ActivityError::ResponseContract {
            problem: "the authoritative activity subject omitted its username",
        })?;
    let subject = ActivitySubject::new(
        user.user_id().clone(),
        resolved_username,
        ActivityFeedKind::OtherPersonalUser,
    );
    Ok(subject)
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

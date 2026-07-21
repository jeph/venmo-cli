use super::{ActivityComment, ActivityDetailApi, ActivityError, ActivityId};
use crate::shared::{ApiOperationFailure, CredentialReader, Limit, Offset, require_credential};

#[derive(Debug, Eq, PartialEq)]
pub struct ActivityCommentListResult {
    activity_id: ActivityId,
    comments: Vec<ActivityComment>,
    total_count: u64,
    offset: Offset,
    next_offset: Option<Offset>,
}

impl ActivityCommentListResult {
    #[must_use]
    pub(crate) fn new(
        activity_id: ActivityId,
        comments: Vec<ActivityComment>,
        total_count: u64,
        offset: Offset,
        next_offset: Option<Offset>,
    ) -> Self {
        Self {
            activity_id,
            comments,
            total_count,
            offset,
            next_offset,
        }
    }

    #[must_use]
    pub const fn activity_id(&self) -> &ActivityId {
        &self.activity_id
    }

    #[must_use]
    pub fn comments(&self) -> &[ActivityComment] {
        &self.comments
    }

    #[must_use]
    pub const fn total_count(&self) -> u64 {
        self.total_count
    }

    #[must_use]
    pub const fn offset(&self) -> Offset {
        self.offset
    }

    #[must_use]
    pub const fn next_offset(&self) -> Option<Offset> {
        self.next_offset
    }
}

pub async fn list<R, A>(
    credentials: &R,
    api: &A,
    activity_id: &ActivityId,
    limit: Limit,
    offset: Offset,
) -> Result<ActivityCommentListResult, ActivityError>
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
    let collection = activity
        .social()
        .comments()
        .ok_or(ActivityError::CommentsUnavailable)?;
    if !collection.is_complete() {
        return Err(ActivityError::CommentsIncomplete);
    }

    let available = collection.items();
    let requested_start = usize::try_from(offset.get()).unwrap_or(usize::MAX);
    let start = requested_start.min(available.len());
    let page_size = usize::try_from(limit.get()).unwrap_or(usize::MAX);
    let end = start.saturating_add(page_size).min(available.len());
    let comments = available[start..end].to_vec();
    let next_offset = if end < available.len() {
        let value = u32::try_from(end).map_err(|_| ActivityError::ResponseContract {
            problem: "the complete activity comment collection exceeded the supported offset range",
        })?;
        Some(Offset::new(value))
    } else {
        None
    };

    Ok(ActivityCommentListResult::new(
        activity_id.clone(),
        comments,
        collection.count(),
        offset,
        next_offset,
    ))
}

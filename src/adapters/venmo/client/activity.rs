use std::collections::HashSet;
use std::future::Future;
use std::str::FromStr;

use crate::features::activity::{
    Activity, ActivityAction, ActivityBeforeId, ActivityComment, ActivityCommentId,
    ActivityCommentMessage, ActivityCommentRemovalApi, ActivityCounterparty, ActivityDetail,
    ActivityDetailApi, ActivityDirection, ActivityFeedKind, ActivityFeedScope, ActivityId,
    ActivityLikeState, ActivityListApi, ActivityPage, ActivityPageRequest, ActivitySocial,
    ActivitySocialCollection, ActivitySocialMutationApi, ActivityStatus,
};
use crate::features::people::User;
use crate::shared::{AccessToken, DeviceId, Limit, Money, UserId};

use super::super::dto::{
    ActivityCommentDto, ActivityCommentEnvelope, ActivityPaymentRecordDto,
    AddActivityCommentRequest, AuthorizationDto, StoriesEnvelope, StoryDto, StoryEnvelope,
    StorySocialCollectionDto, TransferDto, UserDto,
};
use super::super::transport::{ApiSession, ApiTransport, HttpRequest, JsonBody};
use super::error::VenmoApiError;
use super::response::{
    decode_success, require_state_write_success, require_state_write_success_json,
};
use super::support::{
    bounded_optional_label, bounded_optional_text, bounded_required_label, bounded_required_text,
    map_user, parse_required_timestamp, require_query_value, require_query_value_case_insensitive,
    validate_page_count, validate_query_keys,
};
use super::{
    ACTIVITY_COMMENT_ADD_OPERATION, ACTIVITY_COMMENT_REMOVE_OPERATION, ACTIVITY_DETAIL_OPERATION,
    ACTIVITY_LIKE_OPERATION, ACTIVITY_LIST_OPERATION, ACTIVITY_UNLIKE_OPERATION, VenmoApiClient,
};

impl<T: ApiTransport> VenmoApiClient<T> {
    pub(super) async fn fetch_activity_page(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        scope: &ActivityFeedScope,
        page: ActivityPageRequest,
    ) -> Result<ActivityPage, VenmoApiError> {
        let limit_value = page.page_size().get().to_string();
        let mut query = vec![("limit", limit_value.as_str())];
        if scope.kind() == ActivityFeedKind::CurrentUser {
            query.push(("social_only", "false"));
        }
        query.extend(page.before_id().map(|id| ("before_id", id.as_str())));
        let path_segments = [
            "stories",
            "target-or-actor",
            scope.subject_user_id().as_str(),
        ];
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::read("/stories/target-or-actor/{user-id}", &path_segments, &query),
            )
            .await?;
        let envelope: StoriesEnvelope = decode_success(
            ACTIVITY_LIST_OPERATION,
            response,
            "the activity response did not match the supported envelope",
        )?;
        let activities = envelope
            .data
            .into_iter()
            .map(|story| match scope.kind() {
                ActivityFeedKind::CurrentUser => map_activity(story, scope.subject_user_id()),
                ActivityFeedKind::OtherPersonalUser => {
                    map_other_user_activity(story, scope.viewer_user_id(), scope.subject_user_id())
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        validate_page_count(ACTIVITY_LIST_OPERATION, activities.len(), page.page_size())?;
        let next_token = self.parse_activity_next_link(
            envelope.pagination.next.as_deref(),
            &path_segments,
            page.page_size(),
            scope.kind(),
        )?;
        Ok(ActivityPage::new(activities, next_token))
    }

    pub(super) fn parse_activity_next_link(
        &self,
        raw: Option<&str>,
        path_segments: &[&str],
        page_size: Limit,
        kind: ActivityFeedKind,
    ) -> Result<Option<ActivityBeforeId>, VenmoApiError> {
        let Some(raw) = raw else {
            return Ok(None);
        };
        let pairs = self.transport.parse_trusted_next_link(raw, path_segments)?;
        let query = validate_query_keys(
            ACTIVITY_LIST_OPERATION,
            &pairs,
            &["before_id", "limit", "only_public_stories", "social_only"],
        )?;
        require_query_value(
            ACTIVITY_LIST_OPERATION,
            &query,
            "limit",
            &page_size.get().to_string(),
        )?;
        match kind {
            ActivityFeedKind::CurrentUser => require_query_value_case_insensitive(
                ACTIVITY_LIST_OPERATION,
                &query,
                "social_only",
                "false",
            )?,
            ActivityFeedKind::OtherPersonalUser => {
                if let Some(value) = query.get("social_only")
                    && !value.eq_ignore_ascii_case("false")
                {
                    return Err(VenmoApiError::Contract {
                        operation: ACTIVITY_LIST_OPERATION,
                        problem: "the activity continuation changed the social filter",
                    });
                }
            }
        }
        if let Some(value) = query.get("only_public_stories")
            && !value.eq_ignore_ascii_case("false")
        {
            return Err(VenmoApiError::Contract {
                operation: ACTIVITY_LIST_OPERATION,
                problem: "the activity continuation changed the visibility filter",
            });
        }
        let before_id = query.get("before_id").ok_or(VenmoApiError::Contract {
            operation: ACTIVITY_LIST_OPERATION,
            problem: "the activity continuation omitted its before-id value",
        })?;
        let before_id =
            ActivityBeforeId::from_str(before_id).map_err(|_| VenmoApiError::Contract {
                operation: ACTIVITY_LIST_OPERATION,
                problem: "the activity continuation contained an invalid before-id value",
            })?;
        Ok(Some(before_id))
    }

    async fn fetch_activity_by_id(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        current_user_id: &UserId,
        activity_id: &ActivityId,
    ) -> Result<ActivityDetail, VenmoApiError> {
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::read(
                    "/stories/{story-id}",
                    &["stories", activity_id.as_str()],
                    &[],
                ),
            )
            .await?;
        let envelope: StoryEnvelope = decode_success(
            ACTIVITY_DETAIL_OPERATION,
            response,
            "the activity-detail response did not match the supported envelope",
        )?;
        let activity = map_activity_detail(envelope.data.into_story(), current_user_id)?;
        if activity.id() != activity_id {
            return Err(VenmoApiError::Contract {
                operation: ACTIVITY_DETAIL_OPERATION,
                problem: "the activity-detail response returned a different activity ID",
            });
        }
        Ok(activity)
    }
}

impl<T: ApiTransport> ActivityListApi for VenmoApiClient<T> {
    type Error = VenmoApiError;
    fn activity<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        scope: &'a ActivityFeedScope,
        page: ActivityPageRequest,
    ) -> impl Future<Output = Result<ActivityPage, Self::Error>> + Send + 'a {
        self.fetch_activity_page(access_token, device_id, scope, page)
    }
}

impl<T: ApiTransport> ActivityDetailApi for VenmoApiClient<T> {
    type Error = VenmoApiError;
    fn activity_by_id<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        activity_id: &'a ActivityId,
    ) -> impl Future<Output = Result<ActivityDetail, Self::Error>> + Send + 'a {
        self.fetch_activity_by_id(access_token, device_id, current_user_id, activity_id)
    }
}

impl<T: ApiTransport> VenmoApiClient<T> {
    async fn reconcile_activity_social(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        current_user_id: &UserId,
        activity_id: &ActivityId,
        operation: &'static str,
    ) -> Result<ActivityDetail, VenmoApiError> {
        self.fetch_activity_by_id(access_token, device_id, current_user_id, activity_id)
            .await
            .map_err(|_| VenmoApiError::StateMutationOutcomeUnknown {
                operation,
                problem: "the activity could not be re-read after the state mutation",
            })
    }

    async fn set_activity_like(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        current_user_id: &UserId,
        activity_id: &ActivityId,
        like: bool,
    ) -> Result<ActivityDetail, VenmoApiError> {
        let operation = if like {
            ACTIVITY_LIKE_OPERATION
        } else {
            ACTIVITY_UNLIKE_OPERATION
        };
        let path_segments = ["stories", activity_id.as_str(), "likes"];
        let request = if like {
            HttpRequest::state_post("/stories/{story-id}/likes", &path_segments, &[])
        } else {
            HttpRequest::state_delete("/stories/{story-id}/likes", &path_segments, &[])
        };
        let response = self
            .transport
            .send_authenticated(ApiSession::new(access_token, device_id), request)
            .await?;
        require_state_write_success(operation, response)?;
        let activity = self
            .reconcile_activity_social(
                access_token,
                device_id,
                current_user_id,
                activity_id,
                operation,
            )
            .await?;
        let expected = if like {
            ActivityLikeState::Liked
        } else {
            ActivityLikeState::NotLiked
        };
        if activity.social().like_state(current_user_id) != expected {
            return Err(VenmoApiError::StateMutationOutcomeUnknown {
                operation,
                problem: "the refreshed activity did not prove the requested like state",
            });
        }
        Ok(activity)
    }

    async fn create_activity_comment(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        current_user_id: &UserId,
        activity_id: &ActivityId,
        message: &ActivityCommentMessage,
    ) -> Result<ActivityDetail, VenmoApiError> {
        let body = JsonBody::encode(&AddActivityCommentRequest {
            message: message.as_str(),
        })
        .map_err(|_| VenmoApiError::RequestEncoding {
            operation: ACTIVITY_COMMENT_ADD_OPERATION,
        })?;
        let path_segments = ["stories", activity_id.as_str(), "comments"];
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::state_json_post(
                    "/stories/{story-id}/comments",
                    &path_segments,
                    &[],
                    body,
                ),
            )
            .await?;
        let value = require_state_write_success_json(ACTIVITY_COMMENT_ADD_OPERATION, response)?;
        let envelope: ActivityCommentEnvelope = serde_json::from_value(value).map_err(|_| {
            VenmoApiError::StateMutationOutcomeUnknown {
                operation: ACTIVITY_COMMENT_ADD_OPERATION,
                problem: "the successful response did not contain a valid created comment",
            }
        })?;
        let created =
            map_activity_comment(envelope.data.into_comment(), ACTIVITY_COMMENT_ADD_OPERATION)
                .map_err(|_| VenmoApiError::StateMutationOutcomeUnknown {
                    operation: ACTIVITY_COMMENT_ADD_OPERATION,
                    problem: "the successful response contained an invalid created comment",
                })?;
        if created.author().user_id() != current_user_id || created.message() != message.as_str() {
            return Err(VenmoApiError::StateMutationOutcomeUnknown {
                operation: ACTIVITY_COMMENT_ADD_OPERATION,
                problem: "the created comment did not match the authenticated author and message",
            });
        }
        let activity = self
            .reconcile_activity_social(
                access_token,
                device_id,
                current_user_id,
                activity_id,
                ACTIVITY_COMMENT_ADD_OPERATION,
            )
            .await?;
        let comments =
            activity
                .social()
                .comments()
                .ok_or(VenmoApiError::StateMutationOutcomeUnknown {
                    operation: ACTIVITY_COMMENT_ADD_OPERATION,
                    problem: "the refreshed activity omitted its comments",
                })?;
        let matches = comments.items().iter().filter(|comment| {
            comment.id() == created.id()
                && comment.author().user_id() == current_user_id
                && comment.message() == message.as_str()
        });
        if matches.count() != 1 {
            return Err(VenmoApiError::StateMutationOutcomeUnknown {
                operation: ACTIVITY_COMMENT_ADD_OPERATION,
                problem: "the refreshed activity did not prove the created comment",
            });
        }
        Ok(activity)
    }

    async fn delete_activity_comment(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        comment_id: &ActivityCommentId,
    ) -> Result<(), VenmoApiError> {
        let path_segments = ["comments", comment_id.as_str()];
        let response = self
            .transport
            .send_authenticated(
                ApiSession::new(access_token, device_id),
                HttpRequest::state_delete("/comments/{comment-id}", &path_segments, &[]),
            )
            .await?;
        if response.status().as_u16() == 404 {
            return Err(VenmoApiError::ActivityCommentNotFound);
        }
        require_state_write_success(ACTIVITY_COMMENT_REMOVE_OPERATION, response)?;
        Ok(())
    }
}

impl<T: ApiTransport> ActivitySocialMutationApi for VenmoApiClient<T> {
    type Error = VenmoApiError;

    fn like_activity<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        activity_id: &'a ActivityId,
    ) -> impl Future<Output = Result<ActivityDetail, Self::Error>> + Send + 'a {
        self.set_activity_like(access_token, device_id, current_user_id, activity_id, true)
    }

    fn unlike_activity<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        activity_id: &'a ActivityId,
    ) -> impl Future<Output = Result<ActivityDetail, Self::Error>> + Send + 'a {
        self.set_activity_like(access_token, device_id, current_user_id, activity_id, false)
    }

    fn add_activity_comment<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        current_user_id: &'a UserId,
        activity_id: &'a ActivityId,
        message: &'a ActivityCommentMessage,
    ) -> impl Future<Output = Result<ActivityDetail, Self::Error>> + Send + 'a {
        self.create_activity_comment(
            access_token,
            device_id,
            current_user_id,
            activity_id,
            message,
        )
    }
}

impl<T: ApiTransport> ActivityCommentRemovalApi for VenmoApiClient<T> {
    type Error = VenmoApiError;

    fn remove_activity_comment<'a>(
        &'a self,
        access_token: &'a AccessToken,
        device_id: &'a DeviceId,
        comment_id: &'a ActivityCommentId,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
        self.delete_activity_comment(access_token, device_id, comment_id)
    }
}

pub(super) fn map_activity(
    story: StoryDto,
    current_user_id: &UserId,
) -> Result<Activity, VenmoApiError> {
    let operation = ACTIVITY_LIST_OPERATION;
    let StoryDto {
        id,
        date_created: story_created,
        note: story_note,
        audience: story_audience,
        payment,
        transfer,
        authorization,
        likes: _,
        comments: _,
    } = story;
    let id = ActivityId::from_str(&id.into_string()).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the activity response contained an invalid activity ID",
    })?;
    let metadata = StoryMetadata {
        id,
        created_at: story_created,
        note: story_note,
        audience: story_audience,
    };
    match (payment, transfer, authorization) {
        (Some(payment), None, None) => {
            map_payment_activity(metadata, payment, current_user_id, operation)
        }
        (None, Some(transfer), None) => map_transfer_activity(metadata, transfer, operation),
        (None, None, Some(authorization)) => {
            map_authorization_activity(metadata, authorization, current_user_id, operation)
        }
        _ => Err(VenmoApiError::Contract {
            operation,
            problem: "the activity response contained an unsupported or ambiguous record type",
        }),
    }
}

fn map_other_user_activity(
    story: StoryDto,
    viewer_user_id: &UserId,
    subject_user_id: &UserId,
) -> Result<Activity, VenmoApiError> {
    let StoryDto {
        id,
        date_created,
        note,
        audience,
        payment,
        transfer,
        authorization,
        likes: _,
        comments: _,
    } = story;
    let id = ActivityId::from_str(&id.into_string()).map_err(|_| VenmoApiError::Contract {
        operation: ACTIVITY_LIST_OPERATION,
        problem: "the activity response contained an invalid activity ID",
    })?;
    match (payment, transfer, authorization) {
        (Some(payment), None, None) => map_payment_activity_for_scope(
            StoryMetadata {
                id,
                created_at: date_created,
                note,
                audience,
            },
            payment,
            subject_user_id,
            Some(viewer_user_id),
            ACTIVITY_LIST_OPERATION,
        ),
        _ => Err(VenmoApiError::Contract {
            operation: ACTIVITY_LIST_OPERATION,
            problem: "another user's activity feed contained a nonpayment or ambiguous record",
        }),
    }
}

fn map_activity_detail(
    story: StoryDto,
    current_user_id: &UserId,
) -> Result<ActivityDetail, VenmoApiError> {
    let StoryDto {
        id,
        date_created,
        note,
        audience,
        payment,
        transfer,
        authorization,
        likes,
        comments,
    } = story;
    let id = ActivityId::from_str(&id.into_string()).map_err(|_| VenmoApiError::Contract {
        operation: ACTIVITY_DETAIL_OPERATION,
        problem: "the activity response contained an invalid activity ID",
    })?;
    let metadata = StoryMetadata {
        id,
        created_at: date_created,
        note,
        audience,
    };
    let social = map_activity_social(likes, comments, ACTIVITY_DETAIL_OPERATION)?;
    let detail = match (payment, transfer, authorization) {
        (Some(payment), None, None) => map_payment_detail(
            metadata,
            payment,
            current_user_id,
            ACTIVITY_DETAIL_OPERATION,
        ),
        (None, Some(transfer), None) => {
            map_transfer_activity(metadata, transfer, ACTIVITY_DETAIL_OPERATION)
                .map(ActivityDetail::relative)
        }
        (None, None, Some(authorization)) => map_authorization_activity(
            metadata,
            authorization,
            current_user_id,
            ACTIVITY_DETAIL_OPERATION,
        )
        .map(ActivityDetail::relative),
        _ => Err(VenmoApiError::Contract {
            operation: ACTIVITY_DETAIL_OPERATION,
            problem: "the activity response contained an unsupported or ambiguous record type",
        }),
    }?;
    Ok(detail.with_social(social))
}

fn map_activity_social(
    likes: Option<StorySocialCollectionDto<UserDto>>,
    comments: Option<StorySocialCollectionDto<ActivityCommentDto>>,
    operation: &'static str,
) -> Result<ActivitySocial, VenmoApiError> {
    let likes = likes
        .map(|collection| {
            let StorySocialCollectionDto {
                count,
                data,
                pagination,
            } = collection;
            validate_embedded_count(count, data.len(), operation, "like")?;
            let actual = data.len();
            let users = data
                .into_iter()
                .map(|user| map_user(user, operation))
                .collect::<Result<Vec<_>, _>>()?;
            let mut ids = HashSet::with_capacity(users.len());
            if users
                .iter()
                .any(|user| !ids.insert(user.user_id().as_str()))
            {
                return Err(VenmoApiError::Contract {
                    operation,
                    problem: "the activity response contained duplicate liker IDs",
                });
            }
            Ok(ActivitySocialCollection::new(
                count,
                users,
                pagination
                    .as_ref()
                    .and_then(|page| page.next.as_ref())
                    .is_none()
                    && count == u64::try_from(actual).unwrap_or(u64::MAX),
            ))
        })
        .transpose()?;
    let comments = comments
        .map(|collection| {
            let StorySocialCollectionDto {
                count,
                data,
                pagination,
            } = collection;
            validate_embedded_count(count, data.len(), operation, "comment")?;
            let actual = data.len();
            let comments = data
                .into_iter()
                .map(|comment| map_activity_comment(comment, operation))
                .collect::<Result<Vec<_>, _>>()?;
            let mut ids = HashSet::with_capacity(comments.len());
            if comments
                .iter()
                .any(|comment| !ids.insert(comment.id().as_str()))
            {
                return Err(VenmoApiError::Contract {
                    operation,
                    problem: "the activity response contained duplicate comment IDs",
                });
            }
            Ok(ActivitySocialCollection::new(
                count,
                comments,
                pagination
                    .as_ref()
                    .and_then(|page| page.next.as_ref())
                    .is_none()
                    && count == u64::try_from(actual).unwrap_or(u64::MAX),
            ))
        })
        .transpose()?;
    Ok(ActivitySocial::new(likes, comments))
}

fn validate_embedded_count(
    count: u64,
    actual: usize,
    operation: &'static str,
    kind: &'static str,
) -> Result<(), VenmoApiError> {
    if count < u64::try_from(actual).unwrap_or(u64::MAX) {
        return Err(VenmoApiError::Contract {
            operation,
            problem: match kind {
                "like" => "the activity response's like count was smaller than its liker data",
                _ => "the activity response's comment count was smaller than its comment data",
            },
        });
    }
    Ok(())
}

pub(super) fn map_activity_comment(
    comment: ActivityCommentDto,
    operation: &'static str,
) -> Result<ActivityComment, VenmoApiError> {
    let id = ActivityCommentId::from_str(&comment.id.into_string()).map_err(|_| {
        VenmoApiError::Contract {
            operation,
            problem: "the activity response contained an invalid comment ID",
        }
    })?;
    let author = map_user(comment.user, operation)?;
    let message = bounded_required_text(
        comment.message,
        operation,
        "the activity response contained an invalid comment message",
    )?;
    let created_at = parse_required_timestamp(
        Some(comment.date_created.as_str()),
        operation,
        "the activity response contained an invalid comment timestamp",
    )?;
    Ok(ActivityComment::new(id, author, message, created_at))
}

struct StoryMetadata {
    id: ActivityId,
    created_at: Option<String>,
    note: Option<String>,
    audience: Option<String>,
}

fn map_authorization_activity(
    story: StoryMetadata,
    authorization: AuthorizationDto,
    current_user_id: &UserId,
    operation: &'static str,
) -> Result<Activity, VenmoApiError> {
    let AuthorizationDto {
        id: authorization_id,
        status,
        amount,
        created_at,
        descriptor,
        merchant,
        user,
    } = authorization;
    ActivityId::from_str(&authorization_id.into_string()).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the activity response contained an invalid authorization ID",
    })?;
    let user = map_user(user, operation)?;
    if user.user_id() != current_user_id {
        return Err(VenmoApiError::Contract {
            operation,
            problem: "the authorization activity belonged to a different account",
        });
    }
    let amount = Money::from_str(&amount.into_string()).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the activity response contained an invalid authorization amount",
    })?;
    let action =
        ActivityAction::from_str("authorization").map_err(|_| VenmoApiError::Contract {
            operation,
            problem: "the authorization activity action was invalid",
        })?;
    let status = ActivityStatus::from_str(&status).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the activity response contained an invalid authorization status",
    })?;
    let occurred_at = parse_required_timestamp(
        created_at.or(story.created_at).as_deref(),
        operation,
        "the authorization activity omitted or contained an invalid creation timestamp",
    )?;
    let note = bounded_optional_text(
        story.note.or(descriptor),
        operation,
        "the authorization activity contained an oversized note",
    )?;
    let audience = bounded_optional_label(
        story.audience,
        operation,
        "the authorization activity contained an invalid audience",
    )?;
    let merchant_name = bounded_required_text(
        merchant.display_name,
        operation,
        "the authorization activity contained an invalid merchant name",
    )?;
    Ok(Activity::new(
        story.id,
        occurred_at,
        action,
        ActivityDirection::Outgoing,
        ActivityCounterparty::external(merchant_name, "merchant".to_owned(), None),
        Some(amount),
        status,
        note,
        audience,
    ))
}

fn map_payment_activity(
    story: StoryMetadata,
    payment: ActivityPaymentRecordDto,
    current_user_id: &UserId,
    operation: &'static str,
) -> Result<Activity, VenmoApiError> {
    map_payment_activity_for_scope(story, payment, current_user_id, None, operation)
}

fn map_payment_activity_for_scope(
    story: StoryMetadata,
    payment: ActivityPaymentRecordDto,
    subject_user_id: &UserId,
    viewer_user_id: Option<&UserId>,
    operation: &'static str,
) -> Result<Activity, VenmoApiError> {
    let payment = map_payment_fields(story, payment, operation)?;
    if let Some(viewer_user_id) = viewer_user_id {
        validate_visible_other_user_payment(
            payment.audience.as_deref(),
            &payment.actor,
            &payment.target,
            viewer_user_id,
            operation,
        )?;
    }
    let amount = match viewer_user_id {
        Some(_) => None,
        None => Some(require_activity_amount(payment.amount, operation)?),
    };
    let (direction, counterparty) =
        relative_parties(payment.actor, payment.target, subject_user_id, operation)?;
    Ok(Activity::new(
        payment.id,
        payment.occurred_at,
        payment.action,
        direction,
        counterparty,
        amount,
        payment.status,
        payment.note,
        payment.audience,
    ))
}

fn map_payment_detail(
    story: StoryMetadata,
    payment: ActivityPaymentRecordDto,
    viewer_user_id: &UserId,
    operation: &'static str,
) -> Result<ActivityDetail, VenmoApiError> {
    let payment = map_payment_fields(story, payment, operation)?;
    if payment.actor.user_id() == payment.target.user_id() {
        return Err(VenmoApiError::Contract {
            operation,
            problem: "the payment activity did not identify two distinct parties",
        });
    }
    let viewer_participates =
        payment.actor.user_id() == viewer_user_id || payment.target.user_id() == viewer_user_id;
    let amount = if viewer_participates {
        Some(require_activity_amount(payment.amount, operation)?)
    } else {
        validate_visible_other_user_payment(
            payment.audience.as_deref(),
            &payment.actor,
            &payment.target,
            viewer_user_id,
            operation,
        )?;
        None
    };
    Ok(ActivityDetail::payment(
        payment.id,
        payment.occurred_at,
        payment.action,
        payment.actor,
        payment.target,
        amount,
        payment.status,
        payment.note,
        payment.audience,
    ))
}

struct MappedPayment {
    id: ActivityId,
    occurred_at: time::OffsetDateTime,
    action: ActivityAction,
    actor: User,
    target: User,
    amount: Option<Money>,
    status: ActivityStatus,
    note: Option<String>,
    audience: Option<String>,
}

fn map_payment_fields(
    story: StoryMetadata,
    payment: ActivityPaymentRecordDto,
    operation: &'static str,
) -> Result<MappedPayment, VenmoApiError> {
    let ActivityPaymentRecordDto {
        id: payment_id,
        status,
        action,
        amount,
        actor,
        target,
        note: payment_note,
        audience: payment_audience,
        date_created: payment_created,
    } = payment;
    let _ = payment_id.into_string();
    let actor = map_user(actor, operation)?;
    let target = map_user(target.user, operation)?;
    let amount = amount
        .map(|amount| {
            Money::from_str(&amount.into_string()).map_err(|_| VenmoApiError::Contract {
                operation,
                problem: "the activity response contained an invalid positive USD amount",
            })
        })
        .transpose()?;
    let action = ActivityAction::from_str(&action).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the activity response contained an invalid action",
    })?;
    let status = ActivityStatus::from_str(&status).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the activity response contained an invalid status",
    })?;
    let occurred_at = parse_required_timestamp(
        payment_created.or(story.created_at).as_deref(),
        operation,
        "the activity response omitted or contained an invalid creation timestamp",
    )?;
    let note = bounded_optional_text(
        payment_note.or(story.note),
        operation,
        "the activity response contained an oversized note",
    )?;
    let audience = bounded_optional_label(
        payment_audience.or(story.audience),
        operation,
        "the activity response contained an invalid audience",
    )?;
    Ok(MappedPayment {
        id: story.id,
        occurred_at,
        action,
        actor,
        target,
        amount,
        status,
        note,
        audience,
    })
}

fn validate_visible_other_user_payment(
    audience: Option<&str>,
    actor: &User,
    target: &User,
    viewer_user_id: &UserId,
    operation: &'static str,
) -> Result<(), VenmoApiError> {
    if actor.user_id() == target.user_id() {
        return Err(VenmoApiError::Contract {
            operation,
            problem: "the payment activity did not identify two distinct parties",
        });
    }
    match audience {
        Some("public" | "friends") => Ok(()),
        Some("private")
            if actor.user_id() == viewer_user_id || target.user_id() == viewer_user_id =>
        {
            Ok(())
        }
        Some("private") => Err(VenmoApiError::Contract {
            operation,
            problem: "a private activity did not include the authenticated viewer",
        }),
        Some(_) => Err(VenmoApiError::Contract {
            operation,
            problem: "another user's activity contained an unsupported audience",
        }),
        None => Err(VenmoApiError::Contract {
            operation,
            problem: "another user's activity omitted its audience",
        }),
    }
}

fn require_activity_amount(
    amount: Option<Money>,
    operation: &'static str,
) -> Result<Money, VenmoApiError> {
    amount.ok_or(VenmoApiError::Contract {
        operation,
        problem: "an activity involving the authenticated account omitted its amount",
    })
}

fn map_transfer_activity(
    story: StoryMetadata,
    transfer: TransferDto,
    operation: &'static str,
) -> Result<Activity, VenmoApiError> {
    let TransferDto {
        id: transfer_id,
        status,
        transfer_type,
        amount,
        date_requested,
        destination,
        source,
    } = transfer;
    if let Some(transfer_id) = transfer_id {
        ActivityId::from_str(&transfer_id.into_string()).map_err(|_| VenmoApiError::Contract {
            operation,
            problem: "the activity response contained an invalid transfer ID",
        })?;
    }
    let amount = Money::from_str(&amount.into_string()).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the activity response contained an invalid transfer amount",
    })?;
    let action_value = format!("transfer:{transfer_type}");
    let action = ActivityAction::from_str(&action_value).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the activity response contained an invalid transfer type",
    })?;
    let status = ActivityStatus::from_str(&status).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the activity response contained an invalid transfer status",
    })?;
    let occurred_at = parse_required_timestamp(
        date_requested.or(story.created_at).as_deref(),
        operation,
        "the transfer activity omitted or contained an invalid request timestamp",
    )?;
    let note = bounded_optional_text(
        story.note,
        operation,
        "the transfer activity contained an oversized note",
    )?;
    let audience = bounded_optional_label(
        story.audience,
        operation,
        "the transfer activity contained an invalid audience",
    )?;
    let (direction, endpoint) = match (source, destination) {
        (Some(source), None) => (ActivityDirection::Incoming, source),
        (None, Some(destination)) => (ActivityDirection::Outgoing, destination),
        (Some(_), Some(_)) | (None, None) => {
            return Err(VenmoApiError::Contract {
                operation,
                problem: "the transfer activity did not identify exactly one source or destination",
            });
        }
    };
    let name = bounded_required_text(
        endpoint.name,
        operation,
        "the transfer activity contained an invalid external-account name",
    )?;
    let kind = bounded_required_label(
        endpoint.endpoint_type,
        operation,
        "the transfer activity contained an invalid external-account type",
    )?;
    let last_four = endpoint
        .last_four
        .map(|value| {
            bounded_required_label(
                value.into_string(),
                operation,
                "the transfer activity contained an invalid destination suffix",
            )
        })
        .transpose()?;
    Ok(Activity::new(
        story.id,
        occurred_at,
        action,
        direction,
        ActivityCounterparty::external(name, kind, last_four),
        Some(amount),
        status,
        note,
        audience,
    ))
}

fn relative_parties(
    actor: User,
    target: User,
    current_user_id: &UserId,
    operation: &'static str,
) -> Result<(ActivityDirection, ActivityCounterparty), VenmoApiError> {
    let actor_is_self = actor.user_id() == current_user_id;
    let target_is_self = target.user_id() == current_user_id;
    match (actor_is_self, target_is_self) {
        (true, false) => Ok((
            ActivityDirection::Outgoing,
            ActivityCounterparty::user(target),
        )),
        (false, true) => Ok((
            ActivityDirection::Incoming,
            ActivityCounterparty::user(actor),
        )),
        (true, true) | (false, false) => Err(VenmoApiError::Contract {
            operation,
            problem: "the activity response did not identify exactly one active-account party",
        }),
    }
}

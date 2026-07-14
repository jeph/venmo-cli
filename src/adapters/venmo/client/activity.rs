use std::future::Future;
use std::str::FromStr;

use crate::features::activity::{
    Activity, ActivityAction, ActivityBeforeId, ActivityCounterparty, ActivityDetailApi,
    ActivityDirection, ActivityId, ActivityListApi, ActivityPage, ActivityPageRequest,
    ActivityStatus,
};
use crate::features::people::User;
use crate::shared::{AccessToken, DeviceId, Limit, Money, UserId};

use super::super::dto::{
    AuthorizationDto, PaymentRecordDto, StoriesEnvelope, StoryDto, StoryEnvelope, TransferDto,
};
use super::super::transport::{ApiSession, ApiTransport, HttpRequest};
use super::error::VenmoApiError;
use super::response::decode_success;
use super::support::{
    bounded_optional_label, bounded_optional_text, bounded_required_label, bounded_required_text,
    map_user, parse_required_timestamp, require_query_value, require_query_value_case_insensitive,
    validate_page_count, validate_query_keys,
};
use super::{ACTIVITY_DETAIL_OPERATION, ACTIVITY_LIST_OPERATION, VenmoApiClient};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ActivityMappingContext {
    List,
    Detail,
}

impl ActivityMappingContext {
    const fn operation(self) -> &'static str {
        match self {
            Self::List => ACTIVITY_LIST_OPERATION,
            Self::Detail => ACTIVITY_DETAIL_OPERATION,
        }
    }
}

impl<T: ApiTransport> VenmoApiClient<T> {
    pub(super) async fn fetch_activity_page(
        &self,
        access_token: &AccessToken,
        device_id: &DeviceId,
        current_user_id: &UserId,
        page: ActivityPageRequest,
    ) -> Result<ActivityPage, VenmoApiError> {
        let limit_value = page.page_size().get().to_string();
        let query = [("limit", limit_value.as_str()), ("social_only", "false")]
            .into_iter()
            .chain(page.before_id().map(|id| ("before_id", id.as_str())))
            .collect::<Vec<_>>();
        let path_segments = ["stories", "target-or-actor", current_user_id.as_str()];
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
            .map(|story| map_activity(story, current_user_id, ActivityMappingContext::List))
            .collect::<Result<Vec<_>, _>>()?;
        validate_page_count(ACTIVITY_LIST_OPERATION, activities.len(), page.page_size())?;
        let next_token = self.parse_activity_next_link(
            envelope.pagination.next.as_deref(),
            &path_segments,
            page.page_size(),
        )?;
        Ok(ActivityPage::new(activities, next_token))
    }

    pub(super) fn parse_activity_next_link(
        &self,
        raw: Option<&str>,
        path_segments: &[&str],
        page_size: Limit,
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
        require_query_value_case_insensitive(
            ACTIVITY_LIST_OPERATION,
            &query,
            "social_only",
            "false",
        )?;
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
    ) -> Result<Activity, VenmoApiError> {
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
        let activity = map_activity(
            envelope.data.into_story(),
            current_user_id,
            ActivityMappingContext::Detail,
        )?;
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
        current_user_id: &'a UserId,
        page: ActivityPageRequest,
    ) -> impl Future<Output = Result<ActivityPage, Self::Error>> + Send + 'a {
        self.fetch_activity_page(access_token, device_id, current_user_id, page)
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
    ) -> impl Future<Output = Result<Activity, Self::Error>> + Send + 'a {
        self.fetch_activity_by_id(access_token, device_id, current_user_id, activity_id)
    }
}

pub(super) fn map_activity(
    story: StoryDto,
    current_user_id: &UserId,
    context: ActivityMappingContext,
) -> Result<Activity, VenmoApiError> {
    let operation = context.operation();
    let StoryDto {
        id,
        date_created: story_created,
        note: story_note,
        audience: story_audience,
        payment,
        transfer,
        authorization,
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
        amount,
        status,
        note,
        audience,
    ))
}

fn map_payment_activity(
    story: StoryMetadata,
    payment: PaymentRecordDto,
    current_user_id: &UserId,
    operation: &'static str,
) -> Result<Activity, VenmoApiError> {
    let PaymentRecordDto {
        id: _,
        status,
        action,
        amount,
        actor,
        target,
        note: payment_note,
        audience: payment_audience,
        date_created: payment_created,
    } = payment;
    let actor = map_user(actor, operation)?;
    let target = map_user(target.user, operation)?;
    let (direction, counterparty) = relative_parties(actor, target, current_user_id, operation)?;
    let amount = Money::from_str(&amount.into_string()).map_err(|_| VenmoApiError::Contract {
        operation,
        problem: "the activity response contained an invalid positive USD amount",
    })?;
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
    Ok(Activity::new(
        story.id,
        occurred_at,
        action,
        direction,
        counterparty,
        amount,
        status,
        note,
        audience,
    ))
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
        amount,
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

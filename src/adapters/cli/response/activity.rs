use std::io;

use crate::features::activity::comment_list::ActivityCommentListResult;
use crate::features::activity::comment_remove::{
    ActivityCommentRemovalPlan, ActivityCommentRemovalResult,
};
use crate::features::activity::info::ActivityInfoResult;
use crate::features::activity::list::ActivityListResult;
use crate::features::activity::social::{
    ActivitySocialAction, ActivitySocialMutationResult, ActivitySocialPlan,
};
use crate::features::activity::{Activity, ActivitySubject};

use super::Response;
use super::shared;

pub(crate) fn activity_list(
    result: &ActivityListResult,
) -> io::Result<Response<'_, ActivityListResult>> {
    let activities = result
        .activities()
        .iter()
        .map(activity)
        .collect::<io::Result<Vec<_>>>()?;
    Ok(Response::new(
        result,
        serde_json::json!({
            "subject": result.subject().map(subject),
            "activities": activities,
            "next_before_id": result.next_before_id().map(|before| before.as_str()),
        }),
    ))
}

pub(crate) fn activity_info(
    result: &ActivityInfoResult,
) -> io::Result<Response<'_, ActivityInfoResult>> {
    Ok(Response::new(
        result,
        serde_json::json!({ "activity": shared::activity_detail(result.activity())? }),
    ))
}

pub(crate) fn activity_comment_list(
    result: &ActivityCommentListResult,
) -> io::Result<Response<'_, ActivityCommentListResult>> {
    let comments = result
        .comments()
        .iter()
        .map(shared::activity_comment)
        .collect::<io::Result<Vec<_>>>()?;
    Ok(Response::new(
        result,
        serde_json::json!({
            "activity_id": result.activity_id().as_str(),
            "comments": comments,
            "total_count": result.total_count(),
            "offset": result.offset().get(),
            "next_offset": result.next_offset().map(|offset| offset.get()),
        }),
    ))
}

pub(crate) fn activity_social_plan(
    plan: &ActivitySocialPlan,
) -> io::Result<Response<'_, ActivitySocialPlan>> {
    Ok(Response::new(plan, activity_social_plan_data(plan)?))
}

pub(crate) fn activity_social_result(
    result: &ActivitySocialMutationResult,
) -> io::Result<Response<'_, ActivitySocialMutationResult>> {
    let plan = activity_social_plan_data(result.plan())?;
    let result_data = serde_json::json!({
        "activity": shared::activity_detail(result.activity())?,
    });
    Ok(Response::new(
        result,
        super::mutation_data("completed", true, plan, Some(result_data)),
    ))
}

pub(crate) fn activity_comment_removal_plan(
    plan: &ActivityCommentRemovalPlan,
) -> Response<'_, ActivityCommentRemovalPlan> {
    Response::new(plan, activity_comment_removal_plan_data(plan))
}

pub(crate) fn activity_comment_removal_result(
    result: &ActivityCommentRemovalResult,
) -> Response<'_, ActivityCommentRemovalResult> {
    Response::new(
        result,
        super::mutation_data(
            "completed",
            true,
            activity_comment_removal_plan_data(result.plan()),
            Some(serde_json::json!({
                "accepted": true,
                "verification_required": true,
            })),
        ),
    )
}

fn activity(value: &Activity) -> io::Result<serde_json::Value> {
    Ok(serde_json::json!({
        "id": value.id().as_str(),
        "occurred_at": shared::timestamp(value.occurred_at())?,
        "action": value.action().as_str(),
        "direction": shared::activity_direction(value.direction()),
        "counterparty": shared::activity_counterparty(value.counterparty()),
        "amount": value.amount().map(shared::money),
        "status": value.status().map(|status| status.as_str()),
        "note": value.note(),
        "audience": value.audience(),
    }))
}

fn subject(value: &ActivitySubject) -> serde_json::Value {
    serde_json::json!({
        "user_id": value.user_id().as_str(),
        "username": value.username().as_str(),
        "kind": shared::activity_feed_kind(value.kind()),
    })
}

fn activity_social_plan_data(plan: &ActivitySocialPlan) -> io::Result<serde_json::Value> {
    let (action, message) = match plan.action() {
        ActivitySocialAction::Like => ("like", None),
        ActivitySocialAction::Unlike => ("unlike", None),
        ActivitySocialAction::AddComment(message) => ("add_comment", Some(message.as_str())),
    };
    Ok(serde_json::json!({
        "activity": shared::activity_detail(plan.activity())?,
        "action": action,
        "message": message,
        "previous_like_state": shared::like_state(plan.previous_like_state()),
        "automatic_retries": false,
    }))
}

fn activity_comment_removal_plan_data(plan: &ActivityCommentRemovalPlan) -> serde_json::Value {
    serde_json::json!({
        "comment_id": plan.comment_id().as_str(),
        "preflight_scope": "comment_id_only",
        "verification_required": true,
        "automatic_retries": false,
    })
}

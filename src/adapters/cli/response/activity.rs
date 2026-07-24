use std::io;

use crate::features::activity::comment_list::ActivityCommentListResult;
use crate::features::activity::comment_remove::{
    ActivityCommentRemovalPlan, ActivityCommentRemovalResult,
};
use crate::features::activity::info::ActivityInfoResult;
use crate::features::activity::list::ActivityListResult;
use crate::features::activity::reactions::{
    ActivityReactionAction, ActivityReactionListResult, ActivityReactionMutationResult,
    ActivityReactionPlan,
};
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

pub(crate) fn activity_reaction_list(
    result: &ActivityReactionListResult,
) -> Response<'_, ActivityReactionListResult> {
    let reactions = result
        .reactions()
        .items()
        .iter()
        .map(shared::activity_reaction)
        .collect::<Vec<_>>();
    Response::new(
        result,
        serde_json::json!({
            "activity_id": result.activity_id().as_str(),
            "total_count": result.reactions().total_count(),
            "reactions": reactions,
        }),
    )
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

pub(crate) fn activity_reaction_plan(
    plan: &ActivityReactionPlan,
) -> io::Result<Response<'_, ActivityReactionPlan>> {
    Ok(Response::new(plan, activity_reaction_plan_data(plan)?))
}

pub(crate) fn activity_reaction_result(
    result: &ActivityReactionMutationResult,
) -> io::Result<Response<'_, ActivityReactionMutationResult>> {
    let plan = activity_reaction_plan_data(result.plan())?;
    let reconciled_state = result.reconciled_state();
    let reconciled_reaction = result.reconciled_reaction();
    let reacted_by_current_user = reconciled_reaction
        .map(|reaction| reaction.reacted_by_current_user())
        .or_else(|| {
            (reconciled_state == crate::features::activity::ActivityReactionState::Absent)
                .then_some(false)
        });
    let result_data = serde_json::json!({
        "activity_id": result.activity().id().as_str(),
        "target": activity_reaction_target(result.plan().action().target()),
        "state": shared::reaction_state(reconciled_state),
        "count": reconciled_reaction.map(|reaction| reaction.count()),
        "reacted_by_current_user": reacted_by_current_user,
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
    let ActivitySocialAction::AddComment(message) = plan.action();
    Ok(serde_json::json!({
        "activity": shared::activity_detail(plan.activity())?,
        "action": "add_comment",
        "message": message.as_str(),
        "previous_like_state": shared::like_state(plan.previous_like_state()),
        "automatic_retries": false,
    }))
}

fn activity_reaction_plan_data(plan: &ActivityReactionPlan) -> io::Result<serde_json::Value> {
    let (action, target) = match plan.action() {
        ActivityReactionAction::Add(target) => ("add_reaction", target),
        ActivityReactionAction::Remove(target) => ("remove_reaction", target),
    };
    Ok(serde_json::json!({
        "activity": shared::activity_detail(plan.activity())?,
        "action": action,
        "target": activity_reaction_target(target),
        "previous_state": shared::reaction_state(plan.previous_state()),
        "automatic_retries": false,
    }))
}

fn activity_reaction_target(
    target: &crate::features::activity::ActivityReactionTarget,
) -> serde_json::Value {
    serde_json::json!({
        "kind": if target.is_like() { "like" } else { "unicode_emoji" },
        "value": target.as_str(),
    })
}

fn activity_comment_removal_plan_data(plan: &ActivityCommentRemovalPlan) -> serde_json::Value {
    serde_json::json!({
        "comment_id": plan.comment_id().as_str(),
        "preflight_scope": "comment_id_only",
        "verification_required": true,
        "automatic_retries": false,
    })
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::str::FromStr;

    use time::OffsetDateTime;

    use super::*;
    use crate::features::activity::{
        ActivityAction, ActivityDetail, ActivityId, ActivityReaction, ActivityReactionEmoji,
        ActivityReactionValue, ActivityReactions, ActivitySocial, ActivityStatus,
    };
    use crate::features::people::User;
    use crate::shared::{Money, UserId, Username};

    type TestResult<T = ()> = Result<T, Box<dyn Error>>;

    fn reactions() -> TestResult<ActivityReactions> {
        Ok(ActivityReactions::try_new(vec![
            ActivityReaction::new(ActivityReactionEmoji::from_str("🔥")?, 2, true),
            ActivityReaction::new(ActivityReactionEmoji::from_str("❤️")?, 1, false),
            ActivityReaction::from_value(
                ActivityReactionValue::custom_alias(":party_cup:".to_owned())?,
                4,
                false,
            ),
        ])?)
    }

    #[test]
    fn reaction_list_json_exposes_only_counts_and_current_user_state() -> TestResult {
        let result =
            ActivityReactionListResult::new(ActivityId::from_str("story-1")?, reactions()?);

        let response = activity_reaction_list(&result);

        assert_eq!(
            response.data(),
            &serde_json::json!({
                "activity_id":"story-1",
                "total_count":7,
                "reactions":[
                    {"emoji":"🔥","kind":"unicode_emoji","count":2,"reacted_by_current_user":true},
                    {"emoji":"❤️","kind":"unicode_emoji","count":1,"reacted_by_current_user":false},
                    {"emoji":":party_cup:","kind":"custom_alias","count":4,"reacted_by_current_user":false}
                ]
            })
        );
        Ok(())
    }

    #[test]
    fn activity_info_json_adds_only_the_aggregate_reaction_count() -> TestResult {
        let owner = User::new(
            UserId::from_str("123")?,
            Some(Username::from_bare("owner")?),
            None,
        );
        let other = User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("other")?),
            None,
        );
        let detail = ActivityDetail::payment(
            ActivityId::from_str("story-1")?,
            OffsetDateTime::UNIX_EPOCH,
            ActivityAction::from_str("pay")?,
            owner,
            other,
            Some(Money::from_str("1.00")?),
            Some(ActivityStatus::from_str("settled")?),
            Some("Synthetic note".to_owned()),
            Some("private".to_owned()),
        )
        .with_social(ActivitySocial::new(None, None).with_reactions(Some(reactions()?)));
        let result = ActivityInfoResult::new(detail);

        let response = activity_info(&result)?;
        let reaction_data = &response.data()["activity"]["social"]["reactions"];

        assert_eq!(reaction_data, &serde_json::json!({"count":7}));
        assert!(reaction_data.get("items").is_none());
        Ok(())
    }
}

use crate::features::people::friends::FriendsResult;
use crate::features::people::friendship::{
    FriendshipAction, FriendshipMutationResult, FriendshipPlan,
};
use crate::features::people::info::UserInfoResult;
use crate::features::people::users::UserSearchResult;

use super::Response;
use super::shared;

pub(crate) fn user_search(result: &UserSearchResult) -> Response<'_, UserSearchResult> {
    Response::new(
        result,
        serde_json::json!({
            "users": result.users().iter().map(shared::user).collect::<Vec<_>>(),
            "next_offset": result.next_offset().map(|offset| offset.get()),
        }),
    )
}

pub(crate) fn user_info(result: &UserInfoResult) -> Response<'_, UserInfoResult> {
    Response::new(
        result,
        serde_json::json!({ "user": shared::user(result.user()) }),
    )
}

pub(crate) fn friends(result: &FriendsResult) -> Response<'_, FriendsResult> {
    let subject = result.subject().map(|subject| {
        serde_json::json!({
            "user_id": subject.user_id().as_str(),
            "username": subject.username().as_str(),
        })
    });
    Response::new(
        result,
        serde_json::json!({
            "subject": subject,
            "users": result.users().iter().map(shared::user).collect::<Vec<_>>(),
            "next_offset": result.next_offset().map(|offset| offset.get()),
        }),
    )
}

pub(crate) fn friendship_plan(plan: &FriendshipPlan) -> Response<'_, FriendshipPlan> {
    Response::new(plan, friendship_plan_data(plan))
}

pub(crate) fn friendship_result(
    result: &FriendshipMutationResult,
) -> Response<'_, FriendshipMutationResult> {
    let plan = friendship_plan_data(result.plan());
    let result_data = serde_json::json!({
        "accepted": true,
        "action": friendship_action(result.plan().action()),
    });
    Response::new(
        result,
        super::mutation_data("completed", true, plan, Some(result_data)),
    )
}

fn friendship_plan_data(plan: &FriendshipPlan) -> serde_json::Value {
    serde_json::json!({
        "account": shared::account(plan.account()),
        "target": shared::user(plan.target()),
        "previous_status": shared::friendship_status(plan.previous_status()),
        "action": friendship_action(plan.action()),
        "automatic_retries": false,
    })
}

const fn friendship_action(action: FriendshipAction) -> &'static str {
    match action {
        FriendshipAction::SendRequest => "send_request",
        FriendshipAction::AcceptRequest => "accept_request",
        FriendshipAction::Unfriend => "unfriend",
        FriendshipAction::CancelRequest => "cancel_request",
    }
}

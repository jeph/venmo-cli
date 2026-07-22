use std::io;

use crate::features::transfers::model::TransferOutPlan;
use crate::features::transfers::options::TransferOptionsResult;
use crate::features::transfers::out::TransferOutResult;

use super::Response;
use super::shared;

pub(crate) fn transfer_options(
    result: &TransferOptionsResult,
) -> Response<'_, TransferOptionsResult> {
    let options = result.options();
    Response::new(
        result,
        serde_json::json!({
            "preferred_out": options.preferred_out().map(shared::transfer_speed),
            "standard": shared::transfer_mode(options.standard()),
            "instant": shared::transfer_mode(options.instant()),
        }),
    )
}

pub(crate) fn transfer_out_plan(plan: &TransferOutPlan) -> Response<'_, TransferOutPlan> {
    Response::new(plan, transfer_out_plan_data(plan))
}

pub(crate) fn transfer_out_result(
    result: &TransferOutResult,
) -> io::Result<Response<'_, TransferOutResult>> {
    let created = result.created();
    let result_data = serde_json::json!({
        "id": created.id().as_str(),
        "status": created.status(),
        "requested_at": shared::timestamp(created.requested_at())?,
        "net_amount": shared::money(created.net_amount()),
        "fee": shared::unsigned_usd(created.fee_cents()),
    });
    Ok(Response::new(
        result,
        super::mutation_data(
            "completed",
            true,
            transfer_out_plan_data(result.plan()),
            Some(result_data),
        ),
    ))
}

fn transfer_out_plan_data(plan: &TransferOutPlan) -> serde_json::Value {
    serde_json::json!({
        "account": shared::account(plan.account()),
        "balance": shared::balance(plan.balance()),
        "amount_selection": shared::transfer_amount_selection(plan.amount_selection()),
        "amount": shared::money(plan.amount()),
        "speed": shared::transfer_speed(plan.speed()),
        "destination": shared::transfer_instrument(plan.destination()),
        "automatic_retries": false,
    })
}

use std::io;

use crate::features::payments::PayPlan;
use crate::features::payments::pay::PayResult;
use crate::features::requests::accept::AcceptResult;
use crate::features::requests::cancel::CancelResult;
use crate::features::requests::create::RequestCreateResult;
use crate::features::requests::decline::DeclineResult;
use crate::features::requests::{
    AcceptRequestPlan, CancelRequestPlan, CreateRequestPlan, DeclineRequestPlan,
};

use super::Response;
use super::shared;

pub(crate) fn pay_plan(plan: &PayPlan) -> Response<'_, PayPlan> {
    Response::new(plan, pay_plan_data(plan))
}

pub(crate) fn pay_result(result: &PayResult) -> Response<'_, PayResult> {
    let created = result.created();
    let result_data = serde_json::json!({
        "id": created.id().as_str(),
        "status": shared::financial_status(created.status()),
        "purchase_protected": created.is_purchase_protected(),
    });
    Response::new(
        result,
        super::mutation_data(
            "completed",
            true,
            pay_plan_data(result.plan()),
            Some(result_data),
        ),
    )
}

pub(crate) fn request_create_plan(plan: &CreateRequestPlan) -> Response<'_, CreateRequestPlan> {
    Response::new(plan, request_create_plan_data(plan))
}

pub(crate) fn request_create_result(
    result: &RequestCreateResult,
) -> Response<'_, RequestCreateResult> {
    let result_data = serde_json::json!({
        "id": result.created().id().as_str(),
        "status": result.created().status().as_str(),
    });
    Response::new(
        result,
        super::mutation_data(
            "completed",
            true,
            request_create_plan_data(result.plan()),
            Some(result_data),
        ),
    )
}

pub(crate) fn accept_plan(plan: &AcceptRequestPlan) -> io::Result<Response<'_, AcceptRequestPlan>> {
    Ok(Response::new(plan, accept_plan_data(plan)?))
}

pub(crate) fn accept_result(result: &AcceptResult) -> io::Result<Response<'_, AcceptResult>> {
    let result_data = serde_json::json!({
        "request_id": result.plan().request().id().as_str(),
        "payment_id": result.accepted().payment_id().map(|id| id.as_str()),
        "status": result.accepted().status().map(shared::financial_status),
    });
    Ok(Response::new(
        result,
        super::mutation_data(
            "completed",
            true,
            accept_plan_data(result.plan())?,
            Some(result_data),
        ),
    ))
}

pub(crate) fn decline_plan(
    plan: &DeclineRequestPlan,
) -> io::Result<Response<'_, DeclineRequestPlan>> {
    Ok(Response::new(plan, decline_plan_data(plan)?))
}

pub(crate) fn decline_result(result: &DeclineResult) -> io::Result<Response<'_, DeclineResult>> {
    let result_data = serde_json::json!({
        "request_id": result.declined().request_id().as_str(),
        "status": result.declined().status().as_str(),
        "money_sent": false,
    });
    Ok(Response::new(
        result,
        super::mutation_data(
            "completed",
            true,
            decline_plan_data(result.plan())?,
            Some(result_data),
        ),
    ))
}

pub(crate) fn cancel_plan(plan: &CancelRequestPlan) -> io::Result<Response<'_, CancelRequestPlan>> {
    Ok(Response::new(plan, cancel_plan_data(plan)?))
}

pub(crate) fn cancel_result(result: &CancelResult) -> io::Result<Response<'_, CancelResult>> {
    let result_data = serde_json::json!({
        "request_id": result.cancelled().request_id().as_str(),
        "status": result.cancelled().status().as_str(),
        "money_sent": false,
    });
    Ok(Response::new(
        result,
        super::mutation_data(
            "completed",
            true,
            cancel_plan_data(result.plan())?,
            Some(result_data),
        ),
    ))
}

fn pay_plan_data(plan: &PayPlan) -> serde_json::Value {
    serde_json::json!({
        "client_request_id": plan.request_id().to_string(),
        "account": shared::account(plan.account()),
        "recipient": shared::user(plan.recipient()),
        "amount": shared::money(plan.amount()),
        "note": plan.note().as_str(),
        "visibility": plan.visibility().as_str(),
        "balance": shared::balance(plan.balance()),
        "funding_source": shared::funding_source(plan.funding_source()),
        "funding_source_selection": shared::funding_selection(plan.funding_source_selection()),
        "eligibility_fee": shared::unsigned_usd(plan.eligibility_fee_cents()),
        "purchase_protected": plan.is_purchase_protected(),
        "purchase_protection_fee": plan.purchase_protection_fee_cents().map(shared::unsigned_usd),
        "recipient_proceeds": plan.recipient_proceeds_cents().map(shared::unsigned_usd),
        "automatic_retries": false,
    })
}

fn request_create_plan_data(plan: &CreateRequestPlan) -> serde_json::Value {
    serde_json::json!({
        "client_request_id": plan.request_id().to_string(),
        "account": shared::account(plan.account()),
        "recipient": shared::user(plan.recipient()),
        "amount": shared::money(plan.amount()),
        "note": plan.note().as_str(),
        "visibility": plan.visibility().as_str(),
        "automatic_retries": false,
    })
}

fn accept_plan_data(plan: &AcceptRequestPlan) -> io::Result<serde_json::Value> {
    Ok(serde_json::json!({
        "account": shared::account(plan.account()),
        "request": shared::request(plan.request())?,
        "balance": shared::balance(plan.balance()),
        "funding_source": plan.funding_source().map(shared::funding_source),
        "funding_source_selection": plan
            .funding_source_selection()
            .map(shared::funding_selection),
        "purchase_protected": plan.is_purchase_protected(),
        "purchase_protection_fee": plan.approval_fee_cents().map(shared::unsigned_usd),
        "recipient_proceeds": plan.recipient_proceeds_cents().map(shared::unsigned_usd),
        "automatic_retries": false,
    }))
}

fn decline_plan_data(plan: &DeclineRequestPlan) -> io::Result<serde_json::Value> {
    Ok(serde_json::json!({
        "account": shared::account(plan.account()),
        "request": shared::request(plan.request())?,
        "action": "decline",
        "money_sent": false,
        "automatic_retries": false,
    }))
}

fn cancel_plan_data(plan: &CancelRequestPlan) -> io::Result<serde_json::Value> {
    Ok(serde_json::json!({
        "account": shared::account(plan.account()),
        "request": shared::request(plan.request())?,
        "action": "cancel",
        "money_sent": false,
        "automatic_retries": false,
    }))
}

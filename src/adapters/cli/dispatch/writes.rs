use std::future::Future;
use std::io::Write;

use crate::features::activity::comment_remove as activity_comment_remove;
use crate::features::activity::social::{self as activity_social, ActivitySocialIntent};
use crate::features::activity::{
    ActivityCommentId, ActivityCommentRemovalApi, ActivityDetailApi, ActivityId,
    ActivitySocialMutationApi,
};
use crate::features::auth::CurrentAccountApi;
use crate::features::payments::pay;
use crate::features::payments::{
    BlankSourceEligibilityApi, DefaultNoConfirmation, PaymentCreationApi, PaymentStepUpApi,
    PaymentStepUpInput, PeerFundingApi, ProtectedPaymentEligibilityApi,
};
use crate::features::people::friendship::{self, FriendshipIntent};
use crate::features::people::{FriendshipMutationApi, UserLookupApi, UserSearchApi};
use crate::features::requests::{
    RequestAcceptanceApi, RequestApprovalEligibilityApi, RequestApprovalNotificationApi,
    RequestCancellationApi, RequestCreationApi, RequestDeclineApi, RequestLookupApi,
};
use crate::features::requests::{accept, cancel, create as request_create, decline};
use crate::features::transfers::out as transfer_out;
use crate::features::transfers::{TransferOptionsApi, TransferOutCreationApi};
use crate::features::wallet::BalanceApi;
use crate::shared::{ApiFailure, ClientRequestIdGenerator, CredentialReader};

use super::super::args::{
    AcceptArgs, CancelArgs, DeclineArgs, FriendAddArgs, FriendRemoveArgs, PayUserArgs, RequestArgs,
    TransferOutArgs,
};
use super::super::{error::AppError, output, response};

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_pay_with<R, A, G, P, W, E, M, S>(
    args: PayUserArgs,
    store: &R,
    api: &A,
    generator: &G,
    prompt: &P,
    session: &mut output::OutputSession<'_, W, E>,
    make_interruption: M,
) -> Result<(), AppError>
where
    R: CredentialReader,
    A: CurrentAccountApi
        + UserLookupApi
        + UserSearchApi
        + BalanceApi
        + PeerFundingApi
        + BlankSourceEligibilityApi
        + ProtectedPaymentEligibilityApi
        + PaymentCreationApi
        + PaymentStepUpApi,
    <A as CurrentAccountApi>::Error: ApiFailure,
    G: ClientRequestIdGenerator,
    P: DefaultNoConfirmation + PaymentStepUpInput,
    W: Write,
    E: Write,
    M: FnOnce() -> Result<S, AppError>,
    S: Future<Output = Result<(), AppError>>,
{
    let options = pay::PayOptions::new(args.source.as_ref(), args.visibility.into(), args.protect);
    let prepared = pay::prepare(
        store,
        api,
        generator,
        &args.recipient,
        args.amount,
        args.note,
        options,
    )
    .await?;
    {
        let plan = response::pay_plan(prepared.plan());
        session.write_preflight(
            &plan,
            preflight_mode(args.yes, args.dry_run),
            |stderr, plan| output::write_pay_details(stderr, plan),
        )?;
        if args.dry_run {
            let dry_run = response::dry_run(plan.source(), plan.data().clone());
            return write_dry_run_complete(session, &dry_run);
        }
    }
    let authorized = pay::authorize(prompt, prepared, args.yes)?;
    let interruption = make_interruption()?;
    session.mark_write_started();
    let result =
        protect_with_interruption(pay::execute(api, prompt, authorized), interruption).await??;
    session.mark_completed();
    let response = response::pay_result(&result);
    session.write_success(
        &response,
        output::OutputClass::FinancialMutation,
        |stdout, _stderr, response| output::write_pay_result(stdout, response),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_request_create<R, A, G, P, W, E, M, S>(
    args: RequestArgs,
    store: &R,
    api: &A,
    generator: &G,
    prompt: &P,
    session: &mut output::OutputSession<'_, W, E>,
    make_interruption: M,
) -> Result<(), AppError>
where
    R: CredentialReader,
    A: CurrentAccountApi + UserLookupApi + UserSearchApi + RequestCreationApi + PaymentStepUpApi,
    <A as CurrentAccountApi>::Error: ApiFailure,
    G: ClientRequestIdGenerator,
    P: DefaultNoConfirmation + PaymentStepUpInput,
    W: Write,
    E: Write,
    M: FnOnce() -> Result<S, AppError>,
    S: Future<Output = Result<(), AppError>>,
{
    let prepared = request_create::prepare(
        store,
        api,
        generator,
        &args.recipient,
        args.amount,
        args.note,
        args.visibility.into(),
    )
    .await?;
    {
        let plan = response::request_create_plan(prepared.plan());
        session.write_preflight(
            &plan,
            preflight_mode(args.yes, args.dry_run),
            |stderr, plan| output::write_request_create_details(stderr, plan),
        )?;
        if args.dry_run {
            let dry_run = response::dry_run(plan.source(), plan.data().clone());
            return write_dry_run_complete(session, &dry_run);
        }
    }
    let authorized = request_create::authorize(prompt, prepared, args.yes)?;
    let interruption = make_interruption()?;
    session.mark_write_started();
    let result = protect_with_interruption(
        request_create::execute(api, prompt, authorized),
        interruption,
    )
    .await??;
    session.mark_completed();
    let response = response::request_create_result(&result);
    session.write_success(
        &response,
        output::OutputClass::FinancialMutation,
        |stdout, _stderr, response| output::write_request_create_result(stdout, response),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_accept_with<R, A, P, W, E, M, S>(
    args: AcceptArgs,
    store: &R,
    api: &A,
    prompt: &P,
    timestamps: &output::TimestampFormatter,
    session: &mut output::OutputSession<'_, W, E>,
    make_interruption: M,
) -> Result<(), AppError>
where
    R: CredentialReader,
    A: CurrentAccountApi
        + RequestLookupApi
        + UserLookupApi
        + BalanceApi
        + PeerFundingApi
        + RequestApprovalNotificationApi
        + RequestApprovalEligibilityApi
        + RequestAcceptanceApi
        + PaymentStepUpApi,
    <A as CurrentAccountApi>::Error: ApiFailure,
    P: DefaultNoConfirmation + PaymentStepUpInput,
    W: Write,
    E: Write,
    M: FnOnce() -> Result<S, AppError>,
    S: Future<Output = Result<(), AppError>>,
{
    let prepared = accept::prepare_with_protection(
        store,
        api,
        &args.request_id,
        args.source.as_ref(),
        args.protect,
    )
    .await?;
    {
        let plan = response::accept_plan(prepared.plan())?;
        session.write_preflight(
            &plan,
            preflight_mode(args.yes, args.dry_run),
            |stderr, plan| output::write_accept_details(stderr, plan, timestamps),
        )?;
        if args.dry_run {
            let dry_run = response::dry_run(plan.source(), plan.data().clone());
            return write_dry_run_complete(session, &dry_run);
        }
    }
    let authorized = accept::authorize(prompt, prepared, args.yes)?;
    let interruption = make_interruption()?;
    session.mark_write_started();
    let result =
        protect_with_interruption(accept::execute(api, prompt, authorized), interruption).await??;
    session.mark_completed();
    let response = response::accept_result(&result)
        .map_err(|source| AppError::FinancialResultOutput { source })?;
    session.write_success(
        &response,
        output::OutputClass::FinancialMutation,
        |stdout, _stderr, response| output::write_accept_result(stdout, response),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_decline_with<R, A, P, W, E, M, S>(
    args: DeclineArgs,
    store: &R,
    api: &A,
    prompt: &P,
    timestamps: &output::TimestampFormatter,
    session: &mut output::OutputSession<'_, W, E>,
    make_interruption: M,
) -> Result<(), AppError>
where
    R: CredentialReader,
    A: CurrentAccountApi + RequestLookupApi + RequestDeclineApi,
    <A as CurrentAccountApi>::Error: ApiFailure,
    P: DefaultNoConfirmation,
    W: Write,
    E: Write,
    M: FnOnce() -> Result<S, AppError>,
    S: Future<Output = Result<(), AppError>>,
{
    let prepared = decline::prepare(store, api, &args.request_id).await?;
    {
        let plan = response::decline_plan(prepared.plan())?;
        session.write_preflight(
            &plan,
            preflight_mode(args.yes, args.dry_run),
            |stderr, plan| output::write_decline_details(stderr, plan, timestamps),
        )?;
        if args.dry_run {
            let dry_run = response::dry_run(plan.source(), plan.data().clone());
            return write_dry_run_complete(session, &dry_run);
        }
    }
    let authorized = decline::authorize(prompt, prepared, args.yes)?;
    let interruption = make_interruption()?;
    session.mark_write_started();
    let result =
        protect_with_interruption(decline::execute(api, authorized), interruption).await??;
    session.mark_completed();
    let response = response::decline_result(&result)
        .map_err(|source| AppError::FinancialResultOutput { source })?;
    session.write_success(
        &response,
        output::OutputClass::FinancialMutation,
        |stdout, _stderr, response| output::write_decline_result(stdout, response),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_cancel_with<R, A, P, W, E, M, S>(
    args: CancelArgs,
    store: &R,
    api: &A,
    prompt: &P,
    timestamps: &output::TimestampFormatter,
    session: &mut output::OutputSession<'_, W, E>,
    make_interruption: M,
) -> Result<(), AppError>
where
    R: CredentialReader,
    A: CurrentAccountApi + RequestLookupApi + RequestCancellationApi,
    <A as CurrentAccountApi>::Error: ApiFailure,
    P: DefaultNoConfirmation,
    W: Write,
    E: Write,
    M: FnOnce() -> Result<S, AppError>,
    S: Future<Output = Result<(), AppError>>,
{
    let prepared = cancel::prepare(store, api, &args.request_id).await?;
    {
        let plan = response::cancel_plan(prepared.plan())?;
        session.write_preflight(
            &plan,
            preflight_mode(args.yes, args.dry_run),
            |stderr, plan| output::write_cancel_details(stderr, plan, timestamps),
        )?;
        if args.dry_run {
            let dry_run = response::dry_run(plan.source(), plan.data().clone());
            return write_dry_run_complete(session, &dry_run);
        }
    }
    let authorized = cancel::authorize(prompt, prepared, args.yes)?;
    let interruption = make_interruption()?;
    session.mark_write_started();
    let result =
        protect_with_interruption(cancel::execute(api, authorized), interruption).await??;
    session.mark_completed();
    let response = response::cancel_result(&result)
        .map_err(|source| AppError::FinancialResultOutput { source })?;
    session.write_success(
        &response,
        output::OutputClass::FinancialMutation,
        |stdout, _stderr, response| output::write_cancel_result(stdout, response),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_transfer_out_with<R, A, P, W, E, M, S>(
    args: TransferOutArgs,
    store: &R,
    api: &A,
    prompt: &P,
    timestamps: &output::TimestampFormatter,
    session: &mut output::OutputSession<'_, W, E>,
    make_interruption: M,
) -> Result<(), AppError>
where
    R: CredentialReader,
    A: CurrentAccountApi + BalanceApi + TransferOptionsApi + TransferOutCreationApi,
    <A as CurrentAccountApi>::Error: ApiFailure,
    P: DefaultNoConfirmation,
    W: Write,
    E: Write,
    M: FnOnce() -> Result<S, AppError>,
    S: Future<Output = Result<(), AppError>>,
{
    let prepared = transfer_out::prepare(store, api, args.amount.into(), args.speed.into()).await?;
    {
        let plan = response::transfer_out_plan(prepared.plan());
        session.write_preflight(
            &plan,
            preflight_mode(args.yes, args.dry_run),
            |stderr, plan| output::write_transfer_out_details(stderr, plan),
        )?;
        if args.dry_run {
            let dry_run = response::dry_run(plan.source(), plan.data().clone());
            return write_dry_run_complete(session, &dry_run);
        }
    }
    let authorized = transfer_out::authorize(prompt, prepared, args.yes)?;
    let interruption = make_interruption()?;
    session.mark_write_started();
    let result =
        protect_with_interruption(transfer_out::execute(api, authorized), interruption).await??;
    session.mark_completed();
    let response = response::transfer_out_result(&result)
        .map_err(|source| AppError::FinancialResultOutput { source })?;
    session.write_success(
        &response,
        output::OutputClass::FinancialMutation,
        |stdout, _stderr, response| output::write_transfer_out_result(stdout, response, timestamps),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_friend_add_with<R, A, P, W, E, M, S>(
    args: FriendAddArgs,
    store: &R,
    api: &A,
    prompt: &P,
    session: &mut output::OutputSession<'_, W, E>,
    make_interruption: M,
) -> Result<(), AppError>
where
    R: CredentialReader,
    A: CurrentAccountApi + UserLookupApi + UserSearchApi + FriendshipMutationApi,
    <A as CurrentAccountApi>::Error: ApiFailure,
    P: DefaultNoConfirmation,
    W: Write,
    E: Write,
    M: FnOnce() -> Result<S, AppError>,
    S: Future<Output = Result<(), AppError>>,
{
    run_friendship_mutation_with(
        &args.username,
        args.yes,
        args.dry_run,
        FriendshipIntent::Add,
        store,
        api,
        prompt,
        session,
        make_interruption,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_activity_social_with<R, A, P, W, E, M, S>(
    activity_id: &ActivityId,
    intent: ActivitySocialIntent,
    assume_yes: bool,
    dry_run: bool,
    store: &R,
    api: &A,
    prompt: &P,
    timestamps: &output::TimestampFormatter,
    session: &mut output::OutputSession<'_, W, E>,
    make_interruption: M,
) -> Result<(), AppError>
where
    R: CredentialReader,
    A: ActivityDetailApi + ActivitySocialMutationApi,
    P: DefaultNoConfirmation,
    W: Write,
    E: Write,
    M: FnOnce() -> Result<S, AppError>,
    S: Future<Output = Result<(), AppError>>,
{
    let prepared = activity_social::prepare(store, api, activity_id, intent).await?;
    {
        let plan = response::activity_social_plan(prepared.plan())?;
        session.write_preflight(
            &plan,
            preflight_mode(assume_yes, dry_run),
            |stderr, plan| output::write_activity_social_details(stderr, plan, timestamps),
        )?;
        if dry_run {
            let dry_run = response::dry_run(plan.source(), plan.data().clone());
            return write_dry_run_complete(session, &dry_run);
        }
    }
    let authorized = activity_social::authorize(prompt, prepared, assume_yes)?;
    let interruption = make_interruption()?;
    session.mark_write_started();
    let result =
        protect_state_with_interruption(activity_social::execute(api, authorized), interruption)
            .await??;
    session.mark_completed();
    let response = response::activity_social_result(&result)
        .map_err(|source| AppError::StateMutationResultOutput { source })?;
    session.write_success(
        &response,
        output::OutputClass::StateMutation,
        |stdout, _stderr, response| output::write_activity_social_result(stdout, response),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_activity_comment_remove_with<R, A, P, W, E, M, S>(
    comment_id: ActivityCommentId,
    assume_yes: bool,
    dry_run: bool,
    store: &R,
    api: &A,
    prompt: &P,
    session: &mut output::OutputSession<'_, W, E>,
    make_interruption: M,
) -> Result<(), AppError>
where
    R: CredentialReader,
    A: ActivityCommentRemovalApi,
    P: DefaultNoConfirmation,
    W: Write,
    E: Write,
    M: FnOnce() -> Result<S, AppError>,
    S: Future<Output = Result<(), AppError>>,
{
    let prepared = activity_comment_remove::prepare(store, comment_id)?;
    {
        let plan = response::activity_comment_removal_plan(prepared.plan());
        session.write_preflight(
            &plan,
            preflight_mode(assume_yes, dry_run),
            |stderr, plan| output::write_activity_comment_removal_details(stderr, plan),
        )?;
        if dry_run {
            let dry_run = response::dry_run(plan.source(), plan.data().clone());
            return write_dry_run_complete(session, &dry_run);
        }
    }
    let authorized = activity_comment_remove::authorize(prompt, prepared, assume_yes)?;
    let interruption = make_interruption()?;
    session.mark_write_started();
    let result = protect_state_with_interruption(
        activity_comment_remove::execute(api, authorized),
        interruption,
    )
    .await??;
    session.mark_completed();
    let response = response::activity_comment_removal_result(&result);
    session.write_success(
        &response,
        output::OutputClass::StateMutation,
        |stdout, _stderr, response| output::write_activity_comment_removal_result(stdout, response),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_friend_remove_with<R, A, P, W, E, M, S>(
    args: FriendRemoveArgs,
    store: &R,
    api: &A,
    prompt: &P,
    session: &mut output::OutputSession<'_, W, E>,
    make_interruption: M,
) -> Result<(), AppError>
where
    R: CredentialReader,
    A: CurrentAccountApi + UserLookupApi + UserSearchApi + FriendshipMutationApi,
    <A as CurrentAccountApi>::Error: ApiFailure,
    P: DefaultNoConfirmation,
    W: Write,
    E: Write,
    M: FnOnce() -> Result<S, AppError>,
    S: Future<Output = Result<(), AppError>>,
{
    run_friendship_mutation_with(
        &args.username,
        args.yes,
        args.dry_run,
        FriendshipIntent::Remove,
        store,
        api,
        prompt,
        session,
        make_interruption,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn run_friendship_mutation_with<R, A, P, W, E, M, S>(
    username: &crate::shared::Username,
    assume_yes: bool,
    dry_run: bool,
    intent: FriendshipIntent,
    store: &R,
    api: &A,
    prompt: &P,
    session: &mut output::OutputSession<'_, W, E>,
    make_interruption: M,
) -> Result<(), AppError>
where
    R: CredentialReader,
    A: CurrentAccountApi + UserLookupApi + UserSearchApi + FriendshipMutationApi,
    <A as CurrentAccountApi>::Error: ApiFailure,
    P: DefaultNoConfirmation,
    W: Write,
    E: Write,
    M: FnOnce() -> Result<S, AppError>,
    S: Future<Output = Result<(), AppError>>,
{
    let prepared = friendship::prepare(store, api, username, intent).await?;
    {
        let plan = response::friendship_plan(prepared.plan());
        session.write_preflight(
            &plan,
            preflight_mode(assume_yes, dry_run),
            |stderr, plan| output::write_friendship_details(stderr, plan),
        )?;
        if dry_run {
            let dry_run = response::dry_run(plan.source(), plan.data().clone());
            return write_dry_run_complete(session, &dry_run);
        }
    }
    let authorized = friendship::authorize(prompt, prepared, assume_yes)?;
    let interruption = make_interruption()?;
    session.mark_write_started();
    let result =
        protect_state_with_interruption(friendship::execute(api, authorized), interruption)
            .await??;
    session.mark_completed();
    let response = response::friendship_result(&result);
    session.write_success(
        &response,
        output::OutputClass::StateMutation,
        |stdout, _stderr, response| output::write_friendship_result(stdout, response),
    )
}

fn write_dry_run_complete<W: Write, E: Write, T: ?Sized>(
    session: &mut output::OutputSession<'_, W, E>,
    response: &response::Response<'_, T>,
) -> Result<(), AppError> {
    session.write_success(
        response,
        output::OutputClass::DryRun,
        |stdout, _stderr, response| output::write_dry_run_complete(stdout, response),
    )
}

const fn preflight_mode(assume_yes: bool, dry_run: bool) -> output::PreflightMode {
    if dry_run {
        output::PreflightMode::DryRun
    } else if assume_yes {
        output::PreflightMode::AssumeYes
    } else {
        output::PreflightMode::Prompt
    }
}

#[cfg(unix)]
pub(super) fn production_financial_interruption()
-> Result<impl Future<Output = Result<(), AppError>>, AppError> {
    let mut interrupts = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .map_err(|source| AppError::SignalInitialization { source })?;
    let mut terminations =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .map_err(|source| AppError::SignalInitialization { source })?;
    let mut hangups = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
        .map_err(|source| AppError::SignalInitialization { source })?;
    Ok(async move {
        tokio::select! {
            _ = interrupts.recv() => {}
            _ = terminations.recv() => {}
            _ = hangups.recv() => {}
        }
        Ok(())
    })
}

#[cfg(unix)]
pub(super) fn production_state_interruption()
-> Result<impl Future<Output = Result<(), AppError>>, AppError> {
    let mut interrupts = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .map_err(|source| AppError::StateSignalInitialization { source })?;
    let mut terminations =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .map_err(|source| AppError::StateSignalInitialization { source })?;
    let mut hangups = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
        .map_err(|source| AppError::StateSignalInitialization { source })?;
    Ok(async move {
        tokio::select! {
            _ = interrupts.recv() => {}
            _ = terminations.recv() => {}
            _ = hangups.recv() => {}
        }
        Ok(())
    })
}

#[cfg(not(unix))]
pub(super) fn production_financial_interruption()
-> Result<impl Future<Output = Result<(), AppError>>, AppError> {
    Ok(async {
        tokio::signal::ctrl_c()
            .await
            .map_err(|source| AppError::SignalInitialization { source })
    })
}

#[cfg(not(unix))]
pub(super) fn production_state_interruption()
-> Result<impl Future<Output = Result<(), AppError>>, AppError> {
    Ok(async {
        tokio::signal::ctrl_c()
            .await
            .map_err(|source| AppError::StateSignalInitialization { source })
    })
}

async fn protect_with_interruption<F, S>(future: F, interruption: S) -> Result<F::Output, AppError>
where
    F: Future,
    S: Future<Output = Result<(), AppError>>,
{
    let mut future = std::pin::pin!(future);
    let mut interruption = std::pin::pin!(interruption);
    tokio::select! {
        // A signal that is ready in the same poll wins conservatively: transmission may have begun.
        biased;
        interrupted = &mut interruption => {
            interrupted?;
            Err(AppError::FinancialWriteInterruptedUnknown)
        },
        outcome = &mut future => Ok(outcome),
    }
}

async fn protect_state_with_interruption<F, S>(
    future: F,
    interruption: S,
) -> Result<F::Output, AppError>
where
    F: Future,
    S: Future<Output = Result<(), AppError>>,
{
    let mut future = std::pin::pin!(future);
    let mut interruption = std::pin::pin!(interruption);
    tokio::select! {
        biased;
        interrupted = &mut interruption => {
            interrupted?;
            Err(AppError::StateWriteInterruptedUnknown)
        },
        outcome = &mut future => Ok(outcome),
    }
}

#[cfg(test)]
fn human_output<'a, W: Write, E: Write>(
    command: super::super::CommandId,
    stdout: &'a mut W,
    stderr: &'a mut E,
) -> output::OutputSession<'a, W, E> {
    output::OutputSession::new(
        super::super::OutputFormat::Human,
        command,
        false,
        stdout,
        stderr,
    )
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "writes_pay_tests.rs"]
mod pay_handler_tests;

#[cfg(test)]
#[path = "writes_accept_tests.rs"]
mod accept_handler_tests;

#[cfg(test)]
#[path = "writes_decline_tests.rs"]
mod decline_handler_tests;

#[cfg(test)]
#[path = "writes_cancel_tests.rs"]
mod cancel_handler_tests;

#[cfg(test)]
#[path = "writes_transfer_friend_tests.rs"]
mod transfer_friend_handler_tests;

#[cfg(test)]
#[path = "writes_activity_tests.rs"]
mod activity_handler_tests;

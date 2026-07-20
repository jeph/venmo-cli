use std::future::Future;
use std::io::Write;

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
use super::super::{error::AppError, output};
use super::write_and_flush;

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_pay_with<R, A, G, P, W, E, M, S>(
    args: PayUserArgs,
    store: &R,
    api: &A,
    generator: &G,
    prompt: &P,
    stdout: &mut W,
    stderr: &mut E,
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
    write_and_flush(stderr, &prepared, output::write_pay_details)?;
    let authorized = pay::authorize(prompt, prepared, args.yes)?;
    let interruption = make_interruption()?;
    let result =
        protect_with_interruption(pay::execute(api, prompt, authorized), interruption).await??;
    write_and_flush(stdout, &result, output::write_pay_result)
        .map_err(|source| AppError::FinancialResultOutput { source })?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_request_create<R, A, G, P, W, E, M, S>(
    args: RequestArgs,
    store: &R,
    api: &A,
    generator: &G,
    prompt: &P,
    stdout: &mut W,
    stderr: &mut E,
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
    write_and_flush(stderr, &prepared, output::write_request_create_details)?;
    let authorized = request_create::authorize(prompt, prepared, args.yes)?;
    let interruption = make_interruption()?;
    let result = protect_with_interruption(
        request_create::execute(api, prompt, authorized),
        interruption,
    )
    .await??;
    write_and_flush(stdout, &result, output::write_request_create_result)
        .map_err(|source| AppError::FinancialResultOutput { source })?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_accept_with<R, A, P, W, E, M, S>(
    args: AcceptArgs,
    store: &R,
    api: &A,
    prompt: &P,
    timestamps: &output::TimestampFormatter,
    stdout: &mut W,
    stderr: &mut E,
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
    write_and_flush(stderr, &prepared, |writer, prepared| {
        output::write_accept_details(writer, prepared, timestamps)
    })?;
    let authorized = accept::authorize(prompt, prepared, args.yes)?;
    let interruption = make_interruption()?;
    let result =
        protect_with_interruption(accept::execute(api, prompt, authorized), interruption).await??;
    write_and_flush(stdout, &result, output::write_accept_result)
        .map_err(|source| AppError::FinancialResultOutput { source })?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_decline_with<R, A, P, W, E, M, S>(
    args: DeclineArgs,
    store: &R,
    api: &A,
    prompt: &P,
    timestamps: &output::TimestampFormatter,
    stdout: &mut W,
    stderr: &mut E,
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
    write_and_flush(stderr, &prepared, |writer, prepared| {
        output::write_decline_details(writer, prepared, timestamps)
    })?;
    let authorized = decline::authorize(prompt, prepared, args.yes)?;
    let interruption = make_interruption()?;
    let result =
        protect_with_interruption(decline::execute(api, authorized), interruption).await??;
    write_and_flush(stdout, &result, output::write_decline_result)
        .map_err(|source| AppError::FinancialResultOutput { source })?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_cancel_with<R, A, P, W, E, M, S>(
    args: CancelArgs,
    store: &R,
    api: &A,
    prompt: &P,
    timestamps: &output::TimestampFormatter,
    stdout: &mut W,
    stderr: &mut E,
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
    write_and_flush(stderr, &prepared, |writer, prepared| {
        output::write_cancel_details(writer, prepared, timestamps)
    })?;
    let authorized = cancel::authorize(prompt, prepared, args.yes)?;
    let interruption = make_interruption()?;
    let result =
        protect_with_interruption(cancel::execute(api, authorized), interruption).await??;
    write_and_flush(stdout, &result, output::write_cancel_result)
        .map_err(|source| AppError::FinancialResultOutput { source })?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_transfer_out_with<R, A, P, W, E, M, S>(
    args: TransferOutArgs,
    store: &R,
    api: &A,
    prompt: &P,
    timestamps: &output::TimestampFormatter,
    stdout: &mut W,
    stderr: &mut E,
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
    write_and_flush(stderr, &prepared, output::write_transfer_out_details)?;
    let authorized = transfer_out::authorize(prompt, prepared, args.yes)?;
    let interruption = make_interruption()?;
    let result =
        protect_with_interruption(transfer_out::execute(api, authorized), interruption).await??;
    write_and_flush(stdout, &result, |writer, result| {
        output::write_transfer_out_result(writer, result, timestamps)
    })
    .map_err(|source| AppError::FinancialResultOutput { source })?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_friend_add_with<R, A, P, W, E, M, S>(
    args: FriendAddArgs,
    store: &R,
    api: &A,
    prompt: &P,
    stdout: &mut W,
    stderr: &mut E,
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
        FriendshipIntent::Add,
        store,
        api,
        prompt,
        stdout,
        stderr,
        make_interruption,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_friend_remove_with<R, A, P, W, E, M, S>(
    args: FriendRemoveArgs,
    store: &R,
    api: &A,
    prompt: &P,
    stdout: &mut W,
    stderr: &mut E,
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
        FriendshipIntent::Remove,
        store,
        api,
        prompt,
        stdout,
        stderr,
        make_interruption,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn run_friendship_mutation_with<R, A, P, W, E, M, S>(
    username: &crate::shared::Username,
    assume_yes: bool,
    intent: FriendshipIntent,
    store: &R,
    api: &A,
    prompt: &P,
    stdout: &mut W,
    stderr: &mut E,
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
    write_and_flush(stderr, &prepared, output::write_friendship_details)?;
    let authorized = friendship::authorize(prompt, prepared, assume_yes)?;
    let interruption = make_interruption()?;
    let result =
        protect_state_with_interruption(friendship::execute(api, authorized), interruption)
            .await??;
    write_and_flush(stdout, &result, output::write_friendship_result)
        .map_err(|source| AppError::StateMutationResultOutput { source })?;
    Ok(())
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

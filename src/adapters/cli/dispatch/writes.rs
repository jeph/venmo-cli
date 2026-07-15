use std::future::Future;
use std::io::Write;

use crate::features::auth::CurrentAccountApi;
use crate::features::payments::pay;
use crate::features::payments::{
    BlankSourceEligibilityApi, DefaultNoConfirmation, FundingChoiceSelection, PaymentCreationApi,
    PeerFundingApi,
};
use crate::features::people::{UserLookupApi, UserSearchApi};
use crate::features::requests::{
    RequestAcceptanceApi, RequestCreationApi, RequestDeclineApi, RequestLookupApi,
};
use crate::features::requests::{accept, create as request_create, decline};
use crate::features::wallet::BalanceApi;
use crate::shared::{ApiFailure, ClientRequestIdGenerator, CredentialReader};

use super::super::args::{AcceptArgs, DeclineArgs, PayArgs, RequestArgs};
use super::super::{error::AppError, output};
use super::write_and_flush;

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_pay_with<R, A, G, P, W, E, M, S>(
    args: PayArgs,
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
        + PaymentCreationApi,
    <A as CurrentAccountApi>::Error: ApiFailure,
    G: ClientRequestIdGenerator,
    P: FundingChoiceSelection + DefaultNoConfirmation,
    W: Write,
    E: Write,
    M: FnOnce() -> Result<S, AppError>,
    S: Future<Output = Result<(), AppError>>,
{
    let prepared = pay::prepare(
        store,
        api,
        generator,
        prompt,
        &args.recipient,
        args.amount,
        args.note,
        args.visibility.into(),
        args.from.as_ref(),
    )
    .await?;
    write_and_flush(stderr, &prepared, output::write_pay_preflight)?;
    let authorized = pay::authorize(prompt, prepared, args.yes)?;
    let interruption = make_interruption()?;
    let result = protect_with_interruption(pay::execute(api, authorized), interruption).await??;
    write_and_flush(stdout, &result, output::write_pay_result)
        .map_err(|source| AppError::FinancialResultOutput { source })?;
    Ok(())
}

pub(super) async fn run_request_create<R, A, G, W, M, S>(
    args: RequestArgs,
    store: &R,
    api: &A,
    generator: &G,
    stdout: &mut W,
    make_interruption: M,
) -> Result<(), AppError>
where
    R: CredentialReader,
    A: CurrentAccountApi + UserLookupApi + UserSearchApi + RequestCreationApi,
    <A as CurrentAccountApi>::Error: ApiFailure,
    G: ClientRequestIdGenerator,
    W: Write,
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
    let interruption = make_interruption()?;
    let result =
        protect_with_interruption(request_create::execute(api, prepared), interruption).await??;
    write_and_flush(stdout, &result, output::write_request_create_result)
        .map_err(|source| AppError::FinancialResultOutput { source })?;
    Ok(())
}

pub(super) async fn run_accept_with<R, A, P, W, E, M, S>(
    args: AcceptArgs,
    store: &R,
    api: &A,
    prompt: &P,
    stdout: &mut W,
    stderr: &mut E,
    make_interruption: M,
) -> Result<(), AppError>
where
    R: CredentialReader,
    A: CurrentAccountApi + RequestLookupApi + UserLookupApi + BalanceApi + RequestAcceptanceApi,
    <A as CurrentAccountApi>::Error: ApiFailure,
    P: DefaultNoConfirmation,
    W: Write,
    E: Write,
    M: FnOnce() -> Result<S, AppError>,
    S: Future<Output = Result<(), AppError>>,
{
    let prepared = accept::prepare(store, api, &args.request_id).await?;
    write_and_flush(stderr, &prepared, output::write_accept_preflight)?;
    let authorized = accept::authorize(prompt, prepared, args.yes)?;
    let interruption = make_interruption()?;
    let result =
        protect_with_interruption(accept::execute(api, authorized), interruption).await??;
    write_and_flush(stdout, &result, output::write_accept_result)
        .map_err(|source| AppError::FinancialResultOutput { source })?;
    Ok(())
}

pub(super) async fn run_decline_with<R, A, W, E, M, S>(
    args: DeclineArgs,
    store: &R,
    api: &A,
    stdout: &mut W,
    stderr: &mut E,
    make_interruption: M,
) -> Result<(), AppError>
where
    R: CredentialReader,
    A: CurrentAccountApi + RequestLookupApi + RequestDeclineApi,
    <A as CurrentAccountApi>::Error: ApiFailure,
    W: Write,
    E: Write,
    M: FnOnce() -> Result<S, AppError>,
    S: Future<Output = Result<(), AppError>>,
{
    let prepared = decline::prepare(store, api, &args.request_id).await?;
    write_and_flush(stderr, &prepared, output::write_decline_preflight)?;
    let interruption = make_interruption()?;
    let result = protect_with_interruption(decline::execute(api, prepared), interruption).await??;
    write_and_flush(stdout, &result, output::write_decline_result)
        .map_err(|source| AppError::FinancialResultOutput { source })?;
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

#[cfg(not(unix))]
pub(super) fn production_financial_interruption()
-> Result<impl Future<Output = Result<(), AppError>>, AppError> {
    Ok(async {
        tokio::signal::ctrl_c()
            .await
            .map_err(|source| AppError::SignalInitialization { source })
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

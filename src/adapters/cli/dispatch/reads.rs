use std::io::Write;

use crate::features::activity::{self as activity, ActivityDetailApi, ActivityListApi};
use crate::features::people::{
    self as people, FriendsApi, UserLookupApi, UserSearchApi, friends, users,
};
use crate::features::requests::{
    self as requests, RequestLookupApi, RequestsApi, list as request_list,
};
use crate::features::wallet::{BalanceApi, PaymentMethodsApi, balance, payment_methods};
use crate::shared::CredentialReader;

use super::super::args::{
    ActivityArgs, ActivityOperation, FriendsArgs, FriendsOperation, PaymentMethodsArgs,
    PaymentMethodsOperation, RequestInfoArgs, RequestsListArgs, UserInfoArgs, UserSearchArgs,
};
use super::super::{error::AppError, output};

pub(super) async fn run_friends<R, A, W, E>(
    args: FriendsArgs,
    store: &R,
    api: &A,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<(), AppError>
where
    R: CredentialReader,
    A: FriendsApi,
    W: Write,
    E: Write,
{
    match args.operation {
        FriendsOperation::List(args) => {
            let result = friends::list(store, api, args.limit, args.offset).await?;
            output::write_friends(stdout, stderr, &result)?;
        }
    }
    Ok(())
}

pub(super) async fn run_balance<R, A, W>(store: &R, api: &A, stdout: &mut W) -> Result<(), AppError>
where
    R: CredentialReader,
    A: BalanceApi,
    W: Write,
{
    let result = balance::get(store, api).await?;
    output::write_balance(stdout, &result)?;
    Ok(())
}

pub(super) async fn run_activity<R, A, W, E>(
    args: ActivityArgs,
    store: &R,
    api: &A,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<(), AppError>
where
    R: CredentialReader,
    A: ActivityListApi + ActivityDetailApi,
    W: Write,
    E: Write,
{
    match args.operation {
        ActivityOperation::List(args) => {
            let result = activity::list(store, api, args.limit, args.before_id.as_ref()).await?;
            output::write_activity_list(stdout, stderr, &result)?;
        }
        ActivityOperation::Info(args) => {
            let result = activity::info(store, api, &args.activity_id).await?;
            output::write_activity_info(stdout, &result)?;
        }
    }
    Ok(())
}

pub(super) async fn run_requests_list<R, A, W, E>(
    args: RequestsListArgs,
    store: &R,
    api: &A,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<(), AppError>
where
    R: CredentialReader,
    A: RequestsApi,
    W: Write,
    E: Write,
{
    let result = request_list::list(
        store,
        api,
        args.direction.into(),
        args.limit,
        args.before.as_ref(),
    )
    .await?;
    output::write_requests(stdout, stderr, &result)?;
    Ok(())
}

pub(super) async fn run_request_info<R, A, W>(
    args: RequestInfoArgs,
    store: &R,
    api: &A,
    stdout: &mut W,
) -> Result<(), AppError>
where
    R: CredentialReader,
    A: RequestLookupApi,
    W: Write,
{
    let result = requests::info(store, api, &args.request_id).await?;
    output::write_request_info(stdout, &result)?;
    Ok(())
}

pub(super) async fn run_user_search<R, A, W, E>(
    args: UserSearchArgs,
    store: &R,
    api: &A,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<(), AppError>
where
    R: CredentialReader,
    A: UserSearchApi,
    W: Write,
    E: Write,
{
    let result = users::search(store, api, &args.query, args.limit, args.offset).await?;
    output::write_user_search(stdout, stderr, &result)?;
    Ok(())
}

pub(super) async fn run_user_info<R, A, W>(
    args: UserInfoArgs,
    store: &R,
    api: &A,
    stdout: &mut W,
) -> Result<(), AppError>
where
    R: CredentialReader,
    A: UserLookupApi,
    W: Write,
{
    let result = people::info(store, api, &args.user_id).await?;
    output::write_user_info(stdout, &result)?;
    Ok(())
}

pub(super) async fn run_payment_methods<R, A, W>(
    args: PaymentMethodsArgs,
    store: &R,
    api: &A,
    stdout: &mut W,
) -> Result<(), AppError>
where
    R: CredentialReader,
    A: PaymentMethodsApi,
    W: Write,
{
    match args.operation {
        PaymentMethodsOperation::List => {
            let result = payment_methods::list(store, api).await?;
            output::write_payment_methods(stdout, &result)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "reads_tests.rs"]
mod tests;

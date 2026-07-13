use std::future::Future;
use std::io::Write;

use crate::application::ports::PromptError;
use crate::application::{
    accept, activity, auth, balance, decline, doctor, friends, pay, payment_methods,
    request_create, requests, users,
};
use crate::error::AppError;
use crate::infrastructure::credentials::NativeCredentialStore;
use crate::infrastructure::system::{SystemClientRequestIdGenerator, SystemClock};
use crate::infrastructure::venmo_api::VenmoApiClient;

use super::args::{
    AcceptArgs, ActivityArgs, ActivityOperation, AuthArgs, AuthOperation, Cli, Command,
    DeclineArgs, FriendsArgs, FriendsOperation, LoginArgs, LogoutArgs, PayArgs, PaymentMethodsArgs,
    PaymentMethodsOperation, RequestArgs, RequestsArgs, RequestsOperation, UsersArgs,
    UsersOperation,
};
use super::completions;
use super::prompt::{self, DialoguerPrompt};
use super::{output, output::write_logout_report};

const ACCEPT_RELEASED: bool = false;
const DECLINE_RELEASED: bool = false;

pub fn run<W: Write, E: Write>(cli: Cli, stdout: &mut W, stderr: &mut E) -> Result<(), AppError> {
    match cli.command {
        Command::Completions(args) => completions::write(args.shell, stdout)
            .map_err(|source| AppError::CompletionOutput { source }),
        Command::Auth(args) => run_auth(args, stdout, stderr),
        Command::PaymentMethods(args) => run_payment_methods(args, stdout),
        Command::Users(args) => run_users(args, stdout, stderr),
        Command::Friends(args) => run_friends(args, stdout, stderr),
        Command::Balance => run_balance(stdout),
        Command::Activity(args) => run_activity(args, stdout, stderr),
        Command::Requests(args) => run_requests(args, stdout, stderr),
        Command::Doctor => run_doctor(stdout),
        Command::Pay(args) => run_pay(args, stdout, stderr),
        Command::Request(args) => run_request_create(args, stdout),
        Command::Accept(args) if ACCEPT_RELEASED => run_accept(args, stdout, stderr),
        Command::Accept(_) => unavailable("accept"),
        Command::Decline(args) if DECLINE_RELEASED => run_decline(args, stdout, stderr),
        Command::Decline(_) => unavailable("decline"),
    }
}

fn run_pay<W: Write, E: Write>(
    args: PayArgs,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<(), AppError> {
    let store = NativeCredentialStore::new();
    let prompt = DialoguerPrompt::new();
    let runtime = runtime()?;
    let api = production_api()?;
    let generator = SystemClientRequestIdGenerator;
    let prepared = runtime.block_on(pay::prepare(
        &store,
        &api,
        &generator,
        &prompt,
        &args.recipient,
        args.amount,
        args.note,
        args.from.as_ref(),
    ))?;
    output::write_pay_preflight(stderr, &prepared)
        .and_then(|()| stderr.flush())
        .map_err(|source| AppError::CommandOutput { source })?;
    let authorized = pay::authorize(&prompt, prepared, args.yes)?;
    let result = runtime.block_on(protect_financial_write(pay::execute(&api, authorized)))??;
    output::write_pay_result(stdout, &result)
        .and_then(|()| stdout.flush())
        .map_err(|source| AppError::FinancialResultOutput { source })?;
    Ok(())
}

fn run_request_create<W: Write>(args: RequestArgs, stdout: &mut W) -> Result<(), AppError> {
    let store = NativeCredentialStore::new();
    let runtime = runtime()?;
    let api = production_api()?;
    let generator = SystemClientRequestIdGenerator;
    let prepared = runtime.block_on(request_create::prepare(
        &store,
        &api,
        &generator,
        &args.recipient,
        args.amount,
        args.note,
    ))?;
    let result = runtime.block_on(protect_financial_write(request_create::execute(
        &api, prepared,
    )))??;
    output::write_request_create_result(stdout, &result)
        .and_then(|()| stdout.flush())
        .map_err(|source| AppError::FinancialResultOutput { source })?;
    Ok(())
}

fn run_accept<W: Write, E: Write>(
    args: AcceptArgs,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<(), AppError> {
    let store = NativeCredentialStore::new();
    let prompt = DialoguerPrompt::new();
    let runtime = runtime()?;
    let api = production_api()?;
    let prepared = runtime.block_on(accept::prepare(&store, &api, &args.request_id))?;
    output::write_accept_preflight(stderr, &prepared)
        .and_then(|()| stderr.flush())
        .map_err(|source| AppError::CommandOutput { source })?;
    let authorized = accept::authorize(&prompt, prepared, args.yes)?;
    let result = runtime.block_on(protect_financial_write(accept::execute(&api, authorized)))??;
    output::write_accept_result(stdout, &result)
        .and_then(|()| stdout.flush())
        .map_err(|source| AppError::FinancialResultOutput { source })?;
    Ok(())
}

fn run_decline<W: Write, E: Write>(
    args: DeclineArgs,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<(), AppError> {
    let store = NativeCredentialStore::new();
    let runtime = runtime()?;
    let api = production_api()?;
    let prepared = runtime.block_on(decline::prepare(&store, &api, &args.request_id))?;
    output::write_decline_preflight(stderr, &prepared)
        .and_then(|()| stderr.flush())
        .map_err(|source| AppError::CommandOutput { source })?;
    let result = runtime.block_on(protect_financial_write(decline::execute(&api, prepared)))??;
    output::write_decline_result(stdout, &result)
        .and_then(|()| stdout.flush())
        .map_err(|source| AppError::FinancialResultOutput { source })?;
    Ok(())
}

#[cfg(unix)]
async fn protect_financial_write<F>(future: F) -> Result<F::Output, AppError>
where
    F: Future,
{
    let mut interrupts = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .map_err(|source| AppError::SignalInitialization { source })?;
    let mut terminations =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .map_err(|source| AppError::SignalInitialization { source })?;
    let mut hangups = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
        .map_err(|source| AppError::SignalInitialization { source })?;
    let interruption = async {
        tokio::select! {
            _ = interrupts.recv() => {}
            _ = terminations.recv() => {}
            _ = hangups.recv() => {}
        }
    };
    protect_with_interruption(future, interruption).await
}

#[cfg(any(unix, test))]
async fn protect_with_interruption<F, S>(future: F, interruption: S) -> Result<F::Output, AppError>
where
    F: Future,
    S: Future<Output = ()>,
{
    let mut future = std::pin::pin!(future);
    let mut interruption = std::pin::pin!(interruption);
    tokio::select! {
        outcome = &mut future => Ok(outcome),
        () = &mut interruption => Err(AppError::FinancialWriteInterruptedUnknown),
    }
}

#[cfg(not(unix))]
async fn protect_financial_write<F>(future: F) -> Result<F::Output, AppError>
where
    F: Future,
{
    let mut future = std::pin::pin!(future);
    tokio::select! {
        outcome = &mut future => Ok(outcome),
        signal = tokio::signal::ctrl_c() => {
            signal.map_err(|source| AppError::SignalInitialization { source })?;
            Err(AppError::FinancialWriteInterruptedUnknown)
        }
    }
}

fn unavailable(command: &'static str) -> Result<(), AppError> {
    Err(AppError::CommandUnavailable { command })
}

fn run_friends<W: Write, E: Write>(
    args: FriendsArgs,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<(), AppError> {
    match args.operation {
        FriendsOperation::List(args) => {
            let store = NativeCredentialStore::new();
            let runtime = runtime()?;
            let api = production_api()?;
            let result = runtime.block_on(friends::list(&store, &api, args.limit, args.offset))?;
            output::write_friends(stdout, stderr, &result)
                .map_err(|source| AppError::CommandOutput { source })
        }
    }
}

fn run_balance<W: Write>(stdout: &mut W) -> Result<(), AppError> {
    let store = NativeCredentialStore::new();
    let runtime = runtime()?;
    let api = production_api()?;
    let result = runtime.block_on(balance::get(&store, &api))?;
    output::write_balance(stdout, &result).map_err(|source| AppError::CommandOutput { source })
}

fn run_activity<W: Write, E: Write>(
    args: ActivityArgs,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<(), AppError> {
    let store = NativeCredentialStore::new();
    let runtime = runtime()?;
    let api = production_api()?;
    match args.operation {
        ActivityOperation::List(args) => {
            let result = runtime.block_on(activity::list(
                &store,
                &api,
                args.limit,
                args.before_id.as_ref(),
            ))?;
            output::write_activity_list(stdout, stderr, &result)
                .map_err(|source| AppError::CommandOutput { source })
        }
        ActivityOperation::Show(args) => {
            let result = runtime.block_on(activity::show(&store, &api, &args.activity_id))?;
            output::write_activity_show(stdout, &result)
                .map_err(|source| AppError::CommandOutput { source })
        }
    }
}

fn run_requests<W: Write, E: Write>(
    args: RequestsArgs,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<(), AppError> {
    match args.operation {
        RequestsOperation::List(args) => {
            let store = NativeCredentialStore::new();
            let runtime = runtime()?;
            let api = production_api()?;
            let result = runtime.block_on(requests::list(
                &store,
                &api,
                args.direction.into(),
                args.limit,
                args.before.as_ref(),
            ))?;
            output::write_requests(stdout, stderr, &result)
                .map_err(|source| AppError::CommandOutput { source })
        }
    }
}

fn run_doctor<W: Write>(stdout: &mut W) -> Result<(), AppError> {
    let store = NativeCredentialStore::new();
    let runtime = runtime()?;
    let api_result = VenmoApiClient::production();
    let api_initialization_failed = api_result.is_err();
    let api = api_result.ok();
    let report = runtime.block_on(doctor::diagnose::<_, VenmoApiClient>(
        &store,
        api.as_ref(),
        api_initialization_failed,
    ));
    output::write_doctor(stdout, &report).map_err(|source| AppError::CommandOutput { source })?;
    if report.is_healthy() {
        Ok(())
    } else {
        Err(AppError::DoctorIncomplete)
    }
}

fn run_users<W: Write, E: Write>(
    args: UsersArgs,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<(), AppError> {
    match args.operation {
        UsersOperation::Search(args) => {
            let store = NativeCredentialStore::new();
            let runtime = runtime()?;
            let api = production_api()?;
            let result = runtime.block_on(users::search(
                &store,
                &api,
                &args.query,
                args.limit,
                args.offset,
            ))?;
            output::write_user_search(stdout, stderr, &result)
                .map_err(|source| AppError::CommandOutput { source })
        }
    }
}

fn run_payment_methods<W: Write>(args: PaymentMethodsArgs, stdout: &mut W) -> Result<(), AppError> {
    match args.operation {
        PaymentMethodsOperation::List => {
            let store = NativeCredentialStore::new();
            let runtime = runtime()?;
            let api = production_api()?;
            let result = runtime.block_on(payment_methods::list(&store, &api))?;
            output::write_payment_methods(stdout, &result)
                .map_err(|source| AppError::CommandOutput { source })
        }
    }
}

fn run_auth<W: Write, E: Write>(
    args: AuthArgs,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<(), AppError> {
    if matches!(
        args.operation,
        AuthOperation::Login(_) | AuthOperation::Reauthenticate
    ) && !prompt::process_can_prompt()
    {
        return Err(auth::LoginError::Prompt(PromptError::NotInteractive).into());
    }

    let store = NativeCredentialStore::new();
    match args.operation {
        AuthOperation::Login(LoginArgs { token: true }) => {
            let runtime = runtime()?;
            let api = production_api()?;
            let prompt = DialoguerPrompt::new();
            let result =
                runtime.block_on(auth::login_with_token(&store, &prompt, &api, &SystemClock))?;
            output::write_login_result(stdout, &result)
                .map_err(|source| AppError::CommandOutput { source })
        }
        AuthOperation::Login(LoginArgs { token: false }) => {
            let runtime = runtime()?;
            let api = production_api()?;
            let prompt = DialoguerPrompt::new();
            let report = runtime.block_on(auth::login_with_password(
                &store,
                &prompt,
                &api,
                &SystemClock,
            ))?;
            finish_password_login(stdout, stderr, &report)
        }
        AuthOperation::Reauthenticate => {
            let runtime = runtime()?;
            let api = production_api()?;
            let prompt = DialoguerPrompt::new();
            let report =
                runtime.block_on(auth::reauthenticate(&store, &prompt, &api, &SystemClock))?;
            finish_reauthentication(stdout, stderr, &report)
        }
        AuthOperation::Status => {
            let runtime = runtime()?;
            let api = production_api()?;
            let status = runtime.block_on(auth::status(&store, &api))?;
            output::write_auth_status(stdout, &status)
                .map_err(|source| AppError::CommandOutput { source })
        }
        AuthOperation::Logout(LogoutArgs { revoke: false }) => {
            let report = auth::logout_local(&store);
            finish_logout(stdout, stderr, &report)
        }
        AuthOperation::Logout(LogoutArgs { revoke: true }) => {
            let runtime = runtime()?;
            let api = production_api()?;
            let report = runtime.block_on(auth::logout(&store, &api, true));
            finish_logout(stdout, stderr, &report)
        }
    }
}

fn finish_password_login(
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    report: &auth::PasswordLoginReport,
) -> Result<(), AppError> {
    output::write_password_login_report(stdout, stderr, report)
        .map_err(|source| AppError::CommandOutput { source })?;
    if report.is_complete_success() {
        Ok(())
    } else {
        Err(AppError::AuthLoginIncomplete)
    }
}

fn finish_reauthentication(
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    report: &auth::PasswordLoginReport,
) -> Result<(), AppError> {
    output::write_reauthentication_report(stdout, stderr, report)
        .map_err(|source| AppError::CommandOutput { source })?;
    if report.is_complete_success() {
        Ok(())
    } else {
        Err(AppError::AuthLoginIncomplete)
    }
}

fn runtime() -> Result<tokio::runtime::Runtime, AppError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|source| AppError::RuntimeInitialization { source })
}

fn production_api() -> Result<VenmoApiClient, AppError> {
    VenmoApiClient::production().map_err(|source| AppError::ApiInitialization { source })
}

fn finish_logout(
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    report: &auth::LogoutReport,
) -> Result<(), AppError> {
    write_logout_report(stdout, stderr, report)
        .map_err(|source| AppError::CommandOutput { source })?;
    if report.is_complete_success() {
        Ok(())
    } else {
        Err(AppError::AuthLogoutIncomplete)
    }
}

#[cfg(test)]
mod tests {
    use std::future::{pending, ready};

    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn financial_interruption_is_always_ambiguous() {
        let result = protect_with_interruption(pending::<()>(), ready(())).await;

        assert!(matches!(
            result,
            Err(AppError::FinancialWriteInterruptedUnknown)
        ));
    }
}

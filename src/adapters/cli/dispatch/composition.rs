use std::io::Write;

use super::{auth, reads, writes};
use crate::adapters::credentials::NativeCredentialStore;
use crate::adapters::system::SystemClientRequestIdGenerator;
use crate::adapters::venmo::VenmoApiClient;

use super::super::args::{
    Command, FriendsOperation, PayOperation, RequestsOperation, TransferOperation, UsersOperation,
};
use super::super::error::AppError;
use super::super::output::TimestampFormatter;
use super::super::prompt::{DialoguerPrompt, TerminalCapabilities};

/// A deliberately small, stateless production factory. Each method is called only from the
/// command branch that needs it; constructing the factory itself initializes no service.
#[derive(Clone, Copy)]
pub(super) struct ProductionProvider {
    terminal_capabilities: TerminalCapabilities,
}

impl ProductionProvider {
    const fn new(terminal_capabilities: TerminalCapabilities) -> Self {
        Self {
            terminal_capabilities,
        }
    }

    pub(super) fn credential_store(self) -> NativeCredentialStore {
        NativeCredentialStore::new()
    }

    pub(super) fn api(self) -> Result<VenmoApiClient, AppError> {
        VenmoApiClient::production().map_err(|source| AppError::ApiInitialization { source })
    }

    pub(super) fn credential_store_and_api(
        self,
    ) -> Result<(NativeCredentialStore, VenmoApiClient), AppError> {
        Ok((self.credential_store(), self.api()?))
    }

    pub(super) fn prompt(self) -> DialoguerPrompt {
        DialoguerPrompt::new(self.terminal_capabilities)
    }

    pub(super) fn timestamps(self) -> TimestampFormatter {
        TimestampFormatter::system_or_utc()
    }
}

pub(super) async fn run_production<W, E>(
    command: Command,
    stdout: &mut W,
    stderr: &mut E,
    terminal_capabilities: TerminalCapabilities,
) -> Result<(), AppError>
where
    W: Write,
    E: Write,
{
    let provider = ProductionProvider::new(terminal_capabilities);
    match command {
        Command::Auth(args) => auth::run_production(args, provider, stdout, stderr).await,
        Command::Users(args) => match args.operation {
            UsersOperation::Search(args) => {
                let (store, api) = provider.credential_store_and_api()?;
                reads::run_user_search(args, &store, &api, stdout, stderr).await
            }
            UsersOperation::Info(args) => {
                let (store, api) = provider.credential_store_and_api()?;
                reads::run_user_info(args, &store, &api, stdout).await
            }
        },
        Command::Friends(args) => match args.operation {
            FriendsOperation::List(args) => {
                let (store, api) = provider.credential_store_and_api()?;
                reads::run_friends_list(args, &store, &api, stdout, stderr).await
            }
            FriendsOperation::Add(args) => {
                let (store, api) = provider.credential_store_and_api()?;
                let prompt = provider.prompt();
                writes::run_friend_add_with(
                    args,
                    &store,
                    &api,
                    &prompt,
                    stdout,
                    stderr,
                    writes::production_state_interruption,
                )
                .await
            }
            FriendsOperation::Remove(args) => {
                let (store, api) = provider.credential_store_and_api()?;
                let prompt = provider.prompt();
                writes::run_friend_remove_with(
                    args,
                    &store,
                    &api,
                    &prompt,
                    stdout,
                    stderr,
                    writes::production_state_interruption,
                )
                .await
            }
        },
        Command::Balance => {
            let (store, api) = provider.credential_store_and_api()?;
            reads::run_balance(&store, &api, stdout).await
        }
        Command::Activity(args) => {
            let (store, api) = provider.credential_store_and_api()?;
            let timestamps = provider.timestamps();
            reads::run_activity(args, &store, &api, &timestamps, stdout, stderr).await
        }
        Command::Requests(args) => match args.operation {
            RequestsOperation::List(args) => {
                let (store, api) = provider.credential_store_and_api()?;
                let timestamps = provider.timestamps();
                reads::run_requests_list(args, &store, &api, &timestamps, stdout, stderr).await
            }
            RequestsOperation::Create(args) => {
                let (store, api) = provider.credential_store_and_api()?;
                writes::run_request_create(
                    args,
                    &store,
                    &api,
                    &SystemClientRequestIdGenerator,
                    stdout,
                    writes::production_financial_interruption,
                )
                .await
            }
            RequestsOperation::Accept(args) => {
                let (store, api) = provider.credential_store_and_api()?;
                let prompt = provider.prompt();
                let timestamps = provider.timestamps();
                writes::run_accept_with(
                    args,
                    &store,
                    &api,
                    &prompt,
                    &timestamps,
                    stdout,
                    stderr,
                    writes::production_financial_interruption,
                )
                .await
            }
            RequestsOperation::Decline(args) => {
                let (store, api) = provider.credential_store_and_api()?;
                let prompt = provider.prompt();
                let timestamps = provider.timestamps();
                writes::run_decline_with(
                    args,
                    &store,
                    &api,
                    &prompt,
                    &timestamps,
                    stdout,
                    stderr,
                    writes::production_financial_interruption,
                )
                .await
            }
            RequestsOperation::Info(args) => {
                let (store, api) = provider.credential_store_and_api()?;
                let timestamps = provider.timestamps();
                reads::run_request_info(args, &store, &api, &timestamps, stdout).await
            }
        },
        Command::Transfer(args) => match args.operation {
            TransferOperation::Options => {
                let (store, api) = provider.credential_store_and_api()?;
                reads::run_transfer_options(&store, &api, stdout).await
            }
            TransferOperation::Out(args) => {
                let (store, api) = provider.credential_store_and_api()?;
                let prompt = provider.prompt();
                let timestamps = provider.timestamps();
                writes::run_transfer_out_with(
                    args,
                    &store,
                    &api,
                    &prompt,
                    &timestamps,
                    stdout,
                    stderr,
                    writes::production_financial_interruption,
                )
                .await
            }
        },
        Command::Pay(args) => match args.operation {
            PayOperation::Methods => {
                let (store, api) = provider.credential_store_and_api()?;
                reads::run_payment_methods(&store, &api, stdout).await
            }
            PayOperation::User(args) => {
                let (store, api) = provider.credential_store_and_api()?;
                let prompt = provider.prompt();
                writes::run_pay_with(
                    args,
                    &store,
                    &api,
                    &SystemClientRequestIdGenerator,
                    &prompt,
                    stdout,
                    stderr,
                    writes::production_financial_interruption,
                )
                .await
            }
        },
    }
}

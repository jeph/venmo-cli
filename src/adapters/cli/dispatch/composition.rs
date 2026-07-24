use std::io::Write;

use super::{auth, reads, writes};
use crate::adapters::credentials::{CredentialAccessPurpose, CredentialStore};
use crate::adapters::system::SystemClientRequestIdGenerator;
use crate::adapters::venmo::VenmoApiClient;

use super::super::args::{
    ActivityCommentsOperation, ActivityOperation, ActivityReactionsOperation, Command,
    FriendsOperation, PayOperation, RequestsOperation, TransferOperation, UsersOperation,
};
use super::super::command::OutputFormat;
use super::super::error::AppError;
use super::super::failure::CliFailure;
use super::super::output::{OutputSession, TimestampFormatter};
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

    pub(super) fn credential_store(
        self,
        purpose: CredentialAccessPurpose,
    ) -> Result<CredentialStore, AppError> {
        CredentialStore::production(purpose).map_err(|source| {
            AppError::CredentialStoreInitialization {
                source: Box::new(source),
            }
        })
    }

    pub(super) fn api(self) -> Result<VenmoApiClient, AppError> {
        VenmoApiClient::production().map_err(|source| AppError::ApiInitialization { source })
    }

    pub(super) fn credential_store_and_api(
        self,
    ) -> Result<(CredentialStore, VenmoApiClient), AppError> {
        Ok((
            self.credential_store(CredentialAccessPurpose::Ordinary)?,
            self.api()?,
        ))
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
    format: OutputFormat,
    stdout: &mut W,
    stderr: &mut E,
    terminal_capabilities: TerminalCapabilities,
) -> Result<(), CliFailure>
where
    W: Write,
    E: Write,
{
    let provider = ProductionProvider::new(terminal_capabilities);
    let command_id = command.id();
    let mut output = OutputSession::new(
        format,
        command_id,
        terminal_capabilities.can_prompt(),
        stdout,
        stderr,
    );
    let result: Result<(), AppError> = async {
        match command {
            Command::Auth(args) => auth::run_production(args, provider, &mut output).await,
            Command::Users(args) => match args.operation {
                UsersOperation::Search(args) => {
                    let (store, api) = provider.credential_store_and_api()?;
                    reads::run_user_search(args, &store, &api, &mut output).await
                }
                UsersOperation::Info(args) => {
                    let (store, api) = provider.credential_store_and_api()?;
                    reads::run_user_info(args, &store, &api, &mut output).await
                }
            },
            Command::Friends(args) => match args.operation {
                FriendsOperation::List(args) => {
                    let (store, api) = provider.credential_store_and_api()?;
                    reads::run_friends_list(args, &store, &api, &mut output).await
                }
                FriendsOperation::Add(args) => {
                    let (store, api) = provider.credential_store_and_api()?;
                    let prompt = provider.prompt();
                    writes::run_friend_add_with(
                        args,
                        &store,
                        &api,
                        &prompt,
                        &mut output,
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
                        &mut output,
                        writes::production_state_interruption,
                    )
                    .await
                }
            },
            Command::Balance => {
                let (store, api) = provider.credential_store_and_api()?;
                reads::run_balance(&store, &api, &mut output).await
            }
            Command::Activity(args) => match args.operation {
                ActivityOperation::List(args) => {
                    let (store, api) = provider.credential_store_and_api()?;
                    let timestamps = provider.timestamps();
                    reads::run_activity_list(args, &store, &api, &timestamps, &mut output).await
                }
                ActivityOperation::Info(args) => {
                    let (store, api) = provider.credential_store_and_api()?;
                    let timestamps = provider.timestamps();
                    reads::run_activity_info(args, &store, &api, &timestamps, &mut output).await
                }
                ActivityOperation::Comments(args) => match args.operation {
                    ActivityCommentsOperation::List(args) => {
                        let (store, api) = provider.credential_store_and_api()?;
                        let timestamps = provider.timestamps();
                        reads::run_activity_comment_list(
                            args,
                            &store,
                            &api,
                            &timestamps,
                            &mut output,
                        )
                        .await
                    }
                    ActivityCommentsOperation::Add(args) => {
                        let (store, api) = provider.credential_store_and_api()?;
                        let prompt = provider.prompt();
                        let timestamps = provider.timestamps();
                        writes::run_activity_social_with(
                            &args.activity_id,
                            crate::features::activity::social::ActivitySocialIntent::AddComment(
                                args.message,
                            ),
                            args.yes,
                            args.dry_run,
                            &store,
                            &api,
                            &prompt,
                            &timestamps,
                            &mut output,
                            writes::production_state_interruption,
                        )
                        .await
                    }
                    ActivityCommentsOperation::Remove(args) => {
                        let (store, api) = provider.credential_store_and_api()?;
                        let prompt = provider.prompt();
                        writes::run_activity_comment_remove_with(
                            args.comment_id,
                            args.yes,
                            args.dry_run,
                            &store,
                            &api,
                            &prompt,
                            &mut output,
                            writes::production_state_interruption,
                        )
                        .await
                    }
                },
                ActivityOperation::Reactions(args) => match args.operation {
                    ActivityReactionsOperation::List(args) => {
                        let (store, api) = provider.credential_store_and_api()?;
                        reads::run_activity_reaction_list(args, &store, &api, &mut output).await
                    }
                    ActivityReactionsOperation::Add(args) => {
                        let (store, api) = provider.credential_store_and_api()?;
                        let prompt = provider.prompt();
                        let timestamps = provider.timestamps();
                        writes::run_activity_reaction_with(
                            writes::ActivityReactionCommand::new(
                                &args.activity_id,
                                crate::features::activity::reactions::ActivityReactionIntent::Add(
                                    args.reaction,
                                ),
                                args.yes,
                                args.dry_run,
                            ),
                            &store,
                            &api,
                            &prompt,
                            &timestamps,
                            &mut output,
                            writes::production_state_interruption,
                        )
                        .await
                    }
                    ActivityReactionsOperation::Remove(args) => {
                        let (store, api) = provider.credential_store_and_api()?;
                        let prompt = provider.prompt();
                        let timestamps = provider.timestamps();
                        writes::run_activity_reaction_with(
                            writes::ActivityReactionCommand::new(
                                &args.activity_id,
                                crate::features::activity::reactions::ActivityReactionIntent::Remove(
                                    args.reaction,
                                ),
                                args.yes,
                                args.dry_run,
                            ),
                            &store,
                            &api,
                            &prompt,
                            &timestamps,
                            &mut output,
                            writes::production_state_interruption,
                        )
                        .await
                    }
                },
            },
            Command::Requests(args) => match args.operation {
                RequestsOperation::List(args) => {
                    let (store, api) = provider.credential_store_and_api()?;
                    let timestamps = provider.timestamps();
                    reads::run_requests_list(args, &store, &api, &timestamps, &mut output).await
                }
                RequestsOperation::Create(args) => {
                    let (store, api) = provider.credential_store_and_api()?;
                    let prompt = provider.prompt();
                    writes::run_request_create(
                        args,
                        &store,
                        &api,
                        &SystemClientRequestIdGenerator,
                        &prompt,
                        &mut output,
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
                        &mut output,
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
                        &mut output,
                        writes::production_financial_interruption,
                    )
                    .await
                }
                RequestsOperation::Cancel(args) => {
                    let (store, api) = provider.credential_store_and_api()?;
                    let prompt = provider.prompt();
                    let timestamps = provider.timestamps();
                    writes::run_cancel_with(
                        args,
                        &store,
                        &api,
                        &prompt,
                        &timestamps,
                        &mut output,
                        writes::production_financial_interruption,
                    )
                    .await
                }
                RequestsOperation::Info(args) => {
                    let (store, api) = provider.credential_store_and_api()?;
                    let timestamps = provider.timestamps();
                    reads::run_request_info(args, &store, &api, &timestamps, &mut output).await
                }
            },
            Command::Transfer(args) => match args.operation {
                TransferOperation::Options => {
                    let (store, api) = provider.credential_store_and_api()?;
                    reads::run_transfer_options(&store, &api, &mut output).await
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
                        &mut output,
                        writes::production_financial_interruption,
                    )
                    .await
                }
            },
            Command::Pay(args) => match args.operation {
                PayOperation::Options => {
                    let (store, api) = provider.credential_store_and_api()?;
                    reads::run_payment_methods(&store, &api, &mut output).await
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
                        &mut output,
                        writes::production_financial_interruption,
                    )
                    .await
                }
            },
        }
    }
    .await;
    result.map_err(|error| output.into_failure(error))
}

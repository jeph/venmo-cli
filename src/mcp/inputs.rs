use std::sync::Arc;

use clap::Parser;
use rmcp::{ErrorData, handler::server::tool::schema_for_input, model::JsonObject};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::cli::Cli;

#[derive(Debug)]
pub(crate) struct Invocation {
    pub(crate) cli: Cli,
    pub(crate) command: &'static str,
    pub(crate) human_argv: Vec<String>,
    pub(crate) may_write: bool,
    pub(crate) dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EmptyInput {}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum Visibility {
    Private,
    Friends,
    Public,
}

impl Visibility {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Friends => "friends",
            Self::Public => "public",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum RequestDirection {
    All,
    Incoming,
    Outgoing,
}

impl RequestDirection {
    const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Incoming => "incoming",
            Self::Outgoing => "outgoing",
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
enum Funding {
    /// Let the CLI resolve the eligible Venmo balance or unique external default.
    Automatic,
    /// Use exactly the payment source ID returned by `pay.options`.
    Exact {
        /// Exact peer-eligible source ID.
        source_id: String,
    },
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
enum TransferAmount {
    /// Transfer all funds from a fresh available-balance snapshot.
    All,
    /// Transfer one exact positive USD amount.
    Exact {
        /// Decimal USD amount with no symbol and at most two fractional digits.
        amount: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum TransferSpeed {
    Standard,
}

impl TransferSpeed {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PayUserInput {
    /// Exact personal-profile username, with an optional leading `@`.
    username: String,
    /// Positive decimal USD amount with at most two fractional digits.
    amount: String,
    /// Exact non-empty payment note.
    note: String,
    /// Explicit automatic or exact-source funding choice.
    funding: Funding,
    /// Whether to request Venmo Purchase Protection.
    purchase_protection: bool,
    /// Explicit payment visibility.
    visibility: Visibility,
    /// `true` previews without writing; `false` executes immediately after preflight.
    dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FriendsListInput {
    /// Exact username whose visible friends to list; omit for the active account.
    user: Option<String>,
    /// Page size from 1 through 50; omit for the CLI default.
    #[schemars(range(min = 1, max = 50))]
    limit: Option<u8>,
    /// Zero-based friend-list offset; omit for zero.
    offset: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct UsernameMutationInput {
    /// Exact username, with an optional leading `@`.
    username: String,
    /// `true` previews without writing; `false` executes immediately after preflight.
    dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct UserSearchInput {
    /// Exact username or search text. Do not infer or rewrite it.
    query: String,
    /// Page size from 1 through 50; omit for the CLI default.
    #[schemars(range(min = 1, max = 50))]
    limit: Option<u8>,
    /// Zero-based search offset; omit for zero.
    offset: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct UsernameInput {
    /// Exact username, with an optional leading `@`.
    username: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ActivityListInput {
    /// Exact username whose visible activity to list; omit for the active account.
    user: Option<String>,
    /// Page size from 1 through 50; omit for the CLI default.
    #[schemars(range(min = 1, max = 50))]
    limit: Option<u8>,
    /// Endpoint-native continuation token from an earlier `activity.list` result.
    before_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ActivityIdInput {
    /// Exact canonical activity ID.
    activity_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ActivityCommentListInput {
    /// Exact canonical activity ID.
    activity_id: String,
    /// Number of comments from 1 through 50; omit for the CLI default.
    #[schemars(range(min = 1, max = 50))]
    limit: Option<u8>,
    /// Zero-based embedded-comment offset; omit for zero.
    offset: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ActivityCommentAddInput {
    /// Exact canonical activity ID.
    activity_id: String,
    /// Exact non-whitespace comment text, at most 2000 characters.
    message: String,
    /// `true` previews without writing; `false` executes immediately after preflight.
    dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ActivityCommentRemoveInput {
    /// Exact canonical comment ID.
    comment_id: String,
    /// `true` previews without writing; `false` executes immediately.
    dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ActivityReactionMutationInput {
    /// Exact canonical activity ID.
    activity_id: String,
    /// Exact lowercase `like` or one complete Unicode emoji grapheme.
    reaction: String,
    /// `true` previews without writing; `false` executes immediately after preflight.
    dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RequestsListInput {
    /// Direction filter; omit for `all`.
    direction: Option<RequestDirection>,
    /// Server page size from 1 through 50; omit for the CLI default.
    #[schemars(range(min = 1, max = 50))]
    limit: Option<u8>,
    /// Endpoint-native continuation token from an earlier `requests.list` result.
    before: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RequestCreateInput {
    /// Exact personal-profile username, with an optional leading `@`.
    username: String,
    /// Positive decimal USD amount with at most two fractional digits.
    amount: String,
    /// Exact non-empty request note.
    note: String,
    /// Explicit request visibility.
    visibility: Visibility,
    /// `true` previews without writing; `false` executes immediately after preflight.
    dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RequestAcceptInput {
    /// Exact canonical incoming request ID.
    request_id: String,
    /// Explicit automatic or exact-source funding choice.
    funding: Funding,
    /// Whether to turn on Venmo Purchase Protection.
    purchase_protection: bool,
    /// `true` previews without writing; `false` executes immediately after preflight.
    dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RequestMutationInput {
    /// Exact canonical request ID in the direction required by this tool.
    request_id: String,
    /// `true` previews without writing; `false` executes immediately after preflight.
    dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RequestInfoInput {
    /// Exact canonical open request ID.
    request_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TransferOutInput {
    /// Explicit exact-amount or all-available choice.
    amount: TransferAmount,
    /// Transfer speed. Only `standard` is currently supported.
    speed: TransferSpeed,
    /// `true` previews without writing; `false` executes immediately after preflight.
    dry_run: bool,
}

pub(crate) fn input_schema(name: &str) -> Result<Arc<JsonObject>, String> {
    match name {
        "auth.login" | "auth.logout" | "auth.status" | "pay.options" | "balance"
        | "transfer.options" => schema_for_input::<EmptyInput>(),
        "pay.user" => schema_for_input::<PayUserInput>(),
        "friends.list" => schema_for_input::<FriendsListInput>(),
        "friends.add" | "friends.remove" => schema_for_input::<UsernameMutationInput>(),
        "users.search" => schema_for_input::<UserSearchInput>(),
        "users.info" => schema_for_input::<UsernameInput>(),
        "activity.list" => schema_for_input::<ActivityListInput>(),
        "activity.info" | "activity.reactions.list" => schema_for_input::<ActivityIdInput>(),
        "activity.comments.list" => schema_for_input::<ActivityCommentListInput>(),
        "activity.comments.add" => schema_for_input::<ActivityCommentAddInput>(),
        "activity.comments.remove" => schema_for_input::<ActivityCommentRemoveInput>(),
        "activity.reactions.add" | "activity.reactions.remove" => {
            schema_for_input::<ActivityReactionMutationInput>()
        }
        "requests.list" => schema_for_input::<RequestsListInput>(),
        "requests.create" => schema_for_input::<RequestCreateInput>(),
        "requests.accept" => schema_for_input::<RequestAcceptInput>(),
        "requests.decline" | "requests.cancel" => schema_for_input::<RequestMutationInput>(),
        "requests.info" => schema_for_input::<RequestInfoInput>(),
        "transfer.out" => schema_for_input::<TransferOutInput>(),
        _ => Err(format!("no MCP input schema is registered for `{name}`")),
    }
}

pub(crate) fn build_invocation(
    name: &'static str,
    arguments: Option<JsonObject>,
    mutating: bool,
) -> Result<Invocation, ErrorData> {
    let value = Value::Object(arguments.unwrap_or_default());
    let mut argv = base_argv();

    match name {
        "auth.login" => {
            decode::<EmptyInput>(value, name)?;
            push_words(&mut argv, &["auth", "login"]);
        }
        "auth.logout" => {
            decode::<EmptyInput>(value, name)?;
            push_words(&mut argv, &["auth", "logout"]);
        }
        "auth.status" => {
            decode::<EmptyInput>(value, name)?;
            push_words(&mut argv, &["auth", "status"]);
        }
        "pay.options" => {
            decode::<EmptyInput>(value, name)?;
            push_words(&mut argv, &["pay", "options"]);
        }
        "pay.user" => {
            let input = decode::<PayUserInput>(value, name)?;
            push_words(&mut argv, &["pay", "user"]);
            argv.extend([input.username, input.amount, input.note]);
            push_funding(&mut argv, input.funding);
            if input.purchase_protection {
                argv.push("--protect".to_owned());
            }
            push_pair(&mut argv, "--visibility", input.visibility.as_str());
            push_authorization(&mut argv, input.dry_run);
        }
        "friends.list" => {
            let input = decode::<FriendsListInput>(value, name)?;
            push_words(&mut argv, &["friends", "list"]);
            push_optional_pair(&mut argv, "--user", input.user);
            push_optional_limit(&mut argv, input.limit);
            push_optional_number(&mut argv, "--offset", input.offset);
        }
        "friends.add" | "friends.remove" => {
            let input = decode::<UsernameMutationInput>(value, name)?;
            let operation = if name == "friends.add" {
                "add"
            } else {
                "remove"
            };
            push_words(&mut argv, &["friends", operation]);
            argv.push(input.username);
            push_authorization(&mut argv, input.dry_run);
        }
        "users.search" => {
            let input = decode::<UserSearchInput>(value, name)?;
            push_words(&mut argv, &["users", "search"]);
            argv.push(input.query);
            push_optional_limit(&mut argv, input.limit);
            push_optional_number(&mut argv, "--offset", input.offset);
        }
        "users.info" => {
            let input = decode::<UsernameInput>(value, name)?;
            push_words(&mut argv, &["users", "info"]);
            argv.push(input.username);
        }
        "balance" => {
            decode::<EmptyInput>(value, name)?;
            argv.push("balance".to_owned());
        }
        "activity.list" => {
            let input = decode::<ActivityListInput>(value, name)?;
            push_words(&mut argv, &["activity", "list"]);
            push_optional_pair(&mut argv, "--user", input.user);
            push_optional_limit(&mut argv, input.limit);
            push_optional_pair(&mut argv, "--before-id", input.before_id);
        }
        "activity.info" => {
            let input = decode::<ActivityIdInput>(value, name)?;
            push_words(&mut argv, &["activity", "info"]);
            argv.push(input.activity_id);
        }
        "activity.comments.list" => {
            let input = decode::<ActivityCommentListInput>(value, name)?;
            push_words(&mut argv, &["activity", "comments", "list"]);
            argv.push(input.activity_id);
            push_optional_limit(&mut argv, input.limit);
            push_optional_number(&mut argv, "--offset", input.offset);
        }
        "activity.comments.add" => {
            let input = decode::<ActivityCommentAddInput>(value, name)?;
            push_words(&mut argv, &["activity", "comments", "add"]);
            argv.extend([input.activity_id, input.message]);
            push_authorization(&mut argv, input.dry_run);
        }
        "activity.comments.remove" => {
            let input = decode::<ActivityCommentRemoveInput>(value, name)?;
            push_words(&mut argv, &["activity", "comments", "remove"]);
            argv.push(input.comment_id);
            push_authorization(&mut argv, input.dry_run);
        }
        "activity.reactions.list" => {
            let input = decode::<ActivityIdInput>(value, name)?;
            push_words(&mut argv, &["activity", "reactions", "list"]);
            argv.push(input.activity_id);
        }
        "activity.reactions.add" | "activity.reactions.remove" => {
            let input = decode::<ActivityReactionMutationInput>(value, name)?;
            let operation = if name == "activity.reactions.add" {
                "add"
            } else {
                "remove"
            };
            push_words(&mut argv, &["activity", "reactions", operation]);
            argv.extend([input.activity_id, input.reaction]);
            push_authorization(&mut argv, input.dry_run);
        }
        "requests.list" => {
            let input = decode::<RequestsListInput>(value, name)?;
            push_words(&mut argv, &["requests", "list"]);
            if let Some(direction) = input.direction {
                push_pair(&mut argv, "--direction", direction.as_str());
            }
            push_optional_limit(&mut argv, input.limit);
            push_optional_pair(&mut argv, "--before", input.before);
        }
        "requests.create" => {
            let input = decode::<RequestCreateInput>(value, name)?;
            push_words(&mut argv, &["requests", "create"]);
            argv.extend([input.username, input.amount, input.note]);
            push_pair(&mut argv, "--visibility", input.visibility.as_str());
            push_authorization(&mut argv, input.dry_run);
        }
        "requests.accept" => {
            let input = decode::<RequestAcceptInput>(value, name)?;
            push_words(&mut argv, &["requests", "accept"]);
            argv.push(input.request_id);
            push_funding(&mut argv, input.funding);
            if input.purchase_protection {
                argv.push("--protect".to_owned());
            }
            push_authorization(&mut argv, input.dry_run);
        }
        "requests.decline" | "requests.cancel" => {
            let input = decode::<RequestMutationInput>(value, name)?;
            let operation = if name == "requests.decline" {
                "decline"
            } else {
                "cancel"
            };
            push_words(&mut argv, &["requests", operation]);
            argv.push(input.request_id);
            push_authorization(&mut argv, input.dry_run);
        }
        "requests.info" => {
            let input = decode::<RequestInfoInput>(value, name)?;
            push_words(&mut argv, &["requests", "info"]);
            argv.push(input.request_id);
        }
        "transfer.options" => {
            decode::<EmptyInput>(value, name)?;
            push_words(&mut argv, &["transfer", "options"]);
        }
        "transfer.out" => {
            let input = decode::<TransferOutInput>(value, name)?;
            push_words(&mut argv, &["transfer", "out"]);
            match input.amount {
                TransferAmount::All => argv.push("all".to_owned()),
                TransferAmount::Exact { amount } => argv.push(amount),
            }
            push_pair(&mut argv, "--speed", input.speed.as_str());
            push_authorization(&mut argv, input.dry_run);
        }
        _ => {
            return Err(ErrorData::invalid_params(
                format!("no MCP argument mapping is registered for `{name}`"),
                None,
            ));
        }
    }

    let dry_run = argv.iter().any(|argument| argument == "--dry-run");
    let human_argv = argv
        .iter()
        .filter(|argument| argument.as_str() != "--json" && argument.as_str() != "--yes")
        .cloned()
        .collect();
    let cli = Cli::try_parse_from(&argv).map_err(|error| {
        let rendered = error.to_string();
        let summary = rendered.lines().next().unwrap_or("invalid CLI arguments");
        ErrorData::invalid_params(
            format!("invalid arguments for MCP tool `{name}`: {summary}"),
            None,
        )
    })?;
    if cli.command.id().as_str() != name {
        return Err(ErrorData::internal_error(
            "MCP tool mapped to an unexpected CLI command",
            None,
        ));
    }

    Ok(Invocation {
        cli,
        command: name,
        human_argv,
        may_write: mutating && !dry_run,
        dry_run,
    })
}

fn decode<T>(value: Value, name: &str) -> Result<T, ErrorData>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(value).map_err(|error| {
        ErrorData::invalid_params(
            format!("invalid arguments for MCP tool `{name}`: {error}"),
            None,
        )
    })
}

fn base_argv() -> Vec<String> {
    vec!["venmo".to_owned(), "--json".to_owned()]
}

fn push_words(argv: &mut Vec<String>, words: &[&str]) {
    argv.extend(words.iter().map(|word| (*word).to_owned()));
}

fn push_pair(argv: &mut Vec<String>, flag: &str, value: &str) {
    argv.extend([flag.to_owned(), value.to_owned()]);
}

fn push_optional_pair(argv: &mut Vec<String>, flag: &str, value: Option<String>) {
    if let Some(value) = value {
        argv.extend([flag.to_owned(), value]);
    }
}

fn push_optional_limit(argv: &mut Vec<String>, limit: Option<u8>) {
    if let Some(limit) = limit {
        push_pair(argv, "--limit", &limit.to_string());
    }
}

fn push_optional_number(argv: &mut Vec<String>, flag: &str, value: Option<u32>) {
    if let Some(value) = value {
        push_pair(argv, flag, &value.to_string());
    }
}

fn push_funding(argv: &mut Vec<String>, funding: Funding) {
    if let Funding::Exact { source_id } = funding {
        argv.extend(["--source".to_owned(), source_id]);
    }
}

fn push_authorization(argv: &mut Vec<String>, dry_run: bool) {
    argv.push(if dry_run { "--dry-run" } else { "--yes" }.to_owned());
}

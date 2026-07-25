use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use rmcp::{ServerHandler, model::JsonObject};
use serde_json::{Value, json};

use crate::cli::{
    ActivityCommentsOperation, ActivityOperation, ActivityReactionsOperation, Cli, Command,
    FriendsOperation, PayOperation, RequestsOperation, TransferOperation,
};

use super::{
    catalog::{TOOL_SPECS, ToolBehavior},
    execution::{ExecutionFuture, Executor},
    inputs::{self, Invocation},
    resources::{self, HELP_PAGES},
    server::VenmoMcpServer,
};

struct ScriptedExecutor {
    calls: AtomicUsize,
    envelope: Value,
}

impl ScriptedExecutor {
    fn new(envelope: Value) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            envelope,
        }
    }
}

impl Executor for ScriptedExecutor {
    fn execute<'a>(&'a self, _invocation: Invocation) -> ExecutionFuture<'a> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(std::future::ready(self.envelope.clone()))
    }
}

#[test]
fn catalog_has_every_leaf_tool_with_conservative_annotations()
-> Result<(), Box<dyn std::error::Error>> {
    let server = VenmoMcpServer::new()?;
    let tools = server.test_tools();
    assert_eq!(tools.len(), 27);
    assert_eq!(TOOL_SPECS.len(), 27);

    for spec in TOOL_SPECS {
        let tool = tools
            .iter()
            .find(|tool| tool.name == spec.name)
            .ok_or_else(|| format!("missing tool {}", spec.name))?;
        let annotations = tool
            .annotations
            .as_ref()
            .ok_or_else(|| format!("missing annotations for {}", spec.name))?;
        match spec.behavior {
            ToolBehavior::ReadOnly => {
                assert_eq!(annotations.read_only_hint, Some(true));
                assert_eq!(annotations.destructive_hint, Some(false));
                assert_eq!(annotations.idempotent_hint, Some(true));
                assert_eq!(annotations.open_world_hint, Some(true));
            }
            ToolBehavior::HumanHandoff => {
                assert_eq!(annotations.read_only_hint, Some(true));
                assert_eq!(annotations.destructive_hint, Some(false));
                assert_eq!(annotations.idempotent_hint, Some(true));
                assert_eq!(annotations.open_world_hint, Some(false));
            }
            ToolBehavior::Mutation {
                destructive,
                local_only,
            } => {
                assert_eq!(annotations.read_only_hint, Some(false));
                assert_eq!(annotations.destructive_hint, Some(destructive));
                assert_eq!(annotations.idempotent_hint, Some(false));
                assert_eq!(annotations.open_world_hint, Some(!local_only));
            }
        }
        assert_eq!(tool.title.as_deref(), Some(spec.title));
        assert!(tool.output_schema.is_some());
        assert!(tool.meta.is_some());
    }
    Ok(())
}

#[test]
fn every_tool_description_contains_current_leaf_help() -> Result<(), Box<dyn std::error::Error>> {
    let server = VenmoMcpServer::new()?;
    for spec in TOOL_SPECS {
        let help = resources::render_help(spec.help_path)?;
        let description = server
            .test_tools()
            .iter()
            .find(|tool| tool.name == spec.name)
            .and_then(|tool| tool.description.as_deref())
            .ok_or_else(|| format!("missing description for {}", spec.name))?;
        assert!(description.ends_with(&help), "tool: {}", spec.name);
        assert!(description.contains(&resources::help_uri(spec.help_path)));
        assert!(description.contains(spec.reference_uri));
    }
    Ok(())
}

#[test]
fn schemas_forbid_unknown_root_fields_and_expose_limit_ranges()
-> Result<(), Box<dyn std::error::Error>> {
    let server = VenmoMcpServer::new()?;
    for tool in server.test_tools() {
        assert_eq!(
            tool.input_schema.get("additionalProperties"),
            Some(&Value::Bool(false)),
            "tool: {}",
            tool.name
        );
        let serialized = serde_json::to_string(tool.input_schema.as_ref())?;
        for secret in [
            "password",
            "token",
            "v_id",
            "device_id",
            "otp",
            "login_identifier",
        ] {
            assert!(
                !serialized.contains(&format!("\"{secret}\"")),
                "tool: {}",
                tool.name
            );
        }
    }

    for name in [
        "friends.list",
        "users.search",
        "activity.list",
        "activity.comments.list",
        "requests.list",
    ] {
        let schema = server
            .test_tools()
            .iter()
            .find(|tool| tool.name == name)
            .ok_or_else(|| format!("missing tool {name}"))?
            .input_schema
            .as_ref();
        let schema = Value::Object(schema.clone());
        let limit = schema
            .pointer("/properties/limit/anyOf/0")
            .or_else(|| schema.pointer("/properties/limit"))
            .ok_or_else(|| format!("missing limit schema for {name}"))?;
        assert_eq!(limit.get("minimum"), Some(&json!(1)));
        assert_eq!(limit.get("maximum"), Some(&json!(50)));
    }

    for spec in TOOL_SPECS.iter().filter(|spec| {
        matches!(
            spec.behavior,
            ToolBehavior::Mutation {
                local_only: false,
                ..
            }
        )
    }) {
        let required = server
            .test_tools()
            .iter()
            .find(|tool| tool.name == spec.name)
            .and_then(|tool| tool.input_schema.get("required"))
            .and_then(Value::as_array)
            .ok_or_else(|| format!("missing required array for {}", spec.name))?;
        assert!(required.contains(&json!("dry_run")), "tool: {}", spec.name);
    }
    Ok(())
}

#[test]
fn every_input_mapping_rejects_unknown_and_secret_fields() -> Result<(), Box<dyn std::error::Error>>
{
    for spec in TOOL_SPECS {
        let valid = valid_arguments(spec.name, true)?;
        for field in [
            "unexpected",
            "password",
            "token",
            "v_id",
            "device_id",
            "otp",
            "login_identifier",
        ] {
            let mut invalid = valid.clone();
            invalid.insert(
                field.to_owned(),
                Value::String("must-not-be-accepted".to_owned()),
            );
            assert!(
                inputs::build_invocation(spec.name, Some(invalid), spec.behavior.mutating(),)
                    .is_err(),
                "accepted {field} for {}",
                spec.name
            );
        }
    }
    Ok(())
}

#[test]
fn every_mapping_reaches_the_expected_cli_leaf() -> Result<(), Box<dyn std::error::Error>> {
    for spec in TOOL_SPECS {
        let invocation = inputs::build_invocation(
            spec.name,
            Some(valid_arguments(spec.name, true)?),
            spec.behavior.mutating(),
        )?;
        assert_eq!(invocation.cli.command.id().as_str(), spec.name);
    }
    Ok(())
}

#[test]
fn mutation_dry_run_maps_to_dry_run_and_execution_maps_to_yes()
-> Result<(), Box<dyn std::error::Error>> {
    for spec in TOOL_SPECS.iter().filter(|spec| {
        matches!(
            spec.behavior,
            ToolBehavior::Mutation {
                local_only: false,
                ..
            }
        )
    }) {
        for (dry_run, expected) in [(true, (false, true)), (false, (true, false))] {
            let invocation = inputs::build_invocation(
                spec.name,
                Some(valid_arguments(spec.name, dry_run)?),
                true,
            )?;
            assert_eq!(
                mutation_flags(&invocation.cli),
                Some(expected),
                "{}",
                spec.name
            );
            assert_eq!(invocation.may_write, !dry_run, "{}", spec.name);
        }
    }
    Ok(())
}

#[test]
fn resources_include_all_help_and_verbatim_skill_documents()
-> Result<(), Box<dyn std::error::Error>> {
    let server = VenmoMcpServer::new()?;
    let entries = server.test_resources().entries();
    assert_eq!(HELP_PAGES.len(), 37);
    assert_eq!(entries.len(), 47);

    for page in HELP_PAGES {
        let uri = resources::help_uri(page.path);
        let entry = entries
            .iter()
            .find(|entry| entry.metadata.uri == uri)
            .ok_or_else(|| format!("missing resource {uri}"))?;
        assert_eq!(entry.text, resources::render_help(page.path)?);
    }
    for (uri, source) in resources::embedded_skill_documents() {
        let entry = entries
            .iter()
            .find(|entry| entry.metadata.uri == uri)
            .ok_or_else(|| format!("missing resource {uri}"))?;
        assert_eq!(entry.text.as_bytes(), source.as_bytes());
    }
    Ok(())
}

#[tokio::test]
async fn readonly_result_maps_to_structured_and_compatibility_content()
-> Result<(), Box<dyn std::error::Error>> {
    let executor = Arc::new(ScriptedExecutor::new(json!({
        "command": "balance",
        "ok": true,
        "data": {"balance": {"available": {"amount": "1.00", "currency": "USD"}}}
    })));
    let server = VenmoMcpServer::with_test_executor(executor.clone())?;
    let result = server.test_call("balance", JsonObject::new()).await?;
    assert_eq!(executor.calls.load(Ordering::Relaxed), 1);
    assert_eq!(result.is_error, Some(false));
    let structured = result
        .structured_content
        .as_ref()
        .ok_or("missing structured content")?;
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|content| content.text.as_str())
        .ok_or("missing compatibility text")?;
    assert_eq!(text, structured.to_string());
    Ok(())
}

#[tokio::test]
async fn readonly_failure_maps_to_a_structured_tool_error() -> Result<(), Box<dyn std::error::Error>>
{
    let executor = Arc::new(ScriptedExecutor::new(json!({
        "command": "auth.status",
        "ok": false,
        "error": {
            "code": "missing_credential",
            "category": "credential",
            "message": "No Venmo credential is stored.",
            "exit_code": 1,
            "outcome": "not_performed"
        },
        "context": null,
        "partial_result": null
    })));
    let server = VenmoMcpServer::with_test_executor(executor)?;
    let result = server.test_call("auth.status", JsonObject::new()).await?;
    assert_eq!(result.is_error, Some(true));
    assert_eq!(
        result
            .structured_content
            .as_ref()
            .and_then(|value| value.pointer("/error/code"))
            .and_then(Value::as_str),
        Some("missing_credential")
    );
    Ok(())
}

#[tokio::test]
async fn auth_login_is_service_free_handoff() -> Result<(), Box<dyn std::error::Error>> {
    let executor = Arc::new(ScriptedExecutor::new(json!({
        "command": "auth.login",
        "ok": true,
        "data": {}
    })));
    let server = VenmoMcpServer::with_test_executor(executor.clone())?;
    let result = server.test_call("auth.login", JsonObject::new()).await?;
    assert_eq!(executor.calls.load(Ordering::Relaxed), 0);
    assert_eq!(result.is_error, Some(true));
    assert_eq!(
        result
            .structured_content
            .as_ref()
            .and_then(|value| value.pointer("/human_handoff/reason"))
            .and_then(Value::as_str),
        Some("interactive_login_required")
    );
    Ok(())
}

#[test]
fn initialization_instructions_carry_global_safety_rules() -> Result<(), Box<dyn std::error::Error>>
{
    let info = VenmoMcpServer::new()?.get_info();
    let instructions = info.instructions.ok_or("missing server instructions")?;
    for required in [
        "human-only",
        "dry_run: true",
        "dry_run: false",
        "never blindly retry unknown or completed",
        "SMS human_handoff",
        "venmo://skill/venmo-cli",
    ] {
        assert!(instructions.contains(required), "missing `{required}`");
    }
    Ok(())
}

fn valid_arguments(name: &str, dry_run: bool) -> Result<JsonObject, Box<dyn std::error::Error>> {
    let value = match name {
        "auth.login" | "auth.logout" | "auth.status" | "pay.options" | "balance"
        | "transfer.options" => json!({}),
        "pay.user" => json!({
            "username": "alice",
            "amount": "1.00",
            "note": "Synthetic payment",
            "funding": {"mode": "automatic"},
            "purchase_protection": false,
            "visibility": "private",
            "dry_run": dry_run
        }),
        "friends.list" => json!({"user": null, "limit": 10, "offset": 0}),
        "friends.add" | "friends.remove" => {
            json!({"username": "alice", "dry_run": dry_run})
        }
        "users.search" => json!({"query": "alice", "limit": 10, "offset": 0}),
        "users.info" => json!({"username": "alice"}),
        "activity.list" => json!({"user": null, "limit": 10, "before_id": null}),
        "activity.info" | "activity.reactions.list" => json!({"activity_id": "activity-1"}),
        "activity.comments.list" => {
            json!({"activity_id": "activity-1", "limit": 10, "offset": 0})
        }
        "activity.comments.add" => json!({
            "activity_id": "activity-1",
            "message": "Synthetic comment",
            "dry_run": dry_run
        }),
        "activity.comments.remove" => {
            json!({"comment_id": "comment-1", "dry_run": dry_run})
        }
        "activity.reactions.add" | "activity.reactions.remove" => json!({
            "activity_id": "activity-1",
            "reaction": "like",
            "dry_run": dry_run
        }),
        "requests.list" => json!({"direction": "all", "limit": 10, "before": null}),
        "requests.create" => json!({
            "username": "alice",
            "amount": "1.00",
            "note": "Synthetic request",
            "visibility": "private",
            "dry_run": dry_run
        }),
        "requests.accept" => json!({
            "request_id": "request-1",
            "funding": {"mode": "automatic"},
            "purchase_protection": false,
            "dry_run": dry_run
        }),
        "requests.decline" | "requests.cancel" => {
            json!({"request_id": "request-1", "dry_run": dry_run})
        }
        "requests.info" => json!({"request_id": "request-1"}),
        "transfer.out" => json!({
            "amount": {"mode": "exact", "amount": "1.00"},
            "speed": "standard",
            "dry_run": dry_run
        }),
        _ => return Err(format!("missing synthetic arguments for {name}").into()),
    };
    match value {
        Value::Object(object) => Ok(object),
        _ => Err(format!("synthetic arguments for {name} were not an object").into()),
    }
}

fn mutation_flags(cli: &Cli) -> Option<(bool, bool)> {
    match &cli.command {
        Command::Pay(args) => match &args.operation {
            PayOperation::User(args) => Some((args.yes, args.dry_run)),
            PayOperation::Options => None,
        },
        Command::Friends(args) => match &args.operation {
            FriendsOperation::Add(args) => Some((args.yes, args.dry_run)),
            FriendsOperation::Remove(args) => Some((args.yes, args.dry_run)),
            FriendsOperation::List(_) => None,
        },
        Command::Activity(args) => match &args.operation {
            ActivityOperation::Comments(args) => match &args.operation {
                ActivityCommentsOperation::Add(args) => Some((args.yes, args.dry_run)),
                ActivityCommentsOperation::Remove(args) => Some((args.yes, args.dry_run)),
                ActivityCommentsOperation::List(_) => None,
            },
            ActivityOperation::Reactions(args) => match &args.operation {
                ActivityReactionsOperation::Add(args) => Some((args.yes, args.dry_run)),
                ActivityReactionsOperation::Remove(args) => Some((args.yes, args.dry_run)),
                ActivityReactionsOperation::List(_) => None,
            },
            ActivityOperation::List(_) | ActivityOperation::Info(_) => None,
        },
        Command::Requests(args) => match &args.operation {
            RequestsOperation::Create(args) => Some((args.yes, args.dry_run)),
            RequestsOperation::Accept(args) => Some((args.yes, args.dry_run)),
            RequestsOperation::Decline(args) => Some((args.yes, args.dry_run)),
            RequestsOperation::Cancel(args) => Some((args.yes, args.dry_run)),
            RequestsOperation::List(_) | RequestsOperation::Info(_) => None,
        },
        Command::Transfer(args) => match &args.operation {
            TransferOperation::Out(args) => Some((args.yes, args.dry_run)),
            TransferOperation::Options => None,
        },
        Command::Auth(_) | Command::Users(_) | Command::Balance => None,
    }
}

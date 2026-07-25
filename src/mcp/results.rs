use std::sync::Arc;

use rmcp::model::{CallToolResult, JsonObject};
use serde_json::{Value, json};

use super::inputs::Invocation;

const SMS_HANDOFF_MESSAGE: &str =
    "Venmo requires SMS verification; rerun in a terminal that can prompt for the code";

pub(crate) fn output_schema(command: &str) -> Arc<JsonObject> {
    let schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "oneOf": [
            {
                "type": "object",
                "properties": {
                    "command": { "const": command },
                    "ok": { "const": true },
                    "data": { "type": "object" }
                },
                "required": ["command", "ok", "data"],
                "additionalProperties": false
            },
            {
                "type": "object",
                "properties": {
                    "command": { "const": command },
                    "ok": { "const": false },
                    "error": {
                        "type": "object",
                        "properties": {
                            "code": { "type": "string" },
                            "category": {
                                "enum": [
                                    "usage", "cancelled", "credential", "authentication",
                                    "network", "timeout", "api", "api_contract",
                                    "ambiguous_write", "internal"
                                ]
                            },
                            "message": { "type": "string" },
                            "exit_code": { "type": "integer" },
                            "outcome": {
                                "enum": ["not_performed", "partial", "unknown", "completed"]
                            }
                        },
                        "required": ["code", "category", "message", "exit_code", "outcome"],
                        "additionalProperties": false
                    },
                    "context": {
                        "anyOf": [
                            {
                                "type": "object",
                                "properties": { "plan": { "type": "object" } },
                                "required": ["plan"],
                                "additionalProperties": false
                            },
                            { "type": "null" }
                        ]
                    },
                    "partial_result": {
                        "anyOf": [{ "type": "object" }, { "type": "null" }]
                    },
                    "human_handoff": {
                        "type": "object",
                        "properties": {
                            "reason": {
                                "enum": ["interactive_login_required", "sms_verification_required"]
                            },
                            "argv": {
                                "type": "array",
                                "items": { "type": "string" },
                                "minItems": 1
                            },
                            "instructions": { "type": "string" }
                        },
                        "required": ["reason", "argv", "instructions"],
                        "additionalProperties": false
                    }
                },
                "required": ["command", "ok", "error", "context", "partial_result"],
                "additionalProperties": false
            }
        ]
    });
    match schema {
        Value::Object(object) => Arc::new(object),
        _ => Arc::new(JsonObject::new()),
    }
}

pub(crate) fn into_tool_result(command: &str, envelope: Value, may_write: bool) -> CallToolResult {
    let envelope = if valid_envelope(command, &envelope) {
        envelope
    } else {
        internal_envelope(command, may_write)
    };
    if envelope.get("ok").and_then(Value::as_bool) == Some(true) {
        CallToolResult::structured(envelope)
    } else {
        CallToolResult::structured_error(envelope)
    }
}

fn valid_envelope(command: &str, envelope: &Value) -> bool {
    let Some(object) = envelope.as_object() else {
        return false;
    };
    if object.get("command").and_then(Value::as_str) != Some(command) {
        return false;
    }
    match object.get("ok").and_then(Value::as_bool) {
        Some(true) => {
            exact_keys(object, &["command", "ok", "data"], &[])
                && object.get("data").is_some_and(Value::is_object)
        }
        Some(false) => valid_failure(object),
        None => false,
    }
}

fn valid_failure(object: &JsonObject) -> bool {
    if !exact_keys(
        object,
        &["command", "ok", "error", "context", "partial_result"],
        &["human_handoff"],
    ) {
        return false;
    }
    let valid_error = object
        .get("error")
        .and_then(Value::as_object)
        .is_some_and(|error| {
            exact_keys(
                error,
                &["code", "category", "message", "exit_code", "outcome"],
                &[],
            ) && error.get("code").is_some_and(Value::is_string)
                && error.get("message").is_some_and(Value::is_string)
                && error.get("exit_code").and_then(Value::as_i64).is_some()
                && error
                    .get("category")
                    .and_then(Value::as_str)
                    .is_some_and(|category| {
                        matches!(
                            category,
                            "usage"
                                | "cancelled"
                                | "credential"
                                | "authentication"
                                | "network"
                                | "timeout"
                                | "api"
                                | "api_contract"
                                | "ambiguous_write"
                                | "internal"
                        )
                    })
                && error
                    .get("outcome")
                    .and_then(Value::as_str)
                    .is_some_and(|outcome| {
                        matches!(
                            outcome,
                            "not_performed" | "partial" | "unknown" | "completed"
                        )
                    })
        });
    let valid_context = object.get("context").is_some_and(|context| {
        context.is_null()
            || context.as_object().is_some_and(|context| {
                exact_keys(context, &["plan"], &[])
                    && context.get("plan").is_some_and(Value::is_object)
            })
    });
    let valid_partial = object
        .get("partial_result")
        .is_some_and(|partial| partial.is_null() || partial.is_object());
    let valid_handoff = object.get("human_handoff").is_none_or(valid_human_handoff);
    valid_error && valid_context && valid_partial && valid_handoff
}

fn valid_human_handoff(value: &Value) -> bool {
    value.as_object().is_some_and(|handoff| {
        exact_keys(handoff, &["reason", "argv", "instructions"], &[])
            && handoff
                .get("reason")
                .and_then(Value::as_str)
                .is_some_and(|reason| {
                    matches!(
                        reason,
                        "interactive_login_required" | "sms_verification_required"
                    )
                })
            && handoff
                .get("argv")
                .and_then(Value::as_array)
                .is_some_and(|argv| !argv.is_empty() && argv.iter().all(Value::is_string))
            && handoff.get("instructions").is_some_and(Value::is_string)
    })
}

fn exact_keys(object: &JsonObject, required: &[&str], optional: &[&str]) -> bool {
    required.iter().all(|key| object.contains_key(*key))
        && object
            .keys()
            .all(|key| required.contains(&key.as_str()) || optional.contains(&key.as_str()))
}

pub(crate) fn login_handoff(invocation: &Invocation) -> CallToolResult {
    CallToolResult::structured_error(json!({
        "command": invocation.command,
        "ok": false,
        "error": {
            "code": "human_action_required",
            "category": "usage",
            "message": "Venmo login is human-only and must run in an interactive terminal.",
            "exit_code": 2,
            "outcome": "not_performed"
        },
        "context": null,
        "partial_result": null,
        "human_handoff": {
            "reason": "interactive_login_required",
            "argv": invocation.human_argv,
            "instructions": "Run this argv in your own interactive terminal. Enter the account identifier, hidden password, trusted v_id/device ID, and any SMS code only there. Never send those values through MCP or chat."
        }
    }))
}

pub(crate) fn attach_sms_handoff(envelope: &mut Value, dry_run: bool, human_argv: &[String]) {
    if dry_run
        || envelope.pointer("/error/message").and_then(Value::as_str) != Some(SMS_HANDOFF_MESSAGE)
    {
        return;
    }

    if let Some(object) = envelope.as_object_mut() {
        object.insert(
            "human_handoff".to_owned(),
            json!({
                "reason": "sms_verification_required",
                "argv": human_argv,
                "instructions": "Do not ask for or transmit the SMS code. Give this exact argv to the user to run in their own interactive terminal. The final confirmation defaults to No because --yes has been removed; the OTP, if requested, must stay in that terminal."
            }),
        );
    }
}

pub(crate) fn cancelled_envelope(command: &str) -> Value {
    json!({
        "command": command,
        "ok": false,
        "error": {
            "code": "mcp_request_cancelled",
            "category": "cancelled",
            "message": "The read-only or dry-run MCP request was cancelled before completion.",
            "exit_code": 1,
            "outcome": "not_performed"
        },
        "context": null,
        "partial_result": null
    })
}

pub(crate) fn internal_envelope(command: &str, may_write: bool) -> Value {
    json!({
        "command": command,
        "ok": false,
        "error": {
            "code": "mcp_execution_bridge",
            "category": if may_write { "ambiguous_write" } else { "internal" },
            "message": "The MCP execution bridge could not decode the CLI result.",
            "exit_code": if may_write { 3 } else { 1 },
            "outcome": if may_write { "unknown" } else { "not_performed" }
        },
        "context": null,
        "partial_result": null
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_write_result_fails_with_unknown_outcome() {
        let result = into_tool_result("pay.user", json!({"unexpected": true}), true);
        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result
                .structured_content
                .as_ref()
                .and_then(|value| value.pointer("/error/outcome"))
                .and_then(Value::as_str),
            Some("unknown")
        );
    }

    #[test]
    fn incomplete_success_is_replaced_with_a_safe_internal_failure() {
        let result = into_tool_result("balance", json!({"command": "balance", "ok": true}), false);
        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result
                .structured_content
                .as_ref()
                .and_then(|value| value.pointer("/error/code"))
                .and_then(Value::as_str),
            Some("mcp_execution_bridge")
        );
    }
}

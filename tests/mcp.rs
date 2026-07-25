use std::{
    error::Error,
    io::{BufRead, BufReader, Read, Write},
    process::Stdio,
};

use assert_cmd::{Command as AssertCommand, cargo::CommandCargoExt};
use predicates::prelude::*;
use serde_json::{Value, json};

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn help_and_version_are_service_free() -> TestResult {
    AssertCommand::cargo_bin("venmo-mcp")?
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("local MCP stdio server"))
        .stderr(predicate::str::is_empty());

    AssertCommand::cargo_bin("venmo-mcp")?
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::eq("venmo-mcp 0.0.1\n"))
        .stderr(predicate::str::is_empty());
    Ok(())
}

#[test]
fn stdio_initializes_lists_resources_and_runs_only_the_login_handoff() -> TestResult {
    let mut command = std::process::Command::cargo_bin("venmo-mcp")?;
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let mut stdin = child.stdin.take().ok_or("missing child stdin")?;
    let stdout = child.stdout.take().ok_or("missing child stdout")?;
    let mut stdout = BufReader::new(stdout);

    send(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "venmo-mcp-test", "version": "1"}
            }
        }),
    )?;
    let initialized = receive(&mut stdout)?;
    assert_eq!(initialized["id"], 1);
    assert_eq!(initialized["result"]["serverInfo"]["name"], "venmo-mcp");
    assert!(initialized["result"]["capabilities"]["tools"].is_object());
    assert!(initialized["result"]["capabilities"]["resources"].is_object());

    send(
        &mut stdin,
        &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    )?;
    send(
        &mut stdin,
        &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    )?;
    let tools = receive(&mut stdout)?;
    assert_eq!(tools["id"], 2);
    assert_eq!(tools["result"]["tools"].as_array().map(Vec::len), Some(27));

    send(
        &mut stdin,
        &json!({"jsonrpc": "2.0", "id": 3, "method": "resources/list"}),
    )?;
    let resources = receive(&mut stdout)?;
    assert_eq!(resources["id"], 3);
    assert_eq!(
        resources["result"]["resources"].as_array().map(Vec::len),
        Some(47)
    );

    send(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "resources/read",
            "params": {"uri": "venmo://skill/venmo-cli"}
        }),
    )?;
    let skill = receive(&mut stdout)?;
    assert_eq!(skill["id"], 4);
    assert!(
        skill["result"]["contents"][0]["text"]
            .as_str()
            .is_some_and(|text| text.contains("Keep authentication human-only"))
    );

    send(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {"name": "auth.login", "arguments": {}}
        }),
    )?;
    let handoff = receive(&mut stdout)?;
    assert_eq!(handoff["id"], 5);
    assert_eq!(handoff["result"]["isError"], true);
    assert_eq!(
        handoff["result"]["structuredContent"]["human_handoff"]["reason"],
        "interactive_login_required"
    );

    drop(stdin);
    let mut remaining_stdout = String::new();
    stdout.read_to_string(&mut remaining_stdout)?;
    assert!(remaining_stdout.is_empty());
    let status = child.wait()?;
    assert!(status.success());
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .ok_or("missing child stderr")?
        .read_to_string(&mut stderr)?;
    assert!(stderr.is_empty());
    Ok(())
}

fn send(writer: &mut impl Write, message: &Value) -> TestResult {
    serde_json::to_writer(&mut *writer, message)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn receive(reader: &mut impl BufRead) -> Result<Value, Box<dyn Error>> {
    let mut line = String::new();
    let bytes = reader.read_line(&mut line)?;
    if bytes == 0 {
        return Err("MCP server closed before sending a response".into());
    }
    Ok(serde_json::from_str(&line)?)
}

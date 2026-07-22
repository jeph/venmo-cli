use std::error::Error;

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn help_and_version_succeed_without_services() -> TestResult {
    let help = Command::cargo_bin("venmo")?
        .arg("--help")
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
    insta::assert_snapshot!(
        "child_process_help",
        String::from_utf8(help.get_output().stdout.clone())?
    );

    Command::cargo_bin("venmo")?
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::eq("venmo 0.0.1\n"))
        .stderr(predicate::str::is_empty());

    Ok(())
}

#[test]
fn usage_errors_exit_two_without_success_output() -> TestResult {
    let assertion = Command::cargo_bin("venmo")?
        .args(["balance", "--unexpected"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty());
    insta::assert_snapshot!(
        "child_process_usage_error",
        String::from_utf8(assertion.get_output().stderr.clone())?
    );

    Ok(())
}

#[test]
fn json_does_not_override_clap_errors_help_or_version() -> TestResult {
    for arguments in [&["--json", "not-a-command"][..], &["--", "--json"][..]] {
        let assertion = Command::cargo_bin("venmo")?
            .args(arguments)
            .assert()
            .code(2)
            .stdout(predicate::str::is_empty());
        let stderr = &assertion.get_output().stderr;
        assert!(String::from_utf8_lossy(stderr).contains("error:"));
        assert!(serde_json::from_slice::<Value>(stderr).is_err());
    }

    Command::cargo_bin("venmo")?
        .args(["--json", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: venmo"))
        .stderr(predicate::str::is_empty());

    Command::cargo_bin("venmo")?
        .args(["--json", "--version"])
        .assert()
        .success()
        .stdout(predicate::eq("venmo 0.0.1\n"))
        .stderr(predicate::str::is_empty());
    Ok(())
}

#[test]
fn json_global_placement_and_application_failures_keep_unix_streams() -> TestResult {
    for arguments in [
        &["--json", "auth", "login"][..],
        &["auth", "--json", "login"][..],
        &["auth", "login", "--json"][..],
    ] {
        let assertion = Command::cargo_bin("venmo")?
            .args(arguments)
            .assert()
            .code(2)
            .stdout(predicate::str::is_empty());
        let stderr = &assertion.get_output().stderr;
        assert_eq!(stderr.iter().filter(|byte| **byte == b'\n').count(), 1);
        let value: Value = serde_json::from_slice(stderr)?;
        assert_eq!(value["command"], "auth.login");
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "usage_error");
        assert_eq!(value["error"]["category"], "usage");
        assert_eq!(value["error"]["exit_code"], 2);
        assert_eq!(value["error"]["outcome"], "not_performed");
        assert_eq!(value["context"], Value::Null);
        assert_eq!(value["partial_result"], Value::Null);
    }
    Ok(())
}

#[test]
fn clap_errors_remain_redacted_when_json_is_present() -> TestResult {
    let secret = "sensitive request id";
    let assertion = Command::cargo_bin("venmo")?
        .args(["requests", "accept", secret, "--json"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty());
    let stderr = &assertion.get_output().stderr;
    let serialized = String::from_utf8(stderr.clone())?;
    assert!(serialized.contains("error: invalid request ID"));
    assert!(!serialized.contains(secret));
    assert!(!serialized.contains("sensitive"));
    assert!(serde_json::from_slice::<Value>(stderr).is_err());
    Ok(())
}

#[test]
fn invalid_before_usage_errors_are_redacted_and_service_free() -> TestResult {
    for (arguments, label, raw) in [
        (
            &[
                "activity",
                "list",
                "--before-id",
                "sensitive activity token",
            ][..],
            "invalid before-id continuation token",
            "sensitive activity token",
        ),
        (
            &["requests", "list", "--before", "sensitive request token"][..],
            "invalid before continuation token",
            "sensitive request token",
        ),
    ] {
        Command::cargo_bin("venmo")?
            .args(arguments)
            .assert()
            .code(2)
            .stdout(predicate::str::is_empty())
            .stderr(
                predicate::str::contains(label)
                    .and(predicate::str::contains(raw).not())
                    .and(predicate::str::contains("sensitive").not()),
            );
    }

    Ok(())
}

#[test]
fn invalid_request_mutation_ids_are_redacted_and_service_free() -> TestResult {
    for command in ["accept", "decline", "cancel"] {
        for raw in [
            "sensitive request id".to_owned(),
            "sensitive\u{202e}request".to_owned(),
            "s".repeat(513),
        ] {
            Command::cargo_bin("venmo")?
                .args(["requests", command, raw.as_str()])
                .assert()
                .code(2)
                .stdout(predicate::str::is_empty())
                .stderr(
                    predicate::str::contains("invalid request ID")
                        .and(predicate::str::contains(raw).not())
                        .and(predicate::str::contains("sensitive").not()),
                );
        }
    }

    Ok(())
}

#[test]
fn removed_top_level_request_actions_are_usage_errors() -> TestResult {
    for arguments in [
        &["request", "alice", "0.01", "Test"][..],
        &["accept", "request-1"][..],
        &["decline", "request-1"][..],
    ] {
        Command::cargo_bin("venmo")?
            .args(arguments)
            .assert()
            .code(2)
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains("unrecognized subcommand"));
    }
    Ok(())
}

#[test]
fn noninteractive_login_fails_before_keychain_or_network_access() -> TestResult {
    Command::cargo_bin("venmo")?
        .args(["auth", "login"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::eq(
            "error: an interactive terminal is required\n",
        ));

    Ok(())
}

#[test]
fn removed_commands_auth_surfaces_and_secret_arguments_are_rejected() -> TestResult {
    for arguments in [
        &["doctor"][..],
        &["completions", "bash"][..],
        &["auth", "reauthenticate"][..],
        &["auth", "login", "--token", "synthetic-secret-value"][..],
        &["auth", "logout", "--revoke"][..],
        &["auth", "login", "--password", "synthetic-secret-value"][..],
    ] {
        Command::cargo_bin("venmo")?
            .args(arguments)
            .assert()
            .code(2)
            .stdout(predicate::str::is_empty())
            .stderr(
                predicate::str::contains("error:")
                    .and(predicate::str::contains("synthetic-secret-value").not()),
            );
    }

    Ok(())
}

#[test]
fn clap_schema_generates_a_manpage() -> TestResult {
    use clap::CommandFactory;

    let command = venmo_cli::cli::Cli::command();
    let manpage = clap_mangen::Man::new(command);
    let mut rendered = Vec::new();
    manpage.render(&mut rendered)?;
    let text = String::from_utf8(rendered)?;

    assert!(text.contains(".TH venmo"));
    assert!(text.contains("pay"));
    assert!(!text.contains("payment\\-methods"));
    assert!(text.contains("requests"));

    Ok(())
}

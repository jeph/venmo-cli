use std::error::Error;

use assert_cmd::Command;
use predicates::prelude::*;

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn help_and_version_succeed_without_services() -> TestResult {
    Command::cargo_bin("venmo")?
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "An unofficial Venmo command-line client",
        ))
        .stderr(predicate::str::is_empty());

    Command::cargo_bin("venmo")?
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::eq("venmo 0.1.0\n"))
        .stderr(predicate::str::is_empty());

    Ok(())
}

#[test]
fn usage_errors_exit_two_without_success_output() -> TestResult {
    Command::cargo_bin("venmo")?
        .args(["pay", "alice", "1.00", "--note", "Dinner"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("invalid value 'alice'"));

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
fn completion_generation_is_a_service_free_success_path() -> TestResult {
    Command::cargo_bin("venmo")?
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("_venmo"))
        .stderr(predicate::str::is_empty());

    Ok(())
}

#[test]
fn noninteractive_login_fails_before_keychain_or_network_access() -> TestResult {
    for arguments in [
        &["auth", "login"][..],
        &["auth", "login", "--token"][..],
        &["auth", "reauthenticate"][..],
    ] {
        Command::cargo_bin("venmo")?
            .args(arguments)
            .assert()
            .code(1)
            .stdout(predicate::str::is_empty())
            .stderr(predicate::eq(
                "error: an interactive terminal is required\n",
            ));
    }

    Ok(())
}

#[test]
fn reauthenticate_rejects_secret_and_alternate_input_options() -> TestResult {
    for option in [
        "--username",
        "--password",
        "--otp",
        "--token",
        "--device-id",
        "--env",
        "--stdin-file",
    ] {
        Command::cargo_bin("venmo")?
            .args(["auth", "reauthenticate", option, "synthetic-secret-value"])
            .assert()
            .code(2)
            .stdout(predicate::str::is_empty())
            .stderr(
                predicate::str::contains("unexpected argument")
                    .and(predicate::str::contains("synthetic-secret-value").not()),
            );
    }

    Ok(())
}

#[test]
fn unfinished_commands_fail_truthfully() -> TestResult {
    Command::cargo_bin("venmo")?
        .args(["pay", "@alice", "0.01", "--note", "Test", "--yes"])
        .assert()
        .code(1)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::eq(
            "error: the `pay` command is not implemented in this development build\n",
        ));

    Ok(())
}

#[test]
fn verbose_mode_does_not_echo_command_secrets_or_notes() -> TestResult {
    Command::cargo_bin("venmo")?
        .args([
            "--verbose",
            "pay",
            "@alice",
            "0.01",
            "--note",
            "private-test-note",
            "--yes",
        ])
        .assert()
        .code(1)
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains("not implemented in this development build")
                .and(predicate::str::contains("private-test-note").not())
                .and(predicate::str::contains("\u{1b}[").not()),
        );

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
    assert!(text.contains("payment\\-methods"));
    assert!(text.contains("request"));

    Ok(())
}

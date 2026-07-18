use std::error::Error;

use assert_cmd::Command;
use predicates::prelude::*;

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
        .stdout(predicate::eq("venmo 0.2.0\n"))
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
    for command in ["accept", "decline"] {
        for raw in [
            "sensitive request id".to_owned(),
            "sensitive\u{202e}request".to_owned(),
            "s".repeat(513),
        ] {
            Command::cargo_bin("venmo")?
                .args([command, raw.as_str()])
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
fn removed_auth_surfaces_and_secret_arguments_are_rejected() -> TestResult {
    for arguments in [
        &["doctor"][..],
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
    assert!(text.contains("payment\\-methods"));
    assert!(text.contains("request"));
    assert!(text.contains("accept"));
    assert!(text.contains("decline"));

    Ok(())
}

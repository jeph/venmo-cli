use std::io;

use clap::{Command as ClapCommand, CommandFactory, Parser, error::ErrorKind};
use venmo_cli::cli::{
    AuthOperation, Cli, Command, CompletionShell, RequestDirectionArg, RequestsOperation,
    UsersOperation, handle_runtime_initialization_failure, run,
};

fn command_at_path(mut command: ClapCommand, path: &[&str]) -> Option<ClapCommand> {
    for name in path {
        let next = command
            .get_subcommands()
            .find(|candidate| candidate.get_name() == *name)
            .cloned();
        match next {
            Some(next) => command = next,
            None => return None,
        }
    }
    Some(command)
}

fn assert_rejected(arguments: &[&str]) {
    assert!(
        Cli::try_parse_from(arguments).is_err(),
        "accepted {arguments:?}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn public_production_dispatch_keeps_completions_service_free() {
    let cli = Cli::try_parse_from(["venmo", "completions", "bash"]);
    assert!(cli.is_ok());
    if let Ok(cli) = cli {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let result = run(cli, &mut stdout, &mut stderr).await;

        assert!(result.is_ok());
        assert!(String::from_utf8_lossy(&stdout).contains("_venmo"));
        assert!(stderr.is_empty());
    }
}

#[test]
fn public_runtime_fallback_keeps_completions_service_free() {
    let cli = Cli::try_parse_from(["venmo", "completions", "fish"]);
    assert!(cli.is_ok());
    if let Ok(cli) = cli {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let result = handle_runtime_initialization_failure(
            cli,
            &mut stdout,
            &mut stderr,
            io::Error::other("synthetic runtime failure"),
        );

        assert!(result.is_ok());
        assert!(String::from_utf8_lossy(&stdout).contains("complete -c venmo"));
        assert!(stderr.is_empty());
    }
}

#[test]
fn auth_login_modes_expose_no_secret_arguments() {
    for (arguments, expected_token_mode) in [
        (&["venmo", "auth", "login"][..], false),
        (&["venmo", "auth", "login", "--token"][..], true),
    ] {
        let parsed = Cli::try_parse_from(arguments);
        let token_mode = match parsed {
            Ok(cli) => match cli.command {
                Command::Auth(args) => match args.operation {
                    AuthOperation::Login(login) => Some(login.token),
                    AuthOperation::Reauthenticate
                    | AuthOperation::Logout(_)
                    | AuthOperation::Status => None,
                },
                _ => None,
            },
            Err(_) => None,
        };
        assert_eq!(token_mode, Some(expected_token_mode));
    }

    assert_rejected(&["venmo", "auth", "login", "--token", "secret"]);
    assert_rejected(&["venmo", "auth", "login", "--password", "secret"]);
    assert_rejected(&["venmo", "auth", "login", "--device-id", "secret"]);
    assert_rejected(&["venmo", "auth", "login", "--v-id", "secret"]);
    assert_rejected(&["venmo", "auth", "login", "alice@example.com"]);
}

#[test]
fn auth_reauthenticate_exposes_no_secret_or_alternate_input_arguments() {
    let parsed = Cli::try_parse_from(["venmo", "auth", "reauthenticate"]);
    assert!(parsed.is_ok_and(|cli| matches!(
        cli.command,
        Command::Auth(args) if matches!(args.operation, AuthOperation::Reauthenticate)
    )));

    for option in [
        "--username",
        "--email",
        "--account",
        "--identifier",
        "--password",
        "--otp",
        "--otp-secret",
        "--token",
        "--access-token",
        "--device-id",
        "--v-id",
        "--env",
        "--stdin",
        "--stdin-file",
        "--file",
    ] {
        assert_rejected(&["venmo", "auth", "reauthenticate", option, "secret"]);
    }
    assert_rejected(&["venmo", "auth", "reauthenticate", "alice@example.com"]);
}

#[test]
fn direct_request_creation_dispatches_to_create() {
    let parsed = Cli::try_parse_from(["venmo", "request", "@alice", "12.50", "--note", "Dinner"]);
    let dispatches_to_create = parsed.is_ok_and(|cli| matches!(cli.command, Command::Request(_)));
    assert!(dispatches_to_create);
}

#[test]
fn user_and_request_info_parse_exact_typed_ids() {
    let user = Cli::try_parse_from(["venmo", "users", "info", "123456"]);
    let user_id = match user {
        Ok(cli) => match cli.command {
            Command::Users(args) => match args.operation {
                UsersOperation::Info(args) => Some(args.user_id.to_string()),
                UsersOperation::Search(_) => None,
            },
            _ => None,
        },
        Err(_) => None,
    };
    assert_eq!(user_id.as_deref(), Some("123456"));

    let request = Cli::try_parse_from(["venmo", "requests", "info", "request-123"]);
    let request_id = match request {
        Ok(cli) => match cli.command {
            Command::Requests(args) => match args.operation {
                RequestsOperation::Info(args) => Some(args.request_id.to_string()),
                RequestsOperation::List(_) => None,
            },
            _ => None,
        },
        Err(_) => None,
    };
    assert_eq!(request_id.as_deref(), Some("request-123"));

    for arguments in [
        &["venmo", "users", "info"][..],
        &["venmo", "users", "info", "user id"][..],
        &["venmo", "users", "info", "123", "extra"][..],
        &["venmo", "requests", "info"][..],
        &["venmo", "requests", "info", "request id"][..],
        &[
            "venmo",
            "requests",
            "info",
            "request-1",
            "--direction",
            "incoming",
        ][..],
    ] {
        assert_rejected(arguments);
    }
}

#[test]
fn accept_username_is_not_the_accept_subcommand() {
    let parsed = Cli::try_parse_from(["venmo", "request", "@accept", "0.01", "--note", "Test"]);
    let dispatches_to_create = parsed.is_ok_and(|cli| matches!(cli.command, Command::Request(_)));
    assert!(dispatches_to_create);
}

#[test]
fn top_level_accept_and_decline_have_distinct_minimal_arguments() {
    let accept = Cli::try_parse_from(["venmo", "accept", "request-123", "--yes"]);
    assert!(accept.is_ok_and(|cli| matches!(cli.command, Command::Accept(args) if args.yes)));

    let decline = Cli::try_parse_from(["venmo", "decline", "request-123"]);
    assert!(decline.is_ok_and(|cli| matches!(cli.command, Command::Decline(_))));

    assert_rejected(&["venmo", "request", "accept", "request-123"]);
    assert_rejected(&["venmo", "accept", "request-123", "--from", "method-456"]);
    assert_rejected(&["venmo", "decline", "request-123", "--yes"]);
}

#[test]
fn numeric_recipient_and_global_option_placement_are_supported() {
    let parsed = Cli::try_parse_from([
        "venmo",
        "request",
        "--verbose",
        "123456789",
        "0.01",
        "--note",
        "Test",
    ]);
    let dispatches_to_create = match parsed {
        Ok(cli) => cli.verbose && matches!(cli.command, Command::Request(_)),
        Err(_) => false,
    };
    assert!(dispatches_to_create);

    let accept = Cli::try_parse_from(["venmo", "--verbose", "accept", "request-123"]);
    assert!(accept.is_ok_and(|cli| cli.verbose));
}

#[test]
fn request_and_request_mutation_forms_cannot_be_mixed() {
    assert_rejected(&[
        "venmo",
        "accept",
        "request-123",
        "12.50",
        "--note",
        "Dinner",
    ]);
    assert_rejected(&[
        "venmo", "request", "@alice", "12.50", "--note", "Dinner", "--yes",
    ]);
    assert_rejected(&[
        "venmo", "request", "@alice", "12.50", "--note", "Dinner", "--from", "method-1",
    ]);
}

#[test]
fn removed_and_deferred_forms_are_rejected() {
    for arguments in [
        &["venmo", "init"][..],
        &["venmo", "deinit"][..],
        &["venmo", "charge", "@alice", "1.00", "note"][..],
        &[
            "venmo", "request", "create", "@alice", "1.00", "--note", "note",
        ][..],
        &[
            "venmo",
            "pay",
            "@alice",
            "1.00",
            "--note",
            "note",
            "--dry-run",
        ][..],
        &[
            "venmo",
            "request",
            "@alice",
            "1.00",
            "--note",
            "note",
            "--dry-run",
        ][..],
        &["venmo", "activity", "show", "activity-1"][..],
    ] {
        assert_rejected(arguments);
    }
}

#[test]
fn direction_and_completion_shell_are_typed_enums() {
    let requests = Cli::try_parse_from([
        "venmo",
        "requests",
        "list",
        "--direction",
        "incoming",
        "--limit",
        "50",
    ]);
    let request_values = match requests {
        Ok(cli) => match cli.command {
            Command::Requests(args) => match args.operation {
                RequestsOperation::List(list) => Some((list.direction, list.limit.get())),
                RequestsOperation::Info(_) => None,
            },
            _ => None,
        },
        Err(_) => None,
    };
    assert_eq!(request_values, Some((RequestDirectionArg::Incoming, 50)));

    let completions = Cli::try_parse_from(["venmo", "completions", "powershell"]);
    let shell = match completions {
        Ok(cli) => match cli.command {
            Command::Completions(args) => Some(args.shell),
            _ => None,
        },
        Err(_) => None,
    };
    assert_eq!(shell, Some(CompletionShell::PowerShell));
}

#[test]
fn invalid_limits_are_rejected() {
    assert_rejected(&["venmo", "friends", "list", "--limit", "0"]);
    assert_rejected(&["venmo", "users", "search", "alice", "--limit", "-1"]);
    assert_rejected(&["venmo", "users", "search", "alice", "--limit", "51"]);
    assert_rejected(&["venmo", "friends", "list", "--limit", "51"]);
    assert_rejected(&["venmo", "activity", "list", "--limit", "51"]);
    assert_rejected(&["venmo", "requests", "list", "--limit", "51"]);
    assert_rejected(&["venmo", "activity", "list", "--limit", "4294967296"]);
    assert!(Cli::try_parse_from(["venmo", "users", "search", "alice", "--limit", "50"]).is_ok());
    assert!(Cli::try_parse_from(["venmo", "friends", "list", "--limit", "50"]).is_ok());
}

#[test]
fn each_paginated_command_accepts_only_its_endpoint_native_inputs() {
    for arguments in [
        &["venmo", "friends", "list", "--offset", "12"][..],
        &["venmo", "users", "search", "@alice", "--offset", "12"][..],
        &["venmo", "activity", "list", "--before-id", "story-12"][..],
        &["venmo", "requests", "list", "--before", "request-12"][..],
    ] {
        assert!(
            Cli::try_parse_from(arguments).is_ok(),
            "rejected {arguments:?}"
        );
    }

    for arguments in [
        &["venmo", "friends", "list", "--before", "request-12"][..],
        &[
            "venmo",
            "users",
            "search",
            "alice",
            "--before-id",
            "story-12",
        ][..],
        &["venmo", "activity", "list", "--offset", "12"][..],
        &["venmo", "requests", "list", "--offset", "12"][..],
        &["venmo", "payment-methods", "list", "--offset", "12"][..],
        &[
            "venmo",
            "activity",
            "info",
            "activity-1",
            "--before-id",
            "story-12",
        ][..],
        &["venmo", "friends", "list", "--page", "2"][..],
        &["venmo", "friends", "list", "--page-size", "10"][..],
    ] {
        assert_rejected(arguments);
    }
}

#[test]
fn before_token_parsers_are_strict_and_cli_debug_is_redacted() {
    for arguments in [
        &[
            "venmo",
            "activity",
            "list",
            "--before-id",
            "sensitive-activity-token",
        ][..],
        &[
            "venmo",
            "requests",
            "list",
            "--before",
            "sensitive-request-token",
        ][..],
    ] {
        let parsed = Cli::try_parse_from(arguments);
        assert!(parsed.is_ok());
        if let Ok(cli) = parsed {
            let debug = format!("{cli:?}");
            assert!(debug.contains("[REDACTED]"));
            assert!(!debug.contains("sensitive-activity-token"));
            assert!(!debug.contains("sensitive-request-token"));
        }
    }

    for (arguments, expected_message, secret_fragment) in [
        (
            &[
                "venmo",
                "activity",
                "list",
                "--before-id",
                "sensitive activity token",
            ][..],
            "error: invalid before-id continuation token: continuation token must not contain whitespace or control characters",
            "sensitive activity",
        ),
        (
            &[
                "venmo",
                "requests",
                "list",
                "--before",
                "sensitive request token",
            ][..],
            "error: invalid before continuation token: continuation token must not contain whitespace or control characters",
            "sensitive request",
        ),
        (
            &["venmo", "accept", "sensitive request id"][..],
            "error: invalid request ID: request ID must be non-empty, at most 512 bytes, and contain no whitespace or control characters",
            "sensitive request",
        ),
    ] {
        let invalid = Cli::try_parse_from(arguments);
        assert!(invalid.is_err());
        if let Err(error) = invalid {
            assert_eq!(error.kind(), ErrorKind::ValueValidation);
            let rendered = error.to_string();
            assert_eq!(rendered.lines().next(), Some(expected_message));
            assert!(!rendered.contains(secret_fragment));
        }
    }
    let oversized = "a".repeat(1_025);
    let error = Cli::try_parse_from(["venmo", "requests", "list", "--before", &oversized]);
    assert!(error.is_err());
    if let Err(error) = error {
        assert!(!error.to_string().contains(&oversized));
    }
}

#[test]
fn pagination_defaults_are_limit_ten_and_offset_zero() {
    let parsed = Cli::try_parse_from(["venmo", "friends", "list"]);
    let values = match parsed {
        Ok(cli) => match cli.command {
            Command::Friends(args) => match args.operation {
                venmo_cli::cli::FriendsOperation::List(list) => {
                    Some((list.limit.get(), list.offset.get()))
                }
            },
            _ => None,
        },
        Err(_) => None,
    };
    assert_eq!(values, Some((10, 0)));

    assert_rejected(&["venmo", "friends", "list", "--offset", "-1"]);
    assert_rejected(&[
        "venmo",
        "users",
        "search",
        "alice",
        "--offset",
        "4294967296",
    ]);
}

#[test]
fn argument_only_validation_errors_are_clap_errors() {
    assert_rejected(&["venmo", "pay", "alice", "1.00", "--note", "Dinner"]);
    assert_rejected(&["venmo", "pay", "@alice", "0", "--note", "Dinner"]);
    assert_rejected(&["venmo", "pay", "@alice", "1.001", "--note", "Dinner"]);
    assert_rejected(&["venmo", "pay", "@alice", "1.00", "--note", "   "]);
    assert_rejected(&["venmo", "request", "accept"]);
    assert_rejected(&["venmo", "completions", "nushell"]);
    assert_rejected(&["venmo", "users", "search", "   "]);
    assert_rejected(&["venmo", "users", "search", "@"]);
}

#[test]
fn every_command_has_a_help_snapshot() {
    let cases: &[(&str, &[&str])] = &[
        ("top_level", &[]),
        ("auth", &["auth"]),
        ("auth_login", &["auth", "login"]),
        ("auth_reauthenticate", &["auth", "reauthenticate"]),
        ("auth_logout", &["auth", "logout"]),
        ("auth_status", &["auth", "status"]),
        ("pay", &["pay"]),
        ("request", &["request"]),
        ("accept", &["accept"]),
        ("decline", &["decline"]),
        ("friends", &["friends"]),
        ("friends_list", &["friends", "list"]),
        ("users", &["users"]),
        ("users_search", &["users", "search"]),
        ("users_info", &["users", "info"]),
        ("payment_methods", &["payment-methods"]),
        ("payment_methods_list", &["payment-methods", "list"]),
        ("balance", &["balance"]),
        ("activity", &["activity"]),
        ("activity_list", &["activity", "list"]),
        ("activity_info", &["activity", "info"]),
        ("requests", &["requests"]),
        ("requests_list", &["requests", "list"]),
        ("requests_info", &["requests", "info"]),
        ("doctor", &["doctor"]),
        ("completions", &["completions"]),
    ];

    for (snapshot_name, path) in cases {
        let command = command_at_path(Cli::command(), path);
        assert!(command.is_some(), "missing command path: {path:?}");
        if let Some(mut command) = command {
            let mut bytes = Vec::new();
            let result = command.write_long_help(&mut bytes);
            assert!(result.is_ok(), "failed to render help for {path:?}");
            if result.is_ok() {
                let rendered = String::from_utf8_lossy(&bytes);
                let mut help = rendered
                    .lines()
                    .map(str::trim_end)
                    .collect::<Vec<_>>()
                    .join("\n");
                if rendered.ends_with('\n') {
                    help.push('\n');
                }
                insta::assert_snapshot!(*snapshot_name, help);
            }
        }
    }
}

use clap::{Command as ClapCommand, CommandFactory, Parser, error::ErrorKind};
use venmo_cli::cli::{
    ActivityOperation, AuthOperation, Cli, Command, PayOperation, RequestDirectionArg,
    RequestsOperation, TransferAmountArg, TransferOperation, TransferSpeedArg, UsersOperation,
    VisibilityArg,
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

#[test]
fn auth_login_exposes_no_secret_or_import_arguments() {
    let parsed = Cli::try_parse_from(["venmo", "auth", "login"]);
    assert!(parsed.is_ok_and(|cli| matches!(
        cli.command,
        Command::Auth(args) if matches!(args.operation, AuthOperation::Login)
    )));

    assert_rejected(&["venmo", "auth", "login", "--token"]);
    assert_rejected(&["venmo", "auth", "login", "--token", "secret"]);
    assert_rejected(&["venmo", "auth", "login", "--password", "secret"]);
    assert_rejected(&["venmo", "auth", "login", "--device-id", "secret"]);
    assert_rejected(&["venmo", "auth", "login", "--v-id", "secret"]);
    assert_rejected(&["venmo", "auth", "login", "alice@example.com"]);
}

#[test]
fn removed_auth_surfaces_are_rejected() {
    assert_rejected(&["venmo", "auth", "reauthenticate"]);
    assert_rejected(&["venmo", "auth", "logout", "--revoke"]);
}

#[test]
fn direct_request_creation_dispatches_to_create() {
    let parsed = Cli::try_parse_from(["venmo", "requests", "create", "@alice", "12.50", "Dinner"]);
    let dispatches_to_create = parsed.is_ok_and(|cli| {
        matches!(
            cli.command,
            Command::Requests(args)
                if matches!(args.operation, RequestsOperation::Create(_))
        )
    });
    assert!(dispatches_to_create);

    let authorized = Cli::try_parse_from([
        "venmo", "requests", "create", "@alice", "12.50", "Dinner", "--yes",
    ]);
    assert!(authorized.is_ok_and(|cli| matches!(
        cli.command,
        Command::Requests(args)
            if matches!(&args.operation, RequestsOperation::Create(args) if args.yes)
    )));
}

#[test]
fn outgoing_request_cancellation_parses_only_an_id_and_confirmation_override() {
    let parsed = Cli::try_parse_from(["venmo", "requests", "cancel", "request-123", "--yes"]);
    assert!(parsed.is_ok_and(|cli| matches!(
        cli.command,
        Command::Requests(args)
            if matches!(&args.operation, RequestsOperation::Cancel(args)
                if args.request_id.as_str() == "request-123" && args.yes)
    )));
    for arguments in [
        &["venmo", "requests", "cancel"][..],
        &["venmo", "requests", "cancel", "request id"][..],
        &[
            "venmo",
            "requests",
            "cancel",
            "request-123",
            "--source",
            "bank-1",
        ][..],
    ] {
        assert_rejected(arguments);
    }
}

#[test]
fn required_payment_and_request_notes_are_positional() {
    for prefix in [
        &["venmo", "pay", "user"][..],
        &["venmo", "requests", "create"][..],
    ] {
        let mut missing = prefix.to_vec();
        missing.extend(["@alice", "1.00"]);
        assert_rejected(&missing);

        let mut former_flag = prefix.to_vec();
        former_flag.extend(["@alice", "1.00", "--note", "Dinner"]);
        assert_rejected(&former_flag);
    }
}

#[test]
fn user_and_request_info_parse_exact_typed_inputs() {
    for (input, expected) in [
        ("alice", "@alice"),
        ("@alice", "@alice"),
        ("123456", "@123456"),
        ("@123456", "@123456"),
    ] {
        let user = Cli::try_parse_from(["venmo", "users", "info", input]);
        let parsed = match user {
            Ok(cli) => match cli.command {
                Command::Users(args) => match args.operation {
                    UsersOperation::Info(args) => Some(args.username.to_string()),
                    UsersOperation::Search(_) => None,
                },
                _ => None,
            },
            Err(_) => None,
        };
        assert_eq!(parsed.as_deref(), Some(expected), "input: {input}");
    }

    let request = Cli::try_parse_from(["venmo", "requests", "info", "request-123"]);
    let request_id = match request {
        Ok(cli) => match cli.command {
            Command::Requests(args) => match args.operation {
                RequestsOperation::Info(args) => Some(args.request_id.to_string()),
                RequestsOperation::List(_)
                | RequestsOperation::Create(_)
                | RequestsOperation::Accept(_)
                | RequestsOperation::Decline(_)
                | RequestsOperation::Cancel(_) => None,
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
fn pay_and_request_visibility_defaults_and_explicit_values_are_typed() {
    for prefix in [
        &["venmo", "pay", "user"][..],
        &["venmo", "requests", "create"][..],
    ] {
        let mut arguments = prefix.to_vec();
        arguments.extend(["@alice", "0.01", "Synthetic note"]);
        let defaults = Cli::try_parse_from(arguments);
        let default_visibility = defaults.ok().and_then(request_visibility);
        assert_eq!(default_visibility, Some(VisibilityArg::Private));

        for (value, expected) in [
            ("private", VisibilityArg::Private),
            ("friends", VisibilityArg::Friends),
            ("public", VisibilityArg::Public),
        ] {
            let mut arguments = prefix.to_vec();
            arguments.extend(["@alice", "0.01", "Synthetic note", "--visibility", value]);
            let visibility = Cli::try_parse_from(arguments)
                .ok()
                .and_then(request_visibility);
            assert_eq!(visibility, Some(expected));
        }
    }
}

#[test]
fn invalid_or_unrelated_visibility_is_rejected_by_clap() {
    for prefix in [
        &["venmo", "pay", "user"][..],
        &["venmo", "requests", "create"][..],
    ] {
        let mut arguments = prefix.to_vec();
        arguments.extend([
            "@alice",
            "0.01",
            "Synthetic note",
            "--visibility",
            "contacts",
        ]);
        let parsed = Cli::try_parse_from(arguments);
        assert_eq!(
            parsed.as_ref().err().map(clap::Error::kind),
            Some(ErrorKind::InvalidValue)
        );
        if let Err(error) = parsed {
            let rendered = error.to_string();
            for value in ["private", "friends", "public"] {
                assert!(rendered.contains(value));
            }
        }
    }

    for arguments in [
        &[
            "venmo",
            "requests",
            "accept",
            "request-1",
            "--visibility",
            "public",
        ][..],
        &[
            "venmo",
            "requests",
            "decline",
            "request-1",
            "--visibility",
            "public",
        ][..],
        &["venmo", "activity", "list", "--visibility", "public"][..],
    ] {
        assert_rejected(arguments);
    }
}

#[test]
fn pay_rejects_removed_from_option() {
    assert_rejected(&[
        "venmo", "pay", "user", "@alice", "12.50", "Dinner", "--from", "method-1",
    ]);
}

#[test]
fn pay_options_and_user_have_exact_grouped_grammar() {
    let options = Cli::try_parse_from(["venmo", "pay", "options"]);
    assert!(options.is_ok_and(|cli| matches!(
        cli.command,
        Command::Pay(args) if matches!(args.operation, PayOperation::Options)
    )));

    let user = Cli::try_parse_from([
        "venmo", "pay", "user", "@alice", "12.34", "Dinner", "--source", "bank-1", "--yes",
    ]);
    assert!(user.is_ok_and(|cli| matches!(
        cli.command,
        Command::Pay(args)
            if matches!(
                &args.operation,
                PayOperation::User(user)
                    if user.amount.cents() == 1_234
                        && user.note.as_str() == "Dinner"
                        && user.source.as_ref().is_some_and(|id| id.as_str() == "bank-1")
                        && user.yes
            )
    )));

    for arguments in [
        &["venmo", "payment-methods", "list"][..],
        &["venmo", "pay", "@alice", "1.00", "Dinner"][..],
        &["venmo", "pay", "methods"][..],
        &["venmo", "pay", "options", "list"][..],
    ] {
        assert_rejected(arguments);
    }
}

#[test]
fn accept_username_is_not_the_accept_subcommand() {
    let parsed = Cli::try_parse_from(["venmo", "requests", "create", "@accept", "0.01", "Test"]);
    let dispatches_to_create = parsed.is_ok_and(|cli| {
        matches!(
            cli.command,
            Command::Requests(args)
                if matches!(args.operation, RequestsOperation::Create(_))
        )
    });
    assert!(dispatches_to_create);
}

#[test]
fn grouped_accept_and_decline_have_distinct_minimal_arguments() {
    let accept = Cli::try_parse_from([
        "venmo",
        "requests",
        "accept",
        "request-123",
        "--source",
        "bank-1",
        "--protect",
        "--yes",
    ]);
    assert!(accept.is_ok_and(|cli| matches!(
        cli.command,
        Command::Requests(args)
            if matches!(&args.operation, RequestsOperation::Accept(args)
                if args.yes
                    && args.protect
                    && args.source.as_ref().is_some_and(|id| id.as_str() == "bank-1"))
    )));

    let unprotected = Cli::try_parse_from(["venmo", "requests", "accept", "request-123"]);
    assert!(unprotected.is_ok_and(|cli| matches!(
        cli.command,
        Command::Requests(args)
            if matches!(&args.operation, RequestsOperation::Accept(args)
                if !args.yes && !args.protect && args.source.is_none())
    )));

    let decline = Cli::try_parse_from(["venmo", "requests", "decline", "request-123"]);
    assert!(decline.is_ok_and(|cli| matches!(
        cli.command,
        Command::Requests(args)
            if matches!(&args.operation, RequestsOperation::Decline(args) if !args.yes)
    )));

    let decline_yes = Cli::try_parse_from(["venmo", "requests", "decline", "request-123", "--yes"]);
    assert!(decline_yes.is_ok_and(|cli| matches!(
        cli.command,
        Command::Requests(args)
            if matches!(&args.operation, RequestsOperation::Decline(args) if args.yes)
    )));

    assert_rejected(&[
        "venmo",
        "requests",
        "accept",
        "request-123",
        "--from",
        "method-456",
    ]);
    assert_rejected(&["venmo", "requests", "decline", "request-123", "--protect"]);
    assert_rejected(&[
        "venmo",
        "requests",
        "decline",
        "request-123",
        "--source",
        "bank-1",
    ]);
    assert_rejected(&[
        "venmo",
        "requests",
        "create",
        "alice",
        "1.00",
        "Dinner",
        "--protect",
    ]);
    assert_rejected(&[
        "venmo", "requests", "create", "alice", "1.00", "Dinner", "--source", "bank-1",
    ]);
}

#[test]
fn optional_at_usernames_and_global_option_placement_are_supported() {
    for username in ["alice", "@alice"] {
        let parsed = Cli::try_parse_from([
            "venmo", "requests", "--debug", "create", username, "0.01", "Test",
        ]);
        let dispatches_to_create = match parsed {
            Ok(cli) => {
                cli.debug
                    && matches!(
                        cli.command,
                        Command::Requests(args)
                            if matches!(args.operation, RequestsOperation::Create(_))
                    )
            }
            Err(_) => false,
        };
        assert!(dispatches_to_create, "username: {username}");
    }

    let accept = Cli::try_parse_from(["venmo", "--debug", "requests", "accept", "request-123"]);
    assert!(accept.is_ok_and(|cli| cli.debug));
}

#[test]
fn removed_verbose_flags_are_rejected() {
    assert_rejected(&["venmo", "--verbose", "balance"]);
    assert_rejected(&["venmo", "-v", "balance"]);
}

#[test]
fn request_and_request_mutation_forms_cannot_be_mixed() {
    assert_rejected(&[
        "venmo",
        "requests",
        "accept",
        "request-123",
        "12.50",
        "Dinner",
    ]);
    assert_rejected(&[
        "venmo", "requests", "create", "@alice", "12.50", "Dinner", "--from", "method-1",
    ]);
}

#[test]
fn removed_and_deferred_forms_are_rejected() {
    for arguments in [
        &["venmo", "init"][..],
        &["venmo", "deinit"][..],
        &["venmo", "doctor"][..],
        &["venmo", "completions", "bash"][..],
        &["venmo", "charge", "@alice", "1.00", "note"][..],
        &["venmo", "request", "create", "@alice", "1.00", "note"][..],
        &["venmo", "request", "@alice", "1.00", "note"][..],
        &["venmo", "accept", "request-1"][..],
        &["venmo", "decline", "request-1"][..],
        &["venmo", "payment-methods", "list"][..],
        &["venmo", "pay", "@alice", "1.00", "note"][..],
        &[
            "venmo",
            "pay",
            "user",
            "@alice",
            "1.00",
            "note",
            "--dry-run",
        ][..],
        &[
            "venmo",
            "requests",
            "create",
            "@alice",
            "1.00",
            "note",
            "--dry-run",
        ][..],
        &["venmo", "activity", "show", "activity-1"][..],
    ] {
        assert_rejected(arguments);
    }
}

#[test]
fn request_direction_is_a_typed_enum() {
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
                RequestsOperation::Create(_)
                | RequestsOperation::Accept(_)
                | RequestsOperation::Decline(_)
                | RequestsOperation::Cancel(_)
                | RequestsOperation::Info(_) => None,
            },
            _ => None,
        },
        Err(_) => None,
    };
    assert_eq!(request_values, Some((RequestDirectionArg::Incoming, 50)));
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
        &["venmo", "pay", "methods", "--offset", "12"][..],
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
fn activity_list_user_is_optional_and_normalizes_at_prefix() {
    let current = Cli::try_parse_from(["venmo", "activity", "list"]);
    assert!(current.is_ok_and(|cli| matches!(
        cli.command,
        Command::Activity(args)
            if matches!(&args.operation, ActivityOperation::List(args) if args.user.is_none())
    )));

    for username in ["alice", "@alice"] {
        let parsed = Cli::try_parse_from(["venmo", "activity", "list", "--user", username]);
        assert!(parsed.is_ok_and(|cli| matches!(
            cli.command,
            Command::Activity(args)
                if matches!(&args.operation, ActivityOperation::List(args)
                    if args.user.as_ref().is_some_and(|value| value.as_str() == "alice"))
        )));
    }

    assert_rejected(&["venmo", "activity", "info", "story-1", "--user", "alice"]);
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
            &["venmo", "requests", "accept", "sensitive request id"][..],
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
                venmo_cli::cli::FriendsOperation::Add(_)
                | venmo_cli::cli::FriendsOperation::Remove(_) => None,
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
fn friend_mutation_commands_have_exact_grouped_grammar() {
    let add = Cli::try_parse_from(["venmo", "friends", "add", "@alice", "--yes"]);
    assert!(add.is_ok_and(|cli| matches!(
        cli.command,
        Command::Friends(args)
            if matches!(&args.operation, venmo_cli::cli::FriendsOperation::Add(args)
                if args.username.as_str() == "alice" && args.yes)
    )));

    let remove = Cli::try_parse_from(["venmo", "friends", "remove", "alice"]);
    assert!(remove.is_ok_and(|cli| matches!(
        cli.command,
        Command::Friends(args)
            if matches!(&args.operation, venmo_cli::cli::FriendsOperation::Remove(args)
                if args.username.as_str() == "alice" && !args.yes)
    )));

    for arguments in [
        &["venmo", "friend", "add", "alice"][..],
        &["venmo", "friends", "add"][..],
        &["venmo", "friends", "remove"][..],
        &["venmo", "friends", "add", "alice", "--limit", "1"][..],
        &["venmo", "friends", "list", "--yes"][..],
    ] {
        assert_rejected(arguments);
    }
}

#[test]
fn argument_only_validation_errors_are_clap_errors() {
    assert_rejected(&["venmo", "pay", "user", "@alice", "0", "Dinner"]);
    assert_rejected(&["venmo", "pay", "user", "@alice", "1.001", "Dinner"]);
    assert_rejected(&["venmo", "pay", "user", "@alice", "1.00", "   "]);
    assert_rejected(&["venmo", "requests", "create", "@alice", "1.00", "   "]);
    assert_rejected(&["venmo", "requests", "accept"]);
    assert_rejected(&["venmo", "users", "search", "   "]);
    assert_rejected(&["venmo", "users", "search", "@"]);
}

#[test]
fn transfer_options_and_guarded_standard_transfers_have_exact_grammar() {
    let options = Cli::try_parse_from(["venmo", "transfer", "options"]);
    assert!(options.is_ok_and(|cli| matches!(
        cli.command,
        Command::Transfer(args) if matches!(args.operation, TransferOperation::Options)
    )));

    for (arguments, expected_yes) in [
        (&["venmo", "transfer", "out", "12.34"][..], false),
        (
            &[
                "venmo", "transfer", "out", "12.34", "--speed", "standard", "--yes",
            ][..],
            true,
        ),
    ] {
        let out = Cli::try_parse_from(arguments);
        assert!(out.is_ok_and(|cli| matches!(
            cli.command,
            Command::Transfer(args)
                if matches!(
                    &args.operation,
                    TransferOperation::Out(out)
                        if matches!(
                            out.amount,
                            TransferAmountArg::Exact(amount) if amount.cents() == 1_234
                        )
                            && out.speed == TransferSpeedArg::Standard
                            && out.yes == expected_yes
                )
        )));
    }

    let all = Cli::try_parse_from(["venmo", "transfer", "out", "all", "--yes"]);
    assert!(all.is_ok_and(|cli| matches!(
        cli.command,
        Command::Transfer(args)
            if matches!(
                &args.operation,
                TransferOperation::Out(out)
                    if out.amount == TransferAmountArg::All
                        && out.speed == TransferSpeedArg::Standard
                        && out.yes
            )
    )));

    for arguments in [
        &["venmo", "transfer", "out", "ALL"][..],
        &["venmo", "transfer", "out", "all.00"][..],
        &["venmo", "transfer", "out", "1.00", "--speed", "instant"][..],
        &[
            "venmo",
            "transfer",
            "out",
            "1.00",
            "--speed",
            "standard",
            "--destination",
            "bank-1",
        ][..],
        &["venmo", "transfer", "in", "1.00"][..],
        &["venmo", "transfer", "options", "--yes"][..],
    ] {
        assert_rejected(arguments);
    }
}

#[test]
fn every_command_has_a_help_snapshot() {
    let cases: &[(&str, &[&str])] = &[
        ("top_level", &[]),
        ("auth", &["auth"]),
        ("auth_login", &["auth", "login"]),
        ("auth_logout", &["auth", "logout"]),
        ("auth_status", &["auth", "status"]),
        ("pay", &["pay"]),
        ("pay_options", &["pay", "options"]),
        ("pay_user", &["pay", "user"]),
        ("friends", &["friends"]),
        ("friends_list", &["friends", "list"]),
        ("friends_add", &["friends", "add"]),
        ("friends_remove", &["friends", "remove"]),
        ("users", &["users"]),
        ("users_search", &["users", "search"]),
        ("users_info", &["users", "info"]),
        ("balance", &["balance"]),
        ("activity", &["activity"]),
        ("activity_list", &["activity", "list"]),
        ("activity_info", &["activity", "info"]),
        ("requests", &["requests"]),
        ("requests_list", &["requests", "list"]),
        ("requests_create", &["requests", "create"]),
        ("requests_accept", &["requests", "accept"]),
        ("requests_decline", &["requests", "decline"]),
        ("requests_cancel", &["requests", "cancel"]),
        ("requests_info", &["requests", "info"]),
        ("transfer", &["transfer"]),
        ("transfer_options", &["transfer", "options"]),
        ("transfer_out", &["transfer", "out"]),
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

fn request_visibility(cli: Cli) -> Option<VisibilityArg> {
    match cli.command {
        Command::Pay(args) => match args.operation {
            PayOperation::User(args) => Some(args.visibility),
            PayOperation::Options => None,
        },
        Command::Requests(args) => match args.operation {
            RequestsOperation::Create(args) => Some(args.visibility),
            RequestsOperation::List(_)
            | RequestsOperation::Accept(_)
            | RequestsOperation::Decline(_)
            | RequestsOperation::Cancel(_)
            | RequestsOperation::Info(_) => None,
        },
        _ => None,
    }
}

use super::*;

#[tokio::test(flavor = "current_thread")]
async fn users_handler_routes_exact_page_and_continuation_streams_without_flush() -> TestResult {
    // Setup.
    let args = users_args()?;
    let limit = Limit::try_from(1)?;
    let offset = Offset::new(10);
    let query = UserSearchQuery::from_str("alice")?;
    let response = UserSearchPage::new(
        vec![User::new(
            UserId::from_str("123")?,
            Some(Username::from_bare("alice")?),
            Some("Alice\u{1b}[31m".to_owned()),
        )],
        Some(Offset::new(11)),
    );

    // Immutable initial script/state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::standard(Rc::clone(&transcript));
    let api = UserSearchFake {
        responses: ResponseQueue::successful(response),
        transcript: Rc::clone(&transcript),
    };
    let mut stdout = writer(Stream::Stdout, Rc::clone(&transcript));
    let mut stderr = writer(Stream::Stderr, Rc::clone(&transcript));
    let expected = Observed::new(
        ResultSnapshot::Success,
        ReadState {
            calls: vec![
                ReadCall::ReadCredential,
                ReadCall::SearchUsers {
                    session: fixture_session(),
                    query,
                    page: UserSearchPageRequest::new(limit, offset),
                },
                ReadCall::StdoutWrite,
                ReadCall::StderrWrite,
            ],
            remaining_credentials: vec![ResponseId::UnexpectedSecond],
            api: vec![ResponseId::UnexpectedSecond],
            stdout: writer_state(USERS_OUTPUT),
            stderr: writer_state("Next offset: 11\n"),
        },
    );

    // Execute once.
    let result = run_user_search(args, &reader, &api, &mut stdout, &mut stderr).await;
    let observed = observation(
        result,
        &transcript,
        &reader,
        api.responses.remaining(),
        stdout.state,
        stderr.state,
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn user_info_handler_uses_exact_username_resolution_and_writes_sanitized_stdout() -> TestResult
{
    // Setup.
    let args = user_info_args()?;
    let response = User::new(
        UserId::from_str("123")?,
        Some(Username::from_bare("alice")?),
        Some("Alice\nExample".to_owned()),
    )
    .with_financial_attributes(UserProfileKind::Personal, true)
    .with_friendship_status(FriendshipStatus::RequestSent);

    // Immutable initial script/state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::standard(Rc::clone(&transcript));
    let api = UserInfoFake {
        responses: ResponseQueue::successful(response),
        search_responses: Some(ResponseQueue::successful(UserSearchPage::new(
            vec![User::new(
                UserId::from_str("123")?,
                Some(Username::from_bare("alice")?),
                Some("Alice".to_owned()),
            )],
            None,
        ))),
        transcript: Rc::clone(&transcript),
    };
    let mut stdout = writer(Stream::Stdout, Rc::clone(&transcript));
    let stderr = writer(Stream::Stderr, Rc::clone(&transcript));

    // Complete expected outcome and final fake state.
    let expected = Observed::new(
        ResultSnapshot::Success,
        ReadState {
            calls: vec![
                ReadCall::ReadCredential,
                ReadCall::SearchUsers {
                    session: fixture_session(),
                    query: UserSearchQuery::from_str("alice")?,
                    page: UserSearchPageRequest::new(Limit::try_from(50)?, Offset::default()),
                },
                ReadCall::UserInfo {
                    session: fixture_session(),
                    user_id: UserId::from_str("123")?,
                },
                ReadCall::StdoutWrite,
            ],
            remaining_credentials: vec![ResponseId::UnexpectedSecond],
            api: vec![ResponseId::UnexpectedSecond],
            stdout: writer_state(USER_INFO_OUTPUT),
            stderr: WriterState::default(),
        },
    );

    // Execute once.
    let result = run_user_info(args, &reader, &api, &mut stdout).await;
    let observed = observation(
        result,
        &transcript,
        &reader,
        api.responses.remaining(),
        stdout.state,
        stderr.state,
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn user_info_normalizes_optional_at_and_uses_shared_authoritative_lookup() -> TestResult {
    for input in ["alice", "@alice"] {
        let args = match Cli::try_parse_from(["venmo", "users", "info", input])?.command {
            Command::Users(args) => match args.operation {
                UsersOperation::Info(args) => args,
                UsersOperation::Search(_) => {
                    return Err(io::Error::other("info parsed as search").into());
                }
            },
            _ => return Err(io::Error::other("info parsed as another command").into()),
        };
        let detail = User::new(
            UserId::from_str("123")?,
            Some(Username::from_bare("alice")?),
            Some("Alice\nExample".to_owned()),
        )
        .with_financial_attributes(UserProfileKind::Personal, true)
        .with_friendship_status(FriendshipStatus::RequestSent);
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let reader = FakeReader::standard(Rc::clone(&transcript));
        let api = UserInfoFake {
            responses: ResponseQueue::successful(detail),
            search_responses: Some(ResponseQueue::successful(UserSearchPage::new(
                vec![User::new(
                    UserId::from_str("123")?,
                    Some(Username::from_bare("alice")?),
                    Some("Alice".to_owned()),
                )],
                None,
            ))),
            transcript: Rc::clone(&transcript),
        };
        let mut stdout = writer(Stream::Stdout, Rc::clone(&transcript));

        let result = run_user_info(args, &reader, &api, &mut stdout).await;

        assert!(result.is_ok(), "input: {input}");
        assert_eq!(
            transcript.borrow().as_slice(),
            [
                ReadCall::ReadCredential,
                ReadCall::SearchUsers {
                    session: fixture_session(),
                    query: UserSearchQuery::from_str("alice")?,
                    page: UserSearchPageRequest::new(Limit::try_from(50)?, Offset::default()),
                },
                ReadCall::UserInfo {
                    session: fixture_session(),
                    user_id: UserId::from_str("123")?,
                },
                ReadCall::StdoutWrite,
            ]
        );
        assert_eq!(stdout.state, writer_state(USER_INFO_OUTPUT));
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn friends_handler_routes_exact_page_and_continuation_streams_without_flush() -> TestResult {
    // Setup.
    let args = friends_args()?;
    let limit = Limit::try_from(1)?;
    let offset = Offset::new(20);
    let response = FriendsPage::new(
        vec![User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("bob")?),
            Some("Bob\nName".to_owned()),
        )],
        Some(Offset::new(21)),
    );

    // Immutable initial script/state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::standard(Rc::clone(&transcript));
    let api = FriendsFake {
        responses: ResponseQueue::successful(response),
        search_responses: None,
        detail_responses: None,
        transcript: Rc::clone(&transcript),
    };
    let mut stdout = writer(Stream::Stdout, Rc::clone(&transcript));
    let mut stderr = writer(Stream::Stderr, Rc::clone(&transcript));
    let expected = Observed::new(
        ResultSnapshot::Success,
        ReadState {
            calls: vec![
                ReadCall::ReadCredential,
                ReadCall::Friends {
                    session: fixture_session(),
                    subject_user_id: UserId::from_str("1000")?,
                    page: FriendsPageRequest::new(limit, offset),
                },
                ReadCall::StdoutWrite,
                ReadCall::StderrWrite,
            ],
            remaining_credentials: vec![ResponseId::UnexpectedSecond],
            api: vec![ResponseId::UnexpectedSecond],
            stdout: writer_state(FRIENDS_OUTPUT),
            stderr: writer_state("Next offset: 21\n"),
        },
    );

    // Execute once.
    let result = run_friends_list(args, &reader, &api, &mut stdout, &mut stderr).await;
    let observed = observation(
        result,
        &transcript,
        &reader,
        api.responses.remaining(),
        stdout.state,
        stderr.state,
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn friends_handler_resolves_other_user_and_labels_visible_results() -> TestResult {
    let args = match Cli::try_parse_from([
        "venmo", "friends", "list", "--user", "@alice", "--limit", "1", "--offset", "20",
    ])?
    .command
    {
        Command::Friends(args) => match args.operation {
            FriendsOperation::List(args) => args,
            FriendsOperation::Add(_) | FriendsOperation::Remove(_) => {
                return Err(io::Error::other("list parsed as a mutation").into());
            }
        },
        _ => return Err(io::Error::other("friends parsed as another command").into()),
    };
    let limit = Limit::try_from(1)?;
    let offset = Offset::new(20);
    let search_user = User::new(
        UserId::from_str("2000")?,
        Some(Username::from_bare("alice")?),
        Some("Alice".to_owned()),
    );
    let detail_user = search_user
        .clone()
        .with_financial_attributes(UserProfileKind::Personal, true);
    let response = FriendsPage::new(
        vec![User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("bob")?),
            Some("Bob\nName".to_owned()),
        )],
        Some(Offset::new(21)),
    );
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::standard(Rc::clone(&transcript));
    let api = FriendsFake {
        responses: ResponseQueue::successful(response),
        search_responses: Some(ResponseQueue::successful(UserSearchPage::new(
            vec![search_user],
            None,
        ))),
        detail_responses: Some(ResponseQueue::successful(detail_user)),
        transcript: Rc::clone(&transcript),
    };
    let mut stdout = writer(Stream::Stdout, Rc::clone(&transcript));
    let mut stderr = writer(Stream::Stderr, Rc::clone(&transcript));

    let result = run_friends_list(args, &reader, &api, &mut stdout, &mut stderr).await;

    assert!(result.is_ok());
    assert_eq!(
        transcript.borrow().as_slice(),
        [
            ReadCall::ReadCredential,
            ReadCall::SearchUsers {
                session: fixture_session(),
                query: UserSearchQuery::from_str("alice")?,
                page: UserSearchPageRequest::new(Limit::try_from(50)?, Offset::default()),
            },
            ReadCall::UserInfo {
                session: fixture_session(),
                user_id: UserId::from_str("2000")?,
            },
            ReadCall::Friends {
                session: fixture_session(),
                subject_user_id: UserId::from_str("2000")?,
                page: FriendsPageRequest::new(limit, offset),
            },
            ReadCall::StdoutWrite,
            ReadCall::StderrWrite,
        ]
    );
    assert_eq!(
        stdout.state,
        writer_state(&format!("Friends for @alice\n{FRIENDS_OUTPUT}"))
    );
    assert_eq!(stderr.state, writer_state("Next offset: 21\n"));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn representative_record_output_failure_stops_continuation_without_an_extra_api_call()
-> TestResult {
    // Setup.
    let args = friends_args()?;
    let limit = Limit::try_from(1)?;
    let offset = Offset::new(20);
    let response = FriendsPage::new(
        vec![User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("bob")?),
            Some("Bob".to_owned()),
        )],
        Some(Offset::new(21)),
    );

    // Immutable initial script/state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::standard(Rc::clone(&transcript));
    let api = FriendsFake {
        responses: ResponseQueue::successful(response),
        search_responses: None,
        detail_responses: None,
        transcript: Rc::clone(&transcript),
    };
    let mut stdout = RecordingWriter::new(
        Stream::Stdout,
        WriterState {
            fail_write: true,
            ..WriterState::default()
        },
        Rc::clone(&transcript),
    );
    let mut stderr = writer(Stream::Stderr, Rc::clone(&transcript));
    let expected = Observed::new(
        failure_snapshot(),
        ReadState {
            calls: vec![
                ReadCall::ReadCredential,
                ReadCall::Friends {
                    session: fixture_session(),
                    subject_user_id: UserId::from_str("1000")?,
                    page: FriendsPageRequest::new(limit, offset),
                },
                ReadCall::StdoutWrite,
            ],
            remaining_credentials: vec![ResponseId::UnexpectedSecond],
            api: vec![ResponseId::UnexpectedSecond],
            stdout: WriterState {
                fail_write: true,
                ..WriterState::default()
            },
            stderr: WriterState::default(),
        },
    );

    // Execute once.
    let result = run_friends_list(args, &reader, &api, &mut stdout, &mut stderr).await;
    let observed = observation(
        result,
        &transcript,
        &reader,
        api.responses.remaining(),
        stdout.state,
        stderr.state,
    );

    assert_eq!(observed, expected);
    Ok(())
}

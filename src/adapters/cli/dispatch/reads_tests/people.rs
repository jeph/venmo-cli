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
    let api = UsersFake {
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
    let result = run_users(args, &reader, &api, &mut stdout, &mut stderr).await;
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
                    current_user_id: UserId::from_str("1000")?,
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
    let result = run_friends(args, &reader, &api, &mut stdout, &mut stderr).await;
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
                    current_user_id: UserId::from_str("1000")?,
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
    let result = run_friends(args, &reader, &api, &mut stdout, &mut stderr).await;
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

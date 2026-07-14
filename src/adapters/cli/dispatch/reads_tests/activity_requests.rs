use super::*;

#[tokio::test(flavor = "current_thread")]
async fn activity_list_handler_has_exact_page_output_and_continuation_streams() -> TestResult {
    // Setup.
    let args = activity_list_args()?;
    let limit = Limit::try_from(2)?;
    let current = ActivityBeforeId::from_str("story-current")?;
    let primary = synthetic_activity()?;
    let response = ActivityPage::new(
        vec![primary.clone(), synthetic_transfer()?],
        Some(ActivityBeforeId::from_str("story-next")?),
    );

    // Immutable initial script/state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::standard(Rc::clone(&transcript));
    let api = ActivityFake {
        list_responses: ResponseQueue::successful(response),
        show_responses: ResponseQueue::successful(primary),
        transcript: Rc::clone(&transcript),
    };
    let mut stdout = writer(Stream::Stdout, Rc::clone(&transcript));
    let mut stderr = writer(Stream::Stderr, Rc::clone(&transcript));
    let expected = Observed::new(
        ResultSnapshot::Success,
        ReadState {
            calls: vec![
                ReadCall::ReadCredential,
                ReadCall::ActivityList {
                    session: fixture_session(),
                    current_user_id: UserId::from_str("1000")?,
                    page: ActivityPageRequest::new(limit, Some(current)),
                },
                ReadCall::StdoutWrite,
                ReadCall::StderrWrite,
            ],
            remaining_credentials: vec![ResponseId::UnexpectedSecond],
            api: ActivityApiState {
                list: vec![ResponseId::UnexpectedSecond],
                show: vec![ResponseId::Primary, ResponseId::UnexpectedSecond],
            },
            stdout: writer_state(ACTIVITY_LIST_OUTPUT),
            stderr: writer_state("Next before-id: story-next\n"),
        },
    );

    // Execute once.
    let result = run_activity(args, &reader, &api, &mut stdout, &mut stderr).await;
    let observed = Observed::new(
        snapshot_result(result),
        ReadState {
            calls: transcript.borrow().clone(),
            remaining_credentials: reader.remaining(),
            api: ActivityApiState {
                list: api.list_responses.remaining(),
                show: api.show_responses.remaining(),
            },
            stdout: stdout.state,
            stderr: stderr.state,
        },
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn activity_show_handler_uses_only_detail_and_writes_exact_stdout() -> TestResult {
    // Setup.
    let args = activity_show_args()?;
    let primary = synthetic_activity()?;

    // Immutable initial script/state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::standard(Rc::clone(&transcript));
    let api = ActivityFake {
        list_responses: ResponseQueue::successful(ActivityPage::new(Vec::new(), None)),
        show_responses: ResponseQueue::successful(primary),
        transcript: Rc::clone(&transcript),
    };
    let mut stdout = writer(Stream::Stdout, Rc::clone(&transcript));
    let mut stderr = writer(Stream::Stderr, Rc::clone(&transcript));
    let expected = Observed::new(
        ResultSnapshot::Success,
        ReadState {
            calls: vec![
                ReadCall::ReadCredential,
                ReadCall::ActivityShow {
                    session: fixture_session(),
                    current_user_id: UserId::from_str("1000")?,
                    activity_id: ActivityId::from_str("story-1")?,
                },
                ReadCall::StdoutWrite,
            ],
            remaining_credentials: vec![ResponseId::UnexpectedSecond],
            api: ActivityApiState {
                list: vec![ResponseId::Primary, ResponseId::UnexpectedSecond],
                show: vec![ResponseId::UnexpectedSecond],
            },
            stdout: writer_state(ACTIVITY_SHOW_OUTPUT),
            stderr: WriterState::default(),
        },
    );

    // Execute once.
    let result = run_activity(args, &reader, &api, &mut stdout, &mut stderr).await;
    let observed = Observed::new(
        snapshot_result(result),
        ReadState {
            calls: transcript.borrow().clone(),
            remaining_credentials: reader.remaining(),
            api: ActivityApiState {
                list: api.list_responses.remaining(),
                show: api.show_responses.remaining(),
            },
            stdout: stdout.state,
            stderr: stderr.state,
        },
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn requests_handler_has_exact_filter_page_output_and_continuation_streams() -> TestResult {
    // Setup.
    let args = requests_args()?;
    let limit = Limit::try_from(1)?;
    let current = RequestsBefore::from_str("request-current")?;
    let response = PendingRequestsPage::new(
        vec![synthetic_request()?],
        Some(RequestsBefore::from_str("request-next")?),
    );

    // Immutable initial script/state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::standard(Rc::clone(&transcript));
    let api = RequestsFake {
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
                ReadCall::PendingRequests {
                    session: fixture_session(),
                    current_user_id: UserId::from_str("1000")?,
                    page: PendingRequestsPageRequest::new(limit, Some(current)),
                },
                ReadCall::StdoutWrite,
                ReadCall::StderrWrite,
            ],
            remaining_credentials: vec![ResponseId::UnexpectedSecond],
            api: vec![ResponseId::UnexpectedSecond],
            stdout: writer_state(REQUESTS_OUTPUT),
            stderr: writer_state("Next before: request-next\n"),
        },
    );

    // Execute once.
    let result = run_requests(args, &reader, &api, &mut stdout, &mut stderr).await;
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

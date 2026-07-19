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
        info_responses: ResponseQueue::successful(primary),
        transcript: Rc::clone(&transcript),
    };
    let mut stdout = writer(Stream::Stdout, Rc::clone(&transcript));
    let mut stderr = writer(Stream::Stderr, Rc::clone(&transcript));
    let timestamps = timestamps();
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
                info: vec![ResponseId::Primary, ResponseId::UnexpectedSecond],
            },
            stdout: writer_state(ACTIVITY_LIST_OUTPUT),
            stderr: writer_state("Next before-id: story-next\n"),
        },
    );

    // Execute once.
    let result = run_activity(args, &reader, &api, &timestamps, &mut stdout, &mut stderr).await;
    let observed = Observed::new(
        snapshot_result(result),
        ReadState {
            calls: transcript.borrow().clone(),
            remaining_credentials: reader.remaining(),
            api: ActivityApiState {
                list: api.list_responses.remaining(),
                info: api.info_responses.remaining(),
            },
            stdout: stdout.state,
            stderr: stderr.state,
        },
    );

    assert_eq!(observed, expected);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn activity_info_handler_uses_only_detail_and_writes_exact_stdout() -> TestResult {
    // Setup.
    let args = activity_info_args()?;
    let primary = synthetic_activity()?;

    // Immutable initial script/state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::standard(Rc::clone(&transcript));
    let api = ActivityFake {
        list_responses: ResponseQueue::successful(ActivityPage::new(Vec::new(), None)),
        info_responses: ResponseQueue::successful(primary),
        transcript: Rc::clone(&transcript),
    };
    let mut stdout = writer(Stream::Stdout, Rc::clone(&transcript));
    let mut stderr = writer(Stream::Stderr, Rc::clone(&transcript));
    let timestamps = timestamps();
    let expected = Observed::new(
        ResultSnapshot::Success,
        ReadState {
            calls: vec![
                ReadCall::ReadCredential,
                ReadCall::ActivityInfo {
                    session: fixture_session(),
                    current_user_id: UserId::from_str("1000")?,
                    activity_id: ActivityId::from_str("story-1")?,
                },
                ReadCall::StdoutWrite,
            ],
            remaining_credentials: vec![ResponseId::UnexpectedSecond],
            api: ActivityApiState {
                list: vec![ResponseId::Primary, ResponseId::UnexpectedSecond],
                info: vec![ResponseId::UnexpectedSecond],
            },
            stdout: writer_state(ACTIVITY_INFO_OUTPUT),
            stderr: WriterState::default(),
        },
    );

    // Execute once.
    let result = run_activity(args, &reader, &api, &timestamps, &mut stdout, &mut stderr).await;
    let observed = Observed::new(
        snapshot_result(result),
        ReadState {
            calls: transcript.borrow().clone(),
            remaining_credentials: reader.remaining(),
            api: ActivityApiState {
                list: api.list_responses.remaining(),
                info: api.info_responses.remaining(),
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
    let api = RequestsListFake {
        responses: ResponseQueue::successful(response),
        transcript: Rc::clone(&transcript),
    };
    let mut stdout = writer(Stream::Stdout, Rc::clone(&transcript));
    let mut stderr = writer(Stream::Stderr, Rc::clone(&transcript));
    let timestamps = timestamps();
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
    let result =
        run_requests_list(args, &reader, &api, &timestamps, &mut stdout, &mut stderr).await;
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
async fn request_info_handler_uses_only_lookup_and_writes_sanitized_stdout() -> TestResult {
    // Setup.
    let args = request_info_args()?;
    let response = synthetic_request()?;

    // Immutable initial script/state.
    let transcript = Rc::new(RefCell::new(Vec::new()));
    let reader = FakeReader::standard(Rc::clone(&transcript));
    let api = RequestInfoFake {
        responses: ResponseQueue::successful(response),
        transcript: Rc::clone(&transcript),
    };
    let mut stdout = writer(Stream::Stdout, Rc::clone(&transcript));
    let stderr = writer(Stream::Stderr, Rc::clone(&transcript));
    let timestamps = timestamps();

    // Complete expected outcome and final fake state.
    let expected = Observed::new(
        ResultSnapshot::Success,
        ReadState {
            calls: vec![
                ReadCall::ReadCredential,
                ReadCall::RequestInfo {
                    session: fixture_session(),
                    current_user_id: UserId::from_str("1000")?,
                    request_id: RequestId::from_str("request-1")?,
                },
                ReadCall::StdoutWrite,
            ],
            remaining_credentials: vec![ResponseId::UnexpectedSecond],
            api: vec![ResponseId::UnexpectedSecond],
            stdout: writer_state(REQUEST_INFO_OUTPUT),
            stderr: WriterState::default(),
        },
    );

    // Execute once.
    let result = run_request_info(args, &reader, &api, &timestamps, &mut stdout).await;
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

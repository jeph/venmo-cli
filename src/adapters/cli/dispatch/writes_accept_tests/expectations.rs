use super::*;

pub(super) fn preparation_calls() -> Vec<AcceptCall> {
    vec![
        AcceptCall::ReadCredential,
        AcceptCall::CurrentAccount,
        AcceptCall::RequestLookup {
            current_user_id: "123".to_owned(),
            request_id: "request-1".to_owned(),
        },
        AcceptCall::UserLookup {
            user_id: "456".to_owned(),
        },
        AcceptCall::Balance,
    ]
}

pub(super) fn preparation_and_authorization_calls() -> Vec<AcceptCall> {
    let mut calls = preparation_calls();
    calls.extend([
        AcceptCall::StderrWrite,
        AcceptCall::StderrFlush,
        AcceptCall::PromptAvailability,
        AcceptCall::ConfirmDefaultNo {
            prompt: "Accept this request and pay its requester?".to_owned(),
        },
    ]);
    calls
}

pub(super) fn successful_calls_without_stdout_flush() -> Vec<AcceptCall> {
    let mut calls = preparation_and_authorization_calls();
    calls.extend([
        AcceptCall::InstallInterruption,
        accept_call(),
        AcceptCall::StdoutWrite,
    ]);
    calls
}

pub(super) fn successful_calls() -> Vec<AcceptCall> {
    let mut calls = successful_calls_without_stdout_flush();
    calls.push(AcceptCall::StdoutFlush);
    calls
}

pub(super) fn accept_call() -> AcceptCall {
    AcceptCall::Accept {
        plan: AcceptPlanCall {
            account_user_id: "123".to_owned(),
            request_id: "request-1".to_owned(),
            requester_user_id: "456".to_owned(),
            amount_cents: 1,
            note: Some("Synthetic request".to_owned()),
            audience: Some("private".to_owned()),
            available_balance_cents: 1,
            funding_source_id: None,
        },
    }
}

pub(super) fn writer_state(text: &str, flush_count: u32) -> WriterState {
    WriterState {
        text: text.to_owned(),
        flush_count,
        ..WriterState::default()
    }
}

pub(super) fn signal_error() -> AppError {
    AppError::SignalInitialization {
        source: io::Error::other("synthetic signal stream failure"),
    }
}

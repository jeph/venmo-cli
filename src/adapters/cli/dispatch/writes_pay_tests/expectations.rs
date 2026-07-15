use super::*;

pub(super) fn preparation_calls() -> Vec<PayCall> {
    vec![
        PayCall::ReadCredential,
        PayCall::CurrentAccount,
        PayCall::UserById {
            user_id: "456".to_owned(),
        },
        PayCall::Balance,
        PayCall::FundingMethods,
        PayCall::Eligibility {
            recipient_user_id: "456".to_owned(),
            amount_cents: 1,
            note: "Synthetic payment".to_owned(),
        },
        PayCall::GenerateClientRequestId,
    ]
}

pub(super) fn preparation_and_authorization_calls() -> Vec<PayCall> {
    let mut calls = preparation_calls();
    calls.extend([
        PayCall::StderrWrite,
        PayCall::StderrFlush,
        PayCall::PromptAvailability,
        PayCall::ConfirmDefaultNo {
            prompt: "Send this payment?".to_owned(),
        },
    ]);
    calls
}

pub(super) fn successful_calls_without_stdout_flush() -> Vec<PayCall> {
    let mut calls = preparation_and_authorization_calls();
    calls.extend([
        PayCall::InstallInterruption,
        create_payment_call(),
        PayCall::StdoutWrite,
    ]);
    calls
}

pub(super) fn successful_calls() -> Vec<PayCall> {
    successful_calls_with_visibility(Visibility::Private)
}

pub(super) fn successful_calls_with_visibility(visibility: Visibility) -> Vec<PayCall> {
    let mut calls = successful_calls_without_stdout_flush();
    if let Some(PayCall::CreatePayment { plan }) = calls
        .iter_mut()
        .find(|call| matches!(call, PayCall::CreatePayment { .. }))
    {
        plan.visibility = visibility;
    }
    calls.push(PayCall::StdoutFlush);
    calls
}

pub(super) fn create_payment_call() -> PayCall {
    PayCall::CreatePayment {
        plan: PayPlanCall {
            request_id: REQUEST_ID.to_owned(),
            account_user_id: "123".to_owned(),
            recipient_user_id: "456".to_owned(),
            amount_cents: 1,
            note: "Synthetic payment".to_owned(),
            backup_method_id: "bank-1".to_owned(),
            eligibility_fee_cents: 0,
            visibility: Visibility::Private,
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

pub(super) fn signal_error(detail: &str) -> AppError {
    AppError::SignalInitialization {
        source: io::Error::other(detail.to_owned()),
    }
}

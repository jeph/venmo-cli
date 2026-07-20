use std::error::Error;
use std::str::FromStr;

use super::super::{
    write_accept_details, write_accept_result, write_decline_details, write_decline_result,
    write_pay_details, write_pay_result, write_request_create_details, write_request_create_result,
};
use crate::features::payments::pay::{PayResult, PreparedPay};
use crate::features::payments::{
    CreatedPayment, EligibilityToken, FinancialStatus, PayPlan, PaymentId, PeerFundingFee,
    PeerFundingMethod, PeerFundingRole,
};
use crate::features::people::{User, UserProfileKind};
use crate::features::requests::accept::{AcceptResult, PreparedAccept};
use crate::features::requests::create::{PreparedRequest, RequestCreateResult};
use crate::features::requests::decline::{DeclineResult, PreparedDecline};
use crate::features::requests::{
    AcceptRequestPlan, AcceptedRequest, CreateRequestPlan, CreatedRequest, DeclineRequestPlan,
    DeclinedRequest, RequestAction, RequestApprovalFee, RequestApprovalFees, RequestDirection,
    RequestId, RequestNotificationId, RequestRecord, RequestStatus,
};
use crate::features::wallet::{Balance, PaymentMethod, PaymentMethodId, SignedUsdAmount};
use crate::shared::{
    AccessToken, Account, ClientRequestId, CredentialEnvelope, DeviceId, Money, Note, UserId,
    Username, Visibility,
};

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn financial_output_is_complete_sanitized_and_does_not_claim_the_backup_was_used() -> TestResult {
    let prepared = PreparedPay::new(synthetic_credential()?, synthetic_pay_plan()?);
    let mut details = Vec::new();

    write_pay_details(&mut details, &prepared)?;

    let details = String::from_utf8(details)?;
    insta::assert_snapshot!("pay_details", details);
    assert!(!details.contains("Warning:"));
    assert!(!details.contains("Synthetic\nrecipient"));
    for hidden in [
        "synthetic-token",
        "synthetic-device",
        "synthetic-eligibility",
        "123e4567-e89b-12d3-a456-426614174000",
    ] {
        assert!(!details.contains(hidden));
    }

    let pay = PayResult::new(
        synthetic_pay_plan()?,
        CreatedPayment::new(PaymentId::from_str("payment-1")?, FinancialStatus::Settled),
    );
    let mut pay_output = Vec::new();
    write_pay_result(&mut pay_output, &pay)?;
    let pay_output = String::from_utf8(pay_output)?;
    insta::assert_snapshot!("pay_result", pay_output);
    assert!(!pay_output.contains("Actual funding"));
    assert!(!pay_output.contains("does not prove"));

    let request = RequestCreateResult::new(
        synthetic_request_plan()?,
        CreatedRequest::new(
            RequestId::from_str("request-1")?,
            RequestStatus::from_str("pending")?,
        ),
    );
    let mut request_output = Vec::new();
    write_request_create_result(&mut request_output, &request)?;
    let request_output = String::from_utf8(request_output)?;
    insta::assert_snapshot!("request_create_result", request_output);
    assert!(!request_output.contains("Warning:"));
    Ok(())
}

#[test]
fn accept_and_decline_output_is_complete_sanitized_and_truthful() -> TestResult {
    let prepared = PreparedAccept::new(synthetic_credential()?, synthetic_accept_plan()?);
    let mut details = Vec::new();
    let timestamps = super::local_timestamps();
    write_accept_details(&mut details, &prepared, &timestamps)?;
    let details = String::from_utf8(details)?;
    insta::assert_snapshot!("accept_details", details);
    assert!(!details.contains("synthetic-token"));
    assert!(!details.contains("Synthetic\nrequest"));

    let accepted = AcceptResult::new(
        synthetic_accept_plan()?,
        AcceptedRequest::new(PaymentId::from_str("request-1")?, FinancialStatus::Settled),
    );
    let mut accept_output = Vec::new();
    write_accept_result(&mut accept_output, &accepted)?;
    let accept_output = String::from_utf8(accept_output)?;
    insta::assert_snapshot!("accept_result", accept_output);

    let externally_funded_plan = synthetic_external_accept_plan()?;
    let externally_funded =
        PreparedAccept::new(synthetic_credential()?, synthetic_external_accept_plan()?);
    let mut external_details = Vec::new();
    write_accept_details(&mut external_details, &externally_funded, &timestamps)?;
    insta::assert_snapshot!(
        "external_accept_details",
        String::from_utf8(external_details)?
    );

    let externally_accepted =
        AcceptResult::new(externally_funded_plan, AcceptedRequest::source_funded());
    let mut external_result = Vec::new();
    write_accept_result(&mut external_result, &externally_accepted)?;
    insta::assert_snapshot!(
        "external_accept_result",
        String::from_utf8(external_result)?
    );

    let decline_prepared = PreparedDecline::new(synthetic_credential()?, synthetic_decline_plan()?);
    let mut decline_details = Vec::new();
    write_decline_details(&mut decline_details, &decline_prepared, &timestamps)?;
    let decline_details = String::from_utf8(decline_details)?;
    insta::assert_snapshot!("decline_details", decline_details);
    assert!(!decline_details.contains("Synthetic\nrequest"));

    let decline_plan = synthetic_decline_plan()?;
    let decline = DeclineResult::new(
        decline_plan,
        DeclinedRequest::new(
            RequestId::from_str("request-1")?,
            RequestStatus::from_str("cancelled")?,
        ),
    );
    let mut decline_output = Vec::new();
    write_decline_result(&mut decline_output, &decline)?;
    let decline_output = String::from_utf8(decline_output)?;
    insta::assert_snapshot!("decline_result", decline_output);
    assert!(!decline_output.contains("Synthetic\nrequest"));
    Ok(())
}

#[test]
fn creation_output_renders_requested_visibility() -> TestResult {
    let prepared = PreparedPay::new(
        synthetic_credential()?,
        synthetic_pay_plan_with_visibility(Visibility::Friends)?,
    );
    let mut pay_output = Vec::new();
    write_pay_details(&mut pay_output, &prepared)?;
    assert!(String::from_utf8(pay_output)?.contains("Requested audience: friends"));

    let prepared_request = PreparedRequest::new(
        synthetic_credential()?,
        synthetic_request_plan_with_visibility(Visibility::Public)?,
    );
    let mut request_details = Vec::new();
    write_request_create_details(&mut request_details, &prepared_request)?;
    let request_details = String::from_utf8(request_details)?;
    assert!(request_details.contains("Requested audience: public"));
    insta::assert_snapshot!("request_create_details", request_details);

    let request = RequestCreateResult::new(
        synthetic_request_plan_with_visibility(Visibility::Public)?,
        CreatedRequest::new(
            RequestId::from_str("request-1")?,
            RequestStatus::from_str("pending")?,
        ),
    );
    let mut request_output = Vec::new();
    write_request_create_result(&mut request_output, &request)?;
    assert!(String::from_utf8(request_output)?.contains("Requested audience: public"));
    Ok(())
}

fn synthetic_user(id: &str, username: &str) -> Result<User, Box<dyn Error>> {
    Ok(User::new(
        UserId::from_str(id)?,
        Some(Username::from_bare(username)?),
        Some("Synthetic User".to_owned()),
    ))
}

fn synthetic_credential() -> Result<CredentialEnvelope, Box<dyn Error>> {
    Ok(CredentialEnvelope::new(
        AccessToken::from_str("synthetic-token")?,
        DeviceId::from_str("synthetic-device")?,
        UserId::from_str("123")?,
        Username::from_bare("owner")?,
        Some("Synthetic owner".to_owned()),
        time::OffsetDateTime::UNIX_EPOCH,
    ))
}

fn synthetic_pay_plan() -> Result<PayPlan, Box<dyn Error>> {
    synthetic_pay_plan_with_visibility(Visibility::Private)
}

fn synthetic_pay_plan_with_visibility(visibility: Visibility) -> Result<PayPlan, Box<dyn Error>> {
    Ok(PayPlan::new(
        ClientRequestId::from_str("123e4567-e89b-12d3-a456-426614174000")?,
        Account::new(
            UserId::from_str("123")?,
            Username::from_bare("owner")?,
            Some("Synthetic owner".to_owned()),
        ),
        User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("bob")?),
            Some("Synthetic\nrecipient".to_owned()),
        ),
        Money::from_cents(1)?,
        Note::from_str("Synthetic\nnote")?,
        Balance::new(
            SignedUsdAmount::from_cents(0),
            SignedUsdAmount::from_cents(0),
        ),
        PeerFundingMethod::new(
            PaymentMethod::new(
                PaymentMethodId::from_str("bank-1")?,
                Some("Synthetic bank".to_owned()),
                Some("bank".to_owned()),
                Some("1234".to_owned()),
                true,
            ),
            PeerFundingRole::Default,
            PeerFundingFee::Unknown,
        ),
        3,
        EligibilityToken::parse_owned("synthetic-eligibility".to_owned())?,
        visibility,
    ))
}

fn synthetic_request_plan() -> Result<CreateRequestPlan, Box<dyn Error>> {
    synthetic_request_plan_with_visibility(Visibility::Private)
}

fn synthetic_request_plan_with_visibility(
    visibility: Visibility,
) -> Result<CreateRequestPlan, Box<dyn Error>> {
    Ok(CreateRequestPlan::new(
        ClientRequestId::from_str("123e4567-e89b-12d3-a456-426614174000")?,
        Account::new(
            UserId::from_str("123")?,
            Username::from_bare("owner")?,
            Some("Synthetic owner".to_owned()),
        ),
        synthetic_user("456", "bob")?,
        Money::from_cents(1)?,
        Note::from_str("Synthetic note")?,
        visibility,
    ))
}

fn synthetic_incoming_request() -> Result<RequestRecord, Box<dyn Error>> {
    synthetic_incoming_request_with_amount(1)
}

fn synthetic_incoming_request_with_amount(
    amount_cents: u64,
) -> Result<RequestRecord, Box<dyn Error>> {
    Ok(RequestRecord::new(
        RequestId::from_str("request-1")?,
        RequestAction::Charge,
        RequestDirection::Incoming,
        User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("requester")?),
            Some("Synthetic\nrequester".to_owned()),
        )
        .with_financial_attributes(UserProfileKind::Personal, true),
        Money::from_cents(amount_cents)?,
        Some("Synthetic\nrequest".to_owned()),
        Some(time::OffsetDateTime::UNIX_EPOCH),
        RequestStatus::from_str("pending")?,
    )
    .with_audience(Some("private".to_owned())))
}

fn synthetic_accept_plan() -> Result<AcceptRequestPlan, Box<dyn Error>> {
    synthetic_accept_plan_with_balance(1)
}

fn synthetic_accept_plan_with_balance(
    available_cents: i64,
) -> Result<AcceptRequestPlan, Box<dyn Error>> {
    synthetic_accept_plan_with_balance_and_amount(available_cents, 1)
}

fn synthetic_accept_plan_with_balance_and_amount(
    available_cents: i64,
    amount_cents: u64,
) -> Result<AcceptRequestPlan, Box<dyn Error>> {
    Ok(AcceptRequestPlan::new(
        Account::new(
            UserId::from_str("123")?,
            Username::from_bare("owner")?,
            Some("Synthetic owner".to_owned()),
        ),
        synthetic_incoming_request_with_amount(amount_cents)?,
        Balance::new(
            SignedUsdAmount::from_cents(available_cents),
            SignedUsdAmount::from_cents(0),
        ),
    ))
}

fn synthetic_external_accept_plan() -> Result<AcceptRequestPlan, Box<dyn Error>> {
    Ok(
        synthetic_accept_plan_with_balance_and_amount(0, 100)?.with_external_funding(
            RequestNotificationId::from_str("notification-1")?,
            PeerFundingMethod::new(
                PaymentMethod::new(
                    PaymentMethodId::from_str("bank-1")?,
                    Some("Synthetic bank".to_owned()),
                    Some("bank".to_owned()),
                    Some("1234".to_owned()),
                    true,
                ),
                PeerFundingRole::Default,
                PeerFundingFee::Unknown,
            ),
            EligibilityToken::parse_owned("synthetic-approval-eligibility".to_owned())?,
            RequestApprovalFees::present(
                vec![RequestApprovalFee::new(
                    "venmo://fees/request-approval".to_owned(),
                    "recipient".to_owned(),
                    "synthetic-fee-token".to_owned(),
                    Some(25),
                    Some("2.5".to_owned()),
                    25,
                )],
                25,
            ),
            true,
        ),
    )
}

fn synthetic_decline_plan() -> Result<DeclineRequestPlan, Box<dyn Error>> {
    Ok(DeclineRequestPlan::new(
        Account::new(
            UserId::from_str("123")?,
            Username::from_bare("owner")?,
            Some("Synthetic owner".to_owned()),
        ),
        synthetic_incoming_request()?,
    ))
}

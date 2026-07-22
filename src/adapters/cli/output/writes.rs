use std::io::{self, Write};

use crate::features::payments::pay::PayResult;
use crate::features::payments::{FinancialStatus, PayPlan, PeerFundingFee, PeerFundingSource};
use crate::features::requests::accept::AcceptResult;
use crate::features::requests::cancel::CancelResult;
use crate::features::requests::create::RequestCreateResult;
use crate::features::requests::decline::DeclineResult;
use crate::features::requests::{
    AcceptRequestPlan, CancelRequestPlan, CreateRequestPlan, DeclineRequestPlan,
};

use super::super::response::HumanSource;
use super::TimestampFormatter;
use super::shared::{financial_user_label, sanitize_terminal_text};

pub(crate) fn write_dry_run_complete<W: Write, T: ?Sized>(
    writer: &mut W,
    _response: &T,
) -> io::Result<()> {
    writeln!(writer, "Dry run complete; no changes made.")
}

pub(crate) fn write_pay_details<W: Write>(
    writer: &mut W,
    response: &impl HumanSource<PayPlan>,
) -> io::Result<()> {
    let plan = response.human_source();
    writeln!(writer, "Payment details:")?;
    writeln!(
        writer,
        "  From account: {} (ID {})",
        sanitize_terminal_text(&plan.account().username().to_string()),
        plan.account().user_id()
    )?;
    writeln!(
        writer,
        "  Recipient: {} (ID {})",
        sanitize_terminal_text(&financial_user_label(plan.recipient())),
        plan.recipient().user_id()
    )?;
    writeln!(writer, "  Amount: ${}", plan.amount())?;
    writeln!(
        writer,
        "  Note: {}",
        sanitize_terminal_text(plan.note().as_str())
    )?;
    writeln!(writer, "  Requested audience: {}", plan.visibility())?;
    writeln!(
        writer,
        "  Purchase protection: {}",
        if plan.is_purchase_protected() {
            "requested"
        } else {
            "not requested"
        }
    )?;
    writeln!(
        writer,
        "  Available Venmo balance: {}",
        plan.balance().available()
    )?;
    write_funding_source(writer, plan.funding_source())?;
    if let Some(method) = plan.funding_source().external_method() {
        write_method_fee(writer, method.fee())?;
    }
    if let (Some(fee_cents), Some(proceeds_cents)) = (
        plan.purchase_protection_fee_cents(),
        plan.recipient_proceeds_cents(),
    ) {
        writeln!(
            writer,
            "  Estimated purchase-protection seller fee: ${}",
            format_usd_cents(u128::from(fee_cents))
        )?;
        writeln!(
            writer,
            "  Estimated recipient proceeds: ${}",
            format_usd_cents(u128::from(proceeds_cents))
        )?;
    }
    Ok(())
}

pub(crate) fn write_pay_result<W: Write>(
    writer: &mut W,
    response: &impl HumanSource<PayResult>,
) -> io::Result<()> {
    let result = response.human_source();
    writeln!(
        writer,
        "Payment ID: {}",
        sanitize_terminal_text(result.created().id().as_str())
    )?;
    writeln!(
        writer,
        "Status: {}",
        financial_status(result.created().status())
    )?;
    writeln!(
        writer,
        "Recipient: {}",
        sanitize_terminal_text(&financial_user_label(result.plan().recipient()))
    )?;
    writeln!(writer, "Amount: ${}", result.plan().amount())?;
    writeln!(writer, "Requested audience: {}", result.plan().visibility())?;
    writeln!(
        writer,
        "Purchase protection: {}",
        if result.created().is_purchase_protected() {
            "tagged by Venmo"
        } else {
            "not requested"
        }
    )?;
    writeln!(
        writer,
        "Submitted funding source ID: {}",
        sanitize_terminal_text(result.plan().funding_source().method().id().as_str())
    )
}

pub(crate) fn write_request_create_result<W: Write>(
    writer: &mut W,
    response: &impl HumanSource<RequestCreateResult>,
) -> io::Result<()> {
    let result = response.human_source();
    writeln!(
        writer,
        "Request ID: {}",
        sanitize_terminal_text(result.created().id().as_str())
    )?;
    writeln!(
        writer,
        "Status: {}",
        sanitize_terminal_text(result.created().status().as_str())
    )?;
    writeln!(
        writer,
        "Requested from: {}",
        sanitize_terminal_text(&financial_user_label(result.plan().recipient()))
    )?;
    writeln!(writer, "Amount: ${}", result.plan().amount())?;
    writeln!(writer, "Requested audience: {}", result.plan().visibility())
}

pub(crate) fn write_request_create_details<W: Write>(
    writer: &mut W,
    response: &impl HumanSource<CreateRequestPlan>,
) -> io::Result<()> {
    let plan = response.human_source();
    writeln!(writer, "Request creation details:")?;
    writeln!(
        writer,
        "  Requesting account: {} (ID {})",
        sanitize_terminal_text(&plan.account().username().to_string()),
        plan.account().user_id()
    )?;
    writeln!(
        writer,
        "  Requested from: {} (ID {})",
        sanitize_terminal_text(&financial_user_label(plan.recipient())),
        plan.recipient().user_id()
    )?;
    writeln!(writer, "  Amount: ${}", plan.amount())?;
    writeln!(
        writer,
        "  Note: {}",
        sanitize_terminal_text(plan.note().as_str())
    )?;
    writeln!(writer, "  Requested audience: {}", plan.visibility())?;
    writeln!(writer, "  Action: create payment request")
}

pub(crate) fn write_accept_details<W: Write>(
    writer: &mut W,
    response: &impl HumanSource<AcceptRequestPlan>,
    timestamps: &TimestampFormatter,
) -> io::Result<()> {
    let plan = response.human_source();
    let request = plan.request();
    writeln!(writer, "Request acceptance details:")?;
    writeln!(
        writer,
        "  Paying account: {} (ID {})",
        sanitize_terminal_text(&plan.account().username().to_string()),
        plan.account().user_id()
    )?;
    writeln!(
        writer,
        "  Request ID: {}",
        sanitize_terminal_text(request.id().as_str())
    )?;
    writeln!(
        writer,
        "  Requester: {} (ID {})",
        sanitize_terminal_text(&financial_user_label(request.counterparty())),
        request.counterparty().user_id()
    )?;
    writeln!(writer, "  Amount: ${}", request.amount())?;
    writeln!(
        writer,
        "  Note: {}",
        sanitize_terminal_text(request.note().unwrap_or(""))
    )?;
    writeln!(writer, "  Audience: private")?;
    writeln!(
        writer,
        "  Purchase protection: {}",
        if plan.is_purchase_protected() {
            "requested"
        } else {
            "not requested"
        }
    )?;
    writeln!(
        writer,
        "  Current request status: {}",
        sanitize_terminal_text(request.status().as_str())
    )?;
    if let Some(created_at) = request.created_at() {
        let created_at = timestamps.format(created_at)?;
        writeln!(writer, "  Created: {created_at}")?;
    }
    writeln!(
        writer,
        "  Available Venmo balance: {}",
        plan.balance().available()
    )?;
    if let Some(source) = plan.funding_source() {
        write_funding_source(writer, source)?;
        if let Some(method) = source.external_method() {
            write_method_fee(writer, method.fee())?;
        }
        if plan.is_purchase_protected() {
            let fee_cents = plan.approval_fee_cents().unwrap_or(0);
            writeln!(
                writer,
                "  Estimated purchase-protection seller fee: ${}",
                format_usd_cents(u128::from(fee_cents))
            )?;
            writeln!(
                writer,
                "  Estimated recipient proceeds: ${}",
                format_usd_cents(u128::from(plan.recipient_proceeds_cents().unwrap_or(0)))
            )
        } else {
            Ok(())
        }
    } else {
        writeln!(writer, "  Funding plan: available Venmo balance")
    }
}

pub(crate) fn write_accept_result<W: Write>(
    writer: &mut W,
    response: &impl HumanSource<AcceptResult>,
) -> io::Result<()> {
    let result = response.human_source();
    writeln!(
        writer,
        "Accepted request ID: {}",
        sanitize_terminal_text(result.plan().request().id().as_str())
    )?;
    if let Some(payment_id) = result.accepted().payment_id() {
        writeln!(
            writer,
            "Payment ID: {}",
            sanitize_terminal_text(payment_id.as_str())
        )?;
    }
    if let Some(status) = result.accepted().status() {
        writeln!(writer, "Status: {}", financial_status(status))?;
    }
    writeln!(
        writer,
        "Paid requester: {}",
        sanitize_terminal_text(&financial_user_label(
            result.plan().request().counterparty()
        ))
    )?;
    writeln!(writer, "Amount: ${}", result.plan().request().amount())?;
    if let Some(source) = result.plan().funding_source() {
        writeln!(
            writer,
            "Submitted funding source ID: {}",
            sanitize_terminal_text(source.method().id().as_str())
        )?;
    }
    Ok(())
}

pub(crate) fn write_decline_result<W: Write>(
    writer: &mut W,
    response: &impl HumanSource<DeclineResult>,
) -> io::Result<()> {
    let result = response.human_source();
    writeln!(
        writer,
        "Request ID: {}",
        sanitize_terminal_text(result.declined().request_id().as_str())
    )?;
    writeln!(
        writer,
        "Server status: {}",
        sanitize_terminal_text(result.declined().status().as_str())
    )?;
    writeln!(
        writer,
        "Declined requester: {}",
        sanitize_terminal_text(&financial_user_label(
            result.plan().request().counterparty()
        ))
    )?;
    writeln!(
        writer,
        "Amount requested: ${}",
        result.plan().request().amount()
    )?;
    writeln!(writer, "Money sent: no")
}

pub(crate) fn write_decline_details<W: Write>(
    writer: &mut W,
    response: &impl HumanSource<DeclineRequestPlan>,
    timestamps: &TimestampFormatter,
) -> io::Result<()> {
    let request = response.human_source().request();
    writeln!(writer, "Request decline details:")?;
    writeln!(
        writer,
        "  Request ID: {}",
        sanitize_terminal_text(request.id().as_str())
    )?;
    writeln!(
        writer,
        "  Requester: {} (ID {})",
        sanitize_terminal_text(&financial_user_label(request.counterparty())),
        request.counterparty().user_id()
    )?;
    writeln!(writer, "  Amount: ${}", request.amount())?;
    writeln!(
        writer,
        "  Note: {}",
        sanitize_terminal_text(request.note().unwrap_or(""))
    )?;
    writeln!(
        writer,
        "  Audience: {}",
        sanitize_terminal_text(request.audience().unwrap_or("(not provided)"))
    )?;
    writeln!(
        writer,
        "  Current request status: {}",
        sanitize_terminal_text(request.status().as_str())
    )?;
    if let Some(created_at) = request.created_at() {
        let created_at = timestamps.format(created_at)?;
        writeln!(writer, "  Created: {created_at}")?;
    }
    writeln!(
        writer,
        "  Action: decline this exact incoming request without sending money."
    )
}

pub(crate) fn write_cancel_result<W: Write>(
    writer: &mut W,
    response: &impl HumanSource<CancelResult>,
) -> io::Result<()> {
    let result = response.human_source();
    writeln!(
        writer,
        "Request ID: {}",
        sanitize_terminal_text(result.cancelled().request_id().as_str())
    )?;
    writeln!(
        writer,
        "Server status: {}",
        sanitize_terminal_text(result.cancelled().status().as_str())
    )?;
    writeln!(
        writer,
        "Request cancelled for: {}",
        sanitize_terminal_text(&financial_user_label(
            result.plan().request().counterparty()
        ))
    )?;
    writeln!(
        writer,
        "Amount requested: ${}",
        result.plan().request().amount()
    )?;
    writeln!(writer, "Money sent: no")
}

pub(crate) fn write_cancel_details<W: Write>(
    writer: &mut W,
    response: &impl HumanSource<CancelRequestPlan>,
    timestamps: &TimestampFormatter,
) -> io::Result<()> {
    let request = response.human_source().request();
    writeln!(writer, "Outgoing request cancellation details:")?;
    writeln!(
        writer,
        "  Request ID: {}",
        sanitize_terminal_text(request.id().as_str())
    )?;
    writeln!(
        writer,
        "  Requested from: {} (ID {})",
        sanitize_terminal_text(&financial_user_label(request.counterparty())),
        request.counterparty().user_id()
    )?;
    writeln!(writer, "  Amount: ${}", request.amount())?;
    writeln!(
        writer,
        "  Note: {}",
        sanitize_terminal_text(request.note().unwrap_or(""))
    )?;
    writeln!(
        writer,
        "  Audience: {}",
        sanitize_terminal_text(request.audience().unwrap_or("(not provided)"))
    )?;
    writeln!(
        writer,
        "  Current request status: {}",
        sanitize_terminal_text(request.status().as_str())
    )?;
    if let Some(created_at) = request.created_at() {
        let created_at = timestamps.format(created_at)?;
        writeln!(writer, "  Created: {created_at}")?;
    }
    writeln!(
        writer,
        "  Action: cancel this exact outgoing request without sending money."
    )
}

fn write_funding_source(writer: &mut impl Write, source: &PeerFundingSource) -> io::Result<()> {
    let method = source.method();
    let name = sanitize_terminal_text(method.name().unwrap_or("Payment method"));
    let method_type = sanitize_terminal_text(method.method_type().unwrap_or("unknown type"));
    let last_four = method
        .last_four()
        .map(|value| format!(" ending {}", sanitize_terminal_text(value)))
        .unwrap_or_default();
    writeln!(
        writer,
        "  Funding source: {name} ({method_type}{last_four}, ID {})",
        sanitize_terminal_text(method.id().as_str())
    )
}

fn write_method_fee(writer: &mut impl Write, fee: PeerFundingFee) -> io::Result<()> {
    match fee {
        PeerFundingFee::ProvenZero => writeln!(writer, "  Funding-source fee: $0.00"),
        PeerFundingFee::NonZero { cents } => writeln!(
            writer,
            "  Funding-source fee: ${}",
            format_usd_cents(u128::from(cents.get()))
        ),
        PeerFundingFee::Unknown => writeln!(writer, "  Funding-source fee: unknown"),
    }
}

fn format_usd_cents(cents: u128) -> String {
    format!("{}.{:02}", cents / 100, cents % 100)
}

const fn financial_status(status: FinancialStatus) -> &'static str {
    match status {
        FinancialStatus::Settled => "settled",
        FinancialStatus::Pending => "pending",
        FinancialStatus::Held => "held",
    }
}

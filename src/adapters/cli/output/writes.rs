use std::io::{self, Write};

use crate::features::payments::pay::{PayResult, PreparedPay};
use crate::features::payments::{FinancialStatus, PeerFundingFee, PeerFundingMethod};
use crate::features::requests::accept::{AcceptResult, PreparedAccept};
use crate::features::requests::create::RequestCreateResult;
use crate::features::requests::decline::{DeclineResult, PreparedDecline};

use super::TimestampFormatter;
use super::shared::{financial_user_label, sanitize_terminal_text};

pub(crate) fn write_pay_details<W: Write>(
    writer: &mut W,
    prepared: &PreparedPay,
) -> io::Result<()> {
    let plan = prepared.plan();
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
        "  Available Venmo balance: {}",
        plan.balance().available()
    )?;
    write_backup_method(writer, plan.backup_method())?;
    write_method_fee(writer, plan.backup_method().fee())?;
    writeln!(
        writer,
        "  Eligibility-reported fee: ${}",
        format_usd_cents(u128::from(plan.eligibility_fee_cents()))
    )?;
    writeln!(
        writer,
        "  Eligibility-reported total: ${}",
        format_usd_cents(
            u128::from(plan.amount().cents()) + u128::from(plan.eligibility_fee_cents())
        )
    )
}

pub(crate) fn write_pay_result<W: Write>(writer: &mut W, result: &PayResult) -> io::Result<()> {
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
        "Eligibility-reported fee: ${}",
        format_usd_cents(u128::from(result.plan().eligibility_fee_cents()))
    )?;
    writeln!(
        writer,
        "Submitted backup method ID: {}",
        sanitize_terminal_text(result.plan().backup_method().method().id().as_str())
    )
}

pub(crate) fn write_request_create_result<W: Write>(
    writer: &mut W,
    result: &RequestCreateResult,
) -> io::Result<()> {
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

pub(crate) fn write_accept_details<W: Write>(
    writer: &mut W,
    prepared: &PreparedAccept,
    timestamps: &TimestampFormatter,
) -> io::Result<()> {
    let plan = prepared.plan();
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
        "  Current request status: {}",
        sanitize_terminal_text(request.status().as_str())
    )?;
    if let Some(created_at) = request.created_at() {
        let created_at = timestamps.format(created_at)?;
        writeln!(writer, "  Created: {created_at}")?;
    }
    writeln!(
        writer,
        "  Fee/source proof: unavailable in the request-update contract"
    )?;
    writeln!(
        writer,
        "  Available Venmo balance: {}",
        plan.balance().available()
    )?;
    writeln!(
        writer,
        "  Funding guard: available balance covers the request and no external funding method will be submitted."
    )?;
    writeln!(
        writer,
        "  Warning: the update does not bind that balance snapshot or prove the final fee/source; accepting pays the requester and settles this exact request."
    )
}

pub(crate) fn write_accept_result<W: Write>(
    writer: &mut W,
    result: &AcceptResult,
) -> io::Result<()> {
    writeln!(
        writer,
        "Accepted request ID: {}",
        sanitize_terminal_text(result.plan().request().id().as_str())
    )?;
    writeln!(
        writer,
        "Payment ID: {}",
        sanitize_terminal_text(result.accepted().payment_id().as_str())
    )?;
    writeln!(
        writer,
        "Status: {}",
        financial_status(result.accepted().status())
    )?;
    writeln!(
        writer,
        "Paid requester: {}",
        sanitize_terminal_text(&financial_user_label(
            result.plan().request().counterparty()
        ))
    )?;
    writeln!(writer, "Amount: ${}", result.plan().request().amount())?;
    writeln!(
        writer,
        "Validation required full available-balance coverage and submitted no external funding method; the response did not prove the actual source or fee."
    )
}

pub(crate) fn write_decline_result<W: Write>(
    writer: &mut W,
    result: &DeclineResult,
) -> io::Result<()> {
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
    prepared: &PreparedDecline,
    timestamps: &TimestampFormatter,
) -> io::Result<()> {
    let request = prepared.plan().request();
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
    )?;
    writeln!(
        writer,
        "  Confirmation defaults to No; --yes skips only the confirmation prompt."
    )
}

fn write_backup_method(writer: &mut impl Write, method: &PeerFundingMethod) -> io::Result<()> {
    let method = method.method();
    let name = sanitize_terminal_text(method.name().unwrap_or("Payment method"));
    let method_type = sanitize_terminal_text(method.method_type().unwrap_or("unknown type"));
    let last_four = method
        .last_four()
        .map(|value| format!(" ending {}", sanitize_terminal_text(value)))
        .unwrap_or_default();
    writeln!(
        writer,
        "  Submitted backup method: {name} ({method_type}{last_four}, ID {})",
        sanitize_terminal_text(method.id().as_str())
    )
}

fn write_method_fee(writer: &mut impl Write, fee: PeerFundingFee) -> io::Result<()> {
    match fee {
        PeerFundingFee::ProvenZero => writeln!(writer, "  Submitted method fee: $0.00"),
        PeerFundingFee::NonZero { cents } => writeln!(
            writer,
            "  Submitted method fee: ${}",
            format_usd_cents(u128::from(cents.get()))
        ),
        PeerFundingFee::Unknown => writeln!(writer, "  Submitted method fee: unknown"),
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

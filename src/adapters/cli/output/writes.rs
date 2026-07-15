use std::io::{self, Write};

use time::format_description::well_known::Rfc3339;

use crate::features::payments::pay::{PayResult, PreparedPay};
use crate::features::payments::{FinancialStatus, PeerFundingMethod};
use crate::features::requests::accept::{AcceptResult, PreparedAccept};
use crate::features::requests::create::RequestCreateResult;
use crate::features::requests::decline::{DeclineResult, PreparedDecline};

use super::shared::{financial_user_label, sanitize_terminal_text};

pub(crate) fn write_pay_preflight<W: Write>(
    writer: &mut W,
    prepared: &PreparedPay,
) -> io::Result<()> {
    let plan = prepared.plan();
    writeln!(writer, "Payment preflight:")?;
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
    writeln!(writer, "  Audience: private")?;
    writeln!(writer, "  Fee: $0.00")?;
    writeln!(writer, "  Total: ${}", plan.amount())?;
    writeln!(
        writer,
        "  Available Venmo balance: {}",
        plan.balance().available()
    )?;
    write_backup_method(writer, plan.backup_method())?;
    writeln!(
        writer,
        "  Warning: Venmo may use available balance before the submitted backup method."
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
    writeln!(writer, "Audience: private")?;
    writeln!(
        writer,
        "Submitted backup method ID: {}",
        sanitize_terminal_text(result.plan().backup_method().method().id().as_str())
    )?;
    writeln!(
        writer,
        "Venmo may have used available balance before the submitted backup method."
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
    writeln!(writer, "Audience: private")
}

pub(crate) fn write_accept_preflight<W: Write>(
    writer: &mut W,
    prepared: &PreparedAccept,
) -> io::Result<()> {
    let plan = prepared.plan();
    let request = plan.request();
    writeln!(writer, "Request acceptance preflight:")?;
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
        let created_at = created_at.format(&Rfc3339).map_err(io::Error::other)?;
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
        "Preflight required full available-balance coverage and submitted no external funding method; the response did not prove the actual source or fee."
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

pub(crate) fn write_decline_preflight<W: Write>(
    writer: &mut W,
    prepared: &PreparedDecline,
) -> io::Result<()> {
    let request = prepared.plan().request();
    writeln!(writer, "Request decline preflight:")?;
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
        let created_at = created_at.format(&Rfc3339).map_err(io::Error::other)?;
        writeln!(writer, "  Created: {created_at}")?;
    }
    writeln!(
        writer,
        "  Action: decline this exact incoming request without sending money."
    )?;
    writeln!(
        writer,
        "  This command does not pause for confirmation after displaying this validated plan."
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

const fn financial_status(status: FinancialStatus) -> &'static str {
    match status {
        FinancialStatus::Settled => "settled",
        FinancialStatus::Pending => "pending",
        FinancialStatus::Held => "held",
    }
}

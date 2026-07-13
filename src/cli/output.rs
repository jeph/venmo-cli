use std::io::{self, Write};

use tabled::{builder::Builder, settings::Style};
use time::format_description::well_known::Rfc3339;

use crate::application::accept::{AcceptResult, PreparedAccept};
use crate::application::activity::{ActivityListResult, ActivityShowResult};
use crate::application::auth::{
    AuthStatus, DeviceTrustOutcome, LocalDeletionOutcome, LoginResult, LogoutReport,
    PasswordLoginReport, RemoteRevocationOutcome,
};
use crate::application::balance::BalanceResult;
use crate::application::decline::{DeclineResult, PreparedDecline};
use crate::application::doctor::DoctorReport;
use crate::application::friends::FriendsResult;
use crate::application::pay::{PayResult, PreparedPay};
use crate::application::payment_methods::PaymentMethodsResult;
use crate::application::ports::CredentialFormat;
use crate::application::request_create::RequestCreateResult;
use crate::application::requests::RequestsResult;
use crate::application::users::UserSearchResult;
use crate::domain::{
    ActivityBeforeId, ActivityCounterparty, FinancialStatus, Offset, PeerFundingMethod,
    RequestsBefore, User,
};
use crate::error::AppError;

pub fn write_error<W: Write>(writer: &mut W, error: &AppError) -> io::Result<()> {
    writeln!(
        writer,
        "error: {}",
        sanitize_terminal_text(&error.to_string())
    )
}

pub fn write_login_result<W: Write>(writer: &mut W, result: &LoginResult) -> io::Result<()> {
    writeln!(
        writer,
        "Stored Venmo credential for {}.",
        sanitize_terminal_text(&result.account().username().to_string())
    )
}

pub fn write_password_login_report<W: Write, E: Write>(
    stdout: &mut W,
    stderr: &mut E,
    report: &PasswordLoginReport,
) -> io::Result<()> {
    write_login_result(stdout, report.login())?;
    match report.device_trust() {
        DeviceTrustOutcome::NotNeeded => {
            writeln!(stdout, "Venmo accepted the existing trusted login device.")
        }
        DeviceTrustOutcome::Trusted => writeln!(stdout, "Trusted this login device."),
        DeviceTrustOutcome::Failed(source) => writeln!(
            stderr,
            "warning: the validated credential was stored, but Venmo device trust failed: {}; future login may require SMS verification.",
            sanitize_terminal_text(&source.to_string())
        ),
    }
}

pub fn write_reauthentication_report<W: Write, E: Write>(
    stdout: &mut W,
    stderr: &mut E,
    report: &PasswordLoginReport,
) -> io::Result<()> {
    write_password_login_report(stdout, stderr, report)?;
    writeln!(
        stderr,
        "warning: reauthentication does not revoke the previous bearer token; its remote validity is unknown, so use official Venmo session controls if it must be invalidated."
    )
}

pub fn write_auth_status<W: Write>(writer: &mut W, status: &AuthStatus) -> io::Result<()> {
    let saved_at = status
        .saved_at()
        .format(&Rfc3339)
        .map_err(io::Error::other)?;
    let display_name = status.account().display_name().unwrap_or("(not provided)");
    let format = match status.credential_format() {
        CredentialFormat::Version1 => "version 1",
        CredentialFormat::LegacyTypeScript => "legacy TypeScript",
    };

    writeln!(
        writer,
        "Username: {}",
        sanitize_terminal_text(&status.account().username().to_string())
    )?;
    writeln!(
        writer,
        "Display name: {}",
        sanitize_terminal_text(display_name)
    )?;
    writeln!(writer, "User ID: {}", status.account().user_id())?;
    writeln!(writer, "Saved at: {saved_at}")?;
    writeln!(writer, "Credential format: {format}")
}

pub fn write_logout_report<W: Write, E: Write>(
    stdout: &mut W,
    stderr: &mut E,
    report: &LogoutReport,
) -> io::Result<()> {
    match report.remote() {
        RemoteRevocationOutcome::NotRequested | RemoteRevocationOutcome::NotNeeded => {}
        RemoteRevocationOutcome::Revoked => {
            writeln!(stdout, "Revoked the remote Venmo token.")?;
        }
        RemoteRevocationOutcome::Failed(source) => {
            writeln!(
                stderr,
                "warning: remote token revocation failed: {}; the token may still be valid.",
                sanitize_terminal_text(&source.to_string())
            )?;
        }
        RemoteRevocationOutcome::NotAttempted(source) => {
            writeln!(
                stderr,
                "warning: remote token revocation was not attempted: {}; remote token state is unknown.",
                sanitize_terminal_text(&source.to_string())
            )?;
        }
    }

    match report.local() {
        LocalDeletionOutcome::Deleted => {
            writeln!(stdout, "Removed the local Venmo credential.")?;
        }
        LocalDeletionOutcome::Missing => {
            writeln!(stdout, "No local Venmo credential was stored.")?;
        }
        LocalDeletionOutcome::Failed(source) => {
            writeln!(
                stderr,
                "error: local credential deletion failed: {}; the local entry may remain.",
                sanitize_terminal_text(&source.to_string())
            )?;
        }
    }
    Ok(())
}

pub fn write_payment_methods<W: Write>(
    writer: &mut W,
    result: &PaymentMethodsResult,
) -> io::Result<()> {
    if result.methods().is_empty() {
        return writeln!(writer, "No payment methods found.");
    }

    let mut builder = Builder::default();
    builder.push_record(["ID", "NAME", "TYPE", "LAST4", "DEFAULT"]);
    for method in result.methods() {
        builder.push_record([
            sanitize_terminal_text(method.id().as_str()),
            sanitize_terminal_text(method.name().unwrap_or("")),
            sanitize_terminal_text(method.method_type().unwrap_or("")),
            sanitize_terminal_text(method.last_four().unwrap_or("")),
            if method.is_default() {
                "yes".to_owned()
            } else {
                String::new()
            },
        ]);
    }
    let mut table = builder.build();
    table.with(Style::psql());
    writeln!(writer, "{table}")
}

pub fn write_user_search<W: Write, E: Write>(
    stdout: &mut W,
    stderr: &mut E,
    result: &UserSearchResult,
) -> io::Result<()> {
    if result.users().is_empty() {
        writeln!(stdout, "No users found.")?;
    } else {
        let mut builder = Builder::default();
        builder.push_record(["ID", "USERNAME", "NAME"]);
        for user in result.users() {
            let username = user.username().map(ToString::to_string).unwrap_or_default();
            builder.push_record([
                sanitize_terminal_text(user.user_id().as_str()),
                sanitize_terminal_text(&username),
                sanitize_terminal_text(user.display_name().unwrap_or("")),
            ]);
        }
        let mut table = builder.build();
        table.with(Style::psql());
        writeln!(stdout, "{table}")?;
    }

    write_next_offset(stderr, result.next_offset())
}

pub fn write_friends<W: Write, E: Write>(
    stdout: &mut W,
    stderr: &mut E,
    result: &FriendsResult,
) -> io::Result<()> {
    if result.users().is_empty() {
        writeln!(stdout, "No friends found.")?;
    } else {
        write_users_table(stdout, result.users())?;
    }
    write_next_offset(stderr, result.next_offset())
}

pub fn write_balance<W: Write>(writer: &mut W, result: &BalanceResult) -> io::Result<()> {
    writeln!(writer, "Available: {}", result.balance().available())?;
    writeln!(writer, "On hold: {}", result.balance().on_hold())
}

pub fn write_pay_preflight<W: Write>(writer: &mut W, prepared: &PreparedPay) -> io::Result<()> {
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

pub fn write_pay_result<W: Write>(writer: &mut W, result: &PayResult) -> io::Result<()> {
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

pub fn write_request_create_result<W: Write>(
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
        financial_status(result.created().status())
    )?;
    writeln!(
        writer,
        "Requested from: {}",
        sanitize_terminal_text(&financial_user_label(result.plan().recipient()))
    )?;
    writeln!(writer, "Amount: ${}", result.plan().amount())?;
    writeln!(writer, "Audience: private")
}

pub fn write_accept_preflight<W: Write>(
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
    writeln!(writer, "  Current request status: {}", request.status())?;
    if let Some(created_at) = request.created_at() {
        let created_at = created_at.format(&Rfc3339).map_err(io::Error::other)?;
        writeln!(writer, "  Created: {created_at}")?;
    }
    writeln!(
        writer,
        "  Fee/source proof: unavailable in the candidate request-update contract"
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
        "  Warning: the candidate update does not bind that balance snapshot or prove the final fee/source; accepting pays the requester and settles this exact request."
    )
}

pub fn write_accept_result<W: Write>(writer: &mut W, result: &AcceptResult) -> io::Result<()> {
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
        "Preflight required full available-balance coverage and submitted no external funding method; the response did not prove the actual source."
    )
}

pub fn write_decline_result<W: Write>(writer: &mut W, result: &DeclineResult) -> io::Result<()> {
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

pub fn write_decline_preflight<W: Write>(
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
    writeln!(writer, "  Current request status: {}", request.status())?;
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

pub fn write_activity_list<W: Write, E: Write>(
    stdout: &mut W,
    stderr: &mut E,
    result: &ActivityListResult,
) -> io::Result<()> {
    if result.activities().is_empty() {
        writeln!(stdout, "No activity found.")?;
    } else {
        let mut builder = Builder::default();
        builder.push_record([
            "ID",
            "TIME",
            "ACTION",
            "DIRECTION",
            "COUNTERPARTY",
            "AMOUNT",
            "STATUS",
            "NOTE",
        ]);
        for activity in result.activities() {
            let timestamp = activity
                .occurred_at()
                .format(&Rfc3339)
                .map_err(io::Error::other)?;
            builder.push_record([
                sanitize_terminal_text(activity.id().as_str()),
                timestamp,
                sanitize_terminal_text(activity.action().as_str()),
                activity.direction().to_string(),
                sanitize_terminal_text(&activity_counterparty_label(activity.counterparty())),
                format!("${}", activity.amount()),
                sanitize_terminal_text(activity.status().as_str()),
                sanitize_terminal_text(activity.note().unwrap_or("")),
            ]);
        }
        let mut table = builder.build();
        table.with(Style::psql());
        writeln!(stdout, "{table}")?;
    }
    write_next_before_id(stderr, result.next_before_id())
}

pub fn write_activity_show<W: Write>(
    writer: &mut W,
    result: &ActivityShowResult,
) -> io::Result<()> {
    let activity = result.activity();
    let timestamp = activity
        .occurred_at()
        .format(&Rfc3339)
        .map_err(io::Error::other)?;
    writeln!(
        writer,
        "Activity ID: {}",
        sanitize_terminal_text(activity.id().as_str())
    )?;
    writeln!(writer, "Time: {timestamp}")?;
    writeln!(
        writer,
        "Action: {}",
        sanitize_terminal_text(activity.action().as_str())
    )?;
    writeln!(writer, "Direction: {}", activity.direction())?;
    writeln!(
        writer,
        "Counterparty: {}",
        sanitize_terminal_text(&activity_counterparty_label(activity.counterparty()))
    )?;
    writeln!(writer, "Amount: ${}", activity.amount())?;
    writeln!(
        writer,
        "Status: {}",
        sanitize_terminal_text(activity.status().as_str())
    )?;
    writeln!(
        writer,
        "Note: {}",
        sanitize_terminal_text(activity.note().unwrap_or(""))
    )?;
    writeln!(
        writer,
        "Audience: {}",
        sanitize_terminal_text(activity.audience().unwrap_or("(not provided)"))
    )
}

pub fn write_requests<W: Write, E: Write>(
    stdout: &mut W,
    stderr: &mut E,
    result: &RequestsResult,
) -> io::Result<()> {
    if result.requests().is_empty() {
        writeln!(stdout, "No pending requests found.")?;
    } else {
        let mut builder = Builder::default();
        builder.push_record([
            "ID",
            "DIRECTION",
            "COUNTERPARTY",
            "AMOUNT",
            "CREATED",
            "STATUS",
            "NOTE",
        ]);
        for request in result.requests() {
            let created = request
                .created_at()
                .map(|value| value.format(&Rfc3339))
                .transpose()
                .map_err(io::Error::other)?
                .unwrap_or_default();
            builder.push_record([
                sanitize_terminal_text(request.id().as_str()),
                request.direction().to_string(),
                sanitize_terminal_text(&user_label(request.counterparty())),
                format!("${}", request.amount()),
                created,
                sanitize_terminal_text(request.status().as_str()),
                sanitize_terminal_text(request.note().unwrap_or("")),
            ]);
        }
        let mut table = builder.build();
        table.with(Style::psql());
        writeln!(stdout, "{table}")?;
    }
    write_next_before(stderr, result.next_before())
}

pub fn write_doctor<W: Write>(writer: &mut W, report: &DoctorReport) -> io::Result<()> {
    let mut builder = Builder::default();
    builder.push_record(["CHECK", "STATUS", "DETAIL", "REMEDIATION"]);
    for check in report.checks() {
        builder.push_record([
            sanitize_terminal_text(check.name()),
            check.status().label().to_owned(),
            sanitize_terminal_text(check.detail()),
            sanitize_terminal_text(check.remediation().unwrap_or("")),
        ]);
    }
    let mut table = builder.build();
    table.with(Style::psql());
    writeln!(writer, "{table}")
}

fn write_users_table(writer: &mut impl Write, users: &[User]) -> io::Result<()> {
    let mut builder = Builder::default();
    builder.push_record(["ID", "USERNAME", "NAME"]);
    for user in users {
        let username = user.username().map(ToString::to_string).unwrap_or_default();
        builder.push_record([
            sanitize_terminal_text(user.user_id().as_str()),
            sanitize_terminal_text(&username),
            sanitize_terminal_text(user.display_name().unwrap_or("")),
        ]);
    }
    let mut table = builder.build();
    table.with(Style::psql());
    writeln!(writer, "{table}")
}

fn user_label(user: &User) -> String {
    user.username()
        .map(ToString::to_string)
        .unwrap_or_else(|| user.user_id().to_string())
}

fn financial_user_label(user: &User) -> String {
    let identity = user_label(user);
    match user.display_name() {
        Some(display_name) => format!("{identity} ({display_name})"),
        None => identity,
    }
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

fn activity_counterparty_label(counterparty: &ActivityCounterparty) -> String {
    match counterparty {
        ActivityCounterparty::User(user) => user_label(user),
        ActivityCounterparty::External {
            name,
            kind,
            last_four,
        } => match last_four {
            Some(last_four) => format!("{name} ({kind} ••••{last_four})"),
            None => format!("{name} ({kind})"),
        },
    }
}

fn write_next_offset(writer: &mut impl Write, offset: Option<Offset>) -> io::Result<()> {
    match offset {
        Some(offset) => writeln!(writer, "Next offset: {offset}"),
        None => Ok(()),
    }
}

fn write_next_before_id(
    writer: &mut impl Write,
    before_id: Option<&ActivityBeforeId>,
) -> io::Result<()> {
    match before_id {
        Some(before_id) => writeln!(writer, "Next before-id: {}", before_id.as_str()),
        None => Ok(()),
    }
}

fn write_next_before(writer: &mut impl Write, before: Option<&RequestsBefore>) -> io::Result<()> {
    match before {
        Some(before) => writeln!(writer, "Next before: {}", before.as_str()),
        None => Ok(()),
    }
}

#[must_use]
pub fn sanitize_terminal_text(value: &str) -> String {
    let mut sanitized = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\n' => sanitized.push_str("\\n"),
            '\r' => sanitized.push_str("\\r"),
            '\t' => sanitized.push_str("\\t"),
            character if character.is_control() || is_invisible_format_control(character) => {
                sanitized.push_str("\\u{");
                sanitized.push_str(&format!("{:04X}", u32::from(character)));
                sanitized.push('}');
            }
            character => sanitized.push(character),
        }
    }
    sanitized
}

fn is_invisible_format_control(character: char) -> bool {
    matches!(
        character,
        '\u{00AD}'
            | '\u{061C}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{FEFF}'
    )
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::str::FromStr;

    use super::*;
    use crate::application::accept::{AcceptResult, PreparedAccept};
    use crate::application::activity::{ActivityListResult, ActivityShowResult};
    use crate::application::auth::{LoginDisposition, OperationFailure};
    use crate::application::balance::BalanceResult;
    use crate::application::decline::DeclineResult;
    use crate::application::doctor::{DoctorCheck, DoctorCheckStatus, DoctorReport};
    use crate::application::friends::FriendsResult;
    use crate::application::requests::RequestsResult;
    use crate::domain::{
        AcceptRequestPlan, AcceptedRequest, AccessToken, Account, Activity, ActivityAction,
        ActivityCounterparty, ActivityDirection, ActivityId, ActivityStatus, Balance,
        ClientRequestId, CreateRequestPlan, CreatedPayment, CredentialEnvelope, DeclineRequestPlan,
        DeclinedRequest, DeviceId, EligibilityToken, Money, Note, PayPlan, PaymentId,
        PaymentMethod, PaymentMethodId, PeerFundingFee, PeerFundingMethod, PeerFundingRole,
        PendingRequest, RequestDirection, RequestDirectionFilter, RequestId, RequestStatus,
        SignedUsdAmount, User, UserId, UserProfileKind, Username,
    };

    type TestResult = Result<(), Box<dyn Error>>;

    #[test]
    fn sanitizer_escapes_terminal_and_direction_controls() {
        let value = "safe\n\u{1b}[31mred\u{202e}text\u{200b}\u{061c}";
        let sanitized = sanitize_terminal_text(value);
        assert_eq!(
            sanitized,
            "safe\\n\\u{001B}[31mred\\u{202E}text\\u{200B}\\u{061C}"
        );
        assert!(!sanitized.contains('\u{1b}'));
        assert!(!sanitized.contains('\u{202e}'));
        assert!(!sanitized.contains('\u{061c}'));
    }

    #[test]
    fn payment_method_output_is_copyable_and_sanitized() -> TestResult {
        let result = PaymentMethodsResult::from_methods(vec![PaymentMethod::new(
            PaymentMethodId::from_str("method-1")?,
            Some("Bank\nname".to_owned()),
            Some("bank".to_owned()),
            Some("1234".to_owned()),
            true,
        )]);
        let mut output = Vec::new();

        write_payment_methods(&mut output, &result)?;
        let rendered = String::from_utf8(output)?;

        assert!(rendered.contains("method-1"));
        assert!(rendered.contains("Bank\\nname"));
        assert!(rendered.contains("1234"));
        assert!(rendered.contains("yes"));
        assert!(!rendered.contains("Bank\nname"));
        Ok(())
    }

    #[test]
    fn password_login_output_reports_device_trust_truthfully() -> TestResult {
        let existing = PasswordLoginReport::new(login_result()?, DeviceTrustOutcome::NotNeeded);
        let trusted = PasswordLoginReport::new(login_result()?, DeviceTrustOutcome::Trusted);
        let failed = PasswordLoginReport::new(
            login_result()?,
            DeviceTrustOutcome::Failed(OperationFailure::new(io::Error::other(
                "synthetic trust failure",
            ))),
        );
        let mut existing_stdout = Vec::new();
        let mut existing_stderr = Vec::new();
        let mut trusted_stdout = Vec::new();
        let mut trusted_stderr = Vec::new();
        let mut failed_stdout = Vec::new();
        let mut failed_stderr = Vec::new();

        write_password_login_report(&mut existing_stdout, &mut existing_stderr, &existing)?;
        write_password_login_report(&mut trusted_stdout, &mut trusted_stderr, &trusted)?;
        write_password_login_report(&mut failed_stdout, &mut failed_stderr, &failed)?;

        assert!(String::from_utf8(existing_stdout)?.contains("existing trusted login device"));
        assert!(existing_stderr.is_empty());
        assert!(String::from_utf8(trusted_stdout)?.contains("Trusted this login device"));
        assert!(trusted_stderr.is_empty());
        assert!(String::from_utf8(failed_stdout)?.contains("Stored Venmo credential"));
        let warning = String::from_utf8(failed_stderr)?;
        assert!(warning.contains("device trust failed"));
        assert!(warning.contains("future login may require SMS verification"));
        Ok(())
    }

    #[test]
    fn reauthentication_report_output_exposes_no_authentication_material() -> TestResult {
        let report = PasswordLoginReport::new(
            LoginResult::new(
                Account::new(
                    UserId::from_str("123")?,
                    Username::from_bare("alice")?,
                    Some("Alice".to_owned()),
                ),
                time::OffsetDateTime::UNIX_EPOCH,
                LoginDisposition::ReplacedForSameAccount,
            ),
            DeviceTrustOutcome::NotNeeded,
        );
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        write_reauthentication_report(&mut stdout, &mut stderr, &report)?;

        let rendered = String::from_utf8(stdout)?;
        let warning = String::from_utf8(stderr)?;
        assert!(rendered.contains("Stored Venmo credential for @alice"));
        assert!(rendered.contains("existing trusted login device"));
        assert!(warning.contains("does not revoke the previous bearer token"));
        assert!(warning.contains("remote validity is unknown"));
        for secret in [
            "synthetic-password",
            "123456",
            "synthetic-otp-secret",
            "synthetic-issued-token",
            "stored-trusted-device",
        ] {
            assert!(!rendered.contains(secret));
            assert!(!warning.contains(secret));
        }
        Ok(())
    }

    #[test]
    fn user_search_output_is_sanitized_and_reports_a_copyable_offset() -> TestResult {
        let result = UserSearchResult::new(
            vec![User::new(
                UserId::from_str("123")?,
                Some(Username::from_bare("alice".to_owned())?),
                Some("Alice\u{1b}[31m".to_owned()),
            )],
            Some(Offset::new(10)),
        );
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        write_user_search(&mut stdout, &mut stderr, &result)?;
        let stdout = String::from_utf8(stdout)?;
        let stderr = String::from_utf8(stderr)?;

        assert!(stdout.contains("123"));
        assert!(stdout.contains("@alice"));
        assert!(!stdout.contains("@@alice"));
        assert!(stdout.contains("Alice\\u{001B}[31m"));
        assert!(!stdout.contains('\u{1b}'));
        assert!(!stdout.contains("Next offset"));
        assert_eq!(stderr, "Next offset: 10\n");
        Ok(())
    }

    #[test]
    fn friends_and_balance_output_are_copyable_exact_and_sanitized() -> TestResult {
        let friends = FriendsResult::new(
            vec![User::new(
                UserId::from_str("456")?,
                Some(Username::from_bare("bob")?),
                Some("Bob\nName".to_owned()),
            )],
            Some(Offset::new(20)),
        );
        let balance = BalanceResult::new(Balance::new(
            SignedUsdAmount::from_cents(1_234),
            SignedUsdAmount::from_cents(-5),
        ));
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        write_friends(&mut stdout, &mut stderr, &friends)?;
        write_balance(&mut stdout, &balance)?;
        let stdout = String::from_utf8(stdout)?;
        let stderr = String::from_utf8(stderr)?;

        assert!(stdout.contains("@bob"));
        assert!(!stdout.contains("@@bob"));
        assert!(stdout.contains("Bob\\nName"));
        assert!(stdout.contains("Available: $12.34"));
        assert!(stdout.contains("On hold: -$0.05"));
        assert_eq!(stderr, "Next offset: 20\n");
        Ok(())
    }

    #[test]
    fn financial_output_is_complete_sanitized_and_does_not_claim_the_backup_was_used() -> TestResult
    {
        let prepared = PreparedPay::new(synthetic_credential()?, synthetic_pay_plan()?);
        let mut preflight = Vec::new();

        write_pay_preflight(&mut preflight, &prepared)?;

        let preflight = String::from_utf8(preflight)?;
        assert!(preflight.contains("Payment preflight"));
        assert!(preflight.contains("Amount: $0.01"));
        assert!(preflight.contains("Fee: $0.00"));
        assert!(preflight.contains("Total: $0.01"));
        assert!(preflight.contains("Available Venmo balance: $0.00"));
        assert!(preflight.contains("Submitted backup method"));
        assert!(preflight.contains("Venmo may use available balance"));
        assert!(preflight.contains("Synthetic\\nrecipient"));
        assert!(preflight.contains("Synthetic\\nnote"));
        assert!(!preflight.contains("Synthetic\nrecipient"));
        for hidden in [
            "synthetic-token",
            "synthetic-device",
            "synthetic-eligibility",
            "123e4567-e89b-12d3-a456-426614174000",
        ] {
            assert!(!preflight.contains(hidden));
        }

        let pay = PayResult::new(
            synthetic_pay_plan()?,
            CreatedPayment::new(PaymentId::from_str("payment-1")?, FinancialStatus::Settled),
        );
        let mut pay_output = Vec::new();
        write_pay_result(&mut pay_output, &pay)?;
        let pay_output = String::from_utf8(pay_output)?;
        assert!(pay_output.contains("Payment ID: payment-1"));
        assert!(pay_output.contains("Status: settled"));
        assert!(pay_output.contains("Submitted backup method ID: bank-1"));
        assert!(pay_output.contains("may have used available balance"));
        assert!(!pay_output.contains("Actual funding"));

        let request = RequestCreateResult::new(
            synthetic_request_plan()?,
            CreatedPayment::new(PaymentId::from_str("request-1")?, FinancialStatus::Pending),
        );
        let mut request_output = Vec::new();
        write_request_create_result(&mut request_output, &request)?;
        let request_output = String::from_utf8(request_output)?;
        assert!(request_output.contains("Request ID: request-1"));
        assert!(request_output.contains("Status: pending"));
        assert!(request_output.contains("Audience: private"));
        Ok(())
    }

    #[test]
    fn accept_and_decline_output_is_complete_sanitized_and_truthful() -> TestResult {
        let prepared = PreparedAccept::new(synthetic_credential()?, synthetic_accept_plan()?);
        let mut preflight = Vec::new();
        write_accept_preflight(&mut preflight, &prepared)?;
        let preflight = String::from_utf8(preflight)?;
        assert!(preflight.contains("Request acceptance preflight"));
        assert!(preflight.contains("Amount: $0.01"));
        assert!(preflight.contains("Fee/source proof: unavailable"));
        assert!(preflight.contains("does not bind that balance snapshot"));
        assert!(preflight.contains("accepting pays the requester"));
        assert!(preflight.contains("Synthetic\\nrequest"));
        assert!(!preflight.contains("synthetic-token"));

        let accepted = AcceptResult::new(
            synthetic_accept_plan()?,
            AcceptedRequest::new(PaymentId::from_str("request-1")?, FinancialStatus::Settled),
        );
        let mut accept_output = Vec::new();
        write_accept_result(&mut accept_output, &accepted)?;
        let accept_output = String::from_utf8(accept_output)?;
        assert!(accept_output.contains("Accepted request ID: request-1"));
        assert!(accept_output.contains("Payment ID: request-1"));
        assert!(accept_output.contains("submitted no external funding method"));
        assert!(accept_output.contains("did not prove the actual source"));

        let decline_prepared =
            PreparedDecline::new(synthetic_credential()?, synthetic_decline_plan()?);
        let mut decline_preflight = Vec::new();
        write_decline_preflight(&mut decline_preflight, &decline_prepared)?;
        let decline_preflight = String::from_utf8(decline_preflight)?;
        assert!(decline_preflight.contains("Request decline preflight"));
        assert!(decline_preflight.contains("without sending money"));
        assert!(decline_preflight.contains("does not pause for confirmation"));
        assert!(decline_preflight.contains("Synthetic\\nrequest"));

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
        assert!(decline_output.contains("Server status: cancelled"));
        assert!(decline_output.contains("Money sent: no"));
        assert!(!decline_output.contains("Synthetic\nrequest"));
        Ok(())
    }

    #[test]
    fn activity_list_and_show_render_authoritative_status_as_data() -> TestResult {
        let activity = synthetic_activity("failed", "note\n\u{1b}[31mline")?;
        let transfer = Activity::new(
            ActivityId::from_str("story-transfer")?,
            time::OffsetDateTime::UNIX_EPOCH,
            ActivityAction::from_str("transfer:standard")?,
            ActivityDirection::Outgoing,
            ActivityCounterparty::external(
                "Bank\nname".to_owned(),
                "bank".to_owned(),
                Some("1234".to_owned()),
            ),
            Money::from_str("12.34")?,
            ActivityStatus::from_str("issued")?,
            None,
            Some("private".to_owned()),
        );
        let list = ActivityListResult::new(
            vec![activity.clone(), transfer],
            Some(ActivityBeforeId::from_str("story-next")?),
        );
        let show = ActivityShowResult::new(activity);
        let mut list_stdout = Vec::new();
        let mut list_stderr = Vec::new();
        let mut show_stdout = Vec::new();

        write_activity_list(&mut list_stdout, &mut list_stderr, &list)?;
        write_activity_show(&mut show_stdout, &show)?;
        let list_stdout = String::from_utf8(list_stdout)?;
        let show_stdout = String::from_utf8(show_stdout)?;

        assert_eq!(
            String::from_utf8(list_stderr)?,
            "Next before-id: story-next\n"
        );
        assert!(list_stdout.contains("story-1"));
        assert!(list_stdout.contains("failed"));
        assert!(list_stdout.contains("note\\n\\u{001B}[31mline"));
        assert!(list_stdout.contains("Bank\\nname (bank ••••1234)"));
        assert!(!list_stdout.contains('\u{1b}'));
        assert!(show_stdout.contains("Status: failed"));
        assert!(show_stdout.contains("Direction: outgoing"));
        assert!(show_stdout.contains("Audience: private"));
        Ok(())
    }

    #[test]
    fn pending_request_output_preserves_ids_and_sanitizes_notes() -> TestResult {
        let requests = RequestsResult::new(
            vec![PendingRequest::new(
                RequestId::from_str("request-1")?,
                RequestDirection::Incoming,
                synthetic_user("456", "bob")?,
                Money::from_str("0.01")?,
                Some("request\u{202e}note".to_owned()),
                Some(time::OffsetDateTime::UNIX_EPOCH),
                RequestStatus::from_str("pending")?,
            )],
            RequestDirectionFilter::Incoming,
            Some(RequestsBefore::from_str("request-next")?),
        );
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        write_requests(&mut stdout, &mut stderr, &requests)?;
        let stdout = String::from_utf8(stdout)?;
        let stderr = String::from_utf8(stderr)?;

        assert!(stdout.contains("request-1"));
        assert!(stdout.contains("incoming"));
        assert!(stdout.contains("request\\u{202E}note"));
        assert!(!stdout.contains('\u{202e}'));
        assert_eq!(stderr, "Next before: request-next\n");
        Ok(())
    }

    #[test]
    fn continuation_is_not_written_when_buffered_record_output_fails() -> TestResult {
        struct FailingWriter;

        impl Write for FailingWriter {
            fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("synthetic output failure"))
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let result = FriendsResult::new(vec![synthetic_user("456", "bob")?], Some(Offset::new(1)));
        let mut stdout = FailingWriter;
        let mut stderr = Vec::new();

        assert!(write_friends(&mut stdout, &mut stderr, &result).is_err());
        assert!(stderr.is_empty());
        Ok(())
    }

    #[test]
    fn doctor_output_contains_all_statuses_and_sanitizes_details() -> TestResult {
        let report = DoctorReport::new(vec![
            DoctorCheck::new("Build", DoctorCheckStatus::Pass, "ok", None),
            DoctorCheck::new(
                "Credential",
                DoctorCheckStatus::Fail,
                "bad\nstate",
                Some("log in"),
            ),
            DoctorCheck::new("Shapes", DoctorCheckStatus::Skipped, "dependency", None),
        ]);
        let mut output = Vec::new();

        write_doctor(&mut output, &report)?;
        let output = String::from_utf8(output)?;

        assert!(output.contains("Pass"));
        assert!(output.contains("Fail"));
        assert!(output.contains("Skipped"));
        assert!(output.contains("bad\\nstate"));
        assert!(!output.contains("bad\nstate"));
        Ok(())
    }

    fn login_result() -> Result<LoginResult, Box<dyn Error>> {
        Ok(LoginResult::new(
            Account::new(
                UserId::from_str("123")?,
                Username::from_bare("alice")?,
                Some("Alice".to_owned()),
            ),
            time::OffsetDateTime::UNIX_EPOCH,
            LoginDisposition::Created,
        ))
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
                PeerFundingFee::ProvenZero,
            ),
            EligibilityToken::parse_owned("synthetic-eligibility".to_owned())?,
        ))
    }

    fn synthetic_request_plan() -> Result<CreateRequestPlan, Box<dyn Error>> {
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
        ))
    }

    fn synthetic_incoming_request() -> Result<PendingRequest, Box<dyn Error>> {
        Ok(PendingRequest::new(
            RequestId::from_str("request-1")?,
            RequestDirection::Incoming,
            User::new(
                UserId::from_str("456")?,
                Some(Username::from_bare("requester")?),
                Some("Synthetic\nrequester".to_owned()),
            )
            .with_financial_attributes(UserProfileKind::Personal, true),
            Money::from_cents(1)?,
            Some("Synthetic\nrequest".to_owned()),
            Some(time::OffsetDateTime::UNIX_EPOCH),
            RequestStatus::from_str("pending")?,
        )
        .with_audience(Some("private".to_owned())))
    }

    fn synthetic_accept_plan() -> Result<AcceptRequestPlan, Box<dyn Error>> {
        Ok(AcceptRequestPlan::new(
            Account::new(
                UserId::from_str("123")?,
                Username::from_bare("owner")?,
                Some("Synthetic owner".to_owned()),
            ),
            synthetic_incoming_request()?,
            Balance::new(
                SignedUsdAmount::from_cents(1),
                SignedUsdAmount::from_cents(0),
            ),
        ))
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

    fn synthetic_activity(status: &str, note: &str) -> Result<Activity, Box<dyn Error>> {
        Ok(Activity::new(
            ActivityId::from_str("story-1")?,
            time::OffsetDateTime::UNIX_EPOCH,
            ActivityAction::from_str("pay")?,
            ActivityDirection::Outgoing,
            ActivityCounterparty::user(synthetic_user("456", "bob")?),
            Money::from_str("1.25")?,
            ActivityStatus::from_str(status)?,
            Some(note.to_owned()),
            Some("private".to_owned()),
        ))
    }
}

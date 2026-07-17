use std::io::{self, Write};

use tabled::builder::Builder;
use time::format_description::well_known::Rfc3339;

use crate::features::transfers::model::{TransferDirection, TransferModeOptions, TransferSpeed};
use crate::features::transfers::options::TransferOptionsResult;
use crate::features::transfers::out::{PreparedTransferOut, TransferOutResult};

use super::shared::{sanitize_terminal_text, write_table};

pub(crate) fn write_transfer_options<W: Write>(
    writer: &mut W,
    result: &TransferOptionsResult,
) -> io::Result<()> {
    let options = result.options();
    writeln!(
        writer,
        "Preferred inbound speed: {}",
        render_optional_speed(options.preferred_in())
    )?;
    writeln!(
        writer,
        "Preferred outbound speed: {}",
        render_optional_speed(options.preferred_out())
    )?;
    write_mode_summary(writer, TransferSpeed::Standard, options.standard())?;
    write_mode_summary(writer, TransferSpeed::Instant, options.instant())?;

    let mut builder = Builder::default();
    builder.push_record([
        "SPEED",
        "DIRECTION",
        "ID",
        "NAME",
        "ASSET",
        "TYPE",
        "LAST4",
        "DEFAULT",
        "ESTIMATE",
    ]);
    let mut rows = 0_usize;
    for (speed, mode) in [
        (TransferSpeed::Standard, options.standard()),
        (TransferSpeed::Instant, options.instant()),
    ] {
        for (direction, instruments) in [
            (TransferDirection::In, mode.eligible_sources()),
            (TransferDirection::Out, mode.eligible_destinations()),
        ] {
            for instrument in instruments {
                builder.push_record([
                    speed.to_string(),
                    direction.to_string(),
                    sanitize_terminal_text(instrument.id().as_str()),
                    sanitize_terminal_text(instrument.name()),
                    sanitize_terminal_text(instrument.asset_name()),
                    sanitize_terminal_text(instrument.instrument_type()),
                    sanitize_terminal_text(instrument.last_four()),
                    if instrument.is_default() {
                        "yes".to_owned()
                    } else {
                        String::new()
                    },
                    sanitize_terminal_text(instrument.transfer_to_estimate()),
                ]);
                rows += 1;
            }
        }
    }
    if rows == 0 {
        writeln!(writer, "No eligible transfer instruments found.")
    } else {
        write_table(writer, builder)
    }
}

pub(crate) fn write_transfer_out_preflight<W: Write>(
    writer: &mut W,
    prepared: &PreparedTransferOut,
) -> io::Result<()> {
    let plan = prepared.plan();
    let destination = plan.destination();
    writeln!(writer, "Transfer preflight:")?;
    writeln!(
        writer,
        "  From account: {} (ID {})",
        sanitize_terminal_text(&plan.account().username().to_string()),
        plan.account().user_id()
    )?;
    writeln!(writer, "  Direction: out")?;
    writeln!(writer, "  Speed: {}", plan.speed())?;
    writeln!(writer, "  Amount: ${}", plan.amount())?;
    writeln!(
        writer,
        "  Available Venmo balance: {}",
        plan.balance().available()
    )?;
    writeln!(
        writer,
        "  Destination: {} ({} ending {}, ID {})",
        sanitize_terminal_text(destination.name()),
        sanitize_terminal_text(destination.instrument_type()),
        sanitize_terminal_text(destination.last_four()),
        sanitize_terminal_text(destination.id().as_str())
    )?;
    writeln!(
        writer,
        "  Estimate: {}",
        sanitize_terminal_text(destination.transfer_to_estimate())
    )?;
    writeln!(
        writer,
        "  Submitted amount and final amount: identical integer cents"
    )?;
    writeln!(
        writer,
        "  Warning: this unsupported private transfer API must never be retried after an uncertain outcome."
    )
}

pub(crate) fn write_transfer_out_result<W: Write>(
    writer: &mut W,
    result: &TransferOutResult,
) -> io::Result<()> {
    let requested_at = result
        .created()
        .requested_at()
        .format(&Rfc3339)
        .map_err(io::Error::other)?;
    writeln!(
        writer,
        "Transfer ID: {}",
        sanitize_terminal_text(result.created().id().as_str())
    )?;
    writeln!(
        writer,
        "Status: {}",
        sanitize_terminal_text(result.created().status())
    )?;
    writeln!(writer, "Direction: out")?;
    writeln!(writer, "Speed: {}", result.plan().speed())?;
    writeln!(writer, "Requested amount: ${}", result.plan().amount())?;
    writeln!(writer, "Net amount: ${}", result.created().net_amount())?;
    writeln!(
        writer,
        "Fee: ${}.{:02}",
        result.created().fee_cents() / 100,
        result.created().fee_cents() % 100
    )?;
    writeln!(writer, "Requested: {requested_at}")?;
    writeln!(
        writer,
        "Destination ID: {}",
        sanitize_terminal_text(result.plan().destination().id().as_str())
    )
}

fn render_optional_speed(speed: Option<TransferSpeed>) -> &'static str {
    match speed {
        Some(TransferSpeed::Standard) => "standard",
        Some(TransferSpeed::Instant) => "instant",
        None => "none",
    }
}

fn write_mode_summary(
    writer: &mut impl Write,
    speed: TransferSpeed,
    mode: &TransferModeOptions,
) -> io::Result<()> {
    let fee = if mode.fee().is_empty() {
        "absent"
    } else {
        "present; units unverified"
    };
    writeln!(
        writer,
        "{speed} estimate: {}",
        sanitize_terminal_text(mode.transfer_to_estimate())
    )?;
    writeln!(writer, "{speed} fee metadata: {fee}")
}

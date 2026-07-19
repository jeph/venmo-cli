use std::io::{self, Write};

use tabled::builder::Builder;

use crate::features::wallet::balance::BalanceResult;
use crate::features::wallet::payment_methods::PaymentMethodsResult;

use super::shared::{sanitize_terminal_text, write_table};

pub(crate) fn write_payment_methods<W: Write>(
    writer: &mut W,
    result: &PaymentMethodsResult,
) -> io::Result<()> {
    if result.methods().is_empty() {
        return writeln!(writer, "No payment methods found.");
    }

    let mut builder = Builder::default();
    builder.push_record(["Id", "Name", "Type", "Last 4", "Default"]);
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
    write_table(writer, builder)
}

pub(crate) fn write_balance<W: Write>(writer: &mut W, result: &BalanceResult) -> io::Result<()> {
    writeln!(writer, "Available: {}", result.balance().available())?;
    writeln!(writer, "On hold: {}", result.balance().on_hold())
}

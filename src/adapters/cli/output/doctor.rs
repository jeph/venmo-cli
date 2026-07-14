use std::io::{self, Write};

use tabled::builder::Builder;

use crate::features::doctor::DoctorReport;

use super::shared::{sanitize_terminal_text, write_table};

pub(crate) fn write_doctor<W: Write>(writer: &mut W, report: &DoctorReport) -> io::Result<()> {
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
    write_table(writer, builder)
}

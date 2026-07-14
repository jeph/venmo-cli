use std::error::Error;

use super::super::write_doctor;
use crate::features::doctor::{DoctorCheck, DoctorCheckStatus, DoctorReport};

#[test]
fn doctor_output_contains_all_statuses_and_sanitizes_details() -> Result<(), Box<dyn Error>> {
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

    insta::assert_snapshot!("doctor_report", output);
    assert!(!output.contains("bad\nstate"));
    Ok(())
}

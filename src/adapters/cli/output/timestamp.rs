use std::io;

use jiff::{Timestamp, civil::DateTime, tz::TimeZone};
use time::OffsetDateTime;

#[derive(Clone)]
pub(crate) struct TimestampFormatter {
    time_zone: TimeZone,
    show_utc_suffix: bool,
}

impl TimestampFormatter {
    pub(crate) fn system_or_utc() -> Self {
        Self::from_detected(TimeZone::try_system().ok())
    }

    fn from_detected(time_zone: Option<TimeZone>) -> Self {
        match time_zone {
            Some(time_zone) => Self {
                show_utc_suffix: time_zone == TimeZone::UTC,
                time_zone,
            },
            None => Self::utc(),
        }
    }

    fn utc() -> Self {
        Self {
            time_zone: TimeZone::UTC,
            show_utc_suffix: true,
        }
    }

    pub(crate) fn format(&self, timestamp: OffsetDateTime) -> io::Result<String> {
        let nanosecond = i32::try_from(timestamp.nanosecond()).map_err(io::Error::other)?;
        let timestamp =
            Timestamp::new(timestamp.unix_timestamp(), nanosecond).map_err(io::Error::other)?;
        let local = self.time_zone.to_datetime(timestamp);
        Ok(format_local(local, self.show_utc_suffix))
    }

    #[cfg(test)]
    pub(crate) fn for_time_zone(time_zone: TimeZone) -> Self {
        Self::from_detected(Some(time_zone))
    }
}

fn format_local(timestamp: DateTime, show_utc_suffix: bool) -> String {
    let suffix = if show_utc_suffix { "Z" } else { "" };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{suffix}",
        timestamp.year(),
        timestamp.month(),
        timestamp.day(),
        timestamp.hour(),
        timestamp.minute(),
        timestamp.second(),
    )
}

#[cfg(test)]
mod tests {
    use std::io;

    use jiff::tz::TimeZone;
    use time::{OffsetDateTime, format_description::well_known::Rfc3339};

    use super::TimestampFormatter;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn system_detection_failure_falls_back_to_explicit_utc() -> TestResult {
        let formatter = TimestampFormatter::from_detected(None);
        let timestamp = OffsetDateTime::parse("2026-07-18T20:39:51.987654321Z", &Rfc3339)?;

        assert_eq!(formatter.format(timestamp)?, "2026-07-18T20:39:51Z");
        Ok(())
    }

    #[test]
    fn local_output_uses_historical_zone_rules_without_a_zone_suffix() -> TestResult {
        let time_zone = TimeZone::posix("PST8PDT,M3.2.0,M11.1.0")?;
        let formatter = TimestampFormatter::for_time_zone(time_zone);
        let winter = OffsetDateTime::parse("2026-01-18T20:39:51Z", &Rfc3339)?;
        let summer = OffsetDateTime::parse("2026-07-18T20:39:51Z", &Rfc3339)?;

        assert_eq!(formatter.format(winter)?, "2026-01-18T12:39:51");
        assert_eq!(formatter.format(summer)?, "2026-07-18T13:39:51");
        Ok(())
    }

    #[test]
    fn explicit_utc_keeps_the_utc_suffix() -> Result<(), io::Error> {
        let formatter = TimestampFormatter::for_time_zone(TimeZone::UTC);

        assert_eq!(
            formatter.format(OffsetDateTime::UNIX_EPOCH)?,
            "1970-01-01T00:00:00Z"
        );
        Ok(())
    }
}

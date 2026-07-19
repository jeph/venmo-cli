mod activity_requests;
mod auth;
mod people_wallet;
mod shared;
mod transfers;
mod writes;

fn local_timestamps() -> super::TimestampFormatter {
    super::TimestampFormatter::for_time_zone(jiff::tz::TimeZone::fixed(jiff::tz::offset(-8)))
}

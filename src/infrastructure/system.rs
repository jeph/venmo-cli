use time::OffsetDateTime;

use crate::application::ports::Clock;

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_utc(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_returns_a_utc_instant() {
        assert!(SystemClock.now_utc() > OffsetDateTime::UNIX_EPOCH);
    }
}

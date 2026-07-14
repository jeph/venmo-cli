use time::OffsetDateTime;

use crate::shared::Clock;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SystemClock;

impl Clock for SystemClock {
    fn now_utc(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_result_is_bracketed_by_adapter_boundary_observations() {
        let before = OffsetDateTime::now_utc();
        let observed = SystemClock.now_utc();
        let after = OffsetDateTime::now_utc();

        assert!(observed >= before);
        assert!(observed <= after);
        assert_eq!(observed.offset(), time::UtcOffset::UTC);
    }
}

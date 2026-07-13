use time::OffsetDateTime;

use crate::application::ports::{ClientRequestIdGenerator, Clock};
use crate::domain::ClientRequestId;

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_utc(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

#[derive(Clone, Copy, Debug, Default)]
#[allow(dead_code)]
pub(crate) struct SystemClientRequestIdGenerator;

impl ClientRequestIdGenerator for SystemClientRequestIdGenerator {
    fn generate(&self) -> ClientRequestId {
        ClientRequestId::generate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_returns_a_utc_instant() {
        assert!(SystemClock.now_utc() > OffsetDateTime::UNIX_EPOCH);
    }

    #[test]
    fn client_request_ids_are_unique_and_redacted() {
        let first = SystemClientRequestIdGenerator.generate();
        let second = SystemClientRequestIdGenerator.generate();
        assert_ne!(first, second);
        assert!(!format!("{first:?}").contains(&first.to_string()));
    }
}

use std::fmt;
use std::str::FromStr;

use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct ClientRequestId(Uuid);

impl ClientRequestId {
    #[must_use]
    pub(crate) fn generate() -> Self {
        Self(Uuid::new_v4())
    }
}

impl fmt::Display for ClientRequestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl fmt::Debug for ClientRequestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ClientRequestId([REDACTED])")
    }
}

impl FromStr for ClientRequestId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(value).map(Self)
    }
}

pub trait ClientRequestIdGenerator {
    fn generate(&self) -> ClientRequestId;
}

pub trait Clock {
    fn now_utc(&self) -> OffsetDateTime;
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    #[test]
    fn request_ids_are_canonical_and_debug_redacted() -> Result<(), Box<dyn Error>> {
        let id = ClientRequestId::from_str("123e4567-e89b-12d3-a456-426614174000")?;
        assert_eq!(id.to_string(), "123e4567-e89b-12d3-a456-426614174000");
        assert!(!format!("{id:?}").contains("123e4567"));
        Ok(())
    }
}

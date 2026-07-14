use std::fmt;
use std::num::NonZeroU32;
use std::str::FromStr;

use thiserror::Error;

pub const DEFAULT_LIST_LIMIT: u32 = 10;
pub const MAX_LIST_LIMIT: u32 = 50;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limit(NonZeroU32);

impl Limit {
    pub const MIN: Self = Self(NonZeroU32::MIN);

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl TryFrom<u32> for Limit {
    type Error = LimitParseError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        let nonzero = NonZeroU32::new(value).ok_or(LimitParseError::Zero)?;
        if value > MAX_LIST_LIMIT {
            return Err(LimitParseError::TooLarge {
                maximum: MAX_LIST_LIMIT,
            });
        }
        Ok(Self(nonzero))
    }
}

impl fmt::Display for Limit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for Limit {
    type Err = LimitParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let parsed = value.parse::<u32>().map_err(|_| LimitParseError::Invalid)?;
        Self::try_from(parsed)
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum LimitParseError {
    #[error("limit must be a positive integer that fits in 32 bits")]
    Invalid,

    #[error("limit must be greater than zero")]
    Zero,

    #[error("limit must not exceed {maximum}")]
    TooLarge { maximum: u32 },
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Offset(u32);

impl Offset {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for Offset {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for Offset {
    type Err = OffsetParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(OffsetParseError);
        }
        value.parse::<u32>().map(Self).map_err(|_| OffsetParseError)
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("offset must be a nonnegative integer that fits in 32 bits")]
pub struct OffsetParseError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_are_typed_nonnegative_u32_values() {
        assert_eq!(Offset::from_str("0").map(Offset::get), Ok(0));
        assert_eq!(
            Offset::from_str("4294967295").map(Offset::get),
            Ok(u32::MAX)
        );
        for value in ["", "-1", "+1", "4294967296", "1.0"] {
            assert!(Offset::from_str(value).is_err(), "accepted {value:?}");
        }
    }
}

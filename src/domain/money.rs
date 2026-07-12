use std::fmt;
use std::str::FromStr;

use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Money {
    cents: u64,
}

impl Money {
    pub fn from_cents(cents: u64) -> Result<Self, MoneyParseError> {
        if cents == 0 {
            return Err(MoneyParseError::NotPositive);
        }
        Ok(Self { cents })
    }

    #[must_use]
    pub const fn cents(self) -> u64 {
        self.cents
    }
}

impl fmt::Display for Money {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{:02}", self.cents / 100, self.cents % 100)
    }
}

impl FromStr for Money {
    type Err = MoneyParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() {
            return Err(MoneyParseError::InvalidSyntax);
        }

        let (whole, fractional) = match value.split_once('.') {
            Some((whole, fractional)) => (whole, Some(fractional)),
            None => (value, None),
        };

        if whole.is_empty()
            || !whole.bytes().all(|byte| byte.is_ascii_digit())
            || fractional.is_some_and(|digits| {
                digits.is_empty()
                    || digits.len() > 2
                    || !digits.bytes().all(|byte| byte.is_ascii_digit())
            })
        {
            return Err(MoneyParseError::InvalidSyntax);
        }

        let whole_cents = whole
            .parse::<u64>()
            .map_err(|_| MoneyParseError::TooLarge)?
            .checked_mul(100)
            .ok_or(MoneyParseError::TooLarge)?;

        let fractional_cents = match fractional {
            None => 0,
            Some(digits) if digits.len() == 1 => digits
                .parse::<u64>()
                .map_err(|_| MoneyParseError::InvalidSyntax)?
                .checked_mul(10)
                .ok_or(MoneyParseError::TooLarge)?,
            Some(digits) => digits
                .parse::<u64>()
                .map_err(|_| MoneyParseError::InvalidSyntax)?,
        };

        let cents = whole_cents
            .checked_add(fractional_cents)
            .ok_or(MoneyParseError::TooLarge)?;
        Self::from_cents(cents)
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum MoneyParseError {
    #[error("amount must be a decimal number with at most two fractional digits")]
    InvalidSyntax,

    #[error("amount must be greater than zero")]
    NotPositive,

    #[error("amount is too large")]
    TooLarge,
}

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use super::{DecimalMagnitudeError, parse_decimal_magnitude};

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
        let magnitude = parse_decimal_magnitude(value).map_err(|error| match error {
            DecimalMagnitudeError::InvalidSyntax => MoneyParseError::InvalidSyntax,
            DecimalMagnitudeError::TooLarge => MoneyParseError::TooLarge,
        })?;
        let cents = u64::try_from(magnitude).map_err(|_| MoneyParseError::TooLarge)?;
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

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    #[test]
    fn money_preserves_exact_cent_boundaries_through_u64_max() -> Result<(), Box<dyn Error>> {
        for (input, expected_cents, expected_display) in [
            ("0.01", 1, "0.01"),
            ("1", 100, "1.00"),
            ("1.2", 120, "1.20"),
            ("184467440737095516.15", u64::MAX, "184467440737095516.15"),
        ] {
            let money = Money::from_str(input)?;
            assert_eq!(money.cents(), expected_cents);
            assert_eq!(money.to_string(), expected_display);
        }
        assert_eq!(Money::from_cents(u64::MAX)?.cents(), u64::MAX);
        Ok(())
    }

    #[test]
    fn money_rejects_zero_sign_precision_unicode_and_overflow_with_exact_errors() {
        for zero in ["0", "00", "0.0", "0.00", "000.00"] {
            assert_eq!(Money::from_str(zero), Err(MoneyParseError::NotPositive));
        }
        for invalid in [
            "", ".01", "1.", "1.001", "+1", "-1", "1e2", "１.00", "1,00", " 1.00",
        ] {
            assert_eq!(
                Money::from_str(invalid),
                Err(MoneyParseError::InvalidSyntax),
                "misclassified {invalid:?}"
            );
        }
        for oversized in [
            "184467440737095516.16",
            "184467440737095517",
            "340282366920938463463374607431768211456",
        ] {
            assert_eq!(
                Money::from_str(oversized),
                Err(MoneyParseError::TooLarge),
                "misclassified {oversized:?}"
            );
        }
        assert_eq!(Money::from_cents(0), Err(MoneyParseError::NotPositive));
    }
}

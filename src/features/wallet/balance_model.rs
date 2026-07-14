use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use crate::shared::{DecimalMagnitudeError, parse_decimal_magnitude};

/// An exact, signed USD amount used for wallet-balance fields.
#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub struct SignedUsdAmount {
    cents: i64,
}

impl SignedUsdAmount {
    #[must_use]
    pub const fn from_cents(cents: i64) -> Self {
        Self { cents }
    }

    #[must_use]
    pub const fn cents(self) -> i64 {
        self.cents
    }
}

impl fmt::Debug for SignedUsdAmount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SignedUsdAmount([REDACTED])")
    }
}

impl fmt::Display for SignedUsdAmount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let magnitude = self.cents.unsigned_abs();
        if self.cents < 0 {
            formatter.write_str("-")?;
        }
        write!(formatter, "${}.{:02}", magnitude / 100, magnitude % 100)
    }
}

impl FromStr for SignedUsdAmount {
    type Err = SignedUsdAmountParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() {
            return Err(SignedUsdAmountParseError::InvalidSyntax);
        }

        let (negative, unsigned) = match value.strip_prefix('-') {
            Some(unsigned) => (true, unsigned),
            None => (false, value),
        };
        let magnitude = parse_decimal_magnitude(unsigned).map_err(|error| match error {
            DecimalMagnitudeError::InvalidSyntax => SignedUsdAmountParseError::InvalidSyntax,
            DecimalMagnitudeError::TooLarge => SignedUsdAmountParseError::TooLarge,
        })?;
        let cents = if negative && magnitude == i64::MIN.unsigned_abs().into() {
            i64::MIN
        } else {
            let magnitude =
                i64::try_from(magnitude).map_err(|_| SignedUsdAmountParseError::TooLarge)?;
            if negative { -magnitude } else { magnitude }
        };
        Ok(Self::from_cents(cents))
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum SignedUsdAmountParseError {
    #[error("USD amount must be a signed decimal number with at most two fractional digits")]
    InvalidSyntax,

    #[error("USD amount is too large")]
    TooLarge,
}

/// The authoritative Venmo-wallet amounts returned by the account endpoint.
#[derive(Clone, Eq, PartialEq)]
pub struct Balance {
    available: SignedUsdAmount,
    on_hold: SignedUsdAmount,
}

impl Balance {
    #[must_use]
    pub const fn new(available: SignedUsdAmount, on_hold: SignedUsdAmount) -> Self {
        Self { available, on_hold }
    }

    #[must_use]
    pub const fn available(&self) -> SignedUsdAmount {
        self.available
    }

    #[must_use]
    pub const fn on_hold(&self) -> SignedUsdAmount {
        self.on_hold
    }
}

impl fmt::Debug for Balance {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Balance")
            .field("available", &"[REDACTED]")
            .field("on_hold", &"[REDACTED]")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    #[test]
    fn signed_usd_amount_parses_zero_positive_and_negative_values() -> Result<(), Box<dyn Error>> {
        for (input, cents, rendered) in [
            ("0.00", 0, "$0.00"),
            ("12.3", 1_230, "$12.30"),
            ("-4.56", -456, "-$4.56"),
        ] {
            let amount = SignedUsdAmount::from_str(input)?;
            assert_eq!(amount.cents(), cents);
            assert_eq!(amount.to_string(), rendered);
        }
        Ok(())
    }

    #[test]
    fn signed_usd_amount_rejects_lossy_or_oversized_values() {
        for value in ["", "+1.00", ".50", "1.", "1.001", "NaN", "１.00", " 1.00"] {
            assert_eq!(
                SignedUsdAmount::from_str(value),
                Err(SignedUsdAmountParseError::InvalidSyntax),
                "misclassified {value}"
            );
        }
        for value in [
            "92233720368547758.08",
            "-92233720368547758.09",
            "999999999999999999999999.00",
        ] {
            assert_eq!(
                SignedUsdAmount::from_str(value),
                Err(SignedUsdAmountParseError::TooLarge),
                "misclassified {value}"
            );
        }
    }

    #[test]
    fn signed_usd_amount_covers_exact_extrema_sign_and_negative_zero() -> Result<(), Box<dyn Error>>
    {
        for (input, cents, rendered) in [
            ("92233720368547758.07", i64::MAX, "$92233720368547758.07"),
            ("-92233720368547758.08", i64::MIN, "-$92233720368547758.08"),
            ("-0", 0, "$0.00"),
            ("-0.00", 0, "$0.00"),
        ] {
            let amount = SignedUsdAmount::from_str(input)?;
            assert_eq!(amount.cents(), cents);
            assert_eq!(amount.to_string(), rendered);
        }
        Ok(())
    }
}

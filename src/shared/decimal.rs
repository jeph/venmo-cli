#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DecimalMagnitudeError {
    InvalidSyntax,
    TooLarge,
}

pub(crate) fn parse_decimal_magnitude(value: &str) -> Result<u128, DecimalMagnitudeError> {
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
        return Err(DecimalMagnitudeError::InvalidSyntax);
    }

    let whole_cents = whole
        .parse::<u128>()
        .map_err(|_| DecimalMagnitudeError::TooLarge)?
        .checked_mul(100)
        .ok_or(DecimalMagnitudeError::TooLarge)?;
    let fractional_cents = match fractional {
        None => 0,
        Some(digits) if digits.len() == 1 => u128::from(digits.as_bytes()[0] - b'0') * 10,
        Some(digits) => {
            u128::from(digits.as_bytes()[0] - b'0') * 10 + u128::from(digits.as_bytes()[1] - b'0')
        }
    };

    whole_cents
        .checked_add(fractional_cents)
        .ok_or(DecimalMagnitudeError::TooLarge)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_magnitudes_share_exact_cent_syntax() {
        for (input, expected) in [
            ("0", 0),
            ("0.0", 0),
            ("12.3", 1_230),
            ("12.34", 1_234),
            ("0001.02", 102),
        ] {
            assert_eq!(parse_decimal_magnitude(input), Ok(expected));
        }

        for input in ["", ".1", "1.", "1.234", "+1", "-1", "1e2", "1 0"] {
            assert_eq!(
                parse_decimal_magnitude(input),
                Err(DecimalMagnitudeError::InvalidSyntax),
                "accepted {input:?}"
            );
        }
        assert_eq!(
            parse_decimal_magnitude("999999999999999999999999999999999999999999"),
            Err(DecimalMagnitudeError::TooLarge)
        );
    }
}

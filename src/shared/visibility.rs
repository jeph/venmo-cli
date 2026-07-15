use std::fmt;
use std::str::FromStr;

use thiserror::Error;

/// Visibility applied when creating a payment or request.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum Visibility {
    #[default]
    Private,
    Friends,
    Public,
}

impl Visibility {
    /// Returns the exact value used by Venmo's wire protocol.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Friends => "friends",
            Self::Public => "public",
        }
    }

    /// Returns whether this visibility is at least as restrictive as `other`.
    #[must_use]
    pub(crate) const fn is_at_least_as_restrictive_as(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Private, _)
                | (Self::Friends, Self::Friends | Self::Public)
                | (Self::Public, Self::Public)
        )
    }
}

impl fmt::Display for Visibility {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for Visibility {
    type Err = VisibilityParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "private" => Ok(Self::Private),
            "friends" => Ok(Self::Friends),
            "public" => Ok(Self::Public),
            _ => Err(VisibilityParseError),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("visibility must be one of: private, friends, public")]
pub struct VisibilityParseError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_values_round_trip() {
        for visibility in [Visibility::Private, Visibility::Friends, Visibility::Public] {
            assert_eq!(visibility.as_str().parse(), Ok(visibility));
            assert_eq!(visibility.to_string(), visibility.as_str());
        }
        assert!("PRIVATE".parse::<Visibility>().is_err());
    }

    #[test]
    fn restriction_order_matches_venmo_privacy_semantics() {
        for (requested, accepted) in [
            (Visibility::Private, [true, false, false]),
            (Visibility::Friends, [true, true, false]),
            (Visibility::Public, [true, true, true]),
        ] {
            for (actual, accepted) in [Visibility::Private, Visibility::Friends, Visibility::Public]
                .into_iter()
                .zip(accepted)
            {
                assert_eq!(actual.is_at_least_as_restrictive_as(requested), accepted);
            }
        }
    }
}

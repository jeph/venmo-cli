use thiserror::Error;

const MAX_OPAQUE_ID_BYTES: usize = 512;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error(
    "{kind} must be non-empty, at most 512 bytes, and contain no whitespace or control characters"
)]
pub struct OpaqueIdParseError {
    kind: &'static str,
}

pub(crate) fn validate_opaque_id(
    value: &str,
    kind: &'static str,
) -> Result<(), OpaqueIdParseError> {
    if value.is_empty()
        || value.len() > MAX_OPAQUE_ID_BYTES
        || value.chars().any(|character| {
            character.is_whitespace()
                || character.is_control()
                || is_invisible_format_control(character)
        })
    {
        return Err(OpaqueIdParseError { kind });
    }
    Ok(())
}

fn is_invisible_format_control(character: char) -> bool {
    matches!(
        character,
        '\u{00AD}'
            | '\u{061C}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{FEFF}'
    )
}

macro_rules! opaque_id {
    ($name:ident, $kind:literal) => {
        #[derive(Clone, Eq, Hash, PartialEq)]
        pub struct $name(String);

        impl $name {
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(concat!(stringify!($name), "([REDACTED])"))
            }
        }

        impl std::str::FromStr for $name {
            type Err = $crate::shared::OpaqueIdParseError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                $crate::shared::opaque_id::validate_opaque_id(value, $kind)?;
                Ok(Self(value.to_owned()))
            }
        }
    };
}

pub(crate) use opaque_id;

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    opaque_id!(TestId, "test ID");

    #[test]
    fn opaque_ids_are_bounded_and_reject_invisible_controls()
    -> Result<(), Box<dyn std::error::Error>> {
        let id = TestId::from_str("request-1")?;
        assert_eq!(id.as_str(), "request-1");
        let exact_ascii = "x".repeat(MAX_OPAQUE_ID_BYTES);
        let exact_unicode = "é".repeat(MAX_OPAQUE_ID_BYTES / 2);
        assert_eq!(
            TestId::from_str(&exact_ascii)?.as_str().len(),
            MAX_OPAQUE_ID_BYTES
        );
        assert_eq!(
            TestId::from_str(&exact_unicode)?.as_str().len(),
            MAX_OPAQUE_ID_BYTES
        );
        assert!(TestId::from_str(&"x".repeat(MAX_OPAQUE_ID_BYTES + 1)).is_err());
        assert!(TestId::from_str(&"é".repeat(MAX_OPAQUE_ID_BYTES / 2 + 1)).is_err());
        assert!(TestId::from_str("request\n1").is_err());
        assert!(TestId::from_str("request\u{a0}1").is_err());
        assert!(TestId::from_str("request\u{202E}1").is_err());
        assert!(TestId::from_str("request\u{200B}1").is_err());
        assert!(TestId::from_str("request\u{061C}1").is_err());
        Ok(())
    }
}

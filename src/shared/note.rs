use std::fmt;
use std::str::FromStr;

use thiserror::Error;

#[derive(Clone, Eq, PartialEq)]
pub struct Note(String);

impl Note {
    pub fn new(value: String) -> Result<Self, NoteParseError> {
        if !value.chars().any(|character| !character.is_whitespace()) {
            return Err(NoteParseError);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Note {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl fmt::Debug for Note {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Note([REDACTED])")
    }
}

impl FromStr for Note {
    type Err = NoteParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value.to_owned())
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("note must contain non-whitespace text")]
pub struct NoteParseError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_keep_the_evidence_gated_non_whitespace_only_contract() {
        for value in ["", " ", "\t\n", "\u{a0}"] {
            assert_eq!(Note::from_str(value), Err(NoteParseError));
        }
        for value in ["Dinner", "🍜", "  memo  "] {
            assert_eq!(
                Note::from_str(value).map(|note| note.as_str().to_owned()),
                Ok(value.to_owned())
            );
        }

        let long_note = "x".repeat(64 * 1024 + 1);
        assert_eq!(
            Note::from_str(&long_note).map(|note| note.as_str().len()),
            Ok(long_note.len())
        );
    }
}

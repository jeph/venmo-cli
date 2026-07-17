use std::collections::HashSet;

use thiserror::Error;

use super::{TransferInstrument, TransferOptions};
use crate::shared::ApplicationFailureKind;

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TransferSelectionError {
    #[error("no standard outbound transfer destination is available")]
    NoEligibleDestinations,

    #[error("the standard outbound options contain an unsupported instrument type")]
    UnsupportedInstrumentType,

    #[error("the standard outbound options contain fee fields whose units are not established")]
    UnsupportedFeeMetadata,

    #[error("the transfer-options response contained duplicate destination IDs")]
    DuplicateDestinationIds,

    #[error("the transfer-options response marked multiple standard destinations as default")]
    MultipleDefaults,

    #[error(
        "automatic transfer destination selection is ambiguous: multiple standard destinations are available and none is the unique default"
    )]
    AmbiguousAutomaticSelection,
}

impl TransferSelectionError {
    #[must_use]
    pub const fn failure_kind(self) -> ApplicationFailureKind {
        match self {
            Self::NoEligibleDestinations
            | Self::UnsupportedInstrumentType
            | Self::UnsupportedFeeMetadata
            | Self::AmbiguousAutomaticSelection => ApplicationFailureKind::Usage,
            Self::DuplicateDestinationIds | Self::MultipleDefaults => {
                ApplicationFailureKind::ApiContract
            }
        }
    }
}

pub(crate) fn select_standard_out(
    options: &TransferOptions,
) -> Result<TransferInstrument, TransferSelectionError> {
    let mode = options.standard();
    if !mode.fee().is_empty() {
        return Err(TransferSelectionError::UnsupportedFeeMetadata);
    }
    let destinations = mode.eligible_destinations();
    if destinations.is_empty() {
        return Err(TransferSelectionError::NoEligibleDestinations);
    }
    if destinations
        .iter()
        .any(|destination| destination.instrument_type() != "bank")
    {
        return Err(TransferSelectionError::UnsupportedInstrumentType);
    }

    let mut ids = HashSet::with_capacity(destinations.len());
    let mut duplicate = false;
    let mut defaults = 0_usize;
    for destination in destinations {
        if !ids.insert(destination.id()) {
            duplicate = true;
        }
        defaults += usize::from(destination.is_default());
    }
    if duplicate {
        return Err(TransferSelectionError::DuplicateDestinationIds);
    }
    if defaults > 1 {
        return Err(TransferSelectionError::MultipleDefaults);
    }
    if let Some(destination) = destinations
        .iter()
        .find(|destination| destination.is_default())
    {
        return Ok(destination.clone());
    }
    if let [destination] = destinations {
        return Ok(destination.clone());
    }
    Err(TransferSelectionError::AmbiguousAutomaticSelection)
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::str::FromStr;

    use super::*;
    use crate::features::transfers::{
        TransferFeeMetadata, TransferInstrumentId, TransferInstrumentSuffix, TransferModeOptions,
        TransferSpeed,
    };

    type TestResult = Result<(), Box<dyn Error>>;

    #[test]
    fn selection_chooses_unique_default_or_sole_candidate_without_using_order() -> TestResult {
        for (destinations, expected_id) in [
            (
                vec![
                    instrument("backup", false, "bank")?,
                    instrument("default", true, "bank")?,
                ],
                "default",
            ),
            (vec![instrument("sole", false, "bank")?], "sole"),
        ] {
            let options = options(destinations, TransferFeeMetadata::default());
            assert_eq!(select_standard_out(&options)?.id().as_str(), expected_id);
        }
        Ok(())
    }

    #[test]
    fn selection_fails_closed_for_every_unsupported_or_ambiguous_branch() -> TestResult {
        let cases = [
            (
                options(Vec::new(), TransferFeeMetadata::default()),
                TransferSelectionError::NoEligibleDestinations,
            ),
            (
                options(
                    vec![instrument("card", true, "card")?],
                    TransferFeeMetadata::default(),
                ),
                TransferSelectionError::UnsupportedInstrumentType,
            ),
            (
                options(
                    vec![instrument("bank", true, "bank")?],
                    TransferFeeMetadata::new(Some("25".to_owned()), None, None, false),
                ),
                TransferSelectionError::UnsupportedFeeMetadata,
            ),
            (
                options(
                    vec![
                        instrument("same", false, "bank")?,
                        instrument("same", true, "bank")?,
                    ],
                    TransferFeeMetadata::default(),
                ),
                TransferSelectionError::DuplicateDestinationIds,
            ),
            (
                options(
                    vec![
                        instrument("one", true, "bank")?,
                        instrument("two", true, "bank")?,
                    ],
                    TransferFeeMetadata::default(),
                ),
                TransferSelectionError::MultipleDefaults,
            ),
            (
                options(
                    vec![
                        instrument("one", false, "bank")?,
                        instrument("two", false, "bank")?,
                    ],
                    TransferFeeMetadata::default(),
                ),
                TransferSelectionError::AmbiguousAutomaticSelection,
            ),
        ];

        for (options, expected) in cases {
            assert_eq!(select_standard_out(&options), Err(expected));
        }
        Ok(())
    }

    fn instrument(
        id: &str,
        is_default: bool,
        kind: &str,
    ) -> Result<TransferInstrument, Box<dyn Error>> {
        Ok(TransferInstrument::new(
            TransferInstrumentId::from_str(id)?,
            "Bank".to_owned(),
            "Checking".to_owned(),
            kind.to_owned(),
            TransferInstrumentSuffix::from_str("1234")?,
            is_default,
            "Soon".to_owned(),
        ))
    }

    fn options(destinations: Vec<TransferInstrument>, fee: TransferFeeMetadata) -> TransferOptions {
        let standard = TransferModeOptions::new(Vec::new(), destinations, fee, "Soon".to_owned());
        let instant = TransferModeOptions::new(
            Vec::new(),
            Vec::new(),
            TransferFeeMetadata::default(),
            "Soon".to_owned(),
        );
        TransferOptions::new(None, Some(TransferSpeed::Standard), standard, instant)
    }
}

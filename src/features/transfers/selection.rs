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

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TransferInSelectionError {
    #[error("no standard inbound transfer source is available")]
    NoEligibleSources,

    #[error("the selected source is not currently eligible for a standard inbound transfer")]
    SourceNotEligible,

    #[error("the standard inbound options contain an unsupported instrument type")]
    UnsupportedInstrumentType,

    #[error("the standard inbound options contain fee fields whose units are not established")]
    UnsupportedFeeMetadata,

    #[error("the transfer-options response contained duplicate source IDs")]
    DuplicateSourceIds,

    #[error("the transfer-options response marked multiple standard sources as default")]
    MultipleDefaults,

    #[error("no standard inbound source is marked as default; pass `--source <SOURCE_ID>`")]
    NoDefaultSource,
}

impl TransferInSelectionError {
    #[must_use]
    pub const fn failure_kind(self) -> ApplicationFailureKind {
        match self {
            Self::NoEligibleSources
            | Self::SourceNotEligible
            | Self::UnsupportedInstrumentType
            | Self::UnsupportedFeeMetadata
            | Self::NoDefaultSource => ApplicationFailureKind::Usage,
            Self::DuplicateSourceIds | Self::MultipleDefaults => {
                ApplicationFailureKind::ApiContract
            }
        }
    }
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

pub(crate) fn select_standard_in(
    options: &TransferOptions,
    requested_source: Option<&super::TransferInstrumentId>,
) -> Result<TransferInstrument, TransferInSelectionError> {
    let mode = options.standard();
    if !mode.fee().is_empty() {
        return Err(TransferInSelectionError::UnsupportedFeeMetadata);
    }
    let sources = mode.eligible_sources();
    if sources.is_empty() {
        return Err(TransferInSelectionError::NoEligibleSources);
    }
    if sources
        .iter()
        .any(|source| source.instrument_type() != "bank")
    {
        return Err(TransferInSelectionError::UnsupportedInstrumentType);
    }

    let mut ids = HashSet::with_capacity(sources.len());
    if sources.iter().any(|source| !ids.insert(source.id())) {
        return Err(TransferInSelectionError::DuplicateSourceIds);
    }
    let defaults = sources.iter().filter(|source| source.is_default()).count();
    if defaults > 1 {
        return Err(TransferInSelectionError::MultipleDefaults);
    }

    if let Some(requested_source) = requested_source {
        return sources
            .iter()
            .find(|source| source.id() == requested_source)
            .cloned()
            .ok_or(TransferInSelectionError::SourceNotEligible);
    }
    sources
        .iter()
        .find(|source| source.is_default())
        .cloned()
        .ok_or(TransferInSelectionError::NoDefaultSource)
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

    #[test]
    fn inbound_selection_uses_explicit_id_or_exactly_one_marked_default() -> TestResult {
        let sources = vec![
            instrument("backup", false, "bank")?,
            instrument("default", true, "bank")?,
        ];
        let options = inbound_options(sources, TransferFeeMetadata::default());

        assert_eq!(select_standard_in(&options, None)?.id().as_str(), "default");
        let requested = TransferInstrumentId::from_str("backup")?;
        assert_eq!(
            select_standard_in(&options, Some(&requested))?
                .id()
                .as_str(),
            "backup"
        );
        Ok(())
    }

    #[test]
    fn inbound_selection_fails_closed_for_unavailable_unsupported_and_ambiguous_sources()
    -> TestResult {
        let missing = TransferInstrumentId::from_str("missing")?;
        let cases = [
            (
                inbound_options(Vec::new(), TransferFeeMetadata::default()),
                None,
                TransferInSelectionError::NoEligibleSources,
            ),
            (
                inbound_options(
                    vec![instrument("card", true, "card")?],
                    TransferFeeMetadata::default(),
                ),
                None,
                TransferInSelectionError::UnsupportedInstrumentType,
            ),
            (
                inbound_options(
                    vec![instrument("bank", true, "bank")?],
                    TransferFeeMetadata::new(Some("25".to_owned()), None, None, false),
                ),
                None,
                TransferInSelectionError::UnsupportedFeeMetadata,
            ),
            (
                inbound_options(
                    vec![
                        instrument("same", false, "bank")?,
                        instrument("same", true, "bank")?,
                    ],
                    TransferFeeMetadata::default(),
                ),
                None,
                TransferInSelectionError::DuplicateSourceIds,
            ),
            (
                inbound_options(
                    vec![
                        instrument("one", true, "bank")?,
                        instrument("two", true, "bank")?,
                    ],
                    TransferFeeMetadata::default(),
                ),
                None,
                TransferInSelectionError::MultipleDefaults,
            ),
            (
                inbound_options(
                    vec![instrument("sole", false, "bank")?],
                    TransferFeeMetadata::default(),
                ),
                None,
                TransferInSelectionError::NoDefaultSource,
            ),
            (
                inbound_options(
                    vec![instrument("default", true, "bank")?],
                    TransferFeeMetadata::default(),
                ),
                Some(&missing),
                TransferInSelectionError::SourceNotEligible,
            ),
        ];

        for (options, requested, expected) in cases {
            assert_eq!(select_standard_in(&options, requested), Err(expected));
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

    fn inbound_options(
        sources: Vec<TransferInstrument>,
        fee: TransferFeeMetadata,
    ) -> TransferOptions {
        let standard = TransferModeOptions::new(sources, Vec::new(), fee, "Soon".to_owned());
        let instant = TransferModeOptions::new(
            Vec::new(),
            Vec::new(),
            TransferFeeMetadata::default(),
            "Soon".to_owned(),
        );
        TransferOptions::new(None, Some(TransferSpeed::Standard), standard, instant)
    }
}

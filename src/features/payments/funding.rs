use std::collections::HashSet;

use thiserror::Error;

use super::PeerFundingMethod;
use crate::shared::ApplicationFailureKind;

#[derive(Debug, Error)]
pub enum FundingSelectionError {
    #[error("no peer-payment funding method is available")]
    NoEligibleMethods,

    #[error("the payment-method response contained duplicate IDs")]
    DuplicateMethodIds,

    #[error("the payment-method response marked multiple eligible methods as default")]
    MultipleDefaults,

    #[error(
        "automatic funding selection is ambiguous: multiple eligible payment methods are available and none is the unique default"
    )]
    AmbiguousAutomaticSelection,
}

impl FundingSelectionError {
    #[must_use]
    pub const fn failure_kind(&self) -> ApplicationFailureKind {
        match self {
            Self::NoEligibleMethods | Self::AmbiguousAutomaticSelection => {
                ApplicationFailureKind::Usage
            }
            Self::DuplicateMethodIds | Self::MultipleDefaults => {
                ApplicationFailureKind::ApiContract
            }
        }
    }
}

pub(crate) fn select(
    eligible_methods: &[PeerFundingMethod],
) -> Result<PeerFundingMethod, FundingSelectionError> {
    validate_method_contract(eligible_methods)?;
    if eligible_methods.is_empty() {
        return Err(FundingSelectionError::NoEligibleMethods);
    }

    if let Some(method) = eligible_methods.iter().find(|method| method.is_default()) {
        return Ok(method.clone());
    }

    if let [method] = eligible_methods {
        return Ok(method.clone());
    }

    Err(FundingSelectionError::AmbiguousAutomaticSelection)
}

fn validate_method_contract(methods: &[PeerFundingMethod]) -> Result<(), FundingSelectionError> {
    let mut ids = HashSet::with_capacity(methods.len());
    let mut duplicate_id = false;
    let mut default_count = 0_usize;
    for method in methods {
        if !ids.insert(method.method().id()) {
            duplicate_id = true;
        }
        default_count += usize::from(method.is_default());
    }

    if duplicate_id {
        return Err(FundingSelectionError::DuplicateMethodIds);
    }
    if default_count > 1 {
        return Err(FundingSelectionError::MultipleDefaults);
    }
    Ok(())
}

#[cfg(test)]
#[path = "funding_tests.rs"]
mod tests;

use std::collections::HashSet;

use thiserror::Error;

use super::{PeerFundingMethod, PeerFundingSource, PeerFundingSourceSelection, PeerFundingSources};
use crate::features::wallet::{Balance, PaymentMethodId};
use crate::shared::{ApplicationFailureKind, Money};

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

    #[error("the requested payment source is not available for peer payments")]
    RequestedSourceUnavailable,

    #[error("the requested Venmo balance source does not cover the full amount")]
    RequestedBalanceInsufficient,

    #[error("the payment-method response omitted the peer-eligible Venmo balance source")]
    MissingBalanceSource,
}

impl FundingSelectionError {
    #[must_use]
    pub const fn failure_kind(&self) -> ApplicationFailureKind {
        match self {
            Self::NoEligibleMethods
            | Self::AmbiguousAutomaticSelection
            | Self::RequestedSourceUnavailable
            | Self::RequestedBalanceInsufficient => ApplicationFailureKind::Usage,
            Self::DuplicateMethodIds | Self::MultipleDefaults | Self::MissingBalanceSource => {
                ApplicationFailureKind::ApiContract
            }
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct SelectedFundingSource {
    source: PeerFundingSource,
    selection: PeerFundingSourceSelection,
}

impl SelectedFundingSource {
    #[must_use]
    pub(crate) fn into_parts(self) -> (PeerFundingSource, PeerFundingSourceSelection) {
        (self.source, self.selection)
    }
}

pub(crate) fn select_source(
    sources: &PeerFundingSources,
    balance: &Balance,
    amount: Money,
    requested_source: Option<&PaymentMethodId>,
) -> Result<SelectedFundingSource, FundingSelectionError> {
    validate_unique_ids(sources.external())?;
    if let Some(requested_source) = requested_source {
        if let Some(method) = sources
            .balance()
            .filter(|method| method.id() == requested_source)
        {
            if !balance_covers(balance, amount) {
                return Err(FundingSelectionError::RequestedBalanceInsufficient);
            }
            return Ok(SelectedFundingSource {
                source: PeerFundingSource::balance(method.clone()),
                selection: PeerFundingSourceSelection::Explicit,
            });
        }
        if let Some(method) = sources
            .external()
            .iter()
            .find(|method| method.method().id() == requested_source)
        {
            return Ok(SelectedFundingSource {
                source: PeerFundingSource::external(method.clone()),
                selection: PeerFundingSourceSelection::Explicit,
            });
        }
        return Err(FundingSelectionError::RequestedSourceUnavailable);
    }
    if balance_covers(balance, amount) {
        let method = sources
            .balance()
            .ok_or(FundingSelectionError::MissingBalanceSource)?;
        return Ok(SelectedFundingSource {
            source: PeerFundingSource::balance(method.clone()),
            selection: PeerFundingSourceSelection::Automatic,
        });
    }
    Ok(SelectedFundingSource {
        source: PeerFundingSource::external(select_external(sources.external())?),
        selection: PeerFundingSourceSelection::Automatic,
    })
}

fn select_external(
    eligible_methods: &[PeerFundingMethod],
) -> Result<PeerFundingMethod, FundingSelectionError> {
    validate_unique_ids(eligible_methods)?;
    if eligible_methods.is_empty() {
        return Err(FundingSelectionError::NoEligibleMethods);
    }

    if eligible_methods
        .iter()
        .filter(|method| method.is_default())
        .count()
        > 1
    {
        return Err(FundingSelectionError::MultipleDefaults);
    }

    if let Some(method) = eligible_methods.iter().find(|method| method.is_default()) {
        return Ok(method.clone());
    }

    if let [method] = eligible_methods {
        return Ok(method.clone());
    }

    Err(FundingSelectionError::AmbiguousAutomaticSelection)
}

fn balance_covers(balance: &Balance, amount: Money) -> bool {
    u64::try_from(balance.available().cents())
        .is_ok_and(|available_cents| available_cents >= amount.cents())
}

fn validate_unique_ids(methods: &[PeerFundingMethod]) -> Result<(), FundingSelectionError> {
    let mut ids = HashSet::with_capacity(methods.len());
    let mut duplicate_id = false;
    for method in methods {
        if !ids.insert(method.method().id()) {
            duplicate_id = true;
        }
    }

    if duplicate_id {
        return Err(FundingSelectionError::DuplicateMethodIds);
    }
    Ok(())
}

#[cfg(test)]
#[path = "funding_tests.rs"]
mod tests;

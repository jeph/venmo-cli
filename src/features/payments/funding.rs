use std::collections::HashSet;

use thiserror::Error;

use super::{FundingChoiceSelection, PeerFundingMethod};
use crate::features::auth::{PromptError, prompt_failure_kind};
use crate::features::wallet::PaymentMethodId;
use crate::shared::ApplicationFailureKind;

#[derive(Debug, Error)]
pub enum FundingSelectionError {
    #[error("no peer-payment funding method is available")]
    NoEligibleMethods,

    #[error("the requested payment-method ID was not found among eligible methods")]
    ExplicitMethodUnavailable,

    #[error("the payment-method response contained duplicate IDs")]
    DuplicateMethodIds,

    #[error("the payment-method response marked multiple eligible methods as default")]
    MultipleDefaults,

    #[error(
        "multiple eligible payment methods are available; pass `--from <METHOD_ID>` for non-interactive use"
    )]
    ExplicitMethodRequired,

    #[error("payment-method selection failed: {source}")]
    Prompt {
        #[source]
        source: PromptError,
    },
}

impl FundingSelectionError {
    #[must_use]
    pub const fn failure_kind(&self) -> ApplicationFailureKind {
        match self {
            Self::NoEligibleMethods
            | Self::ExplicitMethodUnavailable
            | Self::ExplicitMethodRequired => ApplicationFailureKind::Usage,
            Self::DuplicateMethodIds | Self::MultipleDefaults => {
                ApplicationFailureKind::ApiContract
            }
            Self::Prompt { source } => prompt_failure_kind(source),
        }
    }
}

pub(crate) fn select<P>(
    prompt: &P,
    eligible_methods: &[PeerFundingMethod],
    requested_id: Option<&PaymentMethodId>,
) -> Result<PeerFundingMethod, FundingSelectionError>
where
    P: FundingChoiceSelection,
{
    validate_method_contract(eligible_methods)?;
    if eligible_methods.is_empty() {
        return Err(FundingSelectionError::NoEligibleMethods);
    }

    if let Some(requested_id) = requested_id {
        return eligible_methods
            .iter()
            .find(|method| requested_id == method.method().id())
            .cloned()
            .ok_or(FundingSelectionError::ExplicitMethodUnavailable);
    }

    if let Some(method) = eligible_methods.iter().find(|method| method.is_default()) {
        return Ok(method.clone());
    }

    if let [method] = eligible_methods {
        return Ok(method.clone());
    }

    if !prompt.can_prompt() {
        return Err(FundingSelectionError::ExplicitMethodRequired);
    }
    let choices = eligible_methods
        .iter()
        .map(|method| method.method())
        .collect::<Vec<_>>();
    let index = prompt
        .select_funding_choice("Choose a payment method", &choices)
        .map_err(|source| FundingSelectionError::Prompt { source })?;
    let method = eligible_methods
        .get(index)
        .cloned()
        .ok_or(FundingSelectionError::Prompt {
            source: PromptError::InvalidSelection {
                index,
                choice_count: eligible_methods.len(),
            },
        })?;
    Ok(method)
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

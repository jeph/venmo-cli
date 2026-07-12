use thiserror::Error;

use super::ports::{PromptError, PromptPort};
use crate::domain::{PaymentMethod, PaymentMethodId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FundingSelectionDisposition {
    Explicit,
    Default,
    SoleEligibleMethod,
    Interactive,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundingSelection {
    method: PaymentMethod,
    disposition: FundingSelectionDisposition,
}

impl FundingSelection {
    #[must_use]
    pub fn method(&self) -> &PaymentMethod {
        &self.method
    }

    #[must_use]
    pub const fn disposition(&self) -> FundingSelectionDisposition {
        self.disposition
    }
}

#[derive(Debug, Error)]
pub enum FundingSelectionError {
    #[error("no eligible Venmo payment method is available")]
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

pub fn select<P>(
    prompt: &P,
    eligible_methods: &[PaymentMethod],
    requested_id: Option<&PaymentMethodId>,
) -> Result<FundingSelection, FundingSelectionError>
where
    P: PromptPort,
{
    validate_unique_ids(eligible_methods)?;
    if eligible_methods.is_empty() {
        return Err(FundingSelectionError::NoEligibleMethods);
    }

    if let Some(requested_id) = requested_id {
        let method = eligible_methods
            .iter()
            .find(|method| method.id() == requested_id)
            .cloned()
            .ok_or(FundingSelectionError::ExplicitMethodUnavailable)?;
        return Ok(FundingSelection {
            method,
            disposition: FundingSelectionDisposition::Explicit,
        });
    }

    let mut defaults = eligible_methods.iter().filter(|method| method.is_default());
    let default = defaults.next();
    if defaults.next().is_some() {
        return Err(FundingSelectionError::MultipleDefaults);
    }
    if let Some(method) = default {
        return Ok(FundingSelection {
            method: method.clone(),
            disposition: FundingSelectionDisposition::Default,
        });
    }

    if let [method] = eligible_methods {
        return Ok(FundingSelection {
            method: method.clone(),
            disposition: FundingSelectionDisposition::SoleEligibleMethod,
        });
    }

    if !prompt.can_prompt() {
        return Err(FundingSelectionError::ExplicitMethodRequired);
    }
    let labels = eligible_methods
        .iter()
        .map(payment_method_label)
        .collect::<Vec<_>>();
    let index = prompt
        .select("Choose a payment method", &labels)
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
    Ok(FundingSelection {
        method,
        disposition: FundingSelectionDisposition::Interactive,
    })
}

fn validate_unique_ids(methods: &[PaymentMethod]) -> Result<(), FundingSelectionError> {
    for (index, method) in methods.iter().enumerate() {
        if methods[..index]
            .iter()
            .any(|candidate| candidate.id() == method.id())
        {
            return Err(FundingSelectionError::DuplicateMethodIds);
        }
    }
    Ok(())
}

fn payment_method_label(method: &PaymentMethod) -> String {
    let mut details = Vec::new();
    if let Some(method_type) = method.method_type() {
        details.push(method_type.to_owned());
    }
    if let Some(last_four) = method.last_four() {
        details.push(format!("ending {last_four}"));
    }
    let name = method.name().unwrap_or("Payment method");
    if details.is_empty() {
        format!("{name} [ID {}]", method.id())
    } else {
        format!("{name} ({}) [ID {}]", details.join(", "), method.id())
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::str::FromStr;

    use super::*;
    use crate::domain::{AccessToken, AccountPassword, DeviceId, LoginIdentifier, OtpCode};

    type TestResult = Result<(), Box<dyn Error>>;

    struct FakePrompt {
        interactive: bool,
        selection: usize,
    }

    impl PromptPort for FakePrompt {
        fn can_prompt(&self) -> bool {
            self.interactive
        }

        fn read_login_identifier(&self, _prompt: &str) -> Result<LoginIdentifier, PromptError> {
            Err(PromptError::Cancelled)
        }

        fn read_account_password(&self, _prompt: &str) -> Result<AccountPassword, PromptError> {
            Err(PromptError::Cancelled)
        }

        fn read_otp_code(&self, _prompt: &str) -> Result<OtpCode, PromptError> {
            Err(PromptError::Cancelled)
        }

        fn read_access_token(&self, _prompt: &str) -> Result<AccessToken, PromptError> {
            Err(PromptError::Cancelled)
        }

        fn read_device_id(&self, _prompt: &str) -> Result<DeviceId, PromptError> {
            Err(PromptError::Cancelled)
        }

        fn confirm_default_no(&self, _prompt: &str) -> Result<bool, PromptError> {
            Err(PromptError::Cancelled)
        }

        fn select(&self, _prompt: &str, _items: &[String]) -> Result<usize, PromptError> {
            Ok(self.selection)
        }
    }

    #[test]
    fn explicit_method_takes_precedence() -> TestResult {
        let methods = vec![method("one", true)?, method("two", false)?];
        let requested = PaymentMethodId::from_str("two")?;

        let selected = select(&noninteractive_prompt(), &methods, Some(&requested))?;

        assert_eq!(selected.method().id(), &requested);
        assert_eq!(
            selected.disposition(),
            FundingSelectionDisposition::Explicit
        );
        Ok(())
    }

    #[test]
    fn unique_default_and_sole_method_are_deterministic() -> TestResult {
        let default = select(
            &noninteractive_prompt(),
            &[method("one", false)?, method("two", true)?],
            None,
        )?;
        let sole = select(&noninteractive_prompt(), &[method("only", false)?], None)?;

        assert_eq!(default.method().id().as_str(), "two");
        assert_eq!(default.disposition(), FundingSelectionDisposition::Default);
        assert_eq!(sole.method().id().as_str(), "only");
        assert_eq!(
            sole.disposition(),
            FundingSelectionDisposition::SoleEligibleMethod
        );
        Ok(())
    }

    #[test]
    fn malformed_or_nondeterministic_lists_fail_closed() -> TestResult {
        let duplicate = vec![method("same", false)?, method("same", false)?];
        let multiple_defaults = vec![method("one", true)?, method("two", true)?];
        let multiple = vec![method("one", false)?, method("two", false)?];

        assert!(matches!(
            select(&noninteractive_prompt(), &[], None),
            Err(FundingSelectionError::NoEligibleMethods)
        ));
        assert!(matches!(
            select(&noninteractive_prompt(), &duplicate, None),
            Err(FundingSelectionError::DuplicateMethodIds)
        ));
        assert!(matches!(
            select(&noninteractive_prompt(), &multiple_defaults, None),
            Err(FundingSelectionError::MultipleDefaults)
        ));
        assert!(matches!(
            select(&noninteractive_prompt(), &multiple, None),
            Err(FundingSelectionError::ExplicitMethodRequired)
        ));
        Ok(())
    }

    #[test]
    fn interactive_selection_returns_the_chosen_method() -> TestResult {
        let prompt = FakePrompt {
            interactive: true,
            selection: 1,
        };
        let methods = vec![method("one", false)?, method("two", false)?];

        let selected = select(&prompt, &methods, None)?;

        assert_eq!(selected.method().id().as_str(), "two");
        assert_eq!(
            selected.disposition(),
            FundingSelectionDisposition::Interactive
        );
        Ok(())
    }

    fn noninteractive_prompt() -> FakePrompt {
        FakePrompt {
            interactive: false,
            selection: 0,
        }
    }

    fn method(id: &str, is_default: bool) -> Result<PaymentMethod, Box<dyn Error>> {
        Ok(PaymentMethod::new(
            PaymentMethodId::from_str(id)?,
            Some(format!("Method {id}")),
            Some("synthetic".to_owned()),
            Some("1234".to_owned()),
            is_default,
        ))
    }
}

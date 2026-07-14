use std::io::{self, IsTerminal};

use dialoguer::console::Term;
use dialoguer::theme::SimpleTheme;
use dialoguer::{Confirm, Input, Password, Select};

use super::output::sanitize_terminal_text;
use crate::features::auth::{
    AccountPassword, AuthenticationInput, LoginIdentifier, OtpCode, PromptAvailability, PromptError,
};
use crate::features::payments::{DefaultNoConfirmation, FundingChoiceSelection};
use crate::features::wallet::PaymentMethod;
use crate::shared::{AccessToken, DeviceId};

/// Immutable snapshot of the process streams relevant to safe prompting.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TerminalCapabilities {
    stdin_is_terminal: bool,
    stderr_is_terminal: bool,
}

impl TerminalCapabilities {
    #[cfg(test)]
    #[must_use]
    pub(super) const fn new(stdin_is_terminal: bool, stderr_is_terminal: bool) -> Self {
        Self {
            stdin_is_terminal,
            stderr_is_terminal,
        }
    }

    #[must_use]
    pub(super) fn from_process() -> Self {
        Self {
            stdin_is_terminal: io::stdin().is_terminal(),
            stderr_is_terminal: io::stderr().is_terminal(),
        }
    }

    #[must_use]
    pub(super) const fn can_prompt(self) -> bool {
        self.stdin_is_terminal && self.stderr_is_terminal
    }
}

pub(super) struct DialoguerPrompt {
    term: Term,
    terminal_capabilities: TerminalCapabilities,
}

impl DialoguerPrompt {
    #[must_use]
    pub(super) fn new(terminal_capabilities: TerminalCapabilities) -> Self {
        Self {
            term: Term::stderr(),
            terminal_capabilities,
        }
    }
}

impl Default for DialoguerPrompt {
    fn default() -> Self {
        Self::new(TerminalCapabilities::from_process())
    }
}

impl PromptAvailability for DialoguerPrompt {
    fn can_prompt(&self) -> bool {
        self.terminal_capabilities.can_prompt()
    }
}

impl AuthenticationInput for DialoguerPrompt {
    fn read_login_identifier(&self, prompt: &str) -> Result<LoginIdentifier, PromptError> {
        let prompt = sanitize_terminal_text(prompt);
        let raw = Input::<String>::with_theme(&SimpleTheme)
            .with_prompt(prompt)
            .allow_empty(false)
            .interact_text_on(&self.term)
            .map_err(classify_dialoguer_error)?;
        LoginIdentifier::parse_owned(raw)
            .map_err(|source| PromptError::InvalidLoginIdentifier { source })
    }

    fn read_account_password(&self, prompt: &str) -> Result<AccountPassword, PromptError> {
        let raw = read_hidden(&self.term, prompt)?;
        AccountPassword::parse_owned(raw)
            .map_err(|source| PromptError::InvalidAccountPassword { source })
    }

    fn read_otp_code(&self, prompt: &str) -> Result<OtpCode, PromptError> {
        let raw = read_hidden(&self.term, prompt)?;
        OtpCode::parse_owned(raw).map_err(|source| PromptError::InvalidOtpCode { source })
    }

    fn read_access_token(&self, prompt: &str) -> Result<AccessToken, PromptError> {
        let raw = read_hidden(&self.term, prompt)?;
        AccessToken::parse_owned(raw).map_err(|source| PromptError::InvalidAccessToken { source })
    }

    fn read_device_id(&self, prompt: &str) -> Result<DeviceId, PromptError> {
        let raw = read_hidden(&self.term, prompt)?;
        DeviceId::from_owned(raw).map_err(|source| PromptError::InvalidDeviceId { source })
    }
}

impl DefaultNoConfirmation for DialoguerPrompt {
    fn confirm_default_no(&self, prompt: &str) -> Result<bool, PromptError> {
        let prompt = sanitize_terminal_text(prompt);
        Confirm::with_theme(&SimpleTheme)
            .with_prompt(prompt)
            .default(false)
            .interact_on_opt(&self.term)
            .map_err(classify_dialoguer_error)?
            .ok_or(PromptError::Cancelled)
    }
}

impl FundingChoiceSelection for DialoguerPrompt {
    fn select_funding_choice(
        &self,
        prompt: &str,
        choices: &[&PaymentMethod],
    ) -> Result<usize, PromptError> {
        if choices.is_empty() {
            return Err(PromptError::NoChoices);
        }
        let prompt = sanitize_terminal_text(prompt);
        let sanitized_items = choices
            .iter()
            .map(|method| sanitize_terminal_text(&payment_method_label(method)))
            .collect::<Vec<_>>();

        let index = Select::with_theme(&SimpleTheme)
            .with_prompt(prompt)
            .items(&sanitized_items)
            .default(0)
            .interact_on_opt(&self.term)
            .map_err(classify_dialoguer_error)?
            .ok_or(PromptError::Cancelled)?;

        if index >= choices.len() {
            return Err(PromptError::InvalidSelection {
                index,
                choice_count: choices.len(),
            });
        }
        Ok(index)
    }
}

fn payment_method_label(method: &PaymentMethod) -> String {
    let details = method
        .method_type()
        .map(ToOwned::to_owned)
        .into_iter()
        .chain(
            method
                .last_four()
                .map(|last_four| format!("ending {last_four}")),
        )
        .collect::<Vec<_>>();
    let name = method.name().unwrap_or("Payment method");
    if details.is_empty() {
        format!("{name} [ID {}]", method.id())
    } else {
        format!("{name} ({}) [ID {}]", details.join(", "), method.id())
    }
}

fn read_hidden(term: &Term, prompt: &str) -> Result<String, PromptError> {
    let prompt = sanitize_terminal_text(prompt);
    Password::with_theme(&SimpleTheme)
        .with_prompt(prompt)
        .report(false)
        .interact_on(term)
        .map_err(classify_dialoguer_error)
}

fn classify_dialoguer_error(error: dialoguer::Error) -> PromptError {
    match error {
        dialoguer::Error::IO(source) => classify_io_error(source),
    }
}

fn classify_io_error(source: io::Error) -> PromptError {
    match source.kind() {
        io::ErrorKind::Interrupted | io::ErrorKind::UnexpectedEof => PromptError::Cancelled,
        io::ErrorKind::NotConnected => PromptError::NotInteractive,
        _ => PromptError::Interaction { source },
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::features::wallet::PaymentMethodId;

    #[test]
    fn prompt_io_errors_have_explicit_classifications() {
        assert!(matches!(
            classify_io_error(io::Error::from(io::ErrorKind::Interrupted)),
            PromptError::Cancelled
        ));
        assert!(matches!(
            classify_io_error(io::Error::from(io::ErrorKind::UnexpectedEof)),
            PromptError::Cancelled
        ));
        assert!(matches!(
            classify_io_error(io::Error::from(io::ErrorKind::NotConnected)),
            PromptError::NotInteractive
        ));
        assert!(matches!(
            classify_io_error(io::Error::from(io::ErrorKind::PermissionDenied)),
            PromptError::Interaction { .. }
        ));
    }

    #[test]
    fn empty_selection_fails_before_touching_the_terminal() {
        let prompt = DialoguerPrompt::new(TerminalCapabilities::new(false, false));
        assert!(matches!(
            prompt.select_funding_choice("Choose", &[]),
            Err(PromptError::NoChoices)
        ));
    }

    #[test]
    fn prompting_requires_both_input_and_diagnostic_terminals() {
        assert!(TerminalCapabilities::new(true, true).can_prompt());
        assert!(!TerminalCapabilities::new(true, false).can_prompt());
        assert!(!TerminalCapabilities::new(false, true).can_prompt());
        assert!(!TerminalCapabilities::new(false, false).can_prompt());
    }

    #[test]
    fn presentation_adapter_formats_structured_payment_method_choices()
    -> Result<(), Box<dyn std::error::Error>> {
        let detailed = PaymentMethod::new(
            PaymentMethodId::from_str("bank-1")?,
            Some("Checking".to_owned()),
            Some("bank".to_owned()),
            Some("1234".to_owned()),
            true,
        );
        let unnamed = PaymentMethod::new(
            PaymentMethodId::from_str("method-2")?,
            None,
            None,
            None,
            false,
        );

        assert_eq!(
            payment_method_label(&detailed),
            "Checking (bank, ending 1234) [ID bank-1]"
        );
        assert_eq!(
            payment_method_label(&unnamed),
            "Payment method [ID method-2]"
        );
        Ok(())
    }
}

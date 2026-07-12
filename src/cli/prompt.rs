use std::io::{self, IsTerminal};

use dialoguer::console::Term;
use dialoguer::theme::SimpleTheme;
use dialoguer::{Confirm, Input, Password, Select};

use super::output::sanitize_terminal_text;
use crate::application::ports::{PromptError, PromptPort};
use crate::domain::{AccessToken, AccountPassword, DeviceId, LoginIdentifier, OtpCode};

pub struct DialoguerPrompt {
    term: Term,
}

impl DialoguerPrompt {
    #[must_use]
    pub fn new() -> Self {
        Self {
            term: Term::stderr(),
        }
    }
}

impl Default for DialoguerPrompt {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptPort for DialoguerPrompt {
    fn can_prompt(&self) -> bool {
        process_can_prompt()
    }

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

    fn confirm_default_no(&self, prompt: &str) -> Result<bool, PromptError> {
        let prompt = sanitize_terminal_text(prompt);
        Confirm::with_theme(&SimpleTheme)
            .with_prompt(prompt)
            .default(false)
            .interact_on_opt(&self.term)
            .map_err(classify_dialoguer_error)?
            .ok_or(PromptError::Cancelled)
    }

    fn select(&self, prompt: &str, items: &[String]) -> Result<usize, PromptError> {
        if items.is_empty() {
            return Err(PromptError::NoChoices);
        }
        let prompt = sanitize_terminal_text(prompt);
        let sanitized_items = items
            .iter()
            .map(|item| sanitize_terminal_text(item))
            .collect::<Vec<_>>();

        let index = Select::with_theme(&SimpleTheme)
            .with_prompt(prompt)
            .items(&sanitized_items)
            .default(0)
            .interact_on_opt(&self.term)
            .map_err(classify_dialoguer_error)?
            .ok_or(PromptError::Cancelled)?;

        if index >= items.len() {
            return Err(PromptError::InvalidSelection {
                index,
                choice_count: items.len(),
            });
        }
        Ok(index)
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

#[must_use]
pub fn process_can_prompt() -> bool {
    io::stdin().is_terminal() && io::stderr().is_terminal()
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
    use super::*;

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
        let prompt = DialoguerPrompt::new();
        assert!(matches!(
            prompt.select("Choose", &[]),
            Err(PromptError::NoChoices)
        ));
    }
}

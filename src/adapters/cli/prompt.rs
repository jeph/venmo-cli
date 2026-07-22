use std::io::{self, IsTerminal};

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use dialoguer::console::{Key, Term, measure_text_width};
use dialoguer::theme::SimpleTheme;
use dialoguer::{Confirm, Input};
use zeroize::Zeroizing;

use super::output::sanitize_terminal_text;
use crate::features::auth::{
    AccountPassword, AuthenticationInput, LoginIdentifier, OtpCode, PromptAvailability, PromptError,
};
use crate::features::p2p_step_up::P2pStepUpInput;
use crate::features::payments::DefaultNoConfirmation;
use crate::shared::DeviceId;

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
        let mut raw = read_masked(&self.term, prompt)?;
        AccountPassword::parse_owned(std::mem::take(&mut *raw))
            .map_err(|source| PromptError::InvalidAccountPassword { source })
    }

    fn read_otp_code(&self, prompt: &str) -> Result<OtpCode, PromptError> {
        let mut raw = read_masked(&self.term, prompt)?;
        OtpCode::parse_owned(std::mem::take(&mut *raw))
            .map_err(|source| PromptError::InvalidOtpCode { source })
    }

    fn read_device_id(&self, prompt: &str) -> Result<DeviceId, PromptError> {
        let mut raw = read_masked(&self.term, prompt)?;
        DeviceId::from_owned(std::mem::take(&mut *raw))
            .map_err(|source| PromptError::InvalidDeviceId { source })
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

impl P2pStepUpInput for DialoguerPrompt {
    fn read_p2p_otp(&self, prompt: &str) -> Result<OtpCode, PromptError> {
        let mut raw = read_masked(&self.term, prompt)?;
        OtpCode::parse_owned(std::mem::take(&mut *raw))
            .map_err(|source| PromptError::InvalidOtpCode { source })
    }
}

trait MaskedTerminal {
    fn begin_secure_input(&self) -> io::Result<()>;
    fn end_secure_input(&self) -> io::Result<()>;
    fn read_key(&self) -> io::Result<Key>;
    fn write_str(&self, value: &str) -> io::Result<()>;
    fn flush(&self) -> io::Result<()>;
    fn width(&self) -> usize;
    fn clear_line(&self) -> io::Result<()>;
    fn clear_last_lines(&self, lines: usize) -> io::Result<()>;
}

impl MaskedTerminal for Term {
    fn begin_secure_input(&self) -> io::Result<()> {
        enable_raw_mode()
    }

    fn end_secure_input(&self) -> io::Result<()> {
        disable_raw_mode()
    }

    fn read_key(&self) -> io::Result<Key> {
        // Return Ctrl-C as a key so the prompt can clear itself and restore terminal state.
        self.read_key_raw()
    }

    fn write_str(&self, value: &str) -> io::Result<()> {
        self.write_str(value)
    }

    fn flush(&self) -> io::Result<()> {
        self.flush()
    }

    fn width(&self) -> usize {
        usize::from(self.size().1)
    }

    fn clear_line(&self) -> io::Result<()> {
        self.clear_line()
    }

    fn clear_last_lines(&self, lines: usize) -> io::Result<()> {
        self.clear_last_lines(lines)
    }
}

struct SecureInputGuard<'a, T: MaskedTerminal> {
    term: &'a T,
    active: bool,
}

impl<'a, T: MaskedTerminal> SecureInputGuard<'a, T> {
    fn new(term: &'a T) -> io::Result<Self> {
        term.begin_secure_input()?;
        Ok(Self { term, active: true })
    }

    fn finish(mut self) -> io::Result<()> {
        let result = self.term.end_secure_input();
        if result.is_ok() {
            self.active = false;
        }
        result
    }
}

impl<T: MaskedTerminal> Drop for SecureInputGuard<'_, T> {
    fn drop(&mut self) {
        if self.active {
            let _ = self.term.end_secure_input();
        }
    }
}

fn read_masked(term: &impl MaskedTerminal, prompt: &str) -> Result<Zeroizing<String>, PromptError> {
    let prompt = sanitize_terminal_text(prompt);
    let rendered_prompt = format!("{prompt}: ");
    let prompt_width = measure_text_width(&rendered_prompt);
    let mut value = Zeroizing::new(String::new());
    // Disable echo before displaying the prompt so an immediate paste cannot race raw-mode setup.
    let secure_input = SecureInputGuard::new(term).map_err(classify_io_error)?;

    if let Err(error) = render_masked(term, &rendered_prompt, value.chars().count()) {
        let _ = clear_rendered(term, prompt_width);
        let _ = secure_input.finish();
        return Err(classify_io_error(error));
    }

    let interaction = loop {
        let key = match term.read_key() {
            Ok(key) => key,
            Err(error) => break Err(error),
        };

        match key {
            Key::Char(character) => {
                value.push(character);
                if let Err(error) = term.write_str("*").and_then(|()| term.flush()) {
                    break Err(error);
                }
            }
            Key::Backspace | Key::Del if !value.is_empty() => {
                let previous_mask_width = value.chars().count();
                value.pop();
                if let Err(error) = clear_rendered(term, prompt_width + previous_mask_width)
                    .and_then(|()| render_masked(term, &rendered_prompt, value.chars().count()))
                {
                    break Err(error);
                }
            }
            Key::Enter if value.is_empty() => {
                if let Err(error) = clear_rendered(term, prompt_width)
                    .and_then(|()| render_masked(term, &rendered_prompt, 0))
                {
                    break Err(error);
                }
            }
            Key::Enter => break Ok(()),
            Key::Escape | Key::CtrlC => break Err(io::Error::from(io::ErrorKind::Interrupted)),
            Key::Unknown => break Err(io::Error::from(io::ErrorKind::NotConnected)),
            _ => {}
        }
    };

    let cleanup = clear_rendered(term, prompt_width + value.chars().count());
    let restoration = secure_input.finish();
    match interaction {
        Err(error) => {
            let _ = cleanup;
            let _ = restoration;
            Err(classify_io_error(error))
        }
        Ok(()) => {
            cleanup.map_err(classify_io_error)?;
            restoration.map_err(classify_io_error)?;
            Ok(value)
        }
    }
}

fn render_masked(
    term: &impl MaskedTerminal,
    rendered_prompt: &str,
    mask_width: usize,
) -> io::Result<()> {
    term.write_str(rendered_prompt)?;
    if mask_width > 0 {
        term.write_str(&"*".repeat(mask_width))?;
    }
    term.flush()
}

fn clear_rendered(term: &impl MaskedTerminal, rendered_width: usize) -> io::Result<()> {
    let terminal_width = term.width().max(1);
    let rendered_lines = rendered_width.max(1).div_ceil(terminal_width);
    term.clear_line()?;
    if rendered_lines > 1 {
        term.clear_last_lines(rendered_lines - 1)?;
    }
    term.flush()
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
    use std::cell::RefCell;
    use std::collections::VecDeque;

    use super::*;

    #[derive(Debug, Eq, PartialEq)]
    enum TerminalAction {
        BeginSecureInput,
        EndSecureInput,
        Write(String),
        Flush,
        ClearLine,
        ClearLastLines(usize),
    }

    struct ScriptedTerminal {
        keys: RefCell<VecDeque<io::Result<Key>>>,
        actions: RefCell<Vec<TerminalAction>>,
        width: usize,
    }

    impl ScriptedTerminal {
        fn new(keys: impl IntoIterator<Item = Key>) -> Self {
            Self {
                keys: RefCell::new(keys.into_iter().map(Ok).collect()),
                actions: RefCell::new(Vec::new()),
                width: 80,
            }
        }

        fn with_results(keys: impl IntoIterator<Item = io::Result<Key>>) -> Self {
            Self {
                keys: RefCell::new(keys.into_iter().collect()),
                actions: RefCell::new(Vec::new()),
                width: 80,
            }
        }

        fn with_width(mut self, width: usize) -> Self {
            self.width = width;
            self
        }

        fn written_text(&self) -> String {
            self.actions
                .borrow()
                .iter()
                .filter_map(|action| match action {
                    TerminalAction::Write(value) => Some(value.as_str()),
                    _ => None,
                })
                .collect()
        }
    }

    impl MaskedTerminal for ScriptedTerminal {
        fn begin_secure_input(&self) -> io::Result<()> {
            self.actions
                .borrow_mut()
                .push(TerminalAction::BeginSecureInput);
            Ok(())
        }

        fn end_secure_input(&self) -> io::Result<()> {
            self.actions
                .borrow_mut()
                .push(TerminalAction::EndSecureInput);
            Ok(())
        }

        fn read_key(&self) -> io::Result<Key> {
            match self.keys.borrow_mut().pop_front() {
                Some(result) => result,
                None => Err(io::Error::from(io::ErrorKind::UnexpectedEof)),
            }
        }

        fn write_str(&self, value: &str) -> io::Result<()> {
            self.actions
                .borrow_mut()
                .push(TerminalAction::Write(value.to_owned()));
            Ok(())
        }

        fn flush(&self) -> io::Result<()> {
            self.actions.borrow_mut().push(TerminalAction::Flush);
            Ok(())
        }

        fn width(&self) -> usize {
            self.width
        }

        fn clear_line(&self) -> io::Result<()> {
            self.actions.borrow_mut().push(TerminalAction::ClearLine);
            Ok(())
        }

        fn clear_last_lines(&self, lines: usize) -> io::Result<()> {
            self.actions
                .borrow_mut()
                .push(TerminalAction::ClearLastLines(lines));
            Ok(())
        }
    }

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
    fn prompting_requires_both_input_and_diagnostic_terminals() {
        assert!(TerminalCapabilities::new(true, true).can_prompt());
        assert!(!TerminalCapabilities::new(true, false).can_prompt());
        assert!(!TerminalCapabilities::new(false, true).can_prompt());
        assert!(!TerminalCapabilities::new(false, false).can_prompt());
    }

    #[test]
    fn masked_input_renders_one_star_per_typed_or_pasted_character() -> Result<(), PromptError> {
        let terminal = ScriptedTerminal::new([
            Key::Char('s'),
            Key::Char('e'),
            Key::Char('c'),
            Key::Char('r'),
            Key::Char('e'),
            Key::Char('t'),
            Key::Enter,
        ]);

        let value = read_masked(&terminal, "Password")?;

        assert_eq!(value.as_str(), "secret");
        let written = terminal.written_text();
        assert_eq!(written, "Password: ******");
        assert!(!written.contains("secret"));
        assert_eq!(
            terminal.actions.borrow().first(),
            Some(&TerminalAction::BeginSecureInput)
        );
        assert_eq!(
            terminal.actions.borrow().last(),
            Some(&TerminalAction::EndSecureInput)
        );
        assert!(
            terminal
                .actions
                .borrow()
                .contains(&TerminalAction::ClearLine)
        );
        Ok(())
    }

    #[test]
    fn masked_input_handles_unicode_and_backspace_without_echoing_values() -> Result<(), PromptError>
    {
        let terminal = ScriptedTerminal::new([
            Key::Char('é'),
            Key::Char('🔐'),
            Key::Backspace,
            Key::Char('x'),
            Key::Enter,
        ]);

        let value = read_masked(&terminal, "Secret")?;

        assert_eq!(value.as_str(), "éx");
        let written = terminal.written_text();
        assert!(!written.contains('é'));
        assert!(!written.contains('🔐'));
        assert!(!written.contains('x'));
        Ok(())
    }

    #[test]
    fn empty_enter_reprompts_instead_of_returning_an_empty_value() -> Result<(), PromptError> {
        let terminal = ScriptedTerminal::new([Key::Enter, Key::Char('a'), Key::Enter]);

        let value = read_masked(&terminal, "Secret")?;

        assert_eq!(value.as_str(), "a");
        assert_eq!(terminal.written_text(), "Secret: Secret: *");
        Ok(())
    }

    #[test]
    fn cancellation_clears_the_masked_prompt() {
        for cancellation in [Key::Escape, Key::CtrlC] {
            let terminal = ScriptedTerminal::new([Key::Char('s'), cancellation]);

            assert!(matches!(
                read_masked(&terminal, "Secret"),
                Err(PromptError::Cancelled)
            ));
            assert!(!terminal.written_text().contains('s'));
            assert!(
                terminal
                    .actions
                    .borrow()
                    .contains(&TerminalAction::ClearLine)
            );
            assert_eq!(
                terminal.actions.borrow().last(),
                Some(&TerminalAction::EndSecureInput)
            );
        }
    }

    #[test]
    fn masked_input_classifies_io_errors_and_attempts_cleanup() {
        let terminal =
            ScriptedTerminal::with_results([Err(io::Error::from(io::ErrorKind::PermissionDenied))]);

        assert!(matches!(
            read_masked(&terminal, "Secret"),
            Err(PromptError::Interaction { .. })
        ));
        assert!(
            terminal
                .actions
                .borrow()
                .contains(&TerminalAction::ClearLine)
        );
        assert_eq!(
            terminal.actions.borrow().last(),
            Some(&TerminalAction::EndSecureInput)
        );
    }

    #[test]
    fn wrapped_masked_input_clears_every_rendered_line() -> Result<(), PromptError> {
        let terminal =
            ScriptedTerminal::new([Key::Char('a'), Key::Char('b'), Key::Char('c'), Key::Enter])
                .with_width(4);

        let value = read_masked(&terminal, "P")?;

        assert_eq!(value.as_str(), "abc");
        assert!(
            terminal
                .actions
                .borrow()
                .contains(&TerminalAction::ClearLastLines(1))
        );
        Ok(())
    }

    #[test]
    #[ignore = "requires a human-operated terminal or PTY"]
    fn masked_input_terminal_smoke_test() -> Result<(), PromptError> {
        let value = read_masked(&Term::stderr(), "Masked prompt smoke test")?;

        assert_eq!(value.as_str(), "abx");
        Ok(())
    }
}

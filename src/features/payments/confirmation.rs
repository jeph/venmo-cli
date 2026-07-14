use super::DefaultNoConfirmation;
use crate::features::auth::PromptError;

pub(crate) enum DefaultNoConfirmationError {
    Required,
    Declined,
    Prompt(PromptError),
}

pub(crate) fn authorize<P>(
    prompt: &P,
    assume_yes: bool,
    question: &str,
) -> Result<(), DefaultNoConfirmationError>
where
    P: DefaultNoConfirmation,
{
    if assume_yes {
        return Ok(());
    }
    if !prompt.can_prompt() {
        return Err(DefaultNoConfirmationError::Required);
    }
    if prompt
        .confirm_default_no(question)
        .map_err(DefaultNoConfirmationError::Prompt)?
    {
        Ok(())
    } else {
        Err(DefaultNoConfirmationError::Declined)
    }
}

#[cfg(test)]
#[path = "confirmation_tests.rs"]
mod tests;

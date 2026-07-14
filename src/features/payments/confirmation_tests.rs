use std::cell::RefCell;
use std::io;
use std::rc::Rc;

use super::*;
use crate::features::auth::PromptAvailability;

type Transcript = Rc<RefCell<Vec<Call>>>;

#[derive(Clone, Debug, Eq, PartialEq)]
enum Call {
    PromptAvailability,
    ConfirmDefaultNo { prompt: String },
}

#[derive(Clone, Copy)]
enum PromptScript {
    Answer(bool),
    Cancelled,
    Interaction,
}

struct ScriptedPrompt {
    interactive: bool,
    script: PromptScript,
    transcript: Transcript,
}

impl PromptAvailability for ScriptedPrompt {
    fn can_prompt(&self) -> bool {
        self.transcript.borrow_mut().push(Call::PromptAvailability);
        self.interactive
    }
}

impl DefaultNoConfirmation for ScriptedPrompt {
    fn confirm_default_no(&self, prompt: &str) -> Result<bool, PromptError> {
        self.transcript.borrow_mut().push(Call::ConfirmDefaultNo {
            prompt: prompt.to_owned(),
        });
        match self.script {
            PromptScript::Answer(answer) => Ok(answer),
            PromptScript::Cancelled => Err(PromptError::Cancelled),
            PromptScript::Interaction => Err(PromptError::Interaction {
                source: io::Error::other("synthetic confirmation failure"),
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PromptFailure {
    Cancelled,
    Interaction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Outcome {
    Authorized,
    Required,
    Declined,
    Prompt(PromptFailure),
}

#[derive(Debug, Eq, PartialEq)]
struct Observation {
    outcome: Outcome,
    transcript: Vec<Call>,
}

fn project(result: Result<(), DefaultNoConfirmationError>) -> Outcome {
    match result {
        Ok(()) => Outcome::Authorized,
        Err(DefaultNoConfirmationError::Required) => Outcome::Required,
        Err(DefaultNoConfirmationError::Declined) => Outcome::Declined,
        Err(DefaultNoConfirmationError::Prompt(PromptError::Cancelled)) => {
            Outcome::Prompt(PromptFailure::Cancelled)
        }
        Err(DefaultNoConfirmationError::Prompt(_)) => Outcome::Prompt(PromptFailure::Interaction),
    }
}

#[test]
fn default_no_confirmation_compares_each_complete_outcome_and_transcript() {
    for (interactive, script, assume_yes, expected) in [
        (
            false,
            PromptScript::Answer(false),
            true,
            Observation {
                outcome: Outcome::Authorized,
                transcript: Vec::new(),
            },
        ),
        (
            false,
            PromptScript::Answer(true),
            false,
            Observation {
                outcome: Outcome::Required,
                transcript: vec![Call::PromptAvailability],
            },
        ),
        (
            true,
            PromptScript::Answer(false),
            false,
            observation_with_prompt(Outcome::Declined),
        ),
        (
            true,
            PromptScript::Answer(true),
            false,
            observation_with_prompt(Outcome::Authorized),
        ),
        (
            true,
            PromptScript::Cancelled,
            false,
            observation_with_prompt(Outcome::Prompt(PromptFailure::Cancelled)),
        ),
        (
            true,
            PromptScript::Interaction,
            false,
            observation_with_prompt(Outcome::Prompt(PromptFailure::Interaction)),
        ),
    ] {
        // Setup and immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let prompt = ScriptedPrompt {
            interactive,
            script,
            transcript: Rc::clone(&transcript),
        };

        // Complete expected state is `expected` above. Execute once.
        let result = authorize(&prompt, assume_yes, "Exact question?");
        let observed = Observation {
            outcome: project(result),
            transcript: transcript.borrow().clone(),
        };

        assert_eq!(observed, expected);
    }
}

fn observation_with_prompt(outcome: Outcome) -> Observation {
    Observation {
        outcome,
        transcript: vec![
            Call::PromptAvailability,
            Call::ConfirmDefaultNo {
                prompt: "Exact question?".to_owned(),
            },
        ],
    }
}

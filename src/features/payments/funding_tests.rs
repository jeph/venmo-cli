use std::cell::RefCell;
use std::error::Error;
use std::io;
use std::rc::Rc;
use std::str::FromStr;

use super::*;
use crate::features::auth::PromptAvailability;
use crate::features::payments::{PeerFundingFee, PeerFundingRole};
use crate::features::wallet::PaymentMethod;

type TestResult = Result<(), Box<dyn Error>>;
type Transcript = Rc<RefCell<Vec<Call>>>;

#[derive(Clone, Debug, Eq, PartialEq)]
enum Call {
    PromptAvailability,
    SelectFundingChoice {
        prompt: String,
        choices: Vec<PaymentMethod>,
    },
}

#[derive(Clone, Copy)]
enum PromptScript {
    Index(usize),
    Cancelled,
    Interaction,
}

struct ScriptedPrompt {
    interactive: bool,
    script: PromptScript,
    transcript: Transcript,
}

impl ScriptedPrompt {
    fn new(interactive: bool, script: PromptScript, transcript: Transcript) -> Self {
        Self {
            interactive,
            script,
            transcript,
        }
    }
}

impl PromptAvailability for ScriptedPrompt {
    fn can_prompt(&self) -> bool {
        self.transcript.borrow_mut().push(Call::PromptAvailability);
        self.interactive
    }
}

impl FundingChoiceSelection for ScriptedPrompt {
    fn select_funding_choice(
        &self,
        prompt: &str,
        choices: &[&PaymentMethod],
    ) -> Result<usize, PromptError> {
        self.transcript
            .borrow_mut()
            .push(Call::SelectFundingChoice {
                prompt: prompt.to_owned(),
                choices: choices.iter().map(|choice| (*choice).clone()).collect(),
            });
        match self.script {
            PromptScript::Index(index) => Ok(index),
            PromptScript::Cancelled => Err(PromptError::Cancelled),
            PromptScript::Interaction => Err(PromptError::Interaction {
                source: io::Error::other("synthetic funding prompt failure"),
            }),
        }
    }
}

struct NoPrompt;

impl PromptAvailability for NoPrompt {
    fn can_prompt(&self) -> bool {
        false
    }
}

impl FundingChoiceSelection for NoPrompt {
    fn select_funding_choice(
        &self,
        _prompt: &str,
        _choices: &[&PaymentMethod],
    ) -> Result<usize, PromptError> {
        Err(PromptError::NotInteractive)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PromptFailure {
    Cancelled,
    InvalidSelection { index: usize, choice_count: usize },
    Interaction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FundingFailure {
    NoEligibleMethods,
    ExplicitMethodUnavailable,
    DuplicateMethodIds,
    MultipleDefaults,
    ExplicitMethodRequired,
    Prompt(PromptFailure),
}

impl From<&FundingSelectionError> for FundingFailure {
    fn from(error: &FundingSelectionError) -> Self {
        match error {
            FundingSelectionError::NoEligibleMethods => Self::NoEligibleMethods,
            FundingSelectionError::ExplicitMethodUnavailable => Self::ExplicitMethodUnavailable,
            FundingSelectionError::DuplicateMethodIds => Self::DuplicateMethodIds,
            FundingSelectionError::MultipleDefaults => Self::MultipleDefaults,
            FundingSelectionError::ExplicitMethodRequired => Self::ExplicitMethodRequired,
            FundingSelectionError::Prompt { source } => Self::Prompt(match source {
                PromptError::Cancelled => PromptFailure::Cancelled,
                PromptError::InvalidSelection {
                    index,
                    choice_count,
                } => PromptFailure::InvalidSelection {
                    index: *index,
                    choice_count: *choice_count,
                },
                PromptError::Interaction { .. } => PromptFailure::Interaction,
                _ => PromptFailure::Interaction,
            }),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
enum FundingOutcome {
    Selected(PeerFundingMethod),
    Failure(FundingFailure),
}

fn project(result: Result<PeerFundingMethod, FundingSelectionError>) -> FundingOutcome {
    match result {
        Ok(method) => FundingOutcome::Selected(method),
        Err(error) => FundingOutcome::Failure(FundingFailure::from(&error)),
    }
}

#[test]
fn deterministic_selection_and_validation_table_compares_whole_results() -> TestResult {
    struct Case {
        methods: Vec<PeerFundingMethod>,
        requested: Option<PaymentMethodId>,
        expected: FundingOutcome,
    }

    let default_unknown = method_with_fee("default-unknown", true, PeerFundingFee::Unknown)?;
    let backup_zero = method("backup-zero", false)?;
    let explicit_zero = method("explicit-zero", false)?;
    let cases = vec![
        Case {
            methods: Vec::new(),
            requested: None,
            expected: FundingOutcome::Failure(FundingFailure::NoEligibleMethods),
        },
        Case {
            methods: vec![default_unknown.clone(), backup_zero.clone()],
            requested: None,
            expected: FundingOutcome::Selected(default_unknown),
        },
        Case {
            methods: vec![explicit_zero.clone()],
            requested: Some(PaymentMethodId::from_str("explicit-zero")?),
            expected: FundingOutcome::Selected(explicit_zero),
        },
        Case {
            methods: vec![method_with_fee(
                "unknown-fee",
                false,
                PeerFundingFee::Unknown,
            )?],
            requested: Some(PaymentMethodId::from_str("unknown-fee")?),
            expected: FundingOutcome::Selected(method_with_fee(
                "unknown-fee",
                false,
                PeerFundingFee::Unknown,
            )?),
        },
        Case {
            methods: vec![backup_zero],
            requested: Some(PaymentMethodId::from_str("missing")?),
            expected: FundingOutcome::Failure(FundingFailure::ExplicitMethodUnavailable),
        },
        Case {
            methods: vec![method_with_fee(
                "unknown-only",
                false,
                PeerFundingFee::Unknown,
            )?],
            requested: None,
            expected: FundingOutcome::Selected(method_with_fee(
                "unknown-only",
                false,
                PeerFundingFee::Unknown,
            )?),
        },
        Case {
            methods: vec![method_with_fee(
                "nonzero-explicit",
                false,
                PeerFundingFee::from_cents(3),
            )?],
            requested: Some(PaymentMethodId::from_str("nonzero-explicit")?),
            expected: FundingOutcome::Selected(method_with_fee(
                "nonzero-explicit",
                false,
                PeerFundingFee::from_cents(3),
            )?),
        },
        Case {
            methods: vec![method("duplicate", false)?, method("duplicate", false)?],
            requested: None,
            expected: FundingOutcome::Failure(FundingFailure::DuplicateMethodIds),
        },
        Case {
            methods: vec![method("default-1", true)?, method("default-2", true)?],
            requested: None,
            expected: FundingOutcome::Failure(FundingFailure::MultipleDefaults),
        },
    ];

    for case in cases {
        let observed = project(select(&NoPrompt, &case.methods, case.requested.as_ref()));
        assert_eq!(observed, case.expected);
    }
    Ok(())
}

#[derive(Debug, Eq, PartialEq)]
struct Observation {
    outcome: FundingOutcome,
    transcript: Vec<Call>,
}

#[test]
fn interactive_selection_cancel_error_invalid_choice_and_noninteractive_ambiguity() -> TestResult {
    for (interactive, script, expected, expect_selection) in [
        (
            true,
            PromptScript::Index(1),
            FundingOutcome::Selected(method_with_fee(
                "bank-2",
                false,
                PeerFundingFee::from_cents(3),
            )?),
            true,
        ),
        (
            true,
            PromptScript::Cancelled,
            FundingOutcome::Failure(FundingFailure::Prompt(PromptFailure::Cancelled)),
            true,
        ),
        (
            true,
            PromptScript::Interaction,
            FundingOutcome::Failure(FundingFailure::Prompt(PromptFailure::Interaction)),
            true,
        ),
        (
            true,
            PromptScript::Index(2),
            FundingOutcome::Failure(FundingFailure::Prompt(PromptFailure::InvalidSelection {
                index: 2,
                choice_count: 2,
            })),
            true,
        ),
        (
            false,
            PromptScript::Index(0),
            FundingOutcome::Failure(FundingFailure::ExplicitMethodRequired),
            false,
        ),
    ] {
        // Setup.
        let methods = vec![
            method_with_fee("bank-1", false, PeerFundingFee::Unknown)?,
            method_with_fee("bank-2", false, PeerFundingFee::from_cents(3))?,
        ];

        // Immutable initial state.
        let transcript = Rc::new(RefCell::new(Vec::new()));
        let prompt = ScriptedPrompt::new(interactive, script, Rc::clone(&transcript));

        // Complete expected final outcome and state.
        let mut expected_calls = vec![Call::PromptAvailability];
        if expect_selection {
            expected_calls.push(Call::SelectFundingChoice {
                prompt: "Choose a payment method".to_owned(),
                choices: methods
                    .iter()
                    .map(|method| method.method().clone())
                    .collect(),
            });
        }
        let expected = Observation {
            outcome: expected,
            transcript: expected_calls,
        };

        // Execute once.
        let result = select(&prompt, &methods, None);
        let observed = Observation {
            outcome: project(result),
            transcript: transcript.borrow().clone(),
        };

        assert_eq!(observed, expected);
    }
    Ok(())
}

#[test]
fn nonzero_default_remains_the_automatic_selection() -> TestResult {
    let methods = vec![
        method_with_fee("default-nonzero", true, PeerFundingFee::from_cents(1))?,
        method("backup-zero", false)?,
    ];
    let expected = FundingOutcome::Selected(methods[0].clone());
    let observed = project(select(&NoPrompt, &methods, None));
    assert_eq!(observed, expected);
    Ok(())
}

fn method(id: &str, is_default: bool) -> Result<PeerFundingMethod, Box<dyn Error>> {
    method_with_fee(id, is_default, PeerFundingFee::ProvenZero)
}

fn method_with_fee(
    id: &str,
    is_default: bool,
    fee: PeerFundingFee,
) -> Result<PeerFundingMethod, Box<dyn Error>> {
    let method = PaymentMethod::new(
        PaymentMethodId::from_str(id)?,
        Some(format!("Method {id}")),
        Some("synthetic".to_owned()),
        Some("1234".to_owned()),
        is_default,
    );
    let role = if is_default {
        PeerFundingRole::Default
    } else {
        PeerFundingRole::Backup
    };
    Ok(PeerFundingMethod::new(method, role, fee))
}

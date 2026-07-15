use std::error::Error;
use std::str::FromStr;

use super::*;
use crate::features::payments::{PeerFundingFee, PeerFundingRole};
use crate::features::wallet::{PaymentMethod, PaymentMethodId};

type TestResult = Result<(), Box<dyn Error>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FundingFailure {
    NoEligibleMethods,
    DuplicateMethodIds,
    MultipleDefaults,
    AmbiguousAutomaticSelection,
}

#[derive(Debug, Eq, PartialEq)]
enum FundingOutcome {
    Selected(PeerFundingMethod),
    Failure(FundingFailure),
}

fn project(result: Result<PeerFundingMethod, FundingSelectionError>) -> FundingOutcome {
    match result {
        Ok(method) => FundingOutcome::Selected(method),
        Err(error) => FundingOutcome::Failure(match error {
            FundingSelectionError::NoEligibleMethods => FundingFailure::NoEligibleMethods,
            FundingSelectionError::DuplicateMethodIds => FundingFailure::DuplicateMethodIds,
            FundingSelectionError::MultipleDefaults => FundingFailure::MultipleDefaults,
            FundingSelectionError::AmbiguousAutomaticSelection => {
                FundingFailure::AmbiguousAutomaticSelection
            }
        }),
    }
}

#[test]
fn automatic_policy_validates_contract_and_fails_closed_on_ambiguity() -> TestResult {
    struct Case {
        methods: Vec<PeerFundingMethod>,
        expected: FundingOutcome,
    }

    let sole_unknown = method_with_fee("sole-unknown", false, PeerFundingFee::Unknown)?;
    let cases = vec![
        Case {
            methods: Vec::new(),
            expected: FundingOutcome::Failure(FundingFailure::NoEligibleMethods),
        },
        Case {
            methods: vec![sole_unknown.clone()],
            expected: FundingOutcome::Selected(sole_unknown),
        },
        Case {
            methods: vec![method("duplicate", false)?, method("duplicate", false)?],
            expected: FundingOutcome::Failure(FundingFailure::DuplicateMethodIds),
        },
        Case {
            methods: vec![method("default-1", true)?, method("default-2", true)?],
            expected: FundingOutcome::Failure(FundingFailure::MultipleDefaults),
        },
        Case {
            methods: vec![method("bank-1", false)?, method("bank-2", false)?],
            expected: FundingOutcome::Failure(FundingFailure::AmbiguousAutomaticSelection),
        },
    ];

    for case in cases {
        assert_eq!(project(select(&case.methods)), case.expected);
    }
    Ok(())
}

#[test]
fn unique_default_wins_regardless_of_fee_or_response_order() -> TestResult {
    let default = method_with_fee("default-nonzero", true, PeerFundingFee::from_cents(9))?;
    let zero_fee_backup = method("backup-zero", false)?;
    let unknown_fee_backup = method_with_fee("backup-unknown", false, PeerFundingFee::Unknown)?;

    for methods in [
        vec![
            default.clone(),
            zero_fee_backup.clone(),
            unknown_fee_backup.clone(),
        ],
        vec![
            unknown_fee_backup.clone(),
            zero_fee_backup.clone(),
            default.clone(),
        ],
        vec![zero_fee_backup, default.clone(), unknown_fee_backup],
    ] {
        assert_eq!(
            project(select(&methods)),
            FundingOutcome::Selected(default.clone())
        );
    }
    Ok(())
}

#[test]
fn multiple_nondefaults_are_ambiguous_regardless_of_fee_or_order() -> TestResult {
    let zero = method("zero", false)?;
    let nonzero = method_with_fee("nonzero", false, PeerFundingFee::from_cents(1))?;
    let unknown = method_with_fee("unknown", false, PeerFundingFee::Unknown)?;

    for methods in [
        vec![zero.clone(), nonzero.clone(), unknown.clone()],
        vec![unknown, nonzero, zero],
    ] {
        assert_eq!(
            project(select(&methods)),
            FundingOutcome::Failure(FundingFailure::AmbiguousAutomaticSelection)
        );
    }
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
        Some("bank".to_owned()),
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

use std::error::Error;
use std::str::FromStr;

use super::*;
use crate::features::payments::{PeerFundingFee, PeerFundingRole};
use crate::features::wallet::{Balance, PaymentMethod, PaymentMethodId, SignedUsdAmount};
use crate::shared::Money;

type TestResult = Result<(), Box<dyn Error>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FundingFailure {
    NoEligibleMethods,
    DuplicateMethodIds,
    MultipleDefaults,
    AmbiguousAutomaticSelection,
    RequestedSourceUnavailable,
    RequestedBalanceInsufficient,
    MissingBalanceSource,
}

#[derive(Debug, Eq, PartialEq)]
enum FundingOutcome {
    Selected(PeerFundingMethod),
    Failure(FundingFailure),
}

fn project(result: Result<SelectedFundingSource, FundingSelectionError>) -> FundingOutcome {
    match result {
        Ok(selected) => {
            let (source, _) = selected.into_parts();
            match source.external_method() {
                Some(method) => FundingOutcome::Selected(method.clone()),
                None => FundingOutcome::Failure(FundingFailure::MissingBalanceSource),
            }
        }
        Err(error) => FundingOutcome::Failure(match error {
            FundingSelectionError::NoEligibleMethods => FundingFailure::NoEligibleMethods,
            FundingSelectionError::DuplicateMethodIds => FundingFailure::DuplicateMethodIds,
            FundingSelectionError::MultipleDefaults => FundingFailure::MultipleDefaults,
            FundingSelectionError::AmbiguousAutomaticSelection => {
                FundingFailure::AmbiguousAutomaticSelection
            }
            FundingSelectionError::RequestedSourceUnavailable => {
                FundingFailure::RequestedSourceUnavailable
            }
            FundingSelectionError::RequestedBalanceInsufficient => {
                FundingFailure::RequestedBalanceInsufficient
            }
            FundingSelectionError::MissingBalanceSource => FundingFailure::MissingBalanceSource,
        }),
    }
}

fn select_external_case(
    methods: Vec<PeerFundingMethod>,
) -> Result<SelectedFundingSource, FundingSelectionError> {
    select_source(
        &PeerFundingSources::new(None, methods),
        &Balance::new(
            SignedUsdAmount::from_cents(0),
            SignedUsdAmount::from_cents(0),
        ),
        Money::from_cents(1).map_err(|_| FundingSelectionError::NoEligibleMethods)?,
        None,
    )
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
        assert_eq!(project(select_external_case(case.methods)), case.expected);
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
            project(select_external_case(methods)),
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
            project(select_external_case(methods)),
            FundingOutcome::Failure(FundingFailure::AmbiguousAutomaticSelection)
        );
    }
    Ok(())
}

#[test]
fn source_selection_covers_automatic_and_explicit_balance_and_external_paths() -> TestResult {
    let balance_method = PaymentMethod::new(
        PaymentMethodId::from_str("balance-1")?,
        Some("Venmo balance".to_owned()),
        Some("balance".to_owned()),
        None,
        true,
    );
    let external = method("bank-1", true)?;
    let sources = PeerFundingSources::new(Some(balance_method.clone()), vec![external.clone()]);
    let covered = Balance::new(
        SignedUsdAmount::from_cents(100),
        SignedUsdAmount::from_cents(0),
    );
    let short = Balance::new(
        SignedUsdAmount::from_cents(0),
        SignedUsdAmount::from_cents(0),
    );
    let amount = Money::from_cents(1)?;

    for (balance, requested, expected_source, expected_selection) in [
        (
            &covered,
            None,
            PeerFundingSource::balance(balance_method.clone()),
            PeerFundingSourceSelection::Automatic,
        ),
        (
            &short,
            None,
            PeerFundingSource::external(external.clone()),
            PeerFundingSourceSelection::Automatic,
        ),
        (
            &covered,
            Some(balance_method.id()),
            PeerFundingSource::balance(balance_method.clone()),
            PeerFundingSourceSelection::Explicit,
        ),
        (
            &covered,
            Some(external.method().id()),
            PeerFundingSource::external(external.clone()),
            PeerFundingSourceSelection::Explicit,
        ),
    ] {
        let (source, selection) = select_source(&sources, balance, amount, requested)?.into_parts();
        assert_eq!(source, expected_source);
        assert_eq!(selection, expected_selection);
    }

    assert!(matches!(
        select_source(&sources, &short, amount, Some(balance_method.id())),
        Err(FundingSelectionError::RequestedBalanceInsufficient)
    ));
    let unknown = PaymentMethodId::from_str("unknown-1")?;
    assert!(matches!(
        select_source(&sources, &covered, amount, Some(&unknown)),
        Err(FundingSelectionError::RequestedSourceUnavailable)
    ));
    assert!(matches!(
        select_source(
            &PeerFundingSources::new(None, vec![external]),
            &covered,
            amount,
            None,
        ),
        Err(FundingSelectionError::MissingBalanceSource)
    ));

    let first_default = method("default-1", true)?;
    let second_default = method("default-2", true)?;
    let defaults = PeerFundingSources::new(
        Some(balance_method.clone()),
        vec![first_default.clone(), second_default],
    );
    let (automatic_balance, automatic_selection) =
        select_source(&defaults, &covered, amount, None)?.into_parts();
    assert_eq!(
        automatic_balance,
        PeerFundingSource::balance(balance_method)
    );
    assert_eq!(automatic_selection, PeerFundingSourceSelection::Automatic);
    let (explicit_external, explicit_selection) = select_source(
        &defaults,
        &covered,
        amount,
        Some(first_default.method().id()),
    )?
    .into_parts();
    assert_eq!(
        explicit_external,
        PeerFundingSource::external(first_default)
    );
    assert_eq!(explicit_selection, PeerFundingSourceSelection::Explicit);
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

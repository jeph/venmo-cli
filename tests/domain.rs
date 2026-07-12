use std::str::FromStr;

use proptest::prelude::*;
use venmo_cli::domain::{
    ActivityBeforeId, Limit, Money, Note, Offset, RecipientInput, UserSearchQuery,
};

#[test]
fn money_uses_exact_integer_cents() {
    let cases = [("0.01", 1), ("1", 100), ("1.2", 120), ("12.50", 1_250)];

    for (source, expected_cents) in cases {
        let parsed = Money::from_str(source);
        assert_eq!(parsed.map(Money::cents), Ok(expected_cents));
    }
}

#[test]
fn invalid_money_is_rejected() {
    for source in ["", "0", "0.00", ".50", "1.", "1.234", "-1", "+1", "1e2"] {
        assert!(Money::from_str(source).is_err(), "accepted {source:?}");
    }
}

#[test]
fn recipient_requires_an_exact_username_or_positive_numeric_id() {
    for accepted in ["@alice", "@accept", "1", "0001"] {
        assert!(
            RecipientInput::from_str(accepted).is_ok(),
            "rejected {accepted:?}"
        );
    }
    for rejected in ["", "alice", "@", "0", "000", "12a"] {
        assert!(
            RecipientInput::from_str(rejected).is_err(),
            "accepted {rejected:?}"
        );
    }
    assert!(RecipientInput::from_str(&format!("@{}", "a".repeat(1024))).is_err());
}

#[test]
fn notes_and_limits_enforce_local_invariants() {
    assert!(Note::from_str("Dinner").is_ok());
    assert!(Note::from_str("  \t").is_err());
    assert_eq!(Limit::from_str("50").map(Limit::get), Ok(50));
    assert!(Limit::from_str("0").is_err());
    assert!(Limit::from_str("51").is_err());
    assert_eq!(Offset::from_str("0").map(Offset::get), Ok(0));
    assert!(Offset::from_str("-1").is_err());
    assert!(ActivityBeforeId::from_str("before-token").is_ok());
    assert!(ActivityBeforeId::from_str("before token").is_err());
    assert!(UserSearchQuery::from_str("Alice Smith").is_ok());
    assert!(UserSearchQuery::from_str("@").is_err());
}

proptest! {
    #[test]
    fn money_display_round_trips(cents in 1_u64..=1_000_000_000_000_u64) {
        let money = Money::from_cents(cents);
        prop_assert!(money.is_ok());
        if let Ok(money) = money {
            let reparsed = Money::from_str(&money.to_string());
            prop_assert_eq!(reparsed, Ok(money));
        }
    }
}

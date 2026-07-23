use std::str::FromStr;

use proptest::prelude::*;
use venmo_cli::model::{
    ActivityBeforeId, ActivityCommentListResult, ActivityReactionEmoji, ActivityReactionListResult,
    FriendsSubject, Limit, Money, Note, Offset, PeerFundingFee, RecipientInput, RequestAction,
    RequestDirection, RequestId, RequestInfoResult, RequestRecord, RequestStatus, User, UserId,
    UserInfoResult, UserSearchQuery, Username,
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
fn recipient_accepts_only_normalized_username_syntax() {
    for accepted in ["alice", "@alice", "accept", "@accept", "123"] {
        assert!(
            RecipientInput::from_str(accepted).is_ok(),
            "rejected {accepted:?}"
        );
    }
    for rejected in ["", "@", "white space", "line\nbreak"] {
        assert!(
            RecipientInput::from_str(rejected).is_err(),
            "accepted {rejected:?}"
        );
    }
    assert!(RecipientInput::from_str(&"a".repeat(1024)).is_err());
}

#[test]
fn all_user_inputs_normalize_optional_username_prefixes() {
    assert_eq!(Username::from_str("alice"), Username::from_str("@alice"));
    assert_eq!(
        RecipientInput::from_str("alice"),
        RecipientInput::from_str("@alice")
    );
    assert_eq!(
        UserSearchQuery::from_str("alice"),
        UserSearchQuery::from_str("@alice")
    );
}

#[test]
fn notes_and_limits_enforce_local_invariants() {
    assert!(Note::from_str("Dinner").is_ok());
    assert!(Note::from_str("  \t").is_err());
    assert_eq!(Limit::from_str("50").map(Limit::get), Ok(50));
    assert_eq!(Limit::MIN.get(), 1);
    assert!(Limit::from_str("0").is_err());
    assert!(Limit::from_str("51").is_err());
    assert_eq!(Offset::from_str("0").map(Offset::get), Ok(0));
    assert!(Offset::from_str("-1").is_err());
    assert!(ActivityBeforeId::from_str("before-token").is_ok());
    assert!(ActivityBeforeId::from_str("before token").is_err());
    assert!(ActivityReactionEmoji::from_str("🔥").is_ok());
    assert!(ActivityReactionEmoji::from_str("two emoji 🔥").is_err());
    assert!(UserSearchQuery::from_str("Alice Smith").is_ok());
    assert!(UserSearchQuery::from_str("@").is_err());
}

#[test]
fn request_records_and_peer_fees_preserve_whole_value_invariants()
-> Result<(), Box<dyn std::error::Error>> {
    let record = RequestRecord::new(
        RequestId::from_str("request-1")?,
        RequestAction::Pay,
        RequestDirection::Outgoing,
        User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("bob")?),
            Some("Bob".to_owned()),
        ),
        Money::from_cents(125)?,
        Some("Dinner".to_owned()),
        Some(time::OffsetDateTime::UNIX_EPOCH),
        RequestStatus::from_str("settled")?,
    );

    assert_eq!(record.action(), RequestAction::Pay);
    assert_eq!(record.status().as_str(), "settled");
    assert_eq!(record.amount().cents(), 125);
    assert_eq!(PeerFundingFee::from_cents(0), PeerFundingFee::ProvenZero);
    assert!(matches!(
        PeerFundingFee::from_cents(3),
        PeerFundingFee::NonZero { cents } if cents.get() == 3
    ));
    Ok(())
}

#[test]
fn public_info_result_facades_expose_only_their_completed_records() {
    let user_result: Option<UserInfoResult> = None;
    let request_result: Option<RequestInfoResult> = None;
    let comment_result: Option<ActivityCommentListResult> = None;
    let reaction_result: Option<ActivityReactionListResult> = None;

    assert!(user_result.as_ref().map(UserInfoResult::user).is_none());
    assert!(
        request_result
            .as_ref()
            .map(RequestInfoResult::request)
            .is_none()
    );
    assert!(
        comment_result
            .as_ref()
            .map(ActivityCommentListResult::comments)
            .is_none()
    );
    assert!(
        reaction_result
            .as_ref()
            .map(ActivityReactionListResult::reactions)
            .is_none()
    );
}

#[test]
fn public_friend_subject_preserves_validated_identity() -> Result<(), Box<dyn std::error::Error>> {
    let subject = FriendsSubject::new(UserId::from_str("2000")?, Username::from_bare("alice")?);

    assert_eq!(subject.user_id().as_str(), "2000");
    assert_eq!(subject.username().as_str(), "alice");
    Ok(())
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

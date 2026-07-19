use std::error::Error;
use std::io::{self, Write};
use std::str::FromStr;

use super::super::{
    write_balance, write_friends, write_friendship_details, write_friendship_result,
    write_payment_methods, write_user_info, write_user_search,
};
use crate::features::people::friends::FriendsResult;
use crate::features::people::friendship::{
    FriendshipAction, FriendshipMutationResult, FriendshipPlan, PreparedFriendshipMutation,
};
use crate::features::people::info::UserInfoResult;
use crate::features::people::users::UserSearchResult;
use crate::features::people::{FriendshipStatus, User, UserProfileKind};
use crate::features::wallet::balance::BalanceResult;
use crate::features::wallet::payment_methods::PaymentMethodsResult;
use crate::features::wallet::{Balance, PaymentMethod, PaymentMethodId, SignedUsdAmount};
use crate::shared::{AccessToken, Account, CredentialEnvelope, DeviceId, Offset, UserId, Username};

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn user_info_output_preserves_known_fields_sanitizes_text_and_marks_absence() -> TestResult {
    let complete = UserInfoResult::new(
        User::new(
            UserId::from_str("123")?,
            Some(Username::from_bare("alice")?),
            Some("Alice\n\u{1b}[31mExample".to_owned()),
        )
        .with_financial_attributes(UserProfileKind::Personal, true)
        .with_friendship_status(FriendshipStatus::RequestSent),
    );
    let minimal = UserInfoResult::new(User::new(UserId::from_str("456")?, None, None));
    let mut complete_output = Vec::new();
    let mut minimal_output = Vec::new();

    write_user_info(&mut complete_output, &complete)?;
    write_user_info(&mut minimal_output, &minimal)?;
    let complete_output = String::from_utf8(complete_output)?;
    let minimal_output = String::from_utf8(minimal_output)?;

    insta::assert_snapshot!("user_info_complete", complete_output);
    insta::assert_snapshot!("user_info_minimal", minimal_output);
    assert!(!complete_output.contains("Alice\n"));
    assert!(!complete_output.contains('\u{1b}'));
    Ok(())
}

#[test]
fn payment_method_output_is_copyable_and_sanitized() -> TestResult {
    let result = PaymentMethodsResult::from_methods(vec![PaymentMethod::new(
        PaymentMethodId::from_str("method-1")?,
        Some("Bank\nname".to_owned()),
        Some("bank".to_owned()),
        Some("1234".to_owned()),
        true,
    )]);
    let mut output = Vec::new();

    write_payment_methods(&mut output, &result)?;
    let rendered = String::from_utf8(output)?;

    insta::assert_snapshot!("payment_methods", rendered);
    assert!(!rendered.contains("Bank\nname"));
    Ok(())
}

#[test]
fn user_search_output_is_sanitized_and_reports_a_copyable_offset() -> TestResult {
    let result = UserSearchResult::new(
        vec![User::new(
            UserId::from_str("123")?,
            Some(Username::from_bare("alice".to_owned())?),
            Some("Alice\u{1b}[31m".to_owned()),
        )],
        Some(Offset::new(10)),
    );
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    write_user_search(&mut stdout, &mut stderr, &result)?;
    let stdout = String::from_utf8(stdout)?;
    let stderr = String::from_utf8(stderr)?;

    insta::assert_snapshot!("user_search", stdout);
    assert!(!stdout.contains("@@alice"));
    assert!(!stdout.contains('\u{1b}'));
    assert_eq!(stderr, "Next offset: 10\n");
    Ok(())
}

#[test]
fn friends_and_balance_output_are_copyable_exact_and_sanitized() -> TestResult {
    let friends = FriendsResult::new(
        vec![User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("bob")?),
            Some("Bob\nName".to_owned()),
        )],
        Some(Offset::new(20)),
    );
    let balance = BalanceResult::new(Balance::new(
        SignedUsdAmount::from_cents(1_234),
        SignedUsdAmount::from_cents(-5),
    ));
    let mut friends_stdout = Vec::new();
    let mut friends_stderr = Vec::new();
    let mut balance_stdout = Vec::new();

    write_friends(&mut friends_stdout, &mut friends_stderr, &friends)?;
    write_balance(&mut balance_stdout, &balance)?;
    let friends_stdout = String::from_utf8(friends_stdout)?;
    let balance_stdout = String::from_utf8(balance_stdout)?;
    let friends_stderr = String::from_utf8(friends_stderr)?;

    insta::assert_snapshot!("friends", friends_stdout);
    insta::assert_snapshot!("balance", balance_stdout);
    assert!(!friends_stdout.contains("@@bob"));
    assert!(!friends_stdout.contains("Bob\nName"));
    assert_eq!(friends_stderr, "Next offset: 20\n");
    Ok(())
}

#[test]
fn empty_people_and_payment_method_lists_have_exact_messages() -> TestResult {
    let users = UserSearchResult::new(Vec::new(), None);
    let friends = FriendsResult::new(Vec::new(), None);
    let methods = PaymentMethodsResult::from_methods(Vec::new());
    let mut users_stdout = Vec::new();
    let mut users_stderr = Vec::new();
    let mut friends_stdout = Vec::new();
    let mut friends_stderr = Vec::new();
    let mut methods_stdout = Vec::new();

    write_user_search(&mut users_stdout, &mut users_stderr, &users)?;
    write_friends(&mut friends_stdout, &mut friends_stderr, &friends)?;
    write_payment_methods(&mut methods_stdout, &methods)?;

    assert_eq!(users_stdout, b"No users found.\n");
    assert_eq!(users_stderr, b"");
    assert_eq!(friends_stdout, b"No friends found.\n");
    assert_eq!(friends_stderr, b"");
    assert_eq!(methods_stdout, b"No payment methods found.\n");
    Ok(())
}

#[test]
fn friendship_details_and_result_are_exact_and_sanitized() -> TestResult {
    let account = Account::new(
        UserId::from_str("1000")?,
        Username::from_bare("owner")?,
        Some("Owner".to_owned()),
    );
    let target = User::new(
        UserId::from_str("456")?,
        Some(Username::from_bare("alice")?),
        Some("Alice\nName".to_owned()),
    )
    .with_financial_attributes(UserProfileKind::Personal, true)
    .with_friendship_status(FriendshipStatus::NotFriend);
    let plan = FriendshipPlan::new(
        account,
        target,
        FriendshipStatus::NotFriend,
        FriendshipAction::SendRequest,
    );
    let credential = CredentialEnvelope::new(
        AccessToken::from_str("synthetic-token")?,
        DeviceId::from_str("synthetic-device")?,
        UserId::from_str("1000")?,
        Username::from_bare("owner")?,
        Some("Owner".to_owned()),
        time::OffsetDateTime::UNIX_EPOCH,
    );
    let prepared = PreparedFriendshipMutation::new(credential, plan);
    let mut details = Vec::new();
    write_friendship_details(&mut details, &prepared)?;
    let details = String::from_utf8(details)?;
    insta::assert_snapshot!("friendship_details", details);

    let result = FriendshipMutationResult::new(FriendshipPlan::new(
        Account::new(
            UserId::from_str("1000")?,
            Username::from_bare("owner")?,
            None,
        ),
        User::new(
            UserId::from_str("456")?,
            Some(Username::from_bare("alice")?),
            None,
        ),
        FriendshipStatus::NotFriend,
        FriendshipAction::SendRequest,
    ));
    let mut output = Vec::new();
    write_friendship_result(&mut output, &result)?;
    insta::assert_snapshot!("friendship_result", String::from_utf8(output)?);
    Ok(())
}

#[test]
fn continuation_is_not_written_when_buffered_record_output_fails() -> TestResult {
    struct FailingWriter;

    impl Write for FailingWriter {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("synthetic output failure"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let result = FriendsResult::new(vec![synthetic_user("456", "bob")?], Some(Offset::new(1)));
    let mut stdout = FailingWriter;
    let mut stderr = Vec::new();

    let observed = write_friends(&mut stdout, &mut stderr, &result)
        .map(|()| None)
        .unwrap_or_else(|error| Some(error.kind()));

    assert_eq!(observed, Some(io::ErrorKind::Other));
    assert_eq!(stderr, b"");
    Ok(())
}

fn synthetic_user(id: &str, username: &str) -> Result<User, Box<dyn Error>> {
    Ok(User::new(
        UserId::from_str(id)?,
        Some(Username::from_bare(username)?),
        Some("Synthetic User".to_owned()),
    ))
}

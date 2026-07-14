use std::error::Error;
use std::io::{self, Write};
use std::str::FromStr;

use super::super::{write_balance, write_friends, write_payment_methods, write_user_search};
use crate::features::people::User;
use crate::features::people::friends::FriendsResult;
use crate::features::people::users::UserSearchResult;
use crate::features::wallet::balance::BalanceResult;
use crate::features::wallet::payment_methods::PaymentMethodsResult;
use crate::features::wallet::{Balance, PaymentMethod, PaymentMethodId, SignedUsdAmount};
use crate::shared::{Offset, UserId, Username};

type TestResult = Result<(), Box<dyn Error>>;

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

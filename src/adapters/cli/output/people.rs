use std::io::{self, Write};

use crate::features::people::UserProfileKind;
use crate::features::people::friends::FriendsResult;
use crate::features::people::info::UserInfoResult;
use crate::features::people::users::UserSearchResult;
use crate::shared::Offset;

use super::shared::{sanitize_terminal_text, write_users_table};

pub(crate) fn write_user_info(writer: &mut impl Write, result: &UserInfoResult) -> io::Result<()> {
    let user = result.user();
    let username = user
        .username()
        .map(ToString::to_string)
        .unwrap_or_else(|| "(not provided)".to_owned());
    let profile_kind = match user.profile_kind() {
        Some(UserProfileKind::Personal) => "personal",
        Some(UserProfileKind::Business) => "business",
        Some(UserProfileKind::Charity) => "charity",
        Some(UserProfileKind::Unknown) => "unknown",
        None => "(not provided)",
    };
    let payable = match user.is_payable() {
        Some(true) => "yes",
        Some(false) => "no",
        None => "(not provided)",
    };
    writeln!(
        writer,
        "User ID: {}",
        sanitize_terminal_text(user.user_id().as_str())
    )?;
    writeln!(writer, "Username: {}", sanitize_terminal_text(&username))?;
    writeln!(
        writer,
        "Display name: {}",
        sanitize_terminal_text(user.display_name().unwrap_or("(not provided)"))
    )?;
    writeln!(writer, "Profile kind: {profile_kind}")?;
    writeln!(writer, "Payable: {payable}")
}

pub(crate) fn write_user_search<W: Write, E: Write>(
    stdout: &mut W,
    stderr: &mut E,
    result: &UserSearchResult,
) -> io::Result<()> {
    if result.users().is_empty() {
        writeln!(stdout, "No users found.")?;
    } else {
        write_users_table(stdout, result.users())?;
    }

    write_next_offset(stderr, result.next_offset())
}

pub(crate) fn write_friends<W: Write, E: Write>(
    stdout: &mut W,
    stderr: &mut E,
    result: &FriendsResult,
) -> io::Result<()> {
    if result.users().is_empty() {
        writeln!(stdout, "No friends found.")?;
    } else {
        write_users_table(stdout, result.users())?;
    }
    write_next_offset(stderr, result.next_offset())
}

fn write_next_offset(writer: &mut impl Write, offset: Option<Offset>) -> io::Result<()> {
    match offset {
        Some(offset) => writeln!(writer, "Next offset: {offset}"),
        None => Ok(()),
    }
}

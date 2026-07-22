use std::io::{self, Write};

use crate::features::people::UserProfileKind;
use crate::features::people::friends::FriendsResult;
use crate::features::people::friendship::{FriendshipMutationResult, FriendshipPlan};
use crate::features::people::info::UserInfoResult;
use crate::features::people::users::UserSearchResult;
use crate::shared::Offset;

use super::super::response::HumanSource;
use super::shared::{sanitize_terminal_text, write_users_table};

pub(crate) fn write_user_info(
    writer: &mut impl Write,
    response: &impl HumanSource<UserInfoResult>,
) -> io::Result<()> {
    let result = response.human_source();
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
    let friendship = user
        .friendship_status()
        .map(friendship_status)
        .unwrap_or("(not provided)");
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
    writeln!(writer, "Payable: {payable}")?;
    writeln!(writer, "Friendship: {friendship}")
}

pub(crate) fn write_user_search<W: Write, E: Write>(
    stdout: &mut W,
    stderr: &mut E,
    response: &impl HumanSource<UserSearchResult>,
) -> io::Result<()> {
    let result = response.human_source();
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
    response: &impl HumanSource<FriendsResult>,
) -> io::Result<()> {
    let result = response.human_source();
    if result.users().is_empty() {
        match result.subject() {
            Some(subject) => writeln!(
                stdout,
                "No visible friends found for {}.",
                sanitize_terminal_text(&subject.username().to_string())
            )?,
            None => writeln!(stdout, "No friends found.")?,
        }
    } else {
        if let Some(subject) = result.subject() {
            writeln!(
                stdout,
                "Friends for {}",
                sanitize_terminal_text(&subject.username().to_string())
            )?;
        }
        write_users_table(stdout, result.users())?;
    }
    write_next_offset(stderr, result.next_offset())
}

pub(crate) fn write_friendship_details(
    writer: &mut impl Write,
    response: &impl HumanSource<FriendshipPlan>,
) -> io::Result<()> {
    let plan = response.human_source();
    let target = plan.target();
    let username = target
        .username()
        .map(ToString::to_string)
        .unwrap_or_else(|| "(not provided)".to_owned());
    writeln!(writer, "Friendship details:")?;
    writeln!(
        writer,
        "  From account: {} (ID {})",
        sanitize_terminal_text(&plan.account().username().to_string()),
        plan.account().user_id()
    )?;
    writeln!(
        writer,
        "  Target: {} (ID {})",
        sanitize_terminal_text(&username),
        target.user_id()
    )?;
    writeln!(writer, "  Action: {}", plan.action().label())?;
    writeln!(
        writer,
        "  Current relationship: {}",
        friendship_status(plan.previous_status())
    )?;
    writeln!(writer, "  Automatic retries: disabled")
}

pub(crate) fn write_friendship_result(
    writer: &mut impl Write,
    response: &impl HumanSource<FriendshipMutationResult>,
) -> io::Result<()> {
    let result = response.human_source();
    let target = result.plan().target();
    let username = target
        .username()
        .map(ToString::to_string)
        .unwrap_or_else(|| "(not provided)".to_owned());
    writeln!(writer, "Action: {}", result.plan().action().label())?;
    writeln!(writer, "Target: {}", sanitize_terminal_text(&username))?;
    writeln!(writer, "Result: {}", result.plan().action().result_label())
}

const fn friendship_status(status: crate::features::people::FriendshipStatus) -> &'static str {
    match status {
        crate::features::people::FriendshipStatus::Friend => "friend",
        crate::features::people::FriendshipStatus::NotFriend => "not friend",
        crate::features::people::FriendshipStatus::RequestReceived => "incoming request",
        crate::features::people::FriendshipStatus::RequestSent => "outgoing request",
    }
}

fn write_next_offset(writer: &mut impl Write, offset: Option<Offset>) -> io::Result<()> {
    match offset {
        Some(offset) => writeln!(writer, "Next offset: {offset}"),
        None => Ok(()),
    }
}

use std::io::{self, Write};

use crate::features::people::friends::FriendsResult;
use crate::features::people::users::UserSearchResult;
use crate::shared::Offset;

use super::shared::write_users_table;

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

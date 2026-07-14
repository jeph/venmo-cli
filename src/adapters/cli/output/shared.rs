use std::io::{self, Write};

use tabled::{builder::Builder, settings::Style};

use crate::features::people::User;

#[must_use]
pub(crate) fn sanitize_terminal_text(value: &str) -> String {
    let mut sanitized = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\n' => sanitized.push_str("\\n"),
            '\r' => sanitized.push_str("\\r"),
            '\t' => sanitized.push_str("\\t"),
            character if character.is_control() || is_invisible_format_control(character) => {
                sanitized.push_str("\\u{");
                sanitized.push_str(&format!("{:04X}", u32::from(character)));
                sanitized.push('}');
            }
            character => sanitized.push(character),
        }
    }
    sanitized
}

pub(super) fn user_label(user: &User) -> String {
    user.username()
        .map(ToString::to_string)
        .unwrap_or_else(|| user.user_id().to_string())
}

pub(super) fn financial_user_label(user: &User) -> String {
    let identity = user_label(user);
    match user.display_name() {
        Some(display_name) => format!("{identity} ({display_name})"),
        None => identity,
    }
}

pub(super) fn write_users_table(writer: &mut impl Write, users: &[User]) -> io::Result<()> {
    let mut builder = Builder::default();
    builder.push_record(["ID", "USERNAME", "NAME"]);
    for user in users {
        let username = user.username().map(ToString::to_string).unwrap_or_default();
        builder.push_record([
            sanitize_terminal_text(user.user_id().as_str()),
            sanitize_terminal_text(&username),
            sanitize_terminal_text(user.display_name().unwrap_or("")),
        ]);
    }
    write_table(writer, builder)
}

pub(super) fn write_table(writer: &mut impl Write, builder: Builder) -> io::Result<()> {
    let mut table = builder.build();
    table.with(Style::psql());
    for line in table.to_string().lines() {
        writeln!(writer, "{}", line.trim_end())?;
    }
    Ok(())
}

fn is_invisible_format_control(character: char) -> bool {
    matches!(
        character,
        '\u{00AD}'
            | '\u{061C}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{FEFF}'
    )
}

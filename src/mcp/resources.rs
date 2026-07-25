use clap::{Command as ClapCommand, CommandFactory};
use rmcp::model::{Resource, ResourceContents};

use crate::cli::Cli;

#[derive(Clone, Copy, Debug)]
pub(crate) struct HelpPage {
    pub(crate) path: &'static [&'static str],
}

pub(crate) const HELP_PAGES: [HelpPage; 37] = [
    HelpPage { path: &[] },
    HelpPage { path: &["auth"] },
    HelpPage {
        path: &["auth", "login"],
    },
    HelpPage {
        path: &["auth", "logout"],
    },
    HelpPage {
        path: &["auth", "status"],
    },
    HelpPage { path: &["pay"] },
    HelpPage {
        path: &["pay", "options"],
    },
    HelpPage {
        path: &["pay", "user"],
    },
    HelpPage { path: &["friends"] },
    HelpPage {
        path: &["friends", "list"],
    },
    HelpPage {
        path: &["friends", "add"],
    },
    HelpPage {
        path: &["friends", "remove"],
    },
    HelpPage { path: &["users"] },
    HelpPage {
        path: &["users", "search"],
    },
    HelpPage {
        path: &["users", "info"],
    },
    HelpPage { path: &["balance"] },
    HelpPage {
        path: &["activity"],
    },
    HelpPage {
        path: &["activity", "list"],
    },
    HelpPage {
        path: &["activity", "info"],
    },
    HelpPage {
        path: &["activity", "comments"],
    },
    HelpPage {
        path: &["activity", "comments", "list"],
    },
    HelpPage {
        path: &["activity", "comments", "add"],
    },
    HelpPage {
        path: &["activity", "comments", "remove"],
    },
    HelpPage {
        path: &["activity", "reactions"],
    },
    HelpPage {
        path: &["activity", "reactions", "list"],
    },
    HelpPage {
        path: &["activity", "reactions", "add"],
    },
    HelpPage {
        path: &["activity", "reactions", "remove"],
    },
    HelpPage {
        path: &["requests"],
    },
    HelpPage {
        path: &["requests", "list"],
    },
    HelpPage {
        path: &["requests", "create"],
    },
    HelpPage {
        path: &["requests", "accept"],
    },
    HelpPage {
        path: &["requests", "decline"],
    },
    HelpPage {
        path: &["requests", "cancel"],
    },
    HelpPage {
        path: &["requests", "info"],
    },
    HelpPage {
        path: &["transfer"],
    },
    HelpPage {
        path: &["transfer", "options"],
    },
    HelpPage {
        path: &["transfer", "out"],
    },
];

#[derive(Clone, Copy)]
struct EmbeddedDocument {
    uri: &'static str,
    name: &'static str,
    title: &'static str,
    description: &'static str,
    text: &'static str,
}

const SKILL_DOCUMENTS: [EmbeddedDocument; 10] = [
    EmbeddedDocument {
        uri: "venmo://skill/venmo-cli",
        name: "skill.venmo-cli",
        title: "Venmo CLI agent skill",
        description: "Complete Venmo CLI agent operating and safety instructions.",
        text: include_str!("../../skills/venmo-cli/SKILL.md"),
    },
    EmbeddedDocument {
        uri: "venmo://skill/authentication",
        name: "skill.authentication",
        title: "Venmo authentication guidance",
        description: "Human-only login, credential storage, logout, and OTP guidance.",
        text: include_str!("../../skills/venmo-cli/references/authentication.md"),
    },
    EmbeddedDocument {
        uri: "venmo://skill/json/auth",
        name: "skill.json.auth",
        title: "Auth JSON reference",
        description: "Command-specific JSON data shapes for auth tools.",
        text: include_str!("../../skills/venmo-cli/references/json/auth.md"),
    },
    EmbeddedDocument {
        uri: "venmo://skill/json/balance",
        name: "skill.json.balance",
        title: "Balance JSON reference",
        description: "Command-specific JSON data shape for balance.",
        text: include_str!("../../skills/venmo-cli/references/json/balance.md"),
    },
    EmbeddedDocument {
        uri: "venmo://skill/json/pay",
        name: "skill.json.pay",
        title: "Pay JSON reference",
        description: "Command-specific JSON data shapes for pay tools.",
        text: include_str!("../../skills/venmo-cli/references/json/pay.md"),
    },
    EmbeddedDocument {
        uri: "venmo://skill/json/users",
        name: "skill.json.users",
        title: "Users JSON reference",
        description: "Command-specific JSON data shapes for user tools.",
        text: include_str!("../../skills/venmo-cli/references/json/users.md"),
    },
    EmbeddedDocument {
        uri: "venmo://skill/json/friends",
        name: "skill.json.friends",
        title: "Friends JSON reference",
        description: "Command-specific JSON data shapes for friend tools.",
        text: include_str!("../../skills/venmo-cli/references/json/friends.md"),
    },
    EmbeddedDocument {
        uri: "venmo://skill/json/activity",
        name: "skill.json.activity",
        title: "Activity JSON reference",
        description: "Command-specific JSON data shapes for activity tools.",
        text: include_str!("../../skills/venmo-cli/references/json/activity.md"),
    },
    EmbeddedDocument {
        uri: "venmo://skill/json/requests",
        name: "skill.json.requests",
        title: "Requests JSON reference",
        description: "Command-specific JSON data shapes for request tools.",
        text: include_str!("../../skills/venmo-cli/references/json/requests.md"),
    },
    EmbeddedDocument {
        uri: "venmo://skill/json/transfers",
        name: "skill.json.transfers",
        title: "Transfers JSON reference",
        description: "Command-specific JSON data shapes for transfer tools.",
        text: include_str!("../../skills/venmo-cli/references/json/transfers.md"),
    },
];

#[derive(Clone, Debug)]
pub(crate) struct ResourceEntry {
    pub(crate) metadata: Resource,
    pub(crate) text: String,
}

#[derive(Clone, Debug)]
pub(crate) struct ResourceCatalog {
    entries: Vec<ResourceEntry>,
}

impl ResourceCatalog {
    pub(crate) fn build() -> Result<Self, String> {
        let mut entries = Vec::with_capacity(HELP_PAGES.len() + SKILL_DOCUMENTS.len());

        for page in HELP_PAGES {
            let text = render_help(page.path)?;
            let uri = help_uri(page.path);
            let command = display_command(page.path);
            let metadata = Resource::new(uri, help_name(page.path))
                .with_title(format!("{command} --help"))
                .with_description(format!("Live long-help output for `{command}`."))
                .with_mime_type("text/plain; charset=utf-8")
                .with_size(byte_len(&text));
            entries.push(ResourceEntry { metadata, text });
        }

        for document in SKILL_DOCUMENTS {
            let text = document.text.to_owned();
            let metadata = Resource::new(document.uri, document.name)
                .with_title(document.title)
                .with_description(document.description)
                .with_mime_type("text/markdown; charset=utf-8")
                .with_size(byte_len(&text));
            entries.push(ResourceEntry { metadata, text });
        }

        Ok(Self { entries })
    }

    pub(crate) fn list(&self) -> Vec<Resource> {
        self.entries
            .iter()
            .map(|entry| entry.metadata.clone())
            .collect()
    }

    pub(crate) fn read(&self, uri: &str) -> Option<ResourceContents> {
        self.entries
            .iter()
            .find(|entry| entry.metadata.uri == uri)
            .map(|entry| {
                ResourceContents::text(entry.text.clone(), entry.metadata.uri.clone())
                    .with_mime_type(
                        entry
                            .metadata
                            .mime_type
                            .as_deref()
                            .unwrap_or("text/plain; charset=utf-8"),
                    )
            })
    }

    #[cfg(test)]
    pub(crate) fn entries(&self) -> &[ResourceEntry] {
        &self.entries
    }
}

pub(crate) fn render_help(path: &[&str]) -> Result<String, String> {
    let mut command = Cli::command();
    for segment in path {
        command = subcommand(command, segment).ok_or_else(|| {
            format!(
                "the CLI help command path `{}` does not exist",
                display_command(path)
            )
        })?;
    }

    let mut bytes = Vec::new();
    command.write_long_help(&mut bytes).map_err(|error| {
        format!(
            "could not render long help for `{}`: {error}",
            display_command(path)
        )
    })?;
    String::from_utf8(bytes).map_err(|error| {
        format!(
            "long help for `{}` was not UTF-8: {error}",
            display_command(path)
        )
    })
}

pub(crate) fn help_uri(path: &[&str]) -> String {
    if path.is_empty() {
        "venmo://help/venmo".to_owned()
    } else {
        format!("venmo://help/{}", path.join("/"))
    }
}

fn subcommand(command: ClapCommand, name: &str) -> Option<ClapCommand> {
    command
        .get_subcommands()
        .find(|candidate| candidate.get_name() == name)
        .cloned()
}

fn display_command(path: &[&str]) -> String {
    if path.is_empty() {
        "venmo".to_owned()
    } else {
        format!("venmo {}", path.join(" "))
    }
}

fn help_name(path: &[&str]) -> String {
    if path.is_empty() {
        "help.venmo".to_owned()
    } else {
        format!("help.{}", path.join("."))
    }
}

fn byte_len(text: &str) -> u64 {
    u64::try_from(text.len()).unwrap_or(u64::MAX)
}

#[cfg(test)]
pub(crate) fn embedded_skill_documents() -> impl Iterator<Item = (&'static str, &'static str)> {
    SKILL_DOCUMENTS
        .iter()
        .map(|document| (document.uri, document.text))
}

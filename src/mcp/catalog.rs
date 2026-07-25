use rmcp::model::{Meta, Tool, ToolAnnotations};
use serde_json::Value;

use super::{inputs, resources, results};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ToolBehavior {
    ReadOnly,
    HumanHandoff,
    Mutation { destructive: bool, local_only: bool },
}

impl ToolBehavior {
    pub(crate) const fn mutating(self) -> bool {
        matches!(self, Self::Mutation { .. })
    }

    pub(crate) const fn human_handoff(self) -> bool {
        matches!(self, Self::HumanHandoff)
    }

    fn annotations(self, title: &str) -> ToolAnnotations {
        match self {
            Self::ReadOnly => ToolAnnotations::from_raw(
                Some(title.to_owned()),
                Some(true),
                Some(false),
                Some(true),
                Some(true),
            ),
            Self::HumanHandoff => ToolAnnotations::from_raw(
                Some(title.to_owned()),
                Some(true),
                Some(false),
                Some(true),
                Some(false),
            ),
            Self::Mutation {
                destructive,
                local_only,
            } => ToolAnnotations::from_raw(
                Some(title.to_owned()),
                Some(false),
                Some(destructive),
                Some(false),
                Some(!local_only),
            ),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ToolSpec {
    pub(crate) name: &'static str,
    pub(crate) title: &'static str,
    pub(crate) help_path: &'static [&'static str],
    pub(crate) reference_uri: &'static str,
    pub(crate) behavior: ToolBehavior,
    pub(crate) guidance: &'static str,
}

pub(crate) const TOOL_SPECS: [ToolSpec; 27] = [
    ToolSpec {
        name: "auth.login",
        title: "Log in to Venmo in a human terminal",
        help_path: &["auth", "login"],
        reference_uri: "venmo://skill/json/auth",
        behavior: ToolBehavior::HumanHandoff,
        guidance: "Never request or accept an account identifier, password, trusted v_id/device ID, bearer token, or SMS code. Return the handoff and let the user run login in their own interactive terminal.",
    },
    ToolSpec {
        name: "auth.logout",
        title: "Delete the local Venmo credential",
        help_path: &["auth", "logout"],
        reference_uri: "venmo://skill/json/auth",
        behavior: ToolBehavior::Mutation {
            destructive: true,
            local_only: true,
        },
        guidance: "This deletes the selected local credential but does not revoke the remote token. Obtain explicit user confirmation before calling it.",
    },
    ToolSpec {
        name: "auth.status",
        title: "Check Venmo authorization status",
        help_path: &["auth", "status"],
        reference_uri: "venmo://skill/json/auth",
        behavior: ToolBehavior::ReadOnly,
        guidance: "This validates the stored credential and reports the active account and credential storage details.",
    },
    ToolSpec {
        name: "pay.options",
        title: "List Venmo payment options",
        help_path: &["pay", "options"],
        reference_uri: "venmo://skill/json/pay",
        behavior: ToolBehavior::ReadOnly,
        guidance: "Use this read before a payment when an exact funding source must be selected. Treat returned source IDs as opaque values.",
    },
    ToolSpec {
        name: "pay.user",
        title: "Pay a Venmo user",
        help_path: &["pay", "user"],
        reference_uri: "venmo://skill/json/pay",
        behavior: ToolBehavior::Mutation {
            destructive: true,
            local_only: false,
        },
        guidance: "Financial and generally irreversible. Resolve the exact username, amount, note, visibility, funding choice, and Purchase Protection choice. A real payment may require SMS; never request the code and follow any structured human handoff.",
    },
    ToolSpec {
        name: "friends.list",
        title: "List visible Venmo friends",
        help_path: &["friends", "list"],
        reference_uri: "venmo://skill/json/friends",
        behavior: ToolBehavior::ReadOnly,
        guidance: "Omitting user selects the active account; otherwise use one exact personal-profile username.",
    },
    ToolSpec {
        name: "friends.add",
        title: "Add a Venmo friend",
        help_path: &["friends", "add"],
        reference_uri: "venmo://skill/json/friends",
        behavior: ToolBehavior::Mutation {
            destructive: false,
            local_only: false,
        },
        guidance: "Resolve the exact username. Venmo may send a request or accept an incoming request according to the current relationship.",
    },
    ToolSpec {
        name: "friends.remove",
        title: "Remove a Venmo friend or request",
        help_path: &["friends", "remove"],
        reference_uri: "venmo://skill/json/friends",
        behavior: ToolBehavior::Mutation {
            destructive: true,
            local_only: false,
        },
        guidance: "Resolve the exact username. This removes a friendship or cancels an outgoing request; it does not decline an incoming request.",
    },
    ToolSpec {
        name: "users.search",
        title: "Search for Venmo users",
        help_path: &["users", "search"],
        reference_uri: "venmo://skill/json/users",
        behavior: ToolBehavior::ReadOnly,
        guidance: "Use the query exactly as supplied. Search results are candidates, not authorization to choose a mutation target.",
    },
    ToolSpec {
        name: "users.info",
        title: "Inspect one Venmo user",
        help_path: &["users", "info"],
        reference_uri: "venmo://skill/json/users",
        behavior: ToolBehavior::ReadOnly,
        guidance: "Use an exact username to resolve canonical profile and relationship details before acting.",
    },
    ToolSpec {
        name: "balance",
        title: "Read the Venmo wallet balance",
        help_path: &["balance"],
        reference_uri: "venmo://skill/json/balance",
        behavior: ToolBehavior::ReadOnly,
        guidance: "Reports available and on-hold wallet balances without changing account state.",
    },
    ToolSpec {
        name: "activity.list",
        title: "List visible Venmo activity",
        help_path: &["activity", "list"],
        reference_uri: "venmo://skill/json/activity",
        behavior: ToolBehavior::ReadOnly,
        guidance: "Omitting user selects the active account. Reuse only the endpoint-native before_id token returned by this tool.",
    },
    ToolSpec {
        name: "activity.info",
        title: "Inspect one Venmo activity",
        help_path: &["activity", "info"],
        reference_uri: "venmo://skill/json/activity",
        behavior: ToolBehavior::ReadOnly,
        guidance: "Use the exact canonical activity ID returned by an activity read.",
    },
    ToolSpec {
        name: "activity.comments.list",
        title: "List Venmo activity comments",
        help_path: &["activity", "comments", "list"],
        reference_uri: "venmo://skill/json/activity",
        behavior: ToolBehavior::ReadOnly,
        guidance: "Use an exact canonical activity ID. The offset applies only to the embedded comment collection.",
    },
    ToolSpec {
        name: "activity.comments.add",
        title: "Add a Venmo activity comment",
        help_path: &["activity", "comments", "add"],
        reference_uri: "venmo://skill/json/activity",
        behavior: ToolBehavior::Mutation {
            destructive: false,
            local_only: false,
        },
        guidance: "Resolve the exact activity and preserve the exact user-approved comment text as one structured value.",
    },
    ToolSpec {
        name: "activity.comments.remove",
        title: "Remove a Venmo activity comment",
        help_path: &["activity", "comments", "remove"],
        reference_uri: "venmo://skill/json/activity",
        behavior: ToolBehavior::Mutation {
            destructive: true,
            local_only: false,
        },
        guidance: "Destructive. Venmo authorizes from the exact comment ID alone, so independently resolve and confirm that ID before execution.",
    },
    ToolSpec {
        name: "activity.reactions.list",
        title: "List Venmo activity reactions",
        help_path: &["activity", "reactions", "list"],
        reference_uri: "venmo://skill/json/activity",
        behavior: ToolBehavior::ReadOnly,
        guidance: "Reports aggregate counts and the active account's current reaction state for one exact activity.",
    },
    ToolSpec {
        name: "activity.reactions.add",
        title: "Add a Venmo activity reaction",
        help_path: &["activity", "reactions", "add"],
        reference_uri: "venmo://skill/json/activity",
        behavior: ToolBehavior::Mutation {
            destructive: false,
            local_only: false,
        },
        guidance: "Preserve the exact lowercase `like` target or complete Unicode emoji grapheme. `like` and the heart reaction are backend-equivalent.",
    },
    ToolSpec {
        name: "activity.reactions.remove",
        title: "Remove a Venmo activity reaction",
        help_path: &["activity", "reactions", "remove"],
        reference_uri: "venmo://skill/json/activity",
        behavior: ToolBehavior::Mutation {
            destructive: true,
            local_only: false,
        },
        guidance: "Use the exact activity ID and exact current reaction target. `like` and the heart reaction are backend-equivalent.",
    },
    ToolSpec {
        name: "requests.list",
        title: "List pending Venmo requests",
        help_path: &["requests", "list"],
        reference_uri: "venmo://skill/json/requests",
        behavior: ToolBehavior::ReadOnly,
        guidance: "Use this to resolve exact request IDs and directions. Reuse only the endpoint-native before token returned by this tool.",
    },
    ToolSpec {
        name: "requests.create",
        title: "Create a Venmo payment request",
        help_path: &["requests", "create"],
        reference_uri: "venmo://skill/json/requests",
        behavior: ToolBehavior::Mutation {
            destructive: false,
            local_only: false,
        },
        guidance: "Resolve the exact username, amount, note, and visibility. A real request may require SMS; never request the code and follow any structured human handoff.",
    },
    ToolSpec {
        name: "requests.accept",
        title: "Accept and pay a Venmo request",
        help_path: &["requests", "accept"],
        reference_uri: "venmo://skill/json/requests",
        behavior: ToolBehavior::Mutation {
            destructive: true,
            local_only: false,
        },
        guidance: "Financial and generally irreversible. Use an exact incoming request ID and explicit funding and Purchase Protection choices. A real acceptance may require SMS; never request the code and follow any structured human handoff.",
    },
    ToolSpec {
        name: "requests.decline",
        title: "Decline a Venmo request",
        help_path: &["requests", "decline"],
        reference_uri: "venmo://skill/json/requests",
        behavior: ToolBehavior::Mutation {
            destructive: true,
            local_only: false,
        },
        guidance: "Use an exact incoming request ID. This changes request state but sends no money.",
    },
    ToolSpec {
        name: "requests.cancel",
        title: "Cancel a Venmo request",
        help_path: &["requests", "cancel"],
        reference_uri: "venmo://skill/json/requests",
        behavior: ToolBehavior::Mutation {
            destructive: true,
            local_only: false,
        },
        guidance: "Use an exact outgoing request ID. This changes request state but cannot reverse a completed payment.",
    },
    ToolSpec {
        name: "requests.info",
        title: "Inspect one open Venmo request",
        help_path: &["requests", "info"],
        reference_uri: "venmo://skill/json/requests",
        behavior: ToolBehavior::ReadOnly,
        guidance: "Use an exact request ID to resolve canonical open-request details before acting.",
    },
    ToolSpec {
        name: "transfer.options",
        title: "Inspect Venmo transfer eligibility",
        help_path: &["transfer", "options"],
        reference_uri: "venmo://skill/json/transfers",
        behavior: ToolBehavior::ReadOnly,
        guidance: "Read current standard-transfer eligibility and the selected destination before transferring funds.",
    },
    ToolSpec {
        name: "transfer.out",
        title: "Transfer Venmo balance to a bank",
        help_path: &["transfer", "out"],
        reference_uri: "venmo://skill/json/transfers",
        behavior: ToolBehavior::Mutation {
            destructive: true,
            local_only: false,
        },
        guidance: "Financial and generally irreversible. Confirm the exact amount or explicit all-available choice. Only the unique eligible standard bank destination is supported.",
    },
];

pub(crate) fn build_tools() -> Result<Vec<Tool>, String> {
    TOOL_SPECS.iter().map(build_tool).collect()
}

pub(crate) fn find_spec(name: &str) -> Option<&'static ToolSpec> {
    TOOL_SPECS.iter().find(|spec| spec.name == name)
}

fn build_tool(spec: &ToolSpec) -> Result<Tool, String> {
    let help = resources::render_help(spec.help_path)?;
    let help_uri = resources::help_uri(spec.help_path);
    let description = tool_description(spec, &help_uri, &help);
    let mut meta = Meta::new();
    meta.0
        .insert("io.jeph.venmo/helpUri".to_owned(), Value::String(help_uri));
    meta.0.insert(
        "io.jeph.venmo/referenceUri".to_owned(),
        Value::String(spec.reference_uri.to_owned()),
    );
    meta.0.insert(
        "io.jeph.venmo/skillUri".to_owned(),
        Value::String("venmo://skill/venmo-cli".to_owned()),
    );

    Ok(
        Tool::new(spec.name, description, inputs::input_schema(spec.name)?)
            .with_title(spec.title)
            .with_raw_output_schema(results::output_schema(spec.name))
            .with_annotations(spec.behavior.annotations(spec.title))
            .with_meta(meta),
    )
}

fn tool_description(spec: &ToolSpec, help_uri: &str, help: &str) -> String {
    let behavior = match spec.behavior {
        ToolBehavior::ReadOnly => {
            "This is a read-only API operation. It can contact Venmo but does not mutate account state."
        }
        ToolBehavior::HumanHandoff => {
            "This MCP tool never performs login and never initializes credentials or network services. It only returns instructions for a human-operated terminal login."
        }
        ToolBehavior::Mutation {
            local_only: true, ..
        } => {
            "This local mutation has no dry-run mode and executes immediately when called. The MCP server does not ask for confirmation; the MCP client/LLM must obtain explicit user approval first."
        }
        ToolBehavior::Mutation {
            local_only: false, ..
        } => {
            "This mutation requires `dry_run`. `dry_run: true` performs preflight and returns its plan without writing. `dry_run: false` supplies CLI `--yes` and executes immediately. The MCP server does not ask for confirmation; the MCP client/LLM must confirm the exact action first. Tool annotations conservatively describe the real execution capability even for a dry-run call."
        }
    };

    format!(
        "{behavior}\n\nSafety guidance:\n- {specific}\n- Resolve every dynamic target and choice exactly; do not infer IDs, usernames, amounts, text, visibility, protection, funding, or transfer choices.\n- Never put passwords, bearer tokens, trusted v_id/device IDs, login identifiers, or SMS OTP values in MCP arguments.\n- Inspect failure `error.outcome`: retry only `not_performed` after correcting the cause. Never blindly retry `partial`, `unknown`, or `completed`.\n- Preserve notes, comments, reaction glyphs, searches, IDs, and continuation tokens as individual structured values. Never evaluate output as shell syntax.\n\nResources:\n- CLI help: {help_uri}\n- Command JSON shapes: {reference_uri}\n- Complete agent skill: venmo://skill/venmo-cli\n- Authentication and OTP guidance: venmo://skill/authentication\n\nCurrent CLI long help (authoritative):\n\n{help}",
        specific = spec.guidance,
        reference_uri = spec.reference_uri,
    )
}

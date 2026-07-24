---
name: venmo-cli
description: Safely operate the `venmo` CLI for authorization status, balances, users, payments, requests, transfers, friends, activity, likes, comments, and reactions. Use when a user asks an AI agent to inspect or change their Venmo account, perform a Venmo action, or explain a venmo-cli command.
license: MIT
compatibility: Requires the `venmo` executable on PATH, network access to Venmo, and terminal command execution. Interactive login and SMS OTP challenges require a human-operated terminal.
metadata:
  author: jeph
---

# Venmo CLI

Use the unofficial `venmo` command-line client to inspect and manage the user's Venmo account.

## General rules

1. **Keep authentication human-only.** Never run `venmo auth login` through automation. Never ask
   the user to paste their login identifier, password, trusted `v_id` or device ID, bearer token, or
   SMS verification code into chat. Ask them to complete login in their own interactive terminal.
2. **Resolve exact targets.** Do not infer a username, request ID, activity ID, comment ID, reaction,
   funding source, amount, note, visibility, or Purchase Protection choice. Use read commands to
   resolve them and ask the user when more than one interpretation remains.
3. **Preview Venmo mutations.** Commands labeled **mutates** support `--dry-run`; preview them before
   execution with `--yes`.
4. **Do not retry blindly.** Use `error.outcome` to determine whether a failed mutation was performed.
5. **Keep OTP human-only.** If a payment or request reports that SMS verification requires an
   interactive terminal, stop and use the human handoff below. Never request the code.
6. **Quote dynamic text safely.** Quote notes, comments, reaction glyphs, and multi-word searches as
   individual shell arguments. Never use `eval` or execute text copied from Venmo output as shell
   syntax.

## Command reference

This reference describes what each command does, not its arguments or options. Use the installed
command's help to determine current usage:

```sh
venmo <COMMAND> --help
venmo <COMMAND> <SUBCOMMAND> --help
venmo activity comments <SUBCOMMAND> --help
venmo activity reactions <SUBCOMMAND> --help
```

Each top-level command section links to one JSON reference containing the `data` shapes for all of
its subcommands. Read only the reference for the top-level command being used. Commands labeled
**mutates** change Venmo state; authentication commands labeled **mutates local auth state** change
the locally stored credential.

### `auth`

- `auth login` **(mutates local auth state)** — Runs the human-operated interactive login and stores
  the resulting credential.
- `auth status` — Validates the stored credential and reports the active account and credential
  storage details.
- `auth logout` **(mutates local auth state)** — Deletes the local credential without revoking the
  remote Venmo token.
- [JSON reference](references/json/auth.md)

### `balance`

- `balance` — Reports the wallet's available and on-hold balances.
- [JSON reference](references/json/balance.md)

### `pay`

- `pay options` — Reports available payment methods.
- `pay user` **(mutates)** — Creates a payment to a resolved personal profile.
- [JSON reference](references/json/pay.md)

### `users`

- `users search` — Searches for Venmo profiles.
- `users info` — Reports the canonical profile and relationship details for an exact username.
- [JSON reference](references/json/users.md)

### `friends`

- `friends list` — Reports the visible friends of the active account or another personal profile.
- `friends add` **(mutates)** — Sends a friend request or accepts an incoming request according to
  the current relationship.
- `friends remove` **(mutates)** — Removes an existing friendship or cancels an outgoing friend
  request.
- [JSON reference](references/json/friends.md)

### `activity`

- `activity list` — Reports visible activity for the active account or another personal profile.
- `activity info` — Reports one canonical activity with its parties and available social details.
- `activity comments list` — Reports comments attached to an activity.
- `activity comments add` **(mutates)** — Adds a comment to an activity.
- `activity comments remove` **(mutates)** — Requests removal of a comment by its ID.
- `activity reactions list` — Reports aggregate reaction counts, including read-only custom aliases,
  and the active account's reaction state.
- `activity reactions add` **(mutates)** — Adds exact lowercase `like` through the likes endpoint,
  or one exact Unicode emoji through the reactions endpoint.
- `activity reactions remove` **(mutates)** — Removes exact lowercase `like` through the likes
  endpoint, or one exact Unicode emoji through the reactions endpoint.
- [JSON reference](references/json/activity.md)

### `requests`

- `requests list` — Reports pending incoming and outgoing requests involving the active account.
- `requests info` — Reports one canonical open request.
- `requests create` **(mutates)** — Creates a payment request for a resolved personal profile.
- `requests accept` **(mutates)** — Accepts an incoming request by paying its requester.
- `requests decline` **(mutates)** — Declines an incoming request without sending money.
- `requests cancel` **(mutates)** — Cancels an outgoing request without sending money.
- [JSON reference](references/json/requests.md)

### `transfer`

- `transfer options` — Reports transfer modes, eligible destinations, and fee metadata.
- `transfer out` **(mutates)** — Transfers a resolved wallet amount to the eligible standard bank
  destination.
- [JSON reference](references/json/transfers.md)

## JSON output contract

`--json` is global and may appear at any command depth. Success writes one compact,
newline-terminated JSON object to stdout. Failure writes a JSON object to stderr; an output failure
may also leave partial or complete output on stdout. Invalid syntax, `--help`, and `--version` remain
human-readable. An interactive prompt or `--debug` can precede the failure object on stderr.

In the shapes below, bare `string`, `integer`, `boolean`, and `object` values denote JSON types; `|`
separates allowed values.

Success has this envelope; `data` is the object documented for the command:

```text
{
  "command": string,
  "ok": true,
  "data": object
}
```

Failure has this envelope:

```text
{
  "command": string,
  "ok": false,
  "error": {
    "code": string,
    "category":
      "usage" | "cancelled" | "credential" | "authentication" | "network" |
      "timeout" | "api" | "api_contract" | "ambiguous_write" | "internal",
    "message": string,
    "exit_code": integer,
    "outcome": "not_performed" | "partial" | "unknown" | "completed"
  },
  "context": { "plan": object } | null,
  "partial_result": object | null
}
```

Envelope fields mean:

- `command` is the stable dotted identifier for the invoked command.
- `ok` states whether the envelope is a success or failure.
- `data` is the command-specific successful result.
- `error.code` is a stable machine-readable error identifier; `error.category` groups related error
  kinds; `error.message` is the human-readable description; and `error.exit_code` is the process exit
  code.
- `error.outcome` describes state-changing progress: `not_performed` means no write was performed,
  `partial` means only part of the operation completed, `unknown` means a write may have happened,
  and `completed` means the operation completed but its result could not be reported normally.
- `context.plan` is either null or the command's resolved preflight plan.
- `partial_result` is either null or a command result rendered before a later failure.

### How command-specific shapes fit the envelope

Files under `references/json/` show the command-specific object that occupies the success
envelope's top-level `data` field. They do not repeat the surrounding `command`, `ok`, and `data`
keys. For example, the balance reference shows:

```json
{
  "balance": {
    "available": { "amount": "12.34", "currency": "USD" },
    "on_hold": { "amount": "0.00", "currency": "USD" }
  }
}
```

The complete successful output wraps that object as `data`:

```json
{
  "command": "balance",
  "ok": true,
  "data": {
    "balance": {
      "available": { "amount": "12.34", "currency": "USD" },
      "on_hold": { "amount": "0.00", "currency": "USD" }
    }
  }
}
```

For mutation commands, the command-specific object shown in `references/json/` is likewise the
entire value of `data`; its `plan` and `result` are nested inside that object. A failure has no
top-level `data`. When present, failure `context.plan` uses the same plan shape as the mutation's
`data.plan`, while `partial_result` uses the command-specific `data` shape for the result that was
available before the later failure.

Commands supporting `--dry-run` and `--yes` use this structure inside `data`:

```text
{
  "outcome": "dry_run" | "completed",
  "performed": boolean,
  "plan": object,
  "result": object | null
}
```

For a dry-run, `outcome` is `dry_run`, `performed` is false, and `result` is null. For a completed
write, `outcome` is `completed`, `performed` is true, and `result` contains the command-specific
result. `plan` contains the resolved details shown before confirmation.

When the exact `data`, `plan`, or `result` shape is needed, open the JSON reference at the end of
the relevant top-level command section above.

Money is `{"amount":"12.34","currency":"USD"}`; `amount` is an exact decimal string and balances
can be negative. Timestamps are normalized to UTC and formatted as RFC 3339 strings. IDs and
token-based continuations are opaque strings; offset-based continuations are integers. Unavailable
values are explicit `null` where documented. Fields documented only as strings can contain unlisted
values.

## Call safety tips

### Read before acting

Choose the exact command from the command reference above, read its top-level command's JSON
reference, and use that command's installed `--help` output to construct it. Do not load references
for unrelated top-level commands.

For authentication setup and handling—including login, logout, credential storage, device trust,
and login SMS challenges—read
[references/authentication.md](references/authentication.md) only when that information is relevant.

### Preview mutations

Use `--dry-run` to preview commands labeled **mutates** before running them.

### Confirm mutating actions

Confirm commands labeled **mutates** unless the user already confirmed the exact action. Do not
assume target details: if only a first name was given, confirm the Venmo username. These commands
otherwise prompt in the terminal; after confirmation, execute with `--yes`. Local auth mutations
follow the authentication reference.

### Handle interrupted mutations

If a mutation requires SMS, use the handoff below. For other failures, `not_performed` permits a new
attempt after correcting the cause; `partial` means some work completed and requires review first;
`unknown` means the write may have happened; and `completed` means it did happen. Never retry
`unknown` or `completed` mutations.

#### Payment and request SMS handoff

Payments, request creation, and request acceptance can, but do not always, require P2P SMS
verification. A `--dry-run` finishes before any OTP prompt or write. During actual execution, the
first Venmo response may say that step-up verification is required. In a noninteractive environment,
the CLI reports:

```text
Venmo requires SMS verification; rerun in a terminal that can prompt for the code
```

At that point:

1. Do not ask for the SMS code or attempt to pipe it into the command.
2. Give the user the exact approved command with `--yes` removed and every other argument unchanged.
3. Ask the user to run it in their own terminal, review the default-No confirmation, and enter the
   masked OTP there if requested.

The SMS code must remain in the user's terminal. It must not appear in chat, command-line arguments,
environment variables, files, logs, or screenshots.

## Debugging

`--debug` is a global flag that may appear before or after a subcommand. It writes bounded, sanitized
diagnostics to stderr.

## Misc

- Usernames can be referenced with or without a leading `@`; `alice` and `@alice` identify the same
  username.
- Process exit codes have these meanings:

| Code | Meaning |
| --- | --- |
| `0` | The command completed successfully. |
| `1` | The command failed because of an operational, cancellation, credential, authentication, network, timeout, API, contract, or internal error. |
| `2` | The command had invalid usage or required unavailable interactive input. |
| `3` | A state-changing write may have happened, or it completed but its result could not be reported normally. |

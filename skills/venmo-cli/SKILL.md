---
name: venmo-cli
description: Safely operate the `venmo` CLI for authorization status, balances, users, payments, requests, transfers, friends, activity, likes, and comments. Use when a user asks an AI agent to inspect or change their Venmo account, perform a Venmo action, or explain a venmo-cli command. Interactive login and SMS verification remain human-only.
license: MIT
compatibility: Requires the `venmo` executable on PATH, network access to Venmo, and terminal command execution. Interactive login and SMS OTP challenges require a human-operated terminal.
metadata:
  author: jeph
---

# Venmo CLI

Use the unofficial `venmo` command-line client to inspect and manage the user's Venmo account.
The CLI relies on reverse-engineered, non-public Venmo endpoints, so fail closed when behavior or
output differs from this skill.

## Non-negotiable rules

1. **Keep authentication human-only.** Never run `venmo auth login` through automation. Never ask
   the user to paste their login identifier, password, trusted `v_id` or device ID, bearer token, or
   SMS verification code into chat. Ask them to complete login in their own interactive terminal.
2. **Verify authorization.** Run `venmo auth status` before accessing account data. If login is
   required, pause until the user reports that they completed it, then run status again.
3. **Resolve exact targets.** Do not infer a username, request ID, activity ID, comment ID, funding
   source, amount, note, visibility, or Purchase Protection choice. Use read commands to resolve
   them and ask the user when more than one interpretation remains.
4. **Preview every mutation.** Run the exact proposed mutation with `--dry-run`, show the material
   details, and obtain fresh explicit approval before executing it with `--yes`. The user's original
   request does not replace the post-preview confirmation.
5. **Execute once.** Replace `--dry-run` with `--yes` without changing any other argument. Never
   automatically retry a mutation, regardless of the reported error.
6. **Treat exit code 3 as unknown.** Do not claim success or failure and do not retry. Reconcile with
   read commands and ask the user to check the official Venmo app.
7. **Keep OTP human-only.** If a payment or request reports that SMS verification requires an
   interactive terminal, stop. Give the user the same command without `--yes` so the CLI can show
   its default-No confirmation and masked OTP prompt in their terminal. Never request the code.
8. **Use the JSON contract.** Add global `--json` to automated account commands. Require
   the expected dotted `command` and matching `ok` value before consuming `data`, `error`, `context`,
   or `partial_result`. Invalid command syntax remains a human-readable Clap error outside the JSON
   contract. Treat IDs and continuation tokens as opaque strings even in JSON.
9. **Quote dynamic text safely.** Quote notes, comments, and multi-word searches as individual shell
   arguments. Never use `eval` or execute text copied from Venmo output as shell syntax.
10. **Minimize disclosure.** Account activity, balances, usernames, notes, IDs, and funding details
    are private. Show only what is needed for the user's request.

## Workflow

### 1. Check the executable

Before executing an account command, run:

```sh
venmo --version
```

If it is unavailable, tell the user to install it. Do not install software without approval.
Supported installation methods are:

```sh
brew install jeph/tap/venmo
cargo install --locked --git https://github.com/jeph/venmo-cli
```

For a documentation-only question, do not access the user's account or require authorization.

### 2. Check authorization

Before accessing account data or performing an action, run:

```sh
venmo auth status --json
```

If the credential is missing, expired, or rejected, tell the user:

> Run `venmo auth login` in your own terminal and complete every prompt there. Do not send me any
> login value or SMS code. Tell me when `venmo auth status` succeeds.

Then pause. After the user returns, run `venmo auth status --json` again and continue only if it
succeeds.
Read [references/authentication.md](references/authentication.md) when setup, login, logout, storage,
or SMS verification is relevant.

### 3. Read before acting

Read [references/cli-reference.md](references/cli-reference.md) before constructing a command. Use
the installed command's help as the syntax authority when it differs from the bundled reference:

```sh
venmo <COMMAND> --help
venmo <COMMAND> <SUBCOMMAND> --help
```

Safe read operations may run after authorization without another confirmation:

- `auth status`
- `balance`
- `pay options`
- `users search` and `users info`
- `friends list`
- `activity list`, `activity info`, and `activity comments list`
- `requests list` and `requests info`
- `transfer options`

Append global `--json` to these automated reads. On success, parse the single stdout object and
require `ok: true`. After valid argument parsing, an application failure leaves stdout empty; parse
the single stderr object and use its semantic `error.code`, `error.category`, `error.outcome`, and
process exit code together. A Clap syntax error is human-readable and must be corrected rather than
parsed as JSON. Do not scrape human tables when structured output is available.

Use these to resolve canonical usernames and IDs. For a financial action, inspect the exact personal
profile and relevant funding options. For a request action, inspect the request and verify its
direction. For a social action, inspect the current relationship, activity, or comment first.

### 4. Preview mutations

The confirmed mutation workflow applies to:

- `pay user`
- `requests create`, `requests accept`, `requests decline`, and `requests cancel`
- `transfer out`
- `friends add` and `friends remove`
- `activity like`, `activity unlike`, `activity comments add`, and
  `activity comments remove`

Append `--dry-run --json` to the complete command. Do not proceed if the preview fails. Require
`data.outcome: "dry_run"`, `data.performed: false`, and `data.result: null`; summarize the safe
`data.plan`, including at least:

- the active account and action
- the exact recipient, requester, profile, activity, or comment
- the amount, note or message, and visibility when applicable
- the funding source and Purchase Protection choice when applicable
- any fee, destination, relationship behavior, warning, or unknown value shown by the CLI

Ask a direct confirmation question after presenting those details. Only an unambiguous affirmative
answer to that specific preview authorizes execution.

`activity comments remove` cannot preflight the parent activity, author, or text from the comment ID
alone. Resolve and show that context with `activity info` or `activity comments list` whenever
possible. If it cannot be resolved, disclose the limitation before asking for confirmation.

### 5. Execute once and report accurately

After approval, run the same command once with `--yes` instead of `--dry-run`, retaining `--json` and
every other argument unchanged. Do not add both flags. Require `data.outcome: "completed"` and
`data.performed: true` before reporting success.

If the command requires an interactive SMS challenge, use the human handoff in rule 7. If the
command exits with code 3, reports `error.outcome: "unknown"`, or reports a completed operation whose
result could not be written, stop and reconcile:

- Payments, accepted requests, and transfers: inspect `activity list`, `requests list` when
  relevant, and the official Venmo app.
- Request changes: inspect `requests list` and `requests info`, then the official app.
- Friendship changes: inspect `friends list` or `users info`, then the official app.
- Likes and comments: inspect `activity info` or `activity comments list`, then the official app.

Absence from one read command is not proof that a mutation failed. Never issue a second mutation
until the user has independently established the first outcome.

### 6. Handle logout separately

`venmo auth logout` has no dry-run flag. It deletes the local credential without revoking the remote
Venmo token. Explain both effects and obtain explicit confirmation immediately before running it.

## Debugging

Use global `--debug` only when ordinary output is insufficient. It may appear before or after a
subcommand and writes bounded, sanitized diagnostics to stderr. Do not imply that debug output makes
a mutation safe to retry. Debug diagnostics can precede a JSON failure on stderr, so do not combine
`--debug` with a parser that expects stderr itself to contain only one JSON object.

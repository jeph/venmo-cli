# Venmo CLI reference

This reference describes the command surface bundled with this skill. The installed binary is the
syntax authority. If a command rejects documented syntax or exposes different help, stop and inspect
the relevant help instead of guessing:

```sh
venmo --help
venmo <COMMAND> --help
venmo <COMMAND> <SUBCOMMAND> --help
```

Use global `--json` for automated account commands. It emits a structured envelope while preserving
IDs and continuation values as opaque strings that must be copied exactly.

## Global invocation

```sh
venmo [--debug] [--json] <COMMAND>
venmo --help
venmo --version
```

`--debug` is global and may appear before or after a subcommand. It writes bounded diagnostics to
stderr, including request timing, route, status, retry, response size, and sanitized errors. It omits
credentials, raw bodies, dynamic identifiers, usernames, notes, and amounts. Use it only for
troubleshooting.

`--json` is also global and may appear at any command depth. On success, one compact JSON object plus
one newline is written to stdout. On failure, stdout stays empty and one JSON error object is written
to stderr. Help and version remain human-readable. Clean `--dry-run`, `--yes`, and noninteractive
invocations suppress human preflight text; an actual interactive prompt or explicit `--debug` may
write diagnostics before the final JSON object on stderr.

Require the expected dotted `command` and `ok: true` before consuming success `data`. Exact money is
`{"amount":"12.34","currency":"USD"}`, timestamps are RFC 3339 UTC,
collections are arrays, and unavailable values are `null`. Confirmed mutations use
`data.outcome`, `data.performed`, `data.plan`, and nullable `data.result`. Failures use stable
`error.code`, `error.category`, `error.exit_code`, and `error.outcome`, with optional safe
`context.plan` and `partial_result`. Parser failures use `command: null`. See the repository's
`JSON.md` for the complete contract.

## Common values

- `USERNAME` is an exact username with an optional leading `@`.
- `AMOUNT` is a positive USD decimal with at most two fractional digits. Do not use a sign, comma,
  currency symbol, or exponent.
- `NOTE` and `MESSAGE` must contain non-whitespace text. Quote them when they contain spaces or shell
  metacharacters.
- `VISIBILITY` is `private`, `friends`, or `public`. Payment and request creation default to
  `private`.
- `LIMIT` defaults to 10 where offered and must be between 1 and 50.
- `OFFSET` defaults to 0 where offered and must be a nonnegative integer.
- `SOURCE_ID`, request IDs, activity IDs, comment IDs, and continuation tokens must come from current
  CLI output. Never synthesize or transform them.

## Authorization

Read [authentication.md](authentication.md) before handling authentication or SMS verification.

```sh
venmo auth login
venmo auth status
venmo auth logout
```

- `auth login` always uses an interactive human flow. It can request the account identifier, masked
  password, masked trusted `v_id` or device ID, and masked SMS OTP. Secret prompts display one `*`
  per entered character without echoing the underlying value.
- `auth status` validates the stored credential and shows the active account.
- `auth logout` deletes local credential state. It does not contact Venmo or revoke the remote token.

## Balance and payment sources

Show the active account's Venmo wallet balance:

```sh
venmo balance
```

List funding sources eligible for peer payments and their copyable IDs:

```sh
venmo pay options
```

Run `pay options` before a payment or request acceptance when the user wants a particular balance,
bank, or card. Do not identify sources from names or last-four digits alone when multiple rows could
match.

## Pay a user

```sh
venmo pay user <USERNAME> <AMOUNT> <NOTE> \
  [--source <SOURCE_ID>] \
  [--protect] \
  [--visibility private|friends|public] \
  [--dry-run | --yes]
```

Behavior:

- The recipient must resolve to the exact intended payable personal profile. Use `users info` or
  `users search` first when identity is not already certain.
- Without `--source`, the CLI chooses a peer-eligible Venmo balance when the available balance covers
  the payment. Otherwise it uses the unique default or sole eligible external peer method. It fails
  rather than guessing when no unique source can be established.
- `--source` submits that exact peer-eligible balance, bank, or card ID. The CLI never silently
  substitutes another source.
- `--protect` requests Venmo Purchase Protection for an eligible personal payment. Venmo may deduct a
  seller fee, the purchase tag generally cannot be removed, and the tag does not guarantee item
  coverage.
- Visibility defaults to `private`.
- Payment SMS step-up is supported only through an interactive terminal.

Always dry-run, summarize the resolved recipient, amount, note, source, protection choice,
visibility, and any fee, obtain approval, and then execute once with `--yes`.

## Users

Search for profiles:

```sh
venmo users search <QUERY> [--limit <N>] [--offset <N>]
```

- A query beginning with `@`, or a query with no whitespace, is treated as a username search.
- A multi-word query is a general user search and must be quoted as one argument.
- Search text must be nonblank, contain no control characters, and be no more than 1024 bytes.
- `--limit` defaults to 10 and accepts 1 through 50.
- `--offset` defaults to 0.

Inspect one exact profile:

```sh
venmo users info <USERNAME>
```

Use profile information, including profile kind, payability, and relationship when available, to
disambiguate an action. Do not treat a similar display name as proof of identity.

## Friends

List friends visible to the active account:

```sh
venmo friends list [--limit <N>] [--offset <N>]
```

List friends visible on another exact personal profile:

```sh
venmo friends list --user <USERNAME> [--limit <N>] [--offset <N>]
```

The username may include `@`. Venmo privacy settings can hide some or all of another profile's
friends. Limits default to 10 and offsets default to 0.

Create or accept a friendship:

```sh
venmo friends add <USERNAME> [--dry-run | --yes]
```

The native relationship determines the effect. It sends a request to a nonfriend and accepts an
incoming request. Inspect `users info` first and describe the resolved effect rather than promising
one particular behavior in advance.

Remove a friendship or outgoing request:

```sh
venmo friends remove <USERNAME> [--dry-run | --yes]
```

This removes an established friend or cancels an outgoing friend request. It does not decline an
incoming request. Inspect the current relationship before previewing the mutation.

## Activity

List recent activity for the active account:

```sh
venmo activity list [--limit <N>] [--before-id <TOKEN>]
```

List activity visible on another exact personal profile:

```sh
venmo activity list --user <USERNAME> [--limit <N>] [--before-id <TOKEN>]
```

- Limits default to 10 and accept 1 through 50.
- `--before-id` takes only the continuation token printed by an earlier `activity list` call.
- Venmo privacy settings determine which records are visible.
- Amounts are omitted from other-user activity.
- Some activity types, including rewards disbursements, do not provide an amount or status. The
  list leaves unavailable fields blank, and `activity info` omits unavailable detail lines.

Inspect one canonical activity record:

```sh
venmo activity info <ACTIVITY_ID>
```

Like or unlike a visible activity record:

```sh
venmo activity like <ACTIVITY_ID> [--dry-run | --yes]
venmo activity unlike <ACTIVITY_ID> [--dry-run | --yes]
```

Both commands perform an authoritative activity preflight. Preview the exact activity and action,
obtain approval, and execute once.

### Activity comments

List comments embedded in an activity record:

```sh
venmo activity comments list <ACTIVITY_ID> [--limit <N>] [--offset <N>]
```

The limit defaults to 10. The offset defaults to 0 and indexes the complete embedded comment
collection for that activity.

Add a comment:

```sh
venmo activity comments add <ACTIVITY_ID> <MESSAGE> [--dry-run | --yes]
```

The message must contain non-whitespace text and cannot exceed 2,000 characters. The command
preflights the parent activity.

Remove a comment:

```sh
venmo activity comments remove <COMMENT_ID> [--dry-run | --yes]
```

Venmo authorizes removal from the comment ID alone. The CLI cannot preflight the parent activity,
authorship, or comment text, and it cannot reconcile absence without an activity ID. Before
previewing removal, use `activity info` or `activity comments list` to establish and show the parent,
author, and text whenever possible.

## Requests

List pending requests involving the active account:

```sh
venmo requests list \
  [--direction all|incoming|outgoing] \
  [--limit <N>] \
  [--before <TOKEN>]
```

- Direction defaults to `all`.
- Limit defaults to 10 and controls the server page size before local direction filtering. A filtered
  page can therefore contain fewer rows than the requested limit.
- `--before` takes only the continuation token printed by an earlier `requests list` call.

Inspect one open request:

```sh
venmo requests info <REQUEST_ID>
```

Create a request:

```sh
venmo requests create <USERNAME> <AMOUNT> <NOTE> \
  [--visibility private|friends|public] \
  [--dry-run | --yes]
```

The recipient is an exact username with optional `@`. Visibility defaults to `private`. Request
creation can require an interactive SMS step-up. Preview the exact recipient, amount, note, and
visibility before obtaining approval.

Accept an incoming request and pay its requester:

```sh
venmo requests accept <REQUEST_ID> \
  [--source <SOURCE_ID>] \
  [--protect] \
  [--dry-run | --yes]
```

- Use only a canonical incoming request ID and inspect it with `requests info` first.
- Acceptance is an unprotected personal payment by default.
- Without `--source`, source selection follows the same balance-first and unique-external rules as
  `pay user`.
- `--source` uses that exact peer-eligible source without substitution.
- `--protect` turns on Purchase Protection. Venmo deducts its disclosed seller fee from the amount
  the recipient receives.
- Acceptance can require an interactive SMS step-up.

Decline an incoming request without sending money:

```sh
venmo requests decline <REQUEST_ID> [--dry-run | --yes]
```

Cancel an outgoing request without sending money:

```sh
venmo requests cancel <REQUEST_ID> [--dry-run | --yes]
```

Cancellation cannot reverse a completed payment. Always inspect the request and verify its direction
before previewing either action.

## Transfers

Show current standard bank-transfer eligibility, available balance, and destination information:

```sh
venmo transfer options
```

Transfer an exact amount or the available balance to the unique selected standard bank destination:

```sh
venmo transfer out <AMOUNT_OR_ALL> [--speed standard] [--dry-run | --yes]
```

- Use a positive USD amount or exact lowercase `all`.
- `--speed` defaults to `standard`, the only supported value.
- `all` resolves once from a fresh available-balance snapshot and excludes funds on hold. It is not
  an atomic account drain.
- Only standard bank cash-out to the unique eligible destination is supported.
- Instant transfer, debit destinations, manual destination selection, and transfer challenge
  continuation are not supported.

Always run `transfer options`, preview the resolved amount and destination, disclose any unknown fee
or limitation, obtain approval, and execute once.

## Confirmation and dry-run behavior

Every financial or social mutation displays action details and uses a final confirmation that
defaults to No.

- `--dry-run` completes validation and any available preflight, displays the action, and stops before
  the final confirmation, SMS step-up, or write.
- `--yes` skips only the final confirmation. Validation and available preflight still run.
- `--dry-run` and `--yes` are mutually exclusive.
- A noninteractive mutation without either flag fails because the CLI cannot present a safe
  confirmation.
- A successful dry-run does not reserve state or guarantee the later mutation. The real command
  validates again.

The agent workflow is always `--dry-run --json`, present the exact `data.plan`, obtain fresh approval,
and execute the unchanged command once with `--yes --json`.

## Pagination

List commands print a continuation when another page is available:

- `users search` and `friends list` use `--offset`.
- `activity list` uses `--before-id`.
- `activity comments list` uses `--offset` within the selected activity's comment collection.
- `requests list` uses `--before`.

Continuation types are endpoint-specific and not interchangeable. Use the exact value from the
immediately relevant result. Do not increment, decode, normalize, or reuse an opaque token with a
different command.

## Exit codes and recovery

| Code | Meaning | Agent response |
| --- | --- | --- |
| `0` | Command completed successfully | Report only what the command established. |
| `1` | Operational failure, cancellation, credential/authentication problem, network/timeout/API failure, contract mismatch, or internal failure | Explain the printed error. Do not automatically retry a mutation. |
| `2` | Invalid usage or required interactive prompting was unavailable | Correct syntax, obtain missing confirmation, or hand an interactive login/OTP command to the user as appropriate. |
| `3` | A financial or state-changing write may have happened, or succeeded but its result could not be rendered | Outcome is unknown. Never retry until independently reconciled. |

For JSON failures, use `error.outcome` to refine recovery: `not_performed`, `partial`, `unknown`, or
`completed`. Inspect `partial_result` and `context.plan` when present. `unknown` and `completed`
mutation outcomes must never be retried until independently reconciled.

For exit code 3:

1. Preserve and report the command's exact warning.
2. Do not state that the action succeeded or failed.
3. Do not run the mutation again.
4. Use the narrowest relevant read command: `activity list`, `requests list`, `requests info`,
   `friends list`, `users info`, `activity info`, or `activity comments list`.
5. Ask the user to verify in the official Venmo app.
6. Treat absence from one CLI read as inconclusive.

Mutations are never automatically retried, even for exit codes other than 3. A new attempt requires
a fresh preview and confirmation after the prior outcome is understood.

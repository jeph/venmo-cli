# JSON output contract

Pass the global `--json` flag to make a Venmo CLI command emit its final result as one compact JSON
object followed by one newline. The flag may appear at any command depth before an argument
terminator (`--`). Help and version output remain human-readable even when `--json` is present.

The current JSON contract is intentionally unversioned. A schema-version field can be introduced if
a version 2 contract is added in the future.

## Streams and exit status

- A successful command writes one JSON object to stdout and leaves stderr empty during clean,
  noninteractive operation.
- A failed command leaves stdout empty and writes one JSON error object to stderr.
- Existing exit codes are unchanged: `0` success, `1` operational failure, `2` usage failure, and
  `3` unknown or otherwise ambiguous mutation outcome.
- An actual interactive confirmation or OTP flow may write its prompt and safe preflight details to
  stderr before the final JSON object. Explicit `--debug` diagnostics may also share stderr. Do not
  combine `--debug` with a parser that expects stderr itself to contain only JSON.
- JSON `--dry-run`, JSON `--yes`, and noninteractive JSON failures suppress human preflight text.

## Envelopes

Success:

```json
{"command":"balance","ok":true,"data":{"balance":{"available":{"amount":"12.34","currency":"USD"},"on_hold":{"amount":"0.00","currency":"USD"}}}}
```

Failure:

```json
{"command":"balance","ok":false,"error":{"code":"authentication_required","category":"authentication","message":"authentication failed; run `venmo auth login` and try again","exit_code":1,"outcome":"not_performed","details":null},"context":null,"partial_result":null}
```

Argument parsing can fail before a command is known. Such failures use `"command": null`,
`error.code: "invalid_arguments"`, and a stable parser kind in `error.details.kind`.

`command` is one of these stable identities:

```text
auth.login                    auth.logout                 auth.status
pay.options                   pay.user
friends.list                  friends.add                 friends.remove
users.search                  users.info
balance
activity.list                 activity.info               activity.like
activity.unlike               activity.comments.list      activity.comments.add
activity.comments.remove
requests.list                 requests.create             requests.accept
requests.decline              requests.cancel             requests.info
transfer.options              transfer.out
```

## Common value rules

- Money is never a JSON floating-point number. It is an object containing an exact decimal string
  and currency: `{"amount":"12.34","currency":"USD"}`.
- Timestamps are RFC 3339 in UTC, such as `1970-01-01T00:00:00Z`. Human-mode local-time formatting
  does not affect JSON.
- IDs and continuation tokens are opaque strings. Do not parse, normalize, or interchange them.
- Closed states use lowercase `snake_case` strings.
- Collections are always arrays, including when empty. Unavailable optional values are JSON `null`.
- JSON strings retain their semantic text and use standard JSON escaping. Human-mode terminal
  sanitization and table truncation are not part of the JSON protocol.
- Unvalidated `transfer.options` fee metadata remains an exact decimal string (or `null`) rather
  than being interpreted as USD or a percentage. Its unit is not part of the validated API
  contract; do not treat those metadata strings as money.
- Credentials, bearer tokens, device IDs, passwords, OTPs, eligibility tokens, and fee tokens are
  never response fields.

## Read and authorization data

The `data` object is command-specific:

| Command | Top-level data fields |
| --- | --- |
| `auth.status` | `account`, `saved_at`, `credential_format`, `credential_backend` |
| `auth.login` | `credential_stored`, `account`, `saved_at`, `disposition`, `device_trust`, `previous_remote_token_revoked` |
| `auth.logout` | `local_credential`, `remote_token_revoked` |
| `balance` | `balance` (`available`, `on_hold`) |
| `pay.options` | `payment_methods` |
| `users.search` | `users`, `next_offset` |
| `users.info` | `user` |
| `friends.list` | `subject`, `users`, `next_offset` |
| `activity.list` | `subject`, `activities`, `next_before_id` |
| `activity.info` | `activity` |
| `activity.comments.list` | `activity_id`, `comments`, `total_count`, `offset`, `next_offset` |
| `requests.list` | `direction`, `requests`, `next_before` |
| `requests.info` | `request` |
| `transfer.options` | `preferred_out`, `standard`, `instant` |

User objects consistently expose `user_id`, nullable `username` and `display_name`, nullable
`profile_kind`, nullable `is_payable`, and nullable `friendship_status`. Activity and request records
preserve authoritative IDs, parties, status, visibility/audience, amounts, notes, and timestamps when
available.

## Mutation data

Every confirmed mutation uses the same data shape:

```json
{
  "outcome": "dry_run",
  "performed": false,
  "plan": {},
  "result": null
}
```

or:

```json
{
  "outcome": "completed",
  "performed": true,
  "plan": {},
  "result": {}
}
```

The safe, resolved `plan` includes the material action details shown for confirmation: account,
target, exact money, note/message, visibility, source, protection/fee information, current state, or
transfer destination as applicable. It also states `automatic_retries: false`. It never includes the
credential envelope or write-only API tokens.

This mutation shape applies to `pay.user`, request creation/acceptance/decline/cancellation,
`transfer.out`, friendship add/remove, activity like/unlike/comment creation, and activity comment
removal.

## Failures and uncertain outcomes

`error.category` is one of:

```text
usage  cancelled  credential  authentication  network  timeout  api
api_contract  ambiguous_write  internal
```

`error.code` uses this smaller semantic vocabulary:

```text
invalid_arguments                usage_error
cancelled                        credential_error
authentication_required          network_error
timeout                          api_rejected
api_contract_violation           internal_error
write_outcome_unknown            result_output_failed
device_trust_incomplete          logout_incomplete
credential_cleanup_incomplete    credential_state_unknown
```

`error.outcome` describes mutation/state effects independently of the exit code:

- `not_performed`: no state change is known to have occurred.
- `partial`: a reported subset completed; inspect `partial_result`.
- `unknown`: a write may have occurred. Never retry automatically; reconcile independently.
- `completed`: the operation is known to have completed, but reporting its result failed.

After a mutation reached a successful preflight, failures include the same safe resolved plan in
`context.plan`. Authentication operations that complete only part of their local state transition
use `partial_result`. Both fields are otherwise `null`.

Exit code `3`, `outcome: "unknown"`, and `outcome: "completed"` all require special care. Do not
blindly rerun the mutation. Check the relevant read command and the official Venmo app first.

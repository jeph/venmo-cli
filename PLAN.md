# venmo-cli Rust Product Plan

**Status:** Active; root Rust-only repository cutover completed; local read-only MCP server planned
**Last updated:** 2026-07-20
**Package:** `venmo-cli`
**Binaries:** `venmo`; planned `venmo-mcp`

> **Contributor safety:** Command forms and controlled-live protocols in this plan record product
> contracts and owner-only validation protocols; they are not routine verification instructions. Routine
> contributors must not execute ignored/manual tests, live probes, or real financial commands. Use
> only the service-free checks in [`CONTRIBUTING.md`](CONTRIBUTING.md).

## 1. Product direction

The product is one repository-root Rust package with a shared `venmo_cli` library and two thin native entry points: the terminal CLI `venmo` and the planned local stdio MCP server `venmo-mcp`. Both adapters must call the same feature use cases, feature/shared models, credential implementation, and Venmo API client; only protocol parsing, presentation, and process composition differ. The completed language and repository cutover was also an intentional redesign of the command interface; the former TypeScript command surface remains only historical behavioral evidence, not a compatibility requirement or shipped implementation.

The first release should make the common workflow obvious:

1. Save a bearer token securely.
2. Verify which Venmo account is active.
3. Find a friend or another user.
4. Inspect the Venmo balance and available payment methods.
5. Pay one person, request money from one person, accept one incoming request, decline one incoming request, or transfer balance to the selected standard bank.
6. Inspect activity, pending requests, and current transfer options.
7. Diagnose authentication or private-API failures.

This remains an unofficial client for unsupported/private Venmo endpoints. Endpoint discovery and contract testing are release gates, not assumptions.

The initial MCP release provides structured, read-only access to the already implemented public read operations. It is not a wrapper around terminal commands or table output, and “roughly the same API” does not mean exposing terminal-only credential management, package commands, internal helper functions, or unverified financial mutations. Users authenticate with `venmo`, then configure a trusted local MCP host to launch `venmo-mcp`.

## 2. Confirmed product decisions

- Use Rust with a pinned stable toolchain and a committed `Cargo.lock`.
- Use Rust 1.95.0 for the pinned toolchain and MSRV, package version `0.3.0`, and the MIT license.
- Use [`clap`](https://docs.rs/clap/) with its derive API.
- Use [`dialoguer`](https://docs.rs/dialoguer/) behind a narrow prompt adapter for hidden token input and default-No confirmation.
- Prefer mature, popular, actively community-maintained off-the-shelf crates for common functionality instead of rolling project-owned replacements; keep custom code focused on Venmo-specific model behavior, feature orchestration, and safety policy.
- Use a hybrid command hierarchy: frequent actions are top-level; related management operations are grouped.
- Use [`keyring`](https://docs.rs/keyring/) as the only persistent credential backend.
- `venmo auth login` always uses the legacy mobile-private account-identifier/password, newly supplied trusted-device-ID, and optional SMS-OTP flow. There is no token-import, stored-device reauthentication, password storage, automatic renewal, or authentication retry surface.
- Typed login credentials and OTP material are zeroized on drop and never stored. Serialization, HTTP, and platform libraries may create bounded transient request/header copies that Rust cannot guarantee are overwritten; the CLI never logs or persists those copies and drops them promptly. Only a validated bearer token, persistent device ID, account identity, and local saved-at time enter the OS credential store.
- Target Venmo's bearer-token/device-ID mobile private `/v1` API rather than emulating the browser session API. Web-app traffic is discovery evidence only; do not adopt cookie/CSRF authentication, browser backend routes, or a browser User-Agent merely because the web client exposes an operation.
- A successful explicit login may replace any readable existing credential, including another account, only after the new token validates and the replacement reads back exactly. A failed login leaves the existing entry untouched.
- Do not ship browser automation, browser-cookie import, or web-session authentication as CLI features. Development-time browser observation under Section 7.4 is permitted solely to establish sanitized API contracts.
- Document the unsupported password/SMS-OTP login risk and local-only logout semantics in the README.
- Support one recipient per new `pay user` or `requests create` invocation and one request ID per `requests accept`, `requests decline`, or `requests cancel` invocation.
- Ship `pay user`, request creation, request acceptance, request decline, and outgoing request cancellation as independently validated operations. `pay user` and request creation passed their dated contract, synthetic-test, controlled-live-validation, and reconciliation requirements on 2026-07-12. Balance-covered acceptance and decline passed their operation-specific synthetic and reconciled live validation on 2026-07-14. Unprotected external-funded acceptance is signer-verified, synthetically pinned, and owner-validated under client 1; protected approval, continuation, actual-source, and final-fee evidence gaps remain. Outgoing cancellation is current-native and publicly corroborated with exact synthetic coverage, but current client-1 live authorization remains unproven.
- Keep terminal-capability policy inside private CLI production composition. The public facade exposes no terminal-capability value, and neither normal dispatch nor runtime-initialization fallback accepts caller-supplied terminal state. Prompt capability comes from the actual process; crate-private unit tests may inject synthetic terminal snapshots only while delegating to fake handlers.
- Initialize global `--debug` CLI logging only after service-free dispatch preconditions. Noninteractive login does not install the global subscriber. Every current and future delegated command passes through the same command-lifecycle logging boundary; `requests accept`, `requests decline`, and `requests cancel` require no command-specific setup. Runtime construction remains the one unavoidable asynchronous bootstrap, and runtime-failure logout preserves local deletion semantics.
- Normally limit controlled live mutations to $0.01 and reconcile each mutation before authorizing another. On 2026-07-14 the owner made a narrow exception for one existing legitimate $25 incoming request that the owner already owes: one `approve` attempt, no retries or field variation, full authoritative preflight and default-No confirmation, followed by immediate CLI and official-app reconciliation. This exception does not prove the final funding source or fee and does not authorize any other elevated-value test.
- Group request reads and writes under `venmo requests`: `list`, `create`, `accept`, `decline`, `cancel`, and `info`. Remove the former top-level `request`, `accept`, and `decline` forms without compatibility aliases.
- Call transaction history `activity`.
- Let `pay user` and `requests create` select `private`, `friends`, or `public` visibility with an explicit typed `--visibility` option. Default to `private`. Venmo may apply the more restrictive setting selected between payment partners, so require a creation response to contain a supported audience no more public than requested; missing, unknown, or more-public response audiences remain ambiguous. Label output as the requested audience rather than claiming it is effective. Do not add the option to request acceptance, decline, or cancellation.
- Limit payment release to personal-profile peer-to-peer operations with a valid eligibility token. Ordinary payment keeps its live-validated blank-source eligibility path. Explicit `pay user --protect` instead requires current source-bound eligibility and exactly one complete buyer-protection fee, submits the protected transaction type and fee without fallback, and requires response proof that Venmo tagged the payment. Allow a peer-eligible Venmo balance, bank, or card source regardless of method-level fee; display protected seller-fee and recipient-proceeds estimates without increasing the payer total or guaranteeing coverage. Acceptance defaults to an unprotected personal payment: it retains its live-validated legacy route only for automatic balance-covered acceptance; any shortfall, explicit `--source`, or `--protect` uses the modern route. Unprotected modern acceptance omits returned fee records. Explicit acceptance `--protect` strictly validates and submits bounded fee records. No response proves the actual source or final fee beyond its eligibility evidence.
- Select the submitted peer source deterministically. Without `--source`, use the peer-eligible Venmo balance source when authoritative available balance covers the full amount; otherwise validate external duplicate IDs and defaults, choose the unique external default regardless of fee, choose only a sole eligible external method when no default exists, and fail closed on external ambiguity. `--source` must exactly match a peer-eligible balance, bank, or card ID from `pay options`; explicit balance must cover the full amount, and unknown or ineligible IDs fail before confirmation. Explicit selection never silently substitutes another source and does not require an automatic choice among other external methods. Never choose by response order or fee. Success output may name an actual source only when authoritative evidence proves it.
- Require default-No confirmation for `pay user`, `requests create`, `requests accept`, `requests decline`, and `requests cancel`.
- Expose `--yes` only on `pay user` and all four request mutation commands, and require it unless both stdin and stderr are interactive terminals. A redirected stderr must never hide a mutation confirmation prompt. The flag skips only confirmation, never authoritative validation or the details display.
- Do not expose any `--dry-run` flags.
- Never automatically retry payment creation, request creation, request acceptance, request decline, or request cancellation. The exact payment code-1396 SMS challenge may make one verified continuation submission with the same immutable plan and UUID; this explicit challenge continuation is not an automatic retry.
- Write no first-party `unsafe` Rust and prohibit it mechanically across every first-party target.
- Use no `unwrap`, `unwrap_err`, `expect`, `expect_err`, or equivalent panic-based value extraction in first-party Rust, including tests and build tooling.
- If the correct treatment of any failure is unclear, stop the affected implementation and ask the prompter/project owner rather than guessing, swallowing it, panicking, or choosing an arbitrary fallback.
- Keep one Cargo package and shared library, add `venmo-mcp` as a second thin binary, and set Cargo `default-run = "venmo"` so existing `cargo run -- ...` behavior remains unambiguous.
- Use local stdio as the only initial MCP transport. Do not enable Streamable HTTP, SSE, remote authentication, or network listening in the first MCP release.
- Register only the eight read tools in Section 3.9 initially. Do not expose resources or prompts until a concrete use case requires them.
- Keep the entire credential lifecycle terminal-CLI-only. MCP may report authenticated status but must never offer login, password/OTP collection, logout, credential overrides, remote session mutation, or secret-bearing tool inputs.
- Give MCP its own explicit input/output DTO and JSON Schema boundary. Do not serialize secret-bearing feature/shared types, stored credential envelopes, raw private-API DTOs, headers, bodies, or keychain errors.
- Reload the native credential for every MCP tool invocation and drop it afterward. Never cache a bearer token or device ID in long-lived server state; MCP startup and `tools/list` must touch neither the keychain nor Venmo.
- Serialize MCP Venmo operations through one asynchronous operation permit, preserve zero retries and one-page endpoint-native pagination, and never add hidden fetch-all behavior.
- Annotate every MCP tool accurately with the standard read-only/destructive/idempotent/open-world hints, while treating annotations as advisory metadata rather than authorization enforcement.
- Keep all MCP financial tools absent by default. A later phase may register verified financial tools only after explicit non-secret server startup opt-in and a trustworthy host human-approval mechanism; neither annotations nor startup opt-in alone is sufficient authorization.

### 2.1 Implementation clarification rule

If implementation or validation reveals anything contradictory, materially ambiguous, or incompatible with this plan, stop the affected work and seek clarification from the **prompter/project owner directing the implementation** before choosing a behavior. This means the person interacting with the coding agent during development—not an end user of the finished CLI. This includes conflicts between the command contract, API evidence, financial-safety rules, authentication or credential behavior, tests, documentation, platform behavior, and release requirements.

This is exclusively a development-time workflow rule. Do not add a runtime CLI prompt, feedback mechanism, or network call that asks CLI users to resolve implementation contradictions.

Do not silently guess, weaken a safety rule, add a workaround, alter the public interface, or expand or reduce scope to resolve a contradiction. When asking for clarification:

1. State the conflicting requirements or observed behavior precisely.
2. Explain which commands, safety guarantees, tests, or release targets are affected.
3. Present the viable options and their tradeoffs.
4. Recommend one option when evidence supports it.
5. Continue only independent work that cannot prejudice the decision.

After the prompter/project owner decides, record the resolution in this plan and encode it in the relevant tests before resuming the affected implementation.

### 2.2 Rust code and failure-handling rules

The no-`unsafe` rule applies to all first-party library, binary, test, example, benchmark, build-script, and code-generation targets. Configure workspace/package lints with `unsafe_code = "forbid"` and retain a crate-level `#![forbid(unsafe_code)]` defense where applicable. There is no project-owned exception process. Third-party crates may contain reviewed internal `unsafe`; evaluate that risk through the dependency policy in Section 9 rather than pretending it can be prohibited transitively.

Configure Clippy to deny `clippy::unwrap_used` and `clippy::expect_used` for every first-party target. Do not use `.unwrap()`, `.unwrap_err()`, `.expect()`, `.expect_err()`, or a custom helper/macro that merely hides the same panic-based extraction. Prefer `Result`, `?`, `Option`/`Result` combinators, `let ... else`, exhaustive matching, typed errors, and explicit assertions or pattern matching in tests.

When a new failure does not have a clearly specified treatment, pause only the affected work and ask the prompter/project owner. The clarification must state:

1. Where the failure originates and what evidence detects it.
2. Whether any credential mutation, network transmission, financial write, partial output, or other externally visible side effect may already have occurred.
3. The plausible classifications, exit codes, user messages, retry rules, and recovery actions.
4. The effect of each option on existing safety guarantees and tests.
5. A recommended handling when the evidence supports one.

Do not silently discard the error, convert it to an empty/default value, log and continue, retry it, classify an uncertain write as success or failure, or panic merely to avoid making this decision. After clarification, add the typed error mapping and tests before resuming that path.

## 3. CLI and MCP public surfaces

```text
venmo auth login
venmo auth logout
venmo auth status

venmo pay user <USERNAME> <AMOUNT> <NOTE> [--source <SOURCE_ID>] [--protect] [--visibility <VISIBILITY>] [--yes]
venmo requests create <USERNAME> <AMOUNT> <NOTE> [--visibility <VISIBILITY>] [--yes]
venmo requests accept <REQUEST_ID> [--source <SOURCE_ID>] [--protect] [--yes]
venmo requests decline <REQUEST_ID> [--yes]
venmo requests cancel <REQUEST_ID> [--yes]

venmo friends list [--limit <N>] [--offset <N>]
venmo friends add <USERNAME> [--yes]
venmo friends remove <USERNAME> [--yes]
venmo users search <QUERY> [--limit <N>] [--offset <N>]
venmo users info <USERNAME>
venmo pay options
venmo balance

venmo activity list [--user <USERNAME>] [--limit <N>] [--before-id <TOKEN>]
venmo activity info <ACTIVITY_ID>
venmo requests list [--direction <DIRECTION>] [--limit <N>] [--before <TOKEN>]
venmo requests info <REQUEST_ID>
```

No compatibility aliases should be added initially. Add aliases only in response to demonstrated user need.

All request operations share the plural `requests` group. After the explicit `create` operation, `venmo requests create accept ...` is an unambiguous request to the user named `accept`; bare and optional-`@` usernames remain equivalent. Freeze the grouped forms and rejection of the removed top-level forms in parser, process, and help snapshots rather than adding compatibility aliases.

### 3.1 Global behavior

```text
venmo [--debug] <COMMAND>
```

- `--debug` writes bounded diagnostics to stderr and is global at every command depth. It is
  disabled by default. Removed `-v` and `--verbose` forms are rejected rather than aliased.
- Data and successful results go to stdout.
- Prompts, warnings, diagnostics, and errors go to stderr.
- `--help` and `--version` must not touch the keychain or network.
- Human output uses color only on a terminal and honors `NO_COLOR`.
- Machine-readable **CLI** output is deferred, but command handlers must return typed results so JSON can be added without rewriting feature logic. MCP structured results are a separate protocol requirement and do not settle the future `venmo --json` product decision.

Proposed stable exit codes:

| Code | Meaning |
| ---: | --- |
| `0` | Success |
| `1` | Operational, credential, validation, API, or user-cancellation failure |
| `2` | CLI usage/precondition error, including argument parsing or a required noninteractive `--yes` |
| `3` | Financial or relationship write outcome is unknown and must be verified |
| `130` | Interrupted before a write became ambiguous |

### 3.2 Authentication

#### `venmo auth login`

- Requires an interactive terminal.
- Prompts for the Venmo email/phone/username, then a hidden password, then a newly supplied trusted browser `v_id`/device ID. Hidden inputs use terminal echo suppression rather than rendering masking characters. It never reuses a stored device ID because that device may have been invalidated.
- Handles the verified SMS-OTP challenge by requesting the text message and prompting for a hidden six-digit code. It never prints or stores the account identifier, password, OTP, or OTP secret.
- Exposes no username, password, OTP, token, device-ID, stdin, environment-variable, or browser-cookie argument.
- Login may run with a missing, readable, or targetably unusable entry. A successful explicit login may switch accounts, but it does not mutate storage until the new token validates.
- Preserves the newly supplied trusted device ID across the password/OTP sequence and any post-OTP trust request, and stores it only after an issued token validates. It never generates, reuses, or substitutes a device ID.
- Validates every issued token with the current-account endpoint before storage.
- Never automatically retries token issuance. If timeout, disconnect, redirect, cancellation, or a malformed successful response occurs after issuance may have begun, return exit `1`, state that authentication outcome is unknown and a remote token may have been issued, and direct the user to official Venmo session controls.
- If password/OTP authentication returns a token but current-account validation fails, do not store, trust, automatically revoke, or retry that token. Return exit `1` and warn that the remote token may remain active until the user reviews official Venmo session controls.
- Direct password success proves that Venmo accepted the supplied existing trusted device, so it performs no redundant `/users/devices` mutation and reports complete success. After OTP login, it attempts to trust the supplied device. If token issuance and account validation succeed but that post-OTP trust request fails, save and verify the credential, return exit `1`, and warn that future login may require OTP; never discard an issued token silently.
- After validation, can replace any single targetable unusable entry, including corrupt, invalid, oversized, legacy, or unknown-schema data. Unavailable stores, ambiguous/multiple matches, and untargetable platform failures remain blocking errors.
- Any failure before storage leaves the previous readable credential untouched. Successful cross-account replacement is allowed after validation. Because replacement does not revoke the previous remote bearer, output warns the user to use official Venmo session controls if invalidation is required.
- Reads the saved entry back and requires an exact versioned-envelope match before reporting success. A save error, missing read-back, read-back error, or mismatch returns exit `1`, reports credential storage state as unknown, performs no automatic retry or rollback, and directs the user to `auth status`.
- Prints only the authenticated username and safe trust-device status; never echoes credential, token, OTP, OTP-secret, or device-ID material.

#### `venmo auth logout`

- Deletes the local keychain entry without requiring it to deserialize first.
- Never contacts Venmo or attempts remote token revocation. Successful local deletion warns that the remote bearer may remain valid and points to official session controls.
- Reports local deletion failure truthfully and returns exit code `1`.
- Is idempotent when no local credential exists.

#### `venmo auth status`

- Verifies that the keychain entry can be read and parsed.
- Calls the current-account endpoint to validate the token.
- Displays the active username, display name, user ID, and credential saved-at instant converted to the system time zone; never presents that local timestamp as the token's server-issued creation time.
- Distinguishes missing, corrupt, locked, unavailable, and rejected credentials.
- A definitive HTTP 401 from this or another authenticated request is classified as an authentication failure, preserves the credential, and directs the user to run `venmo auth login`. It is never retried automatically.

### 3.3 Paying one person

```text
venmo pay user @alice 12.50 "Dinner"
venmo pay user alice 12.50 "Dinner" --visibility friends --yes
venmo pay user alice 12.50 "Dinner" --source bank-123 --yes
venmo pay user alice 12.50 "Purchase" --protect
```

Contract:

- Exactly one recipient.
- Recipient is an exact username with an optional leading `@`; user IDs are never accepted as a
  separate input type.
- Amount is a positive decimal with at most two fractional digits.
- `<NOTE>` is a required positional argument and must contain non-whitespace text.
- `--visibility` accepts exactly `private`, `friends`, or `public` and defaults to `private`.
- `--source` accepts one exact peer-eligible balance, bank, or card ID listed by `venmo pay options`; `--from` is rejected.
- `--protect` explicitly requests Purchase Protection. It is off by default, never silently falls back to ordinary payment, and does not guarantee that an item qualifies for coverage.
- `--yes` skips only confirmation; it does not skip validation or the payment-details display.

Funding-source policy:

1. Without `--source`, use the peer-eligible Venmo balance source when authoritative available balance covers the full amount.
2. If automatic selection needs an external source, validate external duplicate IDs and multiple defaults, choose the unique default regardless of method-level fee or response position, or choose a sole external candidate when no default exists.
3. Otherwise fail closed with a Usage-class ambiguity error.
4. With `--source`, require an exact peer-eligible balance, bank, or card ID. Explicit balance must cover the full amount; unavailable or ineligible IDs fail before confirmation. Submit the requested source exactly without substitution.

Selection operates only on methods that the verified mobile contract identifies as eligible for peer payment. Writer-side selection preserves and interprets only the exact peer-payment role and exact `balance`, `bank`, or `card` type; merchant/default-transfer roles, unknown types, and substring matches cannot establish an eligible source. Method-level fee evidence does not filter or rank peer-eligible external methods: proven-zero, known-nonzero, and unknown-fee methods may be submitted. Ordinary payment uses the transaction-specific blank-source eligibility call for its token; its fee is not used as a rejection gate or tiebreaker and is not presented as a payer fee or final total. Protected payment instead binds eligibility to the selected source, requires exactly one valid `venmo:product:buyer_protection:` fee, rejects a fee above the payment amount, and carries that complete fee into the immutable plan. Duplicate IDs and unknown peer roles are contract failures. Multiple external defaults are a contract failure only when automatic external selection is required; multiple non-default external methods are then an ambiguity Usage error. Explicit external selection and automatic balance use do not need to resolve unrelated external-default ambiguity.

Before confirmation, show:

- Resolved recipient username, display name, and user ID.
- Exact amount.
- Note.
- Requested audience.
- Selected external-method fee evidence (`$0.00`, a known nonzero amount, or `unknown`) when applicable.
- Current Venmo wallet balance when supplied by the verified preflight contract.
- Exact selected balance, bank, or card funding source, including a safe label and ID. Keep the automatic/explicit selection mode in the immutable plan without displaying that internal policy marker.
- Whether Purchase Protection was requested; when requested, the eligibility-reported seller-fee estimate and checked recipient-proceeds estimate. Never add the seller fee to the payer amount.

Confirmation defaults to **No**. Non-interactive execution without `--yes` is rejected.

If the initial payment submission receives the exact HTTP 400/403 root code `1396`, treat it as
Venmo's confirmed prewrite SMS challenge rather than an ambiguous write. Continue only on an
interactive terminal: issue one P2P SMS tied to the payment UUID, read one hidden six-digit OTP,
verify it, and submit the same immutable payment and UUID exactly once with
`verification_method: ["sms_otp"]` and `verification_status: "sms_otp_verified"` metadata. Never
loop, accept OTP through an argument, print/log/store the code, or alter the plan. `--yes` skips
only payment confirmation and cannot bypass step-up. A repeated challenge or any failed/unknown OTP
result stops without another payment submission. Full service-free contract coverage is required.
For a protected payment, the continuation must preserve transaction type, singleton fee, source,
eligibility token, and quasi-cash metadata while merging the two verification fields.
One owner-run `$1.00` explicit-bank-source payment completed this exact flow and independently
reconciled as exactly one settled outgoing activity; this proves the continuation, not the actual
debit source or final fee. Exact HTTP 403/root code `1360` is a confirmed 10-minute same-info
duplicate rejection, while exact HTTP 403/root code `10100` means Venmo's server-side checks
blocked the payment. The message must tell the operator to try later or use the official app; it
must not claim which internal check fired.

### 3.4 Creating, accepting, and declining requests

#### Creating a request for one person

```text
venmo requests create @alice 12.50 "Dinner"
venmo requests create @alice 12.50 "Dinner" --visibility public --yes
```

The command shares recipient, amount, note, and preflight validation with `pay user`, but:

- Creates a request rather than a payment.
- Accepts the same typed `--visibility private|friends|public` selection as `pay user` and defaults to `private`.
- Never accepts or sends a funding-source ID.
- Displays and flushes the exact requesting account, recipient, amount, note, requested audience,
  and create action before default-No confirmation.
- Requires `--yes` when stdin or stderr is not interactive; the flag skips only confirmation.
- Rejects payment-only options and `--dry-run` instead of ignoring them.
- Still performs at most one write and is never retried automatically.

#### `venmo requests accept <REQUEST_ID> [--source <SOURCE_ID>] [--protect]`

```text
venmo requests accept 123456789
venmo requests accept 123456789 --source bank-123
venmo requests accept 123456789 --protect
venmo requests accept 123456789 --yes
```

This is a financial write that pays exactly one existing incoming request. Available balance and optional source selection produce one of two immutable funding plans:

- `REQUEST_ID` is the canonical request identifier printed by `requests list`; its validated syntax must follow the verified API contract.
- Fetches the server-side request record before prompting.
- Requires the request to be pending, incoming, addressed to the authenticated account, and explicitly payable through the verified acceptance contract.
- Rejects outgoing, completed, declined, cancelled, inaccessible, malformed, or wrong-account records before any write.
- Takes no recipient, amount, or note arguments. The requester, amount, and note come only from the fetched request record and cannot be changed by this command.
- Acceptance is unprotected by default. Full nonnegative balance coverage retains the live-validated action-only approval route only when `--source` and `--protect` are both absent.
- Any negative, zero, or partial-balance shortfall, explicit `--source`, or `--protect` first resolves exactly one unacknowledged request notification whose payment ID equals `REQUEST_ID`, then loads peer-eligible balance and external methods and applies the shared funding-source policy. A legacy direct notification uses root `id`/`payment.id`; the current wrapper uses `additional_properties.request.id`/`additional_properties.request.payment.id`. The modern write uses the associated request-action ID—not the current wrapper's outer notification ID—and the selected source exactly. Missing, duplicate, conflicting, or malformed notification matches, unavailable explicit sources, and insufficient explicit balance fail before confirmation. Returned fee records are omitted from an unprotected approval.
- `--source` accepts one exact peer-eligible balance, bank, or card ID listed by `venmo pay options`, forces the modern route, and never silently substitutes another source.
- `--protect` explicitly turns on Purchase Protection, forces the modern route even when balance covers, and submits strictly validated fee records. The checked seller fee must not exceed the request amount.
- Requires a verified private audience and a personal, payable requester in the first release.
- `--yes` skips only confirmation; it does not skip account, lookup, request-state, identity, audience, or balance validation.

Before confirmation, show:

- Request ID and current pending status.
- Requester username and user ID, plus the display name when supplied by the authoritative user record.
- Exact requested amount and sanitized note.
- Request creation time when supplied by the verified contract.
- Current available wallet balance and the selected balance or external funding plan, without displaying the internal automatic/explicit policy marker.
- The sanitized selected source and, for an external source, its method-level fee evidence when the modern branch is used.
- For `--protect`, the estimated seller fee deducted from the recipient and estimated recipient proceeds. The fee is never added to the payer's amount.

Confirmation defaults to **No**. Non-interactive execution without `--yes` is rejected. The acceptance operation must use a verified request-acceptance contract; it must not silently substitute an unrelated ordinary payment to the requester. A stale-state rejection is a confirmed failure, while a timeout or disconnect after possible transmission is an ambiguous financial outcome with exit code `3`.

On confirmed success, print the accepted request ID, resulting activity/payment ID when supplied, requester, amount, and server-reported status. Do not claim that the request settled unless the verified response or a follow-up read proves that state.

The reconciled balance-covered contract is `PUT /v1/payments/{id}` with exact JSON
`action: "approve"`. Signer-verified Android 10.31.1 and 26.13.0 establish the external branch:
`GET /v1/notifications?acknowledged=false`, exact payment-ID association to one request-action ID,
source-bound form eligibility, then `PUT /v1/requests/{request-action-id}` with source/token
options. A bounded client-1 read confirmed the current three-ID wrapper structure. An outer-wrapper
ID attempt returned HTTP 404 and reconciled as no mutation; the corrected nested request-action ID
returned 200 and reconciled as exactly one settled $20 activity with the pending request removed.
The actual debit source/final fee, protected branch, and webview or SMS-step-up continuation remain
unproven; continuations are ambiguous and unsupported.

#### `venmo requests decline <REQUEST_ID>`

```text
venmo requests decline 123456789
venmo requests decline 123456789 --yes
```

This state-changing write refuses exactly one existing incoming request without sending money:

- Fetches and validates the authoritative request by ID and requires exact incoming `pending` state addressed to the active account.
- Takes no requester, amount, note, or funding arguments.
- Displays and flushes authoritative request-decline details, then asks for default-No confirmation. Non-interactive execution requires `--yes`.
- `--yes` skips only terminal detection and the final confirmation; it never skips account, lookup, request-state, or preflight validation.
- Sends exactly one `PUT /v1/payments/{id}` JSON update with `action: "deny"`, never outgoing-request `cancel`.
- Requires a successful response to preserve the exact request ID, amount, parties, note, and audience and to prove the supported terminal `cancelled` server status. Any mismatch, empty response, unverified error, or transport uncertainty is exit `3` and requires reconciliation without retry.
- Prints the authoritative server status and explicitly states that no money was sent; it does not invent a `declined` status value.

Confirmation defaults to **No**. No, cancellation, prompt failure, or non-interactive execution without `--yes` sends a write. The `deny` action is strongly documented by archived official Venmo material for a received request, while `cancel` applies to a request made by the authenticated user. Reconciled live validation established terminal remote refusal: the exact request reached server status `cancelled`, disappeared from pending results, created no activity, and changed no balance.

#### `venmo requests cancel <REQUEST_ID>`

```text
venmo requests cancel 123456789
venmo requests cancel 123456789 --yes
```

This state-changing write cancels exactly one outgoing money request without sending money:

- Fetches the authoritative request by ID and requires `action: charge`, self as actor, the
  counterparty as target, and exact `pending` or `held` state.
- Rejects incoming requests, completed payments, cancelled/expired requests, wrong-account
  records, incomplete immutable fields, and ID mismatches before output or any write.
- Takes no counterparty, amount, note, audience, or funding arguments. All displayed and validated
  values come from the authoritative record.
- Displays and flushes the exact outgoing-request details, then asks default-No confirmation.
  Non-interactive execution requires `--yes`; the flag skips only confirmation.
- Sends exactly one form-encoded `PUT /v1/payments/{id}` with `action=cancel`, no funding fields,
  and no automatic retry.
- Requires the response to preserve the exact ID, charge action, self→counterparty parties, amount,
  note, audience, and creation instant and prove terminal status exactly `cancelled`.
- Reports that no money was sent. It does not reverse completed payments, decline incoming
  requests, cancel friend requests, or remind the counterparty.

Any malformed or mismatched success, unverified error, interruption, or transport uncertainty is
exit `3`; reconcile through `requests list` and the official app before any retry. Current Venmo
Android 26.13.0 establishes the form route/action and supports both pending and held outgoing
charges. Historical `deet/govenmo` independently uses the same form contract, while the maintained
Python client uses the same endpoint/action with JSON. Exact synthetic coverage is retained; a
client-1 live cancellation has not yet been performed.

### 3.5 Friends and user discovery

#### `venmo friends list [--limit <N>] [--offset <N>]`

- Lists friends of the active account.
- Displays a copyable recipient value such as `@username`, display name, and user ID.
- Fetches exactly one API page using the supplied server page size and zero-based offset; defaults are 10 and 0, and the page-size maximum is 50.
- If the trusted next link has another page, writes only its validated copyable `Next offset: <N>` value to stderr after record output succeeds.
- Treats the invocation as one page rather than claiming that it is the complete friend list.

#### `venmo friends add <USERNAME> [--yes]`

- Uses shared bounded exact-username search followed by authoritative detail-by-ID; rejects self,
  non-personal profiles, and missing or unknown `friend_status` evidence.
- Follows current native semantics: exact `not_friend` sends a request and exact
  `request_received_by_you` accepts it. Exact `friend` and `request_sent_by_you` fail before any
  write.
- Flushes exact action details and uses an action-specific default-No confirmation. `--yes` skips
  only confirmation.
- Sends exactly one form-urlencoded `POST /v1/friend-requests` with `user_id=<target ID>`, never
  retries, and requires both the signer-evidenced response target and authoritative user-detail
  reconciliation to the expected final status.

#### `venmo friends remove <USERNAME> [--yes]`

- Exact `friend` selects unfriend; exact `request_sent_by_you` selects outgoing-request
  cancellation. `not_friend` fails, and an incoming request is rejected rather than silently
  declined through the separate native endpoint.
- Uses the same details, default-No/`--yes`, interrupt-protection, one-write, and no-retry policy.
- Sends exactly one bodyless `DELETE /v1/users/{self}/friends/{target}` and requires an
  authoritative detail read proving exact `not_friend` before reporting success.

For either command, post-transmission transport uncertainty, malformed or mismatched success,
failed reconciliation, interruption, or result-output failure is exit `3`. Signer-verified Android
9.26.0, 10.31.1, and 26.13.0 consistently establish the routes and state matrix, but those apps use
OAuth client 4 while the CLI uses client 1. Synthetic implementation does not claim live client-1
authorization; no controlled friendship canary has run.

#### `venmo users search <QUERY> [--limit <N>] [--offset <N>]`

- Searches for users who may not be friends.
- Normalizes a single token as a username search, with or without a leading `@`; `alice` and
  `@alice` therefore send the same `query=alice&type=username` request. Multi-word input remains a
  general fuzzy search.
- Returns the same copyable identity fields as `friends list`.
- Uses an exact username match when resolving a payment recipient; search ordering alone never selects a recipient.
- Fetches exactly one API page and treats `--limit` as its server page size, with default 10 and maximum 50. `--offset` is a typed nonnegative `u32` with default 0.
- When a full page is returned, provisionally reports the checked synthesized `Next offset: <N>` value; an empty page at that offset can be required to prove exhaustion.

#### `venmo users info <USERNAME>`

- Fetches one user through the canonical user-detail endpoint and displays every understood safe
  identity and profile field.
- Accepts only an exact username with an optional leading `@`; `users info alice` and
  `users info @alice` are equivalent. Digit-only input is still treated as a username, never as a
  user ID.
- Uses the shared bounded exact-user resolver: case-insensitive exact search, ambiguity rejection,
  then authoritative detail-by-ID verification of both immutable ID and username.
- Shares this resolution implementation and input grammar with `pay user` and `request`.
- Displays the exact supported friendship state when the authoritative response provides one, so
  relationship state can be inspected without invoking a mutation.

All command-level username resolution is shared and fail-closed around the live-verified mobile behavior:

- Commands accept usernames only, normalize an optional leading `@`, and never expose a user-ID
  argument.
- Resolution uses bounded `type=username` search and compares returned usernames
  case-insensitively. Search ordering alone never authorizes a user. One exact match is followed by
  detail-by-ID, which must return the same immutable ID and exact username before the command can
  continue; multiple exact IDs remain ambiguous.
- No exact match within the bounded traversal is reported as username not found.
- Username input is locally bounded to 1,023 bytes so the leading `@` plus username fits the 1,024-byte search-query bound.

### 3.6 Payment and transfer options, and balance

#### `venmo pay options`

- Lists method ID, safe display name, type, masked last four digits when available, and default status.
- Provides inspectable method metadata, including peer-eligible IDs accepted by `pay user --source` and `requests accept --source` after feature-level validation.
- Never displays full account or card numbers.

#### `venmo balance`

- Displays the user's Venmo wallet balance.
- Displays held or unavailable balance separately only if the API supplies a verified field with understood semantics.
- Does not claim to know external bank-account or card balances.
- Never infers a wallet balance from an unrelated payment-method limit.

#### `venmo transfer options`

- Performs one authenticated `GET /v1/transfers/options` and never moves money.
- Preserves standard/instant eligible destination branches; bounds each branch to 100 instruments
  and validates every relied-on ID/text/default. Inbound source metadata is ignored.
- Renders sanitized copyable instrument rows as `Id`, `Name`, `Type`, `Last 4`, `Default`,
  `Direction`, `Speed`, and `Estimated Completion`. The API's semantically unclear `asset_name`
  remains validated internally but is not rendered. The CLI also omits preferred-speed and
  branch-level estimate/fee summaries and does not infer dollars, cents, limits, or final fees from
  unverified fee metadata.

#### `venmo transfer out <AMOUNT_OR_ALL> [--speed standard] [--yes]`

- Runs current-account and balance checks plus fresh transfer options; selects only a unique default
  or sole standard bank destination and never accepts response order or a user-supplied ID.
- Accepts either the existing positive exact-decimal amount or exact lowercase `all`. `all` resolves
  once from the fresh positive available-balance snapshot, excludes on-hold funds, and fails before
  options/write when availability is zero or negative. The immutable plan preserves the selector
  and resolved integer cents.
- Treats `all` as a snapshot shorthand, not an atomic drain. Help and documentation disclose that
  boundary; details show the resolved amount and snapshot source, and default-No confirmation asks
  whether to transfer the entire displayed available balance. `--yes` skips only that prompt.
- Defaults an omitted `--speed` to `standard`; explicit `--speed standard` remains valid and every
  unsupported speed is rejected by the CLI.
- Sends one non-retried integer-cent POST after flushed default-No confirmation. Exact HTTP 201
  pending data must prove ID, timestamp, type, requested/net/fee arithmetic, dollar amount, and
  destination ID/type/suffix. Status alone, challenge, non-201, mismatch, or interruption is
  ambiguous.
- A separately approved $0.01 canary on 2026-07-17 matched exactly one pending outgoing activity
  record with the same transfer ID. This proves accepted submission, not settlement. Transfer-in
  is intentionally unsupported. Instant/debit cash-out, manual selection, OTP continuation,
  cancellation, and expedition remain unavailable.

### 3.7 Activity

#### `venmo activity list [--user <USERNAME>] [--limit <N>] [--before-id <TOKEN>]`

- Without `--user`, lists the authenticated account's existing feed with no behavior change.
- With `--user`, resolves an optional-`@` exact username through bounded search and authoritative detail, requires a personal profile, and lists the native personal-profile feed visible to the authenticated viewer. Business/public-only profile feeds remain a separate unsupported contract.
- Shows activity ID, timestamp, action/direction, counterparty, status, and sanitized note. The
  authenticated user's feed includes its required amount; other-user lists deliberately omit the
  amount column even when the private API supplies a value.
- Interprets direction and counterparty relative to the viewed user. Other-user pages accept only payment stories, require exactly one viewed-user party and a supported audience, and accept private records only when the authenticated viewer is the other participant.
- Accepts a null amount only for external social activity. A supplied amount remains strictly
  parsed, but other-user list mapping discards it before output. Detail omits Amount when the viewer
  is external; participant-owned activity still requires and renders exact money.
- Fetches exactly one source page using `--limit` as the server page size, with default 10 and maximum 50.
- Accepts the endpoint-native bounded opaque `before_id` value through `--before-id` and reports a validated `Next before-id: <TOKEN>` value after successful record output.
- Does not expose a generic public/friends feed or silently discard unsupported records.

#### `venmo activity info <ACTIVITY_ID>`

- Shows all understood fields for one activity record.
- Uses the globally unique story ID without `--user`. Payment details render absolute actor and target identities; transfer and authorization details retain current-account ownership validation.
- Is the primary recovery tool after an ambiguous payment, request creation, request-acceptance, or transfer outcome; request reads and the official app are also required for ambiguous decline state.
- Clearly distinguishes absent, inaccessible, malformed, pending, failed, and completed records.

### 3.8 Pending requests

```text
venmo requests list
venmo requests list --direction incoming
venmo requests list --direction outgoing --limit 50
venmo requests list --direction incoming --before <TOKEN>
```

- Lists pending requests involving the authenticated account.
- `--direction` is a `clap::ValueEnum`: `all`, `incoming`, or `outgoing`; default is `all`.
- Displays the canonical request ID accepted by `requests accept`, `requests decline`, and `requests cancel`, direction, counterparty, amount, note, creation time, and status when supplied.
- Incoming exact-`pending` records can be passed to `requests accept` or `requests decline`; outgoing exact-`pending` or `held` records can be passed to `requests cancel`. Each mutation re-fetches and revalidates the record authoritatively.
- Does not remind an outgoing request. Incoming decline remains a distinct `deny` operation and must never use outgoing `cancel` semantics.
- Fetches exactly one source page using `--limit` as the server page size, with default 10 and maximum 50. `--direction` is applied locally after that complete page is validated, so fewer than `--limit` rows may be rendered.
- Accepts the endpoint-native bounded opaque `before` value through `--before` and reports the source page's validated `Next before: <TOKEN>` even when local filtering leaves an empty result.
- Must be backed by a verified endpoint or a verified complete activity filter. A count without request records is insufficient.

#### `venmo requests info <REQUEST_ID>`

- Calls the broad authoritative payment/request detail capability used by mutation preflight, then
  applies a narrower public read policy.
- Requires the exact returned record ID, `action=charge`, and status exactly `pending` or `held`.
  Both incoming and outgoing open requests are accepted and rendered.
- Rejects `action=pay` records and every terminal or otherwise unsupported request status as usage
  errors. It never presents a payment or closed request as an open request.
- Performs exactly one lookup with no retry and displays all understood request fields with terminal
  sanitization.

#### Deferred `payments list` and `payments info <PAYMENT_ID>` evidence gate

- The intended `payments` resource covers incoming and outgoing peer money movement, including
  direct payments and the distinct resulting payment created by accepting a request. It excludes
  open requests, declined/cancelled requests, transfers, and card activity.
- Neither command is shipped without its own verified read contract. Existing evidence proves only
  the pending-request list query and pending RequestId detail. It does not prove a general payment
  list, its continuation, accepted-payment classification, or settled PaymentId detail.
- Dated evidence includes an HTTP 500 from `GET /payments/{payment-id}` for a settled nested
  PaymentId whose activity story was readable. `ActivityId` and `PaymentId` remain distinct, so
  `GET /stories/{activity-id}` cannot substitute for `payments info`.
- A separately governed bounded read-only dossier must prove direct and accepted-payment inclusion,
  open/declined request exclusion, payer/payee money direction, page bounds and continuation, and
  exact-ID detail behavior before the corresponding command is implemented. Routine tests and
  contributor verification remain service-free.

### 3.9 Local MCP server

```text
venmo-mcp
```

The initial server uses MCP over local stdio only. Stdout is exclusively the MCP JSON-RPC transport; startup failures, redacted diagnostics, and tracing go only to stderr. The server registers tools but no resources or prompts. Its registry is fixed before initialization for the lifetime of the process, so it does not advertise or emit dynamic tool-list changes. It performs no browser interaction, terminal prompting, credential mutation, financial mutation, hidden retry, or hidden pagination.

Before adopting the SDK transport, prove or add a bounded stdio boundary: each incoming JSON-RPC frame is at most 64 KiB before parsing, each serialized tool result is at most 2 MiB before stdout, and oversized input fails without logging or reflecting any payload. Allow one active Venmo tool call and no pending operation queue: a concurrent call receives a sanitized `server_busy` tool error rather than waiting unboundedly. Apply a per-process token bucket with capacity one and a two-second refill interval; excess calls receive `rate_limited` and are not delayed, retried, queued, or sent to Venmo. Initialization and tool discovery are not counted. If the reviewed SDK cannot enforce bounded frames without exposing raw payloads to logs, stop and resolve the transport design before implementation.

Initial tool catalog:

| Tool | Input | Structured result | Shared feature behavior |
| --- | --- | --- | --- |
| `venmo_auth_status` | Empty object | Safe account identity, local credential saved-at time, credential format | Same current-account validation as `venmo auth status` |
| `venmo_balance` | Empty object | Exact available and on-hold USD strings | Same typed balance read |
| `venmo_friends_list` | `limit` 1–50 (default 10), `offset` `u32` (default 0) | User records and always-present nullable `next_offset` | Same one-page friends use case |
| `venmo_users_search` | Validated `query`, `limit` 1–50 (default 10), `offset` `u32` (default 0) | User records and always-present nullable provisional `next_offset` | Same one-page public search, not internal recipient resolution |
| `venmo_payment_methods_list` | Empty object | Safe method IDs, labels/types, optional last four, default marker | Same payment-method list |
| `venmo_activity_list` | nullable exact `username`, `limit` 1–50 (default 10), nullable `before_id` (default null) | Tagged activity records, nullable resolved subject, and always-present nullable `next_before_id` | Same one-page self or personal-profile activity list |
| `venmo_activity_info` | Validated `activity_id` | One tagged activity record | Same activity detail lookup |
| `venmo_requests_list` | `direction` (default `all`), `limit` 1–50 (default 10), nullable `before` (default null) | Pending-request records, applied direction, always-present nullable `next_before` | Same one source page and local direction filtering |

Every tool declares strict `inputSchema` and `outputSchema`. Input DTOs use runtime unknown-field rejection and schemas with `additionalProperties: false`, then convert into existing feature/shared model types before any credential or network access. Output DTOs use string IDs, exact decimal money strings rather than floats, normalized RFC 3339 timestamps, explicit JSON `null` for absent values, tagged activity/counterparty variants, and always-present endpoint-native continuation fields whose values may be null. They never expose raw next URLs. Every success must conform to its declared `outputSchema` and should include the same JSON serialized as text content for clients that do not yet consume `structuredContent`.

Do not register tools for:

- `auth login`, `auth logout`, or any remote session mutation.
- Passwords, OTPs, OTP secrets, bearer tokens, device IDs, cookies, CSRF values, or alternate credential sources.
- Help, version, or package management.
- `pay user`, request creation, request acceptance, or request decline until their CLI contracts and the later MCP write phase are complete.
- Internal exact-recipient resolution, automatic funding policy, request detail, or any lower-level API merely because implementation code exists.

“Auth-status-only” describes the MCP authentication command surface: `venmo_auth_status` is the sole auth-oriented tool.

#### 3.9.1 Tool annotations and permission signaling

Every initial tool must explicitly publish standard MCP annotations equivalent to:

```json
{
  "readOnlyHint": true,
  "destructiveHint": false,
  "openWorldHint": true
}
```

`openWorldHint` is true because each tool can contact Venmo and may return dynamic, user-authored external content. `idempotentHint` may be omitted for the read-only catalog because it is defined primarily for state-changing tools and this project does not want metadata to imply an automatic-retry policy. Each tool also has a clear human-readable title and description that states when it returns private financial, social, or account data.

Any future payment, request creation, request acceptance, or request decline tool must be annotated equivalent to:

```json
{
  "readOnlyHint": false,
  "destructiveHint": true,
  "idempotentHint": false,
  "openWorldHint": true
}
```

Annotations are hints for trusted harness UI, filtering, and approval policy. They are not a server-side permission mechanism, do not prove human approval, and cannot make an unsafe tool safe. MCP has no universal standard hint for “returns sensitive financial data”; descriptions, documentation, and optional namespaced metadata may communicate that fact, but server capability restriction and host policy remain authoritative. Snapshot the exact tool allowlist, schemas, titles, descriptions, and annotations so a later change cannot silently reclassify a write as read-only.

## 4. Explicitly deferred commands and features

- Multi-recipient or batch payments.
- Split payments.
- The old `charge` alias.
- Reminding outgoing requests, local-only request dismissal, partial acceptance, or changing a requested amount.
- Adding, removing, accepting, or blocking friends.
- Transfer-in, instant/debit cash-out, manual transfer destination selection, challenge
  continuation, cancellation, and expedition.
- Adding or removing payment methods.
- Public or friends activity feeds.
- Comments, likes, profile editing, disputes, refunds, or chargebacks.
- Phone-number or email recipient resolution.
- Multiple account profiles.
- Browser automation, browser-cookie/session import, and web-session authentication.
- Persistent token storage outside the OS credential store.
- JSON output from the terminal CLI in the first release; required MCP structured JSON is a separate adapter contract.
- Remote MCP transports, MCP OAuth, remotely hosted MCP service operation, and network listeners in the initial MCP release.
- MCP resources, prompts, sampling, elicitation, tasks, and client functionality until a specific reviewed use case is approved.
- MCP authentication mutation tools and any MCP secret-input or credential-override path.
- Financial MCP tools in the initial server; the later gated phase in Section 11.4 and Phase 10 remains unavailable by default.

## 5. `clap` design

Use derive models as the authoritative command schema:

```rust
use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "venmo", version, about)]
pub struct Cli {
    /// Write bounded debug diagnostics to stderr.
    #[arg(long, global = true)]
    pub debug: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Auth(AuthArgs),
    Pay(PayArgs),
    Friends(FriendsArgs),
    Users(UsersArgs),
    Balance,
    Activity(ActivityArgs),
    Requests(RequestsArgs),
}
```

Model request operations as exhaustive `RequestsOperation::List`, `Create`, `Accept`, `Decline`, and `Info` variants. Keep their argument payloads as independent typed structs rather than a partially populated default-subcommand shape.

Requirements:

- Prefer typed fields and custom `FromStr` value types over parsing `Vec<String>` manually.
- Model `PayOperation::Options` and `PayOperation::User(PayUserArgs)` as exhaustive grouped operations, keeping payment-only fields out of the read-only options branch and matching `TransferOperation::Options` terminology.
- Prove that `requests create @user ...`, `requests create <bare-or-numeric-username> ...`, `requests create @accept ...`, `requests accept <request-id>`, `requests decline <request-id>`, and `requests cancel <request-id>` dispatch exactly as documented.
- Reject removed top-level and mixed forms such as `request @user ...`, `accept <id>`, `decline <id>`, and `requests accept <id> <amount>`.
- Reject `--from` everywhere; accept `--yes` only on `pay user`, `requests create`, `requests accept`, `requests decline`, and `requests cancel`, and reject it on `pay options`.
- Reject `--dry-run` everywhere; it is not part of the schema.
- Use `ValueEnum` for request direction.
- Put validation that depends only on one value in `clap` parsers.
- Put cross-field and network-dependent validation in the owning feature.
- Snapshot top-level and every subcommand's help output.
- Source version output from Cargo metadata; do not hardcode it separately.

`clap` does **not** provide interactive prompting. It parses arguments, validates the command shape, and generates help. Use `std::io::IsTerminal` for TTY detection and `dialoguer` behind a narrow prompt adapter for hidden token entry and default-No confirmation.

This section defines only the `venmo` terminal adapter. `venmo-mcp` must not invoke the `clap` command dispatcher, parse terminal table output, or reuse human output writers. Its stdio stream is the protocol transport, so even a normal CLI banner or success message would corrupt framing. If a future startup policy flag such as `--allow-financial-writes` is added, parse only that server-composition setting before stdio service starts; it must never accept a secret or stand in for per-call human approval.

Interactive mutation confirmation applies to `pay user`, `requests create`, `requests accept`, `requests decline`, and `requests cancel`. On a TTY, each command shows its complete details and asks for default-No confirmation unless `--yes` is present. With `--yes`, the details remain visible but the prompt is skipped. Off a TTY, each command fails before writing unless `--yes` is present. `auth login` remains an intentional non-financial exception because credential and OTP input is interactive-only; it has no `--yes` option.

Prompt adapter rules:

- Use `dialoguer::Input` for the account identifier, `dialoguer::Password` with result reporting disabled for password/OTP/device-ID secrets, and `dialoguer::Confirm` with `default(false)` for mutation confirmation.
- Render through `dialoguer::console::Term::stderr()` so prompts never contaminate stdout data.
- Use `interact_opt` for confirmation so Escape or `q` maps to `Cancelled`. Map password interruption, Ctrl-C, EOF, and terminal failures into typed cancellation or terminal errors rather than panics; ordinary token text such as `q` remains valid password input.
- Use `SimpleTheme` by default. Any later color theme must follow the CLI's TTY and `NO_COLOR` policy.
- Sanitize every prompt inside the concrete CLI prompt adapter immediately before passing it to `dialoguer`; feature code may provide typed/raw display fields but cannot bypass this terminal boundary.
- Keep `dialoguer` types out of features and shared models; tests should substitute a fake prompt port.
- Disable unused default features and enable only password support unless implementation proves another feature is required.

## 6. Bearer-token storage

### 6.1 Selected backend

Follow the same high-level model as `pmail`: use the native OS credential store behind independent `CredentialReader`, `CredentialWriter`, and `CredentialDeleter` capabilities with one shared error type.

Use the maintained Rust `keyring` convenience crate's default `v1` API as the **only persistent credential backend**. Do not compose `keyring-core` and platform-store crates directly, expose a backend selector, or add an encrypted/plaintext file fallback in the first release.

| Platform | Native store | Requirement |
| --- | --- | --- |
| macOS | Keychain Services | Usable login keychain; the OS may request approval |
| Windows | Credential Manager | Normal interactive user profile |
| Linux/Unix desktop | Secret Service | Secret Service provider and user-session D-Bus |

Headless Linux without Secret Service is not a supported persistent-auth environment. Fail closed with setup guidance rather than downgrading storage security.

### 6.2 Entry contract

- Service: `venmo-cli`.
- Account/user: `default`.
- Secret value: one versioned JSON envelope.
- Searchable labels/attributes contain no token, note, or full device ID.

Envelope fields:

- Schema version.
- Normalized access token without `Bearer `.
- Device ID.
- User ID.
- Username.
- Optional display name.
- Credential save time (serialized as the compatibility key `createdAt`).

Construct `Authorization: Bearer <token>` only at the HTTP boundary. `AccessToken` must have redacted `Debug`/`Display`, and logs and errors must never receive the exposed token value.

### 6.3 Keyring selection due diligence

Verified on 2026-07-09 from [crates.io](https://crates.io/crates/keyring), its [API metadata](https://crates.io/api/v1/crates/keyring), and the [upstream repository](https://github.com/open-source-cooperative/keyring-rs):

- First published in February 2016.
- 80 published versions across more than ten years.
- Latest non-yanked release at review time: `4.1.4`, published 2026-07-06.
- 16,285,078 total downloads and 6,155,272 recent downloads reported by crates.io.
- 555 reverse-dependent crates.
- Active, non-archived repository maintained under the Open Source Cooperative.
- Default `v1` feature selects Keychain Services, Windows Credential Manager, and a `zbus` Secret Service backend.
- On 2026-07-10, the Rust adapter passed an isolated macOS Keychain Services create/read/versioned-decode/delete/read-missing smoke test using a randomized test-only service/account and synthetic secret; the production `venmo-cli` / `default` entry was not touched. Legacy cross-library accessibility remains a separate gate.

Popularity does not replace review. Recheck release status, advisories, licenses, backend selection, and platform behavior when locking dependencies and before releases.

### 6.4 Error and migration behavior

- Match typed `keyring` errors such as `NoEntry`, `NoStorageAccess`, `BadEncoding`, `TooLong`, and `Ambiguous`; never match error strings.
- Distinguish missing, locked/unavailable, corrupt, invalid, oversized, and ambiguous entries.
- `auth logout` deletes the entry directly without parsing it first.
- `auth login` replaces a readable or single targetable unusable old entry only after the newly issued token validates, then reads it back and verifies exact versioned content before reporting success. Failed login leaves a readable old entry untouched.
- Explicit successful login may replace a valid different-account entry. Ambiguous or inaccessible entries may not be replaced.
- Decode the legacy TypeScript camelCase envelope for one-time migration.
- Prove on each supported platform whether Rust `keyring` can access the item written by `@napi-rs/keyring` under the same service/account.
- If transparent migration fails, do not delete the old item silently; document a new explicit login or provide a narrowly scoped migration helper.

### 6.5 MCP credential capability boundary

`venmo-mcp` uses the same fixed service/account entry but receives only a read capability:

- Feature paths that only load credentials, including auth status, require only `CredentialReader`. Login requires reader plus writer for verified persistence; local logout requires only `CredentialDeleter`.
- Add a private production read-only credential façade, or equivalent concrete composition, that implements only `read_credential`; MCP server state must not have save/delete methods available by type.
- Keep `CurrentAccountApi` and `PasswordLoginApi` as independent capabilities; terminal login composes only the capabilities it needs, while MCP receives no password/device-trust capability.
- Load and decode the credential separately for every tool call, use it only for that call, and drop it afterward. Never cache the bearer token, device ID, or whole `CredentialEnvelope` in the server.
- Constructing the server and handling `initialize` or `tools/list` must not instantiate a keyring entry, trigger an OS approval prompt, or make a network request.
- No MCP tool input, server flag, environment variable, config file, stdin payload, or alternate backend may supply or override authentication material.
- Missing, corrupt, locked, unavailable, ambiguous, or rejected credentials map to stable sanitized tool errors that direct the operator to the terminal `venmo` authentication commands without exposing platform source chains.
- Per-call loading lets an already-running MCP process observe a successful `venmo auth login` replacement on its next invocation without restart.

The second executable can have a distinct native-code identity from `venmo`. Validate macOS Keychain ACL/approval behavior manually with an isolated test service/account before claiming support. On Linux, an MCP host launched outside the desktop login session may lack Secret Service or user-session D-Bus even when the terminal CLI works. Fail closed with actionable setup guidance; never introduce a plaintext or environment-secret fallback. Automated validation must not access any native production credential entry.

## 7. Private API scope and discovery gates

### 7.1 Historical TypeScript-client evidence

The removed TypeScript client provided historical starting contracts for:

- Revoke access token.
- Current account.
- Search users.
- User by ID.
- Payment methods.
- Create payment/request.

These still require fresh Rust contract tests; existing behavior does not prove the private API is currently stable.

### 7.1.1 Authentication discovery and selected experiment

Read-only Firefox inspection on 2026-07-11 established that the current Venmo web application uses short-lived secure cookies/session state and GraphQL rather than exposing the planned long-lived `Authorization: Bearer` mobile token. Raw browser session values accidentally entered transient tooling output, so that session was invalidated and browser DevTools inspection is prohibited until a structural redactor is in place. The web session model remains out of scope.

An authorized private reference implementation was then reviewed. It pins `venmo-api==0.3.1` and attempts the historical mobile flow: `POST /oauth/access_token` with identifier/password/device ID, SMS challenge via `/account/two-factor/token`, OTP completion through `/oauth/access_token`, and device trust through `/users/devices`. Its token-only mode merely imports an existing token. Its helper is not copied: it has no contract tests, relies on broad exception handling/private assumptions, stores secrets in a dotenv file, and appears incompatible with the pinned library's response type and OTP-secret handling.

The Rust CLI independently implements the narrow historical wire contract using typed, zeroizing values, fixed-origin transport methods, bounded responses, allowlisted response headers, native keyring persistence, and mock-server tests. A human-driven test on 2026-07-11 used a newly generated device ID and received a structured HTTP `403` with error code `240` before OTP or token issuance. No credential was stored. This matches public 2024 reports that Venmo rejects the historical password endpoint for untrusted device IDs; the code is generic and does not prove an incorrect password. Do not repeatedly retry that path.

The approved read-only experiment imported the current web session's short-lived `api_access_token` together with its matching `v_id`, both through hidden local prompts, then performed only current-account validation before storage. The user retrieved both values manually; they never entered source, command arguments, chat, logs, screenshots, or agent output. Validation failure would have stored nothing. Even the successful result does not establish a durable authentication contract: enabled financial commands must not rely on an imported web token until its lifetime and `/v1` compatibility are independently proven. Browser-cookie storage and automatic browser extraction remain out of scope. The later financial validations used the separately verified mobile-issued credential path.

That import experiment succeeded on 2026-07-11: the matching web `api_access_token` and `v_id` pair authenticated the inherited mobile-private current-account read and the exact versioned credential was stored in the OS keychain. An immediate separate `auth status` process reloaded the keychain entry and repeated the current-account read successfully with stdout suppressed. No secret value or account identity is retained in this plan. This proves only immediate cross-API compatibility for that one read; observed expiry, refresh behavior, and every other endpoint remain separate gates.

Follow-up public-source research on 2026-07-11 found no evidence of an endpoint that accepts an existing web `api_access_token` plus `v_id` and exchanges them for a distinct persistent mobile bearer token. Current and historical authentication source in `zackhsi/venmo`, `aaymeloglu/venmo-mcp`, and `adbertram/cli-tools`, together with the complete discussion in `mmohades/Venmo#86`, showed only direct use of an existing bearer token or a separate username/password login. The archived 2024 web bundle identifies `api_access_token` and `v_id` as cookie names and calls `GET /api/auth` to recover session fields including `accessToken`; that is session introspection, not token exchange. No speculative exchange route, request, or live probe will be added.

The only evidenced route for minting the historically reusable mobile token is a credential login using a device identifier that Venmo already trusts. It sends one `POST /v1/oauth/access_token` request with the trusted browser `v_id`/`deviceId` as the sensitive `device-id` header and JSON fields `phone_email_or_username`, `client_id: "1"`, and `password`; a successful response contains `access_token`. If Venmo returns the recognized `81109` challenge, the historical flow requests SMS through `/v1/account/two-factor/token`, submits the OTP to `/v1/oauth/access_token?client_id=1`, and then best-effort trusts the device through `/v1/users/devices`. Public reports from March through December 2024 say that a `v_id` or `/api/auth` `deviceId` obtained after a completed browser login allowed this flow where a generated ID failed, and the June 2026 `aaymeloglu/venmo-mcp` instructions independently switched from generated IDs to a browser-derived device identifier for the same reason. The web access token is not an input to this issuance flow.

Those sources are unofficial and do not prove present-day token lifetime. The historical client says a newly issued mobile token remains valid until explicit logout, and issue participants reported reusing such tokens, but no controlled current expiry test or official guarantee was found. The web cookie's archived 30-minute `maxAge` also does not establish the underlying bearer's lifetime. On 2026-07-11 the owner selected a single trusted-device mobile-login experiment: password login hidden-prompts for a manually obtained trusted browser device ID instead of generating one. It performs one non-retried issuance attempt, follows OTP only after the exact recognized challenge, validates any issued token with the current-account endpoint, stores nothing before validation, and emits no credential, identifier, header, body, or account data. The existing Rust transport and authentication feature flow enforce the remaining issuance safeguards; a live attempt occurs only while the owner enters secrets locally.

That single trusted-device mobile-login experiment succeeded on 2026-07-11. The initial credential request issued a bearer token without an SMS-OTP challenge, the current-account validation passed, and the exact credential was saved and reloaded from the OS keychain. The then-current implementation made an unnecessary post-save `/users/devices` request even after direct password success; Venmo rejected only that redundant request with HTTP 400 and code `81124`. No credible public mapping for `81124` was found, so the code does not assign it a guessed meaning. The historical reference flow calls `/users/devices` only after OTP, and the Rust orchestration now likewise treats direct password success as proof that the existing trusted device was accepted and skips the redundant mutation. The issued token remained valid throughout this outcome.

On 2026-07-11, one separately authorized attempt to enroll a generated CLI device through `/users/devices` using the valid mobile token was definitively rejected with HTTP 400 and error code `81124`. There was no retry, no token was issued by that enrollment attempt, and the pending local experimental keychain entry was deleted. Together with the earlier generated-device password-login rejection with code `240`, this does not establish any fresh-device bootstrap path. The temporary ignored enrollment test and its direct development-only UUID dependency were removed; production authentication continues to require and preserve a device identity Venmo already trusts.

An authorized historical stored-device replacement experiment consumed only the `USERNAME` and `PASSWORD` fields from the owner's local 1Password `Venmo` item in bounded process memory. No field value, 1Password session token, device ID, bearer token, request body, response body, or account identity entered arguments, environment variables, files, logs, chat, or tool output. The experiment reused the existing Keychain device ID internally; Venmo issued a replacement token directly without OTP, same-account current-account validation passed, and the replacement credential was saved and read-back verified. A separate normal `auth status` process then reloaded and validated it successfully with stdout suppressed. The temporary 1Password probe was removed. This proved a wire behavior but the product no longer exposes stored-device replacement: every explicit login now requires a newly supplied trusted device ID. It does not establish headerless authentication, fresh-device bootstrap, a refresh grant, unattended renewal, or guaranteed lifetime.

A subsequent authorized fresh-device experiment established why browser-derived IDs work. A normal Chrome process with an isolated temporary profile received a new secure `v_id` before authentication. After the owner completed Venmo's official login and security challenge, that exact ID was captured only in local bounded process memory and supplied to one mobile password-login initiation. Venmo issued a mobile bearer directly without OTP; same-account current-account validation, Keychain replacement/readback, and a separate rebuilt `auth status` process all succeeded. This proves that completing the current web identity flow can make a newly minted web `v_id` acceptable to the mobile-private password endpoint. It does not establish a standalone device-registration request or make browser-assisted bootstrap an acceptable product design.

The same session's approved local network capture showed the current trust-establishment state machine. `venmo.com` redirects through `account.venmo.com` to `id.venmo.com`; an initial identity request is gated by DataDome, followed by reCAPTCHA Enterprise and PayPal FraudNet/browser-risk collection. The identity application then performs GraphQL public-credential and password verification, creates and completes an MFA/authflow challenge, and finalizes through `authflowNonceGql`. When remembering is enabled, that final mutation calls the same private-credential resolver with `userPreferences.isRememberedDevice: true`. It remains bound to same-origin cookies, CSRF, `_sessionID`, `ctxId`, refreshed `rData`/`fn_sync_data`, and prior server-side challenge state. The account callback subsequently establishes web access/identity credentials, and `/api/auth` exposes the resulting account session. `/api/device-data` carries a separate risk correlation and is not a `v_id` registration endpoint.

This is not a reproducible pure-HTTP CLI contract. Before credential verification, the server requires JavaScript-generated DataDome state, reCAPTCHA output, FraudNet synchronous and asynchronous telemetry, browser fingerprints, timing/input signals, and opaque session-correlated risk state. Synthesizing, replaying, or impersonating those values would be an anti-bot/security-control bypass, not a supported login implementation, and would be brittle across platforms. No OAuth device-authorization grant, verification URL/user code, magic-link bootstrap, refresh grant, or portable remember-device mutation was found. The owner explicitly rejected shipping a browser-assisted bootstrap, so production first login continues to require a manually obtained trusted device ID. Only evidence of a legitimate current native-mobile onboarding contract or an official device-authorization flow would reopen that decision.

The raw capture was kept only in a mode-0700 temporary directory with mode-0600 files, never printed through agent output, and deleted immediately after local analysis; the temporary Chrome profile and all experimental capture code were also removed. The owner elected to rotate exposed login/session material after the experiment and should review official remembered-device and session controls.

Immediately afterward, separate processes successfully used the mobile-issued token for `auth status`, `balance`, bounded `friends list`, bounded `users search`, the payment-option listing now exposed as `pay options`, bounded `activity list`, and bounded `requests list`, with sensitive stdout discarded. This proves authentication compatibility across the implemented read-only adapters and native keychain reload. It does not establish token lifetime or authorize any financial-write contract; no financial mutation was attempted.

The same credential pair was then exercised against every currently implemented read-only API command. `auth status`, bounded `users search`, and the payment-option listing now exposed as `pay options` all succeeded. User search confirmed the inherited endpoint is fuzzy even for username-specific requests and can have more than ten matches; no ordering or exact-lookup guarantee may be inferred from the first result. Public single-token searches now intentionally normalize optional `@` spelling to the same `type=username` request, while multi-word searches remain general; the separate internal exhaustive traversal retains its fail-closed exact-match checks. Payment-method listing returned multiple method classes and one default-role indication, proving the current list shape but not transaction eligibility, fees, fallback, or actual funding behavior. Live output exposed a duplicated `@` rendering bug; it was fixed and regression-tested. No account identity, user result, payment-method identifier, institution name, or last-four value is retained here.

The root Rust package implements `friends list`, `balance`, `activity list`, `activity info`, and `requests list` behind typed feature ports and strict response mapping. Their synthetic HTTP/feature/output test suites pass, and each command completed a read-only live smoke test with sensitive stdout suppressed.

### 7.2 New read capabilities required by the redesign

| CLI capability | Historical lead | Required validation |
| --- | --- | --- |
| Friends | `GET /users/{user-id}/friends` in historical unofficial docs | Auth headers, pagination, visibility, response envelopes |
| Balance | Account and/or payment-method responses have historically exposed Venmo balance | Exact source field, decimal semantics, held balance, missing balance behavior |
| Activity list | `GET /stories/target-or-actor/{user-id}` in historical unofficial docs | Authenticated visibility, pagination, amount/status fields, private records |
| Activity detail | `GET /stories/{activity-id}` in historical unofficial docs | ID semantics, status, amount, counterpart, errors |
| Pending requests | No sufficiently reliable record-list contract identified yet | Find a request-record endpoint or prove activity filtering is complete |
| Pending request detail | Live-validated `GET /payments/{request-id}` contract | Canonical request ID, ownership/direction, current and terminal state, action, requester, immutable amount/note/time, not-found and stale behavior |

Historical research sources, including [mmohades/VenmoApiDocumentation](https://github.com/mmohades/VenmoApiDocumentation), are leads only. They are old, unofficial, and cannot be treated as a current contract. Do not copy source from third-party clients with incompatible licenses.

#### 7.2.1 Dated sanitized read-contract evidence

On 2026-07-11, a purpose-built ignored test used the authorized keychain credential to issue bounded, fixed-origin, authenticated GET requests and emitted only structural schema facts. It retained no raw body, token, device ID, account/user/request/activity ID, name, note, amount, timestamp, URL, or screenshot. The first run received `401` after the imported web token rotated; the owner refreshed only the hidden token, and the immediate second run established these candidate mobile-private `/v1` contracts:

- `GET /account` returned HTTP 200 with string fields `data.balance` and `data.balance_on_hold`, plus `data.user`. These are the sole sources for the signed exact-USD **Available** and **On hold** wallet values; external balances are never inferred.
- `GET /users/{self-user-id}/friends?limit=<N>&offset=<N>` returned HTTP 200 with `data: User[]` and `pagination.next/previous`. The next link used the same fixed origin and route and carried only `limit` and `offset` query fields.
- `GET /stories/target-or-actor/{self-user-id}?limit=<N>&social_only=false` returned HTTP 200 with story records and pagination. A story had a canonical top-level story ID, creation timestamp, private audience, note, and nested payment containing its own ID, `status`, `action`, positive amount, actor, target user, audience, and timestamps. Its next link used the same route and the fields `before_id`, `limit`, `only_public_stories`, and `social_only`.
- The observed activity timestamps use `YYYY-MM-DDTHH:MM:SS` without an offset. The project owner chose to interpret this legacy Venmo shape as UTC; explicit-offset RFC 3339 values are also accepted. CLI rendering subsequently converts the preserved instant to the system-configured time zone, independently of that parsing assumption. This assumption is documented and remains a release-time semantic check rather than assigning the system zone while parsing Venmo data.
- A bounded 200-record follow-up established three current story classes: peer `payment`, wallet `transfer`, and card `authorization`. Transfer records identify exactly one external `source` for incoming `add_funds` activity or one external `destination` for outgoing transfers, plus amount, status, type, request timestamp, account name/type, and optional last four. Authorization records identify the authenticated user, merchant display name, amount, status, and creation time. The typed activity model preserves those distinctions, renders transfer type in the action, uses an external sanitized counterparty label, and rejects ambiguous source/destination or wrong-account authorization records.
- `GET /stories/{story-id}` returned HTTP 200 with the corresponding story shape. A competing `GET /payments/{nested-payment-id}` returned HTTP 500 for the observed settled activity, so `activity info` uses canonical story IDs and the story-detail route. A successfully retrieved pending, failed, expired, or cancelled record remains command data and exits 0; its authoritative status is not converted into a CLI failure.
- `GET /payments?action=charge&status=pending,held&limit=<N>` returned HTTP 200 with actual payment/request records and pagination, not counts. Records had a canonical request/payment ID, `action=charge`, pending-state status, actor, target user, positive amount, note, audience, and dates. Direction is derived relative to the authenticated user: actor=self is outgoing and target.user=self is incoming. The next link used the fixed payments route and only `action`, `before`, `limit`, and `status`.
- `GET /payments/{request-id}` returned HTTP 200 with the corresponding pending charge record and additional fee/medium/refund fields. Unlike the pending-request list, which accepts only `action=charge` and `status=pending|held`, detail mapping preserves `charge` or `pay` plus any understood status so mutation preflight can reject stale, terminal, or action-changed records safely. Detail still requires exactly one authenticated-account party, a positive exact amount, and an exact returned request ID. The later write validation below separately establishes acceptance and decline mutation semantics.

The Rust adapters encode these findings with endpoint-specific opaque tokens, exact path/query/header assertions, strict typed IDs/money/timestamps, exact detail-ID checks, and same-origin/same-route continuation validation. Synthetic fixtures cover malformed records, unsupported state/action values, duplicate/conflicting IDs, oversized pages, and untrusted next links. Each public feature call buffers one complete source page before stdout, applies request direction locally, preserves server order and source continuation, and rejects repeated/no-progress values. Only internal exact-recipient resolution performs a bounded multi-page traversal.

This evidence proves immediate read compatibility only. The imported web token rotates quickly, current mobile-client header/User-Agent behavior remains unverified, final-page exhaustion should receive an additional live boundary check, and no financial write may rely on these findings without completing its separate Phase 0 contract.

After implementation, read-only live smoke tests succeeded for `auth status`, `friends list`, `balance`, `activity list`, `activity info` using canonical payment/transfer/authorization story IDs selected only in process memory, and `requests list` with all/incoming/outgoing filters. Sensitive command output was discarded and no identifiers or account data were retained. The first activity runs safely exposed offsetless timestamps, capitalized `False` continuation booleans, and deeper transfer/authorization records. The owner selected UTC semantics for the legacy timestamp form; the parser now supports it, while user-facing CLI rendering independently converts typed instants to the system zone. Continuation booleans are validated case-insensitively without relaxing their value, all three observed activity classes have synthetic contract regressions, and repeat live runs succeeded. A historical bounded friends traversal reached verified exhaustion, while the larger activity dataset was not claimed complete.

On 2026-07-11, the owner explicitly authorized read-only live pagination validation using the active mobile-issued credential. After the public CLI was aligned with endpoint-native pagination, a one-record first page exposed a validated continuation and a second process successfully supplied the exact native `--offset`, `--before-id`, or `--before` value for each of `friends list`, `users search`, `activity list`, and `requests list --direction all`. A separate bounded friends traversal used 50-record source pages and reached a real final page with no continuation. All record output and continuation values were suppressed and discarded; no account data, query result, identifier, note, amount, or continuation value was retained, and no mutation endpoint was called. This confirms current first-to-second endpoint continuation behavior for all four adapters and final-page signaling for friends. Natural final-page exhaustion for user search, activity, and pending requests remains a release observation rather than a reason to poll aggressively.

### 7.3 Request acceptance and decline write contracts

The former TypeScript client establishes only how to create a payment or negative-amount request. Expanded public research on 2026-07-12 supplied historical action semantics, and controlled live validation on 2026-07-14 established current behavior.

The implemented contracts independently validate:

- The canonical request identifier and a complete lookup path.
- The acceptance method, path, required body, and concurrency or state preconditions.
- Proof that the operation settles the identified request rather than creating an unrelated payment.
- Whether the operation supplies any funding-source ID and whether the actual source can be verified.
- Success response identifiers and the resulting request/activity state transition.
- Confirmed stale/already-settled errors versus transport outcomes that remain ambiguous.
- Whether the operation has an idempotency key or conditional-state mechanism; regardless, the client performs no automatic write retry.
- Whether rejecting an incoming request is a terminal remote denial or merely a local UI dismissal, and its exact resulting server status.

Do not infer that a normal payment to the requester is equivalent to acceptance, or that outgoing-request `cancel` declines an incoming request. Preserve the exact validated operation-specific contracts and fail closed on drift.

#### 7.3.1 Dated public write-contract evidence and selected discovery limit

Public-source research completed on 2026-07-12 produced the contract leads below. Controlled live validation established pay and request creation later that day and acceptance and decline on 2026-07-14:

**Acceptance validation (2026-07-14):** After a separate immediate confirmation, acceptance sent exactly one `approve` update for the owner-approved legitimate $25 request with no retry or field variation. The response represented the resulting settled payment under a distinct payment ID rather than preserving the pending-request ID. The original validator conservatively reported ambiguity; no second write was attempted. Immediate read-only reconciliation proved the request disappeared, a matching private outgoing $25 payment was settled, and available balance decreased by exactly $25. The owner then confirmed exactly one matching settled payment and no remaining request in the official app. A separately executed one-cent acceptance also settled exactly once; its response retained request-style `action: charge` and original party orientation while read-only activity exposed the resulting private outgoing `pay`. Synthetic validation now supports both this live charge-oriented response and the historical pay-oriented representation, requiring the corresponding exact parties plus matching amount, note, audience, supported status, a valid distinct payment ID, and a payment timestamp not predating the request. One later, separately approved one-cent attempt returned a non-success response that could not prove no write occurred; reconciliation showed the request still pending, no matching activity, and no balance change, so it was not retried. A final separately approved one-cent acceptance passed the revised validator and emitted normal success output; reconciliation proved the request disappeared, exactly one matching private outgoing payment settled, and available balance decreased by exactly one cent. Successful responses still did not prove final funding source or fee.

**Decline validation (2026-07-14):** One separately authorized one-cent live validation sent one `deny` update. It returned the original request ID with exact terminal `cancelled` status; the request disappeared from pending reads, payment activity did not change, and available balance remained unchanged. No retry or field variation occurred.

- The maintained `joshhubert-dsp/py-venmo` fork at commit `899cd288d11d3599176ff3b8d40236a7d4886027` states that it targets the mobile API and that payments work. Its payment path first posts JSON to `/v1/protection/eligibility` with blank `funding_source_id`, `action: "pay"`, country code, target type/ID, note, and integer-cent amount. A positive payment then posts JSON to `/v1/payments` with a client UUID, user ID, `audience: "private"`, decimal-dollar amount, note, eligibility token, and funding-source ID. Its response model expects `data.payment`. This is the strongest current public pay lead, but the fork's February 2026 `Venmo/26.1.0` compatibility header is older than the App Store's July 2026 version and must not be copied as a present-day header contract.
- The same fork creates a request through `/v1/payments` with a client UUID, user ID, private audience, negative decimal-dollar amount, and note, omitting eligibility and funding fields. Retired official documentation independently used a negative amount for charge creation. This supplied the request-creation contract lead later validated live.
- Official Venmo payment documentation archived on 2021-06-12 describes `PUT /payments/:payment` as “Complete a Payment Request.” It explicitly assigns `approve` or `deny` to a request received by the authenticated user and `cancel` to a request made by that user. Historical `deet/govenmo` commit `21492682f08bb876a9a8cabaf249733e642e9c20` independently sends form-encoded `action=approve|deny|cancel` to that route. Its success assumption is a direct `data` Payment with a nonempty ID; the archived approval example shows resulting `action=pay`, `status=settled`, and completion data. This is strong historical evidence, not current-production proof.
- Maintained descendants support bearer-authenticated JSON `PUT /v1/payments/{id}` with `action: "cancel"` or `"remind"`, but their listing scope proves only outgoing/requester-owned requests. They do not establish incoming denial. The Rust balance-covered contract combines current mobile authentication/JSON convention with the historically explicit `approve`/`deny` actions and retains exact reconciled live evidence.
- Signer-verified Android 26.13.0 establishes protected peer payment independently from request acceptance: source-bound form eligibility has no `action` field; the app selects an applied fee whose product URI starts with `venmo:product:buyer_protection:`; protected F2 sends `transaction_type: goods_services_protected`, the complete singleton fee, and quasi-cash metadata; the response exposes protected `type`; and SMS step-up rebuilds the same protected transaction while adding verification metadata. The CLI implements those exact boundaries with stricter duplicate matching rejection and service-free tests. No protected live mutation has run.
- Subsequent signer-verified static analysis of Android 10.31.1 and 26.13.0 established a stable newer acceptance contract: unacknowledged notification lookup, exact payment-ID association to a request notification's own ID, source-bound form eligibility, and `PUT /v1/requests/{request-action-id}` options carrying `funding_source_id`, `eligibility_token`, bounded applied-fee records, and quasi-cash metadata. Android 26.13.0 additionally models current wrapper notifications whose actionable request is under `additional_properties.request`, and has planning/linking and webview/SMS-step-up branches. A separately approved bounded client-1 read emitted only structure/counts and confirmed the outer notification ID, nested request-action ID, and nested payment ID are distinct; no values were retained. Native code includes applied fees only when its purchase-protection toggle is on. The CLI therefore defaults unprotected, omits fees in that mode, and normalizes/submits them only for explicit `--protect`, where output labels the estimated seller deduction and recipient proceeds. An outer-wrapper-ID F4S attempt returned HTTP 404 and reconciled as no mutation. A corrected nested-action-ID acceptance returned 200 and reconciled as one matching settled $20 activity with the request removed, proving current client-1 authorization for that branch. Actual source/final fee, protected approval, and continuations remain unproven or ambiguous/non-retried.
- Current official [Purchase Protection](https://venmo.com/purchaseprotection) and [Buying and Selling FAQ](https://help.venmo.com/cs/articles/buying-and-selling-on-venmo-faq-vhel138) guidance states that eligible protected personal payments have no extra buyer fee, the seller pays 2.99%, and that fee is removed from the amount received. The CLI uses the exact bounded eligibility-response cents rather than recomputing the published rate, labels the value as an estimate, and subtracts it only from displayed recipient proceeds.
- A read-only live CLI preflight resolved the exact approved test counterparty through an authoritative user-detail fetch, proved a personal/payable non-self target, excluded wallet balance from external backup selection, and received a transaction-specific eligibility result totaling exactly zero fee. The search reached its bounded traversal limit, so the verified rule accepts one exact username match only after detail-by-ID returns the same ID and case-insensitive exact username; no match is reported as username not found.
- After separate immediate approval, the Rust `pay user` command sent exactly one private $0.01 payment with a unique generated note and zero retries. Its response matched `data.payment` and the submitted action, parties, amount, note, private audience, supported status, creation timestamp, and canonical ID. A separate CLI activity read found the matching latest one-cent pay, and the owner independently confirmed exactly one matching payment in the official Venmo mobile app.
- After the payment was reconciled and a second immediate approval was obtained, the Rust request command sent exactly one private $0.01 request with a distinct generated note and zero retries. Its response passed the same operation-specific strict checks, a separate CLI request read found the matching pending outgoing request, and the owner independently confirmed exactly one matching request in the official mobile app. No token, device ID, user/payment/request ID, note, raw body, account identity, funding details, or mobile traffic was retained.

**Visibility scope and validation decision (2026-07-14):** The owner approved implementing `friends` and `public` creation audiences for both `pay user` and direct `request`, alongside the existing default-private behavior. The typed selection is carried through immutable feature plans and exact request serialization, and synthetic tests pin every value for both positive payment and negative request bodies. Current official Venmo privacy guidance states that an individual payment may select Public, Friends, or Private, while Venmo always applies the more restrictive setting selected between payment partners; it also says past payments can only be made more private ([Manage your Venmo privacy settings](https://help.venmo.com/cs/articles/manage-your-venmo-privacy-settings-vhel351), [Changing Payment Privacy & Hiding Past Payments](https://help.venmo.com/cs/articles/changing-payment-privacy-hiding-past-payments-vhel191)). Creation validation therefore requires a supported response audience no more public than requested. Missing, unknown, or more-public values remain ambiguous, but an equal or more-restrictive value is accepted. Output calls the plan value `Requested audience` rather than claiming it is effective.

Four separately approved, non-retried, one-cent payment validations then used unique notes and immediate CLI plus official-app reconciliation. Two friends-selected payments to counterparties with more restrictive settings each settled exactly once as Private. With a compatible counterparty, a friends-selected payment settled exactly once as Friends and a public-selected payment settled exactly once as Public. Every payment reduced available balance by exactly one cent and appeared exactly once in authoritative activity and the official app. The immediate creation responses did not consistently prove the eventual selected audience, including for the reconciled Friends and Public results, so exact selected-audience response equality would falsely report successful writes as ambiguous. The equal-or-more-restrictive response rule remains fail-closed against any response more public than requested while tolerating Venmo's documented partner restriction and observed response timing.

The owner explicitly waived separate live friends/public request mutations after reviewing that payment and request creation share `POST /v1/payments` and the exact typed `audience` field; request creation differs by a negative amount and omission of payment funding fields, all of which remain pinned by synthetic contract tests. This is an owner-approved evidence decision, not a claim that a non-private request was independently live-validated. The earlier private request validation remains the direct production evidence for request semantics.

The controlled decline validation on 2026-07-14 used one separately authorized pending one-cent incoming request. The CLI sent exactly one `deny` update with no retry or field variation, received the exact original request ID and terminal `cancelled` status, and reported that no money was sent. Immediate reconciliation proved the request disappeared from pending results, no new payment activity appeared, and available balance remained unchanged.

The project owner declined mobile-app traffic capture. Do not install a proxy certificate or intercept the official mobile app. Pay and request creation are verified through exact synthetic tests, one separately approved controlled mutation per operation, strict response validation, CLI read reconciliation, and manual official-app confirmation. Acceptance validation included one existing legitimate $25 incoming request despite the disclosed funding-source/fee evidence gap. That exception authorized exactly one confirmed attempt with no retry or field variation and immediate reconciliation; it does not convert absent source/fee evidence into proof. Decline was verified through the separately approved reconciled one-cent `deny` mutation described above.

### 7.4 Establishing verified HTTP request shapes

For this plan, an HTTP request shape means the complete wire contract: HTTPS origin, method, route template, query fields, required header names and authentication model, content type, request-body field names and types, amount units, identifiers, state/version or idempotency fields, expected response statuses and body schema, and the authoritative state transition caused by the operation.

Phase 0 must produce a dated, sanitized contract dossier for every API operation. Recorded behavior from the removed TypeScript implementation and historical documentation are hypotheses, not sufficient evidence. For an operation exposed by the current Venmo web application, the preferred discovery workflow is:

1. Prefer the official Venmo web application in a dedicated visible/headed browser session using `agent-browser --headed`; live web discovery must not run headlessly. If Venmo blocks its security challenge, do not bypass it: close the browser and use only the already authenticated CLI for bounded reads plus manual official-mobile-app identity/result verification, without capturing mobile traffic.
2. Prompt the prompter/project owner to complete Venmo login manually in that visible browser, then pause until they explicitly confirm login is complete. The agent must not request, read, type, or store the username, password, MFA code, or other login secret and must not inspect or capture login traffic.
3. Verify the intended active account after login, then clear the tracked request log. Record the relevant read-only pre-state immediately before the one action and capture only `fetch`/XHR traffic associated with it.
4. Read-only exploration may exercise account, user/friend search, payment-method, balance, activity, and request listing/detail views. For any mutation, obtain explicit human approval immediately before the action and follow Section 13.6. Request acceptance and decline must each use a separate fresh $0.01 incoming request from the approved test counterparty.
5. Use `agent-browser network requests` to identify candidate calls and `agent-browser network request <NETWORK_REQUEST_ID>` to inspect the selected call's method, route, headers, body, response, and status. The network detail command can expose full secrets and bodies, so its raw output must pass through a purpose-built local structural redactor before it reaches agent/model output, logs, terminal history, or any saved artifact.
6. Keep `Authorization`, cookies, CSRF values, device IDs, account/user/request/payment IDs, notes, and all unrelated response values out of the dossier. Preserve only header names, field names, types, safe enums, amount-unit evidence, route templates, placeholder relationships, statuses, and other facts required to implement the contract.
7. Do not export a HAR by default. A HAR may be used only when request correlation cannot otherwise be completed, must live outside the repository in an approved temporary location, must never be emitted into agent context, and must be destroyed after a sanitized structural record is produced.
8. Record the authoritative post-state in both the official web application and the verified read endpoints. Correlate the mutation's returned IDs with the exact request/activity record rather than assuming the visually adjacent network call caused the transition.
9. Determine whether the observed web call uses the same bearer-token/device-ID API family intended for the CLI. A cookie/CSRF-only browser endpoint, browser backend-for-frontend route, or GraphQL call is a useful lead but is not automatically a CLI contract. The CLI must not start storing browser cookies or silently change the authentication model to imitate it.
10. Convert the sanitized evidence into typed request/response DTOs and exact mock-server assertions before invoking the endpoint from Rust. Then validate the candidate Rust call once under the controlled live protocol and reconcile it before any further mutation.

Each contract dossier must record:

- Observation date and the identifiable official web-client build/version when available.
- Evidence source and whether it is direct observation, existing-client behavior, or a historical lead.
- Method, fixed origin, route template, content type, query schema, and required header names with all values redacted.
- Request-body schema, field provenance, amount/currency units, request/funding identifiers, and any conditional-state or idempotency field.
- Response status classes, sanitized structural schemas, durable result identifiers, and operation-specific success evidence.
- Pre-state and post-state invariants, including recipient/request ownership, amount, status, funding result, and audience where applicable.
- Known confirmed-rejection shapes and which transport/server outcomes remain ambiguous.
- The corresponding sanitized fixture and HTTP contract tests.

Never discover a write contract by blind endpoint/body fuzzing, replaying a captured financial request, repeatedly varying fields against the live service, copying a request as cURL with credentials, or substituting an ordinary payment for request acceptance. If the official web application does not expose the operation, the captured call uses an incompatible authentication model, or the shape and state transition cannot be verified safely, stop and bring the capability back to the prompter/project owner as a scope blocker.

### 7.5 Discovery rules

- Validate candidate endpoints only with an account the developer is authorized to use.
- Use only the controlled live-mutation protocol in Section 13.6 for payment, request, acceptance, and decline discovery. Every live amount is exactly **$0.01**, and the only approved counterparty is the test counterparty configured outside the repository after the account owner authorizes the test session.
- Reconcile every discovery write through the official app and activity/request reads before another attempt.
- Capture the minimum sanitized fixtures necessary for contract tests.
- Remove access tokens, device IDs, user IDs, names, notes, payment IDs, account fragments, and other personal data from fixtures.
- Never commit raw discovery responses.
- Do not inspect or repurpose historical local response caches without first establishing a safe redaction workflow.
- A count such as `outgoing_requests_count` does not satisfy `requests list`; actual records are required.
- `requests accept`, `requests decline`, and `requests cancel` require a fresh authoritative request record; a feed summary, count, or guessed association is insufficient.
- If pending-request records or their acceptance contract cannot be retrieved reliably, treat that as a release-scope blocker requiring an explicit product decision rather than fabricating results or sending a substitute payment.

### 7.6 Feature-facing API ports

Feature interfaces express product operations rather than arbitrary HTTP paths. Each feature owns
the narrow ports it consumes under `src/features/<feature>/ports.rs`; there is no monolithic
service or shared ports module:

- Authentication owns `CurrentAccountApi` and `PasswordLoginApi`.
- Wallet, people, activity, requests, and payments own their endpoint-specific read or
  write capabilities.
- Page requests store `Limit` plus their endpoint-specific validated continuation (`Offset`,
  `ActivityBeforeId`, or `RequestsBefore`) directly.
- Request creation, payment creation, request acceptance, and request decline return distinct
  feature-owned result values.

`adapters::venmo::VenmoApiClient` implements several of these traits, while every feature use case
and fake requires only the capabilities it consumes. A future `ReadOnlyVenmoApi` production façade
for MCP should forward only read traits and keep its inner client private. The MCP adapter should
call the same feature use cases; do not add a second HTTP client or collapse the return-position
`impl Future` traits into one large dynamic service solely for protocol convenience. Pure model
and credential-store work stays synchronous where appropriate.

## 8. Architecture

Use one Cargo package with a shared library target and a thin terminal binary. The feature and
adapter layout is:

```text
Cargo.toml
Cargo.lock
rust-toolchain.toml
src/
  lib.rs
  main.rs                 # venmo terminal entry point
  model.rs                # curated frontend-neutral public model facade
  cli/
    mod.rs                # narrow public terminal facade
  adapters/
    cli/
      args/
      dispatch/
      output/
      error.rs
      logging.rs
      prompt.rs
    credentials/
      codec.rs
      keyring/
    system/
    venmo/
      client/
      dto/
      transport/
  features/
    activity/
    auth/
    payments/
    people/
    requests/
    wallet/
  shared/
    account.rs
    credential.rs
    money.rs
    note.rs
    pagination.rs
    user_id.rs
    username.rs
tests/
  cli.rs
  domain.rs
  errors.rs
  process.rs
```

The `venmo` binary contains process composition only. If the planned MCP entry point is added, it
must be explicitly declared without making existing `cargo run -- ...` commands ambiguous, and its
reusable behavior must live in the library.

### 8.1 Layer boundaries

#### CLI adapter

- Defines the `clap` command schema.
- Converts raw values into typed feature input.
- Handles `std::io::IsTerminal` checks, the narrowly allowed prompts, result rendering, and exit codes.
- Owns actual-process terminal detection; public callers cannot inject synthetic terminal state.
  Private test seams may supply terminal snapshots only with fake executors.
- Resolves service-free preconditions before delegated production logging and service composition.
- Does not construct HTTP payloads or contain financial policy.

#### MCP

- Defines the local stdio server, exact tool registry, protocol schemas, tool annotations, and explicit MCP view DTOs.
- Converts protocol input into existing validated feature/shared models and invokes the same feature use cases as CLI dispatch.
- Converts typed feature results into structured protocol output without parsing or reusing terminal rendering.
- Owns no Venmo business rule, private HTTP DTO, credential mutation, retry loop, or financial authorization policy.
- Treats stdout as an exclusive protocol channel and all Venmo-returned names, notes, and labels as untrusted external data.

#### Features

- Orchestrates each command.
- Loads credentials only when required.
- Resolves recipients and funding sources before any applicable money-sending confirmation.
- Depends on narrow API, credential, prompt, clock, and ID interfaces.
- Returns typed results rather than printing directly.
- Requires only `CredentialReader` for every read-only use case. Credential mutation remains confined to explicitly mutating terminal authentication operations.
- Supports static/generic composition over the existing return-position `impl Future` ports; do not replace the narrow ports with one large object-safe service trait merely for MCP.

#### Feature and shared models

- Contains validated value types and financial-operation policy.
- Has no dependency on `clap`, `reqwest`, `keyring`, Tokio, or terminal rendering.
- Represents money as checked integer cents, never binary floating point.

#### Service adapters

- Implements private HTTP endpoints and DTO mapping.
- Implements the sole native keyring adapter.
- Provides a read-only native credential capability for MCP without save/delete methods.
- Initializes redacted tracing.
- Contains platform-specific behavior behind narrow interfaces.

### 8.2 Core feature and shared types

- `AccessToken`: secret value with redacted formatting.
- `CredentialEnvelope`: versioned stored session.
- `Account`, `User`, `UserId`, and `Username`.
- `RecipientInput`: exact username with an optional leading `@`.
- `Money`: positive checked integer cents.
- `PaymentMethod` and `PaymentMethodId`.
- `Balance`: available amount and optional verified held amount.
- `Activity` and `ActivityId`.
- `RequestRecord`, `RequestAction`, `RequestId`, and `RequestDirection`.
- `CreatedPayment` and request-owned `CreatedRequest`.
- `PayPlan`, `CreateRequestPlan`, `AcceptRequestPlan`, and `DeclineRequestPlan`.
- `PayResult`, `RequestCreateResult`, `AcceptResult`, and `DeclineResult`; ambiguous outcomes remain typed failures rather than success models.
- `Limit`: positive server page size bounded to 50, and `Offset`: a nonnegative `u32`.
- `ActivityBeforeId` and `RequestsBefore`: endpoint-scoped, bounded continuation inputs with redacted formatting, plus internal endpoint page request/result types.

### 8.3 MCP runtime and service composition

The stdio server is long-lived, unlike the terminal dispatcher:

- Start one multi-thread Tokio runtime with only the additional features required by the reviewed MCP stdio transport, synchronization, and server tasks.
- Construct one shared production `ReadOnlyVenmoApi` façade backed privately by `VenmoApiClient`; constructing either must not contact Venmo. Share concrete generic state with `Arc` where required rather than introducing a global singleton or exposing mutating client capabilities to MCP.
- Keep a one-permit `tokio::sync::Semaphore` in server state. After schema/model validation, use non-waiting acquisition before rate admission or any keychain/Venmo operation; if the permit is unavailable, return `server_busy` immediately without consuming rate budget. This prevents unbounded pending calls, concurrent OS approval prompts, credential races, private-API rate bursts, and overlapping future financial operations.
- Do not hold a standard mutex across `.await`, spawn detached retries, or let cancellation launch a replacement operation.
- Reload credentials inside each permitted invocation. If synchronous native keychain access proves disruptive, load it through `spawn_blocking` and add narrow feature seams that accept a loaded credential; do not make all model or API ports async merely to accommodate keyring I/O. Move the owned operation permit into the blocking task and return it with the loaded credential, so cancellation of the awaiting MCP request cannot release the permit while keychain work continues. A dedicated serialized credential worker is the fallback if the SDK/runtime cannot preserve that lifetime.
- Verify production server state is `Send + Sync`. Cancellation of an MCP request must stop further local work where possible, while accurately acknowledging that synchronous keychain work or an already-transmitted HTTP operation may not be cancellable.

### 8.4 MCP serialization boundary

MCP has explicit input and output DTOs deriving only the serialization and JSON Schema traits required by the reviewed SDK. Keep these types in the private MCP frontend adapter; do not add broad serialization derives to feature/shared or credential types for convenience.

Never serialize or schema-expose:

- `AccessToken`, `DeviceId`, `CredentialEnvelope`, or `LoadedCredential`.
- `LoginIdentifier`, `AccountPassword`, `OtpCode`, `OtpSecret`, or password-login state.
- Raw HTTP request/response DTOs, headers, bodies, URLs, keychain source errors, or transport internals.

Explicit MCP views may contain validated account/user identities, safe payment-method metadata, exact balance strings, structured activities, pending requests, diagnostic checks, and endpoint-native continuation values. Arbitrary MCP-provided IDs must receive a defensible byte bound and existing whitespace/control/invisible-character validation before network access; current continuation inputs retain their strict 1,024-byte redacted boundary.

## 9. Dependency plan

Select current compatible versions during scaffolding, minimize features, commit the lockfile, and record the MSRV.

### 9.1 Library-first policy

Use an established community crate by default for non-feature functionality that is common across Rust applications. Do not build project-owned substitutes for capabilities such as argument parsing, hidden prompts, HTTP/TLS, URL handling, serialization, native credential storage, terminal capability detection, logging, manpage generation, secure temporary files, date/time handling, signal handling, backoff primitives, or test servers when a suitable mature crate exists.

This policy is not a mandate to maximize dependency count. Keep Venmo-specific feature/model rules, financial-safety decisions, typed adapter glue, and small straightforward transformations in this repository. A new dependency must still reduce total implementation and maintenance risk compared with the local code it replaces.

For each material dependency:

- Prefer a crate with a multi-year release history, substantial real-world adoption, active non-archived maintenance, responsive issue/security handling, and more than one meaningful community signal such as downloads, reverse dependencies, active maintainers, downstream use, or inclusion in the Rust ecosystem's standard tooling.
- Review its latest stable release, recent release cadence, unresolved critical issues, RustSec advisories, license, MSRV, declared target support, transitive dependency graph, default features, unsafe usage, and maintenance ownership.
- Verify that its public behavior matches this CLI's security and correctness requirements; popularity alone is never sufficient.
- Disable unnecessary features, wrap infrastructure crates behind narrow project interfaces where that improves testability or limits lock-in, and commit the resolved version in `Cargo.lock`.
- Recheck maintenance, advisories, and platform behavior before each release and use reviewed automated dependency updates.

Rolling a common component locally despite an appropriate crate requires an explicit recorded exception stating the candidates considered, why each is unsuitable, the maintenance and security burden accepted, and the tests and review that compensate for it. Never implement custom cryptography, TLS, secret storage, password masking, or an HTTP protocol stack.

### 9.2 Planned dependencies

| Concern | Planned crate | Decision |
| --- | --- | --- |
| CLI | `clap` | Confirmed; derive API |
| Manpages | `clap_mangen` | Generate release documentation |
| Runtime | `tokio` 1.52 | CLI keeps its current-thread runtime; MCP additionally needs reviewed stdio I/O, sync, and multi-thread runtime features |
| HTTP/TLS | `reqwest` 0.13.4 with `rustls` | Confirmed; defaults disabled and only JSON plus rustls enabled |
| Serialization | `serde`, `serde_json` | Endpoint DTOs separate from feature/shared models |
| MCP protocol | Official `rmcp` crate | Planned; server/macros/stdio transport only after published-version/MSRV review |
| MCP schemas | `schemars` through the SDK-compatible version | Explicit MCP input/output JSON Schema only; never credential or feature/shared secrets |
| Terminal tables | `tabled` 0.21.0 | Confirmed; defaults disabled and only `std` enabled; all cells sanitized before rendering |
| Errors | `thiserror` | Typed categories and source chains |
| Timestamp instants and storage | `time` | Typed exact instants and RFC 3339 persistence/wire parsing remain independent of display zones |
| System-time-zone display | `jiff` 0.2.32 | Cross-platform system-zone discovery and historical rules; local whole-second CLI output, with explicit UTC fallback and `Z` only for UTC display |
| Persistent secrets | `keyring` default `v1` API | Confirmed sole backend |
| Secret memory handling | Redacted newtype; evaluate `zeroize` | Not a persistence backend |
| IDs | Validated model newtypes | Device IDs are imported or reused, never generated by the CLI |
| Logging | `tracing`, `tracing-subscriber` | Stderr only with explicit redaction |
| Prompts | `dialoguer` | Confirmed; separate from `clap`, behind a prompt adapter |
| Binary tests | `assert_cmd`, `predicates` | End-to-end process assertions |
| HTTP tests | `wiremock` | Request, response, timeout, and retry contracts |
| Property tests | `proptest` | Money and parser invariants |

Dependency acceptance criteria:

- Maintained and broadly used, or sufficiently small to audit.
- Compatible with the MSRV and all declared targets.
- License-compatible.
- No unnecessary default features.
- Passes `cargo deny` and advisory policy.
- Does not require sidecar runtime assets.

### 9.3 `dialoguer` due diligence

`dialoguer` selection due diligence, verified on 2026-07-09 from crates.io, docs.rs, and the upstream `console-rs/dialoguer` repository:

- First published in May 2017 and maintained in the `console-rs` organization under the MIT license.
- Latest stable release at review time: `0.12.0`, published 2025-08-23.
- crates.io reported 69,171,504 total downloads, 14,924,195 recent downloads, and 2,200 reverse-dependency entries.
- Provides exactly the required `Password`, `Confirm`, and `Select` prompts, renders interactions on stderr, supports explicit terminal targeting, and provides cancellation-aware confirm/select APIs.
- The password feature uses `zeroize`; token handling must still minimize copies and move the returned value promptly into the redacted secret type.

Plan to declare it with default features disabled and only the `password` feature enabled. Recheck the current release, advisories, feature graph, MSRV, and supported-terminal behavior when creating `Cargo.toml` and before releases.

### 9.4 `reqwest` due diligence

`reqwest` selection was rechecked on 2026-07-10 against crates.io, docs.rs, and the active `seanmonstar/reqwest` repository:

- The project dates to 2016, had 125 published releases, and version `0.13.4` was the current stable release.
- The active upstream repository had more than 11,000 stars, more than 1,300 forks, and contributions from a broad community.
- Ecosystem metadata reported roughly 49 million downloads per month and use by more than 38,000 crates.
- Version `0.13.4` is MIT OR Apache-2.0, declares Rust 1.85, supports async streaming bodies, configurable redirects/proxies/decompression/timeouts, and uses rustls without OpenSSL when selected as configured here.
- Reqwest 0.13 can retry low-level protocol NACKs by default, so this project explicitly installs `reqwest::retry::never()` and implements any future bounded safe-read retry above the shared transport. This prevents an HTTP-client default from replaying a financial write.

The implementation disables default features and enables only `json` and `rustls`. Recheck the release, advisory status, TLS provider/platform verifier, feature graph, and target behavior before release.

### 9.5 `tabled` due diligence

`tabled` was selected on 2026-07-11 after comparing maintained Rust terminal-table crates:

- The project was first published in 2020, remains actively developed in the non-archived `zhiburt/tabled` repository, and has substantial ecosystem adoption and more than 2,300 GitHub stars.
- Version `0.21.0`, published 2026-05-31, was the current stable crates.io release and is MIT-licensed.
- It provides a dynamic builder and plain-text styles without requiring project-owned column-width, alignment, or Unicode-width logic.
- This project disables default features and enables only `std`; it does not enable derive macros, table assertion helpers, or ANSI handling. Every untrusted cell is sanitized before it reaches `tabled`.
- Because `tabled` is pre-1.0 and documents that minor releases may break APIs, keep the resolved release committed in `Cargo.lock`, review upgrades rather than applying them blindly, and protect human output with tests.

Recheck maintenance ownership, advisories, license, MSRV, feature graph, and supported-target behavior before release.

### 9.6 Official Rust MCP SDK due diligence

Research on 2026-07-12 selected the official [`modelcontextprotocol/rust-sdk`](https://github.com/modelcontextprotocol/rust-sdk) project and its `rmcp` crate as the implementation candidate rather than a project-owned JSON-RPC/MCP stack. The reviewed upstream workspace identified itself as version `2.2.0`, used Rust edition 2024, was Tokio-based, and carried Apache-2.0 licensing. Before changing `Cargo.toml`, revalidate the exact published stable crate, release provenance, declared MSRV, documentation, advisories, feature graph, transitive dependencies, and compatibility with this package's pinned Rust 1.95 toolchain and cargo-deny policy; do not infer those properties solely from the upstream main branch.

Initial feature policy:

- Disable default features.
- Enable only the features required for an MCP server, its macros/schema generation, and local stdio transport—currently expected to be `server`, `macros`, and `transport-io` or their reviewed published equivalents.
- Do not enable Streamable HTTP/SSE, OAuth, remote-server authentication, MCP client behavior, sampling, elicitation, resources, prompts, tasks, or unrelated transports.
- Use the SDK-compatible `schemars` version for JSON Schema 2020-12 input/output contracts; avoid a second conflicting schema version.
- Preserve `serde_json` as the explicit structured-output representation and do not introduce a second general JSON model.
- Add no sidecar runtime, Node dependency, browser component, generated protocol binary, or network listener.
- Audit the published stdio transport's buffering and logging before acceptance. The production adapter must enforce the 64 KiB inbound frame bound and 2 MiB serialized-result bound from Section 3.9; an SDK path with unbounded line buffering is not acceptable without a bounded wrapper.
- Disable SDK/transport frame and payload tracing targets in production at every verbosity. A malformed JSON-RPC line, schema failure, or protocol error must never cause raw input/output to be written to stderr.

The implementation targets the [MCP specification dated 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25) unless the reviewed stable SDK requires a newer compatible revision. Record any spec-version change and rerun exact schema/protocol snapshots before release.

## 10. HTTP and resilience policy

- The production base is fixed in code to `https://api.venmo.com/v1/`; alternate HTTP origins exist only inside loopback tests.
- Use a 10-second connection timeout, 15-second per-read timeout, and 30-second total request deadline. Revisit only with endpoint evidence and tests.
- Bound every response body to 2 MiB while streaming it; reject an oversized declared or observed body before unbounded buffering.
- Authenticate only endpoints that require it.
- Send token and device ID only in the required headers.
- The selected API family is the bearer-token/device-ID mobile private API, so do not send a browser User-Agent. Send the current iOS-compatible profile pinned by current native/client evidence: `Accept: application/json; charset=utf-8`, `Accept-Language: en-US;q=1.0`, `Venmo/26.13.0 (iPhone; iOS 18.6.2; Scale/3.0)`, and one UUID-derived unsigned-decimal `X-Session-ID` per transport instance. Controlled live diagnostics proved this profile reaches and completes the recognized payment step-up where the old profile repeatedly produced code `10100`; later rapid current-profile attempts also produced `10100`, so treat it as a server-side-check block, not as a header fingerprint, and do not claim which check or individual header is responsible.
- Build endpoint URLs from validated path segments and query pairs; never accept an arbitrary authenticated URL.
- Disable redirects, automatic referer generation, system/environment proxies, and gzip/Brotli/Zstandard/deflate decompression in the first release. A future proxy feature requires an explicit product/security decision because it exposes authenticated traffic to the configured proxy.
- Use rustls with reqwest's native platform verifier. Do not enable native-tls or OpenSSL.
- Disable every reqwest-layer retry, including protocol-NACK retries. A separately tested endpoint-aware layer may retry only verified idempotent reads.
- In `--debug` mode, centrally log a static command name plus route template, method, status,
  duration, retry count, response byte count/type, sanitized API code, and bounded single-line root
  error title/message or first GraphQL error. Known credential values reflected by Venmo are
  replaced before logging.
- Never log authorization values, device IDs, passwords, OTP material, dynamic path/query values,
  command arguments, funding-source IDs, usernames, notes, amounts, raw bodies, headers, or
  credential payloads. Venmo-provided error text is untrusted remote account context; bound and
  terminal-sanitize it and tell operators to review it before sharing.
- Filter out every third-party tracing target; `--debug` enables only first-party `venmo_cli`
  events and must never turn on reqwest, Hyper, Rustls, SDK, or protocol-frame diagnostics.

### 10.1 Pagination policy

Pagination applies to `friends list`, `users search`, `activity list`, and `requests list`. Single-resource commands and `pay options` are not paginated unless Phase 0 proves that their verified endpoint requires it.

User-visible contract:

- Every public invocation fetches exactly one source API page. `--limit <N>` is that server request's page size, defaults to 10, and has a hard maximum of 50.
- Friends and user search accept only a typed nonnegative `u32` `--offset`, defaulting to 0. Activity accepts only its endpoint-native `--before-id <TOKEN>`; pending requests accept only their endpoint-native `--before <TOKEN>`.
- When a validated continuation remains, successful records stay on stdout and exactly one copyable native value is written to stderr after record rendering succeeds: `Next offset: <N>`, `Next before-id: <TOKEN>`, or `Next before: <TOKEN>`. Raw next URLs are never output.
- There is no universal continuation input, page number, page-size alias, or public multi-page collector. Payment methods and single-resource reads remain unchanged.
- `ActivityBeforeId` and `RequestsBefore` are nonempty, bounded to 1,024 bytes, reject whitespace, control, and invisible format-control characters, and redact `Debug`, `Display`, and parse errors. The CLI output adapter has narrow crate-private access solely to print a successful copyable next value.
- Native continuations are best-effort endpoint state, not snapshots. Additions, removals, or reordering between invocations can produce the private API's normal gaps or duplicates.

Public page and server-continuation contract:

- Preserve exact source order. Fully buffer and validate the one page before producing a command result. Reject a page larger than requested, deduplicate identical canonical IDs while preserving first occurrence, and fail on conflicting duplicates.
- Friends parse the server next link only after fixed-origin, exact-route, `limit` equality, and `limit`/`offset` query-allowlist checks. The next offset must be greater than the requested offset.
- User search has no observed server next link. A full nonempty page synthesizes `offset + returned_count` with checked `u32` arithmetic; this can report one provisional final offset whose subsequent page is empty.
- Activity validates the fixed story route and its exact continuation query allowlist before extracting `before_id`. Pending requests likewise validate the fixed payments route, action/status filters, limit, and query allowlist before extracting `before`.
- A next opaque token equal to the supplied token is a no-progress contract failure. Empty source pages with a continuation are rejected for friends, user search, and activity. Pending-request pages may render empty after local filtering, and an empty source page with a valid progressing continuation is tolerated rather than misreported as exhaustion.
- Pending-request direction is local filtering over the fully validated source page. It does not alter the server page size or suppress the source page's continuation, so output may contain fewer than `--limit` matching records.
- Missing `pagination.next` is treated as the end of the current friends, activity, or pending-request page chain. API continuation failures are API-contract errors; invalid user-supplied native values are argument-parsing errors with exit code 2.
- The transport reconstructs every request locally from validated path segments and query pairs. It never follows a response URL directly or forwards credentials to an arbitrary origin.

Internal exact-username traversal:

- Exact username resolution uses a separate, non-public user-search traversal with no user-supplied continuation.
- It requests at most four source pages of at most 50 records, buffers at most 200 unique users, validates increasing offsets and per-page bounds, deduplicates identical IDs, rejects conflicts and no-progress pages, and preserves source order.
- If the traversal reaches its record/page bound, it may authorize only one already-found exact
  username match and only after detail-by-ID returns the same immutable ID and exact username. No
  exact match is username not found, and multiple exact matches remain ambiguous.
- No public list command calls this exhaustive traversal.

For any future paginated endpoint, exact defaults, per-page sizes, hard maxima, ordering, token/offset mapping, and reliable end signaling remain Phase 0 decisions. The selected contracts above remain release-gated where explicitly noted; a missing or unreliable pagination contract blocks claims of completeness.

### 10.2 Retry policy

- The first release performs no automatic retries, including for read-only pagination.
- A network failure, timeout, rejection, malformed response, or continuation failure ends the invocation with its typed error.
- Never retry payment creation, request creation, request acceptance, or request decline automatically. Any future retry proposal requires an explicit endpoint-specific product decision and new safety tests before implementation.

### 10.3 Response handling

Response handling must distinguish:

- Transport failure before a response.
- Timeout or cancellation.
- HTTP failure.
- API-level failure inside an HTTP success.
- Malformed or incompatible response.
- Confirmed financial-write rejection.
- Financial-write outcome unknown.

An identified connection-establishment failure, including DNS/TCP/TLS setup failure, is pre-transmission and retains its normal network/timeout category. If the client cannot prove that any later timed-out, disconnected, or cancelled write was never transmitted, report **outcome unknown**, return exit code `3`, and instruct the user to inspect `activity list`, `requests list`, or the official Venmo application before retrying. Do not assume accepting by request ID is safe to repeat merely because it appears state-bound.

### 10.4 MCP invocation and cancellation policy

- Each MCP tool call executes the same feature use case and exact private-API request policy as its CLI equivalent. MCP is not a second HTTP implementation.
- After strict input validation, acquire the one-operation permit without queuing; return `server_busy` when another operation owns it. Apply rate admission only after acquiring the permit, and release it immediately on `rate_limited`. Hold an admitted permit through credential loading, feature completion, and bounded result conversion.
- Public list tools fetch exactly one source page. Never hide pagination, follow all pages, refill a locally filtered request page, or convert endpoint-native continuations into a generic MCP cursor.
- The server and shared HTTP transport perform zero automatic retries. Tool annotations, host retries, reconnects, and repeated JSON-RPC IDs do not authorize replay.
- A cancellation before network transmission should stop the operation cleanly. Any blocking credential task retains the operation permit until it actually finishes even if its requester is cancelled. Cancellation or transport loss after a future write might have been sent is an ambiguous financial outcome; never report it as a normal cancelled failure or start a replacement call.
- Malformed tool input must fail before credential/network access. A later-page concept does not exist for initial tools because each invocation is exactly one page.
- The stdio connection may remain alive after a feature/tool error. A protocol framing failure, impossible server invariant, or stdout write failure is a fatal server error and must not be hidden behind a successful tool result.

## 11. Financial-write safety model

### 11.1 New payment or request preflight

Before writing, and before the confirmation prompt for `pay user`:

1. Parse one recipient.
2. Resolve that recipient to exactly one Venmo user.
3. Parse the amount into checked integer cents.
4. Validate the non-empty note.
5. Verify a personal, payable, non-self recipient for the exact operation.
6. Read authoritative available Venmo balance and peer-eligible balance/bank/card sources. Without `--source`, choose sufficient balance or the unique default/sole external method; with `--source`, require and preserve that exact eligible source, including full coverage for explicit balance. Never choose by response order or fee.
7. Without `--protect`, preserve the selected external method's fee evidence and blank-source eligibility result without displaying its unbound amount as a payer fee or final total. With `--protect`, require source-bound eligibility, exactly one complete buyer-protection fee, and a seller fee no greater than the payment amount; never fall back to ordinary payment.
8. Preserve whether funding selection was automatic or explicit and the exact submitted source ID.
9. Build an immutable plan carrying the selected requested audience, which defaults to private, and the explicit protected/unprotected state. Protected plans retain the complete selected fee and redact its token.
10. For `pay user`, render sanitized complete payment details including the requested audience,
    selected source, applicable method fee evidence, protection choice, and wallet balance before
    confirmation or immediate `--yes` execution. Protected details also show estimated seller fee
    and recipient proceeds. Do not display the internal automatic/explicit selection marker.

If any preflight step fails, send no write request.

After request-creation preflight, render and flush the complete immutable action summary. Require
default-No confirmation or `--yes` before installing write-interruption protection and executing the
single request-creation write. The flag never skips preflight or details.

### 11.2 Request-acceptance preflight

Before prompting or writing:

1. Parse one canonical request ID.
2. Load the authenticated account identity.
3. Fetch the authoritative request record by ID.
4. Verify that the record is incoming, pending, payable, and addressed to the authenticated account.
5. Require complete, understood requester, amount, note, status, profile type, payability, and ownership fields; never guess missing safety-critical values. Until independently proven otherwise, only exact `pending` is payable and `held` is read-only data.
6. Read authoritative available Venmo balance. Full nonnegative coverage selects the legacy
   balance plan without payment-method or eligibility calls only for default unprotected acceptance with no explicit source.
7. For any shortfall, explicit `--source`, or explicit `--protect`, read unacknowledged notifications and require exactly
   one valid request-action ID whose associated payment ID equals the authoritative request ID.
8. Then read peer funding sources, apply the shared sufficient-balance/requested-source/unique-default-or-sole-external selection
   policy, and request source-bound approval eligibility for the exact requester,
   amount, note, and source. An unavailable explicit source, insufficient explicit balance, denial, missing token, malformed/oversized/unknown fee fields, a
   negative percentage, or checked fee-total overflow sends no write. Valid zero or nonzero fee
   records are normalized; unprotected plans discard them, while protected plans retain them and
   reject a fee exceeding the request amount.
9. Build an immutable acceptance plan tied to the fetched request ID/state, resolved notification
   ID, selected source, and automatic/explicit selection mode; callers cannot supply or override either server-resolved ID.
10. Render the exact balance or external funding plan before default-No confirmation. For
   `--protect`, show the estimated seller deduction and estimated recipient proceeds; never add the
   fee to the payer's amount.

An outgoing, already-settled, cancelled, declined, inaccessible, wrong-account, or malformed request sends no write. The client must use the verified acceptance operation and must not translate the record into an ordinary recipient payment.

The verified server mutation must atomically enforce the request ID's payable state, or expose a conditional token/version that the client sends. A client-side preflight followed by an unconditional unrelated write is not sufficient protection against a concurrent state change. The validated `approve` update is state-bound by request ID, and any state conflict fails without an automatic retry.

### 11.3 Request-decline preflight

Before the decline write:

1. Parse one canonical request ID, load and verify the authenticated account, and fetch authoritative request detail by ID.
2. Require exact incoming `pending` state addressed to that account; outgoing, `held`, terminal, inaccessible, wrong-account, or malformed records send no write.
3. Bind an immutable decline plan to the fetched request ID, parties, amount, note, audience, and observed state.
4. Render and flush a sanitized complete plan stating that this exact request will be declined without money movement.
5. Unless `--yes` was supplied, require both stdin and stderr to be terminals and ask `Decline this request without sending money?` with a default-No confirmation. No, cancellation, or prompt failure sends no write. `--yes` bypasses only this authorization step.
6. Submit exactly one state-bound `deny` mutation with no funding fields. Never substitute outgoing `cancel`, ordinary payment creation, or a notification dismissal.
7. Require the response to preserve every immutable request field and prove the supported terminal `cancelled` server status. Render that server status rather than inventing a remote `declined` value.

`decline` requires explicit default-No confirmation because it is a destructive state change even though it sends no money. Its explicit canonical request ID identifies the state change but does not replace authorization. Possible transmission, interruption, response loss, or an unverified error is an ambiguous state mutation with exit code `3`; never retry it.

### 11.4 Execution rules

Execution rules:

- One invocation creates, accepts, declines, or cancels at most one operation.
- For `pay user` and all four request mutations, prompt only when both stdin and stderr are terminals and default to No.
- For `pay user` and all four request mutations, require `--yes` unless both stdin and stderr are terminals; on a terminal it skips only the final confirmation and never the details display.
- Send exactly one mutation for request creation, request acceptance, request decline, or request cancellation. Payment sends one initial mutation and only an exact confirmed-prewrite SMS challenge may add one same-UUID verified continuation.
- Preserve the original API or transport cause on failure.
- Never claim rollback or retry a write.
- On ambiguous outcome, do not print a false failure or success.
- A Ctrl-C, SIGTERM, or SIGHUP received before the protected write region keeps normal process interruption semantics. Once the financial future may have started, those catchable termination signals drop local waiting and return the ambiguous-write exit code `3`; they never report ordinary cancellation or authorize a retry, and the user must reconcile through reads and the official app. Uncatchable termination such as SIGKILL cannot provide an exit warning, so external supervisors must never retry financial commands automatically.
- On confirmed request-state conflict, report the current understood state and direct the user to refresh `requests list`.

### 11.5 Future MCP financial-write gate

The initial MCP server registers no financial tools. A later financial MCP phase cannot begin until the corresponding CLI feature operations and private-API contracts have satisfied Sections 7, 11, and 13.6. Even then, financial tools remain absent from `tools/list` by default.

Future registration requires all of the following:

1. An explicit non-secret server startup opt-in such as `--allow-financial-writes` that changes the registered tool set for that process.
2. A named, tested, trusted MCP host integration that places a human approval step on each specific execute tool and displays the exact server-resolved immutable plan rather than only the model's original arguments.
3. A two-phase prepare/execute protocol, or an equivalently strong host-attested protocol, that binds the approval to account, resolved recipient/request identity, amount, note, funding source, operation type, and observed request state.
4. Server-side one-operation serialization, no retry, replay/cancellation analysis, and ambiguous-outcome handling equivalent to or stricter than the CLI.

Startup opt-in, standard annotations, a model statement, or a model-supplied field such as `confirmed: true` is never proof of human authorization. Do not add an MCP equivalent of CLI `--yes`. If the chosen local host cannot provide trustworthy per-call human approval, write tools remain unregistered even when startup opt-in is requested.

For a prepare/execute design, the read-only prepare tool resolves and returns the complete immutable plan plus a random process-local opaque handle. The server retains a matching plan for at most five minutes. The execute input repeats every human-visible plan field plus that handle so the host can display the exact values; the server requires byte-for-byte/typed equality with its retained plan, the same authenticated account, unexpired single-use state, and a fresh authoritative preflight. Any changed recipient, amount, note, funding source, request status, or account invalidates the plan and requires a new preparation and approval. The handle is not authorization by itself, is never logged, and becomes invalid on use, timeout, cancellation before execution, or server restart. Do not silently refresh or rewrite a plan after the human approved it.

Every future payment, request-creation, request-acceptance, and request-decline tool must be marked `readOnlyHint=false`, `destructiveHint=true`, `idempotentHint=false`, and `openWorldHint=true`. These hints help trusted hosts display policy and warnings but do not replace capability restriction or approval. The tool executes at most one write except the exact payment SMS challenge's same-UUID verified continuation, and must not be retried by the server after timeout, cancellation, disconnect, protocol reconnect, or unknown outcome. Payment step-up remains unavailable to MCP until a trusted host can provide secret-safe human OTP input without exposing it to the model or protocol transcript.

## 12. Output, errors, and protocol/terminal safety

Typed error categories:

- `Usage`.
- `Cancelled`.
- `Credential`.
- `Network`.
- `Timeout`.
- `Api`.
- `ApiContract`.
- `AmbiguousWrite`.
- `Internal`.

Rendering rules:

- One concise primary error by default.
- Actionable next step when known.
- Bounded command-lifecycle and request metadata only with `--debug`; never print raw source chains.
- No secrets at any verbosity.
- Sanitize all API, user, friend, activity, request, note, payment-method, and keyring-originated strings before human terminal output.
- Sanitize display only; preserve the intended note payload sent to the API.
- Avoid panics for expected input, API, keyring, network, and terminal failures.

### 12.1 MCP output, errors, and untrusted content

- Reserve MCP-process stdout exclusively for SDK-managed JSON-RPC framing. Never write banners, terminal tables, prompt UI, tracing, panic reports, diagnostics, or ad hoc acknowledgements to stdout.
- Send redacted tracing and fatal startup diagnostics only to stderr. Do not log tool arguments, structured results, account/user/payment/request IDs, notes, continuation values, keychain source chains, or protocol payloads.
- Filter SDK and stdio transport logging independently from CLI debug mode so `--debug` can never enable raw JSON-RPC/frame diagnostics.
- Return successful data through explicit structured output conforming to the declared `outputSchema`; provide an equivalent serialized JSON text content item only for MCP client compatibility, not a second independently formatted representation.
- Map feature failures to stable categories such as `invalid_input`, `authentication_required`, `credential_unavailable`, `server_busy`, `rate_limited`, `network`, `timeout`, `remote_rejected`, `remote_contract`, `response_contract`, and `internal`. Return normal feature and validated-input failures as tool execution errors with `isError: true`; malformed JSON-RPC, unknown tools, and invalid protocol shapes remain protocol errors.
- Never expose `Debug`, nested source chains, raw platform errors, raw response text, input values, query text, IDs, notes, or continuation tokens in an error. Messages should be static or category-based and provide a safe remediation where known.
- Treat Venmo notes, names, merchant labels, institution labels, and all other remote text as untrusted external content and potential prompt injection. Keep it only in clearly named structured fields, rely on correct JSON escaping, and never interpolate it into tool descriptions, server instructions, errors, annotations, logs, or authorization decisions.
- MCP hosts may persist tool inputs and results in model context, transcripts, telemetry, or cloud logs. Documentation must prominently warn that read-only tools expose private account, financial, social, and transaction data even though they do not mutate Venmo.
- Standard annotations describe side effects and open-world behavior but provide no universal sensitive-data classification and are not enforceable permissions. The server's static tool registry, startup policy, type-level credential capabilities, and host approval configuration are the actual controls.

## 13. Testing strategy

### 13.1 Model and policy tests

- Amount syntax, bounds, formatting, and checked arithmetic.
- Recipient syntax and exact username matching.
- Automatic funding-source validation, deterministic unique-default/sole-method policy, and fail-closed ambiguity independent of response order and fee.
- Pay, request-creation, request-acceptance, and request-decline plan invariants.
- Request-state transition and authenticated-target invariants.
- Native page-size and offset bounds, strict redacted before-token parsing, duplicate/conflict handling, and continuation no-progress detection.
- Terminal sanitization.
- Error-to-exit-code mapping.
- Property tests for money parse/format round trips.

### 13.2 `clap` tests

Use `Cli::try_parse_from` and help snapshots to cover:

- Every command and subcommand.
- Required recipient, amount, and note.
- Grouped `requests create`/`accept`/`decline`/`cancel`, rejection of the removed top-level forms, and the valid `accept` or `@accept` recipient after `requests create`.
- `--from` rejected by `pay user` and every other command; `--source` accepted only by `pay user`
  and `requests accept` with a typed payment-method ID; `--yes` accepted by `pay user`,
  `requests create`, `requests accept`, `requests decline`, `requests cancel`, `friends add`, and
  `friends remove`, and rejected by `pay options`, `friends list`, and request reads.
- `--dry-run` rejected by every command.
- Request acceptance, decline, and cancellation each require one request ID and reject recipient, amount, note, and unsupported arguments; only acceptance accepts request-mutation `--source` and `--protect`. The separate `pay user` leaf accepts its own `--source` and `--protect` options.
- Absence of compatibility aliases for the removed top-level `request`, `accept`, and `decline` forms.
- Direction enum.
- Limit bounds.
- Exact endpoint-native pagination grammar, default limit/offset values, rejection of cross-endpoint or page/page-size forms, and redacted before-token debug/error formatting.
- Absence of old `init`, `deinit`, `charge`, split, token-source, and multi-recipient forms.
- Help/version paths that never initialize keyring or HTTP dependencies.
- Compile/API coverage that the public CLI facade exports no terminal-capability injection and that
  production dispatch/runtime fallback accept no caller terminal policy.
- Injected logging-initializer coverage proving noninteractive authentication makes zero
  logging/service calls; delegated fake commands preserve logging errors and initialize before
  execution.

### 13.3 Feature tests with fakes

- Login is interactive and hidden-prompt only.
- Login fails before credential/network access when noninteractive and always prompts for a newly supplied trusted device ID rather than reusing stored device identity.
- Direct password success performs exactly one password-login initiation and no device-trust call; the exact recognized challenge alone enables the one SMS-OTP path and its post-save best-effort trust call.
- Password/OTP failure, current-account validation failure, and invalid prompt input leave the old credential untouched; every save/read-back uncertainty is reported as unknown and prevents device trust.
- An issued token that fails validation is never saved.
- A valid replacement preserves the old credential until validation succeeds.
- Corrupt credentials can be replaced and deleted.
- Auth status identifies the active account.
- Friends/search output exposes copyable recipient identifiers.
- Friendship mutation preparation applies the complete native state matrix, fails closed on
  self/non-personal/missing or unknown relationship evidence, and selects exactly send, accept,
  unfriend, or outgoing-request cancellation.
- Friendship details precede action-specific default-No confirmation; `--yes` skips only the
  prompt. Cancellation, noninteractive rejection, details-output failure, and preparation failure
  consume no mutation response.
- Each authorized friendship operation consumes exactly one mutation response, never retries, and
  requires the exact target plus expected authoritative reconciled relationship before success.
- Every public paginated feature flow makes exactly one source request, uses `--limit` as its page size, validates and buffers the page before output, preserves source continuation after local request-direction filtering, and rejects oversized/conflicting/no-progress pages.
- The separate internal exact-recipient traversal remains limited to four 50-record pages/200 unique users and accepts no public continuation. Exhaustion is required for normal not-found; one exact match found at the bound still requires matching authoritative detail-by-ID before use.
- Balance does not infer unsupported values.
- Funding is displayed in details before confirmation or `--yes` execution: `pay user` shows its exact selected balance or external source; `accept` shows either the legacy available-balance plan or its modern selected source and applicable external method-level fee evidence. The internal automatic/explicit selection marker is not displayed. Explicit payment or acceptance `--protect` additionally shows the estimated seller deduction and recipient proceeds.
- Cancelled `pay user`, request creation, acceptance, decline, and outgoing-cancellation prompts and all preflight failures send zero writes.
- `pay user` and all four request mutations prompt default-No on a TTY and require `--yes` off a TTY.
- An exact prewrite payment code-1396 challenge may issue and verify one hidden SMS OTP, then consume one challenge-verified payment response with the same plan and UUID; every other branch consumes no second payment response, and a repeated challenge stops.
- The `dialoguer` adapter sends prompts to stderr, never echoes the bearer token, maps default Enter to No, and distinguishes cancellation from terminal failure.
- First write failure preserves its source.
- Ambiguous writes are never retried.
- Newly created requests carry no funding-source ID.
- Request acceptance uses the exact server-side requester, amount, note, and request ID rather than caller-supplied substitutes.
- Only an incoming, pending request addressed to the active account can reach the acceptance write.
- Outgoing, stale, settled, cancelled, declined, wrong-account, incomplete, and malformed request records send zero writes.
- Unprotected request acceptance preserves the legacy balance-covered route only without `--source`; every shortfall or explicit source uses exact shared source selection plus source-bound eligibility before confirmation while omitting returned fee records. Explicit `--protect` forces the modern route, checks/displays/submits normalized fee records, and rejects malformed, overflowing, or greater-than-request fees before any write.
- Request decline accepts only authoritative incoming exact-`pending` records, performs one `deny` write, and never sends funding fields or money.
- Outgoing-request `cancel` and local-only dismissal semantics cannot satisfy decline.
- A request-state conflict is reported as a confirmed failure; an uncertain transport outcome is classified as ambiguous and is not retried.
- Confirmed acceptance output is tied to the requested ID and does not overstate settlement when the server reports only pending/unknown state.

### 13.4 HTTP contract tests

For every verified operation, test:

- The exact method, fixed origin, path, query, content type, required header names, and body schema recorded in the sanitized contract dossier.
- Bearer and device headers.
- Exact native limit/offset/before queries, fixed-origin and fixed-route next-link validation, strict query allowlists, and local request reconstruction.
- One-page exact-limit boundaries, empty/final pages, local direction filtering, duplicate IDs, conflicting duplicates, repeated/no-progress values, malformed next links, user-search offset overflow, and source continuation preservation.
- A malformed source page emits no command result, and an output failure emits no continuation notice.
- Known response-envelope variants.
- Empty, non-JSON, malformed, and oversized responses.
- HTTP and API-level errors.
- Control characters in remote strings.
- Connect/read/total timeouts.
- Read retry count and `Retry-After` bounds.
- Zero retries for payment creation, request creation, request acceptance, request decline, friend
  add/accept, and friend remove/cancel.
- Ambiguous write classification.
- Exact friendship form POST and bodyless DELETE contracts, both signer-evidenced successful POST
  envelopes, permissive successful DELETE body handling followed by mandatory P2 reconciliation,
  and every post-transmission state-write failure class.

Friends list/mutations, balance, activity, activity detail, pending requests, request detail,
request acceptance, and request decline require dedicated sanitized fixtures before their commands
are considered implemented.

### 13.5 Native and packaged tests

- Keyring read/write/delete smoke tests on each supported platform using a test-only service name.
- Legacy `@napi-rs/keyring` migration test where feasible.
- Pseudo-terminal prompt smoke tests for hidden authentication input, default-No confirmation, explicit yes/no, cancellation, EOF, and stderr-only rendering.
- Compiled-binary help, version, exit code, stdout/stderr, and redaction tests.
- Manpage generation.
- Release archive contents and execution after unpacking.
- Isolated second-executable Keychain/Secret Service smoke tests using a randomized test-only service/account; never production `venmo-cli` / `default` credentials during automated validation.
- Packaged `venmo-mcp` initialize and `tools/list` smoke tests with no keychain or network access.
- No live financial or relationship writes in automated validation.

### 13.6 Controlled live-mutation protocol

Automated tests use fakes and mock HTTP servers; their synthetic amounts are not constrained by this section. Any test against the live Venmo service remains read-only by default. A live financial mutation is a manual, explicitly gated exception and must follow all of these rules:

- Prefer the Venmo web application with `agent-browser --headed`. Prompt the prompter/project owner to log in manually in the visible browser and wait for their confirmation before testing. Never ask them to send login credentials through chat, automate credential/MFA entry, inspect login traffic, or persist the authenticated browser state in the repository. If Venmo blocks the security challenge, do not bypass it; close the browser and use bounded authenticated CLI reads plus manual official-mobile-app verification without capturing mobile traffic.
- After login, read-only tests may inspect the active account and exercise user/friend search, payment-method, balance, activity, and request list/detail workflows. Clear any network capture only after login is complete and keep all observed data under the redaction rules in Section 7.4.
- Use exactly **$0.01 USD** per live payment or request—never a larger amount. If Venmo rejects one cent or requires a higher minimum, stop and report the capability as blocked; never increase the amount to make the test pass.
- Send test money only to the owner-approved test counterparty configured outside the repository and create test requests only from that counterparty, and do either only after the owner of that account has explicitly authorized the current test session.
- Resolve the approved test counterparty to one exact account before each session and verify the displayed username, display identity, and immutable user ID against the official Venmo application. Keep the expected handle and ID only in ignored local test configuration; never commit either value or any returned user record.
- Test sending with one $0.01 payment to the approved test counterparty.
- Test receiving by creating one $0.01 request from the approved test counterparty; any fulfillment by the counterparty must also be exactly $0.01.
- Test request acceptance only from a fresh $0.01 incoming request created by the approved test counterparty for the authenticated test account.
- Only after acceptance has been fully reconciled, test request decline with a different fresh $0.01 incoming request created by the approved test counterparty. Confirm that no money moved and that the request reached the expected terminal state in CLI reads and the official app.
- Use a unique, non-sensitive test note so identical one-cent operations can be reconciled, but never commit the note or raw response.
- Run one mutation at a time. Require an explicit human go-ahead immediately before invocation and never fuzz, loop, schedule, or batch-probe a financial route. Prefer the default-No terminal prompt. When the trusted execution harness has no TTY, `--yes` may carry only the immediately preceding human approval for those exact rendered details; it is never standing, unattended, inferred, or reusable authorization.
- After each mutation, verify the counterparty, amount, request/payment ID, status, audience, and funding result through the CLI reads and official Venmo application before continuing.
- If any result is ambiguous, stop immediately. Do not retry or begin another mutation until the prior outcome is reconciled independently.
- Never upload or commit tokens, device IDs, user IDs, notes, payment/request IDs, raw responses, screenshots containing account data, or live-test logs.

A friendship canary is independently gated and does not inherit approval for a financial test. It
requires a consenting target initially proven `not_friend`, one explicitly approved `friends add`,
reconciliation to `request_sent_by_you` in authoritative state and the official app, then separate
approval for one `friends remove` to revoke that outgoing request and reconciliation back to
`not_friend`. The target may receive a notification. Never remove an established friendship for
testing, never use an incoming request as a decline probe, and never retry an ambiguous mutation.
- Close the dedicated browser session when testing is complete unless the prompter/project owner explicitly asks to keep that local session open; never save or commit its cookies, local storage, or authentication state.

The literal test username in this protocol is the only approved personal identifier in repository documentation. It does not authorize committing any other account data or including live data in fixtures.

### 13.7 MCP schema, protocol, and process tests

MCP tests must be service-free unless a separately ignored native test explicitly says otherwise:

- Unit-test every input DTO conversion and output view, including exact decimal money, string IDs, RFC 3339 timestamps, explicit nulls, tagged activity/counterparty forms, request direction, and endpoint-specific continuation field names.
- Snapshot the exact eight-tool allowlist, titles, descriptions, JSON Schema 2020-12 input/output schemas, defaults, required/nullable fields, and annotations. Every input schema must set `additionalProperties: false`, and runtime deserialization must reject unknown fields. No auth mutation, secret, financial, internal-helper, resource, or prompt surface may appear.
- Exercise `initialize`, `tools/list`, and representative `tools/call` requests through the SDK's in-process or duplex transport using fake services. Initialization and listing must prove zero credential-reader and API calls.
- Build handler tests around a fake that implements only `CredentialReader`, `CurrentAccountApi`, and the required read API traits—not `CredentialWriter`, `CredentialDeleter`, or `PasswordLoginApi`—so MCP code cannot compile if it attempts authentication mutation. Verify production state holds only the read-only façade.
- Prove each invocation reloads the credential and that a changed fake credential is observed on the next call; no credential material remains cached in server state.
- Prove non-waiting one-permit admission returns `server_busy` for overlap, creates no pending queue, does not hold a standard mutex across await, and retains the permit until any cancelled `spawn_blocking` credential operation actually finishes.
- Prove the capacity-one/two-second-refill limiter returns `rate_limited` without sleep, queue, retry, credential access, or network access, and that initialization/tool discovery do not consume its budget.
- Reject inbound frames above 64 KiB before JSON parsing and serialized results above 2 MiB before stdout. Verify neither payload is reflected or logged and malformed SDK/protocol traffic cannot enable raw frame tracing at any verbosity.
- Test stable sanitized tool-error categories and verify invalid arguments fail before credential/network access without echoing the rejected value.
- Test hostile notes, names, and labels containing newlines, terminal controls, bidi/invisible controls, quotes, JSON-looking text, and prompt-injection instructions. They must remain correctly escaped data fields and never alter framing, metadata, logs, or errors.
- Launch the compiled `venmo-mcp` process and prove stdout contains only valid protocol frames while stderr contains no tool inputs, results, secrets, identifiers, notes, or continuations.
- Add compile-time `Send + Sync` checks for production server state and explicit regression tests for structured output matching `outputSchema` plus the equivalent JSON text fallback.
- Do not access the native keychain, Venmo, browser sessions, authentication endpoints, or financial endpoints in normal MCP tests or automated validation.

## 14. Quality and security gates

Required local and release validation (this repository intentionally has no GitHub Actions workflows):

```text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings -D clippy::unwrap_used -D clippy::expect_used
cargo test --all-targets --all-features
cargo doc --no-deps
cargo deny check
cargo build --locked --bin venmo-mcp
```

Also require:

- Advisory scanning.
- Pinned stable toolchain and separate MSRV job.
- Dependency updates through deliberate review.
- Secret scanning for source and fixtures.
- Workspace/package lint configuration forbids first-party `unsafe` across all targets, with no project-owned exceptions.
- Clippy denies `unwrap`/`expect` extraction across all first-party targets, including tests and build tooling.
- No real credentials, user records, notes, payment IDs, or account data in fixtures.
- Native platform tests before claiming platform support.
- Explicit macOS and Linux build/test coverage for `venmo-mcp`, including a service-free stdio initialize/list smoke test.
- MCP dependency MSRV, feature, license, advisory, and duplicate-schema-version review under the same lockfile and cargo-deny policy.
- Tests that fail if non-protocol bytes reach MCP stdout or if the registered tool/annotation permission matrix changes unexpectedly.

## 15. Documentation requirements

The README must let a new user complete setup without source-code knowledge.

Required sections:

1. Unofficial/private API warning.
2. Installation and supported platforms.
3. OS credential-store behavior and Linux Secret Service requirements.
4. The unsupported legacy password/SMS-OTP login flow, including exactly what is sent to Venmo, what is never stored, why every login requires a newly supplied trusted device ID, and the absence of token import or automatic refresh.
5. Explicit warnings that account credentials and the resulting token are equivalent to powerful secrets and must never be shared, logged, committed, or passed as shell arguments.
6. `venmo auth login`, trust-device warnings, status, and local-only logout behavior.
7. Friends and user-search examples, including server-page `--limit`, typed `--offset`, copyable next offsets, one-page behavior, and changing-dataset/no-snapshot semantics.
8. Payment-method and balance examples.
9. Grouped `pay options`/`pay user`, `transfer options`/`transfer out`, and `requests create`/`requests accept`/`requests decline`/`requests cancel` examples; automatic fail-closed and exact explicit `--source` payment/acceptance funding policies; rejection of source input elsewhere; and uniform default-No confirmation/`--yes` rules for `pay user` and all four request mutations.
10. Activity and pending-request examples, including server-page bounds, native before-token notices, sparse locally filtered pages, and how to copy a canonical incoming request ID into `requests accept` or `requests decline`, or an outgoing open request ID into `requests cancel`.
11. Ambiguous mutation recovery steps, including checking activity, request state, and the official app before any later operation.
12. Troubleshooting guidance.
13. Local MCP installation and host configuration that launches `venmo-mcp` over stdio with no secret environment variables, after authenticating through `venmo`.
14. The exact read-only MCP tool catalog, structured result and one-page pagination behavior, standard annotations, and the fact that annotations are hints rather than permissions.
15. Prominent MCP transcript/privacy and prompt-injection warnings: hosts may retain private financial/social output, and remote notes/names are untrusted data.
16. The absence of MCP credential mutation and initial financial tools, plus the requirements for any later startup-gated/human-approved write phase.
17. Upgrade, uninstall, credential removal, and how a running MCP server observes a successful explicit CLI login replacement on the next call.

Documentation must never suggest placing the password, OTP, OTP secret, device ID, or bearer token in a command argument, environment variable, file, browser automation, chat, screenshot, or log. Password/OTP/device-ID entry occurs only in the CLI's interactive prompts and only after an explicit `auth login` invocation.

## 16. Distribution

At minimum, preserve the currently intended targets:

- macOS arm64.
- macOS x86_64.
- Linux x86_64 GNU with documented Secret Service requirements.

Explicitly decide before release whether to add Linux arm64, Linux musl, and Windows x86_64. A target is supported only when every shipped binary and the native credential backend are tested there.

Release artifacts should contain:

- `venmo` executable.
- `venmo-mcp` executable once Phase 9 is complete.
- README.
- LICENSE.
- Manpage.
- SHA-256 checksums.
- Provenance/SBOM when supported by the release pipeline.

Package and smoke-test both executables from the same commit and version. Host configuration examples must reference an absolute installed path or normal executable lookup, use stdio, and contain no bearer token, device ID, password, OTP, or credential environment variable. `cargo install --locked venmo-cli` should install both binaries; document `--bin` selection only as an optional advanced installation choice.

Evaluate `cargo-dist` after initial target builds are stable. Recommended channel order:

1. GitHub release archives.
2. Homebrew tap.
3. `cargo install --locked venmo-cli`, if crates.io publication is desired.

## 17. Implementation phases

### Phase 0: API and UX validation

Deliverables:

- Freeze the terminal command surface and help text in Sections 3.1–3.8 and the MCP tool/schema/annotation surface in Section 3.9.
- Verify token-retrieval steps for the README without automating them.
- Validate the legacy password/SMS-OTP token issuance and device-trust paths without storing account identifier, password, or OTP material.
- Validate current-account, search, user, payment-method, payment-creation, and request-creation contracts.
- Discover and validate friends, balance, activity list/detail, pending-request list/detail, request acceptance, and request decline.
- For operations exposed by the Venmo web application, use the sanitized `agent-browser` network-observation workflow in Section 7.4 to establish the proper request shape; never commit or emit raw captures.
- Produce a dated sanitized contract dossier, typed DTO outline, fixture, and exact mock-server assertion for every retained API operation.
- Prove that accepting by canonical request ID settles that request with request-bound, prewrite fee/source safety and well-understood stale and ambiguous outcomes; independently prove that decline terminally refuses the exact incoming request without moving money.
- Perform any necessary live mutation only under the exactly-$0.01 approved-counterparty protocol in Section 13.6.
- Produce fully sanitized fixtures.
- For every paginated endpoint, verify ordering, continuation or offset semantics, reliable end/has-more signaling, filtering behavior, default and hard limits, per-page size, and maximum page/request bounds.
- Prove or reject legacy keychain interoperability.

Exit criteria:

- Every first-release command has a credible API or local implementation path.
- `requests list` has actual records and canonical IDs, not only counts.
- Complete request lookup, acceptance, and decline contracts have been verified without substituting an ordinary payment, outgoing cancellation, or local-only dismissal.
- No discovery fixture contains personal or secret data.
- Any unavailable required capability is brought back as an explicit scope decision.

### Phase 1: Rust scaffold (completed)

The scaffold is complete as the repository-root Cargo package. The temporary nested development layout no longer exists.

Deliverables:

- Shared library and thin terminal binary.
- Pinned toolchain and lockfile.
- Complete `clap` tree and help snapshots.
- Typed errors and exit codes.
- Workspace-enforced no-`unsafe`, no-`unwrap`, and no-`expect` lint policy across all first-party targets.
- Redacted tracing.
- Initial local-validation and dependency policy.

Exit criteria:

- Every selected command parses.
- Old command forms are rejected intentionally.
- Help and version require no services.

### Phase 2: Model and HTTP foundation

Deliverables:

- Feature/shared value types and pagination models.
- `reqwest` client, deadlines, response limits, and enforced zero-retry policy.
- Endpoint DTOs and mapping.
- Mock-server contract suite.

Exit criteria:

- Model invariants and all verified read contracts pass.
- Financial-mutation retry count is provably zero regardless of HTTP method.

### Phase 3: Keyring and authentication

Deliverables:

- Sole `keyring` adapter.
- `dialoguer` prompt adapter and interactive hidden-token prompt.
- Validation-before-save.
- Password/SMS-OTP `auth login`, `auth status`, and local-only `auth logout`.
- Corrupt-entry recovery and migration behavior.

Exit criteria:

- Invalid tokens are never stored.
- No public token argument/stdin/environment path exists.
- Reauthentication can replace an expired token for only the stored user without exposing or changing the stored device identity.
- Native keyring smoke tests pass on supported targets.

### Phase 4: Read and discovery commands

Deliverables:

- `friends list`.
- `users search`.
- `pay options`.
- `balance`.
- Sanitized output and truthful pagination.

Exit criteria:

- Output provides recipient identifiers accepted by `pay user`/request creation and inspectable payment-method metadata, including IDs accepted by the typed `--source` options after peer-eligibility validation; acceptance never claims the unproven actual debit source or fee.
- No command silently truncates without saying so.
- Public `--limit` is enforced as the one-request endpoint page size, and any validated native continuation is reported without describing the page as the complete collection.

### Phase 5: Activity and request reads

Deliverables:

- `activity list` and `activity info`.
- `requests list` with direction filtering.
- Pending-request detail lookup used by acceptance and decline preflight.
- Ambiguous-outcome recovery messaging.

Exit criteria:

- Activity can be used to inspect a known record.
- Pending-request output is based on complete verified records and prints IDs accepted by the planned mutation.
- Activity and request pagination obeys the one-page endpoint-native limit, ordering, local-filtering, and continuation policy in Section 10.1.

### Phase 6: Guarded financial and relationship writes

Deliverables:

- Single-recipient payment and request-creation models plus single-ID `requests accept`, `requests decline`, and `requests cancel` models.
- Recipient and funding-source resolution.
- Authoritative incoming-request lookup and state validation.
- Complete details and default-No confirmation rendering for `pay user` and all four request mutations.
- One-write execution, stale-state handling, and ambiguous-outcome handling.
- Native-state `friends add`/`friends remove`, exact P4/P5 transport, and mandatory P2
  reconciliation with the client-1 authorization gap documented.

Exit criteria:

- Every preflight failure sends zero writes.
- `pay user` and all request-mutation confirmation follows a complete authoritative preflight and defaults to No; user-facing output labels the prepared plan as details.
- Request creation exposes `--yes` under the same prompt-skipping-only policy.
- Request decline exposes `--yes`, requires default-No confirmation without it, and proves a terminal server state without money movement.
- No write is automatically retried.
- Acceptance settles the identified pending request and cannot silently become an unrelated payment.
- Decline terminally refuses the identified pending request and cannot silently become outgoing cancellation or local dismissal.
- Friendship add/remove selects only the four signer-evidenced native transitions, uses
  action-specific default-No confirmation, never treats remove as incoming decline, and never
  reports success without authoritative final-state proof.
- No multi-recipient, split, or `charge` surface exists.

### Phase 7: Documentation and release engineering

Deliverables:

- Complete README, including verified password/SMS-OTP login and local-only logout behavior.
- Contributor architecture documentation for feature/shared ownership, hexagonal dependency
  direction, dependency injection, the single Venmo client/transport, and future frontend
  composition.
- Testing, retained-contract, mutation-policy, and evidence-gated follow-up documentation.
- Curated `cli`/`model` facade inventory and pre-1.0 semver expectations.
- Manpage generation.
- LICENSE and security policy.
- Release archives containing every completed binary, checksums, and native smoke tests.

Exit criteria:

- A user can install, retrieve and store a token, find a recipient, inspect funding, safely perform a payment, create a request, and safely accept an incoming request using only shipped documentation.
- README and CONTRIBUTING link the architecture, testing, retained-contract, and evidence-gated
  records, and routine contributor guidance prohibits ignored/live tests and real financial
  commands.
- Every declared release target passes keyring and all shipped-binary smoke tests.

### Phase 8: Root Rust-only cutover (completed 2026-07-12)

- Promoted the Rust package, lockfile, toolchain, policy, source, and tests to the repository root.
- Removed the Node/pnpm/TypeScript implementation, SEA packaging, generated `dist`, dependency/build output, and obsolete product caches from the working tree.
- Updated local-validation guidance, ignore rules, package metadata, and development documentation to operate from the root Cargo package only.
- Kept legacy credential-envelope decoding solely for one-time native-keychain compatibility; no legacy runtime or source tree ships.
- Added explicit TypeScript-to-Rust credential and command migration notes to the root README, including removed financial interfaces and the absence of a legacy fallback.

### Phase 9: Initial local read-only MCP server

Deliverables:

- Keep auth status and every non-mutating feature entry point bounded by `CredentialReader`, keep credential writes and deletion behind their independent capabilities, keep current-account separate from password/device-trust mutation capabilities, and add production read-only credential/API façades without changing CLI behavior.
- Add defensive bounds for every arbitrary ID accepted through MCP before any credential or network access.
- Review and lock the official `rmcp` stable release and minimal stdio/server/schema feature set against Rust 1.95, supported targets, and cargo-deny.
- Add explicit MCP input/output DTOs, JSON Schema, exact money/time/ID conversions, tagged activity views, safe error mapping, and hostile-content tests.
- Add the exact eight-tool registry in Section 3.9 with accurate annotations and no resources, prompts, auth mutation, secret input, internal helper, or financial tools.
- Add generic handlers over the existing feature/API ports, one shared production read-only API façade, per-call credential loading, and a one-permit async operation gate.
- Add bounded 64 KiB ingress, 2 MiB serialized-result enforcement, non-waiting `server_busy` admission, capacity-one/two-second-refill rate limiting, and SDK payload-log suppression.
- Add the `venmo-mcp` stdio binary with protocol-only stdout and stderr-only diagnostics while preserving `default-run = "venmo"`.
- Add protocol, process, schema, permission-matrix, no-cache, concurrency, cancellation, nonleakage, and service-free startup/list tests.
- Update local/release validation, release packaging, README host configuration, transcript/privacy warnings, and troubleshooting for the second executable.
- Manually validate the second executable's native credential access with an isolated service/account on supported platforms.

Exit criteria:

- `initialize` and `tools/list` succeed without keychain or network access and list exactly the eight read-only tools.
- Every tool calls the same feature use case as its CLI counterpart, preserves one-page endpoint-native pagination, and returns schema-valid structured output.
- Server state has no credential save/delete capability and caches no bearer token, device ID, or loaded credential between calls.
- Stdout contains only valid MCP protocol frames; stderr and tool errors contain no arguments, results, account data, notes, continuations, or secrets.
- Exact tool schemas and annotations are regression-tested, and no tool capable of mutation is registered.
- Manual release validation on macOS and Linux builds/tests both binaries, cargo-deny passes, and packaged stdio smoke tests remain service-free.
- A user can authenticate with `venmo`, configure a trusted local MCP host to launch `venmo-mcp` without secret environment variables, and invoke the documented reads.

### Phase 10: Future opt-in financial MCP tools (deferred and gated)

Entry criteria:

- Phase 6 has complete verified CLI feature/API contracts for the exact proposed MCP write tools, including stale-state, no-retry, and ambiguous-outcome behavior.
- A named supported MCP host exposes a trustworthy approval policy on the execute tools and can present every server-resolved safety-critical field immediately before execution.
- The project owner explicitly approves the concrete tool schemas, host integration, startup flag, replay/cancellation model, and documentation.

Potential deliverables only after those gates:

- A non-secret `--allow-financial-writes` startup mode whose tool registry is absent by default and fixed for that server process.
- A short-lived, single-use, process-local prepare/execute plan protocol (or reviewed stronger attestation) that binds host approval to the immutable shared feature plan.
- Financial execute tools that require exact plan equality, fresh authoritative preflight, one-operation serialization, no retry, and post-state/ambiguity handling.
- Destructive, non-idempotent, open-world annotations and host configuration requiring human approval for every invocation.
- Protocol/process tests proving default absence, no model-supplied confirmation bypass, no duplicate execution, and truthful cancellation/unknown-outcome behavior.

Exit criteria:

- A default `venmo-mcp` process cannot discover or invoke any financial tool.
- Startup opt-in without a supported trusted host approval path still does not register financial tools.
- No tool accepts `--yes`, `confirmed`, or any model-provided substitute for human approval.
- A model cannot alter any plan field after preparation, reuse a consumed/expired handle, or cause changed state to execute without a new host-visible approval.
- Every approved invocation can execute at most one financial write, is never automatically retried, and preserves the CLI's exact safety and reconciliation guarantees.

## 18. Definition of done

- All CLI commands in Sections 3.1–3.8 and the initial MCP tools in Section 3.9 are implemented and documented.
- No old `init`, `deinit`, `charge`, split, or multi-recipient interface remains.
- `auth login` prompts for the identifier, hidden password, and a newly supplied hidden trusted device ID in that order, and handles SMS OTP when required. It accepts no secrets through arguments, stdin flags, environment variables, or files.
- Interactive prompts use the tested `dialoguer` adapter and never echo token material or write prompt UI to stdout.
- README trusted-device setup steps are verified and carry prominent secret-handling warnings.
- The token is stored only in the native OS credential store through `keyring`.
- Friends and search output provide copyable recipient identifiers.
- Payment-method output provides inspectable IDs and metadata but no command accepts them as funding input; acceptance selects any external backup internally and does not claim the planned source is the final debit source.
- Balance semantics are verified and do not overclaim external balances.
- Activity supports list and detail views usable for write reconciliation.
- Pending requests are based on complete verified records and expose canonical IDs accepted by `requests accept`, `requests decline`, and `requests cancel` according to direction/state.
- Payment and request-acceptance confirmation follows recipient/request and operation-specific funding resolution; request creation, decline, and cancellation confirmation follows authoritative operation-specific preflight.
- Only `pay user`, `requests create`, `requests accept`, `requests decline`, and `requests cancel` expose `--yes` or use mutation confirmation; `pay options` does neither.
- No command exposes `--dry-run`.
- `requests accept` validates an authoritative incoming pending record and settles that exact record through a verified contract.
- `requests decline` validates an authoritative incoming pending record, sends no money, and proves that exact record reached the supported terminal server state.
- `requests cancel` validates an authoritative outgoing pending/held charge, sends no money, and proves that exact record reached terminal `cancelled` state.
- One invocation executes at most one financial write.
- Failed writes preserve actionable root causes.
- Ambiguous writes are never retried and direct the user to verification.
- Every HTTP call has a deadline; no command automatically retries.
- Common infrastructure uses suitable mature, popular, community-maintained crates; every deliberate local reimplementation of common functionality has the required recorded exception.
- All first-party targets contain no `unsafe`, `unwrap`, `unwrap_err`, `expect`, `expect_err`, or hidden equivalents and pass the enforced workspace lints.
- Every previously unspecified failure behavior was clarified with the prompter/project owner and encoded as a typed mapping plus tests rather than guessed or swallowed.
- Public paginated commands fetch one bounded source page, reject invalid native inputs or API continuations with distinct typed categories, emit only the endpoint-native next value after successful output, and never expose or follow a raw next URL.
- Corrupt credentials can be replaced or deleted.
- Every terminal-bound untrusted string is sanitized.
- Unit, property, feature, HTTP, keyring, CLI, and release smoke tests pass.
- Packaged `--debug` logging works at every command depth, remains silent by default, bounds and
  terminal-sanitizes remote error text, and leaks no known secrets or request data.
- Public production dispatch and runtime fallback own actual terminal detection; no external caller
  can inject synthetic TTY state.
- Service-free dispatch preconditions do not initialize the process-global logging subscriber.
- Production and release paths require only the Rust/Cargo toolchain and documented native platform services.
- No live financial write occurs in automated tests or release validation.
- Every manually approved live payment, request, acceptance, or decline test uses exactly $0.01 and the approved test counterparty under Section 13.6, apart from the documented one-time acceptance exception.
- No material contradiction or ambiguity discovered during implementation is resolved without clarification from the prompter/project owner and a recorded plan/test update; finished CLI users are never asked to make implementation decisions.
- `venmo` and `venmo-mcp` are thin adapters over the same feature use cases, shared models, credential implementation, and private-API client; MCP does not parse terminal output or duplicate HTTP contracts.
- The initial MCP registry exposes exactly the eight Section 3.9 read tools, no resources/prompts, no auth mutation or secret input, and no financial/internal-helper tools.
- Every initial MCP tool has accurate read-only/non-destructive/open-world annotations, strict input/output schemas, and structured results using exact money, time, ID, tagged activity, and native continuation representations.
- MCP initialization and tool listing are keychain/network-free; every tool reloads credentials per call through read-only capability and leaves no credential cached.
- Type-level MCP state exposes neither local credential mutation nor remote login/device-trust capabilities.
- Bounded frames/results, non-waiting one-operation admission, conservative rate limiting, and cancellation-safe permit ownership prevent unbounded buffering, queues, and overlapping keychain/Venmo work.
- MCP stdout is protocol-only, diagnostics are stderr-only, and hostile remote content remains labeled JSON data rather than instructions, metadata, errors, or logs.
- Documentation warns that annotations are hints, read-only results remain private/sensitive, and MCP hosts may retain tool data in transcripts or telemetry.
- Both executables are built, tested, packaged, and smoke-tested on supported targets. Financial MCP tools remain absent unless every Phase 10 gate is met.

## 19. Principal risks

| Risk | Mitigation |
| --- | --- |
| Private endpoints changed or disappear | Phase 0 validation, isolated DTOs, sanitized fixtures, typed per-command errors, and `auth status` |
| Pending request records are unavailable | Fail closed; never substitute counts, incomplete guesses, an ordinary payment, outgoing cancellation, or local-only dismissal |
| Live discovery moves money | Use only the Section 13.6 protocol: exactly $0.01 with the approved test counterparty, one immediately confirmed mutation at a time, no unattended authorization, and immediate reconciliation |
| A timed-out write actually succeeded | Never retry; classify as ambiguous; use activity detail, request state, and the official app for verification |
| Wrong recipient | Exact resolution and one recipient; show identity in `pay user` details before default-No confirmation and in the confirmed request-creation result |
| Wrong funding source | Apply the validated automatic policy, fail closed on ambiguity, never rank by order or fee, and display the submitted safe method label and ID before confirmation |
| Wrong, stale, or already-settled request | Fetch authoritative record, require incoming/pending/current-account ownership, confirm all immutable fields, and handle state conflict without a retry |
| Token exposure | Hidden prompt, keyring-only persistence, redacted token type, no body/header logs |
| Malicious terminal text | Sanitize every human-rendered untrusted string |
| Rust cannot read the legacy Node-created keychain item | Native migration spike, explicit-login replacement documentation, no silent deletion |
| Linux has no Secret Service | Fail closed with actionable guidance; do not use insecure fallback |
| Cross-compiled binary hides runtime failures | Native keyring and archive smoke tests before claiming support |
| Third-party research has stale or incompatible code | Use it only as a behavioral lead; validate independently and do not copy incompatible code |
| Browser network discovery exposes session or account data | Use the Section 7.4 `agent-browser` workflow, inspect only the correlated call through a local structural redactor, avoid HAR by default, never emit raw details, and destroy any approved temporary capture after sanitization |
| Imported web token expires or is bound to the wrong device | Import the token and matching `v_id` only through hidden prompts, validate before storage, perform read-only lifetime observation, and do not use that credential for enabled financial commands until durability is proven |
| MCP stdout is contaminated by banners, tracing, or terminal output | SDK owns stdout exclusively; diagnostics use stderr; process tests parse every stdout frame and fail on any extra byte |
| MCP host retains private financial or social data | Prominent transcript/telemetry warnings, structured minimal results, no secrets, and host-specific privacy guidance |
| Venmo notes or names contain prompt injection | Treat all remote text as untrusted labeled data; never interpolate it into tool metadata, instructions, errors, logs, or authorization decisions; test hostile payloads |
| MCP tool annotations are wrong or treated as enforcement | Snapshot the exact annotation matrix, use conservative values, state that hints are advisory, and enforce capability through registration and server policy |
| The second executable cannot access the same native credential session | Per-call read-only adapter, isolated native ACL/session tests, actionable failure, and no file/environment fallback |
| Concurrent MCP calls cause rate bursts, credential prompts, or races | One async operation permit around credential and Venmo work; no detached retries or standard mutex held across await |
| A host or model replays a future financial tool | Tools absent by default, explicit startup gate plus trustworthy per-call human approval, non-idempotent/destructive annotations, one-write execution, and no retry |
| MCP cancellation occurs after a future write was transmitted | Preserve ambiguous-outcome classification, stop further work, never replay, and require independent reconciliation |
| MCP input, output, or pending calls consume unbounded memory | Enforce 64 KiB inbound frames, 2 MiB serialized results, no pending operation queue, immediate `server_busy`, and tested SDK transport bounds |
| A local harness polls Venmo aggressively | Capacity-one/two-second-refill process limiter, one active operation, immediate `rate_limited`, no delayed queue, and zero automatic retries |
| Read-only MCP code can reach auth login/trust methods | Split current-account and mutation ports, hold only read-only credential/API façades, and compile-test fakes without mutating traits |
| Host approval covers model input but not the resolved write plan | Require a short-lived single-use prepare/execute plan, repeat exact fields in host-visible execute args, reject any drift, and keep writes blocked without a named tested host policy |

## 20. Remaining decisions

The CLI grammar and initial read-only MCP tool set are settled. Remaining implementation/release decisions are:

1. Natural final-page release observation for user search, activity, and pending requests, plus best-effort native continuation behavior as each private endpoint's dataset changes. First-to-second endpoint continuation is live-verified for all four paginated commands, and friends exhaustion is live-verified.
2. Whether transparent legacy keychain migration is possible on every supported platform.
3. Whether the first release adds Linux arm64, musl, or Windows.
4. GitHub-only initial distribution versus Homebrew and crates.io at launch.
5. Whether JSON output for the terminal CLI belongs in the next release or a later milestone; MCP structured output does not resolve that separate decision.
6. Whether controlled validation can establish protected external-funded acceptance, actual
   source/fee evidence, and a supported request-acceptance webview/SMS-step-up continuation. Unprotected
   notification-ID acceptance is already owner-validated under client 1.
7. Whether a legitimate current native-mobile onboarding contract or official device-authorization flow can eliminate manual trusted-device bootstrap without reproducing browser anti-bot controls. The observed web flow is explicitly not a production candidate.
8. The exact published official `rmcp` release and minimal feature set that satisfy Rust 1.95, macOS/Linux, schema, license, advisory, and cargo-deny requirements.
9. Whether native MCP validation shows synchronous keychain reads need a `spawn_blocking`/loaded-credential feature seam rather than relying only on a multi-thread runtime.
10. Which local MCP hosts, if any, can provide a trustworthy per-call human approval contract sufficient to unlock the separately gated future financial phase. Until one is selected and tested, writes remain unavailable.
11. Whether any later release should add a remote MCP transport. The initial release is definitively local stdio only and must not accumulate HTTP/OAuth features speculatively.
12. Whether future evidence justifies any independently gated instant, debit, transfer step-up,
    cancellation, or expedition command. Transfer-in is intentionally out of scope; the
    standard-bank cash-out pending-submission scope is finalized as implemented.
13. Whether the owner can provide a consenting nonfriend for one separately approved add/revoke
    canary proving current client-1 authorization. Until then, friendship mutations remain
    signer-evidenced and synthetically verified but not live validated.

## 21. Immediate next steps

1. Complete formatting, lint, test, rustdoc, dependency-policy, and independent safety review for every production CLI command.
2. Retain the reconciled acceptance evidence that the response uses a distinct settled-payment ID and cannot prove the final funding source or fee; do not repeat the mutation.
3. Retain the reconciled decline evidence: one owner-approved one-cent `deny` write returned exact terminal `cancelled` state, removed the pending request, created no payment activity, and left available balance unchanged. Do not repeat the mutation speculatively.
4. Keep auth status and every shared read feature use case bounded by `CredentialReader`, keep current-account separate from password-login/device-trust ports, add read-only native credential/API façades, and prove existing CLI behavior remains unchanged.
5. Revalidate and lock the official `rmcp` stable release, minimal stdio/server/schema features, MSRV, license, advisories, and transitive graph.
6. Implement explicit MCP DTOs, schemas, conversions, annotations, generic read-only handlers, the exact registry, bounded framing/results, and the thin `venmo-mcp` stdio binary with protocol/process/nonleak tests.
7. Update local/release validation, release packaging, and README with both binaries, local host configuration, transcript privacy, prompt-injection treatment, annotations, and troubleshooting.
8. Perform isolated manual native credential access validation for the second executable on supported platforms; never use production credentials during automated validation.
9. Continue occasional explicit read-only observation of the validated mobile-issued token's lifetime. Do not poll aggressively, infer an expiry date, store the password, automatically renew, or retry after rejection.
10. Keep the live-verified CLI pay and request-creation contracts covered by exact synthetic tests and monitor them only through normal use; do not repeat canary mutations speculatively. Keep every MCP financial command unavailable until its own request, response, ambiguity, verification, and human-approval contracts are established.
11. Retain the reconciled standard-out evidence: one approved $0.01 HTTP-201 write returned exact
    pending data and matched exactly one outgoing activity record by transfer ID. Do not repeat the
    canary speculatively; preserve strict response proof, one-write/no-retry handling, and the
    independently gated status of every other transfer variant.
12. Keep friendship mutation validation service-free unless the owner separately approves the
    consenting-target add/revoke protocol. Never use an established friendship or infer permission
    from the implementation request alone.

# venmo-cli Rust Product Plan

**Status:** Active; root Rust-only repository cutover completed; local read-only MCP server planned
**Last updated:** 2026-07-12
**Package:** `venmo-cli`
**Binaries:** `venmo`; planned `venmo-mcp`

## 1. Product direction

The product is one repository-root Rust package with a shared `venmo_cli` library and two thin native entry points: the terminal CLI `venmo` and the planned local stdio MCP server `venmo-mcp`. Both adapters must call the same application use cases, domain types, credential implementation, and Venmo API client; only protocol parsing, presentation, and process composition differ. The completed language and repository cutover was also an intentional redesign of the command interface; the former TypeScript command surface remains only historical behavioral evidence, not a compatibility requirement or shipped implementation.

The first release should make the common workflow obvious:

1. Save a bearer token securely.
2. Verify which Venmo account is active.
3. Find a friend or another user.
4. Inspect the Venmo balance and available payment methods.
5. Pay one person, request money from one person, or accept one incoming request.
6. Inspect activity and pending requests.
7. Diagnose authentication or private-API failures.

This remains an unofficial client for unsupported/private Venmo endpoints. Endpoint discovery and contract testing are release gates, not assumptions.

The initial MCP release provides structured, read-only access to the already implemented public read operations. It is not a wrapper around terminal commands or table output, and “roughly the same API” does not mean exposing terminal-only credential management, package commands, internal helper functions, or unverified financial mutations. Users authenticate and reauthenticate with `venmo`, then configure a trusted local MCP host to launch `venmo-mcp`.

## 2. Confirmed product decisions

- Use Rust with a pinned stable toolchain and a committed `Cargo.lock`.
- Use Rust 1.95.0 for the pinned toolchain and MSRV, package version `0.1.0`, and the MIT license.
- Use [`clap`](https://docs.rs/clap/) with its derive API.
- Use [`dialoguer`](https://docs.rs/dialoguer/) behind a narrow prompt adapter for hidden token input, default-No confirmation, and funding-method selection.
- Prefer mature, popular, actively community-maintained off-the-shelf crates for common functionality instead of rolling project-owned replacements; keep custom code focused on Venmo-specific domain behavior, application orchestration, and safety policy.
- Use a hybrid command hierarchy: frequent actions are top-level; related management operations are grouped.
- Use [`keyring`](https://docs.rs/keyring/) as the only persistent credential backend.
- `venmo auth login` uses the legacy mobile-private trusted-device-ID plus username/password and SMS-OTP flow by default; `venmo auth login --token` preserves hidden import of an existing bearer token. `venmo auth reauthenticate` explicitly repeats full password/optional-OTP issuance for the stored account while reusing its stored trusted device ID; it is not a refresh operation.
- Typed login credentials and OTP material are zeroized on drop and never stored. Serialization, HTTP, and platform libraries may create bounded transient request/header copies that Rust cannot guarantee are overwritten; the CLI never logs or persists those copies and drops them promptly. Only a validated bearer token, persistent device ID, account identity, and local saved-at time enter the OS credential store.
- Target Venmo's bearer-token/device-ID mobile private `/v1` API rather than emulating the browser session API. Web-app traffic is discovery evidence only; do not adopt cookie/CSRF authentication, browser backend routes, or a browser User-Agent merely because the web client exposes an operation.
- A validated token for a different account never replaces a valid stored credential; `auth login` fails without mutation and requires an explicit `auth logout` first.
- Do not ship browser automation, browser-cookie import, or web-session authentication as CLI features. Development-time browser observation under Section 7.4 is permitted solely to establish sanitized API contracts.
- Document the unsupported password/SMS-OTP login risk and the optional existing-token import path in the README.
- Support one recipient per new `pay` or `request` invocation and one request ID per acceptance invocation.
- Release `pay`, request creation, and request acceptance independently. `pay` and request creation passed their dated contract, synthetic-test, controlled-live-validation, and reconciliation gates on 2026-07-12 and are enabled. Request acceptance remains unavailable; its unverified contract does not block the two verified commands.
- Treat the controlled-live-test boundary as absolute: send only one $0.01 payment at a time to `@georgecma`, create only one $0.01 request at a time from `@georgecma`, and reconcile that mutation before authorizing another. This restriction governs operator testing even though synthetic tests may use invented users and amounts.
- Keep direct request creation as `venmo request ...`; reserve the exact `accept` token for `venmo request accept ...`.
- Call transaction history `activity`.
- Keep payment audience private in the first release.
- Limit the first financial release to ordinary personal-profile peer-to-peer operations with a fee proven by the verified contract to be exactly zero. Reject business, charity, purchase-protection, nonzero-fee, and unknown-fee operations before any write.
- Treat `--from` as the submitted preferred external or backup funding method, not a guarantee that the method will be debited. Venmo wallet balance may take priority; preflight must state that behavior and success output may name an actual source only when authoritative evidence proves it.
- Require default-No confirmation only when the user sends money: `pay` and `request accept`.
- Expose `--yes` only on `pay` and `request accept`, and require it unless both stdin and stderr are interactive terminals. A redirected stderr must never hide a financial confirmation prompt.
- Do not expose any `--dry-run` flags.
- Never automatically retry payment creation, request creation, or request acceptance.
- Write no first-party `unsafe` Rust and prohibit it mechanically across every first-party target.
- Use no `unwrap`, `unwrap_err`, `expect`, `expect_err`, or equivalent panic-based value extraction in first-party Rust, including tests and build tooling.
- If the correct treatment of any failure is unclear, stop the affected implementation and ask the prompter/project owner rather than guessing, swallowing it, panicking, or choosing an arbitrary fallback.
- Include shell completion generation in the first release.
- Keep one Cargo package and shared library, add `venmo-mcp` as a second thin binary, and set Cargo `default-run = "venmo"` so existing `cargo run -- ...` behavior remains unambiguous.
- Use local stdio as the only initial MCP transport. Do not enable Streamable HTTP, SSE, remote authentication, or network listening in the first MCP release.
- Register only the nine read tools in Section 3.11 initially. Do not expose resources or prompts until a concrete use case requires them.
- Keep the entire credential lifecycle terminal-CLI-only. MCP may report authenticated status but must never offer login, token import, password/OTP collection, reauthentication, logout, revocation, credential overrides, or secret-bearing tool inputs.
- Give MCP its own explicit input/output DTO and JSON Schema boundary. Do not serialize secret domain types, stored credential envelopes, raw private-API DTOs, headers, bodies, or keychain errors.
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
venmo auth login [--token]
venmo auth reauthenticate
venmo auth logout [--revoke]
venmo auth status

venmo pay <RECIPIENT> <AMOUNT> --note <NOTE> [--from <METHOD_ID>] [--yes]
venmo request <RECIPIENT> <AMOUNT> --note <NOTE>
venmo request accept <REQUEST_ID> [--from <METHOD_ID>] [--yes]

venmo friends list [--limit <N>] [--offset <N>]
venmo users search <QUERY> [--limit <N>] [--offset <N>]
venmo payment-methods list
venmo balance

venmo activity list [--limit <N>] [--before-id <TOKEN>]
venmo activity show <ACTIVITY_ID>
venmo requests list [--direction <DIRECTION>] [--limit <N>] [--before <TOKEN>]

venmo doctor
venmo completions <SHELL>
```

No compatibility aliases should be added initially. Add aliases only in response to demonstrated user need.

`request` deliberately has a default create form plus an `accept` subcommand. The exact first token `accept` selects the subcommand; valid creation recipients must begin with `@` or be numeric, so this reserves no valid recipient spelling. `venmo request @accept ...` remains an unambiguous request to the user named `accept`. Freeze this behavior in parser and help snapshots rather than adding a `request create` alias.

### 3.1 Global behavior

```text
venmo [--verbose] <COMMAND>
```

- `--verbose` writes redacted diagnostics to stderr.
- Data and successful results go to stdout.
- Prompts, warnings, diagnostics, and errors go to stderr.
- `--help` and `--version` must not touch the keychain or network.
- Human output uses color only on a terminal and honors `NO_COLOR`.
- Machine-readable **CLI** output is deferred, but command handlers must return typed results so JSON can be added without rewriting application logic. MCP structured results are a separate protocol requirement and do not settle the future `venmo --json` product decision.

Proposed stable exit codes:

| Code | Meaning |
| ---: | --- |
| `0` | Success |
| `1` | Operational, credential, validation, API, or user-cancellation failure |
| `2` | CLI usage/precondition error, including argument parsing or a required noninteractive `--yes` |
| `3` | Financial write outcome is unknown and must be verified |
| `130` | Interrupted before a write became ambiguous |

### 3.2 Authentication

#### `venmo auth login [--token]`

- Requires an interactive terminal.
- Default mode prompts for the Venmo email/phone/username, then a hidden password, then hidden-prompts for a browser `v_id`/device ID that Venmo already trusts. Hidden inputs use terminal echo suppression rather than rendering masking characters. It sends the values only to the fixed Venmo mobile-private HTTPS origin through the narrow legacy authentication adapter.
- Default mode handles the verified SMS-OTP challenge by requesting the text message and prompting for a hidden six-digit code. It never prints or stores the password, OTP, or OTP secret.
- `--token` instead prompts for an existing bearer token using hidden input. It accepts raw token, `Bearer ...`, or `Authorization: Bearer ...` forms and stores only the normalized token value. On first-time or unusable-entry recovery it also prompts, using hidden input, for the matching `v_id`/device ID; it never invents a replacement device ID for imported browser session material. A readable existing credential keeps its stored device ID.
- Exposes no username, password, OTP, token, device-ID, stdin, environment-variable, or browser-cookie argument.
- Password login is blocked before prompting or network access when any readable credential already exists; the user must run `auth logout` first. A targetable unusable entry may be recovered after the new token validates.
- Preserves the manually supplied trusted device ID across the password/OTP sequence and any post-OTP trust request, and stores it only after an issued token validates. It never generates or substitutes a device ID. Token-import mode reuses a readable existing device ID or requires the matching device ID through a second hidden prompt.
- Validates every issued or imported token with the current-account endpoint before storage.
- Never automatically retries token issuance. If timeout, disconnect, redirect, cancellation, or a malformed successful response occurs after issuance may have begun, return exit `1`, state that authentication outcome is unknown and a remote token may have been issued, and direct the user to official Venmo session controls.
- If password/OTP authentication returns a token but current-account validation fails, do not store, trust, automatically revoke, or retry that token. Return exit `1` and warn that the remote token may remain active until the user reviews official Venmo session controls.
- Direct password success proves that Venmo accepted the supplied existing trusted device, so it performs no redundant `/users/devices` mutation and reports complete success. After OTP login, it attempts to trust the supplied device. If token issuance and account validation succeed but that post-OTP trust request fails, save and verify the credential, return exit `1`, and warn that future login may require OTP; never discard an issued token silently.
- After validation, can replace any single targetable unusable entry, including corrupt, invalid, oversized, legacy, or unknown-schema data. Unavailable stores, ambiguous/multiple matches, and untargetable platform failures remain blocking errors.
- Token-import mode sends no storage mutation when a readable credential belongs to a different account and requires `auth logout` before switching accounts. Password login requires `auth logout` before it prompts whenever any readable credential exists.
- Reads the saved entry back and requires an exact versioned-envelope match before reporting success. A save error, missing read-back, read-back error, or mismatch returns exit `1`, reports credential storage state as unknown, performs no automatic retry or rollback, and directs the user to `auth status`.
- Prints only the authenticated username and safe trust-device status; never echoes credential, token, OTP, OTP-secret, or device-ID material.

#### `venmo auth reauthenticate`

- Requires an interactive terminal and exactly one readable stored credential. Missing and unreadable entries are typed login failures; unlike token import, this command never treats corrupt or otherwise unusable stored data as replaceable because it must recover the existing trusted device and account identity.
- Exposes no arguments or options for username, email, password, OTP, OTP secret, token, device ID, environment input, stdin, stdin files, or similar alternate secret sources. It prompts for the account identifier and hidden password only after the terminal and stored credential checks pass, and prompts for a hidden SMS code only after the exact recognized `81109` challenge.
- Reuses the stored device ID for the one legacy password-login initiation, optional OTP request/completion, current-account validation, and any post-OTP trust request. It never prompts for, prints, generates, or substitutes a device ID.
- Performs full password/optional-OTP token issuance with zero automatic retries and the same unknown-issuance handling as initial password login. Typed password, OTP, and OTP-secret values are zeroized on drop and never stored; bounded transient copies inside serialization/HTTP libraries are dropped promptly but cannot be guaranteed overwritten.
- Does not validate or otherwise require the old bearer token to remain usable. The purpose is renewal after token expiration while retaining the established device identity; there is no implemented or evidenced token-refresh endpoint.
- Validates the newly issued token through current-account using the stored device ID and requires the returned user ID to equal the stored credential's user ID. Validation failure or a different account performs no credential save, trust call, rollback, revocation, or retry, leaving the old credential untouched. A mismatch truthfully warns that the newly issued remote token may remain active and directs the user to official session controls.
- Only after successful same-account validation, replaces the credential through the existing single-entry save, reads it back, and requires the exact versioned envelope. Save/read-back uncertainty preserves the issued-token ambiguity warning and performs no trust request.
- Direct password success performs no redundant `/users/devices` call. An OTP-completed flow best-effort trusts the device only after the validated replacement has been saved and verified; trust failure retains the credential, reports incomplete success, and warns that future login may require SMS verification.
- Uses the existing password-login report and prints only safe account/trust status. It never reveals the stored device ID or any authentication material.

#### `venmo auth logout [--revoke]`

- Deletes the local keychain entry without requiring it to deserialize first.
- With `--revoke`, attempts remote token revocation before local deletion.
- If revocation fails, still honors the explicit request to remove the local credential, returns exit code `1`, confirms the local deletion, and warns that the remote token may remain valid.
- Every partial outcome is reported truthfully and exits `1`: if revocation succeeds but deletion fails, report that the remote token was revoked but the local entry remains; if both fail, report that local deletion failed and remote state is unknown. Do not let either result hide the other.
- Is idempotent when no local credential exists.

#### `venmo auth status`

- Verifies that the keychain entry can be read and parsed.
- Calls the current-account endpoint to validate the token.
- Displays the active username, display name, user ID, and local credential saved-at time; never presents that local timestamp as the token's server-issued creation time.
- Distinguishes missing, corrupt, locked, unavailable, and rejected credentials.

### 3.3 Paying one person

```text
venmo pay @alice 12.50 --note "Dinner"
venmo pay @alice 12.50 --note "Dinner" --from 123456789 --yes
```

Contract:

- Exactly one recipient.
- Recipient is an exact `@username` or positive numeric Venmo user ID.
- Amount is a positive decimal with at most two fractional digits.
- `--note` is required and must contain non-whitespace text.
- `--from` accepts a payment-method ID shown by `payment-methods list` as the preferred external or backup method submitted to Venmo. It does not promise that Venmo will debit that method instead of available Venmo balance.
- `--yes` skips only confirmation; it does not skip validation or preflight.

Funding-source selection order:

1. Exact `--from <METHOD_ID>` match.
2. The one source marked as default.
3. The only available source.
4. An interactive selection when a terminal is available.
5. A clear failure in non-interactive use.

Selection operates only on methods that the verified mobile contract identifies as eligible for peer payment. Writer-side selection preserves and interprets only the exact peer-payment role and exact external `bank`/`card` type; wallet balance, merchant/default-transfer roles, unknown types, and substring matches cannot establish an eligible backup source. The live response can omit a usable method-level fee for the automatic default external source, so only that exact default may proceed with an unknown method fee to the transaction-specific eligibility check; the immutable plan upgrades it to proven-zero only when that response totals exactly zero cents. A known nonzero method fee always fails, and an explicit `--from` or non-default source with an unknown method fee remains blocked because the eligibility request's blank funding-source field does not bind its proof to an override. Duplicate IDs, unknown peer roles, or multiple defaults are contract failures; an explicit unavailable ID fails; and multiple non-default methods require an explicit proven-zero `--from` or interactive selection among already proven-zero methods followed by the same transaction eligibility check.

Before confirmation, show:

- Resolved recipient username, display name, and user ID.
- Exact amount.
- Note.
- Private audience.
- Proven fee of `$0.00` and exact total.
- Current Venmo wallet balance when supplied by the verified preflight contract.
- Submitted preferred external or backup payment method, including a safe label and ID.
- A warning that Venmo may use available wallet balance before the submitted backup method.

Confirmation defaults to **No**. Non-interactive execution without `--yes` is rejected.

### 3.4 Creating and accepting requests

#### Creating a request for one person

```text
venmo request @alice 12.50 --note "Dinner"
```

The command shares recipient, amount, note, and preflight validation with `pay`, but:

- Creates a request rather than a payment.
- Never accepts or sends a funding-source ID.
- Has no confirmation prompt and no `--yes`; after successful preflight it creates the request immediately.
- Works non-interactively without an acknowledgement flag.
- Rejects payment-only options, `--yes`, and `--dry-run` instead of ignoring them.
- Still performs at most one write and is never retried automatically.

#### `venmo request accept <REQUEST_ID>`

```text
venmo request accept 123456789
venmo request accept 123456789 --from 987654321 --yes
```

This is a financial write that pays exactly one existing incoming request. Contract:

- `REQUEST_ID` is the canonical request identifier printed by `requests list`; its validated syntax must follow the verified API contract.
- Fetches the server-side request record before prompting.
- Requires the request to be pending, incoming, addressed to the authenticated account, and explicitly payable through the verified acceptance contract.
- Rejects outgoing, completed, declined, cancelled, inaccessible, malformed, or wrong-account records before any write.
- Takes no recipient, amount, or note arguments. The requester, amount, and note come only from the fetched request record and cannot be changed by this command.
- `--from` and automatic backup-funding selection follow the same rules and balance-priority disclosure as `pay`.
- `--yes` skips only confirmation; it does not skip lookup, state validation, or funding-source resolution.

Before confirmation, show:

- Request ID and current pending status.
- Requester username, display name, and user ID.
- Exact requested amount and sanitized note.
- Request creation time when supplied by the verified contract.
- Proven fee of `$0.00`, current wallet balance when available, and submitted backup payment method including a safe label and ID.
- A warning that Venmo may use available wallet balance before the submitted backup method.
- An explicit statement that accepting will pay the requester and settle this request.

Confirmation defaults to **No**. Non-interactive execution without `--yes` is rejected. The acceptance operation must use a verified request-acceptance contract; it must not silently substitute an unrelated ordinary payment to the requester. A stale-state rejection is a confirmed failure, while a timeout or disconnect after possible transmission is an ambiguous financial outcome with exit code `3`.

On confirmed success, print the accepted request ID, resulting activity/payment ID when supplied, requester, amount, and server-reported status. Do not claim that the request settled unless the verified response or a follow-up read proves that state.

### 3.5 Friends and user discovery

#### `venmo friends list [--limit <N>] [--offset <N>]`

- Lists friends of the active account.
- Displays a copyable recipient value such as `@username`, display name, and user ID.
- Fetches exactly one API page using the supplied server page size and zero-based offset; defaults are 10 and 0, and the page-size maximum is 50.
- If the trusted next link has another page, writes only its validated copyable `Next offset: <N>` value to stderr after record output succeeds.
- Treats the invocation as one page rather than claiming that it is the complete friend list.

#### `venmo users search <QUERY> [--limit <N>] [--offset <N>]`

- Searches for users who may not be friends.
- Returns the same copyable identity fields as `friends list`.
- Uses an exact username match when resolving a payment recipient; search ordering alone never selects a recipient.
- Fetches exactly one API page and treats `--limit` as its server page size, with default 10 and maximum 50. `--offset` is a typed nonnegative `u32` with default 0.
- When a full page is returned, provisionally reports the checked synthesized `Next offset: <N>` value; an empty page at that offset can be required to prove exhaustion.

Financial recipient resolution is fail-closed around the live-verified mobile behavior:

- Numeric recipients use the inherited `GET /users/{user-id}` hypothesis and require the response's immutable user ID to equal the requested ID.
- Username recipients use bounded `type=username` search and compare returned usernames case-insensitively. Search ordering alone never authorizes a recipient. One exact match is followed by detail-by-ID, which must return the same immutable ID and exact username before the recipient is usable; multiple exact IDs remain ambiguous.
- A missing exact match after verified exhaustion is not found. A missing match when the 200-record/four-page bound is reached remains incomplete and tells the user to use a numeric Venmo user ID. The bound may stop without exhaustion only when it has already found one exact match whose authoritative detail passes both identity checks.
- Username input is locally bounded to 1,023 bytes so the leading `@` plus username fits the 1,024-byte search-query bound.

### 3.6 Payment methods and balance

#### `venmo payment-methods list`

- Lists method ID, safe display name, type, masked last four digits when available, and default status.
- Includes the exact ID accepted by `pay --from` and `request accept --from`.
- Never displays full account or card numbers.

#### `venmo balance`

- Displays the user's Venmo wallet balance.
- Displays held or unavailable balance separately only if the API supplies a verified field with understood semantics.
- Does not claim to know external bank-account or card balances.
- Never infers a wallet balance from an unrelated payment-method limit.

### 3.7 Activity

#### `venmo activity list [--limit <N>] [--before-id <TOKEN>]`

- Lists recent activity visible to the authenticated account.
- Shows activity ID, timestamp, action/direction, counterparty, amount when available, status, and sanitized note.
- Fetches exactly one source page using `--limit` as the server page size, with default 10 and maximum 50.
- Accepts the endpoint-native bounded opaque `before_id` value through `--before-id` and reports a validated `Next before-id: <TOKEN>` value after successful record output.
- Does not expose a public or friends feed.

#### `venmo activity show <ACTIVITY_ID>`

- Shows all understood fields for one activity record.
- Is the primary recovery tool after an ambiguous payment, request creation, or request-acceptance outcome.
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
- Displays the canonical request ID accepted by `request accept`, direction, counterparty, amount, note, creation time, and status when supplied.
- Incoming pending records can be passed to `venmo request accept <REQUEST_ID>`; outgoing IDs and non-pending records are not acceptable.
- Does not decline, remind, or cancel a request in the first release.
- Fetches exactly one source page using `--limit` as the server page size, with default 10 and maximum 50. `--direction` is applied locally after that complete page is validated, so fewer than `--limit` rows may be rendered.
- Accepts the endpoint-native bounded opaque `before` value through `--before` and reports the source page's validated `Next before: <TOKEN>` even when local filtering leaves an empty result.
- Must be backed by a verified endpoint or a verified complete activity filter. A count without request records is insufficient.

### 3.9 Diagnostics

#### `venmo doctor`

Runs read-only checks and reports each independently:

1. Build and platform information.
2. Native credential-store availability.
3. Credential presence and schema validity.
4. Current-account authentication.
5. Connectivity and TLS.
6. Required private-API response shapes.

Rules:

- Writes the complete six-check report to stdout using explicit `Pass`, `Fail`, and `Skipped` states; dependency-blocked checks are `Skipped` rather than fabricated failures.
- Continues every independent check that can safely run. Connectivity remains independently testable without credentials; authenticated shape checks are skipped when credential/account validation fails.
- Treats every required check as healthy only when it passes, so any failure (and its dependent skipped checks) yields exit 1 after the report is written.
- Never creates, changes, or deletes a credential.
- Never creates a financial operation.
- Never prints a token, authorization header, raw response body, full device ID, or private note.
- Returns non-zero when a required check fails.
- Provides actionable remediation, especially for Linux Secret Service and API drift.

### 3.10 Shell completions

```text
venmo completions bash
venmo completions zsh
venmo completions fish
venmo completions powershell
```

- Generates completion source to stdout using `clap_complete`.
- Requires no authentication, keychain, or network access.
- Release archives should also include pre-generated completion files.

### 3.11 Local MCP server

```text
venmo-mcp
```

The initial server uses MCP over local stdio only. Stdout is exclusively the MCP JSON-RPC transport; startup failures, redacted diagnostics, and tracing go only to stderr. The server registers tools but no resources or prompts. Its registry is fixed before initialization for the lifetime of the process, so it does not advertise or emit dynamic tool-list changes. It performs no browser interaction, terminal prompting, credential mutation, financial mutation, hidden retry, or hidden pagination.

Before adopting the SDK transport, prove or add a bounded stdio boundary: each incoming JSON-RPC frame is at most 64 KiB before parsing, each serialized tool result is at most 2 MiB before stdout, and oversized input fails without logging or reflecting any payload. Allow one active Venmo tool call and no pending operation queue: a concurrent call receives a sanitized `server_busy` tool error rather than waiting unboundedly. Apply a per-process token bucket with capacity one and a two-second refill interval; excess calls receive `rate_limited` and are not delayed, retried, queued, or sent to Venmo. Initialization and tool discovery are not counted. If the reviewed SDK cannot enforce bounded frames without exposing raw payloads to logs, stop and resolve the transport design before implementation.

Initial tool catalog:

| Tool | Input | Structured result | Shared application behavior |
| --- | --- | --- | --- |
| `venmo_auth_status` | Empty object | Safe account identity, local credential saved-at time, credential format | Same current-account validation as `venmo auth status` |
| `venmo_balance` | Empty object | Exact available and on-hold USD strings | Same typed balance read |
| `venmo_friends_list` | `limit` 1–50 (default 10), `offset` `u32` (default 0) | User records and always-present nullable `next_offset` | Same one-page friends use case |
| `venmo_users_search` | Validated `query`, `limit` 1–50 (default 10), `offset` `u32` (default 0) | User records and always-present nullable provisional `next_offset` | Same one-page public search, not internal recipient resolution |
| `venmo_payment_methods_list` | Empty object | Safe method IDs, labels/types, optional last four, default marker | Same payment-method list |
| `venmo_activity_list` | `limit` 1–50 (default 10), nullable `before_id` (default null) | Tagged activity records and always-present nullable `next_before_id` | Same one-page activity list |
| `venmo_activity_show` | Validated `activity_id` | One tagged activity record | Same activity detail lookup |
| `venmo_requests_list` | `direction` (default `all`), `limit` 1–50 (default 10), nullable `before` (default null) | Pending-request records, applied direction, always-present nullable `next_before` | Same one source page and local direction filtering |
| `venmo_doctor` | Empty object | Overall health and six structured check results | Same read-only diagnostic orchestration |

Every tool declares strict `inputSchema` and `outputSchema`. Input DTOs use runtime unknown-field rejection and schemas with `additionalProperties: false`, then convert into existing domain types before any credential or network access. Output DTOs use string IDs, exact decimal money strings rather than floats, normalized RFC 3339 timestamps, explicit JSON `null` for absent values, tagged activity/counterparty variants, and always-present endpoint-native continuation fields whose values may be null. They never expose raw next URLs. Every success must conform to its declared `outputSchema` and should include the same JSON serialized as text content for clients that do not yet consume `structuredContent`.

Do not register tools for:

- `auth login`, token import, `auth reauthenticate`, `auth logout`, or remote revocation.
- Passwords, OTPs, OTP secrets, bearer tokens, device IDs, cookies, CSRF values, or alternate credential sources.
- Help, version, shell completion generation, or package management.
- `pay`, request creation, or request acceptance until their CLI contracts and the later MCP write phase are complete.
- Internal exact-recipient resolution, funding-method selection, pending-request detail, or any lower-level API merely because implementation code exists.

“Auth-status-only” describes the MCP authentication command surface: `venmo_auth_status` is the sole auth-oriented tool. `venmo_doctor` may still report its existing read-only credential-store, credential-presence/schema, and current-account checks, but it cannot mutate credentials or disclose authentication material.

#### 3.11.1 Tool annotations and permission signaling

Every initial tool must explicitly publish standard MCP annotations equivalent to:

```json
{
  "readOnlyHint": true,
  "destructiveHint": false,
  "openWorldHint": true
}
```

`openWorldHint` is true because each tool can contact Venmo and may return dynamic, user-authored external content. `idempotentHint` may be omitted for the read-only catalog because it is defined primarily for state-changing tools and this project does not want metadata to imply an automatic-retry policy. Each tool also has a clear human-readable title and description that states when it returns private financial, social, or account data.

Any future payment, request creation, or request acceptance tool must be annotated equivalent to:

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
- Declining, reminding, or cancelling pending requests; partial acceptance or changing a requested amount.
- Adding, removing, accepting, or blocking friends.
- Transfers or cash-out to a bank.
- Adding or removing payment methods.
- Public/friends audience selection.
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
    /// Write redacted diagnostics to stderr.
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Auth(AuthArgs),
    Pay(PayArgs),
    Request(RequestArgs),
    Friends(FriendsArgs),
    Users(UsersArgs),
    PaymentMethods(PaymentMethodsArgs),
    Balance,
    Activity(ActivityArgs),
    Requests(RequestsArgs),
    Doctor,
    Completions(CompletionsArgs),
}
```

Model `request` as a typed default create operation plus one nested operation:

```rust
#[derive(Debug, Subcommand)]
pub enum RequestOperation {
    Accept(AcceptRequestArgs),
}

pub enum RequestInvocation {
    Create(RequestCreateArgs),
    Accept(AcceptRequestArgs),
}
```

Use the same supported shape as `clap`'s documented Git-style default-operation pattern: direct-create arguments on the `request` command, an optional nested subcommand, `args_conflicts_with_subcommands`, and `subcommand_negates_reqs`. Dispatch must immediately convert the raw derive shape into `RequestInvocation`; no handler should receive a partially populated request-creation shape.

Requirements:

- Prefer typed fields and custom `FromStr` value types over parsing `Vec<String>` manually.
- Keep `PayArgs`, `RequestCreateArgs`, and `AcceptRequestArgs` separate after parsing so invalid options are impossible by construction.
- Prove that `request @user ...`, `request <numeric-id> ...`, `request @accept ...`, and `request accept <request-id>` dispatch exactly as documented.
- Reject mixed forms such as `request accept <id> <amount>`.
- Accept `--from` and `--yes` only on `pay` and `request accept`; reject both on request creation.
- Reject `--dry-run` everywhere; it is not part of the schema.
- Use `ValueEnum` for request direction and shell selection.
- Put validation that depends only on one value in `clap` parsers.
- Put cross-field and network-dependent validation in the application layer.
- Snapshot top-level and every subcommand's help output.
- Source version output from Cargo metadata; do not hardcode it separately.

`clap` does **not** provide interactive prompting. It parses arguments, validates the command shape, and generates help/completions. Use `std::io::IsTerminal` for TTY detection and `dialoguer` behind a narrow prompt adapter for hidden token entry, default-No confirmation, and funding-method selection.

This section defines only the `venmo` terminal adapter. `venmo-mcp` must not invoke the `clap` command dispatcher, parse terminal table output, or reuse human output writers. Its stdio stream is the protocol transport, so even a normal CLI banner or success message would corrupt framing. If a future startup policy flag such as `--allow-financial-writes` is added, parse only that server-composition setting before stdio service starts; it must never accept a secret or stand in for per-call human approval.

Interactive financial behavior is limited to `pay` and `request accept`. On a TTY, either command shows its complete plan and asks for default-No confirmation unless `--yes` is present. Off a TTY, either command fails before writing unless `--yes` is present. Request creation never prompts and does not accept `--yes`. `auth login` and `auth reauthenticate` remain intentional non-financial exceptions because credentials, OTP, and token import are interactive-only; neither has a `--yes` option.

Prompt adapter rules:

- Use `dialoguer::Input` for the account identifier, `dialoguer::Password` with result reporting disabled for password/OTP/bearer-token secrets, `dialoguer::Confirm` with `default(false)` for money-sending confirmation, and `dialoguer::Select` for funding methods.
- Render through `dialoguer::console::Term::stderr()` so prompts never contaminate stdout data.
- Use `interact_opt` for confirm/select so Escape or `q` maps to `Cancelled`. Map password interruption, Ctrl-C, EOF, and terminal failures into typed cancellation or terminal errors rather than panics; ordinary token text such as `q` remains valid password input.
- Use `SimpleTheme` by default. Any later color theme must follow the CLI's TTY and `NO_COLOR` policy.
- Sanitize every prompt and choice label inside the concrete CLI prompt adapter immediately before passing it to `dialoguer`; application code may provide typed/raw display fields but cannot bypass this terminal boundary.
- Keep `dialoguer` types out of the application and domain layers; tests should substitute a fake prompt port.
- Disable unused default features and enable only password support unless implementation proves another feature is required.

## 6. Bearer-token storage

### 6.1 Selected backend

Follow the same high-level model as `pmail`: use the native OS credential store behind narrow `CredentialStore` and read-only `CredentialReader` capabilities.

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
- Credential creation time.

Construct `Authorization: Bearer <token>` only at the HTTP boundary. The domain token type must have redacted `Debug`/`Display`, and logs and errors must never receive the exposed token value.

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
- `auth login` replaces a single targetable old entry only after the new token validates, then reads it back and verifies exact versioned content before reporting success. `auth reauthenticate` instead requires a readable entry and replaces it only after the new token validates as the same stored user while using the stored device ID.
- A valid different-account entry is never replaced implicitly. An unusable but targetable entry may be replaced by this explicit login operation; ambiguous or inaccessible entries may not.
- Decode the legacy TypeScript camelCase envelope for one-time migration.
- Prove on each supported platform whether Rust `keyring` can access the item written by `@napi-rs/keyring` under the same service/account.
- If transparent migration fails, do not delete the old item silently; document reauthentication or provide a narrowly scoped migration helper.

### 6.5 MCP credential capability boundary

`venmo-mcp` uses the same fixed service/account entry but receives only a read capability:

- Refactor all non-mutating application use cases, including auth status, to require `CredentialReader` rather than `CredentialStore`.
- Add a production `NativeCredentialReader` wrapper, or an equivalent concrete composition, that implements only `read_credential`; MCP server state must not have save/delete methods available by type.
- Split the current mixed `AuthenticationApi` into a read-only `CurrentAccountApi` and a mutating `TokenRevocationApi`; terminal login composes the read capability with `PasswordLoginApi`, while MCP receives neither revocation nor password/device-trust capability.
- Load and decode the credential separately for every tool call, use it only for that call, and drop it afterward. Never cache the bearer token, device ID, or whole `CredentialEnvelope` in the server.
- Constructing the server and handling `initialize` or `tools/list` must not instantiate a keyring entry, trigger an OS approval prompt, or make a network request.
- No MCP tool input, server flag, environment variable, config file, stdin payload, or alternate backend may supply or override authentication material.
- Missing, corrupt, locked, unavailable, ambiguous, or rejected credentials map to stable sanitized tool errors that direct the operator to the terminal `venmo` authentication commands without exposing platform source chains.
- Per-call loading lets an already-running MCP process observe a successful `venmo auth reauthenticate` on its next invocation without restart.

The second executable can have a distinct native-code identity from `venmo`. Validate macOS Keychain ACL/approval behavior manually with an isolated test service/account before claiming support. On Linux, an MCP host launched outside the desktop login session may lack Secret Service or user-session D-Bus even when the terminal CLI works. Fail closed with actionable setup guidance; never introduce a plaintext or environment-secret fallback. CI must not access any native production credential entry.

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

The private `jeffreyshi17/venmo-safe-mcp` project was then reviewed with authorization. It pins `venmo-api==0.3.1` and attempts the historical mobile flow: `POST /oauth/access_token` with identifier/password/device ID, SMS challenge via `/account/two-factor/token`, OTP completion through `/oauth/access_token`, and device trust through `/users/devices`. Its token-only mode merely imports an existing token. Its helper is not copied: it has no contract tests, relies on broad exception handling/private assumptions, stores secrets in a dotenv file, and appears incompatible with the pinned library's response type and OTP-secret handling.

The Rust CLI independently implements the narrow historical wire contract using typed, zeroizing values, fixed-origin transport methods, bounded responses, allowlisted response headers, native keyring persistence, and mock-server tests. A human-driven test on 2026-07-11 used a newly generated device ID and received a structured HTTP `403` with error code `240` before OTP or token issuance. No credential was stored. This matches public 2024 reports that Venmo rejects the historical password endpoint for untrusted device IDs; the code is generic and does not prove an incorrect password. Do not repeatedly retry that path.

The approved read-only experiment imported the current web session's short-lived `api_access_token` together with its matching `v_id`, both through hidden local prompts, then performed only current-account validation before storage. The user retrieved both values manually; they never entered source, command arguments, chat, logs, screenshots, or agent output. Validation failure would have stored nothing. Even the successful result does not establish a durable authentication contract: enabled financial commands must not rely on an imported web token until its lifetime and `/v1` compatibility are independently proven. Browser-cookie storage and automatic browser extraction remain out of scope. The later financial validations used the separately verified mobile-issued credential path.

That import experiment succeeded on 2026-07-11: the matching web `api_access_token` and `v_id` pair authenticated the inherited mobile-private current-account read and the exact versioned credential was stored in the OS keychain. An immediate separate `auth status` process reloaded the keychain entry and repeated the current-account read successfully with stdout suppressed. No secret value or account identity is retained in this plan. This proves only immediate cross-API compatibility for that one read; observed expiry, refresh behavior, and every other endpoint remain separate gates.

Follow-up public-source research on 2026-07-11 found no evidence of an endpoint that accepts an existing web `api_access_token` plus `v_id` and exchanges them for a distinct persistent mobile bearer token. Current and historical authentication source in `zackhsi/venmo`, `aaymeloglu/venmo-mcp`, and `adbertram/cli-tools`, together with the complete discussion in `mmohades/Venmo#86`, showed only direct use of an existing bearer token or a separate username/password login. The archived 2024 web bundle identifies `api_access_token` and `v_id` as cookie names and calls `GET /api/auth` to recover session fields including `accessToken`; that is session introspection, not token exchange. No speculative exchange route, request, or live probe will be added.

The only evidenced route for minting the historically reusable mobile token is a credential login using a device identifier that Venmo already trusts. It sends one `POST /v1/oauth/access_token` request with the trusted browser `v_id`/`deviceId` as the sensitive `device-id` header and JSON fields `phone_email_or_username`, `client_id: "1"`, and `password`; a successful response contains `access_token`. If Venmo returns the recognized `81109` challenge, the historical flow requests SMS through `/v1/account/two-factor/token`, submits the OTP to `/v1/oauth/access_token?client_id=1`, and then best-effort trusts the device through `/v1/users/devices`. Public reports from March through December 2024 say that a `v_id` or `/api/auth` `deviceId` obtained after a completed browser login allowed this flow where a generated ID failed, and the June 2026 `aaymeloglu/venmo-mcp` instructions independently switched from generated IDs to a browser-derived device identifier for the same reason. The web access token is not an input to this issuance flow.

Those sources are unofficial and do not prove present-day token lifetime. The historical client says a newly issued mobile token remains valid until explicit logout, and issue participants reported reusing such tokens, but no controlled current expiry test or official guarantee was found. The web cookie's archived 30-minute `maxAge` also does not establish the underlying bearer's lifetime. On 2026-07-11 the owner selected a single trusted-device mobile-login experiment: password login hidden-prompts for a manually obtained trusted browser device ID instead of generating one. It performs one non-retried issuance attempt, follows OTP only after the exact recognized challenge, validates any issued token with the current-account endpoint, stores nothing before validation, and emits no credential, identifier, header, body, or account data. The existing Rust transport and application flow enforce the remaining issuance safeguards; a live attempt occurs only while the owner enters secrets locally.

That single trusted-device mobile-login experiment succeeded on 2026-07-11. The initial credential request issued a bearer token without an SMS-OTP challenge, the current-account validation passed, and the exact credential was saved and reloaded from the OS keychain. The then-current implementation made an unnecessary post-save `/users/devices` request even after direct password success; Venmo rejected only that redundant request with HTTP 400 and code `81124`. No credible public mapping for `81124` was found, so the code does not assign it a guessed meaning. The historical reference flow calls `/users/devices` only after OTP, and the Rust orchestration now likewise treats direct password success as proof that the existing trusted device was accepted and skips the redundant mutation. The issued token remained valid throughout this outcome.

On 2026-07-11, one separately authorized attempt to enroll a generated CLI device through `/users/devices` using the valid mobile token was definitively rejected with HTTP 400 and error code `81124`. There was no retry, no token was issued by that enrollment attempt, and the pending local experimental keychain entry was deleted. Together with the earlier generated-device password-login rejection with code `240`, this does not establish any fresh-device bootstrap path. The temporary ignored enrollment test and its direct development-only UUID dependency were removed; production authentication continues to require and preserve a device identity Venmo already trusts.

An authorized one-shot stored-device reauthentication experiment then consumed only the `USERNAME` and `PASSWORD` fields from the owner's local 1Password `Venmo` item in bounded process memory. No field value, 1Password session token, device ID, bearer token, request body, response body, or account identity entered arguments, environment variables, files, logs, chat, or tool output. The experiment reused the existing Keychain device ID internally; Venmo issued a replacement token directly without OTP, same-account current-account validation passed, and the replacement credential was saved and read-back verified. A separate normal `auth status` process then reloaded and validated it successfully with stdout suppressed. The temporary 1Password probe was removed; production `auth reauthenticate` uses only interactive hidden prompts and does not invoke `op`. This proves an email/password-only user experience after trusted-device bootstrap, not headerless authentication, a fresh-install bootstrap, a refresh grant, unattended renewal, or guaranteed lifetime. The prior bearer's remote state was not established, and automatic revocation is unsafe while the private endpoint's scope is undocumented; successful reauthentication therefore warns that the previous token may remain valid and points to official session controls.

A subsequent authorized fresh-device experiment established why browser-derived IDs work. A normal Chrome process with an isolated temporary profile received a new secure `v_id` before authentication. After the owner completed Venmo's official login and security challenge, that exact ID was captured only in local bounded process memory and supplied to one mobile password-login initiation. Venmo issued a mobile bearer directly without OTP; same-account current-account validation, Keychain replacement/readback, and a separate rebuilt `auth status` process all succeeded. This proves that completing the current web identity flow can make a newly minted web `v_id` acceptable to the mobile-private password endpoint. It does not establish a standalone device-registration request or make browser-assisted bootstrap an acceptable product design.

The same session's approved local network capture showed the current trust-establishment state machine. `venmo.com` redirects through `account.venmo.com` to `id.venmo.com`; an initial identity request is gated by DataDome, followed by reCAPTCHA Enterprise and PayPal FraudNet/browser-risk collection. The identity application then performs GraphQL public-credential and password verification, creates and completes an MFA/authflow challenge, and finalizes through `authflowNonceGql`. When remembering is enabled, that final mutation calls the same private-credential resolver with `userPreferences.isRememberedDevice: true`. It remains bound to same-origin cookies, CSRF, `_sessionID`, `ctxId`, refreshed `rData`/`fn_sync_data`, and prior server-side challenge state. The account callback subsequently establishes web access/identity credentials, and `/api/auth` exposes the resulting account session. `/api/device-data` carries a separate risk correlation and is not a `v_id` registration endpoint.

This is not a reproducible pure-HTTP CLI contract. Before credential verification, the server requires JavaScript-generated DataDome state, reCAPTCHA output, FraudNet synchronous and asynchronous telemetry, browser fingerprints, timing/input signals, and opaque session-correlated risk state. Synthesizing, replaying, or impersonating those values would be an anti-bot/security-control bypass, not a supported login implementation, and would be brittle across platforms. No OAuth device-authorization grant, verification URL/user code, magic-link bootstrap, refresh grant, or portable remember-device mutation was found. The owner explicitly rejected shipping a browser-assisted bootstrap, so production first login continues to require a manually obtained trusted device ID. Only evidence of a legitimate current native-mobile onboarding contract or an official device-authorization flow would reopen that decision.

The raw capture was kept only in a mode-0700 temporary directory with mode-0600 files, never printed through agent output, and deleted immediately after local analysis; the temporary Chrome profile and all experimental capture code were also removed. The owner elected to rotate exposed login/session material after the experiment and should review official remembered-device and session controls.

Immediately afterward, separate processes successfully used the mobile-issued token for `auth status`, `balance`, bounded `friends list`, bounded `users search`, `payment-methods list`, bounded `activity list`, bounded `requests list`, and `doctor`, with sensitive stdout discarded. This proves authentication compatibility across every currently implemented read-only adapter and native keychain reload. It does not establish token lifetime or authorize any financial-write contract; no financial mutation was attempted.

The same credential pair was then exercised against every currently implemented read-only API command. `auth status`, bounded `users search`, and `payment-methods list` all succeeded. User search confirmed the inherited endpoint is fuzzy even when the query begins with `@` and can have more than ten matches; no ordering or exact-lookup guarantee may be inferred from the first result. The implementation preserves that exact query distinction in every public page request and in the separate internal exhaustive traversal. Payment-method listing returned multiple method classes and one default-role indication, proving the current list shape but not transaction eligibility, fees, fallback, or actual funding behavior. Live output exposed a duplicated `@` rendering bug; it was fixed and regression-tested. No account identity, user result, payment-method identifier, institution name, or last-four value is retained here.

The root Rust package implements `friends list`, `balance`, `activity list/show`, `requests list`, and `doctor` behind typed application ports and strict response mapping. Their synthetic HTTP/application/output test suites pass, and each command completed a read-only live smoke test with sensitive stdout suppressed. Shell completion generation remains service-free.

### 7.2 New read capabilities required by the redesign

| CLI capability | Historical lead | Required validation |
| --- | --- | --- |
| Friends | `GET /users/{user-id}/friends` in historical unofficial docs | Auth headers, pagination, visibility, response envelopes |
| Balance | Account and/or payment-method responses have historically exposed Venmo balance | Exact source field, decimal semantics, held balance, missing balance behavior |
| Activity list | `GET /stories/target-or-actor/{user-id}` in historical unofficial docs | Authenticated visibility, pagination, amount/status fields, private records |
| Activity detail | `GET /stories/{activity-id}` in historical unofficial docs | ID semantics, status, amount, counterpart, errors |
| Pending requests | No sufficiently reliable record-list contract identified yet | Find a request-record endpoint or prove activity filtering is complete |
| Pending request detail | No sufficiently reliable direct lookup contract identified yet | Canonical request ID, ownership/direction, current state, requester, immutable amount/note, not-found and stale behavior |

Historical research sources, including [mmohades/VenmoApiDocumentation](https://github.com/mmohades/VenmoApiDocumentation), are leads only. They are old, unofficial, and cannot be treated as a current contract. Do not copy source from third-party clients with incompatible licenses.

#### 7.2.1 Dated sanitized read-contract evidence

On 2026-07-11, a purpose-built ignored test used the authorized keychain credential to issue bounded, fixed-origin, authenticated GET requests and emitted only structural schema facts. It retained no raw body, token, device ID, account/user/request/activity ID, name, note, amount, timestamp, URL, or screenshot. The first run received `401` after the imported web token rotated; the owner refreshed only the hidden token, and the immediate second run established these candidate mobile-private `/v1` contracts:

- `GET /account` returned HTTP 200 with string fields `data.balance` and `data.balance_on_hold`, plus `data.user`. These are the sole sources for the signed exact-USD **Available** and **On hold** wallet values; external balances are never inferred.
- `GET /users/{self-user-id}/friends?limit=<N>&offset=<N>` returned HTTP 200 with `data: User[]` and `pagination.next/previous`. The next link used the same fixed origin and route and carried only `limit` and `offset` query fields.
- `GET /stories/target-or-actor/{self-user-id}?limit=<N>&social_only=false` returned HTTP 200 with story records and pagination. A story had a canonical top-level story ID, creation timestamp, private audience, note, and nested payment containing its own ID, `status`, `action`, positive amount, actor, target user, audience, and timestamps. Its next link used the same route and the fields `before_id`, `limit`, `only_public_stories`, and `social_only`.
- The observed activity timestamps use `YYYY-MM-DDTHH:MM:SS` without an offset. The project owner chose to interpret this legacy Venmo shape as UTC and render normalized RFC 3339 `Z` output; explicit-offset RFC 3339 values are also accepted. This assumption is documented and remains a release-time semantic check rather than silently assigning the local timezone.
- A bounded 200-record follow-up established three current story classes: peer `payment`, wallet `transfer`, and card `authorization`. Transfer records identify exactly one external `source` for incoming `add_funds` activity or one external `destination` for outgoing transfers, plus amount, status, type, request timestamp, account name/type, and optional last four. Authorization records identify the authenticated user, merchant display name, amount, status, and creation time. The typed activity model preserves those distinctions, renders transfer type in the action, uses an external sanitized counterparty label, and rejects ambiguous source/destination or wrong-account authorization records.
- `GET /stories/{story-id}` returned HTTP 200 with the corresponding story shape. A competing `GET /payments/{nested-payment-id}` returned HTTP 500 for the observed settled activity, so `activity show` uses canonical story IDs and the story-detail route. A successfully retrieved pending, failed, expired, or cancelled record remains command data and exits 0; its authoritative status is not converted into a CLI failure.
- `GET /payments?action=charge&status=pending,held&limit=<N>` returned HTTP 200 with actual payment/request records and pagination, not counts. Records had a canonical request/payment ID, `action=charge`, pending-state status, actor, target user, positive amount, note, audience, and dates. Direction is derived relative to the authenticated user: actor=self is outgoing and target.user=self is incoming. The next link used the fixed payments route and only `action`, `before`, `limit`, and `status`.
- `GET /payments/{request-id}` returned HTTP 200 with the corresponding pending charge record and additional fee/medium/refund fields. The read model accepts only `action=charge`, `status=pending|held`, exactly one authenticated-account party, a positive exact amount, and an exact returned request ID. Acceptance mutation semantics remain wholly unverified.

The Rust adapters encode these findings with endpoint-specific opaque tokens, exact path/query/header assertions, strict typed IDs/money/timestamps, exact detail-ID checks, and same-origin/same-route continuation validation. Synthetic fixtures cover malformed records, unsupported state/action values, duplicate/conflicting IDs, oversized pages, and untrusted next links. Each public application call buffers one complete source page before stdout, applies request direction locally, preserves server order and source continuation, and rejects repeated/no-progress values. Only internal exact-recipient resolution performs a bounded multi-page traversal.

This evidence proves immediate read compatibility only. The imported web token rotates quickly, current mobile-client header/User-Agent behavior remains unverified, final-page exhaustion should receive an additional live boundary check, and no financial write may rely on these findings without completing its separate Phase 0 contract.

After implementation, read-only live smoke tests succeeded for `auth status`, `friends list`, `balance`, `activity list`, `activity show` using canonical payment/transfer/authorization story IDs selected only in process memory, `requests list` with all/incoming/outgoing filters, and `doctor`. Sensitive command output was discarded and no identifiers or account data were retained. The first activity runs safely exposed offsetless timestamps, capitalized `False` continuation booleans, and deeper transfer/authorization records. The owner selected UTC semantics for the legacy timestamp form; the parser now supports it, continuation booleans are validated case-insensitively without relaxing their value, all three observed activity classes have synthetic contract regressions, and repeat live runs succeeded. A historical bounded friends traversal reached verified exhaustion, while the larger activity dataset was not claimed complete.

On 2026-07-11, the owner explicitly authorized read-only live pagination validation using the active mobile-issued credential. After the public CLI was aligned with endpoint-native pagination, a one-record first page exposed a validated continuation and a second process successfully supplied the exact native `--offset`, `--before-id`, or `--before` value for each of `friends list`, `users search`, `activity list`, and `requests list --direction all`. A separate bounded friends traversal used 50-record source pages and reached a real final page with no continuation. All record output and continuation values were suppressed and discarded; no account data, query result, identifier, note, amount, or continuation value was retained, and no mutation endpoint was called. This confirms current first-to-second endpoint continuation behavior for all four adapters and final-page signaling for friends. Natural final-page exhaustion for user search, activity, and pending requests remains a release observation rather than a reason to poll aggressively.

### 7.3 New write capability required by request acceptance

The former TypeScript client and the historical public leads establish only how to create a payment or create a negative-amount request. Targeted public research on 2026-07-09 did not identify a sufficiently reliable contract for accepting an existing incoming request.

Phase 0 must independently discover and validate:

- The canonical request identifier and a complete lookup path.
- The acceptance method, path, required body, and concurrency or state preconditions.
- Proof that the operation settles the identified request rather than creating an unrelated payment.
- How an explicit funding-source ID is supplied and how the actually selected source is verified.
- Success response identifiers and the resulting request/activity state transition.
- Confirmed stale/already-settled errors versus transport outcomes that remain ambiguous.
- Whether the operation has an idempotency key or conditional-state mechanism; regardless, the client performs no automatic write retry.

Do not infer that a normal payment to the requester is equivalent to acceptance. If a complete, controllable, and verifiable acceptance contract cannot be established, `request accept` is a first-release blocker requiring an explicit scope decision.

#### 7.3.1 Dated public write-contract evidence and selected discovery limit

Public-source research completed on 2026-07-12 produced the candidate contracts below. Controlled live validation later that day supplied the release proof described after them:

- The maintained `joshhubert-dsp/py-venmo` fork at commit `899cd288d11d3599176ff3b8d40236a7d4886027` states that it targets the mobile API and that payments work. Its payment path first posts JSON to `/v1/protection/eligibility` with blank `funding_source_id`, `action: "pay"`, country code, target type/ID, note, and integer-cent amount. A positive payment then posts JSON to `/v1/payments` with a client UUID, user ID, `audience: "private"`, decimal-dollar amount, note, eligibility token, and funding-source ID. Its response model expects `data.payment`. This is the strongest current public pay lead, but the fork's February 2026 `Venmo/26.1.0` compatibility header is older than the App Store's July 2026 version and must not be copied as a present-day header contract.
- The same fork creates a request through `/v1/payments` with a client UUID, user ID, private audience, negative decimal-dollar amount, and note, omitting eligibility and funding fields. Retired official documentation independently used a negative amount for charge creation. This is a candidate request-creation contract only.
- Retired official Venmo documentation archived in 2021 described `PUT /v1/payments/{payment}` with `action: "approve"` to complete an incoming request. Current maintained public clients implement only reminder/cancellation updates, and the 2026 web `RedeemablePayment` GraphQL flow is an opaque-token/iMessage flow rather than the canonical pending-request API. No current acceptance body, funding, atomic-state, response, or stale-state contract is established.
- Current Venmo help states that available Venmo balance is used before a selected linked bank or card. Historical client reports independently observed a supplied funding-source ID being overridden by balance. The first release therefore presents `--from` as a backup preference and never claims an actual debit source without authoritative result evidence.
- A read-only live CLI preflight resolved the exact `@georgecma` search match through an authoritative user-detail fetch, proved a personal/payable non-self target, excluded wallet balance from external backup selection, and received a transaction-specific eligibility result totaling exactly zero fee. The fuzzy search reached its bounded traversal limit, so the verified rule now accepts one exact username match only after detail-by-ID returns the same ID and case-insensitive exact username; no match still fails when traversal is incomplete.
- After separate immediate approval, the Rust `pay` command sent exactly one private $0.01 payment with a unique generated note and zero retries. Its response matched `data.payment` and the submitted action, parties, amount, note, private audience, supported status, creation timestamp, and canonical ID. A separate CLI activity read found the matching latest one-cent pay, and the owner independently confirmed exactly one matching payment in the official Venmo mobile app.
- After the payment was reconciled and a second immediate approval was obtained, the Rust request command sent exactly one private $0.01 request with a distinct generated note and zero retries. Its response passed the same operation-specific strict checks, a separate CLI request read found the matching pending outgoing request, and the owner independently confirmed exactly one matching request in the official mobile app. No token, device ID, user/payment/request ID, note, raw body, account identity, funding details, or mobile traffic was retained.

The project owner declined mobile-app traffic capture. Do not install a proxy certificate or intercept the official mobile app. Pay and request creation are now verified through contract-independent code, exact synthetic tests, one separately approved controlled mutation per operation, strict response validation, CLI read reconciliation, and manual official-app confirmation. Request acceptance remains unavailable until a new credible public/static source establishes its current complete contract; the historical `approve` hypothesis alone must not be probed live.

### 7.4 Establishing verified HTTP request shapes

For this plan, an HTTP request shape means the complete wire contract: HTTPS origin, method, route template, query fields, required header names and authentication model, content type, request-body field names and types, amount units, identifiers, state/version or idempotency fields, expected response statuses and body schema, and the authoritative state transition caused by the operation.

Phase 0 must produce a dated, sanitized contract dossier for every API operation. Recorded behavior from the removed TypeScript implementation and historical documentation are hypotheses, not sufficient evidence. For an operation exposed by the current Venmo web application, the preferred discovery workflow is:

1. Prefer the official Venmo web application in a dedicated visible/headed browser session using `agent-browser --headed`; live web discovery must not run headlessly. If Venmo blocks its security challenge, do not bypass it: close the browser and use only the already authenticated CLI for bounded reads plus manual official-mobile-app identity/result verification, without capturing mobile traffic.
2. Prompt the prompter/project owner to complete Venmo login manually in that visible browser, then pause until they explicitly confirm login is complete. The agent must not request, read, type, or store the username, password, MFA code, or other login secret and must not inspect or capture login traffic.
3. Verify the intended active account after login, then clear the tracked request log. Record the relevant read-only pre-state immediately before the one action and capture only `fetch`/XHR traffic associated with it.
4. Read-only exploration may exercise account, user/friend search, payment-method, balance, activity, and request listing/detail views. For a financial action, obtain explicit human approval immediately before the agent-browser click and follow Section 13.6. Request acceptance must use only a fresh $0.01 incoming request from the authorized `@georgecma` counterparty.
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
- Use only the controlled live-mutation protocol in Section 13.6 for payment, request, and acceptance discovery. Every live amount is exactly **$0.01**, and the only approved counterparty is `@georgecma` after the account owner authorizes the test session.
- Reconcile every discovery write through the official app and activity/request reads before another attempt.
- Capture the minimum sanitized fixtures necessary for contract tests.
- Remove access tokens, device IDs, user IDs, names, notes, payment IDs, account fragments, and other personal data from fixtures.
- Never commit raw discovery responses.
- Do not inspect or repurpose historical local response caches without first establishing a safe redaction workflow.
- A count such as `outgoing_requests_count` does not satisfy `requests list`; actual records are required.
- `request accept` requires a fresh authoritative request record; a feed summary, count, or guessed association is insufficient.
- If pending-request records or their acceptance contract cannot be retrieved reliably, treat that as a release-scope blocker requiring an explicit product decision rather than fabricating results or sending a substitute payment.

### 7.6 Application-facing API ports

Application interfaces express product operations rather than arbitrary HTTP paths. The implemented `application::ports` module deliberately uses narrow traits instead of one monolithic API object:

- `CurrentAccountApi`, `TokenRevocationApi`, and `PasswordLoginApi`, with no mixed read/revocation authentication trait.
- `PaymentMethodsApi`, `UsersApi`, `FriendsApi`, and `BalanceApi`.
- `ActivityApi`, `RequestsApi`, and `DoctorApi`.
- Endpoint-scoped page request/result and continuation types.

`VenmoApiClient` may implement several of these traits, while each application use case and fake requires only the capability it consumes. Add a `ReadOnlyVenmoApi` production façade with a private inner client that forwards only `CurrentAccountApi`, `PaymentMethodsApi`, `UsersApi`, `FriendsApi`, `BalanceApi`, `ActivityApi`, `RequestsApi`, and `DoctorApi`; MCP server state holds this façade rather than a directly reachable `VenmoApiClient`. The MCP adapter should remain generic over this concrete read-only façade and call the shared use cases; do not add a second MCP-specific HTTP client or collapse the return-position `impl Future` traits into one large dynamic service solely for protocol convenience. Pure domain and keychain work stays synchronous where appropriate.

## 8. Architecture

Use one Cargo package with a shared library target and two thin binary targets:

```text
Cargo.toml
Cargo.lock
rust-toolchain.toml
src/
  lib.rs
  error.rs
  main.rs                 # venmo terminal entry point
  bin/
    venmo-mcp.rs          # stdio composition only
  cli/
    args.rs
    completions.rs
    dispatch.rs
    output.rs
    prompt.rs
  mcp/
    mod.rs
    server.rs
    tools.rs
    views.rs
    error.rs
    services.rs           # optional production composition
  application/
    auth.rs
    activity.rs
    balance.rs
    doctor.rs
    friends.rs
    funding.rs
    payment_methods.rs
    ports.rs
    read.rs
    recipients.rs
    requests.rs
    users.rs
  domain/
    account.rs
    activity.rs
    balance.rs
    credential.rs
    identifiers.rs
    login.rs
    money.rs
    note.rs
    pagination.rs
    payment_method.rs
    pending_request.rs
    recipient.rs
    user.rs
  infrastructure/
    credentials/
      codec.rs
      native.rs
    logging.rs
    system.rs
    venmo_api/
      client.rs
      dto.rs
      transport.rs
tests/
  cli.rs
  domain.rs
  errors.rs
  process.rs
  mcp_protocol.rs
  mcp_process.rs
```

Set package `default-run = "venmo"` and explicitly declare both binaries. Adding the MCP entry point must not make existing `cargo run -- ...` commands ambiguous. The binary files contain process composition only; reusable MCP behavior belongs in the library.

### 8.1 Layer boundaries

#### CLI

- Defines the `clap` command schema.
- Converts raw values into typed application input.
- Handles `std::io::IsTerminal` checks, the narrowly allowed prompts, result rendering, and exit codes.
- Does not construct HTTP payloads or contain financial policy.

#### MCP

- Defines the local stdio server, exact tool registry, protocol schemas, tool annotations, and explicit MCP view DTOs.
- Converts protocol input into existing validated domain types and invokes the same application functions as CLI dispatch.
- Converts typed application results into structured protocol output without parsing or reusing terminal rendering.
- Owns no Venmo business rule, private HTTP DTO, credential mutation, retry loop, or financial authorization policy.
- Treats stdout as an exclusive protocol channel and all Venmo-returned names, notes, and labels as untrusted external data.

#### Application

- Orchestrates each command.
- Loads credentials only when required.
- Resolves recipients and funding sources before any applicable money-sending confirmation.
- Depends on narrow API, credential, prompt, clock, and ID interfaces.
- Returns typed results rather than printing directly.
- Requires only `CredentialReader` for every read-only use case. Credential mutation remains confined to explicitly mutating terminal authentication operations.
- Supports static/generic composition over the existing return-position `impl Future` ports; do not replace the narrow ports with one large object-safe service trait merely for MCP.

#### Domain

- Contains validated value types and financial-operation policy.
- Has no dependency on `clap`, `reqwest`, `keyring`, Tokio, or terminal rendering.
- Represents money as checked integer cents, never binary floating point.

#### Infrastructure

- Implements private HTTP endpoints and DTO mapping.
- Implements the sole native keyring adapter.
- Provides a read-only native credential capability for MCP without save/delete methods.
- Initializes redacted tracing.
- Contains platform-specific behavior behind narrow interfaces.

### 8.2 Core domain types

- `AccessToken`: secret value with redacted formatting.
- `CredentialEnvelope`: versioned stored session.
- `Account`, `User`, `UserId`, and `Username`.
- `RecipientInput`: exact username or numeric user ID.
- `Money`: positive checked integer cents.
- `PaymentMethod` and `PaymentMethodId`.
- `Balance`: available amount and optional verified held amount.
- `Activity` and `ActivityId`.
- `PendingRequest`, `RequestId`, and `RequestDirection`.
- `PayPlan`, `CreateRequestPlan`, and `AcceptRequestPlan`.
- `WriteOutcome`: confirmed result, confirmed failure, or ambiguous outcome.
- `Limit`: positive server page size bounded to 50, and `Offset`: a nonnegative `u32`.
- `ActivityBeforeId` and `RequestsBefore`: endpoint-scoped, bounded continuation inputs with redacted formatting, plus internal endpoint page request/result types.

### 8.3 MCP runtime and service composition

The stdio server is long-lived, unlike the terminal dispatcher:

- Start one multi-thread Tokio runtime with only the additional features required by the reviewed MCP stdio transport, synchronization, and server tasks.
- Construct one shared production `ReadOnlyVenmoApi` façade backed privately by `VenmoApiClient`; constructing either must not contact Venmo. Share concrete generic state with `Arc` where required rather than introducing a global singleton or exposing mutating client capabilities to MCP.
- Keep a one-permit `tokio::sync::Semaphore` in server state. After schema/domain validation, use non-waiting acquisition before rate admission or any keychain/Venmo operation; if the permit is unavailable, return `server_busy` immediately without consuming rate budget. This prevents unbounded pending calls, concurrent OS approval prompts, credential races, private-API rate bursts, and overlapping future financial operations.
- Do not hold a standard mutex across `.await`, spawn detached retries, or let cancellation launch a replacement operation.
- Reload credentials inside each permitted invocation. If synchronous native keychain access proves disruptive, load it through `spawn_blocking` and add narrow application seams that accept a loaded credential; do not make all domain or API ports async merely to accommodate keyring I/O. Move the owned operation permit into the blocking task and return it with the loaded credential, so cancellation of the awaiting MCP request cannot release the permit while keychain work continues. A dedicated serialized credential worker is the fallback if the SDK/runtime cannot preserve that lifetime.
- Verify production server state is `Send + Sync`. Cancellation of an MCP request must stop further local work where possible, while accurately acknowledging that synchronous keychain work or an already-transmitted HTTP operation may not be cancellable.

### 8.4 MCP serialization boundary

MCP has explicit input and output DTOs deriving only the serialization and JSON Schema traits required by the reviewed SDK. Keep these types in `src/mcp/`; do not add broad serialization derives to domain or credential types for convenience.

Never serialize or schema-expose:

- `AccessToken`, `DeviceId`, `CredentialEnvelope`, or `LoadedCredential`.
- `LoginIdentifier`, `AccountPassword`, `OtpCode`, `OtpSecret`, or password-login state.
- Raw HTTP request/response DTOs, headers, bodies, URLs, keychain source errors, or transport internals.

Explicit MCP views may contain validated account/user identities, safe payment-method metadata, exact balance strings, structured activities, pending requests, diagnostic checks, and endpoint-native continuation values. Arbitrary MCP-provided IDs must receive a defensible byte bound and existing whitespace/control/invisible-character validation before network access; current continuation inputs retain their strict 1,024-byte redacted boundary.

## 9. Dependency plan

Select current compatible versions during scaffolding, minimize features, commit the lockfile, and record the MSRV.

### 9.1 Library-first policy

Use an established community crate by default for non-domain functionality that is common across Rust applications. Do not build project-owned substitutes for capabilities such as argument parsing, hidden prompts, HTTP/TLS, URL handling, serialization, native credential storage, terminal capability detection, logging, shell completions, manpage generation, secure temporary files, date/time handling, signal handling, backoff primitives, or test servers when a suitable mature crate exists.

This policy is not a mandate to maximize dependency count. Keep Venmo-specific domain rules, financial-safety decisions, typed adapter glue, and small straightforward transformations in this repository. A new dependency must still reduce total implementation and maintenance risk compared with the local code it replaces.

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
| Completions | `clap_complete` | Confirmed |
| Manpages | `clap_mangen` | Generate release documentation |
| Runtime | `tokio` 1.52 | CLI keeps its current-thread runtime; MCP additionally needs reviewed stdio I/O, sync, and multi-thread runtime features |
| HTTP/TLS | `reqwest` 0.13.4 with `rustls` | Confirmed; defaults disabled and only JSON plus rustls enabled |
| Serialization | `serde`, `serde_json` | Endpoint DTOs separate from domain |
| MCP protocol | Official `rmcp` crate | Planned; server/macros/stdio transport only after published-version/MSRV review |
| MCP schemas | `schemars` through the SDK-compatible version | Explicit MCP input/output JSON Schema only; never credential/domain secrets |
| Terminal tables | `tabled` 0.21.0 | Confirmed; defaults disabled and only `std` enabled; all cells sanitized before rendering |
| Errors | `thiserror` | Typed categories and source chains |
| UTC timestamps | `time` | Typed UTC instants and RFC 3339 storage/output; no local-time dependency |
| Persistent secrets | `keyring` default `v1` API | Confirmed sole backend |
| Secret memory handling | Redacted newtype; evaluate `zeroize` | Not a persistence backend |
| IDs | Validated domain newtypes | Device IDs are imported or reused, never generated by the CLI |
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
- Audit the published stdio transport's buffering and logging before acceptance. The production adapter must enforce the 64 KiB inbound frame bound and 2 MiB serialized-result bound from Section 3.11; an SDK path with unbounded line buffering is not acceptable without a bounded wrapper.
- Disable SDK/transport frame and payload tracing targets in production at every verbosity. A malformed JSON-RPC line, schema failure, or protocol error must never cause raw input/output to be written to stderr.

The implementation targets the [MCP specification dated 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25) unless the reviewed stable SDK requires a newer compatible revision. Record any spec-version change and rerun exact schema/protocol snapshots before release.

## 10. HTTP and resilience policy

- The production base is fixed in code to `https://api.venmo.com/v1/`; alternate HTTP origins exist only inside loopback tests.
- Use a 10-second connection timeout, 15-second per-read timeout, and 30-second total request deadline. Revisit only with endpoint evidence and tests.
- Bound every response body to 2 MiB while streaming it; reject an oversized declared or observed body before unbounded buffering.
- Authenticate only endpoints that require it.
- Send token and device ID only in the required headers.
- The selected API family is the bearer-token/device-ID mobile private API, so do not send a browser User-Agent. Until Phase 0 captures or validates a current mobile request contract, preserve the former TypeScript client's historical compatibility headers: `Accept: application/json` and `Venmo/7.44.0 (iPhone; iOS 13.0; Scale/2.0)`. Treat that exact old mobile User-Agent as a dated hypothesis that must be revalidated or replaced before release, not as a permanent compatibility promise.
- Build endpoint URLs from validated path segments and query pairs; never accept an arbitrary authenticated URL.
- Disable redirects, automatic referer generation, system/environment proxies, and gzip/Brotli/Zstandard/deflate decompression in the first release. A future proxy feature requires an explicit product/security decision because it exposes authenticated traffic to the configured proxy.
- Use rustls with reqwest's native platform verifier. Do not enable native-tls or OpenSSL.
- Disable every reqwest-layer retry, including protocol-NACK retries. A separately tested endpoint-aware layer may retry only verified idempotent reads.
- Log route template, method, status, duration, retry count, and sanitized API code in verbose mode.
- Never log authorization values, full device IDs, request notes, raw bodies, or credential payloads.

### 10.1 Pagination policy

Pagination applies to `friends list`, `users search`, `activity list`, and `requests list`. Single-resource commands and `payment-methods list` are not paginated unless Phase 0 proves that their verified endpoint requires it.

User-visible contract:

- Every public invocation fetches exactly one source API page. `--limit <N>` is that server request's page size, defaults to 10, and has a hard maximum of 50.
- Friends and user search accept only a typed nonnegative `u32` `--offset`, defaulting to 0. Activity accepts only its endpoint-native `--before-id <TOKEN>`; pending requests accept only their endpoint-native `--before <TOKEN>`.
- When a validated continuation remains, successful records stay on stdout and exactly one copyable native value is written to stderr after record rendering succeeds: `Next offset: <N>`, `Next before-id: <TOKEN>`, or `Next before: <TOKEN>`. Raw next URLs are never output.
- There is no universal continuation input, page number, page-size alias, or public multi-page collector. Payment methods and single-resource reads remain unchanged.
- `ActivityBeforeId` and `RequestsBefore` are nonempty, bounded to 1,024 bytes, reject whitespace, control, and invisible format-control characters, and redact `Debug`, `Display`, and parse errors. Application output has narrow crate-private access solely to print a successful copyable next value.
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

Internal exact-recipient traversal:

- Exact username resolution uses a separate, non-public user-search traversal with no user-supplied continuation.
- It requests at most four source pages of at most 50 records, buffers at most 200 unique users, validates increasing offsets and per-page bounds, deduplicates identical IDs, rejects conflicts and no-progress pages, and preserves source order.
- Verified exhaustion authorizes normal not-found handling. If the traversal reaches its record/page bound, it may authorize only one already-found exact username match and only after detail-by-ID returns the same immutable ID and exact username; no exact match remains incomplete with guidance to use a numeric user ID, and multiple exact matches remain ambiguous.
- No public list command calls this exhaustive traversal.

For any future paginated endpoint, exact defaults, per-page sizes, hard maxima, ordering, token/offset mapping, and reliable end signaling remain Phase 0 decisions. The selected contracts above remain release-gated where explicitly noted; a missing or unreliable pagination contract blocks claims of completeness.

### 10.2 Retry policy

- The first release performs no automatic retries, including for read-only pagination.
- A network failure, timeout, rejection, malformed response, or continuation failure ends the invocation with its typed error.
- Never retry payment creation, request creation, or request acceptance automatically. Any future retry proposal requires an explicit endpoint-specific product decision and new safety tests before implementation.

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

- Each MCP tool call executes the same application use case and exact private-API request policy as its CLI equivalent. MCP is not a second HTTP implementation.
- After strict input validation, acquire the one-operation permit without queuing; return `server_busy` when another operation owns it. Apply rate admission only after acquiring the permit, and release it immediately on `rate_limited`. Hold an admitted permit through credential loading, application completion, and bounded result conversion.
- Public list tools fetch exactly one source page. Never hide pagination, follow all pages, refill a locally filtered request page, or convert endpoint-native continuations into a generic MCP cursor.
- The server and shared HTTP transport perform zero automatic retries. Tool annotations, host retries, reconnects, and repeated JSON-RPC IDs do not authorize replay.
- A cancellation before network transmission should stop the operation cleanly. Any blocking credential task retains the operation permit until it actually finishes even if its requester is cancelled. Cancellation or transport loss after a future write might have been sent is an ambiguous financial outcome; never report it as a normal cancelled failure or start a replacement call.
- Malformed tool input must fail before credential/network access. A later-page concept does not exist for initial tools because each invocation is exactly one page.
- The stdio connection may remain alive after an application/tool error. A protocol framing failure, impossible server invariant, or stdout write failure is a fatal server error and must not be hidden behind a successful tool result.

## 11. Financial-write safety model

### 11.1 New payment or request preflight

Before writing, and before the confirmation prompt for `pay`:

1. Parse one recipient.
2. Resolve that recipient to exactly one Venmo user.
3. Parse the amount into checked integer cents.
4. Validate the non-empty note.
5. Verify a personal, payable, non-self recipient for the exact operation.
6. Resolve the strictly peer-eligible backup funding method for `pay`.
7. Prove that every applicable fee is exactly zero; missing or unknown fee semantics fail closed.
8. Read the wallet balance required for the balance-priority disclosure.
9. Build an immutable private-audience plan.
10. For `pay`, render a sanitized complete summary including zero fee, exact total, wallet balance, backup method, and balance-priority warning before confirmation.

If any preflight step fails, send no write request.

Request creation has no interactive confirmation. Once its preflight succeeds, execute its one request-creation write immediately and render the confirmed result afterward. Its explicit recipient, amount, and required note are the complete user authorization for that non-money-sending operation.

### 11.2 Request-acceptance preflight

Before prompting or writing:

1. Parse one canonical request ID.
2. Load the authenticated account identity.
3. Fetch the authoritative request record by ID.
4. Verify that the record is incoming, pending, payable, and addressed to the authenticated account.
5. Require complete, understood requester, amount, note, status, profile type, payability, and ownership fields; never guess missing safety-critical values. Until independently proven otherwise, only exact `pending` is payable and `held` is read-only data.
6. Resolve one peer-eligible backup funding method using the same precedence as `pay`.
7. Prove a zero fee and obtain the wallet balance required for balance-priority disclosure.
8. Build an immutable acceptance plan tied to the fetched request ID and observed state.
9. Render a sanitized complete summary that explicitly says this operation pays the requester and that wallet balance may precede the submitted backup method.

An outgoing, already-settled, cancelled, declined, inaccessible, wrong-account, or malformed request sends no write. The client must use the verified acceptance operation and must not translate the record into an ordinary recipient payment.

The verified server mutation must atomically enforce the request ID's payable state, or expose a conditional token/version that the client sends. A client-side preflight followed by an unconditional unrelated write is not sufficient protection against a concurrent state change.

### 11.3 Execution rules

Execution rules:

- One invocation creates or accepts at most one financial operation.
- For `pay` and `request accept`, prompt only when both stdin and stderr are terminals and default to No.
- For `pay` and `request accept`, require `--yes` unless both stdin and stderr are terminals; on a terminal it skips only the final confirmation.
- Request creation never prompts, never requires `--yes`, and rejects that flag.
- Send exactly one verified financial mutation for payment creation, request creation, or request acceptance.
- Preserve the original API or transport cause on failure.
- Never claim rollback or retry a write.
- On ambiguous outcome, do not print a false failure or success.
- A Ctrl-C, SIGTERM, or SIGHUP received before the protected write region keeps normal process interruption semantics. Once the financial future may have started, those catchable termination signals drop local waiting and return the ambiguous-write exit code `3`; they never report ordinary cancellation or authorize a retry, and the user must reconcile through reads and the official app. Uncatchable termination such as SIGKILL cannot provide an exit warning, so external supervisors must never retry financial commands automatically.
- On confirmed request-state conflict, report the current understood state and direct the user to refresh `requests list`.

### 11.4 Future MCP financial-write gate

The initial MCP server registers no financial tools. A later financial MCP phase cannot begin until the corresponding CLI application operations and private-API contracts have satisfied Sections 7, 11, and 13.6. Even then, financial tools remain absent from `tools/list` by default.

Future registration requires all of the following:

1. An explicit non-secret server startup opt-in such as `--allow-financial-writes` that changes the registered tool set for that process.
2. A named, tested, trusted MCP host integration that places a human approval step on each specific execute tool and displays the exact server-resolved immutable plan rather than only the model's original arguments.
3. A two-phase prepare/execute protocol, or an equivalently strong host-attested protocol, that binds the approval to account, resolved recipient/request identity, amount, note, funding source, operation type, and observed request state.
4. Server-side one-operation serialization, no retry, replay/cancellation analysis, and ambiguous-outcome handling equivalent to or stricter than the CLI.

Startup opt-in, standard annotations, a model statement, or a model-supplied field such as `confirmed: true` is never proof of human authorization. Do not add an MCP equivalent of CLI `--yes`. If the chosen local host cannot provide trustworthy per-call human approval, write tools remain unregistered even when startup opt-in is requested.

For a prepare/execute design, the read-only prepare tool resolves and returns the complete immutable plan plus a random process-local opaque handle. The server retains a matching plan for at most five minutes. The execute input repeats every human-visible plan field plus that handle so the host can display the exact values; the server requires byte-for-byte/typed equality with its retained plan, the same authenticated account, unexpired single-use state, and a fresh authoritative preflight. Any changed recipient, amount, note, funding source, request status, or account invalidates the plan and requires a new preparation and approval. The handle is not authorization by itself, is never logged, and becomes invalid on use, timeout, cancellation before execution, or server restart. Do not silently refresh or rewrite a plan after the human approved it.

Every future payment, request-creation, and request-acceptance tool must be marked `readOnlyHint=false`, `destructiveHint=true`, `idempotentHint=false`, and `openWorldHint=true`. These hints help trusted hosts display policy and warnings but do not replace capability restriction or approval. The tool executes at most one write and must not be retried by the server after timeout, cancellation, disconnect, protocol reconnect, or unknown outcome.

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
- Source-chain and request metadata only with `--verbose`.
- No secrets at any verbosity.
- Sanitize all API, user, friend, activity, request, note, payment-method, and keyring-originated strings before human terminal output.
- Sanitize display only; preserve the intended note payload sent to the API.
- Avoid panics for expected input, API, keyring, network, and terminal failures.

### 12.1 MCP output, errors, and untrusted content

- Reserve MCP-process stdout exclusively for SDK-managed JSON-RPC framing. Never write banners, terminal tables, prompt UI, tracing, panic reports, diagnostics, or ad hoc acknowledgements to stdout.
- Send redacted tracing and fatal startup diagnostics only to stderr. Do not log tool arguments, structured results, account/user/payment/request IDs, notes, continuation values, keychain source chains, or protocol payloads.
- Filter SDK and stdio transport logging independently from application verbosity so `--verbose` can never enable raw JSON-RPC/frame diagnostics.
- Return successful data through explicit structured output conforming to the declared `outputSchema`; provide an equivalent serialized JSON text content item only for MCP client compatibility, not a second independently formatted representation.
- Map domain/application failures to stable categories such as `invalid_input`, `authentication_required`, `credential_unavailable`, `server_busy`, `rate_limited`, `network`, `timeout`, `remote_rejected`, `remote_contract`, `pagination_contract`, and `internal`. Return normal application and validated-input failures as tool execution errors with `isError: true`; malformed JSON-RPC, unknown tools, and invalid protocol shapes remain protocol errors.
- Never expose `Debug`, nested source chains, raw platform errors, raw response text, input values, query text, IDs, notes, or continuation tokens in an error. Messages should be static or category-based and provide a safe remediation where known.
- Treat Venmo notes, names, merchant labels, institution labels, and all other remote text as untrusted external content and potential prompt injection. Keep it only in clearly named structured fields, rely on correct JSON escaping, and never interpolate it into tool descriptions, server instructions, errors, annotations, logs, or authorization decisions.
- MCP hosts may persist tool inputs and results in model context, transcripts, telemetry, or cloud logs. Documentation must prominently warn that read-only tools expose private account, financial, social, and transaction data even though they do not mutate Venmo.
- Standard annotations describe side effects and open-world behavior but provide no universal sensitive-data classification and are not enforceable permissions. The server's static tool registry, startup policy, type-level credential capabilities, and host approval configuration are the actual controls.

## 13. Testing strategy

### 13.1 Domain tests

- Amount syntax, bounds, formatting, and checked arithmetic.
- Recipient syntax and exact username matching.
- Funding-source selection precedence.
- Pay, request-creation, and request-acceptance plan invariants.
- Request-state transition and authenticated-target invariants.
- Native page-size and offset bounds, strict redacted before-token parsing, duplicate/conflict handling, and continuation no-progress detection.
- Terminal sanitization.
- Error-to-exit-code mapping.
- Property tests for money parse/format round trips.

### 13.2 `clap` tests

Use `Cli::try_parse_from` and help snapshots to cover:

- Every command and subcommand.
- Required recipient, amount, and note.
- Direct request creation versus the nested `request accept` grammar, including the valid `@accept` recipient.
- `--from` and `--yes` accepted by `pay` and `request accept` but rejected by request creation.
- `--dry-run` rejected by every command.
- Request acceptance's required request ID and rejection of recipient, amount, and note arguments.
- Absence of an undocumented `request create` alias.
- Direction and shell enums.
- Limit bounds.
- Exact endpoint-native pagination grammar, default limit/offset values, rejection of cross-endpoint or page/page-size forms, and redacted before-token debug/error formatting.
- Absence of old `init`, `deinit`, `charge`, split, token-source, and multi-recipient forms.
- Help/version paths that never initialize keyring or HTTP dependencies.

### 13.3 Application tests with fakes

- Login is interactive and hidden-prompt only.
- Reauthentication fails before credential/network access when noninteractive, requires one readable stored credential, reuses its exact device ID without prompting, and never requires the old token to validate.
- Direct reauthentication performs exactly one password-login initiation and no device-trust call; the exact recognized challenge alone enables the one SMS-OTP path and its post-save best-effort trust call.
- Reauthentication account mismatch, current-account validation failure, and invalid prompt input leave the old credential untouched; every save/read-back uncertainty is reported as unknown and prevents device trust.
- Invalid token is never saved.
- A valid replacement preserves the old credential until validation succeeds.
- Corrupt credentials can be replaced and deleted.
- Auth status identifies the active account.
- Friends/search output exposes copyable recipient identifiers.
- Every public paginated application flow makes exactly one source request, uses `--limit` as its page size, validates and buffers the page before output, preserves source continuation after local request-direction filtering, and rejects oversized/conflicting/no-progress pages.
- The separate internal exact-recipient traversal remains limited to four 50-record pages/200 unique users and accepts no public continuation. Exhaustion is required for normal not-found; one exact match found at the bound still requires matching authoritative detail-by-ID before use.
- Balance does not infer unsupported values.
- Funding method is resolved and displayed before `pay` or request-acceptance confirmation.
- Cancelled `pay`/acceptance prompts and all preflight failures send zero writes.
- `pay` and `request accept` prompt default-No on a TTY and require `--yes` off a TTY.
- Request creation never prompts and works off a TTY without `--yes`.
- The `dialoguer` adapter sends prompts to stderr, never echoes the bearer token, maps default Enter to No, and distinguishes cancellation from terminal failure.
- First write failure preserves its source.
- Ambiguous writes are never retried.
- Newly created requests carry no funding-source ID.
- Request acceptance uses the exact server-side requester, amount, note, and request ID rather than caller-supplied substitutes.
- Only an incoming, pending request addressed to the active account can reach the acceptance write.
- Outgoing, stale, settled, cancelled, declined, wrong-account, incomplete, and malformed request records send zero writes.
- Request-acceptance funding is resolved and displayed before confirmation.
- A request-state conflict is reported as a confirmed failure; an uncertain transport outcome is classified as ambiguous and is not retried.
- Confirmed acceptance output is tied to the requested ID and does not overstate settlement when the server reports only pending/unknown state.
- Doctor is read-only and redacted.

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
- Zero retries for payment creation, request creation, and request acceptance.
- Ambiguous write classification.

Friends, balance, activity, activity detail, pending requests, pending-request detail, and request acceptance require dedicated sanitized fixtures before their commands are considered implemented.

### 13.5 Native and packaged tests

- Keyring read/write/delete smoke tests on each supported platform using a test-only service name.
- Legacy `@napi-rs/keyring` migration test where feasible.
- Pseudo-terminal prompt smoke tests for hidden token input, default-No confirmation, explicit yes/no, funding selection, cancellation, EOF, and stderr-only rendering.
- Compiled-binary help, version, exit code, stdout/stderr, and redaction tests.
- Completion generation for every supported shell.
- Manpage generation.
- Release archive contents and execution after unpacking.
- Isolated second-executable Keychain/Secret Service smoke tests using a randomized test-only service/account; never production `venmo-cli` / `default` credentials in CI.
- Packaged `venmo-mcp` initialize and `tools/list` smoke tests with no keychain or network access.
- No live financial writes in automated CI.

### 13.6 Controlled live-mutation protocol

Automated tests use fakes and mock HTTP servers; their synthetic amounts are not constrained by this section. Any test against the live Venmo service remains read-only by default. A live financial mutation is a manual, explicitly gated exception and must follow all of these rules:

- Prefer the Venmo web application with `agent-browser --headed`. Prompt the prompter/project owner to log in manually in the visible browser and wait for their confirmation before testing. Never ask them to send login credentials through chat, automate credential/MFA entry, inspect login traffic, or persist the authenticated browser state in the repository. If Venmo blocks the security challenge, do not bypass it; close the browser and use bounded authenticated CLI reads plus manual official-mobile-app verification without capturing mobile traffic.
- After login, read-only tests may inspect the active account and exercise user/friend search, payment-method, balance, activity, and request list/detail workflows. Clear any network capture only after login is complete and keep all observed data under the redaction rules in Section 7.4.
- Use exactly **$0.01 USD** per live payment or request—never a larger amount. If Venmo rejects one cent or requires a higher minimum, stop and report the capability as blocked; never increase the amount to make the test pass.
- Send test money only to `@georgecma` and create test requests only from `@georgecma`, and do either only after the owner of that account has explicitly authorized the current test session.
- Resolve `@georgecma` to one exact account before each session and verify the displayed username, display identity, and immutable user ID against the official Venmo application. Keep the expected ID only in ignored local test configuration; never commit it or any returned user record.
- Test sending with one $0.01 payment to `@georgecma`.
- Test receiving by creating one $0.01 request from `@georgecma` (with `@georgecma` as the request target); any fulfillment by the counterparty must also be exactly $0.01.
- Test request acceptance only from a fresh $0.01 incoming request created by `@georgecma` for the authenticated test account.
- Use a unique, non-sensitive test note so identical one-cent operations can be reconciled, but never commit the note or raw response.
- Run one mutation at a time. Require an explicit human go-ahead immediately before invocation and never fuzz, loop, schedule, or batch-probe a financial route. Prefer the default-No terminal prompt. When the trusted execution harness has no TTY, `--yes` may carry only the immediately preceding human approval for that exact rendered preflight; it is never standing, unattended, inferred, or reusable authorization.
- After each mutation, verify the counterparty, amount, request/payment ID, status, audience, and funding result through the CLI reads and official Venmo application before continuing.
- If any result is ambiguous, stop immediately. Do not retry or begin another mutation until the prior outcome is reconciled independently.
- Never upload or commit tokens, device IDs, user IDs, notes, payment/request IDs, raw responses, screenshots containing account data, or live-test logs.
- Close the dedicated browser session when testing is complete unless the prompter/project owner explicitly asks to keep that local session open; never save or commit its cookies, local storage, or authentication state.

The literal test username in this protocol is the only approved personal identifier in repository documentation. It does not authorize committing any other account data or including live data in fixtures.

### 13.7 MCP schema, protocol, and process tests

MCP tests must be service-free unless a separately ignored native test explicitly says otherwise:

- Unit-test every input DTO conversion and output view, including exact decimal money, string IDs, RFC 3339 timestamps, explicit nulls, tagged activity/counterparty forms, request direction, and endpoint-specific continuation field names.
- Snapshot the exact nine-tool allowlist, titles, descriptions, JSON Schema 2020-12 input/output schemas, defaults, required/nullable fields, and annotations. Every input schema must set `additionalProperties: false`, and runtime deserialization must reject unknown fields. No auth mutation, secret, financial, internal-helper, resource, or prompt surface may appear.
- Exercise `initialize`, `tools/list`, and representative `tools/call` requests through the SDK's in-process or duplex transport using fake services. Initialization and listing must prove zero credential-reader and API calls.
- Build handler tests around a fake that implements only `CredentialReader`, `CurrentAccountApi`, and the required read API traits—not `CredentialStore`, `TokenRevocationApi`, or `PasswordLoginApi`—so MCP code cannot compile if it attempts local or remote authentication mutation. Verify production state holds only the read-only façade.
- Prove each invocation reloads the credential and that a changed fake credential is observed on the next call; no credential material remains cached in server state.
- Prove non-waiting one-permit admission returns `server_busy` for overlap, creates no pending queue, does not hold a standard mutex across await, and retains the permit until any cancelled `spawn_blocking` credential operation actually finishes.
- Prove the capacity-one/two-second-refill limiter returns `rate_limited` without sleep, queue, retry, credential access, or network access, and that initialization/tool discovery do not consume its budget.
- Reject inbound frames above 64 KiB before JSON parsing and serialized results above 2 MiB before stdout. Verify neither payload is reflected or logged and malformed SDK/protocol traffic cannot enable raw frame tracing at any verbosity.
- Test stable sanitized tool-error categories and verify invalid arguments fail before credential/network access without echoing the rejected value.
- Test hostile notes, names, and labels containing newlines, terminal controls, bidi/invisible controls, quotes, JSON-looking text, and prompt-injection instructions. They must remain correctly escaped data fields and never alter framing, metadata, logs, or errors.
- Launch the compiled `venmo-mcp` process and prove stdout contains only valid protocol frames while stderr contains no tool inputs, results, secrets, identifiers, notes, or continuations.
- Add compile-time `Send + Sync` checks for production server state and explicit regression tests for structured output matching `outputSchema` plus the equivalent JSON text fallback.
- Do not access the native keychain, Venmo, browser sessions, authentication endpoints, or financial endpoints in normal MCP tests or CI.

## 14. Quality and security gates

Required in CI:

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
- Dependency updates through reviewed automation.
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
4. The unsupported legacy password/SMS-OTP login flow, including exactly what is sent to Venmo, what is never stored, how `--token` imports an existing bearer token without exposing it as an argument, and how explicit stored-device `auth reauthenticate` performs full issuance rather than a nonexistent refresh.
5. Explicit warnings that account credentials and the resulting token are equivalent to powerful secrets and must never be shared, logged, committed, or passed as shell arguments.
6. `venmo auth login`, stored-device `venmo auth reauthenticate`, trust-device warnings, status, logout, and revocation behavior.
7. Friends and user-search examples, including server-page `--limit`, typed `--offset`, copyable next offsets, one-page behavior, and changing-dataset/no-snapshot semantics.
8. Payment-method and balance examples.
9. Pay, request-creation, and request-acceptance examples; `--from`; confirmation and `--yes` rules for money-sending commands; and request creation's immediate no-prompt behavior.
10. Activity and pending-request examples, including server-page bounds, native before-token notices, sparse locally filtered pages, and how to copy a canonical incoming request ID into `request accept`.
11. Ambiguous financial-write recovery steps, including checking both activity and request state before retrying acceptance.
12. Doctor and troubleshooting guidance.
13. Completion installation.
14. Local MCP installation and host configuration that launches `venmo-mcp` over stdio with no secret environment variables, after authenticating through `venmo`.
15. The exact read-only MCP tool catalog, structured result and one-page pagination behavior, standard annotations, and the fact that annotations are hints rather than permissions.
16. Prominent MCP transcript/privacy and prompt-injection warnings: hosts may retain private financial/social output, and remote notes/names are untrusted data.
17. The absence of MCP credential mutation and initial financial tools, plus the requirements for any later startup-gated/human-approved write phase.
18. Upgrade, uninstall, credential removal, and how a running MCP server observes CLI reauthentication on the next call.

Documentation must never suggest placing the password, OTP, OTP secret, device ID, or bearer token in a command argument, environment variable, file, browser automation, chat, screenshot, or log. Password/OTP entry occurs only in the CLI's interactive prompts and only after an explicit `auth login` or `auth reauthenticate` invocation.

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
- Shell completions.
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

- Freeze the terminal command surface and help text in Sections 3.1–3.10 and the MCP tool/schema/annotation surface in Section 3.11.
- Verify token-retrieval steps for the README without automating them.
- Validate the legacy password/SMS-OTP token issuance, device trust, and existing-token import paths without storing password or OTP material.
- Validate current-account, search, user, payment-method, revoke, payment-creation, and request-creation contracts.
- Discover and validate friends, balance, activity list/detail, pending-request list/detail, and request acceptance.
- For operations exposed by the Venmo web application, use the sanitized `agent-browser` network-observation workflow in Section 7.4 to establish the proper request shape; never commit or emit raw captures.
- Produce a dated sanitized contract dossier, typed DTO outline, fixture, and exact mock-server assertion for every retained API operation.
- Prove that accepting by canonical request ID settles that request, supports a known funding source, and has well-understood stale and ambiguous outcomes.
- Perform any necessary live mutation only under the exactly-$0.01, `@georgecma` protocol in Section 13.6.
- Produce fully sanitized fixtures.
- For every paginated endpoint, verify ordering, continuation or offset semantics, reliable end/has-more signaling, filtering behavior, default and hard limits, per-page size, and maximum page/request bounds.
- Prove or reject legacy keychain interoperability.

Exit criteria:

- Every first-release command has a credible API or local implementation path.
- `requests list` has actual records and canonical IDs, not only counts.
- A complete request lookup and acceptance contract has been verified without substituting an ordinary payment.
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
- Initial CI and dependency policy.

Exit criteria:

- Every selected command parses.
- Old command forms are rejected intentionally.
- Help, version, and completions require no services.

### Phase 2: Domain and HTTP foundation

Deliverables:

- Domain value types and pagination model.
- `reqwest` client, deadlines, response limits, and enforced zero-retry policy.
- Endpoint DTOs and mapping.
- Mock-server contract suite.

Exit criteria:

- Domain invariants and all verified read contracts pass.
- Financial-mutation retry count is provably zero regardless of HTTP method.

### Phase 3: Keyring and authentication

Deliverables:

- Sole `keyring` adapter.
- `dialoguer` prompt adapter and interactive hidden-token prompt.
- Validation-before-save.
- Password/SMS-OTP `auth login`, hidden `auth login --token` import, stored-device full-issuance `auth reauthenticate`, `auth status`, and `auth logout`.
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
- `payment-methods list`.
- `balance`.
- Sanitized output and truthful pagination.

Exit criteria:

- Output provides recipient identifiers accepted by `pay`/request creation and funding IDs accepted by `pay --from`/`request accept --from`.
- No command silently truncates without saying so.
- Public `--limit` is enforced as the one-request endpoint page size, and any validated native continuation is reported without describing the page as the complete collection.

### Phase 5: Activity, request reads, and diagnostics

Deliverables:

- `activity list` and `activity show`.
- `requests list` with direction filtering.
- Pending-request detail lookup used by acceptance preflight.
- `doctor`.
- Ambiguous-outcome recovery messaging.

Exit criteria:

- Activity can be used to inspect a known record.
- Pending-request output is based on complete verified records and prints IDs accepted by the planned mutation.
- Activity and request pagination obeys the one-page endpoint-native limit, ordering, local-filtering, and continuation policy in Section 10.1.
- Doctor is read-only and leaks no sensitive data.

### Phase 6: Pay, request, and request acceptance

Deliverables:

- Single-recipient payment and request-creation models plus the single-ID `request accept` model.
- Recipient and funding-source resolution.
- Authoritative incoming-request lookup and state validation.
- Complete default-No confirmation rendering for `pay` and request acceptance; no prompt for request creation.
- One-write execution, stale-state handling, and ambiguous-outcome handling.

Exit criteria:

- Every preflight failure sends zero writes.
- `pay` and request-acceptance confirmation contains every safety-critical field and defaults to No.
- Request creation exposes neither `--yes` nor an interactive confirmation.
- No write is automatically retried.
- Acceptance settles the identified pending request and cannot silently become an unrelated payment.
- No multi-recipient, split, or `charge` surface exists.

### Phase 7: Documentation and release engineering

Deliverables:

- Complete README, including verified password/SMS-OTP login behavior and existing-token import.
- Manpage and completion generation.
- LICENSE and security policy.
- Release archives containing every completed binary, checksums, and native smoke tests.

Exit criteria:

- A user can install, retrieve and store a token, find a recipient, inspect funding, safely perform a payment, create a request, and safely accept an incoming request using only shipped documentation.
- Every declared release target passes keyring and all shipped-binary smoke tests.

### Phase 8: Root Rust-only cutover (completed 2026-07-12)

- Promoted the Rust package, lockfile, toolchain, policy, source, and tests to the repository root.
- Removed the Node/pnpm/TypeScript implementation, SEA packaging, generated `dist`, dependency/build output, and obsolete product caches from the working tree.
- Updated CI, ignore rules, package metadata, and development documentation to operate from the root Cargo package only.
- Kept legacy credential-envelope decoding solely for one-time native-keychain compatibility; no legacy runtime or source tree ships.
- Added explicit TypeScript-to-Rust credential and command migration notes to the root README, including removed financial interfaces and the absence of a legacy fallback.

### Phase 9: Initial local read-only MCP server

Deliverables:

- Refactor auth status and every non-mutating application entry point from `CredentialStore` to `CredentialReader`, split `AuthenticationApi` into current-account and revocation capabilities, and add production read-only credential/API façades without changing CLI behavior.
- Add defensive bounds for every arbitrary ID accepted through MCP before any credential or network access.
- Review and lock the official `rmcp` stable release and minimal stdio/server/schema feature set against Rust 1.95, supported targets, and cargo-deny.
- Add explicit MCP input/output DTOs, JSON Schema, exact money/time/ID conversions, tagged activity views, safe error mapping, and hostile-content tests.
- Add the exact nine-tool registry in Section 3.11 with accurate annotations and no resources, prompts, auth mutation, secret input, internal helper, or financial tools.
- Add generic handlers over the existing application/API ports, one shared production read-only API façade, per-call credential loading, and a one-permit async operation gate.
- Add bounded 64 KiB ingress, 2 MiB serialized-result enforcement, non-waiting `server_busy` admission, capacity-one/two-second-refill rate limiting, and SDK payload-log suppression.
- Add the `venmo-mcp` stdio binary with protocol-only stdout and stderr-only diagnostics while preserving `default-run = "venmo"`.
- Add protocol, process, schema, permission-matrix, no-cache, concurrency, cancellation, nonleakage, and service-free startup/list tests.
- Update CI, release packaging, README host configuration, transcript/privacy warnings, and troubleshooting for the second executable.
- Manually validate the second executable's native credential access with an isolated service/account on supported platforms.

Exit criteria:

- `initialize` and `tools/list` succeed without keychain or network access and list exactly the nine read-only tools.
- Every tool calls the same application use case as its CLI counterpart, preserves one-page endpoint-native pagination, and returns schema-valid structured output.
- Server state has no credential save/delete capability and caches no bearer token, device ID, or loaded credential between calls.
- Stdout contains only valid MCP protocol frames; stderr and tool errors contain no arguments, results, account data, notes, continuations, or secrets.
- Exact tool schemas and annotations are regression-tested, and no tool capable of mutation is registered.
- macOS and Linux CI build/test both binaries, cargo-deny passes, and packaged stdio smoke tests remain service-free.
- A user can authenticate with `venmo`, configure a trusted local MCP host to launch `venmo-mcp` without secret environment variables, and invoke the documented reads.

### Phase 10: Future opt-in financial MCP tools (deferred and gated)

Entry criteria:

- Phase 6 has complete verified CLI application/API contracts for the exact proposed MCP write tools, including stale-state, no-retry, and ambiguous-outcome behavior.
- A named supported MCP host exposes a trustworthy approval policy on the execute tools and can present every server-resolved safety-critical field immediately before execution.
- The project owner explicitly approves the concrete tool schemas, host integration, startup flag, replay/cancellation model, and documentation.

Potential deliverables only after those gates:

- A non-secret `--allow-financial-writes` startup mode whose tool registry is absent by default and fixed for that server process.
- A short-lived, single-use, process-local prepare/execute plan protocol (or reviewed stronger attestation) that binds host approval to the immutable shared application plan.
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

- All CLI commands in Sections 3.1–3.10 and the initial MCP tools in Section 3.11 are implemented and documented.
- No old `init`, `deinit`, `charge`, split, or multi-recipient interface remains.
- `auth login` prompts for the identifier, hidden password, and hidden trusted device ID in that order, and handles SMS OTP when required; `auth login --token` prompts for an existing bearer token and, when needed, its matching device ID. Neither mode accepts secrets through arguments, stdin flags, environment variables, or files.
- Interactive prompts use the tested `dialoguer` adapter and never echo token material or write prompt UI to stdout.
- README token-retrieval steps are verified and carry prominent secret-handling warnings.
- The token is stored only in the native OS credential store through `keyring`.
- Friends and search output provide copyable recipient identifiers.
- Payment-method output provides IDs accepted by `pay --from` and `request accept --from`.
- Balance semantics are verified and do not overclaim external balances.
- Activity supports list and detail views usable for write reconciliation.
- Pending requests are based on complete verified records and expose canonical IDs accepted by `request accept`.
- Payment and request-acceptance confirmation follows recipient/request and funding-source resolution.
- Only `pay` and `request accept` expose `--yes` or use financial confirmation; request creation prompts for neither.
- No command exposes `--dry-run`.
- `request accept` validates an authoritative incoming pending record and settles that exact record through a verified contract.
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
- Unit, property, application, HTTP, keyring, CLI, and release smoke tests pass.
- Packaged verbose logging works and leaks no secrets.
- Production and release paths require only the Rust/Cargo toolchain and documented native platform services.
- No live financial write occurs in automated CI.
- Every manually approved live payment, request, or acceptance test uses exactly $0.01 and the authorized `@georgecma` counterparty under Section 13.6.
- No material contradiction or ambiguity discovered during implementation is resolved without clarification from the prompter/project owner and a recorded plan/test update; finished CLI users are never asked to make implementation decisions.
- `venmo` and `venmo-mcp` are thin adapters over the same shared application, domain, credential, and private-API implementations; MCP does not parse terminal output or duplicate HTTP contracts.
- The initial MCP registry exposes exactly the nine Section 3.11 read tools, no resources/prompts, no auth mutation or secret input, and no financial/internal-helper tools.
- Every initial MCP tool has accurate read-only/non-destructive/open-world annotations, strict input/output schemas, and structured results using exact money, time, ID, tagged activity, and native continuation representations.
- MCP initialization and tool listing are keychain/network-free; every tool reloads credentials per call through read-only capability and leaves no credential cached.
- Type-level MCP state exposes neither local credential mutation nor remote login/trust/revocation capabilities.
- Bounded frames/results, non-waiting one-operation admission, conservative rate limiting, and cancellation-safe permit ownership prevent unbounded buffering, queues, and overlapping keychain/Venmo work.
- MCP stdout is protocol-only, diagnostics are stderr-only, and hostile remote content remains labeled JSON data rather than instructions, metadata, errors, or logs.
- Documentation warns that annotations are hints, read-only results remain private/sensitive, and MCP hosts may retain tool data in transcripts or telemetry.
- Both executables are built, tested, packaged, and smoke-tested on supported targets. Financial MCP tools remain absent unless every Phase 10 gate is met.

## 19. Principal risks

| Risk | Mitigation |
| --- | --- |
| Private endpoints changed or disappear | Phase 0 validation, isolated DTOs, sanitized fixtures, doctor diagnostics |
| Pending request records or acceptance are unavailable | Treat as explicit release blocker; never substitute counts, incomplete guesses, or an ordinary payment |
| Live discovery moves money | Use only the Section 13.6 protocol: exactly $0.01 with authorized `@georgecma`, one immediately confirmed mutation at a time, no unattended authorization, and immediate reconciliation |
| A timed-out write actually succeeded | Never retry; classify as ambiguous; use activity detail, request state, and the official app for verification |
| Wrong recipient | Exact resolution and one recipient; show identity before default-No `pay` confirmation and in the confirmed request-creation result |
| Wrong funding source | Resolve before confirmation, support `--from`, display safe method label and ID |
| Wrong, stale, or already-settled request | Fetch authoritative record, require incoming/pending/current-account ownership, confirm all immutable fields, and handle state conflict without a retry |
| Token exposure | Hidden prompt, keyring-only persistence, redacted token type, no body/header logs |
| Malicious terminal text | Sanitize every human-rendered untrusted string |
| Rust cannot read the legacy Node-created keychain item | Native migration spike, reauthentication documentation, no silent deletion |
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
| Read-only MCP code can reach auth revocation/trust methods | Split current-account and mutation ports, hold only read-only credential/API façades, and compile-test fakes without mutating traits |
| Host approval covers model input but not the resolved write plan | Require a short-lived single-use prepare/execute plan, repeat exact fields in host-visible execute args, reject any drift, and keep writes blocked without a named tested host policy |

## 20. Remaining decisions

The CLI grammar and initial read-only MCP tool set are settled. Remaining implementation/release decisions are:

1. Natural final-page release observation for user search, activity, and pending requests, plus best-effort native continuation behavior as each private endpoint's dataset changes. First-to-second endpoint continuation is live-verified for all four paginated commands, and friends exhaustion is live-verified.
2. Whether transparent legacy keychain migration is possible on every supported platform.
3. Whether the first release adds Linux arm64, musl, or Windows.
4. GitHub-only initial distribution versus Homebrew and crates.io at launch.
5. Whether JSON output for the terminal CLI belongs in the next release or a later milestone; MCP structured output does not resolve that separate decision.
6. What product decision to make if complete pending-request list/detail or acceptance contracts cannot be verified.
7. Whether a legitimate current native-mobile onboarding contract or official device-authorization flow can eliminate manual trusted-device bootstrap without reproducing browser anti-bot controls. The observed web flow is explicitly not a production candidate.
8. The exact published official `rmcp` release and minimal feature set that satisfy Rust 1.95, macOS/Linux, schema, license, advisory, and cargo-deny requirements.
9. Whether native MCP validation shows synchronous keychain reads need a `spawn_blocking`/loaded-credential application seam rather than relying only on a multi-thread runtime.
10. Which local MCP hosts, if any, can provide a trustworthy per-call human approval contract sufficient to unlock the separately gated future financial phase. Until one is selected and tested, writes remain unavailable.
11. Whether any later release should add a remote MCP transport. The initial release is definitively local stdio only and must not accumulate HTTP/OAuth features speculatively.

## 21. Immediate next steps

1. Refactor auth status and every shared read application use case to `CredentialReader`, split current-account from revocation/login ports, add read-only native credential/API façades, and prove existing CLI behavior remains unchanged.
2. Add defensive bounds for arbitrary activity/payment/request IDs that will become MCP inputs.
3. Revalidate and lock the official `rmcp` stable release, minimal stdio/server/schema features, MSRV, license, advisories, and transitive graph.
4. Implement explicit MCP DTOs, schemas, conversions, annotations, and unit tests without serializing secret or raw transport types.
5. Implement generic read-only handlers and the exact tool registry using fake read-only credential/API services plus bounded framing/results, non-waiting one-operation admission, and conservative rate limiting.
6. Add the thin `venmo-mcp` stdio binary, protocol/process/framing/nonleak tests, and preserve Cargo `default-run = "venmo"`.
7. Update CI, release packaging, and README with both binaries, local host configuration, transcript privacy, prompt-injection treatment, annotations, and troubleshooting.
8. Perform isolated manual native credential access validation for the second executable on supported platforms; never use production credentials in CI.
9. Continue occasional explicit read-only observation of the validated mobile-issued token's lifetime. Do not poll aggressively, infer an expiry date, store the password, automatically reauthenticate, or retry after rejection.
10. Keep the live-verified CLI pay and request-creation contracts covered by exact synthetic tests and monitor them only through normal use; do not repeat canary mutations speculatively. Complete Phase 0 contract discovery for exact pending-request acceptance, which remains unavailable. Keep every MCP financial command unavailable until its own request, response, ambiguity, verification, and human-approval contracts are established.

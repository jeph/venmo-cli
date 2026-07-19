# venmo-cli

`venmo-cli` is a Rust-only project that provides the `venmo` command for a small, unofficial Venmo CLI.

> This project uses unsupported/private Venmo API endpoints. They can change or stop working without notice.

See the [implemented CLI-to-Venmo private API contract inventory](API.md) for wire-level behavior, evidence status, current web discovery leads, and unresolved transfer gates.

## Requirements and installation

Building requires Rust 1.95.0. The checked-in `rust-toolchain.toml` pins the required toolchain. From the repository root:

```sh
cargo build
cargo test
cargo run -- --help
```

To install the current checkout into Cargo's binary directory instead:

```sh
cargo install --path .
venmo --help
```

On macOS, `cargo run` development-signs the `venmo` executable immediately before running it so
Keychain approval can persist across rebuilds. Another Cargo command that relinks the executable
can leave it ad-hoc signed until the next `cargo run`. See
[macOS code signing](docs/macos-code-signing.md) for one-time setup, identity overrides,
limitations, and signature inspection.

All `cargo run -- ...` examples below run from the repository root. After installation, use the same arguments after `venmo` instead.

## Migration from the removed TypeScript CLI

The repository now ships only the Rust implementation. Node.js, pnpm, the TypeScript source tree, generated JavaScript, and standalone Node packaging are no longer part of the build or release path.

The Rust CLI continues to use the OS credential-store service/account pair `venmo-cli` / `default`. It can read the former TypeScript client's legacy camelCase credential envelope for compatibility; a later successfully validated credential replacement is written in the versioned Rust format. It never silently deletes an unreadable credential.

The command interface is intentionally not backward-compatible:

| Removed TypeScript form | Rust form |
| --- | --- |
| `venmo init` | `venmo auth login` |
| `venmo status` | `venmo auth status` |
| `venmo deinit [--revoke]` | `venmo auth logout` (local credential removal only) |
| `venmo search <QUERY>` | `venmo users search <QUERY>` |
| `venmo payment-methods list` | `venmo pay methods` |

Payment operations are grouped under `venmo pay`. Use `venmo pay methods` for the read-only method
list and `venmo pay user ...` to send a payment. The former top-level `payment-methods` command and
ungrouped `venmo pay <USERNAME> ...` form are rejected rather than retained as aliases.

The old `charge`, split-payment, and multi-recipient interfaces were removed. `pay user` and direct request creation use the mobile-private API through independently verified Rust contracts; each passed exact synthetic HTTP tests and a separately approved, reconciled private one-cent live validation on 2026-07-12. Both enforce a personal/payable recipient, one-write/no-retry behavior, strict success validation, and ambiguous-outcome classification. They accept `--visibility private|friends|public`, default to `private`, and submit the selected value as the requested Venmo audience. Venmo may apply the more restrictive setting selected between payment partners, so creation responses are accepted only when they contain a supported audience no more public than requested; missing, unknown, or more-public responses remain ambiguous. Output labels the selection as the requested audience rather than claiming it is effective, and payment details warn about the possible restriction before confirmation. Controlled one-cent payment validation on 2026-07-14 proved that both non-private values can become effective with a compatible counterparty and that another participant's settings can instead make a friends-selected payment private. Direct non-private request creation retains exact synthetic coverage and the same `/v1/payments` creation contract; at the owner's direction it was not repeated live. `pay user` obtains the transaction eligibility token and reports the eligibility response's fee, but it does not reject a method or transaction because the fee is nonzero or unknown. Request creation has no funding or fee fields and never sends money from the authenticated account.

Request creation, inspection, and incoming-request actions are grouped under `venmo requests`. The write commands are:

```sh
venmo requests create <USERNAME> <AMOUNT> --note <NOTE> [--visibility private|friends|public]
venmo requests accept <REQUEST_ID> [--yes]
venmo requests decline <REQUEST_ID> [--yes]
```

The former top-level `request`, `accept`, and `decline` forms are not compatibility aliases.

`requests accept` fetches an authoritative incoming exact-`pending` private request, requires a personal/payable requester and enough available Venmo balance to cover the full amount, submits no external funding method, shows validated request details before default-No confirmation, and uses the historical incoming action `approve`. That balance check is only a snapshot: the update does not bind it or prove the final funding source/fee, so the CLI explicitly discloses the limitation and does not claim wallet funding or `$0.00` fees. Controlled live validation established that a successful response uses a distinct settled-payment ID and can retain request-style `action: charge` and party orientation even though activity exposes the resulting private outgoing `pay`; the validator supports both this live representation and the historical pay-oriented representation while requiring all operation-specific fields to match. Reconciled validations confirmed that the request disappears, exactly one matching private outgoing payment settles, and available balance decreases by the exact request amount.

`requests decline` fetches an authoritative incoming exact-`pending` request with a supported audience, displays and flushes validated request details, and asks for default-No confirmation before writing. When either stdin or stderr is not a terminal, `--yes` is required; the flag skips only confirmation, never validation or the details display. Decline sends no money or funding fields and uses historical incoming action `deny`—never outgoing-request `cancel`. A controlled live validation of a private request sent one non-retried `deny` update, received the original request ID with exact terminal `cancelled` status, removed the request from pending results, produced no payment activity, and left available balance unchanged. The implementation accepts and preserves any supported audience, but non-private success has not been independently pinned. Both request mutations treat any unverified response or transport uncertainty as ambiguous and must not be retried before reconciliation.

For `pay user`, the CLI internally chooses the unique default peer-eligible external bank or card, or the sole eligible method when no default exists. It rejects duplicate method IDs, multiple defaults, and ambiguous sets of multiple non-default methods; it never chooses by response order or fee. Any automatically chosen eligible method may carry zero, nonzero, or unknown method-level fee evidence. Venmo may still use available wallet balance before the submitted external backup method. Payment details display the method-level fee evidence and the separate eligibility-reported fee and warn that eligibility is not bound to the submitted method, so the final fee may differ. Users cannot choose a funding method. `pay user` retains only the final default-No confirmation when both stdin and stderr are terminals; otherwise it requires `--yes`.

Creation visibility is explicit and typed on both applicable commands:

```sh
venmo pay user @alice 0.01 --note "Dinner" --visibility friends --yes
venmo requests create alice 0.01 --note "Dinner" --visibility public
```

User-taking commands accept exact usernames with or without a leading `@`; they do not expose user-ID arguments. Each command resolves the normalized username through the shared bounded username search, requires a case-insensitive exact match, and verifies the resulting user through authoritative detail-by-ID before continuing. Omitting `--visibility` preserves the private default. Venmo's participant privacy settings may make the effective audience more restrictive than requested. The flag does not apply to `requests accept` or `requests decline`; those action-only mutations preserve and validate the audience of the existing request.

Direct request creation writes immediately after its account and recipient validation; it has no confirmation prompt and does not accept `--yes`. Decline instead always displays authoritative request details, then requires default-No confirmation unless `--yes` carries noninteractive authorization, while remaining protected as an ambiguous state mutation after possible transmission. If any mutation exits with code `3`, says its outcome is unknown, or says a successful result could not be written, **do not retry it**. Reconcile first with `venmo activity list`, `venmo requests list`, and the official Venmo application.

### Transfer options and standard cash-out

The enabled read-only command:

```sh
venmo transfer options
```

uses the current bearer/device-authenticated `GET /v1/transfers/options` contract. It shows only
sanitized eligible cash-out destination rows, including each instrument's estimated completion. It
moves no money and does not expose inbound source metadata.

The enabled first write shape is:

```sh
venmo transfer out <AMOUNT> [--speed standard] [--yes]
```

Standard-out performs current-account validation, checks that available Venmo balance covers the amount, then reads fresh transfer options. Omitting `--speed` defaults to `standard`; explicit `--speed standard` remains valid, and no other speed is supported. It accepts only the standard destination branch and exact `bank` type, requires absent standard fee metadata, rejects duplicate IDs/multiple defaults/ambiguous nondefaults, and chooses the unique default or otherwise sole candidate. Users cannot provide a destination ID or select by response order. Validated transfer details are flushed before the write; without `--yes`, they are followed by default-No confirmation.

The command sends one non-retried `POST /v1/transfers` with identical positive integer-cent `amount` and `final_amount`, the selected destination ID, and `transfer_type: standard`. Success requires HTTP 201 and the controlled-live direct `data` envelope: valid transfer ID/timestamp, exact `pending` status, standard type, exact requested cents, arithmetically consistent net/fee cents, matching dollar amount, and matching destination ID/type/suffix. Output distinguishes requested amount, net amount, and fee. A separately approved one-cent canary on 2026-07-17 returned HTTP 201 with ID/time, pending/standard, exact `$0.01`, numeric requested/net/fee-cent fields, and destination data; exactly one matching pending outgoing activity record had the same transfer ID. Runtime arithmetic and destination equality are additionally enforced fail-closed. This proves accepted/pending submission, not bank settlement.

Every unverified response, non-201 result, interruption, challenge, or output failure is ambiguous:
**do not retry** before checking activity and the official app. Adding money to the Venmo balance
(`transfer in`) is intentionally unsupported. Instant cash-out, debit-card cash-out, manual
destination selection, OTP/challenge continuation, cancellation, and expedition also remain
unavailable. Fee/minimum/maximum units outside the validated standard success fields remain
evidence-gated.

### Endpoint-native read pagination

Four read-only commands expose the continuation fields used by their source endpoints:

```sh
venmo friends list [--limit N] [--offset N]
venmo users search <QUERY> [--limit N] [--offset N]
venmo activity list [--limit N] [--before-id TOKEN]
venmo requests list [--direction all|incoming|outgoing] [--limit N] [--before TOKEN]
```

Each of these four invocations requests exactly one source API page, validates and buffers that complete page, and then renders its records. `--limit` is the server request page size; it defaults to 10 and cannot exceed 50. Friend listing and user search use a typed nonnegative `--offset` that defaults to 0. Activity uses its opaque `--before-id`, while pending requests use their opaque `--before`. There is no universal continuation input, universal offset, page number, or public multi-page collector. A single-token user search is normalized as a username search, so `users search alice` and `users search @alice` are equivalent; multi-word input remains a general fuzzy search.

The non-paginated `users info <USERNAME>`, `activity info <ACTIVITY_ID>`, and
`requests info <REQUEST_ID>` commands inspect one user, activity, or open request. User info accepts a
username with an optional leading `@` and uses the same shared, fail-closed exact search and authoritative
detail verification as financial recipient resolution; `users info alice` and `users info @alice` are
equivalent. It never accepts a user ID as a distinct argument type. Request detail uses the
broad payment/request lookup internally but exposes only `action=charge` records whose status is
exactly `pending` or `held`, in either direction. Payment (`action=pay`) and terminal request
records are rejected as usage errors rather than displayed as open requests.

Peer-payment reads are intentionally not exposed yet. The request-specific `/payments` contracts do
not establish a safe general payment list, and a dated settled-PaymentId detail probe returned HTTP
500 even though the corresponding activity story was readable. `payments list` and `payments info`
remain evidence-gated rather than guessing from activity IDs or payment-shaped request data.

When the validated source response has another page, the CLI writes only the endpoint-native copyable continuation to stderr, after record output succeeds: `Next offset: <N>`, `Next before-id: <TOKEN>`, or `Next before: <TOKEN>`. Friends use the offset from the validated server next link. User search provisionally synthesizes the next offset when the source page is full, so one final empty page can be required to observe exhaustion. Raw next URLs are never printed or followed.

Pending-request direction remains a local filter over that one source page. A filtered invocation can therefore emit fewer than `--limit` records, including none, while still reporting the source page's `before` continuation. Continuation tokens are bounded, reject whitespace and control characters, and are redacted from parsing errors and diagnostic formatting. The transport still validates every server next link against the fixed origin, route, and endpoint query allowlist and reconstructs subsequent requests locally. Internal exact-recipient resolution remains a separate exhaustive user-search traversal with its existing 200-record/four-page fail-closed bounds and no user-supplied continuation.

User-facing timestamps are converted to the system-configured time zone and rendered to whole seconds as `YYYY-MM-DDTHH:MM:SS`. Local output has no zone suffix; when the configured zone is UTC, or when system-zone discovery fails and the CLI falls back to UTC, output uses `YYYY-MM-DDTHH:MM:SSZ`. Conversion uses the rules for each timestamp, including historical daylight-saving transitions. API parsing, comparisons, and credential persistence continue to preserve exact UTC instants independently of this display formatting.

### Trusted-device mobile login

`venmo auth login` uses Venmo's unsupported legacy mobile-private password/SMS-OTP flow. Before running it, sign in through Venmo's normal browser interface and manually obtain the `v_id`/device ID for that already trusted browser session. The CLI does not automate the browser, import cookies, or generate a replacement device ID.

Fresh-device research confirmed that Venmo creates a `v_id` before web login and makes it acceptable to the mobile endpoint only after a session-bound identity flow involving DataDome, reCAPTCHA Enterprise, browser-risk telemetry, password, MFA, and an explicit remember-device finalization. No standalone registration, OAuth device grant, or browserless refresh/bootstrap endpoint was found. Replaying or fabricating those browser security signals is intentionally out of scope, and browser-assisted login is not a production CLI design; initial setup therefore still requires the trusted ID to be supplied through hidden input.

Run the login only in your own interactive terminal:

```sh
cargo run -- auth login
```

The CLI always prompts for the account identifier, password, and a newly supplied trusted device ID, followed—only if Venmo returns the recognized challenge—by SMS OTP. Password, device ID, and OTP input are hidden with terminal echo disabled. Login never reuses a stored device ID because Venmo may have invalidated it. It sends one non-retried authentication attempt, validates any issued token with the current-account endpoint, then stores only the validated bearer token, newly supplied device ID, and account metadata in the native OS credential store. Direct password success requires no redundant device-trust update; after OTP, device trust is attempted once after the validated credential is safely stored. Account identifiers, passwords, OTPs, and OTP secrets are never stored.

Login does not require logout first. A successful login may replace any readable existing credential, including one for a different account, but only after the new token validates and the replacement reads back exactly. Any failure before storage leaves the previous entry untouched. Replacing a credential does not revoke the previous bearer token, so the CLI warns that it may remain remotely valid and points to official Venmo session controls.

The CLI does not import bearer tokens, store reusable passwords, refresh sessions automatically, retry authentication, or expose a separate reauthentication command. A definitive HTTP 401 from an authenticated request preserves the keyring entry and directs the user to run `venmo auth login` explicitly. Arbitrary non-401 API rejections retain their general classification, and uncertain financial writes remain ambiguous and must not be retried.

Never put a device ID, password, OTP, bearer token, or browser cookie in a command argument, environment variable, file, chat, screenshot, or log. Login exposes no secret, environment, or stdin-file flags. If Venmo rejects an attempt, stop rather than repeatedly retrying.

`venmo auth status` validates the stored session and account. `venmo auth logout` removes only the local keyring entry; it does not contact Venmo or revoke the remote bearer token, and it reports that consequence truthfully.

## Contributor documentation

Routine contributor verification is service-free. Do not run ignored/manual tests or live probes,
load the production credential for tests, or run a real financial command—including a transfer—as a development check.
The user-facing command descriptions above are not contributor test instructions.

- [Contributing and required verification](CONTRIBUTING.md)
- [Architecture and dependency boundaries](docs/architecture.md)
- [Testing strategy](docs/testing.md)
- [Retained integration/manual contracts](docs/retained-test-contracts.md)
- [Evidence-gated follow-ups](docs/evidence-gated-follow-ups.md)
- [CLI-to-Venmo private API contract inventory](API.md)
- [Public facade inventory and compatibility](docs/public-api.md)

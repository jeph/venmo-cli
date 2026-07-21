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

## Debug diagnostics

Pass the global `--debug` flag before or after any subcommand to write bounded diagnostics to
stderr. Debug mode records a static command name and API method/route template, HTTP status,
duration, retry count, response size/type, sanitized error code, and bounded Venmo error
title/message when present. It never prints raw request or response bodies, authorization values,
device IDs, OTPs, dynamic path/query values, funding-source IDs, usernames, notes, or amounts.
Third-party dependency tracing is filtered out rather than enabled by the flag.
Venmo's human-readable error text is untrusted remote content and can contain account context;
review debug output before sharing it. Debug output is disabled by default. The former `-v` and
`--verbose` flags are intentionally rejected rather than retained as aliases.

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
| `venmo payment-methods list` | `venmo pay options` |

Payment operations are grouped under `venmo pay`. Use `venmo pay options` for the read-only funding
option list and `venmo pay user ...` to send a payment. The former top-level `payment-methods`
command, `venmo pay methods`, and ungrouped `venmo pay <USERNAME> ...` form are rejected rather than
retained as aliases. This matches the existing `venmo transfer options` naming.

```sh
venmo pay user <USERNAME> <AMOUNT> <NOTE> [--source <SOURCE_ID>] [--protect] [--visibility private|friends|public] [--yes]
```

The old `charge`, split-payment, and multi-recipient interfaces were removed. `pay user` and direct request creation use the mobile-private API through independently verified Rust contracts; each passed exact synthetic HTTP tests and a separately approved, reconciled private one-cent live validation on 2026-07-12. Both enforce a personal/payable recipient, zero automatic retries, strict success validation, and ambiguous-outcome classification. Each creation normally makes one write; Venmo's exact prewrite SMS challenge may add one challenge-verified continuation with the same client UUID. They accept `--visibility private|friends|public`, default to `private`, and submit the selected value as the requested Venmo audience. Venmo may apply the more restrictive setting selected between payment partners, so creation responses are accepted only when they contain a supported audience no more public than requested; missing, unknown, or more-public responses remain ambiguous. Output labels the selection as the requested audience rather than claiming it is effective. Controlled one-cent payment validation on 2026-07-14 proved that both non-private values can become effective with a compatible counterparty and that another participant's settings can instead make a friends-selected payment private. Direct non-private request creation retains exact synthetic coverage and the same `/v1/payments` creation contract; at the owner's direction it was not repeated live. Without `--protect`, `pay user` obtains and validates the existing blank-source transaction eligibility token and fee data but does not reject a method or transaction because a fee is nonzero or unknown. It does not present that unbound eligibility fee as a payer fee or add it to the payment amount. Request creation has no funding or fee fields and never sends money from the authenticated account.

Request creation, inspection, and request mutations are grouped under `venmo requests`. The write commands are:

```sh
venmo requests create <USERNAME> <AMOUNT> <NOTE> [--visibility private|friends|public] [--yes]
venmo requests accept <REQUEST_ID> [--source <SOURCE_ID>] [--protect] [--yes]
venmo requests decline <REQUEST_ID> [--yes]
venmo requests cancel <REQUEST_ID> [--yes]
```

The former top-level `request`, `accept`, and `decline` forms are not compatibility aliases.

`requests accept` fetches an authoritative incoming exact-`pending` private request and requires a
personal/payable requester. Acceptance is an unprotected personal payment by default. Every
acceptance resolves one unacknowledged request notification whose payment ID equals the pending
request ID, loads peer-eligible balance/bank/card sources, applies the shared automatic or explicit
source policy, obtains source-bound eligibility, and uses the request notification's own action ID
for `PUT /v1/requests/{request-action-id}`. There is no automatic legacy fallback. Missing, duplicate,
conflicting, or malformed notification matches, unavailable explicit sources, and insufficient
explicit balance fail before confirmation or a write. An explicit source is submitted exactly
without silent substitution. Unprotected approval omits eligibility fee records.
`--protect` turns on Venmo Purchase Protection and submits only
strictly validated normalized fee records. Current official Venmo guidance says the buyer pays no
extra Purchase Protection fee and the seller pays 2.99%; the seller fee is removed from the received
amount. The CLI does not calculate that rate itself: before confirmation it labels the exact
eligibility response amount as an estimated seller deduction and shows estimated recipient proceeds,
never a larger payer total. Every path shows its immutable plan before default-No confirmation and
normally attempts one financial write. The path is pinned by signer-verified
Android 10.31.1 and 26.13.0, exact synthetic tests, and reconciled client-1 approvals. Neither those
responses nor current source-selection tests prove the actual debit source or final fee, and explicit
source selection has not been separately validated live.

Current notification responses can wrap the actionable request at `additional_properties.request`.
In that shape, the CLI matches the canonical pending request to the nested request's `payment.id`
and submits the nested request's own `id`; it never substitutes the wrapper's outer notification
ID. A bounded live read proved the three-ID structure. An outer-ID attempt returned HTTP 404 and
reconciled as no mutation; the corrected nested-action-ID attempt returned 200 and reconciled as
exactly one settled $20 activity with the pending request removed.

`requests decline` fetches an authoritative incoming exact-`pending` request with a supported audience, displays and flushes validated request details, and asks for default-No confirmation before writing. When either stdin or stderr is not a terminal, `--yes` is required; the flag skips only confirmation, never validation or the details display. Decline sends no money or funding fields and uses historical incoming action `deny`—never outgoing-request `cancel`. A controlled live validation of a private request sent one non-retried `deny` update, received the original request ID with exact terminal `cancelled` status, removed the request from pending results, produced no payment activity, and left available balance unchanged. The implementation accepts and preserves any supported audience, but non-private success has not been independently pinned. Both incoming-request actions treat any unverified response or transport uncertainty as ambiguous and must not be retried before reconciliation.

`requests cancel` is the distinct outgoing-request operation. It fetches the authoritative record and requires an exact outgoing `charge` owned by the active account in `pending` or `held` state before displaying and flushing the request details. It then asks for default-No confirmation and sends one non-retried form-encoded `action=cancel` update. The response must preserve the exact ID, parties, amount, note, audience, and creation time and prove terminal server status `cancelled`; otherwise the outcome is unknown. The command sends no money or funding fields, cannot reverse a completed payment, and never substitutes incoming `deny`. The wire contract is pinned by Venmo Android 26.13.0 plus independent public Go and Python implementations and exact synthetic tests; current client-1 live authorization has not yet been validated.

For `pay user`, automatic funding uses the peer-eligible Venmo balance source when authoritative available balance covers the full amount. Otherwise it chooses the unique default peer-eligible external bank or card, or the sole external method when no default exists. External ambiguity is evaluated only when an automatic external choice is needed; selection never uses response order or fee. `--source` accepts an exact peer-eligible balance, bank, or card ID from `venmo pay options`; explicit balance must cover the full amount, and unknown or ineligible IDs fail before confirmation. The CLI submits an explicitly requested source exactly and never silently substitutes another source. Before confirmation, payment details show the selected funding source and applicable method-level `Funding-source fee` evidence without exposing the internal automatic/explicit policy marker. They omit the blank-source eligibility fee because it does not prove a payer charge or final total. Success output identifies the source ID submitted in the request; Venmo's response does not prove the actual debit source or final fee. `pay user` retains only the final default-No confirmation when both stdin and stderr are terminals; otherwise it requires `--yes`.

`pay user --protect` explicitly requests Venmo Purchase Protection for one eligible purchase from a personal profile. After selecting the funding source, the CLI performs the current source-bound form eligibility call and requires exactly one complete fee whose product URI is in Venmo's buyer-protection namespace. It fails before confirmation on denial, missing/duplicate/malformed fees, an invalid token, or a seller fee above the payment amount; it never silently falls back to an unprotected payment. The payment sends `transaction_type: goods_services_protected`, the exact singleton fee, and quasi-cash metadata. Before confirmation, output labels the fee as an estimated seller deduction and shows estimated recipient proceeds—never a larger payer total or a hard-coded percentage. A successful response must explicitly return `type: payment_protected` before output says `tagged by Venmo`. Tagging does not guarantee item coverage, generally cannot be removed, may create seller tax consequences, and does not prove the final fee or debit source. This branch is pinned by signer-verified Venmo Android 26.13.0, exact service-free tests, and one owner-authorized, non-retried $1 live validation on 2026-07-20: eligibility returned 201, payment creation returned 200 without OTP, one matching payment settled, and read-only detail returned `type: payment_protected`. The initial release expected the request-side transaction type in the response and conservatively reported that successful write as ambiguous; no duplicate payment was attempted.

Venmo may reject the first `pay user`, `requests create`, or `requests accept` submission with its exact P2P SMS-OTP step-up response: HTTP 403 with root `error.title` exactly `OTP_STEP_UP_REQUIRED`. The CLI supports that continuation only on an interactive terminal. All three commands request one SMS, show the same hidden `Venmo SMS verification code` prompt, verify once, and perform at most one operation-specific continuation. Payment and request creation retain the same client UUID; acceptance uses the server challenge UUID from root `error.metadata.uuid` and fails closed if it is missing or invalid. Verified metadata is merged into the otherwise unchanged financial request, and the OTP itself is never included. There is no loop, code-only `1396` trigger, or automatic legacy fallback. `--yes` skips confirmation but cannot bypass OTP. The code is never accepted as an argument, printed, logged, or stored. Payment's complete flow is additionally owner-validated live; request creation and acceptance continuations are statically pinned by signer-verified Venmo Android 26.13.0 and exact service-free tests. Exact payment-creation HTTP 403/root code `1360` is reported as Venmo's 10-minute same-recipient/amount/note duplicate rejection; exact code `10100` reports that Venmo's server-side checks blocked the payment and tells the operator to try again later or use the official app.

Financial error guidance is operation-specific: a numeric code is never interpreted without the API operation, HTTP result, root location, and—where required—exact root title. Confirmed peer declines, acceptance risk rejection, pending teen-account restrictions, scam-warning review, and Plaid relinking receive direct recovery instructions. APK-known risk/decline and standard cash-out codes that do not independently prove a non-write remain exit `3`, but explain the likely cause and still require activity/request reconciliation before retrying. Codes under the wrong operation, wrong status, a nested location, or an unknown shape inherit no special meaning. Raw server messages are never used as command errors.

Creation visibility is explicit and typed on both applicable commands:

```sh
venmo pay user @alice 0.01 "Dinner" --visibility friends --yes
venmo requests create alice 0.01 "Dinner" --visibility public --yes
```

User-taking commands accept exact usernames with or without a leading `@`; they do not expose user-ID arguments. Each command resolves the normalized username through the shared bounded username search, requires a case-insensitive exact match, and verifies the resulting user through authoritative detail-by-ID before continuing. Omitting `--visibility` preserves the private default. Venmo's participant privacy settings may make the effective audience more restrictive than requested. The flag does not apply to `requests accept`, `requests decline`, or `requests cancel`; those mutations preserve and validate the audience of the existing request.

Friendship mutations are grouped with friend listing:

```sh
venmo friends add <USERNAME> [--yes]
venmo friends remove <USERNAME> [--yes]
```

Both commands perform the same exact-username and authoritative-detail validation before showing
friendship details and asking for default-No confirmation. They follow the current native app's
state semantics: `add` sends a request from `not friend` or accepts an incoming request; `remove`
unfriends an established friend or cancels an outgoing request. `remove` never silently declines
an incoming request. Each command sends at most one non-retried write and then requires an
authoritative user-detail read to prove the resulting relationship. A successful HTTP response
without matching reconciliation, an interruption, or post-transmission transport uncertainty exits
`3`; do not retry until the relationship is checked in the official Venmo app.

The request shapes are pinned by signer-verified Android 9.26.0, 10.31.1, and 26.13.0 artifacts,
but no independent public mutation implementation was found. Those Android clients authenticate as
OAuth client 4 while this CLI retains its historical client-1 session. The implementation is
therefore synthetically verified but has not yet received a controlled live canary proving that the
current service authorizes these writes for the CLI's session family.

Request creation, acceptance, decline, and outgoing cancellation each display and flush a complete validated action summary before authorization. Confirmation defaults to No; when either stdin or stderr is not a terminal, `--yes` is required. The flag skips only the prompt, never preflight or summary output. If any mutation exits with code `3`, says its outcome is unknown, or says a successful result could not be written, **do not retry it**. Reconcile first with `venmo activity list`, `venmo requests list`, and the official Venmo application.

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
venmo transfer out <AMOUNT_OR_ALL> [--speed standard] [--yes]
```

Standard-out performs current-account validation and reads a fresh available-balance snapshot before
fresh transfer options. A positive decimal amount retains the exact balance-coverage check. Exact
lowercase `all` instead resolves once to every positive available cent in that snapshot; it excludes
on-hold funds and is not an atomic account drain. Zero or negative availability fails before options
or a write. Omitting `--speed` defaults to `standard`; explicit `--speed standard` remains valid, and
no other speed is supported. The command accepts only the standard destination branch and exact
`bank` type, requires absent standard fee metadata, rejects duplicate IDs/multiple
defaults/ambiguous nondefaults, and chooses the unique default or otherwise sole candidate. Users
cannot provide a destination ID or select by response order. Validated transfer details show the
resolved amount and, for `all`, its snapshot source. Without `--yes`, action-specific default-No
confirmation follows.

The command sends one non-retried `POST /v1/transfers` with identical positive integer-cent `amount` and `final_amount`, the selected destination ID, and `transfer_type: standard`. Success requires HTTP 201 and the controlled-live direct `data` envelope: valid transfer ID/timestamp, exact `pending` status, standard type, exact requested cents, arithmetically consistent net/fee cents, matching dollar amount, and matching destination ID/type/suffix. Output distinguishes requested amount, net amount, and fee. A separately approved one-cent canary on 2026-07-17 returned HTTP 201 with ID/time, pending/standard, exact `$0.01`, numeric requested/net/fee-cent fields, and destination data; exactly one matching pending outgoing activity record had the same transfer ID. Runtime arithmetic and destination equality are additionally enforced fail-closed. This proves accepted/pending submission, not bank settlement.

Every unverified response, non-201 result, interruption, or output failure is ambiguous: **do not
retry** before checking activity and the official app. Known standard cash-out codes add bounded
guidance for insufficient funds, amount/limits, unavailable bank methods, account restrictions, and
risk declines without changing that ambiguous classification. Exact HTTP 403 plus root title
`OTP_STEP_UP_REQUIRED` instead reports that the official app requires an unsupported SMS
continuation and confirms that the CLI did not retry. Adding money to the Venmo balance
(`transfer in`) is intentionally unsupported. Instant cash-out, debit-card cash-out, manual
destination selection, OTP/challenge continuation, cancellation, and expedition also remain
unavailable. Fee/minimum/maximum units outside the validated standard success fields remain
evidence-gated.

### Endpoint-native read pagination

Four read-only commands expose the continuation fields used by their source endpoints:

```sh
venmo friends list [--user USERNAME] [--limit N] [--offset N]
venmo users search <QUERY> [--limit N] [--offset N]
venmo activity list [--user USERNAME] [--limit N] [--before-id TOKEN]
venmo requests list [--direction all|incoming|outgoing] [--limit N] [--before TOKEN]
```

Each of these four invocations requests exactly one source API page, validates and buffers that complete page, and then renders its records. `--limit` is the server request page size; it defaults to 10 and cannot exceed 50. Friend listing and user search use a typed nonnegative `--offset` that defaults to 0. Activity uses its opaque `--before-id`, while pending requests use their opaque `--before`. There is no universal continuation input, universal offset, page number, or public multi-page collector. A single-token user search is normalized as a username search, so `users search alice` and `users search @alice` are equivalent; multi-word input remains a general fuzzy search.

Friend listing defaults to the active account. Optional `--user alice` and `--user @alice` are
equivalent: they use the shared bounded exact-username search and authoritative detail read, require
a personal profile, and request the friends visible to the authenticated viewer. A matching active
username uses the stored self ID without lookup. Other-user output is explicitly scoped as
`Friends for @username`; an empty page says `No visible friends found` because Venmo privacy
settings can hide a list or individual appearances. Business, charity, unknown, and missing
profile types fail before the friends request. The CLI never claims that a visible page is an
exhaustive social graph.

Activity listing defaults to the authenticated user's existing feed. Optional `--user alice` and
`--user @alice` are equivalent: they use the shared bounded exact-username search and authoritative
user detail, require a personal profile, and fetch the normal personal-profile feed visible to the
authenticated viewer. Directions and counterparties are then relative to the selected user.
Amounts are deliberately omitted from other-user list output even when the private API supplies a
value; the normal social feed does not present transaction amounts as public profile data.
Business, charity, unknown, and missing profile types fail before the feed request; current Android
uses a separate public-only business-feed contract. Other-user pages accept only payment stories
with a supported audience, and private stories must include the authenticated viewer. A malformed
or privacy-inconsistent page fails closed rather than silently dropping records.

The non-paginated `users info <USERNAME>`, `activity info <ACTIVITY_ID>`, and
`requests info <REQUEST_ID>` commands inspect one user, activity, or open request. User info accepts a
username with an optional leading `@` and uses the same shared, fail-closed exact search and authoritative
detail verification as financial recipient resolution; `users info alice` and `users info @alice` are
equivalent. It displays a supported friendship state when P2 supplies one and marks missing or
unknown evidence as not provided. It never accepts a user ID as a distinct argument type. Request detail uses the
broad payment/request lookup internally but exposes only `action=charge` records whose status is
exactly `pending` or `held`, in either direction. Payment (`action=pay`) and terminal request
records are rejected as usage errors rather than displayed as open requests.
Activity IDs are globally unique, so `activity info` has no `--user` flag. Payment details render
the absolute actor and target and can therefore represent a server-visible story involving two
other users; private external stories are rejected, and an unavailable or external-only amount is
not rendered. Transfer and authorization details retain
their current-account ownership checks.

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

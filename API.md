# Venmo private API contract inventory

> [!CAUTION]
> **Every Venmo endpoint in this document is unsupported and private.** Venmo can
> change, reject, or remove it without notice. This inventory describes evidence;
> it is not public API documentation, a compatibility promise, or a live-probing
> runbook.

> [!WARNING]
> **Contributor safety:** routine development and CI are service-free. Do not use a
> production credential, invoke ignored/manual probes, replay browser traffic, or
> run a real `pay user`, `requests create`, `requests accept`, `requests decline`, `requests cancel`, or
> transfer as a test. Use the
> commands in [CONTRIBUTING.md](CONTRIBUTING.md), with fakes or loopback servers.
> Never put a bearer token, device ID, password, cookie, CSRF value, OTP material,
> client secret, full card/bank value, or personal identifier in fixtures, docs,
> logs, screenshots, or issue reports.

> [!IMPORTANT]
> Financial writes are one-shot and are never automatically retried. Once
> transmission may have started, an unprovable result is **outcome unknown**, not
> success or failure. Reconcile through independent reads and the official Venmo
> product before considering another action.

## 1. Purpose, scope, and boundaries
This repository-root artifact inventories two deliberately separate bodies of
evidence:

1. the bearer/device-authenticated mobile `/v1` contracts actually implemented by
   `venmo-cli`; and
2. current authenticated-web intent emitted by captured Venmo browser build
   `OHCscQK3gMovYos3VtRgA`, observed on 2026-07-15, plus clearly marked historical
   leads.

This is not [`docs/public-api.md`](docs/public-api.md), which inventories the
supported Rust facade exported by this crate. The product and safety rationale are
in [PLAN.md](PLAN.md); contribution rules are in
[CONTRIBUTING.md](CONTRIBUTING.md); unresolved model decisions are in
[`docs/evidence-gated-follow-ups.md`](docs/evidence-gated-follow-ups.md). See also
the user-facing framing in [README.md](README.md).

The implemented mobile audit began from repository commit
`775b35ae2009203da02fc08512a8335bff10218c`; transfer-option work is dated
2026-07-17. Static web evidence is not implemented CLI behavior. In particular:

- same-origin `/api/...` URLs are browser backend-for-frontend (**BFF**) contracts,
  not demonstrated upstream Venmo service paths;
- browser cookies, CSRF state, SSR eligibility, and browser-risk context are not a
  substitute for the CLI's bearer token plus device ID;
- a GraphQL document proves only the emitted operation and selected fields, not its
  full schema, current authorization, or successful execution; and
- an SSR prop proves a consumed shape, not the hidden server request that produced
  it.

All example values use placeholders. Exact enum strings, field names, status/error
codes, build IDs, public commit IDs, and fixed protocol constants are retained where
they are contract evidence.

## 2. Evidence vocabulary
Source classification and verification state are independent; retain both without
silent promotion.
### 2.1 Source classifications
| Classification | Precise meaning |
|---|---|
| **existing-client behavior** | Repository implementation/tests at the cited commit: code truth, not current service support. |
| **direct observation** | Sanitized controlled interaction or identified current static artifact; say which. Static artifacts prove only emitted/consumed client intent. |
| **historical lead** | Archived official or third-party discovery material; not proof of a current endpoint, shape, auth, response, or retry policy. |
### 2.2 Verification states
| State | Precise meaning |
|---|---|
| **exact synthetic contract test** | Service-free fake/loopback assertion of method, path, query, auth, body, and supported response/error semantics. |
| **controlled live validation** | Separately authorized bounded interaction on its date; neither contributor instruction nor timeless guarantee. |
| **reconciled live validation** | Controlled interaction matched to independent authoritative state, such as reads and the official app. |
| **owner-approved evidence decision** | Governance acceptance without another live mutation; not new wire observation. |
| **residual evidence gap** | Material auth, request, response, transition, pagination, or policy remains unproved. |
### 2.3 Static-web sublabels
All web findings below are **direct observation (static client artifact)** dated
2026-07-15 unless stated otherwise.

| Label | Meaning |
|---|---|
| **exact browser call** | Captured page call site invokes the shown REST/GraphQL operation. |
| **runtime-gated browser call** | Call site exists; flag, gateway, error, or merchant context controls execution. |
| **SSR contract only** | Browser consumes a server-rendered value; producing request is absent. |
| **route only** | Navigation only; no API contract. |
| **bundled/inactive** | Present helper/document has no relevant page call site. |
| **third-party call** | Plaid, Braintree, PayPal, or presigned object URL—not Venmo API. |

## 3. Complete implemented CLI action inventory
The CLI has 21 enabled leaf actions. Operation labels resolve to the mobile wire catalog in §5.
Counts begin after local credential loading; failed local gates can stop earlier.

| # | CLI leaf | Calls and maximum orchestration | Classification |
|---:|---|---|---|
| 1 | `auth login` | Password: **A1 → A5**. OTP: **A1 → A2 → A3 → A5**, verified local save, then **A4** (maximum 5). | Remote auth plus hidden identifier/password/fresh-device prompts and optional hidden OTP. |
| 2 | `auth logout` | No API call; delete only the local keyring entry. | Entirely local; does not revoke the remote token. |
| 3 | `auth status` | **A5** once. | Live identity validation. |
| 4 | `pay user <USERNAME> <AMOUNT> <NOTE> [--source SOURCE_ID] [--protect] [--visibility ...] [--yes \| --dry-run]` | **A5**; 1–4 × **P1** then **P2**; **W2 → W1**, then ordinary **F1** or protected **F1P**, then **F2**; exact step-up only: **F2O → F2V → F2** (maximum 13). | Financial; protection is explicit and never falls back to ordinary payment. One initial **F2**, or one initial rejected **F2** plus one challenge-verified **F2** with the same UUID. No automatic retry. |
| 5 | `requests create <USERNAME> <AMOUNT> <NOTE> [--visibility ...] [--yes \| --dry-run]` | **A5**; 1–4 × **P1** then **P2**; **F3**; exact step-up only: **F2O → F2V → F3** (maximum 10). | Financial; one initial **F3**, or one initial rejected **F3** plus one challenge-verified **F3** with the same UUID. No automatic retry. |
| 6 | `requests accept <REQUEST_ID> [--source SOURCE_ID] [--protect] [--yes \| --dry-run]` | **A5 → R2 → P2 → W2 → F4N → W1 → F4E → F4S**; exact step-up only: **F2O → F2V → F4S** (maximum 11). | Financial; always uses the current request-action route. One initial **F4S**, or one initial rejected **F4S** plus one challenge-verified **F4S** using the server challenge UUID. No legacy fallback or automatic retry. |
| 7 | `requests decline <REQUEST_ID> [--yes \| --dry-run]` | **A5 → R2 → F5** (3). | State-changing; exactly one **F5** after default-No confirmation. |
| 8 | `requests cancel <REQUEST_ID> [--yes \| --dry-run]` | **A5 → R2 → F6** (3). | State-changing; exactly one **F6** after default-No confirmation. |
| 9 | `friends list [--user USERNAME] [--limit N] [--offset N]` | Self: **P3** once. Other: 1–4 × **P1**, then **P2 → P3** (maximum 6). | One visible page; optional exact personal-profile subject. |
| 10 | `friends add <USERNAME> [--yes \| --dry-run]` | **A5**; 1–4 × **P1** then **P2**; exactly one **P4**, then reconciling **P2** (maximum 8). | Relationship write; sends or accepts according to authoritative state. |
| 11 | `friends remove <USERNAME> [--yes \| --dry-run]` | **A5**; 1–4 × **P1** then **P2**; exactly one **P5**, then reconciling **P2** (maximum 8). | Relationship write; unfriends or cancels an outgoing friend request. |
| 12 | `users search <QUERY> [--limit N] [--offset N]` | **P1** once. | One page. |
| 13 | `users info <USERNAME>` | 1–4 × **P1** then **P2** (maximum 5). | Shared exact-username resolution followed by detail read. |
| 14 | `pay options` | **W1** once. | One read. |
| 15 | `balance` | **W2** once. | One read. |
| 16 | `activity list [--user USERNAME] [--limit N] [--before-id TOKEN]` | Self: **AC1** once. Other: 1–4 × **P1**, then **P2 → AC1** (maximum 6). | One page; direction relative to selected personal subject. |
| 17 | `activity info <ACTIVITY_ID>` | **AC2** once. | One detail read. |
| 18 | `activity like <ACTIVITY_ID> [--yes \| --dry-run]` | **AC2 → AC3 → AC2** (3). | One bodyless state write, then exact liker reconciliation. |
| 19 | `activity unlike <ACTIVITY_ID> [--yes \| --dry-run]` | **AC2 → AC4 → AC2** (3). | One bodyless state write, then complete-absence reconciliation. |
| 20 | `activity comments add <ACTIVITY_ID> <MESSAGE> [--yes \| --dry-run]` | **AC2 → AC5 → AC2** (3). | One JSON state write, then exact created-comment reconciliation. |
| 21 | `activity comments remove <COMMENT_ID> [--yes \| --dry-run]` | **AC6** once. | Direct comment-ID state write; Venmo authorizes it. No parent-story preflight or automatic reconciliation is possible. |
| 22 | `requests list [--direction all\|incoming\|outgoing] [--limit N] [--before TOKEN]` | **R1** once; direction is filtered locally. | One unfiltered source page. |
| 23 | `requests info <REQUEST_ID>` | **R2** once; narrowed locally to open requests. | One detail read. |
| 24 | `transfer options` | **T1** once. | Enabled read-only current eligibility view. |
| 25 | `transfer out <AMOUNT_OR_ALL> [--speed standard] [--yes \| --dry-run]` | **A5 → W2 → T1 → T2** (4). | Financial; exact lowercase `all` resolves from W2 available cents; exactly one **T2** after default-No confirmation. |

Help and version output are also service-free but are not leaf actions. There is no
implemented generic transaction/payment list/detail, outgoing-request remind, explicit
funding-source selection, token refresh, incoming-friend-request decline, payment-method mutation,
remote token revocation, transfer-in command, instant transfer write, or manual
transfer-destination selection.

For eleven mutation leaves, `--dry-run` performs the same calls shown before the final mutation
(F2, F3, F4S, F5, F6, P4, P5, AC3, AC4, AC5, or T2), renders and flushes the validated details,
then exits 0 without confirmation, interruption installation, mutation, OTP, or post-write
reconciliation. Comment removal has no service preflight because its public input intentionally
contains only a comment ID; its dry-run performs credential validation and renders the exact intent
plus that limitation without sending AC6. Other preflights can read authenticated service state and
can use POST-based F1/F1P/F4E eligibility; dry-run means no final state change, not necessarily no
network traffic. `--dry-run` and `--yes` conflict.

## 4. Shared implemented mobile boundary
### 4.1 Transport and headers
- Fixed origin/base: `https://api.venmo.com/v1/`; ordinary production composition
  has no alternate base URL. TLS uses Rustls.
- Timeouts: connect 10 seconds, response read 15 seconds, whole request 30 seconds.
- Responses are captured completely with a 2 MiB maximum.
- Redirects, proxies, and automatic gzip/Brotli/Zstandard/deflate decoding are
  disabled. No `Referer` is sent.
- There are **zero automatic retries**, including reads, login, and writes.
- Every request sends a transport-instance session ID plus:

  ```http
  Accept: application/json; charset=utf-8
  Accept-Language: en-US;q=1.0
  User-Agent: Venmo/26.13.0 (iPhone; iOS 18.6.2; Scale/3.0)
  X-Session-ID: <UUID_V4_AS_UNSIGNED_DECIMAL_U128>
  ```

- A JSON body additionally sends `Content-Type: application/json`. P4 instead sends exact
  `application/x-www-form-urlencoded`; P5 sends no body.
- Query values are URL-encoded. Dynamic path segments must be nonempty, not `.` or
  `..`, contain neither slash nor backslash, and contain no control character.
- The global `--debug` flag installs bounded stderr diagnostics for every command. API events expose
  only method, static route template, status, duration, retry count, response byte count/type,
  sanitized error code, and bounded single-line root error title/message or first GraphQL error.
  They never expose raw bodies, headers, credentials, OTPs, dynamic URL values, or command
  arguments, and the subscriber filters out third-party dependency targets. Known credential
  values are replaced if Venmo reflects them in diagnostic text.
  Venmo-provided human-readable text remains untrusted account context and must be reviewed before
  sharing. No tracing subscriber is installed by default; removed `-v`/`--verbose` forms are errors.
- The iOS-compatible header profile is code truth. Venmo Android 26.13.0 and a maintained
  current client independently establish the current-version/session-header pattern; controlled
  live diagnostics under the old profile repeatedly produced generic code `10100`, while the
  current profile reached the recognized HTTP-403/root-title OTP challenge (with observed code
  `1396`) and completed one verified
  payment. Later rapid attempts also showed that `10100` means Venmo's server-side checks blocked
  the payment and is not a header fingerprint. These observations do not identify the check that
  fired or prove which individual header is necessary.

| Header mode | Additional exact headers | Operations |
|---|---|---|
| Device-only | `device-id: <DEVICE_ID>` | **A1** |
| OTP-secret | `device-id: <DEVICE_ID>`, `venmo-otp-secret: <OTP_SECRET>` | **A2** |
| OTP completion | preceding headers plus `venmo-otp: <SIX_DIGIT_OTP>` | **A3** |
| Authenticated | `Authorization: Bearer <ACCESS_TOKEN>`, `device-id: <DEVICE_ID>` | **A4–A5** and all other mobile operations |

No authenticated implemented call omits the device ID. The client implements no
cookie, browser-session, CSRF, or refresh-token contract.
### 4.2 Generic response acceptance
Unless a token or financial rule below is stricter:

1. Any HTTP 2xx is status-success; non-2xx is an API failure.
2. The first string or numeric code found in this precedence order controls the
   result: `error.code`, root `error_code`, root `code`, `data.error_code`, then
   `data.error.code`; later locations are ignored. Its rendered value is nonfailure
   only when exactly `0` or `0.0`.
3. Reads require a nonempty valid JSON body in a listed envelope. Void operations
   may accept an empty 2xx body but still reject a controlling code other than exact
   `0` or `0.0`.
4. Unknown fields are tolerated; required fields, envelope alternatives, exact-ID
   checks, and semantic cross-checks are not relaxed.
5. A response is accepted only after bounded full capture. Redirects are never
   followed.

Financial non-2xx interpretation is stricter than generic code extraction. User-facing meaning is
assigned only from an allowlisted tuple of operation, HTTP status or non-success class, root
`error.code`, and exact root `error.title` where applicable. A code found at root `error_code`, root
`code`, or under `data` may be rendered as bounded diagnostics but cannot inherit a financial
meaning. Known-but-not-conclusive Android mappings add fixed recovery guidance while preserving
outcome-unknown exit `3`; they never render the server's title, message, metadata, or body.
### 4.3 Shared values and records
- **User ID:** positive ASCII decimal digits. Leading zeroes are preserved and
  compared exactly; no explicit length maximum is currently imposed.
- **Username input:** all user-taking commands accept a username with an optional
  leading `@`; no command accepts a user ID argument. The normalized 1–1,023 byte
  value has no whitespace/control characters. Digit-only input is a username, not
  an ID selector.
- **Search query:** nonblank, at most 1,024 bytes, no controls. Any single-token
  bare or optional-`@` query is normalized on wire and adds `type=username`;
  multi-word input remains a general query.
- **Opaque IDs:** activity, request/payment, and payment-method IDs are 1–512 bytes
  and reject whitespace, controls, and supported invisible formatting characters.
- **Continuation:** 1–1,024 bytes; rejects Unicode whitespace/control characters,
  `U+200B–U+200F`, `U+202A–U+202E`, `U+2060–U+206F`, and `U+FEFF`, but not
  `U+00AD`, `U+061C`, or `U+180E`. Rust `Debug`/`Display` redact it; `activity list`
  and `requests list` bypass those traits and intentionally print the validated raw
  token to stderr for copy/reuse. Full server `pagination.next` URLs are never
  followed or printed.
- **Page controls:** `limit` is 1–50, default 10; `offset` is a `u32`, default 0.
- **Money:** positive exact base-10 USD, no sign/exponent/whitespace, at most two
  fractional digits, internally `u64` cents. Mobile payment/request JSON uses a
  JSON number, not a string.
- **Note:** nonblank. No guessed product-length maximum is imposed.
- **Audience:** exact `private`, `friends`, or `public`, default `private`;
  restrictiveness is `private` > `friends` > `public`.

A supported user has a valid string/integer `id`; optional `username`,
`display_name`/`displayName`/`name`, `identity_type`, `is_payable`, and exact
`friend_status` (`friend`, `not_friend`, `request_received_by_you`, or
`request_sent_by_you`). Unknown/missing friendship state remains readable as absent evidence but
cannot authorize P4/P5. Where P2
recipient detail is authoritative (`pay user`, request creation, and `accept`), the
counterparty must be nonself, exactly personal, and `is_payable: true`. `Decline`
does no P2 and does not prove profile type or payability. Identity types are
interpreted case-insensitively as personal, business, charity, or unknown.

Except for the nested payment in an activity story, payment/request records require
a valid opaque `id`, bounded `status` and `action`, positive amount, `actor`, and
`target.user`; optional note, audience, and creation time are operation-specific.
IDs and amount may be JSON integer/number alternatives. A payment activity may carry a null amount
when the viewer is not a participant; current-account activity still requires a positive amount.
Other-user list mapping validates any supplied amount but deliberately discards it before the
frontend model. A payment activity requires
nested `payment.id` to be present as a string/integer, but its mapper otherwise
discards and does not validate it; only the top-level story ID is the activity ID.
Timestamps accept RFC 3339 with offset or offsetless `YYYY-MM-DDTHH:MM:SS`, the
latter interpreted as UTC. Mutation comparisons preserve the exact instant. This
wire interpretation is independent of CLI display: typed instants are converted to
the system-configured time zone for terminal output, with UTC used when zone
discovery fails.

## 5. Exact implemented mobile `/v1` wire catalog
Unless stated otherwise, calls are authenticated, have no query/body, and use the
generic rules above. Paths include `/v1` to make the on-wire request explicit.
### 5.1 Authentication and account
#### A1 — password token initiation
```http
POST /v1/oauth/access_token
```

Device-only JSON body:

```json
{"phone_email_or_username":"<LOGIN_IDENTIFIER>","client_id":"1","password":"<PASSWORD>"}
```

Parsed error code `81109` is handled as an OTP challenge before ordinary status
handling. Exactly one `venmo-otp-secret` response header must be valid UTF-8,
nonempty, at most 4,096 bytes, and free of whitespace/controls. Direct success
requires 2xx JSON with a token at root `access_token` or `data.access_token`; it is
nonempty, at most 64 KiB, and free of whitespace/controls. An apparent success with
an empty/malformed body or missing/invalid token is authentication-outcome-unknown
and is not retried.
#### A2 — request SMS OTP
```http
POST /v1/account/two-factor/token
```

OTP-secret mode; exact body `{"via":"sms"}`. Generic success applies and an empty
2xx body is allowed.
#### A3 — complete OTP token issuance
```http
POST /v1/oauth/access_token?client_id=1
```

OTP-completion headers carry an exactly six-ASCII-digit code. There is no body or
JSON content type. Token envelopes and authentication-outcome-unknown handling are
identical to A1 direct success.
#### A4 — trust/register authenticated device
```http
POST /v1/users/devices
```

Uses the newly issued authenticated credential; no body. Generic success permits an
empty 2xx body.
#### A5 — current account
```http
GET /v1/account
```

Response is `{"data":<USER>}` or `{"data":{"user":<USER>}}`. ID and username are
required. Existing-credential callers require exact equality with the stored user ID.
### 5.2 People, wallet, activity, and pending requests
| Ref | Exact request | Supported response and semantic checks |
|---|---|---|
| **P1** | `GET /v1/users?query=<QUERY>&limit=<LIMIT>&offset=<OFFSET>[&type=username]` | `data` array or `data.users`; each item is a user and count ≤ limit. Server pagination is ignored; §7 synthesizes offsets. |
| **P2** | `GET /v1/users/<USER_ID>` | `data` user or `data.user`; returned ID must exactly equal the path ID. |
| **P3** | `GET /v1/users/<SUBJECT_USER_ID>/friends?limit=<LIMIT>&offset=<OFFSET>` | `data` user array and optional `pagination.next`; count ≤ limit and any next URL passes §7 for the exact selected subject path. The result is only the friends visible to the authenticated viewer. |
| **P4** | `POST /v1/friend-requests`, form body `user_id=<TARGET_USER_ID>` | 2xx JSON must contain exact target `data.user`; then P2 must prove `request_sent_by_you` for send or `friend` for accept. |
| **P5** | `DELETE /v1/users/<SELF_USER_ID>/friends/<TARGET_USER_ID>`, no body | Any complete 2xx status is provisionally accepted because signer-verified current implementations disagree on response body; then P2 must prove `not_friend`. |
| **W2** | `GET /v1/account` | Requires direct string fields `data.balance` and `data.balance_on_hold`, each an exact signed USD decimal with ≤2 fractional digits. It never infers external balances. |
| **AC1** | Self: `GET /v1/stories/target-or-actor/<SELF_USER_ID>?limit=<LIMIT>&social_only=false[&before_id=<TOKEN>]`. Other personal user: same path with `<SUBJECT_USER_ID>`, `limit`, and optional `before_id`, without a public-only filter. | `data` story array and optional `pagination.next`; count ≤ limit. Self accepts the three supported classes below. Other-user pages accept only payment stories with exactly one subject party and strict audience checks; their amounts are not exposed by the CLI. |
| **AC2** | `GET /v1/stories/<ACTIVITY_ID>` | `data` story or `data.story`; top-level story ID must exactly equal the path ID. Optional `likes` and `comments` are counted embedded collections; count may exceed embedded rows, and a next link or unequal count marks the collection partial. |
| **AC3** | `POST /v1/stories/<ACTIVITY_ID>/likes`, no body | Preflight AC2 requires complete liker data proving the authenticated user absent. Any complete 2xx status is followed by AC2 proving the user embedded. |
| **AC4** | `DELETE /v1/stories/<ACTIVITY_ID>/likes`, no body | Preflight AC2 requires the authenticated user embedded. Any complete 2xx status is followed by AC2 with complete liker data proving the user absent. |
| **AC5** | `POST /v1/stories/<ACTIVITY_ID>/comments`, JSON `{"message":"<TEXT>"}` | 2xx JSON returns one created comment; ID, authenticated author, and exact message must match, then AC2 must embed that exact comment once. |
| **AC6** | `DELETE /v1/comments/<COMMENT_ID>`, no body | The route has no parent activity input. The CLI sends exactly one request after default-No confirmation (or `--yes`) and accepts a complete 2xx response. Venmo enforces authorization. Without an activity ID the CLI cannot preflight membership/authorship/text or automatically prove absence; users must verify independently before retrying an uncertain outcome. HTTP 404 is the confirmed rejection `Comment not found.` |
| **R1** | `GET /v1/payments?action=charge&status=pending,held&limit=<LIMIT>[&before=<TOKEN>]` | `data` records and optional `pagination.next`; every record is exact `charge`, status `pending` or `held`, positive, involves self exactly once, and count ≤ limit. Direction is derived and filtered locally. |
| **R2** | `GET /v1/payments/<REQUEST_ID>` | `data` record or `data.payment`; exact path ID. Mapper permits `charge`/`pay` and bounded status, but public `requests info` narrows to open `charge`; mutation preflight is stricter below. |

Other-user AC1 is documented by the historical unofficial iOS/client-1 API inventory and its
companion wrapper as `/stories/target-or-actor/{user_id}` with `limit`/`before_id`. Current
signer-verified Android 10.31.1 and 26.13.0 independently retain that route for normal personal
profile feeds. Android uses a separate `only_public_stories=true` call path for business/public
profiles, which is why the CLI fails those profile types closed rather than reusing the personal
contract.

AC2–AC6 are pinned by signer-verified Android 26.13.0 classic story detail and mutation callers.
The current app reads liker/comment rows from the story's embedded counted collections; no dedicated
comments-list or likers-list route is claimed. It locally rejects blank comments and caps them at
2,000 characters. The CLI preserves distinct activity, comment, user, nested payment, and ledger ID
namespaces. Arbitrary Feed V2 emoji reactions and comment editing are not implemented.

Current signer-verified Android 26.13.0 also routes another personal profile's external user ID
through its friends-list data source to `GET /v1/users/{id}/friends` with `limit` and `offset`.
Official Venmo guidance says friend-list visibility can be public, friends-only, or private and that
a user can opt out of appearing in others' lists. The CLI therefore labels these records as visible
friends, requires an authoritative personal profile, and does not infer completeness.

P4/P5 static evidence comes from Android artifacts signed by Venmo certificate SHA-256
`c77e2631ac0451c086f25f79ae258238afa400961e8d599db78e25eadcdcc947`: 9.26.0 APK
`b1eaf465d791da85c1f73f84e67ce16f5cc2e9ceb81ea9c41cb199edc6335832`, 10.31.1 APK
`31e22397a40d99ea0ea39ad3506602b1b6a7d45c98ce1528763425d2b4d9b0d6`, and 26.13.0 base
APK `0eff4f830aa402da999631bbf7dbbf8c345a0a262b4521ae1f510ebb387ca04c`. Public and
GitHub searches found only friend-list implementations, not an independent mutation contract.
Android's client-4 session lineage does not prove authorization for the CLI's client-1 token.
#### W1 — payment methods
```http
GET /v1/payment-methods
```

Response is `{"data":[<METHOD>,...]}` or
`{"data":{"payment_methods":[<METHOD>,...]}}`. Read-only listing requires unique
valid IDs and accepts optional `name`/`display_name`/`label`,
`type`/`payment_method_type`, `last_four`/`lastFour`, and
`is_default`/`isDefault`. A read-only default is also inferred when `role`,
`payment_method_role`, `peer_payment_role`, or `merchant_payment_role`
case-insensitively contains `default`.

The peer-funding consumer used by `pay user` and modern request acceptance is stricter:

1. every method has exact case-insensitive `peer_payment_role` `default`, `backup`,
   or `none`; `none` is skipped and any unknown/omitted role fails the response;
2. participating type is `balance`, `bank`, or `card`; at most one peer-eligible
   balance source is retained, external bank/card sources are retained with their
   roles, and unknown participating types fail closed;
3. IDs are valid and unique across the full response;
4. method fee is proven only by `fee.calculated_fee_amount_in_cents` as JSON `u64`;
   otherwise it is unknown; and
5. without `--source`, choose the balance source when authoritative available balance
   covers the full amount. Otherwise choose the unique eligible external default even
   with backup methods present; if none is default, only a sole eligible external
   method is selected. Multiple no-default candidates are ambiguous and multiple
   eligible external defaults fail. Response order and fee never choose a source; and
6. `--source` must exactly match a retained peer-eligible balance, bank, or card ID.
   An explicit balance source must cover the full amount. Unknown or ineligible IDs
   fail before confirmation, and an explicit external source is not blocked by other
   external defaults because no automatic external choice is needed.
#### Activity story classes and viewed-user scope
An activity has a valid top-level ID and exactly one non-null class:

- **payment:** nested status/action/actor/target and a positive amount when the authenticated user
  participates; external social records may redact the amount. A list page requires exactly one
  selected feed subject party; detail preserves the absolute actor/target. Nested creation time,
  note, and audience use the documented top-level
  fallbacks. Time uses `payment.date_created`, then story `date_created`. Nested
  `payment.id` must deserialize as a string/integer but is discarded without opaque-ID
  validation and is not the activity ID;
- **transfer:** status/type/positive amount and exactly one of `source` (incoming)
  or `destination` (outgoing); the external endpoint has name/type and optional
  last four; transfer ID is validated when present; time uses
  `transfer.date_requested`, then story `date_created`; or
- **authorization:** valid authorization ID, status, positive amount,
  `merchant.display_name`, and user exactly self; always outgoing; time uses
  `created_at`, then story `date_created`.

Without `--user`, AC1 retains the stored-self contract above. An exact own username is optimized to
that same path without search. Another username uses bounded exact P1 traversal and authoritative
P2 equality validation, requires exact personal profile type, and then requests one normal native
profile-feed page. Direction and counterparty are derived relative to that selected subject.
Other-user pages reject transfer/authorization records, missing or unknown audiences, and any
payment where exactly one actor/target is not the subject. Public/friends records rely on server
visibility; private records additionally require the authenticated viewer to be the other party.
Other-user amounts are never retained or rendered, including when the private response supplies
one. Current-user activity retains its required exact amount.
AC2 maps optional classic `likes` and `comments`. Each collection requires `count >= data.len()`,
unique user/comment IDs, valid users, and valid comment ID/message/timestamp. Completeness requires
both no `pagination.next` and exact count/data equality. Output always labels embedded rows as
complete or partial; mutation preflight never infers absence from partial data.
Timestamp parsing accepts RFC 3339 and Venmo's naive-UTC whole-second or fractional-second forms;
fractional precision is retained internally while terminal rendering remains whole-second.
The continuation remains bound to the exact subject path, limit, effective false filters, and one
opaque progressing `before_id`.

AC2 remains globally keyed by story ID and accepts no subject argument. Payment detail preserves and
renders absolute actor and target identities, so it does not invent a viewer-relative direction.
An external private payment must include the authenticated viewer; transfers and authorizations
continue to require current-account ownership.

Pending, failed, or another bounded remote status remains readable data rather than
being converted to command failure.
#### R2 preflight narrowing
`requests info` allows only `action: charge` with status `pending` or `held`, in
either direction. Accept/decline additionally require an incoming `charge`, status
exactly `pending`, complete note, supported audience, requester username, and valid
creation time. Cancel requires an outgoing `charge` in `pending` or `held` state with the same
complete immutable fields. Accept then uses P2 and proves the requester
nonself/personal/payable; decline does no P2 and proves neither profile type nor
payability.
#### T1 — current transfer options
```http
GET /v1/transfers/options
```

The required `data` envelope contains `preferred_transfer_type.in|out` (null,
`standard`, or `instant`) and required `standard`/`instant` branches. Each branch
contains `eligible_sources`, `eligible_destinations`, `fee`, and a bounded string
`transfer_to_estimate`. Instrument arrays are capped at 100 per direction/speed;
each item requires a branch-unique valid ID, bounded name/asset/type/estimate, an
exactly four-visible-character suffix, and Boolean
`is_default`. Fee minimum/maximum/percentage numbers are retained as unit-unverified
strings; any additional non-null fee field is recorded. Unknown preferred speeds,
invalid IDs/text, malformed envelopes, and oversized arrays fail the read contract.
### 5.3 Eligibility and financial writes
#### T2 — create standard outbound bank transfer
```http
POST /v1/transfers
```

```json
{"amount":<INTEGER_CENTS>,"destination_id":"<SELECTED_STANDARD_BANK_ID>","final_amount":<SAME_INTEGER_CENTS>,"transfer_type":"standard"}
```

Success requires exact HTTP 201 and direct `data` fields: valid transfer ID and
`date_requested`, status exactly `pending`, type exactly `standard`, dollar `amount`,
unsigned `amount_cents`, `amount_fee_cents`, and `amount_requested_cents`, plus a
destination with matching ID/type/four-character suffix. Requested cents must equal
the plan; net cents plus fee cents must equal requested cents; dollar amount must
equal net cents exactly. Empty/malformed JSON, any other status, any mismatch, every
non-201/non-2xx, challenge, or transport uncertainty is ambiguous. Pending proves
accepted submission, not bank settlement.

#### F1 — blank-source payment eligibility
```http
POST /v1/protection/eligibility
```

Exact body:

```json
{"funding_source_id":"","action":"pay","country_code":"1","target_type":"user_id","note":"<NOTE>","target_id":"<RECIPIENT_USER_ID>","amount":<INTEGER_CENTS>}
```

The `data` envelope requires string `eligibility_token`, Boolean `eligible`, array
`fees`, string `fee_disclaimer`, and optional string `ineligible_reason`. If
`eligible` is false, the client returns a confirmed, nonambiguous prewrite
eligibility rejection before consuming token or fee semantics, and no F2 follows.
If true, the token must be nonempty, at most 4,096 bytes, and free of
whitespace/controls; every fee must have unsigned
`calculated_fee_amount_in_cents`, and the checked sum must not overflow. This
nonfinancial preflight intentionally uses a blank source: its fee proves neither the
selected W1 method's fee nor the final payment fee. The CLI therefore does not present
that amount as a payer fee or add it to the displayed payment total.
#### F1P — source-bound protected-payment eligibility
```http
POST /v1/protection/eligibility
Content-Type: application/x-www-form-urlencoded
```

Used only for explicit `pay user --protect`, after W1/W2 source selection. Exact fields, in order:

```text
target_type=user_id&target_id=<RECIPIENT_USER_ID>&country_code=1&amount=<INTEGER_CENTS>&note=<FORM_ENCODED_NOTE>&funding_source_id=<SELECTED_PEER_SOURCE_ID>
```

There is no `action` field. The response must prove `eligible: true`, provide a valid bounded
eligibility token, and contain at most 16 complete fee objects. Every object has bounded valid
`product_uri`, `applied_to`, and `fee_token`; optional unsigned `base_fee_amount`; optional
nonnegative JSON-number `fee_percentage`; and unsigned `calculated_fee_amount_in_cents`. Exactly
one object must have a `product_uri` beginning with
`venmo:product:buyer_protection:`. Missing or duplicate matching fees, unknown fields, malformed
values, denial, or a selected fee above the payment amount fails before confirmation. Nonmatching
valid fee objects are ignored; response order never selects between matching objects.
#### F2 — create peer payment
```http
POST /v1/payments
```

```json
{
  "uuid": "<CLIENT_UUID_V4>",
  "user_id": "<RECIPIENT_USER_ID>",
  "audience": "<REQUESTED_AUDIENCE>",
  "amount": <POSITIVE_USD_JSON_NUMBER>,
  "note": "<NOTE>",
  "eligibility_token": "<ELIGIBILITY_TOKEN>",
  "funding_source_id": "<SELECTED_PEER_SOURCE_ID>"
}
```

Protected F2 adds the following exact fields while retaining the same base request:

```json
{
  "transaction_type": "goods_services_protected",
  "fees": [{"product_uri":"<F1P_VALUE>","applied_to":"<F1P_VALUE>","fee_token":"<F1P_VALUE>","base_fee_amount":<OPTIONAL_U64>,"fee_percentage":<OPTIONAL_NONNEGATIVE_JSON_NUMBER>,"calculated_fee_amount_in_cents":<U64>}],
  "metadata": {"quasi_cash_disclaimer_viewed": false}
}
```

The fee array is exactly the one F1P buyer-protection fee; optional fee fields are omitted only when
F1P omitted them. Request `transaction_type` and response `type` are deliberately different current
wire values: protected success requires response `type` exactly `payment_protected`.
Missing or different type after a 2xx write makes the outcome unknown.
This proves only that Venmo tagged the transaction, not that an item qualifies for reimbursement.

One UUID is generated before the initial attempt; it is not a retry license. A 2xx
response must use exactly `{"data":{"payment":<RECORD>}}`, with valid creation
time, valid payment ID, `action: pay`, status `settled`, `pending`, or `held`, exact
amount, self as actor, intended recipient as target, exact note, and a supported
audience equal to or more restrictive than requested. Every post-send proof failure
is outcome-unknown.

An initial F2 HTTP 403 whose root `error.title` is exactly `OTP_STEP_UP_REQUIRED` is the only
supported P2P step-up. Code `1396` alone is not sufficient. The exact challenge proves rejection before creation rather than an unknown
write. The CLI requires an interactive terminal, sends F2O, reads one hidden six-digit
OTP, sends F2V, and—only after exact verification success—sends F2 once more with the
same plan and UUID plus this additional field:

```json
"metadata":{"verification_method":["sms_otp"],"verification_status":"sms_otp_verified"}
```

That second F2 is an explicit challenge continuation, not an automatic retry. It occurs
at most once; a repeated challenge stops. `--yes` cannot bypass the hidden OTP prompt.
The code is never accepted through an argument, printed, logged, or stored. For protected payment,
the verified metadata merges these two verification fields with
`quasi_cash_disclaimer_viewed:false`; the transaction type, exact F1P fee, source, token, plan, and
UUID are otherwise unchanged.

#### F2O — issue P2P SMS OTP
```http
POST /graphql
```

This orchestration endpoint is the sibling of `/v1`, on the same fixed
`https://api.venmo.com` origin, and uses authenticated device/session headers. Exact JSON:

```json
{"query":"mutation SendOtp($input: SendOtpRequest!) { sendOtp(input: $input) { success } }","variables":{"input":{"flowType":"P2P","deliveryMethod":"sms","uuid":"<SAME_CLIENT_UUID_V4>"}}}
```

Only a 2xx GraphQL envelope with no `errors` and exact true
`data.sendOtp.success` proves issuance.

#### F2V — verify P2P SMS OTP
```http
POST /graphql
```

```json
{"query":"mutation validateOtp($input: ValidateOtpRequest!) { validateOtp(input: $input) { validated reasonCode } }","variables":{"input":{"flowType":"P2P","otp":"<SIX_DIGIT_OTP>","uuid":"<SAME_CLIENT_UUID_V4>"}}}
```

Exact `validated: true` succeeds; matching the official app, `reasonCode` is inspected
only when `validated` is false. Exact false reasons `otpIncorrect`, `otpExpired`,
`otpUnexpected`, and `tooManyIncorrectAttempts` are terminal confirmed failures.
GraphQL errors, malformed envelopes, and unknown false-result combinations stop without
the verified F2.

#### F3 — create peer request
```http
POST /v1/payments
```

```json
{
  "uuid": "<CLIENT_UUID_V4>",
  "user_id": "<RECIPIENT_USER_ID>",
  "audience": "<REQUESTED_AUDIENCE>",
  "amount": <NEGATIVE_USD_JSON_NUMBER>,
  "note": "<NOTE>"
}
```

No eligibility or funding field is sent. The same exact creation envelope is
required. The returned record must have valid request ID, `action: charge`, exact
`pending`, a **positive** amount equal to the requested magnitude, self as actor,
recipient as target, exact note, valid creation time, and audience equal to or more
restrictive than requested. Post-send proof failure is outcome-unknown.

F3 uses the same exact P2P challenge detector and F2O/F2V flow as F2. Its session ID is the original
client request UUID. After validation, one F3 continuation retains that UUID and all original fields
while adding `metadata.verification_method: ["sms_otp"]` and
`metadata.verification_status: "sms_otp_verified"`. A repeated challenge stops.

F2/F3 non-2xx errors are interpreted only in their peer-creation operation context. Exact code
`1393` is a terminal decline, exact `10104` a terminal risk decline, and exact `230500` a pending
teen-account restriction. Codes `10200`/`10201` require an unsupported in-app scam-warning review;
payment code `17461` requires Plaid relinking, while F3 has no funding method and does not inherit
that meaning. Codes `10101`–`10103`, `10105`–`10199`, and
`60000`–`60099` receive fixed risk/decline guidance but remain outcome-unknown. Exact `10100` and
`10104` use their narrower dossiers rather than a range fallback. The same numbers under request
acceptance, request mutation, or transfer APIs do not inherit these meanings.

#### F4N — request-approval notification resolution
```http
GET /v1/notifications?acknowledged=false
```

Response `data` must be an array of at most 200 notifications. The CLI supports both evidenced
representations: a legacy request notification with root `id` plus `payment.id`, and the current
wrapper with `additional_properties.request.id` plus
`additional_properties.request.payment.id`. It finds exactly one record whose payment ID equals the
canonical R2 request ID and retains the corresponding request-action ID (root `id` for the legacy
shape or nested request `id` for the current shape) in the redacted immutable approval plan. The
current wrapper's outer notification ID is not an approval-path ID. No match, multiple matches,
conflicting representations, an invalid action ID, or an unsupported envelope fails before funding
selection, confirmation, or any write. F4S uses the resolved action ID; the canonical payment ID
and current outer notification ID are not interchangeable with it.

#### F4E — source-bound request-approval eligibility
```http
POST /v1/protection/eligibility
Content-Type: application/x-www-form-urlencoded
```

Exact fields are `target_type=user_id`, requester `target_id`, `country_code=1`, positive
integer-cent `amount`, request `note`, and selected peer-source `funding_source_id`. This is distinct
from F1's blank-source JSON payment eligibility. The response must prove `eligible: true` and
provide a valid eligibility token. Optional `fees` is a bounded array of exact objects containing
required bounded `product_uri`, `applied_to`, and `fee_token` strings, optional nonnegative integer
`base_fee_amount`, optional nonnegative JSON-number `fee_percentage`, and required nonnegative
integer `calculated_fee_amount_in_cents`. The calculated cents are summed with checked arithmetic;
denial is a confirmed prewrite rejection, while a missing/invalid token, malformed/oversized fee,
unknown fee field, or total overflow fails before approval.

#### F4S — source-funded request approval
```http
PUT /v1/requests/<REQUEST_ACTION_ID>
```

The JSON body contains the exact selected peer-source `funding_source_id`, F4E `eligibility_token`, and
`metadata.quasi_cash_disclaimer_viewed: false`. An unprotected acceptance omits F4E fee records.
Explicit `--protect` includes normalized validated `fees`, preserving omitted versus explicit
empty. The checked fee may not exceed the request amount; terminal output treats it as Venmo's
estimated seller/recipient deduction and computes estimated recipient proceeds rather than adding
it to the payer's amount. The body contains no `action` field. A complete 2xx JSON object without `data.url` is accepted;
the current native contract need not return a payment ID or status. A webview continuation,
non-object success, malformed/empty response, non-2xx result, or transport uncertainty after
possible transmission is outcome-unknown and is never retried.

An initial F4S HTTP 403 with root `error.title` exactly `OTP_STEP_UP_REQUIRED` is the one supported
acceptance step-up. Root `error.metadata.uuid` must be a valid UUID; missing, invalid, or nested
elsewhere fails closed. F2O/F2V use that server UUID. After validation, one F4S continuation uses the
same request-action ID and preserves source, eligibility token, fees, and quasi-cash metadata while
merging `verification_method: ["sms_otp"]`, `verification_status: "sms_otp_verified"`, and
`uuid: "<SERVER_CHALLENGE_UUID>"`. A repeated challenge stops. The OTP is never included in F4S.
Exact F4S HTTP 403 root titles `RISK_DECLINED` and `RISK_INELIGIBLE` are terminal risk
rejections. Code `17461` reports that the selected bank requires Plaid relinking in the official
app. Other numeric codes remain outcome-unknown except the separately proven HTTP 400/root `1360`
duplicate dossier. No F4S meaning is inferred from a nested title/code or another API's mapping.
#### F5 — decline an incoming request
```http
PUT /v1/payments/<REQUEST_ID>
```

Exact body: `{"action":"deny"}`. A direct or wrapped detail response must preserve
the exact request ID, `action: charge`, requester→self orientation, amount, note,
audience, and creation instant, and must prove status exactly `cancelled`. It is
subject to financial ambiguity policy even though no money/funding field is sent.

**Code truth:** `accept` is restricted to an original audience exactly `private`.
`decline` permits **any supported audience**—`private`, `friends`, or `public`—and
requires exact preservation. The code path is audience-generic, but the retained exact
success fixture and live reconciliation both use `private`; non-private success has not
been independently pinned.

#### F6 — cancel an outgoing request
```http
PUT /v1/payments/<REQUEST_ID>
Content-Type: application/x-www-form-urlencoded
```

Exact form body: `action=cancel`. A direct or wrapped detail response must preserve the exact
request ID, `action: charge`, self→recipient orientation, amount, note, audience, and creation
instant, and must prove status exactly `cancelled`. Preflight requires an authoritative outgoing
record in exact `pending` or `held` state; incoming requests, completed payments, and terminal
requests send no write. F6 sends no money or funding fields and is subject to financial ambiguity
policy because it mutates payment-request state. The CLI attempts it once with no retry.
## 6. Implemented orchestration and local gates
### 6.1 Credential lifecycle
Credentials use OS-keyring service/account `venmo-cli` / `default`, not an API. The
≤128 KiB v1 JSON stores schema version, validated token, device/user IDs, bare
username, optional display name, and save time; legacy TypeScript envelopes remain
readable. Secrets are hidden-prompted, never normal arguments/environment/files;
password/OTP values are zeroized and not stored. Login needs a terminal, always
prompts for a new trusted device ID, and every save is reread and checked for v1
equivalence.

- Password login may replace any readable existing credential, including a different
  account, but only after A5 validates the newly issued token. Failure before storage
  leaves the prior entry untouched. Direct A1 runs A5, saves, and skips A4. OTP runs
  A2/A3, A5, save, then A4; A4 failure reports incomplete trust but leaves the
  validated credential stored.
- Replacement does not revoke the previous remote token; output directs the user to
  official Venmo session controls when invalidation is required.
- Logout deletes only the local keyring entry, makes no remote call, and reports that
  the bearer token may remain valid. Missing state succeeds.
- Status is A5 validation, not token-presence inspection.
### 6.2 Financial workflows
All mutation workflows support a full-preflight `--dry-run` at the boundary between flushed details
and authorization. This path sends none of the final mutation calls described below and never enters
OTP or reconciliation. Failure to write its local completion line is an ordinary command-output
failure because no mutation has started.

**Pay:** A5 validates self; recipient search/detail proves a nonself personal/payable
user; W2 captures balance; strict W1 supplies peer-eligible balance and external sources; the
shared policy selects sufficient balance, the requested exact source, or the unique default/sole
external source. Ordinary payment uses F1. Explicit `--protect` instead uses source-bound F1P,
requires one exact buyer-protection fee, and never falls back to ordinary payment. Either branch
confirms eligibility or denies it without F2. The CLI generates a UUID, renders and
flushes validated payment details, then requires default-No confirmation unless `--yes`
(noninteractive always needs `--yes`). It installs interruption protection before
 the initial F2. Only an exact HTTP-403/root-title challenge can continue through one
interactive F2O/F2V sequence and one same-UUID verified F2; this is not an automatic
retry. Protected OTP continuation preserves F1P fields and merges verification metadata. The
selected source ID is submitted exactly, but neither W1, F1/F1P, nor the creation response proves
the final debit source, final seller fee, or item coverage.

**Request:** A5 and recipient resolution build an immutable F3 plan. There is no W1, W2, F1,
funding field, or balance gate. The CLI renders and flushes the account, recipient, amount, note,
requested audience, and create action, then requires default-No confirmation unless `--yes`
(noninteractive always needs `--yes`). It installs interruption protection before the initial F3.
Only the exact P2P challenge can add one same-client-UUID verified F3 continuation.

**Accept:** A5 and R2 prove an incoming exact-`pending`, exact-`private` request; P2 proves the
requester personal/payable; W2 supplies balance evidence. Every acceptance requires an exact F4N
match, then loads W1, applies the same balance/requested-source/unique-default-or-sole-external
selection policy as pay, and requires F4E before building an F4S plan. Unprotected plans discard/omit returned fee records;
protected plans retain normalized records, reject a fee above the request amount, and display the
estimated seller fee and recipient proceeds. Validated details are always flushed; unless `--yes`
is supplied, default-No confirmation follows. Interruption protection is then installed and the
initial F4S is attempted. Only the exact P2P challenge can add one verified F4S continuation using
the server challenge UUID; no legacy route is attempted.

**Decline:** A5 and R2 prove an incoming exact-`pending` request with any supported
audience. The CLI renders and flushes authoritative request-decline details, requires default-No
confirmation unless `--yes`, installs interruption protection, then attempts exactly
one F5. No P2/W1/W2/F1 runs, profile type/payability is unproved, and no money/funding
field is sent.

**Cancel:** A5 and R2 prove an outgoing exact-`pending` or `held` charge request owned by the
active account with complete immutable fields. The CLI renders and flushes cancellation details,
requires default-No confirmation unless `--yes`, installs interruption protection, and attempts
exactly one form-encoded F6. It cannot reverse a completed payment and never substitutes incoming
F5 denial.
### 6.3 Friendship workflows
**Add:** A5 validates self and shared exact username resolution proves one authoritative,
nonself personal P2 target. Exact `not_friend` selects send; exact
`request_received_by_you` selects accept. Existing friend and outgoing-request states fail before
output or write. The CLI flushes friendship details, requires action-specific default-No
confirmation unless `--yes`, installs relationship-write interruption protection, and attempts
exactly one P4.

**Remove:** the same gates select unfriend only for exact `friend` and outgoing-request
cancellation only for exact `request_sent_by_you`. `not_friend` and incoming requests fail closed;
remove never silently declines an incoming request. After exactly one P5, P2 must prove
`not_friend`.

Both operations require post-write authoritative P2 reconciliation. A malformed/mismatched P4
success, failed reconciliation, wrong target, unexpected final status, interruption, or result
output failure exits 3 and must not be retried automatically. Static Android evidence establishes
the request/state contract, but current client-1 authorization has not been live validated.

### 6.4 Reads
Every public list fetches exactly one page. Full server next URLs are never followed
or printed. `activity list` and `requests list` do print validated raw continuation
tokens to stderr; default `friends list` and default `activity list` use stored self ID without A5.
Other-user friends and activity use exact P1/P2 resolution but no A5. A `--user` matching the
stored username case-insensitively keeps the self path and skips P1/P2. `activity info` uses the
unique story ID.
Request direction is local, so a displayed page can be empty and still
have a continuation.

## 7. Implemented pagination and recipient resolution
A server `pagination.next` is parsed only to reconstruct a local continuation. It must
resolve to the fixed HTTPS origin, port, and exact expected path; userinfo, password,
fragment, duplicate/disallowed query keys, and invalid values are rejected.

| Family | Allowed next query and progress rules |
|---|---|
| Friends | Only `limit`, `offset`; same limit; offset valid `u32` and strictly increases. Empty page plus next is rejected. |
| Activity | Only `before_id`, `limit`, `only_public_stories`, `social_only`; exact subject path and same limit. Self requires `social_only=false`; other-user continuations may omit it, but any supplied visibility/social flag must remain false. `only_public_stories=true` is always rejected. A new valid token is required, and empty page plus next is rejected. |
| Requests | Only `action`, `before`, `limit`, `status`; exact `action=charge`, `status=pending,held`, same limit, required new valid token. Empty source page plus progressing next is allowed. |

P1 ignores server pagination. A nonempty page exactly equal to limit synthesizes
`offset + returned_count` with checked `u32` arithmetic; short/empty pages end. A full
final page can therefore require an explicit empty-page invocation.

Exact username resolution is the only automatic pagination: P1 uses limit 50
for at most four pages/200 records, performs case-insensitive exact matching, and
continues enough to detect different IDs for the same exact username. Multiple are
ambiguous. No match at the bound is reported as username not found. One match at the
bound is usable only after P2 returns the same ID and username.

## 8. No-retry and ambiguous-outcome policy
F2–F6 and T2 are enabled financial-policy writes; P4/P5 are externally visible relationship
writes. A1/A3 use a parallel but distinct authentication issuance ambiguity.

- Proven pre-send construction/encoding failure or connect-stage DNS/TCP/TLS failure
  is ordinary and nonambiguous.
- A non-connect send timeout/disconnect is unknown.
- Once a response begins, redirect rejection, read/capture failure, truncation, 2 MiB
  overflow, and resource exhaustion are unknown.
- A complete non-2xx financial response is still unknown, except F2/F3/F4S HTTP 403 with root
  `error.title` exactly `OTP_STEP_UP_REQUIRED`, which is the confirmed prewrite P2P step-up; F2 HTTP
  400/403 with root `error.code` exactly `1396` but no exact title, which is a confirmed rejection; F2 HTTP
  400/root `13006`, which is a confirmed rejection; F2 HTTP 403/root `1360`, which is the
  same-recipient/amount/note duplicate rejection; and F2 HTTP 403/root `10100`, which is
  a server-side-check block with try-later-or-use-the-official-app guidance. Other complete
  non-2xx financial errors retain safe HTTP status and sanitized root-code suffix in the
  outcome-unknown error.
- Every other non-2xx F3/F4S/F5/F6 or transfer response remains unknown, even if it resembles stale
  state or a challenge.
- On 2xx, a controlling API code other than rendered exact `0`/`0.0`, empty/malformed
  JSON, wrong envelope, or any ID/party/amount/note/audience/time/status mismatch is
  unknown. The §4.2 first-code precedence still applies.
- No business-looking error is reinterpreted as success, and no unknown result is
  replayed automatically.
- For P4/P5, complete non-2xx responses are confirmed failures. Post-transmission transport
  uncertainty, malformed successful P4 JSON, target mismatch, failed/mismatched P2 reconciliation,
  interruption, or result-output failure is ambiguous exit 3. P5 accepts any complete 2xx body only
  provisionally and still requires P2 proof.

After preflight/authorization and installation of interruption protection, a ready
interrupt wins conservatively and exits 3—including before the write future is first
polled and in a same-poll tie with a ready result. Failure to install protection is an
ordinary pre-write internal failure. Financial exit 3 also covers failure to flush a
successful result to the operator; nonfinancial state writes have the same result-output boundary. Usage exits 2, ordinary failure/cancellation 1,
and success 0. Token issuance ambiguity is authentication failure, not financial exit
3, and is likewise never retried.

## 9. Implemented-mobile evidence ledger
| Date | Area | Source / state | Established boundary |
|---|---|---|---|
| 2026-07-17 | §§3–8 | Existing-client behavior / exact synthetic contract tests with residual edge-coverage gaps | Every enabled wire operation has retained service-free request coverage and representative response/failure coverage. Some code-accepted envelope alternatives and edge branches are not individually pinned. |
| 2026-07-11 | A5/session validation | Direct observation / controlled live validation | Matching token/device, A5 identity, and keyring save/reread. |
| 2026-07-11 | A1 direct login | Direct observation / controlled live validation | Trusted-device direct issuance; current login always prompts for a fresh device ID. Redundant post-direct-login A4 was rejected and removed. |
| 2026-07-11 | OTP/A4 | Historical lead + existing behavior / exact synthetic; residual gap | A1 challenge, A2, A3, and post-OTP A4 are implemented; no current controlled OTP success. Token lifetime/refresh unknown. |
| 2026-07-11 | Reads/story classes | Direct observation / controlled live validation | A5, W1/W2, P1–P3, AC1/AC2, R1/R2, all activity classes, and read smoke. Settled payment-ID detail returned HTTP 500. |
| 2026-07-11 | Pagination | Direct observation / controlled live plus residual gap | First→second pages for four families; friends exhaustion. Natural user-search/activity/request exhaustion remains unobserved. |
| 2026-07-12 | F2 and private F3 | Direct observation / controlled and reconciled live validation | Separately approved minimal-value operations matched synthetic contracts. |
| 2026-07-20 | F2 headers, SMS step-up, and rejections | Signer-verified Android 26.13.0 + maintained current mobile client + owner-approved reconciled live operations + exact synthetic | The current `/v1/payments` body remains compatible. The current profile reached HTTP 403/root title `OTP_STEP_UP_REQUIRED` with observed code `1396`, issued and verified SMS OTP through F2O/F2V, then completed one same-UUID verified F2 for an explicit-bank-source `$1.00` payment; an independent activity read found exactly one matching settled outgoing payment. Android establishes title—not code-only—challenge detection and `validated: true` precedence. A same-recipient/amount/note repeat received HTTP 403/root `1360`; current APK names it same-info denial and historical documentation specifies a 10-minute window. A note-varied rapid repeat received HTTP 403/root `10100`; public-current corroboration and repeated reconciliation establish that Venmo's server-side checks blocked the payment, but do not identify the specific check or its duration. Neither rejected attempt created activity. Old headers also produced `10100`, so that code is not treated as a header fingerprint. Actual debit source/final fee remain unproven. |
| 2026-07-20 | F1P/protected F2 | Signer-verified Android 26.13.0 + current official Purchase Protection guidance + existing web G&S evidence + exact synthetic + one owner-authorized reconciled client-1 payment | Native source-bound form eligibility, exact buyer-protection product namespace, complete singleton fee forwarding, request `transaction_type: goods_services_protected`, quasi-cash metadata, response `type: payment_protected`, and same-UUID OTP field preservation are implemented. The non-retried $1 validation received eligibility 201 and payment 200 without OTP, reconciled to exactly one settled activity, and exposed `payment_protected` through read-only payment detail. The first validator incorrectly expected the request-side value in the response and conservatively reported ambiguity; no second payment was attempted. Final seller fee, tax treatment, item eligibility, and actual debit source remain unproven. |
| 2026-07-14 | Visibility | Direct observation / reconciled live; owner-approved decision | Friends/public pay and more-private response behavior reconciled; non-private F3 accepted without another mutation. |
| 2026-07-14 | Retired legacy F4 | Direct observation / reconciled live validation plus historical lead | The action-only route once reconciled successfully, but current Android and client-1 evidence superseded it. Production acceptance no longer uses or falls back to this route. |
| 2026-07-20 | F4N/F4E/F4S approval | Signer-verified Android 10.31.1 and 26.13.0 + separately approved bounded client-1 notification probes + owner-run reconciled unprotected approvals + current official Purchase Protection/Buying and Selling guidance + exact synthetic | Native code submits a request notification's own `id` to `PUT /v1/requests/{id}` with source/token/fee options. Current production can wrap that request notification at `additional_properties.request`: the nested request has its own action ID and nested `payment.id`, while the outer notification has a third ID. A bounded read retained only paths/types and proved this exact shape. Using the outer wrapper ID returned HTTP 404 and reconciliation proved no mutation; using `additional_properties.request.id` returned 200, removed the pending request, and produced exactly one matching settled $20 activity. The CLI supports this current shape and the older direct `id`/`payment.id` shape, matching the R2 ID only to the payment ID and routing with the associated action ID. All acceptance now uses this route for balance or external funding. Android 26.13.0 additionally pins the server-UUID SMS continuation, which has exact synthetic coverage but no live validation. Native code submits fee records only when protection is selected; official guidance identifies them as seller deductions. Actual debit source/final fee and protected approval remain unproven. |
| 2026-07-14 | F5 | Direct observation / reconciled live validation | Private decline became exact `cancelled`, with no balance/activity movement. Non-private success is accepted by code but is not independently pinned by an exact success fixture or live validation. |
| 2026-07-20 | F6 | Signer-verified Android 26.13.0 + historical Go client + maintained Python client + exact synthetic | Current native code uses form-urlencoded `PUT /v1/payments/{id}` with `action=cancel` for outgoing pending/held charges and returns payment data. Independent clients corroborate the endpoint/action; exact synthetic tests pin form encoding, ownership/state gates, terminal proof, and no retry. Current client-1 live authorization is unproven. |
| 2026-07-16 | T1 | Direct observation / controlled live validation | One bounded read proved current bearer/device auth, direction/speed branches, standard bank candidates in both directions on the observed account, and fee/estimate structure; values/counts were not retained. |
| 2026-07-17 | T2 standard out | Direct observation / controlled and reconciled live validation + exact synthetic | One approved $0.01 HTTP-201 write returned direct pending standard transfer data; exactly one matching outgoing activity record had the same transfer ID. Exact body, fail-closed selection, strict response proof, and one-write ambiguity are implemented. |
| 2026-07-19 | P4/P5 friendship mutations | Direct observation of signer-verified Android 9.26.0, 10.31.1, and 26.13.0 + exact synthetic; residual auth gap | Current native route/body/state semantics are consistent across artifacts. No public mutation implementation was found. The CLI's client-1 session has not received a controlled live mutation validation; no canary has run. |
| 2026-07-19 | AC1 other-user personal feed | Historical public client-1 documentation/wrapper + signer-verified Android 10.31.1 and 26.13.0 + exact synthetic | The single-user `target-or-actor` route is long-lived and current. Normal personal feeds use the unfiltered route; business/public-only feeds are a distinct branch and remain unsupported. |
| 2026-07-19 | AC1 other-user response shape | Separately approved bounded structure-only probe using the active credential | Exact username resolution and one single-record personal-profile request returned the direct `data` story-array shape with a peer-payment record. Only paths and JSON types/nullness were emitted; no values, raw body, identifiers, names, note, amount, token, or continuation was retained. The probe made no mutation. |
| 2026-07-21 | AC2–AC6 activity social state | Signer-verified Android 26.13.0 classic and Feed V2 callers/models + exact service-free synthetic tests + one owner-approved reversible client-1 canary | On one private settled activity, bodyless like added the authenticated user, duplicate like failed in preflight, bodyless unlike removed it, and duplicate unlike failed in preflight. JSON comment add created exactly one self-authored comment; its fractional naive-UTC timestamp exposed and fixed a parser gap. Bodyless removal then reconciled exact absence, and the activity returned to zero likes/comments. No operation retried. Arbitrary emoji reactions, comment editing, non-author moderation, and broader durability remain unavailable/unproven. |
| 2026-07-20 | P3 other-user visible friends | Signer-verified Android 26.13.0 + current official privacy guidance + exact synthetic | Current native profile navigation passes another user's external ID through the friends data source to `GET /v1/users/{id}/friends` with limit/offset. The CLI uses bounded exact P1/P2 resolution, personal-profile gating, exact subject-path continuation binding, and privacy-aware visible-result wording. No live probe was needed or performed. |
| 2026-07-20 | Mobile compatibility headers | Android 26.13.0 + maintained current client + controlled live differential | Current version/session conventions are established and the profile reaches the recognized payment challenge; individual header necessity remains unknown. |
| 2026-07-20 | Financial error guidance | Signer-verified Android 26.13.0 + prior reconciled F2/F4S observations + exact synthetic cross-operation matrices | Error meaning is bound to operation, HTTP result, root code, and exact root title where required. Confirmed rejection and unsupported-continuation messages are distinct from APK-context hints that preserve exit 3. Wrong-operation, wrong-status, nested, malformed, and unknown values remain generic outcome-unknown. No additional live mutation was performed. |

## 10. Transfers: enabled options and standard out
### 10.1 Current command state
```text
venmo transfer options
venmo transfer out <AMOUNT_OR_ALL> [--speed standard] [--yes | --dry-run]
```

`transfer options` implements the outbound portion of T1. Its CLI rendering contains eligible
cash-out destination rows ordered as
`Id`, `Name`, `Type`, `Last 4`, `Default`, `Direction`, `Speed`, and `Estimated Completion`.
The semantically unclear API `asset_name` remains validated internally but is not rendered, and
preferred-speed and branch-level estimate/fee summaries are also omitted.
`transfer out` implements standard-bank T2 after A5/W2/T1 validation and default-No confirmation;
omitted speed defaults to standard and explicit `--speed standard` remains valid. The positional
selector accepts either the existing positive exact-decimal amount or exact lowercase `all`.
Transfer-in is
intentionally unsupported. No instant, debit, manual destination, or challenge-continuation
command is accepted.

### 10.2 Controlled current mobile options evidence
On 2026-07-16, one owner-approved, non-retried, fixed-origin
`GET /v1/transfers/options` used the active bearer plus device ID and returned HTTP
200 JSON. No raw body, credential, ID, name, account fragment, amount, fee, limit, or
count was retained. Sanitized structure proved:

- `preferred_transfer_type.in: null` and `.out: "standard"`;
- nonempty `standard.eligible_sources` and `.eligible_destinations` containing
  string ID/name/asset/type/last-four/estimate plus Boolean `is_default`; observed
  type was `bank`;
- empty instant source/destination arrays;
- standard fee minimum/maximum/percentage null, while instant fee fields were
  numeric; and
- branch-level transfer estimates and additional unrelied-on fields.

This direct observation proves current direction/speed partitioning and account-specific standard
bank eligibility, not universal support, units, write semantics, debit behavior, or fees. The
implemented T1 view ignores source metadata, preserves the standard/instant destination branches,
bounds each destination array to 100, and validates all fields relied on by cash-out.

### 10.3 Standard-out contract
The command performs A5 → W2 → T1 and requires exact stored/current account equality. An explicit
amount must be ≤ available balance. `all` requires positive W2 available cents, safely converts that
one signed snapshot to positive `Money`, excludes on-hold funds, and preserves both the selector and
resolved amount in the immutable plan. Zero or negative availability fails before T1 or T2. This is
not an atomic drain: concurrent balance changes can cause rejection or leave funds that arrived
after the snapshot. The command then selects only
`standard.eligible_destinations`. It accepts exact type `bank`, requires absent
known/additional non-null standard fee metadata, rejects duplicate IDs and multiple
defaults, chooses the unique default even with backups or otherwise a sole candidate,
and rejects multiple nondefaults. It never uses response order or accepts a raw CLI
destination ID. Validated transfer details are flushed before the write; `all` details disclose the
resolved amount, snapshot source, on-hold exclusion, and non-atomic behavior. Unless `--yes` is
supplied, default-No confirmation follows, with an action-specific entire-displayed-balance prompt
for `all`.

The exact current write, initially sourced from the historical lead and then
controlled-live validated, is one financial, non-retried:

```http
POST /v1/transfers
content-type: application/json
authorization: Bearer <TOKEN>
device-id: <DEVICE_ID>
```

```json
{"amount":<INTEGER_CENTS>,"destination_id":"<SELECTED_STANDARD_ID>","final_amount":<SAME_INTEGER_CENTS>,"transfer_type":"standard"}
```

The adapter encodes this as `FinancialWrite`, so post-connect uncertainty is
ambiguous. Success is only the exact T2 HTTP-201/direct-data contract in §5.3. Output
reports requested amount, selector source when `all` was used, validated net amount, and fee; exact `pending` means
accepted, not settled. Empty or mismatched JSON, status-only 2xx, non-201, interruption, and output
uncertainty remain ambiguous. Known non-success T2 codes add fixed guidance for insufficient funds
(`1319`, `9903`), amount/limits (`1358`, `1346`, `1757`, `1758`, `1766`, `13010`, `80907`),
unavailable bank methods (`5204`, `5207`, `9902`, `1734`), account restrictions (`1361`, `1362`,
`1364`, `81005`), and transfer/risk declines (`1760`, `1763`, `99027`) without changing exit `3`.
Exact HTTP 403/root title `OTP_STEP_UP_REQUIRED` is an unsupported transfer SMS continuation and is
never retried. Codes listed only for instant cash-out or add-money are not assigned T2 meaning. The
historical client's status-only 200–204 rule is rejected.

One owner-approved canary on 2026-07-17 refreshed T1, sent exactly one $0.01 T2, and
made exactly two reconciliation reads with no retry or challenge continuation. T2
returned HTTP 201 JSON with numeric ID, `pending`, `standard`, exact `$0.01`,
requested/net/fee-cent fields, timestamp, and destination. The first activity page
contained exactly one matching pending outgoing standard transfer and its transfer
ID exactly matched the T2 response. The bounded balance read was valid, but no raw
balance or delta was retained. No raw response, credential, or destination ID was
retained, and the temporary canary helper was removed.

### 10.4 Corroborating evidence and boundaries
The 2022 historical source is pinned at
[`transfer_api.py`](https://github.com/evanpurkhiser/venmo-api-unofficial/blob/3e290a575ac2f638cc82a3d1a1580bf41bf503aa/venmo_api/apis/transfer_api.py)
and [`transfer.py`](https://github.com/evanpurkhiser/venmo-api-unofficial/blob/3e290a575ac2f638cc82a3d1a1580bf41bf503aa/venmo_api/models/transfer.py).
It supplied the paths/body above but chose the first standard destination, omitted
the device header, returned local `true`, and discovered no inbound write.

Current web build `OHCscQK3gMovYos3VtRgA` separately emits same-origin CSRF/cookie
`POST /api/transfer` for cash-out. It uses integer cents, SSR-partitioned standard/instant IDs,
instant net amount, and a code-1396 challenge coupled to GraphQL SMS OTP before a challenge-bound
PATCH. This is a browser BFF/orchestration contract, not mobile write proof, and cannot be
transplanted into the CLI.

Official help accessed 2026-07-15 establishes product-level standard/instant
cash-out and bank/debit add-money capability only:
[add money](https://help.venmo.com/cs/articles/adding-money-to-your-venmo-balance-vhel169),
[transfer to bank](https://help.venmo.com/cs/articles/how-to-transfer-money-to-a-bank-account-vhel293),
and [transfer issues](https://help.venmo.com/cs/articles/issues-with-a-bank-transfer-vhel114).

### 10.5 Remaining scope
Transfer-in/add-funds is intentionally unsupported and is not a future implementation gate.
Instant/debit cash-out, step-up, cancellation, and expedition remain independently gated. Standard response net/fee cents are
validated arithmetically, but T1 fee values and broader fee policy remain
unit-unverified. Do not repeat the canary merely to broaden evidence; any future
variant needs its own read, request, response, ambiguity, and reconciliation dossier.

## 11. Current static-web personal-account inventory
### 11.1 Shared browser transport and SSR boundary
Evidence: `_app-7c5e4c17c954f843.js`.

The REST manager has empty `baseUrl`, maps fetch methods, JSON-stringifies truthy
object bodies, and uses normal same-origin credentials. State-changing call sites or
the global handler add `csrf-token` and `xsrf-token` where shown. JSON 401/403
redirects to sign-in; other non-2xx becomes structured/status error; 2xx JSON passes
unchanged.

Apollo POSTs to runtime `NEXT_PUBLIC_ORCHESTRATION_LAYER_GQL_API`, fallback
`https://api.venmo.com/graphql`. When needed, `GET /api/auth` with the CSRF pair
supplies a cached `accessToken`. GraphQL can send bearer auth, `Venmo-Client-Id: 10`,
`Venmo-Device-Id`, optional `pwv-id-token`, and runtime locale/environment/risk
headers—browser orchestration auth, not mobile auth. All 37 route chunks export
`__N_SSP = true`; server implementations are absent. Output lists below are compact.
### 11.2 Money, peer payments, and requests
Cash-out and shared OTP are inventoried in §10. Evidence for the remaining money pages is
`pay-048a41b49c168600.js`,
`payment-ab4dcd083a4d7c8c.js`, `incomplete-50212b5d913b53ac.js`, and
`backup-payment-02d9a99e9c813a30.js`.

| Route / exact operation | Compact exact input | Key output/state handling |
|---|---|---|
| `/pay`: concurrent `POST /api/payments`, once/recipient | Base `{targetUserDetails:<TARGET>,amountInCents,audience,note,type:<PAY_OR_REQUEST>,acceptRisk:false,paymentUUID}`. `<TARGET>` is `{phone:<PHONE>}`, else `{email:<EMAIL>}`, else `{userId:<ID>}`; type exact `pay`/`request`; per-recipient cents and v4 UUID. Request adds nothing. Pay adds `fundingSourceID` and conditionally `quasiCashDisclaimerViewed:true`, e-check `metadata`, eligibility fields, `promotionToken`, or verified `verificationMetadata`. Single-recipient eligibility sends token+fees for G&S, token for eligible non-G&S, or token+reason when ineligible. | Reads `status,paymentId`; every 2xx JSON joins success list. Partial success makes whole-form replay unsafe; the UUID does not prove idempotency. |
| `POST /api/eligibility` | `{targetType:<USER_ID_EMAIL_OR_PHONE>,targetId,amountInCents,action:"pay",note}`; exact target types `user_id`, `email`, `phone`, precedence ID→email→phone. | Reads `eligible,eligibilityToken,ineligibleReason,fees`; ignores errors. |
| `POST /api/payments/pwu` | `{externalId:<RECIPIENT_ID>,lastFour:<USER_ENTERED_PLACEHOLDER>}` | Reads `{valid}` for `13012`; false blocks or permits `acceptRisk:true` retry. |
| P2P e-check retry | Original plus `metadata:{echeck_disclaimer_ack_result:"TRUE"}` after `1425`. | Feature-gated consent; dismissal regenerates UUIDs. |
| GraphQL `getUserFundingInstruments` (`network-only`) | No variables. | Profile capabilities; wallet IDs/types/names/assets/fees/metadata and `roles.peerPayments`/`merchantPayments`. Balance needs capability+coverage; otherwise peer `backup`; unverified bank routes to verification. |
| GraphQL `EstimatePromotionReward` | `{input:{businessId,transactionAmount?:{currencyCode:"USD",value:<CENTS_STRING>}}}` | Business-recipient feature; reads reward metadata/`redeemableToken`, sends `promotionToken`. |
| `/payment`: GraphQL `UpdateRedeemablePayment` | `{input:{action:<ACCEPT_OR_DECLINE>,token:<SSR_REDEEMABLE_TOKEN>,fundingSourceID?}}`; exact `accept`/`decline`; FI only accepts incoming request. | Returns `id,status,createdOn,completedOn,type`. Settled is view-only; other nonpending has no action; pending actor links to incomplete; incoming pay accepts without FI, request uses chooser, decline omits FI. No step-up. |
| `/payment`: `RedeemablePayment` document | **SSR contract only**: page receives payment/token; no client lookup or variables. | Reads amount/audience/ID/note/status/times/title/sender/type; update input reveals no lookup input. |
| `/incomplete`: `GET /api/payments/outgoing?userId=<SELF>[&nextId=<CURSOR>]&action=charge&status=held,pending` | Initial omits `nextId`; pagination inserts it after `userId`. | Appends `{payments,nextId}`. |
| `/incomplete`: `GET /api/payments/outgoing?userId=<SELF>[&nextId=<CURSOR>]&action=pay` | Initial omits `nextId`; lazy-tab pagination has shown order. | Same envelope. |
| `/incomplete`: `PUT /api/payments/<PAYMENT_ID>` | `{action:<REMIND_OR_CANCEL>}`; exact `remind`/`cancel`. | Body ignored; local reminded/completed update. `inReview`/completed have no action; no strict cancel proof. |
| `/settings/backup-payment`: GraphQL `updateBackupFundingInstrument` | `{input:{id:<SSR_FI_ID>,paymentContextId?:<PWV_CONTEXT_ID>}}` | Returns ID; supplied-list default has merchant role `backup`, not P2P selection. |

Web `/pay` error branches also include `1393` beneficial-owner verification and
`85000` OFAC review. Client amount helpers multiply JavaScript numbers and do not
consistently round; those UI conversions are not safe integer-money contracts.

`/payment-link` (`payment-link-d59e7309898eb23b.js`) is SSR/navigation only. It
consumes resolved personal/business recipient props and forwards `note`, `amount`, and
`txn` into a `/pay`/sign-in URL; no link-create or client resolver API exists.
### 11.3 Payment-method lifecycle
Evidence bundles/routes:
- `/settings/payment-methods`: `payment-methods-7199d614588f9a03.js`;
- `/settings/payment-methods/add` and `/settings/payment-methods/add/card`:
  `add-41407b507a0f48c4.js`, `card-7c17c176ca4b51f3.js`;
- `/settings/payment-methods/[id]` and `/settings/payment-methods/update/card/[id]`:
  `[id]-626ed446ba68f01e.js`, `[id]-d0ea51ba1a6a9ba8.js`;
- `/settings/payment-methods/bankaccounts/add` and its `/manual` child:
  `add-d105a6dfc8de21e7.js`, `manual-bbb73af5f6b6a563.js`;
- bank-account `added`, `verify`, and `verify/unauthorized` dynamic routes:
  `added-ea79767593ed638a.js`, `verify-c9c9062cf6bd7b45.js`,
  `unauthorized-ba07f37100498119.js`; and
- `/settings/payment-methods/verify/phone`: `phone-e0f6219615f64964.js`.
Initial list, FI/update-card detail, bank-added/verification state, and Plaid Link
token are **SSR contract only**. List fields cover IDs, instrument type/name/assets,
bank verification/last-four/unique ID, card issuer/last-four/Venmo-card/expiry, and
balance metadata. Exact types include
`balance`, `bank`, `card`, `creditCard`, `debitCard`, `externalWallet`,
`prepaidCard`, and `venmoCreditCard`; expiry is `active`, `expired`, or
`expiresSoon`.

| Exact Venmo call | Compact exact input | Output and state |
|---|---|---|
| GraphQL `deleteFundingInstrument` | `{input:{id:<FUNDING_INSTRUMENT_ID>}}` | Returns ID, toast/list. Venmo-card delete hidden; merchant-backup error special-cased. |
| `POST /api/auth/external-access-token` | CSRF pair + Venmo bearer; `{tokenType:"walletsdk_venmo"}`. | Reads secret `external_access_token` (never document); 30-minute buffered/coalesced cache. Three attempts are per token-fetch cycle, not whole flow. |
| `POST /api/payment-methods/card` (Braintree) | `{btNonce:<BRAINTREE_NONCE>,paymentContextId?:<PWV_ID>}` | Returned card/FI ID drives success. |
| `POST /api/payment-methods/card` (PayPal) | `{paypalFundingInstrumentId:<PAYPAL_FI_ID>,correlationId:<CORRELATION_ID>,paymentContextId?:<PWV_ID>}` | Registers PayPal card; failure triggers best-effort PayPal removal. |
| `PUT /api/payment-methods/card` | `{btNonce:<BRAINTREE_NONCE>,id:<FI_ID>,expirationDate:<MM_YY>}` | Updates tokenized expiry/CVV and returns to list. |
| GraphQL `addPlaidAccount` | `{input:{publicToken:<PLAID_PUBLIC_TOKEN>,metadata:{accounts:[{id,name}]},paymentContextId?}}` | Returns FI ID, then best-effort backup update. |
| GraphQL `updateBackupFundingInstrument` | `{input:{id:<NEW_FI_ID>}}` | Nonfatal attempt after Plaid/manual bank add. |
| `GET /api/bank-accounts/routing-number/<NINE_DIGIT_ROUTING>` | No body; global handler adds CSRF pair. | Response body ignored; a successful `clientManager.get` resolves the field validator and a thrown error rejects it. |
| `POST /api/bank-accounts/add/manual` | `{routingNumber:<ROUTING>,accountNumber:<ACCOUNT>}` | Reads `id,name,metadata.uniqueIdentifier`; backup attempt, then added route. Any microdeposit initiation is BFF-hidden. |
| `POST /api/bank-accounts/verify-deposits` | `{bankAccountId:<UNIQUE_ID>,amount1:<MINOR_UNITS>,amount2:<MINOR_UNITS>}` | Success returns; distinguishes two-deposit/max-attempt errors; no auto-retry. |
| GraphQL `resendPhoneVerification` | `{input:{phoneNumber:"+" + <SSR_CURRENT_USER_PHONE>}}` | Scalar ignored; auto-sent once on mount and by resend. This is bank-flow verification only. |
| GraphQL `verifyPrimaryPhone` | `{input:{phoneNumber:"+" + <SSR_CURRENT_USER_PHONE>,smsCode:String(<FOUR_DIGIT_CODE>)}}` | Reads `success`; selected `phoneNumber`, `phoneClaim`, `hasAssociatedAccount` are ignored. True routes to manual bank add. |

Bank flow concatenates literal `+` with SSR `currentUser.phone`; its current-user
lookup is absent, so E.164 is unproved. Routing is exactly nine digits. Each deposit
UI value is 1–99 cents; body uses `Math.floor(<DISPLAYED_DOLLARS> * 100)`. These are
local checks, not broader server policy.

For card add, runtime flag `wallet_convergence_add_card` selects PayPal when enabled
and Braintree when disabled; the static artifact does not establish its value. The
external-access-token request is eagerly preloaded on form mount before branch
selection, and preload errors are swallowed. Direct PayPal `AddCard`, merchant-only
`AutodetectCard`, and cleanup `RemoveCard` use a helper that clears the cached token
after a caught error and can retry the direct operation once. This is captured
third-party-client behavior, not a retry recommendation for a new client.
#### Non-Venmo payment-method boundaries
| Provider | Exact captured boundary | Role in complete flow |
|---|---|---|
| **Plaid (third party)** | Loads `https://cdn.plaid.com/link/v2/stable/link-initialize.js` with SSR token. | Internals absent; success token/account IDs/names go to `addPlaidAccount`; no exchange path visible. |
| **Braintree (third party)** | `braintree-web` 3.79.0 bootstraps via `POST https://payments.braintree-api.com/graphql` `ClientConfiguration`, or legacy `GET https://api.braintreegateway.com:443/merchants/<MERCHANT>/client_api/v1/configuration`. | Omitted authorization is not Venmo credential; runtime branch unknown. |
| **Braintree (third party)** | GraphQL `TokenizeCreditCard` at `https://payments.braintree-api.com/graphql`, sanitized card/expiry/CVV/postal and `options:{validate:false}`; or legacy `POST <clientApiUrl>/v1/payment_methods/credit_cards`. | Only nonce reaches Venmo; completed gateway unknown. |
| **PayPal (third party)** | Runtime wallet GraphQL, fallback `https://api.paypal.com/graphql`; `AddCard` sends card, expiry, partial US postal, CVV, optional brand, and profile first/last names with empty fallbacks. | Returns PayPal card ID. |
| **PayPal (third party)** | Merchant `AutodetectCard({input:{cardNumber:<SANITIZED>,userCountryCode:"US"}})` selects `brand,supportedCardProductTypes`; failure cleanup uses `RemoveCard({cardId:<PAYPAL_FI_ID>})`. | Merchant eligibility/cleanup, not ordinary registration; helper retry boundary above. |
| **PayPal FraudNet (conditional third party)** | A gateway-configured post-nonce path can inject `https://c.paypal.com/da/r/fb.js`. | Conditional capability only; the artifact does not prove it ran. |

The card selector route only navigates. No active browser request obtains the initial
personal list/detail/status. Do not transplant FI IDs among P2P, add-funds, cash-out,
merchant backup, connected-business, or group contexts.
### 11.4 Identity, security, and privacy
Evidence: `profile-f1def6eb78cc836b.js`,
`privacy-2207fb7e8bf7ee64.js`, `social-0b4f682714179b6a.js`,
`security-271861b156f93155.js`, `change-password-518dcad98df071c7.js`, and
`_app-7c5e4c17c954f843.js`.

| Exact operation | Compact exact input | Key output/state handling |
|---|---|---|
| GraphQL `Identity` layout query | No variables (`ProfileInput` omitted). | `availableIdentities`: ID/type/display/handle/avatar/suspension/balances plus business/charity legal fields; exact `personal,business,charity,teens`. Selects personal/switcher. Client-side only on captured desktop/settings identity-layout pages; SSR-skipped. |
| GraphQL `editPrimaryPhone` | `{input:{phoneNumber:<NORMALIZED_E164_US_PHONE>}}` | Returned phone updates profile; runs first only if changed/unmasked. |
| GraphQL `editIdentity` | `{input:{firstName,lastName,handle,email,id}}` | Store uses submitted values, not selected identity/avatar. Prior phone success can survive failure; detail code `104` exposes invalid-input text. |
| `PUT <NEXT_BASE_URL_V1>/v1/account/photos/profile` | Bearer + configured device; raw blob. | V3; response ignored; `1007` enters policy state; runtime base/header unresolved. |
| GraphQL `getUploadImageUrl`, presigned `PUT`, GraphQL `uploadImage` | Start `{input:{imageType:"profile"}}`; choose URL/meta; finalize `{input:{meta:<RETURNED_META>}}`. | V2; finalize scalar ignored; object storage is separate. |
| GraphQL `updatePrivacyPreferences` | `{input:{audience:<PUBLIC_FRIENDS_OR_PRIVATE>}}` or `{input:{autoFriendContacts:<BOOLEAN>}}`; exact `public,friends,private`. | Optimistic update/rollback. Audience/block shared; contact control personal. |
| GraphQL `changePastTransactionsAudience` | `{input:"friends"}` or `{input:"private"}`. | Scalar ignored after confirmation. |
| GraphQL `blockOrUnblockUser` | `{input:{userId:<USER_ID>,action:<BLOCK_OR_UNBLOCK>}}`; exact `block`/`unblock`. | Local toggle/rollback; reads identity/blocked. |
| `PUT /api/users/discoverability` | `{id:<SSR_CONSENT_ID>,state:<SSR_BOOLEAN>}`, CSRF pair. | Body ignored; personal-gated optimistic rollback; options SSR-only. |
| GraphQL `changePassword` | `{input:{currentPassword:<CURRENT>,newPassword:<NEW>}}`; confirmation local. | Scalar ignored; `212` marks current wrong; success routes profile. |
| GraphQL `unauthorizeDevice` | `{input:{deviceId:<DEVICE_ID>}}` | Removes requested device regardless of returned ID; initial devices SSR-only. |

Initial profile/privacy/blocked/discoverability/contact/device values are SSR-only.
Bundled `GET /api/user/profile` and GraphQL `UserProfile` lack target-page call sites:
**bundled/inactive**, not initial reads or hidden SSR sources.
`/settings/privacy/report` is route-only; no data-request call is emitted.

Client-only new-password checks are 8–20 characters with lowercase, uppercase, digit,
one of `~!@#$%^&*()+=`, and matching confirmation; they are not server-schema proof.
### 11.5 Stories, social graph, search, and notifications
Evidence: `index-53e65d27fdf93ab0.js`,
`[storyId]-1b714a0d199f5eca.js` (exact registered route
`/story/logged-in/[storyId]`), `[username]-09224ee6337cfc47.js`
(`/u/logged-in/[username]`),
`search-fd910549c2221f6d.js`, `notifications-019ffe81cf800202.js`, and
`notifications-b4bd29dc1fce5a5d.js`.
#### Stories and feeds
| Exact REST call | Input | Output/state |
|---|---|---|
| `GET /api/stories?feedType=friend&externalId=<SELF>[&nextId=<CURSOR>]` | Home friends. | Appends `{stories,nextId}`; `undefined` exhausted, `""` unfetched. |
| `GET /api/stories?feedType=me&externalId=<SELF>[&nextId=<CURSOR>]` | Own feed. | Same cursor. |
| `GET /api/stories?feedType=otherPublic&otherUserId=<OTHER>&externalId=<SELF>[&nextId=<CURSOR>]` | Other public. | Same. |
| `GET /api/stories?feedType=betweenYou&otherUserId=<OTHER>&externalId=<SELF>[&nextId=<CURSOR>]` | Between-you. | Same; initially lazy. |
| `POST /api/stories/comments/<STORY_ID>` | `{comment:<TEXT>}` | Appends result; pending teens blocked locally. |
| `DELETE /api/stories/comments/<COMMENT_ID>` | No body. | `.ok`; removes actor-owned row. |
| `POST /api/stories/likes/<STORY_ID>` / `DELETE` same path | No body; explicit CSRF. | Body ignored; local like/count on success. |
| `PUT /api/stories/<STORY_ID>` | `{audience:<PUBLIC_FRIENDS_OR_PRIVATE>}` exact `public,friends,private`; CSRF. | Body ignored; retain/rollback; unavailable for transfers. |

Rendered fields include IDs/payment ID, title/note/attachment/date/audience, avatar/
amount, like/comment counts/flags, subtype, wallet status, processing/type, and crypto.

Story detail and initial comments arrive only as SSR `detailStory.story`, `.comments`,
and `.allowedComment`; no detail GET or comment pagination is emitted. Story/profile
navigation URLs are not APIs.
#### Social graph
| GraphQL operation | Exact input | State handling |
|---|---|---|
| `AddFriend` | `{input:{userId:<PROFILE_ID>}}` | Returns ID; state `request_sent_by_you`. |
| `RemoveFriend` | `{input:{userId:<PROFILE_ID>}}` | Returns ID; state `not_friend`. |
| `UpdateRequest` | `{input:{id:<FRIEND_REQUEST_ID>,action:<ACCEPT_OR_DECLINE>}}`; exact `accept`/`decline`. | Returns transaction balance/ID and request ID; updates friend/notification state. |
| `blockOrUnblockUser` | Profile emits `{input:{userId:<PROFILE_ID>,action:"block"}}`. | May load between-you, then blocks; privacy page unblocks. |

Current/other profile and relationship data are SSR-only. No client personal friend-list
query was found; the group `/settings/people` search is not a substitute.
#### Search
Mutually exclusive **runtime-gated browser calls**: SSR
`isP2pSearchNeptuneEnabled=false` selects legacy `People`; true selects Neptune
`Search`. Captured value is unknown; one execution never issues both.

| Operation | Exact active variables | Selected output/pagination |
|---|---|---|
| GraphQL `People` (legacy) | Initial `{input:{name:<TERM>,type:<PERSONAL_OR_BUSINESS_OR_CHARITY>}}`; threshold 3; load-more selected bucket `{after:<END_CURSOR>}` only. | People/business/charity edges+pageInfo; Apollo merges; no page `queryVariant`. |
| GraphQL `Search` (Neptune) | `{input:{name:<TERM>,type:<TYPE>,pagination:{first:10,after?:<CURSOR>}}}`; threshold 1. | Unified personal/business/charity edges+pageInfo; explicit concatenation. |

SSR supplies suggested results. Bundled `Suggest` and `SearchPersonal` documents have
no active `/search` call site and remain inactive leads.
#### Notifications
| Exact operation | Compact input | Output/state |
|---|---|---|
| `GET /api/notifications?isEcheckEnabled=<BOOLEAN>[&nextId=<CURSOR>]` | No body. | Appends `{notifications,nextId}` while cursor truthy; items include identity/payment display fields. |
| GraphQL `AcknowledgeNotification` | `{input:{id:<NOTIFICATION_ID>}}` | Reads `id,acknowledged`; dismisses backup notices/clamps count. |
| GraphQL `UpdateRequest` | `{input:{id:<FRIEND_REQUEST_ID>,action:<ACCEPT_OR_DECLINE>}}`; exact `accept`/`decline`. | Replaces friend action/note; decrements count. |
| `PUT /api/notifications` | Pre-stringified `{"notificationId":"<ID>"}`. | Body ignored; locally dismisses e-check decline. |
| GraphQL `updateNotificationSettings` | `{input:Object.keys(values).map(id => ({id,settingState}))}`; all keys, including optional `emailDebtCollectionOptOut`. | Returns setting rows; clears dirty state. Initial settings/phone/email SSR-only. |

Notification categories observed are `email`, `sms`, and `push`; displayed groups are
`money`, `social`, and `everythingElse`. Group-role notification settings are
informational and do not mutate.
### 11.6 Addresses, statements, and account closure
#### Address book
Evidence: `/settings/addresses`, `addresses-4e0191c08bdf9145.js`. Initial addresses
are SSR-only; the post-write network read is exact.

| GraphQL operation | Exact input | Selected output/state |
|---|---|---|
| `Identity` address-book query (`network-only`) | `{addressBookInput:{fetchPrimaryAddress:true}}`; profile omitted. | Primary/shipping address and control fields; missing list→`[]`, primary→`null`. |
| `addAddress` | `{input:<PREPARED_ADDRESS>}` | Ignores returned address/control; refreshes. |
| `updateAddress` | `{input:{id:<ADDRESS_ID>,address:<PREPARED_ADDRESS>}}` | Edit/promote, then refresh. |
| `DeleteAddress` | `{input:{id:<ADDRESS_ID>}}` | Truthy scalar, then refresh; primary is promoted, not deleted directly. |

Prepared input has recipient/street/locality/region/postal, `preferred:false`, type or
`shipping`, `country:"US"`, optional `isPrimaryAddress`, and
`options:{skipValidation:false}`; local `useForOtherAddress` is removed. First,
no-primary, and current-primary edits force primary.
#### Statements and history
Evidence: `/statement`, `statement-12dc365087f2d51c.js`; detail bundles
`[id]-94482b0890b2d83f.js` and `details-3da21404474e0fdb.js`.

GraphQL `TransactionsHistory` (`network-only`) takes:

```json
{"input":{"startDate":"<LOCAL_YYYY_MM_FIRST>","endDate":"<LOCAL_YYYY_MM_LAST>","pendingTransactionsPagination":{"first":10,"before":null,"after":null},"completedTransactionsPagination":{"first":10,"before":null,"after":null},"profileId":"<OPTIONAL_PROFILE_ID>","accountType":"<OPTIONAL_ACCOUNT_TYPE>"}}
```

`all-profiles-id` omits profile keys; some charity views add `profiles:[{id,type}]`.
With all profiles plus teens, the main aggregate query is skipped for composed special
sections, so it does not always run on `/statement`. Output covers notice/balances/
fees/taxes/summaries and pending/completed transaction edges. Load-more retains base
filters, sends only relevant `after:<END_CURSOR>`, appends edges, and replaces pageInfo.

Statement CSV/email uses same-origin **GET**:

```text
/api/statement/<download-v2|download|send-email>
  ?referer=<ENCODED_ROUTE>&startDate=<UTC_FIRST>&endDate=<UTC_LAST>
  &<taxCsv=true|csv=true>[&profileId=<ID>&accountType=<TYPE>|&hasTeens=true]
  [&emailStatement=true]
```

`send-email` ignores response. `download-v2` consumes `{jsonData}` and builds a CSV
blob; `download` is a link/new-tab target. Transaction detail itself is SSR-only at
`/statement/detail/[id]`; its rich consumed shape does not reveal a lookup API.

GraphQL `backupWithholdingTransactionDetails` has SSR initial data; active load-more
uses exact lowercase `transactionid`:

`{input:{profileId:<PROFILE_ID>,transactionid:<TRANSACTION_ID>,transactionsPagination:{first:10,before:null,after:<END_CURSOR>}}}`

It returns note plus paginated transaction edges; pages append.
#### Account closure and logout
Evidence: `/settings/profile/cancel/pending-payments`,
`pending-payments-8e549835817cd825.js`; `/account/logout`,
`logout-93043d9bb07f50cb.js`.

GraphQL `updatePendingPaymentRequest` is an exact prerequisite action:

```json
{"input":{"paymentIds":["<EVERY_ID_FROM_SSR_PENDING_PAYMENTS>"],"actionType":"cancel","fundingSourceID":"Web"}}
```

Returned ID is ignored; completion routes onward. No per-item/partial handling;
retry/idempotency unknown. This cancels outgoing requests, **not** the account. Its list,
`/settings/profile/cancel/initiate`, final close mutation, and post-close session are
SSR/route gaps.

`/account/logout` is an SSR-opaque null page. It emits no browser fetch, GraphQL
document, response, or redirect contract. An analytics logout call before navigation
is third-party telemetry, not Venmo session invalidation.

## 12. Personal versus business/group and route-only context
Business, connected-merchant, owner, and manager operations are exclusion/context,
not promoted to the personal contract.
Artifact context includes `profile-f1def6eb78cc836b.js`,
`card-7c17c176ca4b51f3.js`, `import-8dbf05a81f642fd4.js`, and
`people-f65b2832f77821b7.js` (`/settings/people`).
| Context | Exact operation(s) present in captured artifacts | Why excluded from personal contract |
|---|---|---|
| Business profile | GraphQL `updateBusinessProfile` `{id,handle,publicInfo,businessPreferences}`; V2 presigned profile/cover. | Not personal identity/image mutation. |
| Connected-business eligibility | GraphQL `MerchantEligibility({merchantId,braintreeNonce,fundingSourceType:"card"})`, `Merchant({id})`, PayPal `AutodetectCard` under §11.3. | Merchant context only. |
| Connected-business post-add | GraphQL `updateMerchantConnection` with connection and branch-selected primary/backup FI IDs; conditional backup update. | Not ordinary add completion. |
| Business QR | `GET /api/qrcode?output=svg&type=bizprofile&payload=<BUSINESS_ID>&v=2`. | Rendering, not personal action. |
| Group profile | `PUT /api/groups/<GROUP_ID>` `{name,username}`. | Owner identity update. |
| Group manager invitation | `PUT`/`DELETE /api/groups/<GROUP_ID>/manager/requests` `{groupId,requestId}`. | Group transition. |
| Group FI delete | GraphQL `deleteFundingInstrument({input:{id:<FI_ID>}})` in group context. | Owner removal; input has no group ID. |
| Group FI import | `getUserFundingInstruments` (network-only), then `POST /api/groups/<GROUP_ID>/payment-methods` `{paymentMethodID:<SOURCE_FI_ID>}`. | Associates SSR source; adds no personal FI. |
| `/settings/people` search/invite | `GET /api/users?q=<TERM>`; `POST`/`GET`/`DELETE /api/groups/<GROUP_ID>/manager/requests`; `{userId}` or `{userId,requestId}`. | Group management, not friends. |
| `/settings/people` roles | `DELETE /api/groups/<GROUP_ID>/manager` `{userId}`; `GET`/`DELETE /api/groups/<GROUP_ID>/owner/requests`, delete `{requestId}`. | Group administration. |

Canonical route-only and SSR-only boundaries are recorded once in the relevant rows
of §13 rather than duplicated here.

## 13. Final evidence gaps
| Area | What is established | What must not yet be claimed |
|---|---|---|
| Mobile OTP/device trust | A1–A4 synthetic contract + historical lead. | Current OTP/A4 success or fresh-device bootstrap. |
| Token lifecycle | Explicit issuance, local storage, and local-only logout. | Refresh/lifetime or remote revocation by this CLI. |
| Mobile compatibility headers | Current-version iOS-compatible literal, JSON Accept, language, decimal session ID; Android/current-client corroboration and live old-versus-current payment differential. | Which individual header is required or how long this profile remains accepted. |
| F2 SMS step-up | Android 26.13.0 exact F2O/F2V requests, reason mapping, same-UUID verified metadata; exact synthetic orchestration; one reconciled live OTP payment with an explicitly submitted bank source. | Universal challenge frequency, actual debit source, or final fee. |
| F1P/protected F2 | Android 26.13.0 source-bound eligibility, buyer-protection fee selection, protected transaction fields, response type, and OTP preservation; official seller-fee/coverage guidance; exact synthetic tests. | Live authorization/success, actual debit source, final seller fee, tax treatment, item eligibility, or a guarantee of Purchase Protection coverage. |
| Natural pagination endings | First→second; friends exhaustion. | Natural search/activity/request ending. |
| F4S response identity | Current success may omit payment ID and status; output does not invent either. | A resulting payment ID or settlement state when the response omits them. |
| F4S funding and step-up | W2 plus signer-verified F4N/F4E/F4S action ID, selected peer source, token, and server-UUID SMS continuation; exact automatic/explicit balance/bank/card selection and OTP tests; client-1 structure-only F4N read; one owner-run corrected unprotected F4S success; fee records omitted by default and normalized only for explicit `--protect`. | Actual debited source or final fee beyond eligibility evidence; live explicit-source, protected, or SMS-continuation validation. |
| Non-private decline | Audience-generic code, common supported-audience validation, and representative response-preservation tests. | An exact friends/public success fixture, independent current live proof, or every accepted envelope alternative being individually pinned. |
| F6 outgoing request cancellation | Android 26.13.0 form route/action and pending/held native scope; independent public endpoint/action corroboration; exact synthetic preflight/wire/response tests. | Current client-1 live success, confirmed rejection codes, or cancellation of anything other than an outgoing open charge request. |
| General transaction/payment reads | Android ledger list/detail and separate incomplete-request merge are statically established; pending requests and social activity are implemented. | Final command naming/scope, composite ledger continuation policy, merged app-parity view, or ID-namespace substitution. |
| Model limits | Existing leading-zero IDs, username matching, nonblank note, read-only role compatibility, 200-record resolution bound. | Normalization, stricter grammar/note limit, exact read-only role set, or global username uniqueness without evidence. |
| Friendship mutations | Signer-verified current P4/P5 route/body/state semantics, exact synthetic transport/client/orchestration tests, and mandatory P2 reconciliation. | Current client-1 authorization, controlled live success, incoming decline, notification timing, or retry/idempotency. |
| Activity social mutations | Signer-verified Android 26.13.0 classic AC2–AC6 contracts; exact embedded-data, wire, dry-run, one-write, and available reconciliation tests; direct comment removal explicitly delegates authorization and independent verification; one reversible client-1 like/unlike/comment-add/remove canary on a private activity. | Broad authorization/durability across story classes, complete social pagination, arbitrary emoji reactions, comment editing, or non-author moderation. |
| Transfers | Outbound-only T1 options view; enabled fail-closed T2 standard-bank write; exact HTTP-201 pending response; matching-ID activity reconciliation. Transfer-in is intentionally unsupported. | Settlement completion, confirmed rejection codes, instant/debit cash-out, T1 fee units, step-up, cancellation, or expedition. |
| Web BFF/GraphQL | Captured-build emitted calls. | Current success, mobile equivalence, full schema, or retry safety. |
| SSR reads | Consumed props/shapes. | Hidden method/path/document/variables/auth or complete response. |
| Payment methods | Exact add/update/delete/verify browser flows and provider boundaries. | Initial list/detail/status request, Plaid Link-token endpoint, microdeposit-initiation endpoint, or runtime provider branch. |
| Account closure/logout | Pending-request prerequisite + routes. | Final close/session-logout API. |
| Route/card discovery | Captured navigation and negative findings. | APIs for direct deposit, tax documents, connected business, privacy report, payment-link/story/statement detail resolution, initial personal profile/device/privacy/address reads, bank-added/unauthorized screens, or Venmo card lock/unlock, PIN, replacement, and controls. |

## 14. Maintenance rules
1. Record source classification and verification state separately. Never word static
   discovery, SSR shape, historical lead, or owner decision as implemented/current proof.
2. Code and exact service-free tests define the implemented contract at a cited commit.
   If prose conflicts, state code truth and either fix code/tests or retain the
   discrepancy explicitly.
3. For each web capture record date/build, exact bundle/route, call-site status,
   request, selected output handling, and SSR/runtime gaps. Bundled is not active.
4. Keep browser BFF, Venmo orchestration GraphQL, mobile `/v1`, and third-party
   Plaid/Braintree/PayPal contracts in separate trust/authentication domains.
5. Never infer an upstream endpoint from a same-origin BFF path, a request from an SSR
   prop, an API from a route string, or idempotency from a UUID.
6. Preserve exact enum/field casing and per-flow opaque-ID roles. Never cross-use an
   ID because two UIs call it a funding instrument.
7. Keep financial and relationship writes one-shot: never replay after an ambiguous transmission. A
   second pass is allowed only when an exact protocol proves a complete challenge
   response and binds the continuation's method, correlation value, original context,
   and verification metadata. A new financial operation also requires strict response
   proof, confirmed-rejection rules, interruption/output handling, and authoritative
   reconciliation before implementation.
8. Tests and routine verification stay service-free; never add production-probe or
   mutation procedures.
9. Use sanitized placeholders only. Do not retain raw requests/responses, credentials,
   OTP values, client keys, personal IDs/data, or full card/bank values as evidence.
10. When an evidence gate changes, update this inventory, relevant exact tests,
    [PLAN.md](PLAN.md), and
    [`docs/evidence-gated-follow-ups.md`](docs/evidence-gated-follow-ups.md) together;
    do not silently strengthen claims from discovery evidence.

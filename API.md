# Venmo private API contract inventory

> [!CAUTION]
> **Every Venmo endpoint in this document is unsupported and private.** Venmo can
> change, reject, or remove it without notice. This inventory describes evidence;
> it is not public API documentation, a compatibility promise, or a live-probing
> runbook.

> [!WARNING]
> **Contributor safety:** routine development and CI are service-free. Do not use a
> production credential, invoke ignored/manual probes, replay browser traffic, or
> run a real `pay user`, `requests create`, `requests accept`, `requests decline`, or
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
The CLI has 20 enabled leaf actions. Operation labels resolve to the mobile wire catalog in §5.
Counts begin after local credential loading; failed local gates can stop earlier.

| # | CLI leaf | Calls and maximum orchestration | Classification |
|---:|---|---|---|
| 1 | `auth login` | Password: **A1 → A5**. OTP: **A1 → A2 → A3 → A5**, verified local save, then **A4** (maximum 5). | Remote auth plus hidden identifier/password/fresh-device prompts and optional hidden OTP. |
| 2 | `auth logout` | No API call; delete only the local keyring entry. | Entirely local; does not revoke the remote token. |
| 3 | `auth status` | **A5** once. | Live identity validation. |
| 4 | `pay user <USERNAME> <AMOUNT> --note <NOTE> [--visibility ...] [--yes]` | **A5**; 1–4 × **P1** then **P2**; **W2 → W1 → F1 → F2** (maximum 10). | Financial; exactly one **F2**. |
| 5 | `requests create <USERNAME> <AMOUNT> --note <NOTE> [--visibility ...]` | **A5**; 1–4 × **P1** then **P2**; **F3** (maximum 7). | Financial; exactly one **F3**; no prompt. |
| 6 | `requests accept <REQUEST_ID> [--yes]` | **A5 → R2 → P2 → W2 → F4** (5). | Financial; exactly one **F4** after default-No confirmation. |
| 7 | `requests decline <REQUEST_ID> [--yes]` | **A5 → R2 → F5** (3). | State-changing; exactly one **F5** after default-No confirmation. |
| 8 | `friends list [--limit N] [--offset N]` | **P3** once. | One page; stored self ID. |
| 9 | `friends add <USERNAME> [--yes]` | **A5**; 1–4 × **P1** then **P2**; exactly one **P4**, then reconciling **P2** (maximum 8). | Relationship write; sends or accepts according to authoritative state. |
| 10 | `friends remove <USERNAME> [--yes]` | **A5**; 1–4 × **P1** then **P2**; exactly one **P5**, then reconciling **P2** (maximum 8). | Relationship write; unfriends or cancels an outgoing request. |
| 11 | `users search <QUERY> [--limit N] [--offset N]` | **P1** once. | One page. |
| 12 | `users info <USERNAME>` | 1–4 × **P1** then **P2** (maximum 5). | Shared exact-username resolution followed by detail read. |
| 13 | `pay methods` | **W1** once. | One read. |
| 14 | `balance` | **W2** once. | One read. |
| 15 | `activity list [--limit N] [--before-id TOKEN]` | **AC1** once. | One page; stored self ID. |
| 16 | `activity info <ACTIVITY_ID>` | **AC2** once. | One detail read. |
| 17 | `requests list [--direction all\|incoming\|outgoing] [--limit N] [--before TOKEN]` | **R1** once; direction is filtered locally. | One unfiltered source page. |
| 18 | `requests info <REQUEST_ID>` | **R2** once; narrowed locally to open requests. | One detail read. |
| 19 | `transfer options` | **T1** once. | Enabled read-only current eligibility view. |
| 20 | `transfer out <AMOUNT_OR_ALL> [--speed standard] [--yes]` | **A5 → W2 → T1 → T2** (4). | Financial; exact lowercase `all` resolves from W2 available cents; exactly one **T2** after default-No confirmation. |

Help and version output are also service-free but are not leaf actions. There is no
implemented generic payment list/detail, outgoing-request cancel/remind, explicit
funding-source selection, token refresh, incoming-friend-request decline, payment-method mutation,
remote token revocation, transfer-in command, instant transfer write, or manual
transfer-destination selection.

## 4. Shared implemented mobile boundary
### 4.1 Transport and headers
- Fixed origin/base: `https://api.venmo.com/v1/`; ordinary production composition
  has no alternate base URL. TLS uses Rustls.
- Timeouts: connect 10 seconds, response read 15 seconds, whole request 30 seconds.
- Responses are captured completely with a 2 MiB maximum.
- Redirects, proxies, and automatic gzip/Brotli/Zstandard/deflate decoding are
  disabled. No `Referer` is sent.
- There are **zero automatic retries**, including reads, login, and writes.
- Every request sends:

  ```http
  Accept: application/json
  User-Agent: Venmo/7.44.0 (iPhone; iOS 13.0; Scale/2.0)
  ```

- A JSON body additionally sends `Content-Type: application/json`. P4 instead sends exact
  `application/x-www-form-urlencoded`; P5 sends no body.
- Query values are URL-encoded. Dynamic path segments must be nonempty, not `.` or
  `..`, contain neither slash nor backslash, and contain no control character.
- The compatibility User-Agent is code truth; its present server necessity remains
  a residual evidence gap.

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
IDs and amount may be JSON integer/number alternatives. A payment activity requires
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
| **P3** | `GET /v1/users/<SELF_USER_ID>/friends?limit=<LIMIT>&offset=<OFFSET>` | `data` user array and optional `pagination.next`; count ≤ limit and any next URL passes §7. |
| **P4** | `POST /v1/friend-requests`, form body `user_id=<TARGET_USER_ID>` | 2xx JSON must contain exact target `data.user`; then P2 must prove `request_sent_by_you` for send or `friend` for accept. |
| **P5** | `DELETE /v1/users/<SELF_USER_ID>/friends/<TARGET_USER_ID>`, no body | Any complete 2xx status is provisionally accepted because signer-verified current implementations disagree on response body; then P2 must prove `not_friend`. |
| **W2** | `GET /v1/account` | Requires direct string fields `data.balance` and `data.balance_on_hold`, each an exact signed USD decimal with ≤2 fractional digits. It never infers external balances. |
| **AC1** | `GET /v1/stories/target-or-actor/<SELF_USER_ID>?limit=<LIMIT>&social_only=false[&before_id=<TOKEN>]` | `data` story array and optional `pagination.next`; count ≤ limit; every story is one supported class below. |
| **AC2** | `GET /v1/stories/<ACTIVITY_ID>` | `data` story or `data.story`; top-level story ID must exactly equal the path ID. |
| **R1** | `GET /v1/payments?action=charge&status=pending,held&limit=<LIMIT>[&before=<TOKEN>]` | `data` records and optional `pagination.next`; every record is exact `charge`, status `pending` or `held`, positive, involves self exactly once, and count ≤ limit. Direction is derived and filtered locally. |
| **R2** | `GET /v1/payments/<REQUEST_ID>` | `data` record or `data.payment`; exact path ID. Mapper permits `charge`/`pay` and bounded status, but public `requests info` narrows to open `charge`; mutation preflight is stricter below. |

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

The `pay user` consumer is stricter:

1. every method has exact case-insensitive `peer_payment_role` `default`, `backup`,
   or `none`; `none` is skipped and any unknown/omitted role fails the response;
2. participating type is `balance`, `bank`, or `card`; balance is skipped and only
   external bank/card is eligible; unknown participating types fail closed;
3. IDs are valid and unique across the full response;
4. method fee is proven only by `fee.calculated_fee_amount_in_cents` as JSON `u64`;
   otherwise it is unknown; and
5. choose the unique eligible external default even with backup methods present. If
   none is default, only a sole eligible external method is selected; multiple
   no-default candidates are ambiguous. Multiple eligible external defaults fail.
   Response order and fee never choose a source, and no CLI override exists.
#### Activity story classes
An activity has a valid top-level ID and exactly one non-null class:

- **payment:** nested status/action/positive amount/actor/target, exactly one self
  party; nested creation time, note, and audience with documented top-level
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

Pending, failed, or another bounded remote status remains readable data rather than
being converted to command failure.
#### R2 preflight narrowing
`requests info` allows only `action: charge` with status `pending` or `held`, in
either direction. Accept/decline additionally require an incoming `charge`, status
exactly `pending`, complete note, supported audience, requester username, and valid
creation time. `held` is not mutable. Accept then uses P2 and proves the requester
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
selected W1 method's fee nor the final payment fee.
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
  "funding_source_id": "<SELECTED_EXTERNAL_METHOD_ID>"
}
```

One UUID is generated before the single attempt; it is not a retry license. A 2xx
response must use exactly `{"data":{"payment":<RECORD>}}`, with valid creation
time, valid payment ID, `action: pay`, status `settled`, `pending`, or `held`, exact
amount, self as actor, intended recipient as target, exact note, and a supported
audience equal to or more restrictive than requested. Every post-send proof failure
is outcome-unknown.
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
#### F4 — accept an incoming request
```http
PUT /v1/payments/<REQUEST_ID>
```

Exact body: `{"action":"approve"}`. A 2xx response may be `data` record or
`data.payment`. It must prove a valid payment ID, status `settled`, `pending`, or
`held`, exact original amount/note/audience, and creation time no earlier than the
request. Two representations are supported:

- `action: charge`, original requester as actor and self as target; or
- `action: pay`, self as actor and original requester as target.

**Code truth:** the returned ID is validated as a payment ID, but the implementation
does **not** enforce that it differs from `<REQUEST_ID>`. PLAN/README prose at this
snapshot describes a distinct ID based on observed behavior; that stronger claim is
not the implemented acceptance contract.
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
**Pay:** A5 validates self; recipient search/detail proves a nonself personal/payable
user; W2 captures balance; strict W1 selects an external peer method; F1 either
confirms eligibility or denies it without F2. The CLI generates a UUID, renders and
flushes validated payment details, then requires default-No confirmation unless `--yes`
(noninteractive always needs `--yes`). It installs interruption protection before
exactly one F2. W2 may coexist with an external backup; neither W1 nor F1 proves the
final source/fee.

**Request:** A5 and recipient resolution run, then one F3. There is no W1, W2, F1,
funding field, balance gate, prompt, or `--yes`.

**Accept:** A5 and R2 prove an incoming exact-`pending`, exact-`private` request; P2
proves the requester personal/payable; W2 available balance must cover the amount.
Validated request-acceptance details are always flushed; unless `--yes` is supplied,
default-No confirmation follows. Interruption protection is then installed and exactly one F4 is
attempted. F4 carries no funding source. Balance
coverage is only a safety snapshot and proves neither actual source nor fee.

**Decline:** A5 and R2 prove an incoming exact-`pending` request with any supported
audience. The CLI renders and flushes authoritative request-decline details, requires default-No
confirmation unless `--yes`, installs interruption protection, then attempts exactly
one F5. No P2/W1/W2/F1 runs, profile type/payability is unproved, and no money/funding
field is sent.
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
tokens to stderr; `friends list`, `activity list`, and `activity info` use stored self
ID without A5. Request direction is local, so a displayed page can be empty and still
have a continuation.

## 7. Implemented pagination and recipient resolution
A server `pagination.next` is parsed only to reconstruct a local continuation. It must
resolve to the fixed HTTPS origin, port, and exact expected path; userinfo, password,
fragment, duplicate/disallowed query keys, and invalid values are rejected.

| Family | Allowed next query and progress rules |
|---|---|
| Friends | Only `limit`, `offset`; same limit; offset valid `u32` and strictly increases. Empty page plus next is rejected. |
| Activity | Only `before_id`, `limit`, `only_public_stories`, `social_only`; same limit; `social_only=false`; optional `only_public_stories=false`; required new valid token. Empty page plus next is rejected. |
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
F2–F5 and T2 are enabled financial-policy writes; P4/P5 are externally visible relationship
writes. A1/A3 use a parallel but distinct authentication issuance ambiguity.

- Proven pre-send construction/encoding failure or connect-stage DNS/TCP/TLS failure
  is ordinary and nonambiguous.
- A non-connect send timeout/disconnect is unknown.
- Once a response begins, redirect rejection, read/capture failure, truncation, 2 MiB
  overflow, and resource exhaustion are unknown.
- A complete non-2xx financial response is still unknown, except only F2 HTTP 400 with
  root `error.code` exactly `1396` or `13006`, which is a confirmed rejection.
- Every non-2xx F3/F4/F5 or transfer response remains unknown, even if it resembles
  stale state or a challenge.
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
successful result to the operator; relationship exit 3 has the same result-output boundary. Usage exits 2, ordinary failure/cancellation 1,
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
| 2026-07-14 | Visibility | Direct observation / reconciled live; owner-approved decision | Friends/public pay and more-private response behavior reconciled; non-private F3 accepted without another mutation. |
| 2026-07-14 | F4 | Direct observation / reconciled live validation plus historical lead / representative synthetic coverage | Live responses used the charge-oriented representation while activity exposed the resulting outgoing `pay`; the pay-oriented response representation remains historical/synthetic. Source and fee remain unproved. |
| 2026-07-14 | F5 | Direct observation / reconciled live validation | Private decline became exact `cancelled`, with no balance/activity movement. Non-private success is accepted by code but is not independently pinned by an exact success fixture or live validation. |
| 2026-07-16 | T1 | Direct observation / controlled live validation | One bounded read proved current bearer/device auth, direction/speed branches, standard bank candidates in both directions on the observed account, and fee/estimate structure; values/counts were not retained. |
| 2026-07-17 | T2 standard out | Direct observation / controlled and reconciled live validation + exact synthetic | One approved $0.01 HTTP-201 write returned direct pending standard transfer data; exactly one matching outgoing activity record had the same transfer ID. Exact body, fail-closed selection, strict response proof, and one-write ambiguity are implemented. |
| 2026-07-19 | P4/P5 friendship mutations | Direct observation of signer-verified Android 9.26.0, 10.31.1, and 26.13.0 + exact synthetic; residual auth gap | Current native route/body/state semantics are consistent across artifacts. No public mutation implementation was found. The CLI's client-1 session has not received a controlled live mutation validation; no canary has run. |
| Current gap | User-Agent | Existing behavior / residual gap | Literal is known; current necessity/currency is not. |

## 10. Transfers: enabled options and standard out
### 10.1 Current command state
```text
venmo transfer options
venmo transfer out <AMOUNT_OR_ALL> [--speed standard] [--yes]
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
accepted, not settled. Empty or mismatched JSON, status-only 2xx, non-201, error code,
challenge, interruption, and output uncertainty remain ambiguous. The historical
client's status-only 200–204 rule is rejected.

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
| Compatibility User-Agent | Exact hard-coded value. | That server still requires it or that it is current. |
| Natural pagination endings | First→second; friends exhaustion. | Natural search/activity/request ending. |
| F4 response ID | Valid returned payment ID is enforced. | That response ID is necessarily distinct from request ID. |
| F4 funding/fee | Balance coverage gate and no submitted external FI. | Actual wallet funding, final source, or zero/final fee. |
| Non-private decline | Audience-generic code, common supported-audience validation, and representative response-preservation tests. | An exact friends/public success fixture, independent current live proof, or every accepted envelope alternative being individually pinned. |
| General mobile payment reads | Pending requests and activity. | `payments list`, settled PaymentId detail, or ActivityId substitution. |
| Model limits | Existing leading-zero IDs, username matching, nonblank note, read-only role compatibility, 200-record resolution bound. | Normalization, stricter grammar/note limit, exact read-only role set, or global username uniqueness without evidence. |
| Friendship mutations | Signer-verified current P4/P5 route/body/state semantics, exact synthetic transport/client/orchestration tests, and mandatory P2 reconciliation. | Current client-1 authorization, controlled live success, incoming decline, notification timing, or retry/idempotency. |
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

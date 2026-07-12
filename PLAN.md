# venmo-cli Rust Product Plan

**Status:** Active; root Rust-only repository cutover completed
**Last updated:** 2026-07-12
**Package:** `venmo-cli`
**Binary:** `venmo`

## 1. Product direction

The CLI is one native Rust binary built from the repository root. The completed language and repository cutover was also an intentional redesign of the command interface; the former TypeScript command surface remains only historical behavioral evidence, not a compatibility requirement or shipped implementation.

The first release should make the common workflow obvious:

1. Save a bearer token securely.
2. Verify which Venmo account is active.
3. Find a friend or another user.
4. Inspect the Venmo balance and available payment methods.
5. Pay one person, request money from one person, or accept one incoming request.
6. Inspect activity and pending requests.
7. Diagnose authentication or private-API failures.

This remains an unofficial client for unsupported/private Venmo endpoints. Endpoint discovery and contract testing are release gates, not assumptions.

## 2. Confirmed product decisions

- Use Rust with a pinned stable toolchain and a committed `Cargo.lock`.
- Use Rust 1.95.0 for the pinned toolchain and MSRV, package version `0.1.0`, and the MIT license.
- Use [`clap`](https://docs.rs/clap/) with its derive API.
- Use [`dialoguer`](https://docs.rs/dialoguer/) behind a narrow prompt adapter for hidden token input, default-No confirmation, and funding-method selection.
- Prefer mature, popular, actively community-maintained off-the-shelf crates for common functionality instead of rolling project-owned replacements; keep custom code focused on Venmo-specific domain behavior, application orchestration, and safety policy.
- Use a hybrid command hierarchy: frequent actions are top-level; related management operations are grouped.
- Use [`keyring`](https://docs.rs/keyring/) as the only persistent credential backend.
- `venmo auth login` uses the legacy mobile-private trusted-device-ID plus username/password and SMS-OTP flow by default; `venmo auth login --token` preserves hidden import of an existing bearer token. `venmo auth reauthenticate` explicitly repeats full password/optional-OTP issuance for the stored account while reusing its stored trusted device ID; it is not a refresh operation.
- Login credentials and OTP material are zeroized after use and never stored. Only a validated bearer token, persistent device ID, account identity, and local saved-at time enter the OS credential store.
- Target Venmo's bearer-token/device-ID mobile private `/v1` API rather than emulating the browser session API. Web-app traffic is discovery evidence only; do not adopt cookie/CSRF authentication, browser backend routes, or a browser User-Agent merely because the web client exposes an operation.
- A validated token for a different account never replaces a valid stored credential; `auth login` fails without mutation and requires an explicit `auth logout` first.
- Do not ship browser automation, browser-cookie import, or web-session authentication as CLI features. Development-time browser observation under Section 7.4 is permitted solely to establish sanitized API contracts.
- Document the unsupported password/SMS-OTP login risk and the optional existing-token import path in the README.
- Support one recipient per new `pay` or `request` invocation and one request ID per acceptance invocation.
- Keep direct request creation as `venmo request ...`; reserve the exact `accept` token for `venmo request accept ...`.
- Call transaction history `activity`.
- Keep payment audience private in the first release.
- Require default-No confirmation only when the user sends money: `pay` and `request accept`.
- Expose `--yes` only on `pay` and `request accept`, and require it for those commands when stdin is non-interactive.
- Do not expose any `--dry-run` flags.
- Never automatically retry payment creation, request creation, or request acceptance.
- Write no first-party `unsafe` Rust and prohibit it mechanically across every first-party target.
- Use no `unwrap`, `unwrap_err`, `expect`, `expect_err`, or equivalent panic-based value extraction in first-party Rust, including tests and build tooling.
- If the correct treatment of any failure is unclear, stop the affected implementation and ask the prompter/project owner rather than guessing, swallowing it, panicking, or choosing an arbitrary fallback.
- Include shell completion generation in the first release.

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

## 3. First-release command surface

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
- Machine-readable output is deferred, but command handlers must return typed results so JSON can be added without rewriting application logic.

Proposed stable exit codes:

| Code | Meaning |
| ---: | --- |
| `0` | Success |
| `1` | Operational, credential, validation, API, or user-cancellation failure |
| `2` | `clap` usage or argument-parsing error |
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
- Performs full password/optional-OTP token issuance with zero automatic retries and the same unknown-issuance handling as initial password login. Password, OTP, and OTP-secret material are zeroized after use and never stored.
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
- `--from` accepts a payment-method ID shown by `payment-methods list`.
- `--yes` skips only confirmation; it does not skip validation or preflight.

Funding-source selection order:

1. Exact `--from <METHOD_ID>` match.
2. The one source marked as default.
3. The only available source.
4. An interactive selection when a terminal is available.
5. A clear failure in non-interactive use.

Selection operates only on methods that the verified mobile contract identifies as eligible for the specific peer-payment operation. The current payment-method list/default-role mapping does not yet prove transaction eligibility, fees, fallback behavior, or actual debit source, so no financial command may consume it until Phase 0 establishes those semantics. Within an eligible set, duplicate IDs or multiple defaults are contract failures; an explicit unavailable ID fails; and multiple non-default methods require either interactive selection or `--from`.

Before confirmation, show:

- Resolved recipient username, display name, and user ID.
- Exact amount.
- Note.
- Private audience.
- Selected payment method, including a safe label and ID.

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
- `--from` and automatic funding-source selection follow the same rules as `pay`.
- `--yes` skips only confirmation; it does not skip lookup, state validation, or funding-source resolution.

Before confirmation, show:

- Request ID and current pending status.
- Requester username, display name, and user ID.
- Exact requested amount and sanitized note.
- Request creation time when supplied by the verified contract.
- Selected payment method, including a safe label and ID.
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

Development-time recipient resolution is deliberately fail-closed while the mobile contract remains unverified:

- Numeric recipients use the inherited `GET /users/{user-id}` hypothesis and require the response's immutable user ID to equal the requested ID.
- Username recipients use the provisional `type=username` search, compare the returned username case-insensitively, and require exactly one exact match from a search that reaches verified exhaustion within the 200-record/four-page safety bounds.
- Search ordering and partial results never authorize a recipient. A missing exact match after exhaustion is not found; multiple exact IDs are ambiguous; any non-exhaustive search fails and tells the user to use a numeric Venmo user ID.
- Username input is locally bounded to 1,023 bytes so the leading `@` plus username fits the 1,024-byte search-query bound. Final username syntax, case handling, direct-lookup availability, and eligibility rules remain Phase 0 release gates.

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
- JSON output in the first release.

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

Follow the same high-level model as `pmail`: use the native OS credential store behind one narrow `CredentialStore` trait.

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

The approved read-only experiment imported the current web session's short-lived `api_access_token` together with its matching `v_id`, both through hidden local prompts, then performed only current-account validation before storage. The user retrieved both values manually; they never entered source, command arguments, chat, logs, screenshots, or agent output. Validation failure would have stored nothing. Even the successful result does not establish a durable authentication contract: the token's expiry must be observed, and no financial command may rely on it until lifetime/refresh and `/v1` compatibility are proven. Browser-cookie storage and automatic browser extraction remain out of scope.

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

### 7.4 Establishing verified HTTP request shapes

For this plan, an HTTP request shape means the complete wire contract: HTTPS origin, method, route template, query fields, required header names and authentication model, content type, request-body field names and types, amount units, identifiers, state/version or idempotency fields, expected response statuses and body schema, and the authoritative state transition caused by the operation.

Phase 0 must produce a dated, sanitized contract dossier for every API operation. Recorded behavior from the removed TypeScript implementation and historical documentation are hypotheses, not sufficient evidence. For an operation exposed by the current Venmo web application, the preferred discovery workflow is:

1. Launch the official Venmo web application in a dedicated visible/headed browser session using `agent-browser --headed`; live Venmo discovery must not run headlessly.
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

### 7.6 Target API trait

The application-facing interface should express product operations, not HTTP paths:

```rust
pub trait VenmoApi {
    async fn revoke_access_token(&self) -> Result<(), ApiError>;
    async fn current_account(&self) -> Result<Account, ApiError>;
    async fn friends(&self, page: PageRequest) -> Result<Page<User>, ApiError>;
    async fn search_users(&self, query: &str, page: PageRequest)
        -> Result<Page<User>, ApiError>;
    async fn user_by_id(&self, id: &UserId) -> Result<User, ApiError>;
    async fn payment_methods(&self) -> Result<Vec<PaymentMethod>, ApiError>;
    async fn balance(&self) -> Result<Balance, ApiError>;
    async fn activity(&self, page: PageRequest) -> Result<Page<Activity>, ApiError>;
    async fn activity_by_id(&self, id: &ActivityId) -> Result<Activity, ApiError>;
    async fn pending_requests(
        &self,
        direction: RequestDirection,
        page: PageRequest,
    ) -> Result<Page<PendingRequest>, ApiError>;
    async fn pending_request_by_id(
        &self,
        id: &RequestId,
    ) -> Result<PendingRequest, ApiError>;
    async fn create_payment(&self, payment: CreatePayment)
        -> Result<CreatedActivity, FinancialWriteError>;
    async fn create_request(&self, request: CreateRequest)
        -> Result<CreatedActivity, FinancialWriteError>;
    async fn accept_request(&self, request: AcceptRequest)
        -> Result<CreatedActivity, FinancialWriteError>;
}
```

The exact async-trait mechanism is an implementation decision. Keep the interface mockable and do not force pure domain or keychain work to be async.

## 8. Architecture

Use one Cargo package with a library target and thin binary target:

```text
Cargo.toml
Cargo.lock
rust-toolchain.toml
src/
  main.rs
  lib.rs
  cli/
    args.rs
    completions.rs
    dispatch.rs
    output.rs
    prompt.rs
  application/
    auth.rs
    pay.rs
    request.rs
    friends.rs
    users.rs
    payment_methods.rs
    balance.rs
    activity.rs
    requests.rs
    doctor.rs
  domain/
    account.rs
    activity.rs
    credential.rs
    money.rs
    payment.rs
    payment_method.rs
    recipient.rs
    user.rs
  infrastructure/
    credentials.rs
    logging.rs
    venmo_api/
      client.rs
      dto.rs
      error.rs
tests/
  cli.rs
  http_contract.rs
  migration.rs
  release_smoke.rs
  support/
```

### 8.1 Layer boundaries

#### CLI

- Defines the `clap` command schema.
- Converts raw values into typed application input.
- Handles `std::io::IsTerminal` checks, the narrowly allowed prompts, result rendering, and exit codes.
- Does not construct HTTP payloads or contain financial policy.

#### Application

- Orchestrates each command.
- Loads credentials only when required.
- Resolves recipients and funding sources before any applicable money-sending confirmation.
- Depends on narrow API, credential, prompt, clock, and ID interfaces.
- Returns typed results rather than printing directly.

#### Domain

- Contains validated value types and financial-operation policy.
- Has no dependency on `clap`, `reqwest`, `keyring`, Tokio, or terminal rendering.
- Represents money as checked integer cents, never binary floating point.

#### Infrastructure

- Implements private HTTP endpoints and DTO mapping.
- Implements the sole native keyring adapter.
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
| Runtime | `tokio` 1.52 | Current-thread runtime initially; enable only macros, runtime, and time features |
| HTTP/TLS | `reqwest` 0.13.4 with `rustls` | Confirmed; defaults disabled and only JSON plus rustls enabled |
| Serialization | `serde`, `serde_json` | Endpoint DTOs separate from domain |
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
- Only verified exhaustion can authorize an exact username match. Reaching any record/page bound with another offset remains incomplete and fails closed with guidance to use a numeric user ID.
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

If the client cannot prove that a timed-out, disconnected, or cancelled write was never transmitted, report **outcome unknown**, return exit code `3`, and instruct the user to inspect `activity list`, `requests list`, or the official Venmo application before retrying. Do not assume accepting by request ID is safe to repeat merely because it appears state-bound.

## 11. Financial-write safety model

### 11.1 New payment or request preflight

Before writing, and before the confirmation prompt for `pay`:

1. Parse one recipient.
2. Resolve that recipient to exactly one Venmo user.
3. Parse the amount into checked integer cents.
4. Validate the non-empty note.
5. Resolve the funding method for `pay`.
6. Build an immutable plan.
7. For `pay`, render a sanitized complete summary before confirmation.

If any preflight step fails, send no write request.

Request creation has no interactive confirmation. Once its preflight succeeds, execute its one request-creation write immediately and render the confirmed result afterward. Its explicit recipient, amount, and required note are the complete user authorization for that non-money-sending operation.

### 11.2 Request-acceptance preflight

Before prompting or writing:

1. Parse one canonical request ID.
2. Load the authenticated account identity.
3. Fetch the authoritative request record by ID.
4. Verify that the record is incoming, pending, payable, and addressed to the authenticated account.
5. Require complete, understood requester, amount, note, status, and ownership fields; never guess missing safety-critical values.
6. Resolve one funding method using the same precedence as `pay`.
7. Build an immutable acceptance plan tied to the fetched request ID and observed state.
8. Render a sanitized complete summary that explicitly says this operation pays the requester.

An outgoing, already-settled, cancelled, declined, inaccessible, wrong-account, or malformed request sends no write. The client must use the verified acceptance operation and must not translate the record into an ordinary recipient payment.

The verified server mutation must atomically enforce the request ID's payable state, or expose a conditional token/version that the client sends. A client-side preflight followed by an unconditional unrelated write is not sufficient protection against a concurrent state change.

### 11.3 Execution rules

Execution rules:

- One invocation creates or accepts at most one financial operation.
- For `pay` and `request accept`, prompt only on a terminal and default to No.
- For `pay` and `request accept`, require `--yes` for non-interactive execution; on a terminal it skips only the final confirmation.
- Request creation never prompts, never requires `--yes`, and rejects that flag.
- Send exactly one verified financial mutation for payment creation, request creation, or request acceptance.
- Preserve the original API or transport cause on failure.
- Never claim rollback or retry a write.
- On ambiguous outcome, do not print a false failure or success.
- On confirmed request-state conflict, report the current understood state and direct the user to refresh `requests list`.

## 12. Output, errors, and terminal safety

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
- The separate internal exact-recipient traversal remains limited to four 50-record pages/200 unique users, requires exhaustion, and accepts no public continuation.
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
- No live financial writes in automated CI.

### 13.6 Controlled live-mutation protocol

Automated tests use fakes and mock HTTP servers; their synthetic amounts are not constrained by this section. Any test against the live Venmo service remains read-only by default. A live financial mutation is a manual, explicitly gated exception and must follow all of these rules:

- Open the Venmo web application with `agent-browser --headed`. Prompt the prompter/project owner to log in manually in the visible browser and wait for their confirmation before testing. Never ask them to send login credentials through chat, automate credential/MFA entry, inspect login traffic, or persist the authenticated browser state in the repository.
- After login, read-only tests may inspect the active account and exercise user/friend search, payment-method, balance, activity, and request list/detail workflows. Clear any network capture only after login is complete and keep all observed data under the redaction rules in Section 7.4.
- Use exactly **$0.01 USD** per live payment or request—never a larger amount. If Venmo rejects one cent or requires a higher minimum, stop and report the capability as blocked; never increase the amount to make the test pass.
- Use only `@georgecma` as the counterparty for sending or receiving test money, and only after the owner of that account has explicitly authorized the current test session.
- Resolve `@georgecma` to one exact account before each session and verify the displayed username, display identity, and immutable user ID against the official Venmo application. Keep the expected ID only in ignored local test configuration; never commit it or any returned user record.
- Test sending with one $0.01 payment to `@georgecma`.
- Test receiving by creating one $0.01 request to `@georgecma`; any fulfillment by the counterparty must also be exactly $0.01.
- Test request acceptance only from a fresh $0.01 incoming request created by `@georgecma` for the authenticated test account.
- Use a unique, non-sensitive test note so identical one-cent operations can be reconciled, but never commit the note or raw response.
- Run one mutation at a time. Require an explicit human go-ahead immediately before invocation, never use `--yes` for a live test, and never fuzz, loop, schedule, or batch-probe a financial route.
- After each mutation, verify the counterparty, amount, request/payment ID, status, audience, and funding result through the CLI reads and official Venmo application before continuing.
- If any result is ambiguous, stop immediately. Do not retry or begin another mutation until the prior outcome is reconciled independently.
- Never upload or commit tokens, device IDs, user IDs, notes, payment/request IDs, raw responses, screenshots containing account data, or live-test logs.
- Close the dedicated browser session when testing is complete unless the prompter/project owner explicitly asks to keep that local session open; never save or commit its cookies, local storage, or authentication state.

The literal test username in this protocol is the only approved personal identifier in repository documentation. It does not authorize committing any other account data or including live data in fixtures.

## 14. Quality and security gates

Required in CI:

```text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings -D clippy::unwrap_used -D clippy::expect_used
cargo test --all-targets --all-features
cargo doc --no-deps
cargo deny check
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

## 15. Documentation requirements

The README must let a new user complete setup without source-code knowledge.

Required sections:

1. Unofficial/private API warning.
2. Installation and supported platforms.
3. OS credential-store behavior and Linux Secret Service requirements.
4. The unsupported legacy password/SMS-OTP login flow, including exactly what is sent to Venmo, what is never stored, how `--token` imports an existing bearer token without exposing it as an argument, and how explicit stored-device `auth reauthenticate` performs full issuance rather than a nonexistent refresh.
5. Explicit warnings that account credentials and the resulting token are equivalent to powerful secrets and must never be shared, logged, committed, or passed as shell arguments.
6. `venmo auth login`, stored-device `venmo auth reauthenticate`, trust-device warnings, status, logout, and revocation behavior.
8. Friends and user-search examples, including server-page `--limit`, typed `--offset`, copyable next offsets, one-page behavior, and changing-dataset/no-snapshot semantics.
9. Payment-method and balance examples.
10. Pay, request-creation, and request-acceptance examples; `--from`; confirmation and `--yes` rules for money-sending commands; and request creation's immediate no-prompt behavior.
11. Activity and pending-request examples, including server-page bounds, native before-token notices, sparse locally filtered pages, and how to copy a canonical incoming request ID into `request accept`.
12. Ambiguous financial-write recovery steps, including checking both activity and request state before retrying acceptance.
13. Doctor and troubleshooting guidance.
14. Completion installation.
15. Upgrade, uninstall, and credential removal.

Documentation must never suggest placing the password, OTP, OTP secret, device ID, or bearer token in a command argument, environment variable, file, browser automation, chat, screenshot, or log. Password/OTP entry occurs only in the CLI's interactive prompts and only after an explicit `auth login` or `auth reauthenticate` invocation.

## 16. Distribution

At minimum, preserve the currently intended targets:

- macOS arm64.
- macOS x86_64.
- Linux x86_64 GNU with documented Secret Service requirements.

Explicitly decide before release whether to add Linux arm64, Linux musl, and Windows x86_64. A target is supported only when the binary and native credential backend are tested there.

Release artifacts should contain:

- `venmo` executable.
- README.
- LICENSE.
- Manpage.
- Shell completions.
- SHA-256 checksums.
- Provenance/SBOM when supported by the release pipeline.

Evaluate `cargo-dist` after initial target builds are stable. Recommended channel order:

1. GitHub release archives.
2. Homebrew tap.
3. `cargo install --locked venmo-cli`, if crates.io publication is desired.

## 17. Implementation phases

### Phase 0: API and UX validation

Deliverables:

- Freeze the command surface and help text in Section 3.
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

- Library and thin binary.
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
- `reqwest` client, deadlines, response limits, and safe-read retries.
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
- Release archives, checksums, and native smoke tests.

Exit criteria:

- A user can install, retrieve and store a token, find a recipient, inspect funding, safely perform a payment, create a request, and safely accept an incoming request using only shipped documentation.
- Every declared release target passes keyring and binary smoke tests.

### Phase 8: Root Rust-only cutover (completed 2026-07-12)

- Promoted the Rust package, lockfile, toolchain, policy, source, and tests to the repository root.
- Removed the Node/pnpm/TypeScript implementation, SEA packaging, generated `dist`, dependency/build output, and obsolete product caches from the working tree.
- Updated CI, ignore rules, package metadata, and development documentation to operate from the root Cargo package only.
- Kept legacy credential-envelope decoding solely for one-time native-keychain compatibility; no legacy runtime or source tree ships.
- Added explicit TypeScript-to-Rust credential and command migration notes to the root README, including removed financial interfaces and the absence of a legacy fallback.

## 18. Definition of done

- All commands in Section 3 are implemented and documented.
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

## 19. Principal risks

| Risk | Mitigation |
| --- | --- |
| Private endpoints changed or disappear | Phase 0 validation, isolated DTOs, sanitized fixtures, doctor diagnostics |
| Pending request records or acceptance are unavailable | Treat as explicit release blocker; never substitute counts, incomplete guesses, or an ordinary payment |
| Live discovery moves money | Use only the Section 13.6 protocol: exactly $0.01 with authorized `@georgecma`, one manually confirmed mutation at a time, never `--yes`, and immediate reconciliation |
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
| Imported web token expires or is bound to the wrong device | Import the token and matching `v_id` only through hidden prompts, validate before storage, perform read-only lifetime observation, and keep all financial commands blocked until durability is proven |

## 20. Remaining decisions

The command set and primary grammar are settled. Remaining implementation/release decisions are:

1. Natural final-page release observation for user search, activity, and pending requests, plus best-effort native continuation behavior as each private endpoint's dataset changes. First-to-second endpoint continuation is live-verified for all four paginated commands, and friends exhaustion is live-verified.
2. Whether transparent legacy keychain migration is possible on every supported platform.
3. Whether the first release adds Linux arm64, musl, or Windows.
4. GitHub-only initial distribution versus Homebrew and crates.io at launch.
5. Whether JSON output belongs in the next release or a later milestone.
6. What product decision to make if complete pending-request list/detail or acceptance contracts cannot be verified.
7. Whether a legitimate current native-mobile onboarding contract or official device-authorization flow can eliminate manual trusted-device bootstrap without reproducing browser anti-bot controls. The observed web flow is explicitly not a production candidate.

## 21. Immediate next steps

1. Observe the validated mobile-issued token's lifetime with occasional explicit read-only `auth status` checks across separate processes. Do not poll aggressively, infer an expiry date, store the password, automatically reauthenticate, or retry after rejection.
2. Complete Phase 0 contract discovery for payment creation, request creation, and exact pending-request acceptance. Keep every financial command unavailable until its own request, response, ambiguity, and verification contracts are established.
3. Resolve the remaining release decisions in Section 20, refresh end-user documentation, and repeat the full formatting, lint, test, rustdoc, dependency-policy, and forbidden-pattern checks.

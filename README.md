# venmo-cli

`venmo-cli` is a Rust-only project that provides the `venmo` command for a small, unofficial Venmo CLI.

> This project uses unsupported/private Venmo API endpoints. They can change or stop working without notice.

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
| `venmo init` | `venmo auth login --token` or `venmo auth login` |
| `venmo status` | `venmo auth status` |
| `venmo deinit [--revoke]` | `venmo auth logout [--revoke]` |
| `venmo search <QUERY>` | `venmo users search <QUERY>` |
| `venmo payment-methods list` | `venmo payment-methods list` |

The old `charge`, split-payment, and multi-recipient interfaces were removed. `pay` and direct request creation use the mobile-private API through independently verified Rust contracts; each passed exact synthetic HTTP tests and a separately approved, reconciled one-cent live validation on 2026-07-12. Both enforce a personal/payable recipient, private audience, one-write/no-retry behavior, strict success validation, and ambiguous-outcome classification. `pay` obtains the transaction eligibility token and reports the eligibility response's fee, but it does not reject a method or transaction because the fee is nonzero or unknown. Request creation has no funding or fee fields and never sends money from the authenticated account.

Top-level `accept` and `decline` handle incoming request mutations:

```sh
venmo accept <REQUEST_ID> [--yes]
venmo decline <REQUEST_ID>
```

`accept` fetches an authoritative incoming exact-`pending` private request, requires a personal/payable requester and enough available Venmo balance to cover the full amount, submits no external funding method, shows a default-No preflight, and uses the historical incoming action `approve`. That balance check is only a snapshot: the update does not bind it or prove the final funding source/fee, so the CLI explicitly discloses the limitation and does not claim wallet funding or `$0.00` fees. Controlled live validation established that a successful response uses a distinct settled-payment ID and can retain request-style `action: charge` and party orientation even though activity exposes the resulting private outgoing `pay`; the validator supports both this live representation and the historical pay-oriented representation while requiring all operation-specific fields to match. Reconciled validations confirmed that the request disappears, exactly one matching private outgoing payment settles, and available balance decreases by the exact request amount.

`decline` fetches an authoritative incoming exact-`pending` private request, displays the validated plan and writes immediately without pausing for a prompt or accepting `--yes`, sends no money or funding fields, and uses historical incoming action `deny`—never outgoing-request `cancel`. A controlled live validation sent one non-retried `deny` update, received the original request ID with exact terminal `cancelled` status, removed the request from pending results, produced no payment activity, and left available balance unchanged. Both request mutations treat any unverified response or transport uncertainty as ambiguous and must not be retried before reconciliation.

For `pay`, `--from` identifies the preferred external or backup funding method submitted to Venmo. It does not guarantee that method is debited: Venmo may use available wallet balance first. Pay prompts only when both stdin and stderr are terminals; otherwise it requires `--yes`. Any peer-eligible external bank or card may be selected regardless of whether its method-level fee is zero, nonzero, or unknown. Preflight displays the method-level fee evidence and the separate eligibility-reported fee and warns that eligibility is not bound to the submitted method, so the final fee may differ.

Direct request creation writes immediately after its account and recipient validation; it has no confirmation prompt and does not accept `--yes`. Decline follows the same no-prompt rule because it sends no money, but remains protected as an ambiguous state mutation after possible transmission. If any mutation exits with code `3`, says its outcome is unknown, or says a successful result could not be written, **do not retry it**. Reconcile first with `venmo activity list`, `venmo requests list`, and the official Venmo application.

### Endpoint-native read pagination

Four read-only commands expose the continuation fields used by their source endpoints:

```sh
venmo friends list [--limit N] [--offset N]
venmo users search <QUERY> [--limit N] [--offset N]
venmo activity list [--limit N] [--before-id TOKEN]
venmo requests list [--direction all|incoming|outgoing] [--limit N] [--before TOKEN]
```

Each invocation requests exactly one source API page, validates and buffers that complete page, and then renders its records. `--limit` is the server request page size; it defaults to 10 and cannot exceed 50. Friend listing and user search use a typed nonnegative `--offset` that defaults to 0. Activity uses its opaque `--before-id`, while pending requests use their opaque `--before`. There is no universal continuation input, universal offset, page number, or public multi-page collector.

When the validated source response has another page, the CLI writes only the endpoint-native copyable continuation to stderr, after record output succeeds: `Next offset: <N>`, `Next before-id: <TOKEN>`, or `Next before: <TOKEN>`. Friends use the offset from the validated server next link. User search provisionally synthesizes the next offset when the source page is full, so one final empty page can be required to observe exhaustion. Raw next URLs are never printed or followed.

Pending-request direction remains a local filter over that one source page. A filtered invocation can therefore emit fewer than `--limit` records, including none, while still reporting the source page's `before` continuation. Continuation tokens are bounded, reject whitespace and control characters, and are redacted from parsing errors and diagnostic formatting. The transport still validates every server next link against the fixed origin, route, and endpoint query allowlist and reconstructs subsequent requests locally. Internal exact-recipient resolution remains a separate exhaustive user-search traversal with its existing 200-record/four-page fail-closed bounds and no user-supplied continuation.

### Trusted-device mobile login and reauthentication

`venmo auth login` uses Venmo's unsupported legacy mobile-private password/SMS-OTP flow. Before running it, sign in through Venmo's normal browser interface and manually obtain the `v_id`/device ID for that already trusted browser session. The CLI does not automate the browser, import cookies, or generate a replacement device ID.

Fresh-device research confirmed that Venmo creates a `v_id` before web login and makes it acceptable to the mobile endpoint only after a session-bound identity flow involving DataDome, reCAPTCHA Enterprise, browser-risk telemetry, password, MFA, and an explicit remember-device finalization. No standalone registration, OAuth device grant, or browserless refresh/bootstrap endpoint was found. Replaying or fabricating those browser security signals is intentionally out of scope, and browser-assisted login is not a production CLI design; initial setup therefore still requires the trusted ID to be supplied through hidden input.

Run the login only in your own interactive terminal:

```sh
cargo run -- auth login
```

The CLI prompts for the account identifier, password, trusted device ID, and—only if Venmo returns the recognized challenge—SMS OTP, in that order. Password, device ID, and OTP input are hidden with terminal echo disabled. It sends one non-retried authentication attempt, validates any issued token with the current-account endpoint, then stores only the validated bearer token, device ID, and account metadata in the native OS credential store. Direct password success requires no redundant device-trust update; after OTP, device trust is attempted once after the validated credential is safely stored. Password and OTP material are never stored.

After a credential has been stored, renew an expired token without finding or entering the device ID again:

```sh
cargo run -- auth reauthenticate
```

`venmo auth reauthenticate` requires one readable stored credential and an interactive terminal. It reuses that credential's stored trusted device ID without prompting for or printing it, then prompts only for the account identifier, hidden password, and—only for the exact recognized challenge—hidden SMS OTP. The old bearer token is not validated or required to remain valid. This is a complete, single-attempt password/optional-OTP token issuance, **not** a refresh-token operation; no refresh mechanism is known.

The replacement token is validated through current-account with the stored device ID. Its returned user ID must match the stored credential's user ID before the CLI replaces and reads back the credential. A validation failure or different account leaves the old credential untouched; because a mismatched token was still issued remotely, the CLI warns that it may remain active. Direct password success skips the redundant device-trust request. After OTP, the CLI saves and verifies the replacement first and then makes one best-effort trust request; if only that request fails, it truthfully reports incomplete success while keeping the validated replacement.

This stored-device flow has been live-verified across separate processes: direct password reauthentication issued and saved a same-account replacement token without OTP, and a subsequent `auth status` successfully reloaded and validated it. That proves the no-device-prompt renewal path, not fresh-device bootstrap, headerless authentication, automatic refresh, or any guaranteed token lifetime. Reauthentication does not automatically revoke the previous bearer because the scope of private-API revocation is undocumented; the command warns that the previous token's remote validity is unknown.

Never put a device ID, password, OTP, bearer token, or browser cookie in a command argument, environment variable, file, chat, screenshot, or log. Neither authentication command exposes secret, environment, or stdin-file flags. If Venmo rejects an attempt, stop rather than repeatedly retrying. Password, OTP, and OTP-secret material are never stored.

An existing bearer token can instead be imported through hidden prompts:

```sh
cargo run -- auth login --token
```

Initial password login refuses to replace a readable stored credential. Use `auth reauthenticate` to renew the same account while retaining its device identity. Run `cargo run -- auth logout` first only when intentionally switching accounts; omit `--revoke` unless remote revocation is also intended.

## Contributor documentation

Routine contributor verification is service-free. Do not run ignored/manual tests or live probes,
load the production credential for tests, or run a real financial command as a development check.
The user-facing command descriptions above are not contributor test instructions.

- [Contributing and required verification](CONTRIBUTING.md)
- [Architecture and dependency boundaries](docs/architecture.md)
- [Testing strategy](docs/testing.md)
- [Retained integration/manual contracts](docs/retained-test-contracts.md)
- [Evidence-gated follow-ups](docs/evidence-gated-follow-ups.md)
- [Public facade inventory and 0.1 compatibility](docs/public-api.md)

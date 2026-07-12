# venmo-cli

`venmo-cli` is a Rust-only project that provides the `venmo` command for a small, unofficial Venmo CLI.

> This project uses unsupported/private Venmo API endpoints. They can change or stop working without notice.

## Requirements and installation

Building requires Rust 1.95.0. The checked-in `rust-toolchain.toml` pins the required toolchain. From the repository root:

```sh
cargo build
cargo test
cargo run -- --help
cargo run -- friends list
```

To install the current checkout into Cargo's binary directory instead:

```sh
cargo install --path .
venmo --help
```

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

The old `charge`, split-payment, and multi-recipient interfaces were removed. Financial commands in the Rust grammar remain unavailable until their current private-API contracts and safety checks are independently verified; the CLI does not fall back to the former implementation.

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

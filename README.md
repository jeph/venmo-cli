# Venmo CLI

[![Release](https://github.com/jeph/venmo-cli/actions/workflows/release.yml/badge.svg)](https://github.com/jeph/venmo-cli/actions/workflows/release.yml)
[![Latest release](https://img.shields.io/github/v/release/jeph/venmo-cli)](https://github.com/jeph/venmo-cli/releases/latest)
[![License](https://img.shields.io/github/license/jeph/venmo-cli)](LICENSE)

> [!IMPORTANT]
> This project uses reverse engineered non-public Venmo API endpoints. They can change or stop working without notice.

## Why this one?

Most Venmo CLIs focus on basic payments and requests. This one covers much more of the Venmo
experience from the terminal:

- **Full payment controls.** Inspect funding options, select an exact balance/bank/card source,
  choose private/friends/public visibility, and optionally request Venmo Purchase Protection.
- **The complete request lifecycle.** List and inspect pending requests, create new ones, accept with
  funding and protection controls, decline incoming requests, or cancel outgoing requests.
- **Built-in OTP challenges (2FA).** Interactive login, payments, request creation, and request
  acceptance can complete Venmo's SMS verification flow when required.
- **Social features.** Search and inspect users, browse your or another user's visible friends and
  activity, manage friendships, like or unlike activity, and list, add, or remove comments.
- **Balance and cash-out.** Check your wallet balance, inspect transfer eligibility, and transfer a
  fixed amount or all available funds to the selected standard bank destination.
- **Native and security-conscious.** Prebuilt arm64 and x86_64 binaries require no language runtime.
  Linux releases are standalone, fully static musl binaries with no glibc dependency. macOS releases
  are signed and notarized, credentials use the platform keyring when available, and uncertain API
  outcomes fail closed instead of being guessed.

## Installation

### Homebrew

On macOS or Linux, install the latest release with Homebrew:

```sh
brew install jeph/tap/venmo
```

### Build from source

With Rust installed, build and install from the repository root:

```sh
cargo install --locked --path .
```

### Agent Skill

After installing the CLI, install the optional skill globally for your supported AI agents:

```sh
npx skills add jeph/venmo-cli@venmo-cli -g
```

## Usage

### Authorization

Start the interactive login flow:

```sh
venmo auth login
```

Venmo requires a trusted browser `v_id` when the CLI signs in. Retrieve it from your own Venmo
session:

1. Open [Venmo's account site](https://account.venmo.com/) in a normal browser window.
2. Sign in, complete any MFA challenge, and choose to remember or trust the device if prompted.
3. After the account page loads, open the browser's developer tools and select
   **Application/Storage → Cookies**.
4. Select the `account.venmo.com` or `venmo.com` origin, find the cookie named `v_id`, and copy only
   its value.
5. Paste the value at the `Trusted Venmo v_id/device ID` prompt.

The cookie exists before authentication but is not trusted until the browser login and device-trust
flow completes, so retrieve it only after signing in.

Check the current authorization status:

```sh
venmo auth status
```

Remove the locally stored authorization:

```sh
venmo auth logout
```

Credentials are stored in macOS Keychain or Linux Secret Service when available. On Linux without
Secret Service, the bearer token is stored unencrypted in an owner-only XDG state file at
`$XDG_STATE_HOME/venmo-cli/credential.json`. If a keyring becomes available later, run
`venmo auth login` again; the CLI verifies the new keyring entry before deleting the fallback file.
`venmo auth logout` removes locally stored credentials but does not revoke the remote token.

### Paying and requesting money

Inspect your balance and the funding sources available for peer payments:

```sh
venmo balance
venmo pay options
```

Pay a user or create a request:

```sh
venmo pay user <USERNAME> <AMOUNT> <NOTE> [--source <SOURCE_ID>] [--protect] [--visibility private|friends|public]
venmo requests create <USERNAME> <AMOUNT> <NOTE> [--visibility private|friends|public]
```

Quote notes, messages, and multi-word search queries when they contain spaces.

Without `--source`, payments choose an eligible Venmo balance or the unique eligible external
method. Use a source ID from `venmo pay options` to select an exact balance, bank, or card.
`--protect` requests Venmo Purchase Protection for an eligible personal payment.

List, inspect, and act on pending requests:

```sh
venmo requests list [--direction all|incoming|outgoing] [--limit <N>] [--before <TOKEN>]
venmo requests info <REQUEST_ID>
venmo requests accept <REQUEST_ID> [--source <SOURCE_ID>] [--protect]
venmo requests decline <REQUEST_ID>
venmo requests cancel <REQUEST_ID>
```

Use an incoming request ID with `accept` or `decline`, and an outgoing request ID with `cancel`.
Accepting a request supports the same funding-source and Purchase Protection controls as paying a
user.

### Viewing and managing activity

List activity for your account or another visible personal profile, then inspect an individual
record:

```sh
venmo activity list [--user <USERNAME>] [--limit <N>] [--before-id <TOKEN>]
venmo activity info <ACTIVITY_ID>
```

`activity list` defaults to the active account. Pass `--user <USERNAME>` to list the activity
visible to you for another personal profile. The username may include a leading `@`. Venmo privacy
settings determine which records are visible, and amounts are omitted from other-user activity.
Some activity types, including rewards disbursements, do not provide an amount or status. Their
unavailable table fields are blank, and `activity info` omits those detail lines.

Like, unlike, and manage comments on visible activity:

```sh
venmo activity like <ACTIVITY_ID>
venmo activity unlike <ACTIVITY_ID>
venmo activity comments list <ACTIVITY_ID> [--limit <N>] [--offset <N>]
venmo activity comments add <ACTIVITY_ID> <MESSAGE>
venmo activity comments remove <COMMENT_ID>
```

List commands print a continuation when another page is available. Pass that value back through
the matching `--before-id` or `--offset` option. Continuation types are not interchangeable.

### Transferring funds

Inspect standard bank-transfer eligibility, then transfer a fixed amount or the full available
balance:

```sh
venmo transfer options
venmo transfer out <AMOUNT_OR_ALL> [--speed standard]
```

Use a positive dollar amount or the exact value `all`. The CLI supports standard cash-out to the
unique eligible bank destination. Instant transfers and manual destination selection are not
supported.

### Social features

Search for users, inspect a profile, and browse visible friend lists:

```sh
venmo users search <QUERY> [--limit <N>] [--offset <N>]
venmo users info <USERNAME>
venmo friends list [--user <USERNAME>] [--limit <N>] [--offset <N>]
```

`friends list` defaults to the active account. Pass `--user <USERNAME>` to list the friends visible
to you for another personal profile. The username may include a leading `@`. Venmo privacy settings
may hide some or all of that profile's friends.

Manage friendships by exact username:

```sh
venmo friends add <USERNAME>
venmo friends remove <USERNAME>
```

`friends add` sends a new request or accepts an incoming request according to the current
relationship. `friends remove` removes an existing friend or cancels an outgoing request. It does
not decline an incoming request.

### Mutating commands

Every financial or social mutation displays its action details and requires confirmation that
defaults to No. Pass `--yes` to skip only the final confirmation. Validation and any available
preflight still run. Pass `--dry-run` instead to preview the action without performing the mutation
or asking for confirmation. The two flags are mutually exclusive.

For example, preview a payment or perform it without the confirmation prompt:

```sh
venmo pay user <USERNAME> <AMOUNT> <NOTE> --dry-run
venmo pay user <USERNAME> <AMOUNT> <NOTE> --yes
```

The second command performs the payment after validation and preflight.

Mutations are never automatically retried. If the CLI reports exit code 3 or an unknown outcome,
check activity, pending requests, and the official Venmo app before attempting anything else.

### Debug logging

Pass the global `--debug` flag before or after a subcommand:

```sh
venmo --debug balance
venmo activity list --debug
```

Debug diagnostics are written to stderr and include bounded request timing, route, status, retry,
response-size, and sanitized error information. Credentials, raw request and response bodies,
dynamic identifiers, usernames, notes, and amounts are omitted.

### JSON output

Pass the global `--json` flag before or after a subcommand to receive one compact, newline-terminated
JSON object:

```sh
venmo --json balance
venmo activity list --json
venmo requests create @alice 12.50 "Dinner" --dry-run --json
```

Example:

```console
$ venmo balance --json
{"command":"balance","ok":true,"data":{"balance":{"available":{"amount":"12.34","currency":"USD"},"on_hold":{"amount":"0.00","currency":"USD"}}}}
```

Successful results go to stdout. Failures go to stderr.

## Roadmap

This project is currently **alpha** and will probably remain there for a while. The commands are
tested to a reasonable degree, but broader user testing and issue reports are needed before the
implementation can be considered stable. The OTP challenge (2FA) flows especially need more
real-world validation.

Planned work includes:

- **Transfer in.** Add a command for moving money into Venmo once the private API contract can be
  reverse engineered and validated with enough confidence. That endpoint has not yet been reliably
  established.
- **Simpler login.** Investigate a reliable way to establish a trusted device without requiring
  users to retrieve and enter a browser `v_id` manually.
- **Non-interactive OTP challenges.** Design a secure structured flow that lets scripts and
  LLM-based tools respond to SMS OTP challenges without an interactive terminal prompt.
- **Clearer terminal output.** Make the regular non-JSON output less wordy, more consistent, and
  easier to scan while preserving important safety and recovery information.
- **Windows support.** Add and validate native Windows builds, credential storage, terminal prompts,
  and release packaging.
- **Fee visibility.** Resolve and display applicable fee rates more consistently so fewer commands
  fall back to `unknown`, without guessing when Venmo does not provide authoritative fee data.
- **Activity reactions.** Support Venmo reactions on activity beyond the current like, unlike, and
  comment commands once the private API behavior is reliably established.
- **Beta stabilization.** Move to beta after enough users have exercised the CLI and reported issues.
  The beta phase will focus especially on making the `--json` output shapes consistent and stable.

## Contributing

Pull requests are welcome. If you run into a bug, unexpected behavior, unclear output, or a
platform-specific problem, please [open an issue](https://github.com/jeph/venmo-cli/issues). Reports
from real-world usage are especially helpful while the project is in alpha.

## Donations

If this project is useful to you and you would like to support its development, donations are
welcome to **@Jephrey** on Venmo.

I'd encourage you to use the CLI if donating 😊

```sh
venmo pay user @Jephrey 5.00 "Supporting venmo-cli"
```

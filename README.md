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
- **Social features.** Search and inspect users, browse your or another user's visible friends and
  activity, manage friendships, like or unlike activity, and list, add, or remove comments.
- **Balance and cash-out.** Check your wallet balance, inspect transfer eligibility, and transfer a
  fixed amount or all available funds to the selected standard bank destination.
- **Native and security-conscious.** Prebuilt arm64 and x86_64 binaries require no runtime;
  macOS releases are signed and notarized, credentials use the platform keyring when available, and
  uncertain API outcomes fail closed instead of being guessed.

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

## Authorization

Venmo requires a trusted browser `v_id` when the CLI signs in. Retrieve it from your own Venmo
session:

1. Open [Venmo's account site](https://account.venmo.com/) in a normal browser window.
2. Sign in, complete any MFA challenge, and choose to remember or trust the device if prompted.
3. After the account page loads, open the browser's developer tools and select
   **Application/Storage → Cookies**.
4. Select the `account.venmo.com` or `venmo.com` origin, find the cookie named `v_id`, and copy only
   its value.
5. Run `venmo auth login` and paste the value at the hidden `Trusted Venmo v_id/device ID` prompt.

The cookie exists before authentication but is not trusted until the browser login and device-trust
flow completes, so retrieve it only after signing in.

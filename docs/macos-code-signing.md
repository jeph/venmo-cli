# macOS code signing

Local macOS builds use a development signature so the login Keychain can recognize rebuilt
versions of `venmo` as the same application. This avoids making every linker-generated ad-hoc
signature a new Keychain application identity.

## Local `cargo run` behavior

On macOS, [`.cargo/config.toml`](../.cargo/config.toml) installs
[`scripts/cargo-runner-macos.sh`](../scripts/cargo-runner-macos.sh) as Cargo's target runner. The
runner signs only an executable whose basename is exactly `venmo`; unit-test, integration-test, and
benchmark harnesses execute unchanged. It then verifies the signature before replacing itself with
the executable, preserving the program's arguments, exit status, and signals.

The default local identity and code identifier are:

```text
Apple Development: Jeph Liu (7CJV28MFNU)
io.jeph.venmo
```

If the default certificate is not installed, the runner warns and executes the binary without
adding the development signature. This keeps normal Cargo commands usable for other contributors.
To deliberately select another installed identity, set `VENMO_CODESIGN_IDENTITY`; an explicitly
requested but unavailable or empty identity is an error:

```sh
VENMO_CODESIGN_IDENTITY='Apple Development: Example (TEAMID)' cargo run -- --help
```

`cargo run -- ...` signs immediately before execution. Cargo has no post-build runner, so any other
command that relinks the CLI—such as `cargo build` or some `cargo test` invocations—can leave the
new executable ad-hoc signed until a subsequent `cargo run` signs it. Running
`target/debug/venmo` directly in between can consequently produce a new Keychain approval prompt.
`cargo install` likewise does not use this runner for the installed copy.

## One-time Keychain approval

The first signing operation can ask permission to use the development certificate's private key.
Confirm that the requesting program is `/usr/bin/codesign`, then choose **Always Allow**. The first
credential-reading command from the newly signed `venmo` can separately ask for access to the
existing `venmo-cli` / `default` generic-password item; after confirming the signed executable,
choose **Always Allow** there as well.

Do not choose an option that allows all applications to access the credential, and do not broadly
rewrite key partition lists with `security set-key-partition-list`. After the signed workflow is
validated, obsolete application entries left by the removed Node CLI or old ad-hoc builds may be
removed from that one credential item's access-control list using Keychain Access.

Routine checks must remain service-free. `cargo run -- --help` is safe signing verification, but a
production credential read is a separate, explicitly authorized owner action—not an automated test
or contributor verification step.

## Inspecting the local signature

After a service-free run:

```sh
cargo run -- --help
codesign --verify --strict --verbose=2 target/debug/venmo
codesign -dvvv target/debug/venmo
codesign -d -r- target/debug/venmo
```

The details should show identifier `io.jeph.venmo`, the Apple Development authority, and the
certificate's team identifier. The designated requirement should be certificate-backed rather than
a requirement consisting only of a changing `cdhash`. Rebuilding can change the executable's
CDHash, while the certificate-backed designated requirement remains equivalent.

## Release signing is separate

The local runner intentionally omits hardened runtime and a secure timestamp. Future distributed
artifacts must use the `Developer ID Application: Jeph Liu (F4KLVJSVC2)` identity with the same
`io.jeph.venmo` identifier, hardened runtime, a secure timestamp, strict signature verification,
and Apple notarization using a Keychain-stored `notarytool` profile. Release automation must never
store certificate or notarization secrets in the repository.

Because the Developer ID certificate produces a different designated requirement from the local
Apple Development certificate, the first release-signed executable may require its own one-time
credential approval.

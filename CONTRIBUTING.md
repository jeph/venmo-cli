# Contributing

Start with the [architecture](docs/architecture.md), [testing strategy](docs/testing.md), and
[public facade inventory](docs/public-api.md). Also review the
[retained test contracts](docs/retained-test-contracts.md) and
[evidence-gated follow-ups](docs/evidence-gated-follow-ups.md) before changing a boundary or an
evidence-sensitive invariant. macOS contributors should also review the local
[code-signing workflow](docs/macos-code-signing.md).

## Safety boundaries

This project controls authentication and financial operations through unsupported private APIs. Automated development and CI must never use a live credential or invoke a real financial mutation. Financial-path tests must use in-process fakes or loopback mock servers. Tests marked `ignored` are manual contracts and must not be included in routine verification.

Routine contributors must not run an ignored test by name, enable ignored tests, run a live schema
probe, use the production keychain entry, or invoke a real `pay`, request-creation, `accept`, or
`decline` operation for development or verification. Do not turn a product command into a test
step. Separately authorized owner/release procedures are outside routine contribution and are not
initiated by these docs.

Changes must preserve the behavioral and safety contracts in [`PLAN.md`](PLAN.md), especially evidence requirements, one-write/no-retry behavior, bounded resource use, redaction, and ambiguous-outcome reporting.

## Required verification

Run these commands from the repository root with the toolchain pinned by `rust-toolchain.toml`:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --doc --all-features
cargo deny check
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
```

Do not add an ignored-test selector to the test command. Review snapshot changes explicitly rather than accepting them automatically. See the [testing strategy](docs/testing.md) for test-layer and assertion requirements.

## Unsafe and panicking operations

First-party code must not use `unsafe`, `unwrap`, `expect`, `panic`, `unreachable`, `todo`, or `unimplemented`. Workspace lints enforce these rules. Fallible boundaries must return typed errors, and error text must not contain credentials, secrets, raw untrusted continuation values, or unsanitized terminal control characters.

## Mutation policy

Prefer immutable values, immutable references, struct-update construction, and iterator combinators when they preserve clarity and allocation behavior. Production mutation is acceptable only when it is localized and required for one of these reasons:

- an external trait or stateful API requires a mutable reference;
- secret ownership transfer or explicit zeroization requires in-place access;
- bounded accumulation enforces a resource limit;
- stable ordering, deduplication, or trust-boundary validation requires incremental state;
- a writer, response stream, signal stream, or pinned future must advance.

Do not hide these cases behind functional abstractions that allocate more, weaken bounds, or obscure security-sensitive sequencing. Production interior mutability requires an explicit concurrency and invariant rationale. Test-only interior mutability is acceptable when it is confined to a fake and exposed as one comparable final-state snapshot.

Keep safety-sensitive imperative order visible where it is itself a contract: secret ownership and
zeroization, bounded response accumulation, first-seen duplicate validation, credential
persistence/read-back handling, and financial preflight -> authorization -> interrupt protection
-> one write -> result rendering. These are narrow exceptions, not permission for unrelated
mutable state.

## Test structure

The complete policy, including typed transcript and snapshot rules, is in the
[testing strategy](docs/testing.md#required-stateful-test-shape).

Tests driven by a stateful script or fake should follow this order:

1. Set up dependency scripts and fakes.
2. Create the immutable initial state.
3. Create the complete expected final outcome.
4. Execute the behavior once.
5. Compare the complete result, fake state, call transcript, and output with the expected outcome.

Use typed calls rather than boolean argument-match flags. Exact one-write and no-retry behavior belongs in the transcript. Secret-bearing values and dynamic errors should be projected into redacted, stable snapshots before comparison. Pure value and property tests should compare whole values directly without artificial state objects.

Representative loopback `wiremock`/`reqwest` integration tests retain exact matcher and
captured-request assertions rather than artificial fake-state wrappers; see the documented
exception in the testing strategy.

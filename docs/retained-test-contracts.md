# Retained test contracts

> These are retained contract descriptions, not routine test instructions. Contributors must not
> execute ignored tests, access a live credential or Venmo service, or invoke a real financial
> command. Use only the service-free checks in [`CONTRIBUTING.md`](../CONTRIBUTING.md).

## Service-free process coverage

`tests/process.rs` intentionally retains child-process coverage for help, version, invalid input,
release-gated commands, noninteractive prompt rejection, completions, and manpage generation. Each
path is deterministic and must complete without keychain or network access. Financial behavior is
covered below the process boundary with fakes and synthetic HTTP contracts, not a real command.

## Interactive PTY behavior

Phase 7 does not add pseudo-terminal process tests. The supported hidden token, device-ID,
password, OTP, funding-selection, and default-No confirmation flows remain an explicit contract
category for later validation with a portable PTY harness. A harness must provide deterministic
terminal sizing, input, signal delivery, output normalization, cancellation, and cleanup on both
macOS and Linux CI before these tests can become required.

Current automated coverage deliberately stops at deterministic boundaries: terminal-capability
gating, prompt error classification, structured choice presentation, injected financial
interruption behavior, non-interactive child-process rejection, and exact sanitized renderer
output. This retention is not permission to add a test-only environment switch or to exercise a
live credential, keychain prompt, Venmo service, or financial operation.

## Global tracing subscriber

Verbose delegated production execution installs Rust's one-per-process global tracing subscriber.
Phase 7 keeps that real installation out of in-process unit tests because exercising it would make
results depend on test order, parallelism, and subscribers installed by the test harness or
dependencies. An injected recording/failing initializer proves that completions, closed candidate
gates, and noninteractive authentication never attempt installation, while delegated commands do
so before their fake executor. The isolated logging-unit contract proves that disabled diagnostics
do not install a subscriber. Exact redaction is tested at the structured values, HTTP
request/response wrappers, errors, and renderers that can carry sensitive data.

A future packaged-binary diagnostics test may exercise `--verbose` only in a fresh child process
with deterministic stderr capture and no keychain or network access. It must not weaken the
single-subscriber production behavior, add a reset hook, serialize the entire unit suite, or assert
unstable formatting such as timing data.

## Native credential-store contract

The ignored native credential-store round trip under
`src/adapters/credentials/keyring/tests.rs` is retained for separately controlled platform/release
validation. It uses a randomized test-only service and synthetic values, but it can still trigger
OS credential-store behavior and is therefore excluded from routine and automated tests. Its
presence does not authorize a contributor to execute it or use the production `venmo-cli` /
`default` entry.

Routine codec and keyring-adapter coverage uses a scripted backend and whole-state equality. That
coverage must remain service-free.

## Concrete loopback HTTP integration contracts

Representative Venmo adapter and transport tests use `wiremock` with the concrete `reqwest`
transport on a loopback server. They intentionally retain exact matcher and captured-request
assertions for serialization, headers, fixed-origin continuation handling, response bounds,
timeouts, redirects, and no-retry behavior. Those tests are routine and service-free, but they are
not mutable-fake tests and are not claimed to use one final fake-state snapshot.

The one-snapshot requirement still applies to `ScriptedTransport` tests: their observation includes
the projected result, ordered typed request transcript, remaining scripted responses, and the
unexpected-request flag. Neither style may contact the production service.

## Historical live read-contract probes

The ignored probes under `src/adapters/venmo/client/tests/live.rs` preserve the bounded,
structure-only read investigations that supported earlier contract work. They use the active
native credential and production endpoint, so routine contributors and CI must not execute them,
even though they contain no write operation. Exact sanitized synthetic transport and loopback
tests are the maintained regression contract.

The probes are not a general discovery harness and must never be extended to a financial route.
Evidence-gated questions remain recorded in
[`evidence-gated-follow-ups.md`](evidence-gated-follow-ups.md), not assigned to contributors as live
experiments.

## No retained live financial test

There is deliberately no ignored test or contributor runbook for a live payment or request
mutation. Financial safety is proved routinely with typed feature transcripts and exact synthetic
wire tests. Any separately authorized owner procedure is outside this test suite and does not make
a live financial command part of development, CI, or release verification.

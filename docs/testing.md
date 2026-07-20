# Testing strategy

> **Routine contributor tests are service-free. Do not run ignored/manual tests, live probes, or
> real remote mutations, including friendship, payment, request, and transfer writes. Do not
> supply a real credential to test code.** Follow
> [`CONTRIBUTING.md`](../CONTRIBUTING.md#safety-boundaries) and use only the routine verification
> commands documented there.

The suite tests each boundary at the lowest level that can prove its contract. Feature behavior is
tested against narrow fakes, wire behavior against synthetic transports or loopback servers, and
terminal behavior against captured writers or service-free child processes.

Transfer shorthand coverage resolves exact lowercase `all` only from synthetic fresh positive
available-balance snapshots, rejects zero/negative values before options or a write, pins the
action-specific confirmation and disclosure output, and proves that T2 receives only the resolved
integer cents. It never performs a live transfer.

Other-user activity coverage is also service-free. Feature tests pin own-username optimization,
bounded exact personal-profile resolution, subject retention, and unsupported-profile rejection.
Scripted transport tests pin the selected-user path, native unfiltered query, subject-relative
direction, scope-aware continuation, supported audiences, private viewer participation, and
rejection of nonpayment records. Detail tests preserve absolute payment actor/target identities.

## Test layers

| Layer | Primary contract |
| --- | --- |
| Shared and feature-model unit tests | Parsing, invariants, exact value behavior, redaction, and failure classification |
| Feature tests | Orchestration, capability use, ordered calls, zero/one-write behavior except the exact same-UUID payment step-up continuation, no automatic retry, and complete outcomes |
| Venmo adapter tests | Exact request shape, credential mode, response mapping, bounds, fixed-origin continuation validation, and transport error classification |
| Credential adapter tests | Exact codec compatibility and native-adapter behavior through a scripted backend |
| CLI adapter tests | Argument grammar, dispatch, logging/precondition order, private terminal seams, prompts, interruption handling, deterministic time-zone conversion and UTC fallback, rendering, error categories, and output failures |
| `tests/*.rs` and facade compile-fail doctests | Public `cli`/`model` facade usability and service-free compiled-process contracts |

Use synthetic values only. `ScriptedTransport` proves the typed client request without opening a
socket; loopback `wiremock` tests prove the concrete HTTP transport. Neither is a reason to call the
production service.

## Required stateful-test shape

The required pattern is **Setup -> initial state -> expected outcome -> execute -> whole
equality**.

Every feature, adapter, or dispatch test driven by a stateful script or fake must make these phases
visible and keep them in this order. This includes the scripted credential backend and
`ScriptedTransport` client tests:

1. **Setup** dependency scripts, fixtures, and case data. Setup must not execute the behavior.
2. **Initial state** construct the complete immutable starting state and shared typed transcript.
3. **Expected outcome** construct the complete expected result and final fake state before
   execution.
4. **Execute** the behavior exactly once.
5. **Whole equality** project the observation and compare it to the expected value in one equality
   assertion.

The preferred outline is:

```rust,ignore
// Setup.
let script = /* synthetic dependency responses */;

// Immutable initial state.
let transcript = /* empty typed transcript */;
let dependencies = /* fakes created from the script */;

// Complete expected outcome and final fake state.
let expected = Snapshot { /* result, state, transcript, output */ };

// Execute once.
let result = behavior(&dependencies).await;
let observed = snapshot(result, &dependencies, &transcript);

assert_eq!(observed, expected);
```

`Snapshot` or `Observation` should include everything the behavior can observably change: the
typed outcome, final fake state, remaining scripted responses, ordered calls, and stdout/stderr or
writer state. Keeping an unexpected second response in the script is useful: whole equality then
proves that it was not consumed. Do not replace this with scattered assertions that can omit an
extra call, retry, partial write, or state transition.

Pure value tests and property tests are intentionally simpler. They should compare whole values or
state the property directly rather than inventing fake mutable state.

## Typed transcripts

Represent interactions with an enum whose variants carry typed arguments:

```rust,ignore
enum Call {
    ReadCredential,
    CurrentAccount { session: RedactedSession },
    CreatePayment { plan: PayPlanSnapshot },
}
```

The transcript must prove order, count, and safety-relevant arguments. Do not use loose booleans
such as `called`, `arguments_matched`, or `wrote_once`; they cannot distinguish ordering, repeated
calls, or a call that partially matched. Snapshot remaining response scripts and fake persistence
state alongside the transcript.

Tests may use interior mutability to record calls or advance a script. Keep it inside the fake and
expose one immutable, equality-comparable final snapshot. Production interior mutability follows
the stricter [mutation policy](../CONTRIBUTING.md#mutation-policy).

Representative loopback `wiremock`/`reqwest` tests are the deliberate integration-contract
exception to the fake-state shape. Their concrete matcher registrations, captured-request checks,
request counts, response bounds, timeout behavior, redirect/origin enforcement, and no-retry
checks are themselves the HTTP integration contract. They may retain exact request/result
assertions instead of being wrapped in an artificial fake-state snapshot. This exception does not
apply to `ScriptedTransport` or another mutable scripted fake, and it never permits a production
service call.

Activity adapter fixtures separately prove that current-account payment stories require exact
positive money while authorized external social stories may carry a null amount. Other-user output
snapshots omit the Amount column entirely, and external detail output omits the Amount line rather
than rendering a placeholder. Supplied malformed amounts remain contract failures.

Friendship mutation coverage follows the same zero/one-write rules as financial workflows while
remaining a distinct state-write class. Exact tests pin form-urlencoded P4, bodyless P5, native
relationship-state selection, default-No and `--yes`, interruption ties, post-transmission
ambiguity, and authoritative user-detail reconciliation. Tests must leave an unexpected second
mutation response unconsumed to prove there is no retry.

Request-acceptance coverage proves both funding branches, explicit source selection, and explicit
protection. Sufficient balance must skip payment methods and approval eligibility before the legacy
action-only write only for a default unprotected acceptance. Every shortfall, explicit `--source`,
and `--protect` plan must first resolve exactly one
unacknowledged notification by matching its payment ID to the canonical pending-request ID. Tests
cover both direct root `id`/`payment.id` notifications and current
`additional_properties.request.id`/`additional_properties.request.payment.id` wrappers, retain the
associated request-action ID rather than the current wrapper's outer ID, then select sufficient
balance, the exact requested peer-eligible balance/bank/card source, or the unique default-or-sole
external method. Tests bind
source eligibility to the authoritative requester/amount/note/source, reject unavailable explicit
sources and insufficient explicit balance, reject denial/missing token/malformed or overflowing fee
data before writing, and send one modern source-funded approval to that action ID. Missing,
duplicate, conflicting, invalid, and oversized notification responses fail before funding selection.
Unprotected plans must omit returned fees.
Protected plans must reject a fee above the request amount, submit normalized fee records, and show
the estimated seller deduction plus recipient proceeds without increasing the payer amount. Webview
or malformed success is ambiguous and never retried.

All four request mutations have exact prepare/render/authorize/execute coverage. Detail output is
flushed before confirmation, default-No rejection and cancellation send zero writes, off-TTY use
requires `--yes`, and `--yes` skips only the prompt. Output failure before authorization prevents
signal installation and the write; post-write output failure retains ambiguous-result semantics.
Outgoing cancellation tests additionally pin exact outgoing ownership, `pending`/`held` state
acceptance, form-urlencoded `action=cancel`, self-to-counterparty response orientation, exact
terminal `cancelled` status, and rejection of incoming, payment, stale, or mismatched records. An
exact one-request assertion proves the mutation is never retried.

Peer-payment output snapshots show the selected source under the neutral prewrite `Funding source`
label and applicable method metadata under `Funding-source fee`. They omit the internal
automatic/explicit policy marker and the blank-source eligibility fee/total, while success output
continues to identify only the funding-source ID submitted in the request. Request-acceptance
snapshots apply the same prewrite source labels while retaining the separate protected seller-fee
and recipient-proceeds estimates.

Payment step-up coverage is entirely service-free. Exact transcripts prove an initial creation
challenge, terminal-capability check, one SMS issue, one hidden six-digit prompt, one verification,
and at most one verified creation using the same immutable plan and UUID. Wire tests pin `/graphql`,
the P2P issue/validate documents and inputs, `validated: true` precedence over reason metadata,
known false-result rejection reasons, and the exact verified-payment metadata. Tests must prove the
OTP is redacted and that noninteractive use, issuance failure, prompt
failure, incorrect/expired/unexpected OTP, excessive attempts, malformed responses, and a repeated
challenge stop without another payment submission.

Payment rejection tests map only exact HTTP/status/root-code dossiers. HTTP 403/root `1360` must
render the 10-minute duplicate explanation, and HTTP 403/root `10100` must say that Venmo's
server-side checks blocked the payment and that the operator should try again later or use the
official app. Different statuses, nested/non-root locations, and unknown codes remain
outcome-unknown rather than inheriting those messages.

## Secret and error snapshots

Never place raw access tokens, device IDs, passwords, OTPs, OTP secrets, eligibility tokens, or
real account data in a transcript, `Debug` value, snapshot file, panic message, or expected error.
Project secret-bearing values before comparison, for example to `Redacted`, `Stored`, `Imported`,
`Issued`, or `Other`. The projection may prove which synthetic value flowed through a boundary,
but its comparable/debug representation must not contain the value.

Likewise, project dynamic errors to stable typed observations: an error variant, safe failure kind,
exit category, and—only where it is a user contract—exact sanitized display text. Do not snapshot
platform source chains, timing, addresses, raw response bodies, or `Debug` output that could carry
secrets. Renderer and process snapshots must contain only reviewed synthetic data.

Debug-diagnostic tests must use a scoped test subscriber rather than installing the process-global
production subscriber. Exact fixtures must prove that `--debug` emits only static command names,
static route templates, bounded transport metadata, and bounded terminal-safe error fields. Include
synthetic tokens, device IDs, OTP material, dynamic URL values, notes, amounts, funding IDs, and
unrelated JSON fields and prove none appear. Default execution must remain silent, and removed
`-v`/`--verbose` forms must be parser errors.

## Exact assertions and property assertions

Use **exact assertions** for contracts where any changed byte, field, order, or count matters:

- command/help schemas, rendered tables, sanitized errors, exit codes, and stream
  placement;
- the complete ordered feature transcript, including zero/one write or the exact payment step-up
  continuation, and no automatic retry;
- HTTP method, fixed origin, route, query, required headers, content type, body, credential mode,
  operation class, and response-capture mode;
- response-envelope mapping, pagination continuation behavior, and bounded failure outcomes; and
- credential compatibility envelopes and redacted debug/display output.

Use **property assertions** for broad mathematical or parser invariants, such as exact-money
display round trips, bounds, rejection classes, or sanitization over generated inputs. Properties
supplement rather than replace named exact examples at safety boundaries. A generated test that
proves “some valid request succeeds” cannot replace the exact financial wire contract or its
complete call-count transcript.

Review every snapshot diff as code. Never accept snapshots automatically merely to make a test
pass.

## Integration and retained manual contracts

`tests/cli.rs`, `tests/domain.rs`, and `tests/errors.rs` exercise only the curated public facades.
The CLI facade compile-fail doctest proves that terminal-capability snapshots cannot be imported
publicly, while positive public-API tests exercise the curated parser and runtime-fallback
signatures. Private dispatch tests inject only synthetic terminal state, logging initialization,
and fake command execution.

`tests/process.rs` launches the compiled binary only for service-free paths such as help, version,
argument rejection, and manpage rendering. A process test
must fail before keyring or network access unless access is the explicitly isolated contract under
test.

Debug precondition coverage injects a recording/failing logging initializer into the private
dispatch seam. Complete state equality proves noninteractive login makes zero initializer and
executor calls. Delegated fake
commands, including `requests accept`, `requests decline`, and `requests cancel`, prove initialization occurs first and
that an initialization error prevents execution.
These tests never install the real process-global subscriber.

Some platform, interactive-terminal, global-subscriber, and historical read-contract coverage is
deliberately retained outside the routine suite. Their exact status and replacement criteria are
documented in [retained test contracts](retained-test-contracts.md). An ignored test is a manual
contract marker, not an invitation or a contributor verification step. Do not execute it directly
or by enabling ignored tests.

Evidence-sensitive model questions are tracked in
[evidence-gated follow-ups](evidence-gated-follow-ups.md). Tests must preserve those current
contracts; they must not probe the live service to settle them.

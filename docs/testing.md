# Testing strategy

> **Routine contributor tests are service-free. Do not run ignored/manual tests, live probes, or
> real financial commands. Do not supply a real credential to test code.** Follow
> [`CONTRIBUTING.md`](../CONTRIBUTING.md#safety-boundaries) and use only the routine verification
> commands documented there.

The suite tests each boundary at the lowest level that can prove its contract. Feature behavior is
tested against narrow fakes, wire behavior against synthetic transports or loopback servers, and
terminal behavior against captured writers or service-free child processes.

## Test layers

| Layer | Primary contract |
| --- | --- |
| Shared and feature-model unit tests | Parsing, invariants, exact value behavior, redaction, and failure classification |
| Feature tests | Orchestration, capability use, ordered calls, zero/one-write behavior, no retry, and complete outcomes |
| Venmo adapter tests | Exact request shape, credential mode, response mapping, bounds, fixed-origin continuation validation, and transport error classification |
| Credential adapter tests | Exact codec compatibility and native-adapter behavior through a scripted backend |
| CLI adapter tests | Argument grammar, dispatch, logging/precondition order, private terminal seams, prompts, interruption handling, rendering, error categories, and output failures |
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

## Exact assertions and property assertions

Use **exact assertions** for contracts where any changed byte, field, order, or count matters:

- command/help/completion schemas, rendered tables, sanitized errors, exit codes, and stream
  placement;
- the complete ordered feature transcript, including zero/one write and no retry;
- HTTP method, fixed origin, route, query, required headers, content type, body, credential mode,
  operation class, and response-capture mode;
- response-envelope mapping, pagination continuation behavior, and bounded failure outcomes; and
- credential compatibility envelopes and redacted debug/display output.

Use **property assertions** for broad mathematical or parser invariants, such as exact-money
display round trips, bounds, rejection classes, or sanitization over generated inputs. Properties
supplement rather than replace named exact examples at safety boundaries. A generated test that
proves “some valid request succeeds” cannot replace the exact financial wire contract or the
one-write transcript.

Review every snapshot diff as code. Never accept snapshots automatically merely to make a test
pass.

## Integration and retained manual contracts

`tests/cli.rs`, `tests/domain.rs`, and `tests/errors.rs` exercise only the curated public facades.
The CLI facade compile-fail doctest proves that terminal-capability snapshots cannot be imported
publicly, while positive public-API tests compile and execute completion paths using
`run(cli, stdout, stderr)` and the runtime fallback signatures. Private dispatch tests inject only
synthetic terminal state, logging initialization, and fake command execution.

`tests/process.rs` launches the compiled binary only for service-free paths such as help, version,
argument rejection, completion generation, and manpage rendering. A process test
must fail before keyring or network access unless access is the explicitly isolated contract under
test.

Verbose precondition coverage injects a recording/failing logging initializer into the private
dispatch seam. Complete state equality proves completion generation and
noninteractive login make zero initializer and executor calls. Delegated fake
commands, including `accept` and `decline`, prove initialization occurs first and that an
initialization error prevents execution.
These tests never install the real process-global subscriber.

Some platform, interactive-terminal, global-subscriber, and historical read-contract coverage is
deliberately retained outside the routine suite. Their exact status and replacement criteria are
documented in [retained test contracts](retained-test-contracts.md). An ignored test is a manual
contract marker, not an invitation or a contributor verification step. Do not execute it directly
or by enabling ignored tests.

Evidence-sensitive model questions are tracked in
[evidence-gated follow-ups](evidence-gated-follow-ups.md). Tests must preserve those current
contracts; they must not probe the live service to settle them.

# Architecture

This document describes the implemented 0.0.1 architecture. It is a hybrid of feature-first
organization and hexagonal boundaries: product concepts live in vertical feature slices, while
side effects are reached through feature-owned ports and private adapters.

## Repository shape

```text
src/
  lib.rs
  main.rs                       # thin `venmo` process entry point
  cli/mod.rs                    # curated public terminal facade
  model.rs                      # curated public frontend-neutral model facade
  shared/                       # cross-feature values and capabilities
  features/
    activity/
    auth/
    payments/
    people/
    requests/
    transfers/
    wallet/
  adapters/
    cli/                        # Clap, dispatch, prompts, tables, and terminal errors
    credentials/                # credential codec and native keyring implementation
    system/                     # wall clock, request IDs, and build information
    venmo/
      client/                   # feature-port implementations and response mapping
      dto/                      # private wire representations
      transport/                # fixed-origin HTTP transport
```

`features`, `shared`, and `adapters` are crate-private. The library exposes only the curated
[`cli`](../src/cli/mod.rs) and [`model`](../src/model.rs) modules. Public re-exports do not make
their defining modules part of the supported surface. See the [public facade inventory](public-api.md).

## Dependency direction

In the following list, `A -> B` means that `A` may depend on `B`:

```text
main -> cli facade -> adapters::cli
model facade -> selected feature/shared models (re-export only)
adapters::cli composition -> features + credential/system/Venmo adapters
credential/system/Venmo adapters -> feature ports/models + shared
features -> shared
features -> explicitly owned APIs of lower-level features
shared -> standard library and general-purpose crates
```

The important hexagonal inversion is that a feature owns the port it consumes and a service
adapter implements that port. A feature never imports `adapters`, `cli`, `clap`, `reqwest`,
`keyring`, a Venmo DTO, or terminal rendering. `shared` never imports a feature or adapter. The
public facades are outward-facing allowlists, not modules for internal code to depend on.

Cross-feature dependencies must remain explicit and acyclic. For example, activity owns activity
records but uses the people-owned `User`; payments composes auth, people, and wallet capabilities;
requests composes auth, payments, people, and wallet capabilities; transfers composes auth,
payments confirmation/account validation, and wallet balance capabilities. Move a concept to `shared` only
when its identity and invariant are genuinely shared. Do not move a type merely to avoid choosing
an owner.

## Ownership rules

| Area | Owns | Must not own |
| --- | --- | --- |
| `shared` | Cross-feature validated primitives, credential capability traits, common failure categories, `Clock`, and `ClientRequestIdGenerator` | Command workflows, endpoint DTOs, rendering, or feature-specific policy |
| `features/<name>` | Models and invariants for that feature, use cases, errors, and the narrow ports those use cases consume | Concrete keyring/HTTP/UI technology or public protocol formatting |
| `adapters/venmo` | The private API wire contract, DTO decoding/encoding, fixed-origin transport, response bounds, and feature-port implementations | Product orchestration or terminal/MCP presentation |
| `adapters/credentials` | Versioned credential serialization and the sole native keyring implementation | Authentication workflow policy or file fallback |
| `adapters/cli` | Argument grammar, production composition, prompt capability, interruption handling, rendering, and CLI error/exit mapping | Venmo business invariants or a second API implementation |
| `cli` and `model` | Deliberate external re-exports | New implementation logic or blanket exposure of internals |

Put a port beside its consumer, normally in `features/<name>/ports.rs`. The shared credential
reader/writer/deleter traits are the deliberate exception because several features consume those
independent capabilities. Authentication keeps `CurrentAccountApi` and `PasswordLoginApi`
separate; possessing one capability must not imply possessing the other.

## One Venmo client and transport

There is one production private-API stack:

- `adapters::venmo::VenmoApiClient<T>` implements the feature-owned API traits. Endpoint modules
  split implementation and mapping work; they are not separate clients.
- Its production transport is `VenmoHttpTransport`, which owns fixed-origin URL construction,
  headers, deadlines, bounded responses, and the no-retry HTTP client configuration.
- DTOs remain private to `adapters::venmo::dto` and are mapped to feature models before crossing a
  port.
- Tests may parameterize the same client with `ScriptedTransport` or use its loopback transport
  constructor. Those are test seams, not alternate production clients.

“One client” means one implementation path and one wire contract, not a global singleton. A
composition root may construct or privately wrap the client for the lifetime appropriate to its
frontend. No feature or frontend may add its own HTTP client, duplicate endpoint DTOs, or parse
another frontend's rendered output.

## Dependency injection boundaries

Use cases use generic, statically dispatched capabilities and request only what they need. The
current injected boundaries are:

- `CredentialReader`, `CredentialWriter`, and `CredentialDeleter`;
- feature-owned API traits, including the independently scoped authentication and financial-write
  traits;
- `AuthenticationInput`, `PromptAvailability`, and `DefaultNoConfirmation`;
- `Clock` for persisted UTC save time and `ClientRequestIdGenerator` for externally significant
  request IDs;
- CLI output through caller-provided `Write` values, a captured system-time-zone formatter for
  timestamp-bearing output, and financial interruption through an injected future factory; and
- crate-private CLI unit seams that inject terminal snapshots, a logging initializer, and a fake
  command executor.

This makes service access, nondeterminism, user authorization, and externally visible I/O
replaceable in tests. Fakes should implement only the capabilities under test; that is also a
compile-time capability check. Production wiring stays in the frontend composition adapter rather
than in a global service locator.

Prompt availability is owned by the production boundary. The public `cli::run` and
runtime-initialization fallback inspect the actual process streams rather than accepting a caller
supplied terminal snapshot. Synthetic terminal state exists only behind crate-private test seams.
The dispatcher resolves noninteractive authentication before invoking the delegated production
executor; that executor owns verbose logging initialization and service
composition. The runtime itself is the one process bootstrap attempted before those asynchronous
preconditions.

Do not introduce an interface merely because a library function is called:

- **Serialization is not abstracted.** `serde`/`serde_json` are deterministic implementation
  details of credential and Venmo wire adapters. Tests assert their exact compatibility and wire
  shapes directly. A serializer port would hide rather than isolate those contracts. A future
  protocol frontend should own explicit protocol DTOs instead of adding serialization derives to
  feature models.
- **Table formatting is not abstracted.** `tabled` is private to the CLI adapter and is tested as a
  pure renderer. Injecting the destination `Write` captures the meaningful I/O boundary.
- **Time-zone conversion stays in the CLI adapter.** Persisted and wire timestamps remain exact
  `time::OffsetDateTime` instants. Production composition asks `jiff` for the system-configured
  zone once for each timestamp-bearing command and explicitly falls back to UTC; render tests inject
  deterministic zones to prove daylight-saving conversion and exact output without changing feature
  models.
- **The filesystem is not abstracted.** Production code has no project-owned file repository:
  persistence goes through native credential capabilities, and generated text goes to a supplied
  writer. A speculative filesystem trait would have no feature contract to represent.
- **`Instant` is not abstracted.** The transport uses monotonic elapsed time only for redacted
  diagnostics; it does not decide a feature outcome. HTTP timeout behavior belongs to the reviewed
  transport library. In contrast, UTC time is persisted and therefore uses the injected `Clock`;
  converting an already typed instant for display is a separate CLI concern.

Add a new seam only when it separates a real side effect, nondeterministic product decision, or
capability boundary and when a test needs to control the behavior on the other side.

## Adding a future MCP frontend

A future MCP implementation belongs inside the same library crate so it can compose private
features directly. Follow the CLI pattern:

1. Keep protocol handlers, DTOs, schemas, and error mapping in a private MCP adapter.
2. Call the existing feature use cases through their narrow ports. Do not parse CLI text or copy
   feature orchestration.
3. Back production reads with a private read-only facade around the same `VenmoApiClient` and
   `VenmoHttpTransport`. The facade restricts capabilities; it is not another client.
4. Give server state only `CredentialReader` and required read API capabilities. Do not expose
   credential writing/deletion, password login/device trust, adapter modules, or raw DTOs.
5. If a separate binary needs library entry points, add a small curated public `mcp` facade for
   process startup/protocol types, analogous to `cli`; do not make `features` or `adapters` public.
6. Keep MCP-specific serialization in that frontend and return structured views mapped from the
   same feature results exposed through `model` where appropriate.

The detailed MCP capability and release gates remain in [`PLAN.md`](../PLAN.md). They are design
constraints, not permission to expose or execute financial tools.

## Mutation and imperative safety sequences

Follow the repository's [mutation policy](../CONTRIBUTING.md#mutation-policy). In particular,
security-sensitive ownership transfer/zeroization, bounded accumulation, duplicate detection,
writer or stream advancement, and the ordered financial preflight/authorization/single-write path
may remain visibly imperative. Do not conceal those sequences behind abstractions that weaken
bounds or make side-effect order harder to review.

## Related documentation

- [Testing strategy](testing.md)
- [Retained test contracts](retained-test-contracts.md)
- [Evidence-gated follow-ups](evidence-gated-follow-ups.md)
- [Contributing](../CONTRIBUTING.md)

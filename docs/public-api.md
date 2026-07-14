# Public facade inventory and 0.1 compatibility

The library target exists to support the shipped frontend and future in-repository frontends. It is
not a general Venmo SDK. [`src/lib.rs`](../src/lib.rs) exposes exactly two curated modules:
`venmo_cli::cli` and `venmo_cli::model`.

This inventory is a review aid for 0.1 releases. The Rust source and generated rustdoc remain the
signature-level source of truth.

## `venmo_cli::cli`

The terminal facade exposes the Clap schema and the narrow process surface needed by `src/main.rs`
and black-box tests.

### Argument schema

- Root: `Cli`, `Command`.
- Authentication: `AuthArgs`, `AuthOperation`, `LoginArgs`, `LogoutArgs`.
- Friends: `FriendsArgs`, `FriendsOperation`, `FriendsListArgs`.
- Users: `UsersArgs`, `UsersOperation`, `UserSearchArgs`.
- Wallet reads: `PaymentMethodsArgs`, `PaymentMethodsOperation`.
- Activity: `ActivityArgs`, `ActivityOperation`, `ActivityListArgs`, `ActivityShowArgs`.
- Requests: `RequestsArgs`, `RequestsOperation`, `RequestsListArgs`, `RequestDirectionArg`.
- Financial command inputs: `PayArgs`, `RequestArgs`, `AcceptArgs`, `DeclineArgs`.
- Completion generation: `CompletionsArgs`, `CompletionShell`.

### Process/runtime surface

- Types: `AppError`, `ErrorCategory`.
- Functions: `run`, `handle_runtime_initialization_failure`, and `write_error`.

`run(cli, stdout, stderr)` is the only asynchronous production dispatcher. It always applies the
closed production gates for candidate `accept` and `decline`, reads prompt capability from the
actual process stdin/stderr, and initializes verbose diagnostics only after service-free
preconditions delegate a command to the production executor. Callers can supply output writers,
but cannot supply release policy or synthetic terminal state.

`handle_runtime_initialization_failure(cli, stdout, stderr, source)` is the synchronous fallback
used when the process runtime cannot be built. It owns the same closed gates and process terminal
check. Completion generation, unavailable candidates, and noninteractive authentication return
before logging initialization; local-only logout still deletes locally, and revoking logout still
attempts local deletion while reporting that remote revocation was not attempted.

The concrete CLI adapter, production provider, prompts, renderers, and dispatch helpers remain
private even though selected items are re-exported through this facade.

## `venmo_cli::model`

The model facade contains frontend-neutral validated values and completed-operation results. It
does not expose service ports, credentials, HTTP DTOs, or a client.

### Shared values

- `Account`, `ClientRequestId`.
- `Money`, `MoneyParseError`, `Note`, `NoteParseError`.
- `UserId`, `UserIdParseError`, `Username`, `UsernameParseError`.
- `Limit`, `LimitParseError`, `Offset`, `OffsetParseError`, `DEFAULT_LIST_LIMIT`,
  `MAX_LIST_LIMIT`.
- `ContinuationTokenParseError`, `MAX_CONTINUATION_TOKEN_BYTES`, `OpaqueIdParseError`.

### People

- `User`, `UserProfileKind`, `UserSearchQuery`, `UserSearchQueryParseError`.
- `RecipientInput`, `RecipientParseError`.
- `FriendsResult`, `UserSearchResult`.

### Wallet

- `Balance`, `SignedUsdAmount`, `SignedUsdAmountParseError`, `BalanceResult`.
- `PaymentMethod`, `PaymentMethodId`, `PaymentMethodsResult`.

### Activity

- `Activity`, `ActivityId`, `ActivityAction`, `ActivityCounterparty`, `ActivityDirection`,
  `ActivityStatus`, `ActivityLabelParseError`.
- `ActivityBeforeId`, `ActivityListResult`, `ActivityShowResult`.

### Payments

- `PaymentId`, `CreatedPayment`, `FinancialStatus`.
- `PeerFundingMethod`, `PeerFundingRole`, `PeerFundingFee`.
- `PayPlan`, `PayResult`.

### Requests

- `RequestId`, `RequestAction`, `RequestDirection`, `RequestDirectionFilter`, `RequestStatus`,
  `RequestStatusParseError`, `RequestRecord`, `RequestsBefore`.
- `CreatedRequest`, `AcceptedRequest`, `DeclinedRequest`.
- `CreateRequestPlan`, `AcceptRequestPlan`, `DeclineRequestPlan`.
- `RequestCreateResult`, `AcceptResult`, `DeclineResult`, `RequestsResult`.

## Intentionally private

The following are implementation details, even when their defining item needs Rust `pub`
visibility for an internal re-export:

- all `features`, `shared`, and `adapters` module paths;
- feature use-case functions, feature errors, and API/prompt/credential port traits;
- `VenmoApiClient`, `VenmoHttpTransport`, transport request/response types, and every Venmo DTO;
- native keyring and credential-codec types;
- CLI composition providers, renderer functions, prompt implementations, and test transports;
- release-gate values, terminal-capability snapshots, logging initialization, and the internal
  `run_with`/runtime-fallback test seams; and
- secret-bearing credential/authentication values.

Do not widen visibility to help an external caller build another frontend. An in-repository
frontend composes crate-private features and publishes its own minimal facade as described in the
[architecture](architecture.md#adding-a-future-mcp-frontend).

## Semver expectations for 0.1

The crate is pre-1.0, but version numbers still communicate intent:

- `0.1.x` patch releases should preserve source compatibility for the `cli` and `model` facade
  paths and signatures. Public field/type changes, removed trait implementations, and enum variant
  changes require the same compatibility review as other public API changes.
- An intentional incompatible facade change requires at least `0.2.0` and a documented migration.
  Pre-1.0 minor releases may make such changes; callers must not assume 1.0-level stability.
- Crate-private modules may change in any release and carry no compatibility promise. Their names
  in architecture documentation describe ownership, not a supported import path.
- Adding a future curated facade is reviewed separately. It does not justify exposing adapters or
  treating private ports as an external SDK.

The executable's command and safety contracts are governed by [`README.md`](../README.md) and
[`PLAN.md`](../PLAN.md). Rust facade compatibility does not convert unsupported private Venmo wire
behavior into a stability guarantee.

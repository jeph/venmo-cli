# Public facade inventory and compatibility

The library target exists to support the shipped frontend and future in-repository frontends. It is
not a general Venmo SDK. [`src/lib.rs`](../src/lib.rs) exposes exactly two curated modules:
`venmo_cli::cli` and `venmo_cli::model`.

This inventory is a review aid for pre-1.0 releases. The Rust source and generated rustdoc remain the
signature-level source of truth.

## `venmo_cli::cli`

The terminal facade exposes the Clap schema and the narrow process surface needed by `src/main.rs`
and black-box tests.

### Argument schema

- Root: `Cli`, `Command`.
- Authentication: `AuthArgs`, `AuthOperation`.
- Friends: `FriendsArgs`, `FriendsOperation`, `FriendsListArgs`, `FriendAddArgs`,
  `FriendRemoveArgs`.
- Users: `UsersArgs`, `UsersOperation`, `UserSearchArgs`, `UserInfoArgs`.
- Activity: `ActivityArgs`, `ActivityOperation`, `ActivityListArgs`, `ActivityInfoArgs`.
- Requests: `RequestsArgs`, `RequestsOperation`, `RequestsListArgs`, `RequestInfoArgs`,
  `RequestArgs`, `AcceptArgs`, `DeclineArgs`, `RequestDirectionArg`.
- Transfers: `TransferArgs`, `TransferOperation`, `TransferOutArgs`, `TransferAmountArg`,
  `TransferSpeedArg`.
- Payment commands: `PayArgs`, `PayOperation`, `PayUserArgs`, `VisibilityArg`.

### Process/runtime surface

- Types: `AppError`, `ErrorCategory`.
- Functions: `run`, `handle_runtime_initialization_failure`, and `write_error`.

`run(cli, stdout, stderr)` is the only asynchronous production dispatcher. It reads prompt
capability from the actual process stdin/stderr and initializes global debug diagnostics only after
service-free preconditions delegate a command to the production executor. Callers can supply
output writers, but cannot supply synthetic terminal state. Every current and future command passes
through this central boundary; no command-specific debug initialization is exposed publicly.

`handle_runtime_initialization_failure(cli, stdout, stderr, source)` is the synchronous fallback
used when the process runtime cannot be built. It owns the same process terminal check.
Noninteractive authentication returns before logging initialization; local-only logout still
deletes the keyring entry without a runtime.

The concrete CLI adapter, production provider, prompts, renderers, and dispatch helpers remain
private even though selected items are re-exported through this facade.

## `venmo_cli::model`

The model facade contains frontend-neutral validated values and completed-operation results. It
does not expose service ports, credentials, HTTP DTOs, or a client.

### Shared values

- `Account`, `ClientRequestId`.
- `Money`, `MoneyParseError`, `Note`, `NoteParseError`.
- `UserId`, `UserIdParseError`, `Username`, `UsernameParseError`.
- `Visibility`, `VisibilityParseError`.
- `Limit`, `LimitParseError`, `Offset`, `OffsetParseError`, `DEFAULT_LIST_LIMIT`,
  `MAX_LIST_LIMIT`.
- `ContinuationTokenParseError`, `MAX_CONTINUATION_TOKEN_BYTES`, `OpaqueIdParseError`.

### People

- `User`, `UserProfileKind`, `FriendshipStatus`, `UserSearchQuery`,
  `UserSearchQueryParseError`.
- `RecipientInput`, `RecipientParseError`.
- `FriendsResult`, `UserSearchResult`, `UserInfoResult`.

### Wallet

- `Balance`, `SignedUsdAmount`, `SignedUsdAmountParseError`, `BalanceResult`.
- `PaymentMethod`, `PaymentMethodId`, `PaymentMethodsResult`.

### Activity

- `Activity`, `ActivityId`, `ActivityAction`, `ActivityCounterparty`, `ActivityDirection`,
  `ActivityStatus`, `ActivityLabelParseError`.
- `ActivityDetail`, `ActivityDetailParties`, `ActivityFeedKind`, `ActivitySubject`.
- `ActivityBeforeId`, `ActivityListResult`, `ActivityInfoResult`. Other-user list results expose the
  resolved subject; current-user results retain the existing unscoped presentation. Activity and
  detail amounts are optional because server-visible external social stories can omit them;
  participant-owned records retain the required exact-money contract.

### Payments

- `PaymentId`, `CreatedPayment`, `FinancialStatus`.
- `PeerFundingMethod`, `PeerFundingRole`, `PeerFundingFee`.
- `PayPlan`, `PayResult`.

### Requests

- `RequestId`, `RequestAction`, `RequestDirection`, `RequestDirectionFilter`, `RequestStatus`,
  `RequestStatusParseError`, `RequestRecord`, `RequestsBefore`.
- `CreatedRequest`, `AcceptedRequest`, `DeclinedRequest`.
- `CreateRequestPlan`, `AcceptRequestPlan`, `DeclineRequestPlan`.
- `AcceptRequestPlan` exposes the internally selected optional backup method but not its secret
  eligibility token or fee tokens. It exposes whether Purchase Protection was selected and, only
  then, the checked aggregate seller-fee estimate and recipient-proceeds estimate for frontend
  rendering. `AcceptedRequest` payment ID/status are optional because the modern
  source-funded approval success object does not guarantee either field.
- `RequestCreateResult`, `AcceptResult`, `DeclineResult`, `RequestsResult`, `RequestInfoResult`.

### Transfers

- `TransferId`, `TransferInstrumentId`, `TransferInstrumentSuffix`,
  `TransferInstrumentSuffixParseError`, `TransferSpeed`.
- `TransferInstrument`, `TransferFeeMetadata`, `TransferModeOptions`, `TransferOptions`,
  `TransferOutAmount`.
- `TransferOutPlan`, `CreatedTransfer`, `TransferOptionsResult`, `TransferOutResult`.

The public model types describe the frontend-neutral options and validated standard-out states.
They do not expose the private adapter as a supported SDK or bypass CLI safety policy.

## Intentionally private

The following are implementation details, even when their defining item needs Rust `pub`
visibility for an internal re-export:

- all `features`, `shared`, and `adapters` module paths;
- feature use-case functions, feature errors, and API/prompt/credential port traits;
- `VenmoApiClient`, `VenmoHttpTransport`, transport request/response types, and every Venmo DTO;
- native keyring and credential-codec types;
- CLI composition providers, renderer functions, prompt implementations, and test transports;
- terminal-capability snapshots, logging initialization, and the internal `run_with`/
  runtime-fallback test seams; and
- secret-bearing credential/authentication values.

Do not widen visibility to help an external caller build another frontend. An in-repository
frontend composes crate-private features and publishes its own minimal facade as described in the
[architecture](architecture.md#adding-a-future-mcp-frontend).

## 0.1 to 0.2 migration

Version 0.2 intentionally standardizes resource detail commands on `info` and expands the public
Clap schema:

- Replace `venmo activity show <ACTIVITY_ID>` with `venmo activity info <ACTIVITY_ID>`; there is no
  compatibility alias.
- Replace `ActivityOperation::Show`, `ActivityShowArgs`, and `ActivityShowResult` with
  `ActivityOperation::Info`, `ActivityInfoArgs`, and `ActivityInfoResult`.
- Handle the new `UsersOperation::Info(UserInfoArgs)` and
  `RequestsOperation::Info(RequestInfoArgs)` variants in exhaustive matches.
- Request actions are consolidated under `Command::Requests`: exhaustive `RequestsOperation`
  matches must also handle `Create(RequestArgs)`, `Accept(AcceptArgs)`, and
  `Decline(DeclineArgs)`. The former top-level `Command::Request`, `Command::Accept`, and
  `Command::Decline` variants and command forms have no compatibility aliases.
- `UserInfoResult` and `RequestInfoResult` are now available from `venmo_cli::model`.
- `UserInfoArgs` exposes `username: Username` rather than a user-ID field. `Username` and
  `RecipientInput` normalize usernames with or without `@`; no command argument exposes `UserId` as
  an alternate user selector. `UserSearchQuery` likewise normalizes single-token optional-`@`
  inputs as username searches; multi-word values remain general searches.

`PayUserArgs` and `RequestArgs` expose a public `visibility: VisibilityArg` field. Callers
constructing either argument struct directly must select a value, normally
`VisibilityArg::Private` to preserve the CLI default. `PayPlan` and `CreateRequestPlan` expose the
corresponding requested, frontend-neutral `Visibility` through `visibility()`; it does not claim
the eventual audience Venmo applies after participant privacy settings.

`PayUserArgs` and `AcceptArgs` expose a public `source: Option<PaymentMethodId>` field. `None`
preserves automatic balance/default-or-sole-external selection; `Some` requests one exact
peer-eligible balance, bank, or card source. `PayOperation::Options` is the read-only payment-option
leaf that supplies copyable candidate IDs. The former `PayOperation::Methods` variant and
`venmo pay methods` spelling are intentionally not retained as aliases.

## Initial 0.0.1 surface

Version 0.0.1 exposes grouped `PayOperation::Options` and `PayOperation::User`, matching
`TransferOperation::Options` plus guarded standard-bank `TransferOperation::Out` with typed
exact/all amount selection.
It intentionally does not expose
inbound transfers. Library callers cannot invoke private adapter internals or bypass preflight,
confirmation, ambiguity, and no-retry policy.

## Semver expectations

The crate is pre-1.0, but version numbers still communicate intent:

- Patch releases should preserve source compatibility for the `cli` and `model` facade
  paths and signatures. Public field/type changes, removed trait implementations, and enum variant
  changes require the same compatibility review as other public API changes.
- An intentional incompatible facade change requires at least the next minor version and a documented migration.
  Pre-1.0 minor releases may make such changes; callers must not assume 1.0-level stability.
- Crate-private modules may change in any release and carry no compatibility promise. Their names
  in architecture documentation describe ownership, not a supported import path.
- Adding a future curated facade is reviewed separately. It does not justify exposing adapters or
  treating private ports as an external SDK.

The executable's command and safety contracts are governed by [`README.md`](../README.md) and
[`PLAN.md`](../PLAN.md). Rust facade compatibility does not convert unsupported private Venmo wire
behavior into a stability guarantee.

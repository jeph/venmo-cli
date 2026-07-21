# Evidence-gated model follow-ups

Phase 6 deliberately leaves the following contracts unchanged. They affect accepted input,
recipient identity, or financial-policy evidence, so a naming or type cleanup alone is not enough
to change them safely.

This is a decision record, not a contributor live-testing queue. Routine work must preserve these
contracts and must not probe Venmo, run an ignored test, or invoke a financial command to gather
evidence. Follow [`CONTRIBUTING.md`](../CONTRIBUTING.md) and the
[testing strategy](testing.md); only separately governed evidence can reopen an item.

## User ID leading zeroes

`UserId` continues to accept a positive decimal identifier with leading zeroes and preserves the
original spelling. Changing normalization or equality requires sanitized API evidence showing
whether Venmo treats `1` and `0001` as the same identifier across lookup and write responses.

## Username grammar and case folding

The existing username grammar and exact-recipient case-insensitive comparison remain unchanged.
A stricter grammar, Unicode normalization, or a different case-folding rule requires documented
service behavior and sanitized fixtures for accepted usernames and ambiguous search results.
Command inputs normalize an optional leading `@`; this frontend spelling rule does not change the
underlying bare-username grammar.

## Note limits

No undocumented product-length limit is added to `Note`. The current nonblank and safety-bound
rules remain in force until official documentation or controlled, non-financial API evidence
establishes a service limit and its unit (bytes, Unicode scalar values, or grapheme clusters).

## Payment-role `contains("default")`

Read-only payment-method mapping retains its existing `contains("default")` interpretation where
that compatibility behavior is already used. Tightening it to an exact role requires sanitized
response evidence for every supported role spelling so valid documented methods are not hidden.
The stricter peer-funding path continues to use its separately verified exact role contract.

## Method-bound eligibility

The eligibility request intentionally sends an empty `funding_source_id`. Its reported fee describes
the candidate transaction, but it cannot prove the fee for the internally chosen method.
That method therefore remains `Unknown` when its own response lacks fee evidence; it is not
upgraded to `ProvenZero`. Method-level fee evidence does not gate selection: zero-fee, nonzero-fee,
and unknown-fee peer methods may all be submitted under the automatic policy. That policy validates
duplicate IDs and multiple defaults, chooses the unique default regardless of fee, otherwise chooses
only a sole eligible method, and fails closed when multiple non-default methods remain; it never uses
response order or fee as a tiebreaker. Payment details report only the selected source and its own
method-level fee evidence. They do not present the blank-source eligibility amount as a payer fee or
total because its application is not proven. Binding the eligibility proof to a method requires a
controlled capture showing the request field, token, response, and resulting payment all refer to
that exact method.

## Protected peer-payment validation

Signer-verified Venmo Android 26.13.0 establishes the implemented source-bound form eligibility
request, exact buyer-protection product-URI prefix, complete singleton fee forwarding, request
`transaction_type: goods_services_protected`, quasi-cash metadata, response
`type: payment_protected`, and preservation through the same-UUID SMS continuation. Current
official guidance establishes that the seller fee is deducted from proceeds and that tagging does
not guarantee item coverage. Exact service-free tests pin those boundaries. One owner-authorized,
non-retried $1 client-1 validation on 2026-07-20 received eligibility 201 and payment 200 without
OTP and reconciled to exactly one settled matching payment. A bounded read-only payment-detail
probe confirmed `type: payment_protected`; the first validator had incorrectly required the
request-side value in the response and conservatively reported ambiguity, so no second payment was
attempted. This proves live client-1 authorization and the protected response marker, but not the
actual debit source, final seller fee, tax treatment, or item eligibility.

## Exact-recipient acceptance at the exhaustive bound

Exact-recipient resolution keeps the existing four-page/200-record exhaustive safety bound. At
that bound, one already-found exact username match remains acceptable only after detail-by-ID
returns the same immutable ID and case-insensitive exact username; no match is reported as username
not found and multiple matches remain ambiguous. Changing that boundary rule in either direction requires
evidence about server-side username uniqueness and whether detail-by-ID independently proves the
absence of another exact match beyond the bounded search.

## Request-acceptance funding and fee evidence

Signer-verified Android 10.31.1 and 26.13.0 establish unacknowledged notification lookup,
association of a request notification's `payment.id` with that request's own action ID, source-bound
eligibility, and explicit funding options on `PUT /v1/requests/{request-action-id}`. Current
production wraps some request notifications at `additional_properties.request`; a separately
approved bounded client-1 read confirmed distinct outer notification, nested request-action, and
nested payment IDs without retaining values. Native code includes applied fees only when Purchase Protection
is selected. The CLI uses this route for every acceptance, only with its deterministic
automatic/explicit peer-source policy and a valid
eligibility token. Unprotected approval
omits returned fees. Protected approval strictly validates and submits normalized fee objects and
shows the eligibility amount as an estimated seller deduction with estimated recipient proceeds.
An outer-wrapper-ID write returned HTTP 404 and reconciled as no mutation. A subsequent owner-run
unprotected source-funded approval using the corrected nested request-action ID returned 200,
removed the pending request, and produced exactly one matching settled $20 activity, proving current
client-1 authorization for that F4S branch. Residual gaps
remain: the successful object does not prove the actual debit source or final fee beyond the
eligibility response; explicit source selection and protected approval have not received separate
live validation. Signer-verified Android 26.13.0 establishes the acceptance SMS-step-up contract;
the CLI implements its exact HTTP-403/root-title detector, server challenge UUID, shared hidden
prompt, and one verified continuation. That continuation has not been validated live. Payment
creation's separately evidenced P2P SMS continuation is implemented with the same UUID and one verified submission, and
one explicit-bank-source live payment completed and reconciled through that flow. The response and
activity do not prove the actual debit source or final fee. No output may claim either, and no
request-acceptance continuation may be retried.

## Outgoing request cancellation

`requests cancel` implements the narrow outgoing-charge contract independently from incoming
decline and completed-payment cancellation. Signer-verified Venmo Android 26.13.0 calls
form-urlencoded `PUT /v1/payments/{id}` with `action=cancel`, returns a
`BaseSingleObjectResponse<Payment>`, and obtains cancellable outgoing charges with
`status=pending,held`, `actor=self`, and `action=charge`. Historical `deet/govenmo` independently
uses the same form action and parses returned payment data. The maintained Python fork uses the
same endpoint/action with JSON, so current native form encoding controls.

The CLI requires an authoritative self→counterparty open charge, shows default-No confirmation,
sends one write, and requires exact terminal `cancelled` proof. It never treats this as reversal of
a completed payment, incoming denial, friend-request cancellation, or reminder. Exact synthetic
coverage proves the request and response contract, but current client-1 authorization has not yet
received a controlled live cancellation. Any live canary requires separate owner approval and an
identified harmless outgoing request; routine tests must remain service-free.

## Friendship mutation session authorization

`friends add` and `friends remove` retain exact synthetic contracts derived from signer-verified
Venmo Android 9.26.0, 10.31.1, and 26.13.0. The artifacts agree on form-urlencoded
`POST /v1/friend-requests` for send/accept, bodyless
`DELETE /v1/users/{self}/friends/{target}` for unfriend/outgoing-request cancellation, and the four
closed relationship states. Public-source research found friend listing but no independent mutation
implementation.

Every inspected Android generation authenticates as OAuth client 4, while this CLI intentionally
retains its historical client-1 credential profile. Exact request construction and response
reconciliation therefore do not prove current server authorization. Keep post-transmission
uncertainty ambiguous, never retry a write, and do not add an ignored live test. A controlled
add-then-revoke canary requires separate owner approval, a consenting target initially proven
`not_friend`, and separate reconciliation/authorization before the revoke. Never remove an
established friendship merely to broaden evidence.

## Non-personal activity profiles

`activity list --user` initially supports authoritative personal profiles only. Signer-verified
Android uses the same base story path for business/public profiles but a distinct
`only_public_stories=true` method and presenter path. That separate filter contract, profile
taxonomy, continuation behavior, and output semantics are not inferred from personal feeds.
Business, charity, unknown, and missing profile types therefore fail before AC1. Reopening this
scope requires exact synthetic and static evidence for each supported profile class; a generic
personal-feed fallback is not acceptable.

## Peer-payment list and detail reads

`payments list` and `payments info <PAYMENT_ID>` remain unavailable. The only current list contract
proves the request-specific query `GET /payments?action=charge&status=pending,held&limit=N`; it does
not prove a general peer-payment collection, its continuation behavior, or a filter that includes
direct payments and accepted-request payments while excluding open and declined requests.

Detail support is independently blocked. A pending RequestId has succeeded through
`GET /payments/{id}`, but dated evidence records HTTP 500 for a settled PaymentId nested in an
otherwise readable activity story. ActivityId and PaymentId are distinct, so the story-detail route
cannot implement payment detail.

Reopening payment reads requires a separately governed, bounded, read-only contract dossier that
proves both incoming and outgoing direct payments, both observed accepted-request representations,
declined/open-request exclusion, payer/payee money direction, source-page bounds and continuation,
and exact-ID detail behavior for direct and accepted PaymentIds. Routine development must not use
the production credential or extend ignored probes to gather this evidence. List and detail are
approved independently; failure to prove detail must not block an otherwise proven list command.

## Transfer creation

`transfer options` implements the controlled current `GET /v1/transfers/options` read and exposes
only direction/speed-specific cash-out destination branches. Enabled standard out uses only exact
`bank` destinations, identical integer-cent `amount`/`final_amount`, fail-closed automatic
selection, default-No confirmation, and one non-retried `POST /v1/transfers`.
Exact lowercase `all` is a client-side shorthand over the already required fresh available-balance
read: it resolves once to positive available cents, excludes on-hold funds, and changes no wire
contract. It therefore requires synthetic regression coverage, not another live canary.

One separately approved $0.01 canary on 2026-07-17 returned HTTP 201 direct pending transfer data
and reconciled to exactly one outgoing activity record with the same transfer ID. Production
success therefore requires the pinned ID/timestamp/status/type/dollar-amount/destination shape and
additionally enforces requested/net/fee arithmetic; empty, status-only, non-201, mismatch, and
transport uncertainty remain ambiguous. Signer-verified Android 26.13.0 supplies operation-bound
descriptions for selected standard-cash-out error codes and the exact SMS-continuation title. The
CLI may use fixed descriptions to improve recovery guidance, but code-only mappings remain
ambiguous without an independently proven rejection dossier. Retain no live mutation test or raw
response, and do not repeat the canary merely to broaden evidence.

Transfer-in/add-funds is intentionally unsupported and is not an evidence-gated implementation
task. Settlement completion, independently confirmed rejection codes, instant cash-out, debit cash-out, T1 fee
units, step-up, cancellation, and expedition remain independent evidence gates. Routine work must
not probe or mutate production to resolve them.

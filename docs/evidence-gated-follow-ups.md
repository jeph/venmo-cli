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
response order or fee as a tiebreaker and exposes no user override. Payment details report the method
evidence and eligibility fee separately and warns that the final fee may differ. Binding the proof to a method
requires a controlled capture showing the request field, token, response, and resulting payment all
refer to that exact method.

## Exact-recipient acceptance at the exhaustive bound

Exact-recipient resolution keeps the existing four-page/200-record exhaustive safety bound. At
that bound, one already-found exact username match remains acceptable only after detail-by-ID
returns the same immutable ID and case-insensitive exact username; no match is reported as username
not found and multiple matches remain ambiguous. Changing that boundary rule in either direction requires
evidence about server-side username uniqueness and whether detail-by-ID independently proves the
absence of another exact match beyond the bounded search.

## Request-acceptance funding and fee evidence

Request acceptance retains a residual evidence gap. Validation requires an available Venmo balance
snapshot covering the full request, and the approval submits no external funding-method
identifier. The mutation does not bind that
snapshot, however, and its response proves neither the final funding source nor the fee. Output
must disclose that limitation and must not claim wallet funding or a `$0.00` fee. Changing those
claims requires transaction-bound source and fee evidence from the acceptance operation itself.

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
additionally enforces requested/net/fee arithmetic; empty, status-only, non-201, challenge, mismatch, and transport
uncertainty remain ambiguous. Retain no live mutation test or raw response, and do not repeat the
canary merely to broaden evidence.

Transfer-in/add-funds is intentionally unsupported and is not an evidence-gated implementation
task. Settlement completion, confirmed rejection codes, instant cash-out, debit cash-out, T1 fee
units, step-up, cancellation, and expedition remain independent evidence gates. Routine work must
not probe or mutate production to resolve them.

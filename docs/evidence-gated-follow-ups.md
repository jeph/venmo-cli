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

The eligibility request intentionally sends an empty `funding_source_id`. Its zero-fee result can
prove the overall candidate transaction fee, but it cannot prove that a selected default or
override method has a zero method-level fee. The selected method therefore remains `Unknown` when
its own response lacks fee evidence; it is not upgraded to `ProvenZero`. Binding the proof to a
method requires a controlled capture showing the request field, token, response, and resulting
payment all refer to that exact method.

## Exact-recipient acceptance at the exhaustive bound

Exact-recipient resolution keeps the existing four-page/200-record exhaustive safety bound. At
that bound, one already-found exact username match remains acceptable only after detail-by-ID
returns the same immutable ID and case-insensitive exact username; no match remains incomplete and
multiple matches remain ambiguous. Changing that boundary rule in either direction requires
evidence about server-side username uniqueness and whether detail-by-ID independently proves the
absence of another exact match beyond the bounded search.

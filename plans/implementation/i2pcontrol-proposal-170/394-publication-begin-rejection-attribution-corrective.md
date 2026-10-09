# Plan 394 — publication begin rejection attribution corrective

Status: **blocked-encrypted-publication-record-rejection-plan-395**.

Subsystem: Proposal 170 / Red25519 + ELS2.

Plan 393 added rollback for every post-begin compose/request/delivery failure,
but the exact-pinned reverse server still fails at `begin_ls2_publication`.
The begin call can reject because its bounded eight-entry publication cap is
full, no eligible floodfill is known, the key is invalid, or the body is not a
Standard LS2. Plan 394 identifies which condition occurs without recording raw
errors or identifiers.

## Objective

Add bounded coarse publication-begin rejection attribution, identify the
first failure in the exact-pinned NONE lane, and correct only that cause while
preserving Plan 393 rollback and legitimate ACK correlation.

## Dependencies and evidence

Hard inputs: Plans 380/381 for credentials and lane; Plans 385–393 for
post-start authority, build fairness, bounded role capacity, staged bridge
state, LS2 installation, and post-begin publication rollback. Plan 393 closure
contains exact pinned evidence and hashes.

## Invariants

1. Stock i2pd 2.61.0 at exact SHA
   `635b013a612ff47278ef02acf8580a28e10e26c5`; `MAX_ATTEMPTS=1`.
2. Keep payload traffic on loopback SAM and the private live mesh.
3. Diagnostics are bounded counters/enums only. Do not expose request ids,
   store keys, floodfill hashes, addresses, raw error strings, LS2 bytes, or
   reference logs.
4. Preserve publication cap, ACK correlation, and Plan 393 cleanup on every
   post-begin failure. Do not enlarge the cap to hide a leak.
5. Do not claim remote storage from local transport admission.
6. No transcript, credential, support, capability, inventory, or advertisement
   changes.

## Ordered work packages

1. Add per-product bounded counts for begin rejection categories: capacity,
   no eligible floodfill, key/body validation, and other. Include the current
   coordinator pending count as a numeric diagnostic.
2. Run one clean exact-pinned NONE lane and use its first observed rejection
   to trace whether stale requests came from prior initial/startup work, the
   reverse server, or an eligibility/validation mismatch.
3. Add deterministic regression for that exact cause and preserve the existing
   cancellation-capacity test.
4. Correct the owner/lifecycle source. If the pending set can retain
   successfully delivered but unacknowledged entries indefinitely, provide a
   bounded expiry/removal contract before retrying; do not drop legitimate
   response correlation early.
5. Rerun exact-pinned reverse NONE. Only after payload return proceed through
   PSK/DH, authority and three-peer controls, then relevant guards and floor.

## Verification

Format, managed-app sibling build before focused daemon tests, focused
publication coordinator tests, runner/checker self-tests, mutation table,
encrypted-consumer caller guard, and exact-pinned NONE. The full floor remains
conditional on successful live matrix evidence.

## Acceptance and stop conditions

Pass requires a precise bounded begin-rejection diagnosis, an appropriate
correction with max/max+1 and cleanup tests, and the control-created reverse
service publishing a real LS2 through local DatabaseStore admission and
returning the stock NONE payload. If a later publication or reference boundary
fails, register a successor plan.

## Lifecycle, compatibility, and security

No persistent or public wire format changes. Keep coordinator state finite and
ACK correlation valid until completion, explicit cancellation, or a justified
bounded timeout. Do not log raw begin errors. No capability or anonymity claim
changes.

## Closure evidence required

Record bounded begin-rejection counts, root cause and source correction,
publication capacity/ACK/timeout lifecycle evidence, exact commands/results,
live hashes, and registry/roadmap unblock audit.

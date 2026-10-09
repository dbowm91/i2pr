# Plan 397 — inbound type-5 service dispatch corrective

Status: **blocked-no-inbound-streaming-arrives-after-owner-fix-plan-398**.

Subsystem: Proposal 170 / Red25519 + ELS2.

## Objective

Trace why the stock consumer can resolve the i2pr type-5 record and establish a
Streaming connection, but no fixture payload reaches or returns from the
control-created server. Isolate inbound tunnel receive ownership, Garlic
authentication, Streaming admission, local target connect, and response
delivery with transaction-scoped bounded counters; fix the first missing
transition and rerun exact-pinned NONE.

## Evidence and constraints

Plan 396's healthy-mesh artifact
`target/interop/els2-evidence-plan396-none-postadmission-retry-20261009`
shows 1 local DatabaseStore admission, 0 begin rejections, successful client
connect, no fixture payload, zero active server connections, and zero accepted
inbound Streaming payloads. It records seven cumulative orphan receives, but
does not distinguish reverse-attempt traffic from prior authority traffic.
Service provisioning shows two registered inbound roles, two usable leases,
two installed bridge projections, and zero activation/owner failure counts.

## Invariants

1. Stock i2pd 2.61.0 at exact SHA
   `635b013a612ff47278ef02acf8580a28e10e26c5`; each lane invocation keeps
   `MAX_ATTEMPTS=1`.
2. Preserve real tunnel, blinded storage-key, coordinator capacity, ACK, and
   cancellation contracts. No direct transport or synthetic records.
3. Evidence uses bounded counts/enums only; no ids, keys, addresses, payloads,
   or raw reference logs.
4. Keep type-11 transcript, Proposal 170 inventory, support, and advertisement
   unchanged.

## Work packages

1. Capture pre-connect and post-connect deltas for service owner misses,
   recovered Garlic/authentication, Streaming packets accepted, server stream
   acceptance, local target connection, and response admission.
2. Trace the exact first missing transition in production routing and ensure the
   evidence distinguishes unrelated exploratory/orphan traffic.
3. Add a deterministic regression at that owner, including cancellation and
   teardown, while retaining Plans 393/395 lifecycle tests.
4. Run the focused guards/tests and one healthy-mesh exact-pinned NONE lane. If
   the first missing transition is still downstream or reference-specific,
   register a successor rather than broadening this fix.

## Acceptance

The reverse service returns the stock NONE fixture payload through the real
type-5 publication and Streaming route. The exact-pinned artifact must pass the
authority controls and show the transaction-scoped inbound/return path. No
broader Proposal 170 support or advertisement claim is made.

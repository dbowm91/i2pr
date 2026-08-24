# Plan 122 status — corrective closure required

## Current authority

- Status: **`corrective-reopened-plan124`**.
- Reopened: **2026-08-24** after post-closure source audit.
- Original implementation plan: [`122-m6-destination-routing-and-netdb-composition.md`](122-m6-destination-routing-and-netdb-composition.md).
- Corrective plan of record: [`124-m6-plan122-destination-routing-corrective-closure.md`](124-m6-plan122-destination-routing-corrective-closure.md).
- Successor after correction: [`125-m6-streaming-corrective-and-local-closure.md`](125-m6-streaming-corrective-and-local-closure.md).
- Historical closure commit and tests remain useful component evidence; this file corrects the stronger end-to-end acceptance classification.

## Why the prior closure is reopened

The post-closure audit found a concrete production-composition defect in
`i2pr_client::routing::compose_outbound_delivery()`.

The function successfully constructs an ECIES-protected Garlic payload and
retains it in `OutboundDeliveryPlan.encrypted_message`, but the actual outbound
tunnel call is fed the standard-encoded plaintext inner I2NP `Data` envelope:

```text
ECIES Garlic constructed                     yes
ECIES Garlic retained in returned plan        yes
bytes passed to outbound tunnel role           plaintext inner I2NP Data
```

The required product path is instead:

```text
inner I2NP Data
 -> ECIES Garlic ciphertext
 -> I2NP Garlic carrier
 -> destination-owned outbound tunnel
 -> OBEP TUNNEL(remote Lease2 gateway/id, Garlic)
```

Therefore the previous `passed-local-destination-routing` label overstated the
actual integrated path.

The prior `plan122_trajectory.rs` tests also do not provide successful full-path
closure evidence. The nominal outbound-composition test constructs inputs but
does not execute `compose_outbound_delivery()`, and the inbound dispatcher test
is negative malformed-input coverage rather than a successful authenticated
A -> B delivery.

## Retained valid work

The following Plan 122 surfaces remain useful and should not be reimplemented
without a concrete defect:

```text
typed LeaseSet2 lookup/result path
LeaseSet2 daemon NetDbSeam extension
bounded lease selection policy
DestinationRouting cache/state
OutboundRequest I2NP Data construction
ECIES session/Garlic construction from Plan 121
DestinationDispatcher typed failure paths
resource bounds and typed errors
```

Plan 124 is a composition correction and successful-trajectory closure, not a
rewrite of Plans 119-121.

## Current state

```text
plan_118 = closed
plan_119 = passed-leaseset2-protocol-foundation
plan_120 = passed-destination-lifecycle-and-pools
plan_121 = passed-ecies-destination-session-layer
plan_122 = corrective-reopened-plan124
plan_123 = provisional-blocked-on-plan122-correction
milestone6_local_product = not-closed
external_interop = not-claimed
next = plans/124-m6-plan122-destination-routing-corrective-closure.md
```

## Closure requirement

Plan 122 may return to a passed state only when Plan 124 proves that the actual
outbound tunnel path carries an encoded I2NP Garlic message containing the ECIES
ciphertext, that the OBEP emits the selected Lease2 TUNNEL delivery, and that a
successful A -> B -> A trajectory traverses real destination-owned outbound and
inbound tunnel roles with only the explicit post-OBEP local router-link seam.

Required eventual label:

```text
plan_122 = passed-corrected-local-destination-routing
plan_122_transport_boundary = authenticated-router-link-bypassed-local-seam
plan_122_external_interop = not-claimed
```

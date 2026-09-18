# Plan 205 status — Java I2P SAM/helper publication pivot

Status: **`retained-deferred-conditional-after-plan218-direct-i2cp-requalification`**.

## 2026-09-18 follow-up — Plan 220 passed, pivot still deferred

Plan 220 closed as
`passed-m6-java-plan219-diagnostic-attribution-corrective` (see
[`220-status.md`](220-status.md)): the Java main-NetDB path fully
passes on corrected evidence, so the open boundary is below the
layer a SAM bridge would replace. Plan 205 stays retained/deferred
with an unchanged token; no reactivation is authorized.

```text
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_220 = passed-m6-java-plan219-diagnostic-attribution-corrective
plan_221 = registered-ready-m6-java-client-netdb-ocmosj-narrowing
```

## 2026-09-18 authority correction — Plan 219 attribution superseded

Plan 219's J219-B attribution is no longer authority. That does not reactivate
Plan 205: Plan 218 still shows a reverse-delivery failure, but the exact layer
is unknown until Plan 220 repairs the diagnostic.

Plan 205 therefore returns to its pre-Plan-219 disposition: retained/deferred,
conditional on a trustworthy direct-I2CP classification showing that a SAM
experiment addresses a genuinely different boundary.

```text
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_220 = registered-ready-m6-java-plan219-diagnostic-attribution-corrective
```

## Retained prior Plan 205 status narrative

# Plan 205 status — Java I2P SAM/helper publication pivot

Status: **`retained-deferred-conditional-after-plan218-direct-i2cp-requalification`** (Plan 218 inbound-delivery boundary did not reauthorize Plan 205 — the boundary is on Java's helper-side outbound tunnel endpoint's peer-selection path, not on the helper's local LeaseSet publication gap).

## 2026-09-18 disposition amendment

Plan 205 is no longer the immediate executable Java closure path.

Source review after Plan 216 found that Java SAM ultimately rides the same
Java client/I2CP tunnel and LeaseSet machinery that the existing public helpers
already exercise. More importantly, the Plan 216 destination run reached a
stale post-transfer assertion that is reachable only after a completed remote
LS2 ingest. Rewriting the helpers before correcting that lane would discard a
path that may already cross the claimed publication boundary.

Plan 205 is therefore retained, not deleted or declared impossible. It may be
reactivated only if:

1. Plan 217 closes the harness/evidence defects; and
2. Plan 218 then records a reproducible stock-Java direct-I2CP public-client
   boundary for which SAM is a justified independent API experiment.

Plan 218 ran the corrected Plan 217 harness on commit `7762e13` and
recorded a reproducible inbound-delivery boundary
(`java-floodfill-candidate=0` on Router A ⇒ helper's outbound tunnel
endpoint cannot resolve the i2pr destination's LeaseSet through Java's
netDb; Plan 194 §11 stop `client-ls2-local-but-not-network-visible` on
the destination direction; `streaming-b-accept STATUS OK but inbound
SYN never reached the backlog` on the streaming direction). SAM-bridge
helper publication does not address this primitive: Java's SAM bridge
ultimately routes the destination through the same
`ClientConnectionRunner → I2PSessionImpl → FloodfillNetworkDatabaseFacade`
machinery that the direct-I2CP path uses. Plan 205 therefore does NOT
reauthorize itself on the Plan 218 outcome; a future plan-of-record (e.g.
Plan 219 — M6 Java second-family inbound-delivery primitive) would own
the inbound-delivery axis.

Until then:

```text
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_218 = stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary
next_executable_plan = a fresh plan-of-record (e.g. Plan 219 — M6 Java second-family inbound-delivery primitive), not Plan 205
```

The prior Plan 205 registration narrative is retained below for traceability;
statements calling it the "next executable" are historical and superseded by
this amendment.

## Retained prior registration context

Plan of record: [`205-m6-java-sam-bridge-helper-pivot.md`](../../implementation/mixed-router-interop/205-m6-java-sam-bridge-helper-pivot.md).

Plan 205 remains the next Java-side experiment after Plan 201. Its purpose is unchanged: attempt a standards-compatible public-helper path against exact-pinned stock Java I2P 2.13.0 without public-network participation, Java patching, private NetDB injection, fake LeaseSets, or i2pr production-wire changes.

This status corrects only the M10 authority inherited by the earlier Plan 205 registration. Plans 202/203 are no longer considered closed product/application proof; they have been reclassified as partial scaffolding and superseded by Plans 206/207.

Current parallel execution graph:

```text
plan205 = registered-executable-java-publication-pivot
plan206 = registered-executable-m10-production-remote-delivery-corrective
plan207 = registered-blocked-by-plan206

plan205 || plan206 may execute in parallel
plan206 -> plan207
plan205 + plan207 -> plan204 convergence
```

Current authority:

```text
milestone6_i2pd_streaming_interop = passed-via-plan193
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed

plan_202 = partial-m10-remote-routing-capability-surface-superseded-by-plan206
plan_203 = partial-m10-remote-application-evidence-scaffolding-superseded-by-plan207
m10_remote_transport_core = not-yet-passed
m10_remote_application_interop = not-yet-passed
milestone10_remote_service_interop = not-yet-passed
milestone10_final_acceptance = not-yet-closed
```

Plan 205 must not depend on Plan 206/207 and must not use their progress to weaken Java-family evidence. Conversely, M10 corrective work need not wait for Java.

The SAM/helper pivot is still an experiment rather than a guaranteed closure mechanism: previous work already showed stock Java client-tunnel/profile constraints in the controlled topology. Plan 205 must preserve fail-closed classification if the public SAM/helper path encounters the same publication/tunnel boundary.


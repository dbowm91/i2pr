# Plan 220 status — M6 Java Plan 219 diagnostic-attribution corrective

Status: **`registered-ready-m6-java-plan219-diagnostic-attribution-corrective`**.

Plan of record:
[`220-m6-java-plan219-diagnostic-attribution-corrective.md`](../../implementation/mixed-router-interop/220-m6-java-plan219-diagnostic-attribution-corrective.md).

## Registration basis

Review of the landed Plan 219 implementation found that its read-only diagnostic
infrastructure is useful, but its terminal
`J219-B-A-STORED-B-RI-NOT-F` attribution is not trustworthy.

The corrective owns these defects:

- load-bearing A-view-of-B facts are derived before bootstrap;
- Router B hash is built from an invalid raw RouterInfo slice and standard
  Python Base64;
- the destination driver already submits B's signed RouterInfo to A, contrary
  to the Plan 219 closure narrative;
- the stored-B-`f` fact checks presence only;
- selector output is synthesized from PeerManager membership;
- client-NetDB / OCMOSJ facts use hard-coded/default booleans;
- forward `reference-received` is misused as reverse dispatch evidence;
- the closure names the pre-implementation parent as its implementation/run
  SHA; and
- Plan-specific classification machinery leaked into production source.

Plan 220 repairs the diagnostic/evidence path only. It does not own a
bidirectional-bootstrap, topology, NetDB, SAM, tunnel, or i2pr wire fix.

## Current authority

```text
plan_217 = passed-m6-java-closure-harness-and-evidence-corrective
plan_218 = stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary
plan_219 = retained-diagnostic-instrumentation-attribution-invalid-superseded-by-plan220
plan_220 = registered-ready-m6-java-plan219-diagnostic-attribution-corrective

plan_201 = blocked-pending-plan220-corrected-java-attribution
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan220-diagnostic-corrective

milestone6_i2pd_streaming_interop = passed-via-plan193
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
```

A protocol/topology corrective successor is intentionally unregistered until
Plan 220 produces corrected exact-clean-head evidence.

# Plan 217 status — M6 Java closure harness and evidence corrective

Status: **`registered-ready-m6-java-closure-harness-corrective`**.

Plan of record:
[`217-m6-java-closure-harness-corrective.md`](../../implementation/mixed-router-interop/217-m6-java-closure-harness-corrective.md).

## Registration basis

Plan 216 is retained unchanged as a diagnostic snapshot, but its blocker
interpretation is not sufficient authority for another Java helper rewrite:

- the destination driver transfers the installed outbound role out of
  `coord.registry_mut()` and later asserts the registry still owns it;
- reaching that stale assertion requires the preceding remote LS2 lookup to have
  produced `LeaseStoreIngestOutcome::Completed`;
- several Java lifecycle grep counters do not match exact-pinned Java 2.13.0
  logging semantics, and positive labels currently include negative strings;
- destination and Streaming drivers reuse build/tunnel identifiers against the
  same live Java RouterContexts.

Plan 217 owns only the harness/evidence correction and a fresh exact-head
classification.

## Current authority

```text
plan_201 = blocked-by-plan217-java-closure-harness-corrective
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_217 = registered-ready-m6-java-closure-harness-corrective
plan_218 = registered-blocked-on-plan217-m6-java-final-qualification

milestone6_i2pd_streaming_interop    = passed-via-plan193
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable             = not-yet-claimed
```

No closure evidence exists yet. Implementation must satisfy Plan 217 §11 and
write the command-derived closure matrix before this status may transition to a
`passed-*` token.

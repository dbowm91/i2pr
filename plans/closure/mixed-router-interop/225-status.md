# Plan 225 status — M6 Java NO_LEASESET lookup-path observability corrective

Status: **`passed-m6-java-no-leaseset-lookup-path-observability-corrective-with-exact-attribution`**.

Plan of record:
[`225-m6-java-no-leaseset-lookup-path-observability-corrective.md`](../../implementation/mixed-router-interop/225-m6-java-no-leaseset-lookup-path-observability-corrective.md).

Supporting closure record:
[`225-corrective-closure.md`](225-corrective-closure.md).

Plan 224 closed with:

```text
P224-OBSERVABILITY-GAP-LOOKUP-PATH
```

Router B's target LS2 was observed as current and query-answerable, while the
helper client sub-DB remained empty. Plan 225 added effective logger-level
proof, exact Java helper-client Base32 correlation, bounded nested-log
sanitization, and one terminal classification for the authoritative attempt.
The exact result is:

```text
P225-ATTRIBUTION-A-SEARCH-EXHAUSTED-WITHOUT-QUERYING-B
```

This is a diagnostic attribution only: Router A's helper search was observed
to start and exhaust without an actual target lookup dispatch to Router B.
Plan 225 did not change production protocol behavior, topology, timing, SAM,
or Java router state. It does not complete the Java second-family closure.

```text
plan_224 = passed-m6-java-no-leaseset-lookup-path-attribution-observability-gap
plan_225 = passed-m6-java-no-leaseset-lookup-path-observability-corrective-with-exact-attribution
plan_201 = blocked-pending-m6-java-second-family-closure-after-plan225-attribution
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan201-after-plan225-attribution
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
next_executable_plan = none (Plan 201 remains blocked pending Java second-family closure)
```

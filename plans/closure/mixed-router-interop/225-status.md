# Plan 225 status — M6 Java NO_LEASESET lookup-path observability corrective

Status: **`registered-ready-m6-java-no-leaseset-lookup-path-observability-corrective`**.

Plan of record:
[`225-m6-java-no-leaseset-lookup-path-observability-corrective.md`](../../implementation/mixed-router-interop/225-m6-java-no-leaseset-lookup-path-observability-corrective.md).

Plan 224 closed with:

```text
P224-OBSERVABILITY-GAP-LOOKUP-PATH
```

Router B's target LS2 was observed as current and query-answerable, while the
helper client sub-DB remained empty. The class-scoped logger configuration was
installed, but exact A→B/B→A lookup-path facts were not observable without
making an unauthorized source or state change. Plan 225 is registered to own
that bounded observability corrective. It is not a production protocol fix,
and it does not unblock Plan 201 or Plan 204 yet.

```text
plan_224 = passed-m6-java-no-leaseset-lookup-path-attribution-observability-gap
plan_225 = registered-ready-m6-java-no-leaseset-lookup-path-observability-corrective
plan_201 = blocked-pending-plan225-no-leaseset-lookup-path-observability-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan225-observability
next_executable_plan = 225-m6-java-no-leaseset-lookup-path-observability-corrective
```

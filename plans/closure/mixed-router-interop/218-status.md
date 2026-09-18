# Plan 218 status — M6 Java second-family final qualification

Status: **`ready-m6-java-second-family-final-qualification`**.

Plan of record:
[`218-m6-java-second-family-final-qualification.md`](../../implementation/mixed-router-interop/218-m6-java-second-family-final-qualification.md).

Plan 217 closed the harness/evidence corrective that Plan 216 exposed. The
corrected destination driver no longer panics on the duplicate-block
regression; the positive/negative Java lifecycle evidence split is enforced
by the static checker; the controlled-launcher `router.networkDatabase.dbDir`
is relative (no doubled-path bug); the streaming driver has a disjoint
build/tunnel/message-id namespace; and the harness exposes an
`I2PR_M6_JAVA_DRIVER=destination|streaming|both` selector for diagnosis.

Plan 218 is the executable head. Its task is the fresh exact-head external
run on the corrected harness, the terminal `P200-*` classification, and
the publication-path row flips.

Primary path: the existing public Java direct-I2CP helpers.

Conditional fallback only: Plan 205 may be reactivated if a Plan 218 run,
executed after Plan 217, proves a reproducible stock-Java public-client
publication boundary. SAM is not an automatic prerequisite for Plan 218.

Current authority:

```text
plan_217 = passed-m6-java-closure-harness-and-evidence-corrective
plan_218 = ready-m6-java-second-family-final-qualification
plan_201 = blocked-by-plan218-fresh-external-classification
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

milestone6_java_mixed_router_interop = not-yet-passed (corrected harness ready; fresh exact-head classification pending Plan 218)
milestone6_interoperable             = not-yet-claimed
plan_204                             = blocked-on-m6-java-second-family-closure (awaits Plan 218 success or a later explicitly registered fallback)
```

No closure evidence exists yet for Plan 218; the next external
`bash tests/integration/m6-interop/run-java.sh` execution on the corrected
harness owns the qualification matrix flip.

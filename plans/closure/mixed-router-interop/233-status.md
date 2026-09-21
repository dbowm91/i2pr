# Plan 233 status — M6 Java Streaming SYN-ACK corrective after route-derived lease fix

Status: **superseded-before-execution-by-plan234-final-closure-authority-corrective**.

Plan of record:
[`233-m6-java-streaming-syn-ack-corrective-after-route-derived-lease-fix.md`](../../implementation/mixed-router-interop/233-m6-java-streaming-syn-ack-corrective-after-route-derived-lease-fix.md).

## Disposition

Plan 233 was registered by the Plan-232 closure commit
`c3182bd05021609249f57fd925d6bcc6947a61e5` but was never executed and has no
implementation commit.

Its narrow Streaming SYN-ACK investigation remains technically valid, but its
final-closure authority is incomplete:

1. the registration omitted this status record even though the registry and M6
   roadmap linked to it;
2. Plan 233 permits `P233-JAVA-SECOND-FAMILY-PASSED` after the retained
   Streaming rows pass, while authoritative Plan-201/run-java closure still
   treats the Plan-200 `C/D Java public-client LeaseSet lifecycle rows as
   required;
3. `tests/integration/m6-interop/run-java.sh` still marks every non-passed
   `ref_row()` as `REQUIRED_FAILED=1` and exits nonzero when any required row
   remains failed/blocked;
4. Plan 232 explicitly retained those `C/D` rows as unresolved legacy
   publication evidence.

Plan 234 supersedes Plan 233 before execution. It inherits the corrected
Plan-232 fixture and the narrow Streaming SYN-ACK attribution, but adds an
explicit final-closure authority reconciliation so M6 cannot be declared
passed while the required harness still exits nonzero.

No Plan-233 implementation evidence exists and no technical pass/fail result
should be inferred from this supersession.

## Current authority

```text
plan_232 = passed-m6-java-route-derived-lease-gateway-fixture-corrective-with-raw-reverse-passed-streaming-boundary
plan_233 = superseded-before-execution-by-plan234-final-closure-authority-corrective
plan_234 = registered-ready-m6-java-streaming-syn-ack-and-client-ls2-final-closure-authority-corrective

plan_201 = blocked-pending-plan234-streaming-and-final-closure-authority-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan234
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 234-m6-java-streaming-syn-ack-and-client-ls2-final-closure-authority-corrective
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed

milestone10_final_acceptance = closed
```

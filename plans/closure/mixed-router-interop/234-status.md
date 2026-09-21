# Plan 234 status — M6 Java Streaming SYN-ACK and client-LS2 final-closure authority corrective

Status: **registered-ready-m6-java-streaming-syn-ack-and-client-ls2-final-closure-authority-corrective**.

Plan of record:
[`234-m6-java-streaming-syn-ack-and-client-ls2-final-closure-authority-corrective.md`](../../implementation/mixed-router-interop/234-m6-java-streaming-syn-ack-and-client-ls2-final-closure-authority-corrective.md).

## Registration basis

Plan 232 closed at Outcome B:

```text
passed-m6-java-route-derived-lease-gateway-fixture-corrective-with-raw-reverse-passed-streaming-boundary
```

Raw-Destination bidirectional delivery is now digest-matched on the corrected
route-derived lease fixture. The only new data-plane stop is Java Streaming
Direction A:

```text
streaming-syn-sent=true
syn_accepted=false
established=false
pump_error=0
```

reproduced twice on the same Plan-232 implementation SHA with no tuning.

Plan 233 was registered for this narrow stop but never executed. It is
superseded before execution because its final-family closure outcome did not
account for the still-required Plan-200/201 Java public-client LeaseSet
lifecycle rows in `run-java.sh`.

Plan 234 inherits the narrow SYN-ACK attribution and adds the missing
final-closure authority reconciliation.

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

## Required closure gates

Plan 234 cannot close Java-family M6 from a focused Streaming pass alone.

A family pass requires, on one exact head:

1. corrected Plan-232 route parity retained;
2. raw Destination pass retained;
3. Java Streaming Direction A+B and live refresh pass;
4. every required Plan-200 `C/D row either passes or is explicitly superseded
   by stronger mandatory executed external evidence;
5. full `tests/integration/m6-interop/run-java.sh` exits 0;
6. M6 mixed-router acceptance checker passes;
7. final M6 closure evidence checker passes;
8. full routine verification passes.

No production change is authorized unless an exact i2pr-owned Streaming defect
is first proven.

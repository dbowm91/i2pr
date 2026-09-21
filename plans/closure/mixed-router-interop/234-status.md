# Plan 234 status — M6 Java Streaming SYN-ACK and client-LS2 final-closure authority corrective

Status: **passed-m6-java-streaming-syn-ack-attributed-with-java-accepted-no-response-boundary**.

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
plan_234 = passed-m6-java-streaming-syn-ack-attributed-with-java-accepted-no-response-boundary

plan_201 = blocked-pending-plan235-java-streaming-post-accept-response-boundary-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan235
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 235-m6-java-streaming-post-accept-response-boundary-corrective
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

## Closure disposition

Plan 234 closes at **Outcome B**. The corrective implementation and evidence
surface are complete, but Java-family/M6 capability closure is intentionally not
claimed. The earliest counted boundary is stable and exact:

```text
P234-B-JAVA-ACCEPTED-NO-RESPONSE-OBSERVED
```

The Java helper accept worker was requested, entered `server.accept()`,
returned an `I2PSocket`, and stored it. During the frozen 45-second SYN epoch
i2pr observed zero inbound `TunnelData`, so no tunnel recovery, Garlic decode,
destination dispatch, or Streaming adapter call occurred. This is a
post-Java-accept / pre-i2pr-inbound boundary, not an i2pr-owned production
defect proof. No production `src/` file changed.

## Exact implementation and pins

```text
implementation_sha = 343bc156e02ac5f819fba8b3644cefca4624f513
java_i2p = 2.13.0 @ 9134f808337b401e8e53c73734c81fab04280c9d
i2pd = 2.61.0 @ 635b013a612ff47278ef02acf8580a28e10e26c5
plan_233 = superseded-before-execution-by-plan234-final-closure-authority-corrective
```

Plan 233 had no implementation SHA, counted execution, or technical result.

## Implementation landed

- `ReferenceStreamingService` now exposes bounded read-only
  `REPORT_STREAM_STATE` counters for accept requested/entered/returned/stored
  and helper errors.
- `java_tunnel_external.rs` records the typed SYN epoch, destination hashes as
  bounded digests, ports/connection identity, Java accept state, inbound
  TunnelData/recovery/Garlic/adapter counters, pending outbound count, ACK and
  retransmit poll counts, and one deterministic `P234-*` terminal.
- The prior coarse `syn_accepted=false established=false pump_error=0` stop is
  no longer the authority. Decode, recovery, Garlic, queue, adapter, and state
  boundaries remain distinct.
- Sixteen local Plan-234 unit rows and the mixed-router static checker enforce
  the attribution and final-authority rules. No production Rust surface carries
  Plan-234 tokens.
- `check-m6-final-closure-evidence.sh` now requires the Plan-234 epoch and
  classification evidence and rejects a final Java pass without the exact
  established terminal.

## Counted external attempts

All attempts used implementation SHA `343bc15`, the exact pins above, loopback
only, the existing topology, and the frozen windows. No between-attempt tuning
occurred.

| Attempt | Selector/evidence | Result |
| --- | --- | --- |
| 1 | `I2PR_M6_JAVA_DRIVER=streaming` | Rust driver completed with `P234-B-JAVA-ACCEPTED-NO-RESPONSE-OBSERVED`; the wrapper exited 1 because destination-only required rows were not applicable to a Streaming-only run. |
| 2 | `I2PR_M6_JAVA_DRIVER=both` | Raw Destination forward/reverse passed; Streaming emitted the same `P234-B-JAVA-ACCEPTED-NO-RESPONSE-OBSERVED`; full wrapper exited 1 on required Streaming and Plan-200 C/D rows. |
| 3 | `I2PR_M6_JAVA_DRIVER=both`, retained at `target/interop/m6-java-evidence-plan234-attempt3` | Streaming emitted the same terminal; this run hit a stochastic pre-Streaming destination stop and is not used to replace the retained raw-Destination pass. |

The retained attempt-2 sanitized ledger is `target/interop/m6-java-evidence`
and the independently retained attempt-3 ledger is
`target/interop/m6-java-evidence-plan234-attempt3`; raw Java logs remain scratch
only and are not evidence.

## SYN epoch evidence

Attempt 2 and attempt 3 both emitted these bounded facts:

```text
java_accept_thread_started=true
java_accept_returned=true
java_stream_socket_count_delta=1
java_stream_receive_observed=true
java_stream_response_observed=false
i2pr_inbound_tunneldata_count=0
i2pr_expected_stream_tunneldata_count=0
i2pr_tunnel_recovery_count=0
i2pr_garlic_payload_count=0
i2pr_streaming_adapter_calls=0
i2pr_streaming_adapter_successes=0
i2pr_streaming_adapter_errors=0
connection_state=SynSent
pending_outbound_after_receive=0
ack_poll_emissions=0
retransmit_poll_emissions=0
java_accept_errors=0
```

The helper status was `accept_requested=1 accept_entered=1 accept_returned=1
socket_stored=1 accept_errors=0 accepting=false accepted_count=1`. The helper's
public API semantics prove accept returned a socket; they do not, by themselves,
prove that the Java-generated SYN response reached i2pr. That is the exact
successor boundary.

## Plan-232 baseline and retained raw pass

The attempt-2 ledger retained all three route-derived lease sites with
`gateway_route_match=true`, `tunnel_route_match=true`, and
`publication_distinct_from_gateway=true`; the Streaming initial route parity
row and publication separation row were both present. The raw Destination rows
`external-destination-outbound`, `external-reference-received`, and
`external-destination-inbound` passed, with `P232-D-REVERSE-DELIVERY-PASSED`
and digest-matched reverse evidence. This retained pass is not relitigated by
the Streaming stop.

## Plan-200 C/D authority reconciliation matrix

The matrix below uses the full attempt-2 ledger. A failed/absent historical
observation is not superseded merely because another path was usable; no row
has a replacement mapping in this closure.

| Required row | Status | Original invariant | Resolution | Replacement mandatory row |
| --- | --- | --- | --- | --- |
| `external-java-client-subdb-created` | passed | Java created the client-specific NetDB facade | PASS | — |
| `external-java-create-leaseset2-received` | failed | Java received CreateLeaseSet2 | STILL-BLOCKING | — |
| `external-java-client-leaseset-stored-current` | failed | Current client LS2 was stored | STILL-BLOCKING | — |
| `external-java-client-leaseset-publish-scheduled` | failed | Client LS2 republish was scheduled | STILL-BLOCKING | — |
| `external-java-client-leaseset-republish-job-ran` | failed | Client LS2 republish job ran | STILL-BLOCKING | — |
| `external-java-client-inbound-tunnel-eligible` | failed | Client inbound tunnel was selectable | STILL-BLOCKING | — |
| `external-java-client-outbound-tunnel-eligible` | failed | Client outbound tunnel was selectable | STILL-BLOCKING | — |
| `external-java-floodfill-candidate-available` | failed | Floodfill selector had a candidate | STILL-BLOCKING | — |
| `external-java-store-emitted` | failed | Client LS2 DatabaseStore was emitted | STILL-BLOCKING | — |
| `external-java-store-ack-observed` | failed | Client LS2 store acknowledgement was observed | STILL-BLOCKING | — |
| `external-java-store-failure-reason` | failed | Store failure outcome was surfaced | STILL-BLOCKING | — |

The rows are retained as required because the successful raw Destination and
Streaming-helper creation facts do not prove the missing client-LS2 lifecycle,
especially republish scheduling/execution and store acknowledgement. The
`REQUIRED_FAILED` gate remains unchanged and fail-closed.

## Acceptance matrix

| Plan-234 requirement | Result |
| --- | --- |
| Corrected Plan-232 fixture retained | passed on attempt 2 |
| Earliest SYN boundary attributed | passed: `P234-B-JAVA-ACCEPTED-NO-RESPONSE-OBSERVED` |
| Narrow helper-only corrective | passed; Java helper + external driver only |
| Direction A Streaming | blocked at the exact terminal above |
| Direction B and live refresh | not reached; remains blocked by Direction A |
| Every Plan-200 C/D row passed or explicitly superseded | not satisfied; 10 rows still block, none superseded |
| Full `run-java.sh` exit 0 | not satisfied; required rows make it exit 1 |
| M6 mixed-router checker | passed |
| Final closure checker | correctly rejects the non-passing Java ledger |
| Java-family/M6 passed claim | not emitted |
| M10 authority | unchanged and closed via Plans 213–215 |

## Verification run

Passed before and on the implementation commit:

```text
cargo fmt --all --check
cargo test --locked -p i2pr-daemon --test java_tunnel_external p234_ -- --test-threads=1  # 16 passed
cargo test --locked -p i2pr-daemon --test java_tunnel_external p232_ -- --test-threads=1  # 18 passed
cargo test --locked -p i2pr-daemon --test java_tunnel_external p231_ -- --test-threads=1  # 18 passed
bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
bash -n scripts/check-m6-final-closure-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
```

The external wrapper's embedded workspace-gates slice passed formatting,
workspace check, and all invoked static boundary scripts. The full external
Java wrapper and final closure checker correctly remained non-passing because
the required Streaming and C/D rows are unresolved. The remaining routine
floor commands are recorded in the final handoff and must be rerun on the
successor's closing head.

## Unblock audit

No downstream plan can be unblocked by this closure:

- Plan 201 remains `blocked-pending-plan235-java-streaming-post-accept-response-boundary-corrective`; its Java-family publication and final-closure authority gates are not closed.
- Plan 204 remains `blocked-on-m6-java-second-family-closure-pending-plan235`; its cross-milestone normalization dependency is not satisfied.
- Plan 205 remains retained/deferred; no direct-I2CP result authorizes the SAM/helper pivot.
- M10 final acceptance remains closed and unchanged.

Plan 235 is the only new dependency-ready work: it owns the exact
post-Java-accept / pre-i2pr-inbound response boundary and must not change
production behavior without a separately proven i2pr-owned defect.

```text
plan_201 = blocked-pending-plan235-java-streaming-post-accept-response-boundary-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan235
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_234 = passed-m6-java-streaming-syn-ack-attributed-with-java-accepted-no-response-boundary
plan_235 = registered-ready-m6-java-streaming-post-accept-response-boundary-corrective
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
next_executable_plan = 235-m6-java-streaming-post-accept-response-boundary-corrective
```

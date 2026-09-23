# Plan 244 — M6 Java Streaming reverse-direction continuous response attribution

Status: **registered-ready-m6-java-streaming-reverse-direction-continuous-response-attribution**

## 1. Objective

Close the remaining Java → i2pr Streaming response-path uncertainty by running
the now-working Plan-243 hosted lane and correlating the already-proven
Plan-237/238/239/240 response observers in one continuous response epoch.

Plan 243 established Direction A (i2pr → Java Streaming) on two of three
counted same-SHA runs. In those successful runs, the retained Plan-236
classifier still reported:

```text
P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP
```

That token is now only the earliest historical classifier. It MUST NOT cause
Plan 244 to stop if the later retained observers prove deeper stages.

Plans 237–240 already proved separately that stock Java can reach:

```text
SchedulerReceived
→ Connection.sendPacket
→ PacketQueue / I2PSession.sendMessage
→ Router-A I2CP admission
→ OCMOSJ target-LS lookup
→ exact Streaming ISJ with Router B in toTry
```

Plan 244's job is to correlate those stages in the same successful hosted
Plan-243 response epoch and continue until the first real Java → i2pr boundary.

This is an attribution/qualification pass, not a speculative corrective.

## 2. Exact retained authority

Retain unchanged:

- Java I2P 2.13.0 at
  `9134f808337b401e8e53c73734c81fab04280c9d`;
- i2pd 2.61.0 at
  `635b013a612ff47278ef02acf8580a28e10e26c5`;
- Plan-232 route-derived lease-gateway correction and raw-Destination
  bidirectional pass;
- Plan-237 stock response observer;
- Plan-238 Router-A admission observer;
- Plan-239 Router-A dispatch/target-LS lookup observer;
- Plan-240 exact Streaming target-job / Router-B query attribution;
- Plan-241 one-hop Streaming helper profile;
- Plan-242 stock-candidate/non-zero-pair gates and extended tunnel-role
  observation;
- Plan-243 host qualification and hosted attempt discipline;
- frozen A/B/C topology;
- Router-B publication target;
- frozen 45-second response windows;
- scratch-only raw Java logs;
- no production change before an exact i2pr-owned reverse-direction boundary.

Plan-243 implementation authority:

```text
f359baba57fc7d952ab7f5a5367d34671c590b18
```

Plan-243 closure authority:

```text
08a1cfc7ee3844a0657c0f6568738328690cebb6
```

## 3. Core correction: historical classifier != current stop

The Plan-243 successful attempts emitted the historical retained chain:

```text
P234-C-STREAMING-DIRECTION-A-ESTABLISHED
P235-JAVA-STREAMING-PASSED
P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP
P239-F-DIRECTION-A-ESTABLISHED
P240-A-STREAMING-LOOKUP-JOB-NOT-CORRELATED
```

Plan 244 must not treat the P236 token as authoritative when Plan-237+
observations for the same response epoch are available.

The authoritative rule is:

```text
earliest missing stage among the deepest trustworthy same-epoch observers
```

not:

```text
first historical classifier token printed
```

Plan 244 must correlate by epoch and stop at the deepest supported live
boundary without rewriting predecessor history.

## 4. Host and attempt discipline

Reuse Plan-243 host qualification unchanged.

Before counted attempts:

```bash
bash scripts/interop/check-p243-host-qualified.sh --expected-sha <implementation-sha>
```

The host must prove:

- exact Java cache and source pin;
- exact workspace SHA;
- i2pr daemon present;
- source-lock inputs present;
- loopback port capacity;
- writable fresh RouterContexts/evidence directories.

After host qualification:

- commit Plan-244 implementation before counted attempts;
- use one exact implementation SHA;
- run three counted attempts;
- fresh A/B/C RouterContexts each attempt;
- unique evidence directories;
- no between-attempt tuning;
- no retry-until-C;
- no RNG manipulation;
- no zero-hop fallback;
- no topology/publication/timing changes;
- no Java source/jar patch;
- no production i2pr changes unless §11 authorizes one from exact live evidence.

## 5. Direction-A prerequisite

Plan 244 only evaluates the reverse response epoch when Direction A actually
establishes.

Require the retained Plan-243 evidence:

```text
java_accept_returned=true
expected TunnelData observed
tunnel recovery observed
Garlic payload observed
Streaming adapter success
connection_state=Established
```

If the attempt stops earlier at stock Java helper startup/client build,
classify the retained early terminal and do not infer anything about the
reverse direction.

Examples:

```text
P244-A-HOSTED-LANE-NOT-REACHED reason=helper-handshake
P244-A-HOSTED-LANE-NOT-REACHED reason=nonzero-pair
P244-A-DIRECTION-A-NOT-ESTABLISHED
```

An earlier stock-Java startup stop does not invalidate a deeper result on
another counted attempt.

## 6. One continuous response epoch

For every attempt where Direction A establishes, create one response-epoch
identifier and bind every retained observer to it.

The binding must include, as applicable:

```text
helper DBID
target Destination hash
response epoch id
pre/post helper response-stat snapshots
pre/post Router-A admission snapshots
pre/post Router-A dispatch/lookup snapshots
exact Streaming ISJ job IDs
target hash
Router-B candidate/query trace
i2pr expected reverse TunnelData identity
```

The same response epoch must own all downstream attribution.

Destination-lane traffic and pre-existing counters may provide non-zero
absolutes, but only same-epoch deltas/correlated IDs may satisfy a stage.

## 7. Stage A — stock Java response emission

Reuse Plan 237 exactly.

Require real deltas for:

```text
scheduler activity
Connection.sendPacket response construction
PacketQueue / I2PSession.sendMessage return
send failures
send exceptions
```

Canonical progression:

```text
scheduler delta > 0
→ sendPacket construction delta > 0
→ sendMessage lifetime-event delta > 0
→ failure delta = 0
→ exception delta = 0
```

If missing, stop with the earliest exact Plan-244 token:

```text
P244-B-SCHEDULER-NOT-OBSERVED
P244-B-RESPONSE-PACKET-NOT-CONSTRUCTED
P244-B-SENDMESSAGE-NOT-RETURNED
P244-B-SENDMESSAGE-FAILED
```

Do not reuse P236 placeholder booleans as evidence.

## 8. Stage B — Router-A I2CP admission and OCMOSJ lookup

Reuse Plan 238/239 exactly.

After helper sendMessage return, require same-epoch Router-A evidence:

```text
client.distributeTime delta > 0
```

Then record:

```text
client.dispatchTime delta
client.dispatchSendTime delta
target LS local presence pre/post
remote lookup found delta
remote lookup failed delta
```

If Router-A admission is absent:

```text
P244-C-ROUTER-A-I2CP-NOT-ADMITTED
```

If admitted but target LS lookup does not start / cannot be correlated:

```text
P244-C-TARGET-LOOKUP-NOT-CORRELATED
```

Do not infer dispatch failure solely from dispatch counters until the exact
lookup stage is known.

## 9. Stage C — exact Streaming ISJ and Router-B query

Reuse Plan 240's exact correlation:

```text
helper DBID + target hash + exact Streaming ISJ job ID + response epoch
```

Require read-only facts:

- Router B RI present/readiness facts;
- B in the exact job's initial `toTry`;
- negative cache state;
- IP-close state;
- old-router state;
- outbound/inbound tunnel availability;
- crypto compatibility;
- selected lookup outbound tunnel;
- selected tunnel zero/non-zero state;
- exact pre-query guard, if any;
- exact target DLM dispatch to B.

The Plan-242 non-zero client-pool authority changes the interpretation of the
old Plan-240 terminal. If the live lookup still selects zero-hop despite an
authoritative non-zero-only client pool, stop:

```text
P244-D-LOOKUP-ZERO-HOP-SELECTION-CONTRADICTION
```

If another exact pre-query guard fires, emit a typed Plan-244 token naming
that guard and stop.

If the target DLM is dispatched to B:

```text
P244-D-B-QUERY-DISPATCHED
```

and continue.

## 10. Stage D — lookup reply and client-subDB install

After exact B query dispatch, evaluate strictly in order:

```text
B receives exact target DLM
→ B target LS is current/valid/query-answerable
→ B emits reply
→ A helper inbound client tunnel receives encrypted DSM
→ helper client NetDB installs target LS
→ exact lookup-success callback fires
```

Canonical terminals:

```text
P244-E-B-LOOKUP-NOT-RECEIVED
P244-E-B-TARGET-LS-NOT-QUERY-ANSWERABLE
P244-E-B-ANSWER-NOT-EMITTED
P244-E-A-CLIENT-DSM-NOT-RECEIVED
P244-E-A-CLIENT-SUBDB-NOT-INSTALLED
P244-E-LOOKUP-SUCCEEDED
```

Do not evaluate later stages after an earlier missing stage.

## 11. Stage E — OCMOSJ dispatch to reverse tunnel

Only after exact lookup success, continue through the retained Plan-239/231
ordering:

```text
usable target lease
→ outbound response tunnel selected
→ Garlic constructed
→ DispatchJob
→ TunnelDispatcher.dispatchOutbound
→ Router-A outbound gateway enqueue
→ transit/endpoint processing
→ route-derived target IBGW
```

Canonical terminals:

```text
P244-F-NO-USABLE-TARGET-LEASE
P244-F-NO-OUTBOUND-RESPONSE-TUNNEL
P244-F-GARLIC-PREP-FAILED
P244-F-DISPATCH-OUTBOUND-NOT-OBSERVED
P244-F-OUTBOUND-GATEWAY-NOT-ENQUEUED
P244-F-TRANSIT-NOT-PROCESSED
P244-F-TARGET-IBGW-NOT-INSTALLED
P244-F-TARGET-IBGW-NO-DISPATCH
```

The route-derived lease-gateway fixture from Plan 232 remains frozen.

## 12. Stage F — exact i2pr reverse delivery

Only after Java-side dispatch/tunnel stages are proven, observe i2pr.

Require correlation to the exact expected reverse TunnelData / response
payload for this response epoch.

Evaluate:

```text
expected reverse TunnelData received
→ tunnel recovery
→ Garlic decode
→ Destination/Streaming delivery
→ connection response state
→ payload bytes/digest
```

Canonical terminals:

```text
P244-G-I2PR-NO-EXPECTED-REVERSE-TUNNELDATA
P244-G-I2PR-REVERSE-TUNNEL-RECOVERY-FAILED
P244-G-I2PR-REVERSE-GARLIC-DECODE-FAILED
P244-G-I2PR-REVERSE-STREAMING-ADAPTER-FAILED
P244-G-REVERSE-DIRECTION-ESTABLISHED
```

This is the first stage where a production i2pr corrective may become
authorized, and only if exact expected reverse TunnelData reaches i2pr and a
specific i2pr-owned stage then fails.

If no exact expected reverse TunnelData reaches i2pr, production change remains
forbidden.

## 13. Bidirectional Streaming pass

If `P244-G-REVERSE-DIRECTION-ESTABLISHED` is reached, record:

- Direction A retained established proof;
- Java → i2pr response establishment;
- at least one small payload/digest match in each direction;
- no sibling-Destination cross-delivery;
- close/EOF state if already observable without widening the plan.

A Plan-244 reverse-direction pass does not automatically close M6. It unblocks
Plan 201's remaining publication/final-closure work.

Do not fold new publication corrections into Plan 244.

## 14. Required implementation approach

Prefer orchestration/correlation over new probes.

Expected surfaces:

```text
crates/i2pr-daemon/tests/java_tunnel_external.rs
tests/integration/m6-interop/run-java.sh
scripts/check-m6-mixed-router-acceptance-evidence.sh
scripts/interop/check-m6-java-response-source-lock.sh
```

Reuse existing:

```text
ReferenceStreamingService REPORT_RESPONSE_STATS
P238 admission observer
P239 dispatch/lookup observer
P240 exact target-job / B-query observer
P242 extended client-tunnel observer
P243 host qualification script
```

A new Java probe is NOT authorized unless an exact same-epoch correlation
fact is impossible to obtain from retained probes. If that occurs, stop and
write a new plan rather than broadening Plan 244 silently.

## 15. Static checker requirements

Add a Plan-244 checker section that rejects:

- treating P236 placeholder fields as authoritative;
- stopping at P236 merely because the historical token is printed;
- mixing observer epochs;
- using absolute counters where a delta is required;
- using destination-lane jobs to satisfy Streaming correlation;
- using `toTry` membership as proof of query dispatch;
- evaluating B reply stages without exact B query dispatch;
- claiming lookup success without client-subDB target LS presence;
- claiming Router-A dispatch from background tunnel handoff counters;
- claiming an i2pr defect before exact expected reverse TunnelData;
- Java source/jar changes;
- RNG/selector forcing;
- topology/profile/publication/timing changes;
- production Rust changes without §12 authorization;
- raw Java logs in durable evidence;
- fail-open `|| true` / suppressed required failures.

## 16. Focused tests

Add at least equivalents of:

```text
p244_p236_token_is_historical_when_deeper_observers_exist
p244_response_epoch_binds_all_observers
p244_destination_epoch_cannot_satisfy_streaming_epoch
p244_sendmessage_precedes_router_a_admission
p244_router_a_admission_precedes_lookup
p244_lookup_requires_exact_streaming_job
p244_totry_is_not_query_dispatch
p244_nonzero_pool_zero_hop_selection_is_contradiction
p244_b_query_precedes_b_receipt
p244_b_receipt_precedes_b_answer
p244_b_answer_precedes_client_dsm
p244_client_dsm_precedes_subdb_install
p244_subdb_install_precedes_lookup_success
p244_lookup_success_precedes_ocmosj_dispatch
p244_dispatch_precedes_i2pr_reverse_tunneldata
p244_i2pr_defect_requires_exact_reverse_tunneldata
p244_reverse_pass_does_not_close_publication_axis
p244_no_production_change_without_owned_boundary
```

Retain P237–P243 focused floors.

## 17. Counted execution

After implementation is committed and host-qualified:

```bash
I2PR_M6_JAVA_DRIVER=streaming bash tests/integration/m6-interop/run-java.sh
```

Run three counted attempts on one SHA.

For each attempt record:

- host qualification reference;
- Direction-A prerequisite;
- helper response-stat deltas;
- Router-A admission/dispatch deltas;
- target-LS local/remote lookup facts;
- exact Streaming ISJ IDs;
- Router-B candidate/query facts;
- B reply/client-subDB facts if reached;
- OCMOSJ/tunnel path facts if reached;
- i2pr reverse TunnelData/recovery/Garlic/Streaming facts if reached;
- exact deepest terminal.

The deepest supported repeated live boundary governs closure.

## 18. Verification floor

Run:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
focused P237/P238/P239/P240/P241/P242/P243/P244 tests
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources
bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/interop/check-m6-java-response-source-lock.sh <exact-pinned-source> <sanitized-tsv>
bash scripts/interop/check-p243-host-qualified.sh --expected-sha <implementation-sha>
javac <all staged Java helpers against exact-pinned jars>
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
cargo test --locked --workspace --all-targets -- --test-threads=1
```

## 19. Acceptance criteria

Plan 244 closes correctly only when:

1. Plan-243 host qualification is green.
2. Direction A is proven before reverse attribution is evaluated.
3. Plan-237 helper response observations are correlated to the same response
   epoch.
4. Plan-238 Router-A admission is correlated to that same epoch.
5. Plan-239 lookup/dispatch facts are correlated to that same epoch.
6. Plan-240 exact Streaming ISJ / Router-B facts are correlated to that same
   epoch.
7. Historical P236 output does not prematurely stop deeper attribution.
8. The earliest missing live reverse stage is identified without inference.
9. No publication correction is introduced.
10. No production i2pr change is introduced before exact reverse TunnelData
    proves an i2pr-owned defect.
11. Three counted same-SHA attempts execute with no tuning.
12. Plan 201 and Plan 204 are updated together at closure.

## 20. Successor authorization

Only the exact Plan-244 terminal may authorize a successor:

- helper response not emitted → narrow stock-response corrective;
- Router-A admission missing → narrow I2CP admission corrective;
- exact lookup not correlated → narrow lookup observer corrective;
- zero-hop selected despite authoritative non-zero pool → selector
  contradiction corrective;
- B query dispatches but reply chain fails → exact lookup-return corrective;
- lookup succeeds but OCMOSJ/tunnel dispatch fails → exact Java dispatch
  corrective;
- expected reverse TunnelData reaches i2pr then fails → exact production i2pr
  corrective for that owned stage;
- reverse direction establishes → return to Plan 201 publication/final Java
  second-family closure.

Do not pre-register later branches.

## 21. Closure record

`plans/closure/mixed-router-interop/244-status.md` must record:

- implementation SHA;
- exact pins;
- Plan-243 host qualification result;
- three counted evidence directories;
- Direction-A prerequisite per attempt;
- exact response epoch binding;
- P237 response deltas;
- P238 Router-A deltas;
- P239 lookup/dispatch facts;
- P240 exact-job/B-query facts;
- post-query reply/subDB facts if reached;
- OCMOSJ/tunnel facts if reached;
- reverse i2pr TunnelData/recovery/Garlic/Streaming facts if reached;
- exact terminal per attempt;
- no-tuning proof;
- no-production-change proof;
- verification floor;
- Plan-201/204 unblock audit.

## 22. Registration disposition

```text
plan_243 = passed-m6-java-streaming-hosted-stock-client-build-qualification-with-direction-a-established
plan_244 = registered-ready-m6-java-streaming-reverse-direction-continuous-response-attribution

plan_201 = blocked-on-m6-java-streaming-reverse-direction-and-publication-closure-pending-plan244
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan244
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

active_plan = none
next_executable_plan = 244-m6-java-streaming-reverse-direction-continuous-response-attribution

milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
```

## 23. Smaller-model handoff

Do not investigate Plan 236 from scratch.

Direction A already works.

Reuse the existing Plan-237/238/239/240 observers in one hosted response epoch.

Ignore historical classifier tokens once a deeper same-epoch observer has
authoritative evidence.

Follow Java's response until the first real missing stage.

Do not change production i2pr unless exact expected reverse TunnelData reaches
i2pr and then fails at a specific i2pr-owned stage.

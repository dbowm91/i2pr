# Plan 241 — M6 Java Streaming zero-hop client-tunnel fixture corrective

Status: **registered-ready-m6-java-streaming-zero-hop-client-tunnel-fixture-corrective**

## 1. Objective

Remove the now-proven synthetic zero-hop blocker from the Java Streaming
response lane, using only the already-qualified controlled Java topology and
stock client-tunnel machinery, then resume the exact Plan-240 target-LS lookup
chain.

Plan 240 closed with:

```text
P240-C-B-ZERO-HOP-UNKNOWN-RI
```

on three counted executions. The exact Streaming target ISJ correlated,
Router B was eligible and present in every `toTry`, and the first B-specific
guard in `IterativeSearchJob.sendQuery()` rejected the lookup because the
selected outbound tunnel was zero-hop while the helper client NetDB facade
did not contain B's RouterInfo.

The current Streaming helper explicitly forces that state:

```text
inbound.length=0
outbound.length=0
inbound.quantity=1
outbound.quantity=1
inbound.backupQuantity=0
outbound.backupQuantity=0
inbound.allowZeroHop=true
outbound.allowZeroHop=true
```

Therefore Plan 241 is a bounded **test-fixture corrective**. It does not
authorize production i2pr changes.

## 2. Exact source basis

Pinned Java I2P remains:

```text
2.13.0
9134f808337b401e8e53c73734c81fab04280c9d
```

Pinned i2pd remains:

```text
2.61.0
635b013a612ff47278ef02acf8580a28e10e26c5
```

Exact-pinned Java establishes:

### 2.1 Client lookup tunnel selection

For a client NetDB lookup with `fromLocalDest != null`,
`IterativeSearchJob.sendQuery()` selects the client outbound/inbound pools
for that destination.

### 2.2 Zero-hop guard

Before DLM construction:

```text
if outbound tunnel length <= 1:
    if peer == key -> self-lookup stop
    if client facade lacks peer RI -> zero-hop-unknown stop
```

Plan 240 proves the second branch.

### 2.3 TunnelPool selection semantics

`TunnelPoolManager.selectOutboundTunnel(destination, closestTo)` selects from
that destination's client outbound pool.

`TunnelPool.selectTunnel(closestTo)` sorts zero-hop tunnels last **only when
zero-hop is disallowed by pool settings**. When zero-hop is allowed, a
zero-hop fallback may be selected by ordinary closest-far-end ordering.

The current Streaming helper explicitly allows and requires zero-hop
(length=0), so the Plan-240 guard is expected fixture behavior.

## 3. Retained Plan-230 authority

Do not invent a new client-tunnel bootstrap.

Plan 230 already demonstrated the controlled topology can naturally produce:

```text
non-zero exploratory tunnels in both directions
one-hop client inbound tunnel through Router C
one-hop client outbound tunnel through Router C
zero-hop client tunnel absent
ordinary target-LS lookup success
forward i2pr -> Java digest-matched delivery
```

when the exact controlled topology satisfies its reachability/profile
predicates.

Retain the Plan-230 stock correction model:

- Router A = service/floodfill;
- Router B = publication/floodfill;
- Router C = transit/non-floodfill;
- controlled loopback reachability correction only as already authorized;
- Router C bandwidth/profile bootstrap only through the exact Plan-230
  mechanism;
- no profile injection;
- no direct tunnel installation;
- no NetDB seeding;
- no Java source patch.

## 4. Scope

Plan 241 may change only the **Streaming helper test fixture** and the
test-only observability/checker surfaces needed to prove the correction.

In scope:

1. remove the forced zero-hop Streaming client tunnel profile;
2. request the same bounded one-hop client profile already used and qualified
   by the controlled Java topology;
3. require the Plan-230 readiness/bootstrap predicates before treating the
   Streaming attempt as authoritative;
4. prove installed Streaming helper client tunnel shape before sending the
   Direction-A SYN;
5. re-run the exact Plan-240 Streaming target-ISJ correlation;
6. continue through target-LS lookup / DLM dispatch only if the zero-hop guard
   clears;
7. resume Plan-239 OCMOSJ / dispatch attribution only if target-LS lookup
   succeeds;
8. stop at the first exact downstream boundary.

Out of scope:

- changing raw-Destination helper settings;
- changing production Rust;
- changing Java reference source;
- direct RI/LS/profile/tunnel injection;
- `netDb.alwaysQuery`;
- forcing Router B into lookup candidates;
- search-limit or timeout increases;
- arbitrary topology changes;
- bypassing the zero-hop guard;
- changing Streaming protocol semantics;
- changing publication semantics except later evidence-driven work owned by
  Plan 201;
- SAM pivot.

## 5. Streaming helper corrective

The current `ReferenceStreamingService.options()` zero-hop profile is the
fixture defect.

Replace only the Streaming helper's forced zero-hop settings with a bounded
one-hop profile consistent with the retained controlled topology.

Required shape:

```text
inbound.length=1
outbound.length=1
inbound.lengthVariance=0
outbound.lengthVariance=0
inbound.quantity=1
outbound.quantity=1
inbound.backupQuantity=0
outbound.backupQuantity=0
inbound.allowZeroHop=false
outbound.allowZeroHop=false
```

If the exact public I2CP option spelling differs on the pinned Java version,
use the spelling already proven by Plan 227/230 rather than inventing a new
surface.

If an existing explicit-peer option is required by the retained controlled
fixture to route client tunnels through Router C, reuse exactly that already-
qualified Plan-227/230 mechanism. Do not add a new peer-selection override.

The raw-Destination helper remains unchanged so Plan-232 authority is not
requalified by fixture drift.

## 6. Pre-SYN qualification gates

The Streaming helper must not send or receive the counted SYN until all
required controlled-Java prerequisites are proven.

### A — Plan-230 Router-C eligibility

Re-prove, read-only:

```text
Router C RI present/validated on A
C non-floodfill
C reachability/profile creation predicate eligible
C naturally present in the required profile/tier population
C selectable and not banlisted
```

If not:

```text
P241-A-C-NOT-CLIENT-TUNNEL-ELIGIBLE
```

and stop.

### B — non-zero exploratory prerequisite

Reuse Plan-229/230 observation.

Require:

```text
A inbound exploratory nonzero >= 1
A outbound exploratory nonzero >= 1
```

If not:

```text
P241-B-NONZERO-EXPLORATORY-NOT-READY
```

and stop.

### C — Streaming helper client pool

After helper session connect, prove:

```text
client inbound count >= 1
client outbound count >= 1
client inbound nonzero count >= 1
client outbound nonzero count >= 1
client zero-hop inbound count = 0
client zero-hop outbound count = 0
```

For the one-tunnel fixture, additionally prove the unique installed client
tunnel is exactly one-hop and uses Router C where the retained explicit-peer
fixture requires it.

If inbound missing:

```text
P241-C-STREAMING-CLIENT-TUNNEL-NOT-BUILT direction=inbound
```

If outbound missing:

```text
P241-C-STREAMING-CLIENT-TUNNEL-NOT-BUILT direction=outbound
```

If any zero-hop remains:

```text
P241-C-ZERO-HOP-STILL-INSTALLED
```

No counted Streaming lookup begins before C passes.

## 7. Router-B RI visibility discriminator

Immediately before the Streaming SYN, record Router B independently in:

```text
A main NetDB:
  raw RI present
  validated RI present
  RI hash / age bucket

helper client NetDB facade:
  lookupLocallyWithoutValidation(B) present
  entry type if present
```

Do not write either store.

This distinction matters because Plan 240 proved B is known by the router
generally while the exact zero-hop guard uses the client facade.

With a non-zero-hop client outbound tunnel, client-facade RI absence is
diagnostic only and must not be "fixed" unless the stock lookup path later
requires it.

## 8. Plan-240 replay

After A/B/C gates pass, re-run the exact Plan-240 Streaming lookup correlation:

```text
helper DBID
+ target hash
+ exact target ISJ job ID(s)
+ Streaming epoch
```

Require:

```text
B lookup-candidate eligible
B in exact toTry
zero-hop-unknown guard = false
```

If the exact lookup still selects zero-hop despite C proving a valid non-zero
client outbound pool:

```text
P241-D-ZERO-HOP-SELECTED-DESPITE-NONZERO-POOL
```

This is an observability/fixture contradiction. Stop; do not patch Java.

If the one-hop pool is not available at exact sendQuery time despite having
passed pre-SYN:

```text
P241-D-NONZERO-CLIENT-TUNNEL-LOST-BEFORE-LOOKUP
```

and stop.

If the guard clears and B query dispatch occurs:

```text
P241-D-B-QUERY-DISPATCHED
```

then continue to §9.

## 9. Post-query continuation

Reuse the retained Plan-225/240 order:

```text
B receives exact target DLM
-> B current/query-answerable LS2
-> B emits answer
-> A helper inbound client tunnel receives reply DSM
-> helper client sub-DB installs target LS
-> OCMOSJ lookup-success callback
```

Possible terminals:

```text
P241-E-B-LOOKUP-NOT-RECEIVED
P241-E-B-TARGET-LS-NOT-QUERY-ANSWERABLE
P241-E-B-ANSWER-NOT-EMITTED
P241-E-A-CLIENT-DSM-NOT-RECEIVED
P241-E-A-CLIENT-SUBDB-NOT-INSTALLED
P241-E-TARGET-LEASESET-LOOKUP-PASSED
```

Do not infer B-answer failure from A-side absence alone.

## 10. OCMOSJ continuation

Only after:

```text
P241-E-TARGET-LEASESET-LOOKUP-PASSED
```

resume the retained Plan-239 classifier:

```text
target lease selected
-> client outbound tunnel selected
-> garlic constructed
-> DispatchJob
-> dispatchOutbound
-> client.dispatchTime / dispatchSendTime
```

Possible continuation terminals:

```text
P241-F-OCMOSJ-NO-USABLE-LEASE
P241-F-OCMOSJ-NO-OUTBOUND-TUNNEL
P241-F-OCMOSJ-GARLIC-PREP-FAILED
P241-F-DISPATCH-OUTBOUND-PROVEN
```

If dispatch is proven, reuse the retained Plan-231/232 tunnel path:

```text
A outbound gateway
-> C transit
-> target IBGW
-> i2pr expected TunnelData
-> recovery
-> Garlic
-> Streaming adapter
```

No i2pr-owned defect may be claimed before exact expected TunnelData reaches
i2pr.

## 11. Direction-A establishment

If the Streaming SYN-ACK / response reaches i2pr and the connection
establishes:

```text
P241-G-DIRECTION-A-ESTABLISHED
```

record it, but do **not** close M6.

Plan 201 still owns the remaining second-family/publication/final rows.

## 12. Fixture-corrective authorization limits

Plan 240 plus exact helper settings authorize only this correction:

```text
Streaming helper: forced zero-hop -> bounded one-hop stock client profile
```

They do not authorize:

- changing global Java exploratory/client tunnel defaults;
- changing raw helper settings;
- changing A/B/C IPs;
- changing B publication behavior;
- storing B RI into the helper client DB;
- disabling the pinned zero-hop safety guard;
- production i2pr changes.

If the one-hop helper cannot naturally establish under the retained Plan-230
topology, stop at the bootstrap terminal and do not weaken the test.

## 13. Observation reuse

Prefer existing surfaces:

- P227/P228 client tunnel config/build/install observations;
- P229 exploratory settings/install observations;
- P230 C eligibility/profile/bootstrap observations;
- P239 Router-A OCMOSJ counters;
- P240 exact target-job/B lookup trace.

Add only the smallest read-only Streaming-helper pool snapshot required to
distinguish:

```text
installed client tunnel count
installed non-zero count
installed zero-hop count
unique tunnel hop count
unique first/last peer when safe and required
pool settings length/variance/allowZeroHop/quantity
```

Do not add another general-purpose tunnel probe if P227/P228/P230 already
expose the required facts.

## 14. Source-lock additions

Extend the exact source lock only for facts consumed by Plan 241:

```text
TunnelPoolManager.selectOutboundTunnel(destination, closestTo)
client outbound pool lookup
TunnelPool.selectTunnel(closestTo)
avoidZeroHop = !settings.getAllowZeroHop()
TunnelInfoComparator zero-hop-last behavior
IterativeSearchJob client selectOutboundTunnel(fromLocalDest, peer)
IterativeSearchJob zero-hop unknown-RI guard
```

Retain all Plan-236–240 source-lock rows.

## 15. Required focused tests

Add at least equivalent rows:

```text
p241_streaming_helper_no_longer_forces_zero_hop
p241_raw_helper_profile_unchanged
p241_requires_plan230_c_eligibility_before_streaming
p241_requires_nonzero_exploratory_before_streaming
p241_requires_nonzero_client_inbound_before_streaming
p241_requires_nonzero_client_outbound_before_streaming
p241_rejects_any_zero_hop_streaming_client_tunnel
p241_records_main_vs_client_facade_b_ri_independently
p241_plan240_job_correlation_retained
p241_zero_hop_guard_must_clear_before_query_claim
p241_b_query_required_before_b_receipt
p241_b_receipt_required_before_b_answer
p241_b_answer_required_before_a_client_dsm
p241_client_dsm_required_before_subdb_install
p241_lookup_success_required_before_ocmosj_resume
p241_dispatch_required_before_downstream_tunnel_claim
p241_i2pr_owned_terminal_requires_expected_tunneldata
p241_direction_a_established_does_not_close_m6
p241_no_production_change
```

Retain P227/P228/P229/P230/P239/P240 focused tests.

## 16. Attempt discipline

- commit implementation before counted runs;
- max three counted attempts per implementation SHA;
- no between-attempt tuning;
- fresh disposable A/B/C RouterContexts;
- same exact pins;
- frozen 45-second Streaming response window;
- parser/probe defect requires a new SHA and restarted attempt budget;
- bootstrap failure is a valid terminal, not permission to weaken gates;
- raw Java logs remain scratch-only.

## 17. Verification floor

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets

cargo test --locked -p i2pr-daemon --test java_tunnel_external p227_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p228_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p229_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p230_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p239_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p240_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p241_ -- --test-threads=1

cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources

bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/interop/check-m6-java-response-source-lock.sh <exact-pinned-source> <sanitized-tsv>

javac <all staged Java helpers against exact-pinned jars>
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
```

Attempt:

```bash
cargo test --locked --workspace --all-targets -- --test-threads=1
```

Record any unrelated failure honestly; do not modify another subsystem under
Plan 241.

## 18. Acceptance criteria

Plan 241 closes correctly when:

1. The Streaming helper no longer forces zero-hop client tunnels.
2. The raw helper remains unchanged.
3. Plan-230 C eligibility and non-zero exploratory prerequisites are proven.
4. Streaming helper client inbound/outbound tunnels are installed non-zero-hop
   before the SYN.
5. Plan-240 exact target-job correlation is retained.
6. The zero-hop-unknown guard either clears or an exact contradiction/loss
   terminal is emitted.
7. If B is queried, the retained lookup reply/install chain is evaluated in
   order.
8. OCMOSJ resumes only after lookup success.
9. No i2pr defect is claimed before exact expected TunnelData.
10. No production Rust or Java source changes occur.
11. The same terminal repeats on counted execution or variability is explicitly
    recorded.
12. Plan 201 / Plan 204 authority is updated together at closure.

## 19. Successor authorization

Plan 241 may close at several layers.

- If one-hop Streaming client tunnels cannot establish under the retained
  Plan-230-qualified topology: successor may address only that controlled
  fixture/bootstrap boundary.
- If one-hop tunnels exist but Java still selects zero-hop: observability /
  selector contradiction successor only.
- If the B query dispatches but reply/install fails: successor may address only
  that exact lookup return boundary.
- If target LS lookup passes but OCMOSJ fails later: resume the existing
  Plan-239/231 attribution at that stage.
- If Direction A establishes: return to Plan 201's remaining bidirectional,
  publication, refresh, close/EOF, isolation, wrapper, and final M6 closure
  rows.

No production i2pr corrective is pre-authorized.

## 20. Closure evidence

`plans/closure/mixed-router-interop/241-status.md` must record:

- implementation SHA(s);
- exact pins;
- exact Streaming helper option diff;
- proof raw helper unchanged;
- Plan-230 C/profile/exploratory gates;
- Streaming helper pool settings and installed tunnel shape;
- Router-B main-vs-client-facade RI snapshots;
- exact Plan-240 target job IDs / B membership / zero-hop guard state;
- B query/reply/install facts if reached;
- OCMOSJ/dispatch/downstream facts if reached;
- per-attempt terminal;
- no-production-change proof;
- focused/full verification;
- Plan-201/204 unblock audit.

## 21. Registration disposition

```text
plan_240 = passed-m6-java-streaming-target-leaseset-lookup-failure-attribution-with-b-zero-hop-unknown-ri-boundary
plan_241 = registered-ready-m6-java-streaming-zero-hop-client-tunnel-fixture-corrective

plan_201 = blocked-after-plan240-b-zero-hop-unknown-ri-pending-plan241-and-publication-closure
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan241
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 241-m6-java-streaming-zero-hop-client-tunnel-fixture-corrective
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
```

## 22. Smaller-model handoff

Do not change Java routing code.

The current Streaming helper asks for zero-hop tunnels. Fix only that fixture,
using the already-proven one-hop controlled topology.

Before sending the Streaming SYN, prove:

```text
C eligible/profiled
non-zero exploratory pair exists
Streaming helper client inbound tunnel is non-zero
Streaming helper client outbound tunnel is non-zero
zero-hop client tunnels are absent
```

Then replay Plan 240.

If the B lookup dispatches, follow the existing P225/P240 reply path.

If it does not, stop at the first exact reason.

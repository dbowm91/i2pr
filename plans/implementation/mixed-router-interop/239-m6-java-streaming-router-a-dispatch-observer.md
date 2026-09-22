# Plan 239 — M6 Java Streaming Router-A pre-dispatch / OCMOSJ attribution

Status: **registered-ready-m6-java-streaming-router-a-dispatch-observer**

## 1. Objective

Attribute the first Router-A stage *after* the Plan-238 proven I2CP admission
of the Java Streaming response and stop at the earliest exact
LeaseSet / tunnel / dispatch boundary.

Plan 238 proved on three identical counted runs that:

```text
helper SchedulerReceived acted
response packet was constructed
I2PSession.sendMessage returned
Router A client.distributeTime delta = 3
Router A client.dispatchTime delta = 0
Router A client.dispatchSendTime delta = 0
```

The retained historical classifier token is:

```text
P237-D-CLIENT-MESSAGE-NOT-ADMITTED
```

but Plan 238's executed evidence proves Router-A I2CP admission occurred.
For Plan 239, interpret that token only as the retained **post-admission /
pre-dispatch boundary**. Do not re-litigate admission.

No production Rust or pinned Java source change is authorized.

## 2. Exact pins and retained authority

Keep unchanged:

```text
Java I2P 2.13.0
9134f808337b401e8e53c73734c81fab04280c9d

i2pd 2.61.0
635b013a612ff47278ef02acf8580a28e10e26c5
```

Retain:

- Plan 232 route-derived lease-gateway correction and bidirectional raw-Destination pass;
- Plan 234/235 Java accept/socket-surface boundaries;
- Plan 236/237 source lock and real stock helper response observations;
- Plan 238 Router-A `client.distributeTime` admission proof;
- frozen topology, profile policy, publication target, and 45-second windows;
- fail-closed evidence semantics.

## 3. Exact-pinned Router-A ordering

Pinned `OutboundClientMessageOneShotJob` establishes the following ordering.

### 3.1 Constructor

The OCMOSJ constructor first performs:

```java
_leaseSet = ctx.clientNetDb(_from.calculateHash())
               .lookupLeaseSetLocally(toHash);
```

Therefore a zero `client.leaseSetFoundRemoteTime` delta does **not** prove
that no target LeaseSet was available. A local LeaseSet bypasses the remote
lookup-success stat entirely.

### 3.2 runJob

If no usable local LeaseSet is already available, OCMOSJ starts a client-NetDB
lookup.

Remote lookup success records:

```text
client.leaseSetFoundRemoteTime
```

Remote lookup failure records:

```text
client.leaseSetFailedRemoteTime
```

before the failure terminal.

### 3.3 Lease selection

`SendJob.runJob()` calls `getNextLease()`, which may fail before tunnel
selection for:

- target LS absent;
- only unacceptable received-as-published state;
- unsupported/bad LeaseSet type;
- encryption-key incompatibility;
- no usable leases.

### 3.4 Tunnel / garlic preparation

After a usable target lease is selected, `send()` calls
`selectOutboundTunnel(_to)`.

If no outbound tunnel is available, stock Java increments:

```text
client.dispatchNoTunnels
```

and logs the source-locked reason equivalent to:

```text
Could not find any outbound tunnels to send the payload through
```

The same `client.dispatchNoTunnels` stat is also incremented if garlic
construction later fails because required tunnel material is unavailable,
with a distinct stock log equivalent to:

```text
Unable to create the garlic message (no tunnels left or too lagged)
```

Those two cases MUST NOT be conflated.

### 3.5 Dispatch

If preparation succeeds:

```text
DispatchJob.runJob()
  -> tunnelDispatcher().dispatchOutbound(...)
  -> client.dispatchTime
  -> client.dispatchSendTime
return to send()
  -> client.dispatchPrepareTime
```

All four are exact-pinned source assertions and must be added to / retained by
the source lock.

## 4. Scope

Plan 239 is attribution-only.

In scope:

1. exact helper client-NetDB target-LS snapshot on Router A;
2. remote LS lookup success/failure counters;
3. exact installed helper client inbound/outbound tunnel snapshot;
4. `client.dispatchNoTunnels`, `client.dispatchPrepareTime`,
   `client.dispatchTime`, `client.dispatchSendTime` epoch deltas;
5. sanitized exact-source log counters sufficient to distinguish the two
   `dispatchNoTunnels` branches and pre-dispatch LeaseSet failures;
6. continuation into existing Plan-231/232 tunnel attribution **only if**
   `dispatchOutbound` is proven.

Out of scope:

- topology/profile/tunnel-policy changes;
- direct NetDB/tunnel/profile injection;
- Java source patches;
- reflection/private mutation;
- SAM/I2CP behavior changes;
- timeout inflation;
- production Rust changes;
- publication/final-closure reconciliation except as unchanged blockers.

## 5. Observation surface

Reuse existing read-only patterns before adding anything new:

- P224 local/client-subDB LeaseSet snapshot semantics;
- P231 installed client-tunnel snapshot semantics;
- P238 `StatManager` lifetime-event probe pattern;
- exact-pinned stock Java logger extraction.

A single additive `P239-DISPATCH` command may report only bounded facts:

```text
target_ls_local_present
target_ls_current
target_ls_type
target_ls_received_as_published
target_ls_received_as_reply

lease_lookup_found_remote_events
lease_lookup_failed_remote_events

client_outbound_tunnel_count
client_inbound_tunnel_count
client_outbound_send_id_if_unique
client_inbound_receive_id_if_unique

dispatch_no_tunnels_events
dispatch_prepare_events
dispatch_time_events
dispatch_send_time_events

log_no_outbound_tunnel_count
log_garlic_no_tunnel_count
log_local_ls_missing_count
log_only_rap_ls_count
log_bad_or_unsupported_ls_count
```

Use `-1`/Unknown for a rate or fact that cannot be read. Never convert
Unknown into zero.

No destination hash, packet contents, keys, tags, payloads, or raw log lines
enter durable evidence.

## 6. Epoch discipline

Take Router-A pre snapshot immediately before the proven Direction-A Streaming
response epoch and post snapshot at the end of the frozen 45-second window.

Use deltas only for cumulative counters.

The lane must remain quiet with respect to other client sends during the epoch.
If that cannot be proven, stop:

```text
P239-A-ROUTER-A-EPOCH-NOT-ISOLATABLE
```

The helper process remains fresh per counted attempt.

## 7. Ordered classifier

Plan 238 admission is a prerequisite. Then classify in this order.

### D1 — target LeaseSet decision

If a current usable local target LS is present pre/post, record local-LS path.

Else if:

```text
lease_lookup_found_remote_delta > 0
```

record remote-LS-success path.

Else if:

```text
lease_lookup_failed_remote_delta > 0
```

emit:

```text
P239-D-TARGET-LEASESET-LOOKUP-FAILED
```

Else if exact stock logs prove a local LS rejection, emit the matching terminal:

```text
P239-D-TARGET-LEASESET-UNUSABLE
```

Otherwise:

```text
P239-D-TARGET-LEASESET-DECISION-UNKNOWN
```

and stop.

### D2 — outbound tunnel / garlic preparation

Once a usable target LS is proven:

If `dispatch_no_tunnels_delta > 0` and source-locked log evidence says the
outbound-tunnel selection failed:

```text
P239-D-NO-OUTBOUND-TUNNEL
```

If `dispatch_no_tunnels_delta > 0` and source-locked log evidence says garlic
construction lacked required tunnel material:

```text
P239-D-GARLIC-TUNNEL-MATERIAL-UNAVAILABLE
```

If `dispatch_no_tunnels_delta > 0` but the branch cannot be distinguished:

```text
P239-D-NO-TUNNELS-BRANCH-AMBIGUOUS
```

Do not infer the branch from installed-tunnel counts alone.

### D3 — dispatch

If `dispatch_time_delta > 0` and `dispatch_send_time_delta > 0`,
`dispatchOutbound(...)` returned and Router-A dispatch is proven.

`dispatch_prepare_delta > 0` is corroboration that inline
`DispatchJob.runJob()` returned to `send()`.

Then emit continuation:

```text
P239-D-DISPATCH-OUTBOUND-PROVEN
```

and continue to §8.

If target LS + tunnel preparation are proven but dispatch remains zero:

```text
P239-D-PRE-DISPATCH-OBSERVABILITY-GAP
```

Do not call this an i2pr defect.

## 8. Continuation after dispatch is proven

Only after `P239-D-DISPATCH-OUTBOUND-PROVEN` may the plan reuse the existing
Plan-231/232 controlled-route observations:

```text
Router A outbound gateway enqueue
-> Router C / transit OBEP processing
-> route-derived target IBGW present
-> target IBGW dispatch
-> i2pr exact TunnelData
-> tunnel recovery
-> Garlic decode
-> StreamingDestinationAdapter
```

Stop at the first proven stage:

```text
P239-E-OUTBOUND-GATEWAY-NOT-ENQUEUED
P239-E-TRANSIT-NOT-PROCESSED
P239-E-TARGET-IBGW-NOT-PRESENT
P239-E-TARGET-IBGW-NO-DISPATCH
P239-E-I2PR-NO-EXPECTED-TUNNELDATA
P239-F-I2PR-TUNNEL-RECOVERY-FAILED
P239-F-I2PR-GARLIC-DECODE-FAILED
P239-F-I2PR-STREAMING-ADAPTER-FAILED
P239-F-DIRECTION-A-ESTABLISHED
```

No i2pr-owned terminal is legal before exact expected TunnelData reaches i2pr.

## 9. Required source-lock additions

Extend `scripts/interop/check-m6-java-response-source-lock.sh` to prove, on
the exact Java pin:

- constructor local `lookupLeaseSetLocally(toHash)`;
- `client.leaseSetFoundRemoteTime`;
- `client.leaseSetFailedRemoteTime`;
- `client.dispatchNoTunnels`;
- distinct no-outbound-tunnel log branch;
- distinct garlic-no-tunnel log branch;
- `client.dispatchPrepareTime`;
- `tunnelDispatcher().dispatchOutbound`;
- `client.dispatchTime`;
- `client.dispatchSendTime`.

The source-lock TSV shape is additive; retained Plan-236/237/238 rows remain.

## 10. Required tests

Add focused tests equivalent to:

```text
p239_plan238_admission_is_prerequisite
p239_zero_remote_lookup_delta_does_not_mean_no_leaseset
p239_local_leaseset_path_precedes_remote_lookup_interpretation
p239_remote_lookup_failure_precedes_tunnel_attribution
p239_unknown_rate_is_not_zero
p239_dispatch_no_tunnels_requires_branch_discriminator
p239_outbound_tunnel_failure_distinct_from_garlic_tunnel_failure
p239_dispatch_time_proves_dispatch_outbound_returned
p239_dispatch_prepare_is_corroboration_not_primary_dispatch_proof
p239_post_dispatch_stages_require_dispatch_outbound
p239_i2pr_owned_terminal_requires_expected_tunneldata
p239_no_production_change
```

Full `p236_`, `p237_`, and `p238_` focused suites remain green.

## 11. Static checker requirements

Reject:

1. reinterpreting Plan-238 admission as unproven;
2. treating `leaseSetFoundRemoteTime == 0` as proof of no target LS;
3. treating Unknown/`-1` as zero;
4. treating `client.dispatchNoTunnels` alone as proof of which tunnel branch failed;
5. claiming dispatch from `tunnel.dispatchOutboundTunnel` background traffic alone;
6. a post-dispatch stage without `client.dispatchTime`/equivalent exact evidence;
7. any i2pr-owned failure without expected TunnelData;
8. Java source patches;
9. production Rust Plan-239 surface;
10. timeout/topology/pin changes;
11. raw log promotion;
12. `|| true` or `REQUIRED_FAILED` forgiveness.

## 12. Attempt discipline

- commit implementation before counted execution;
- maximum three counted attempts per implementation SHA;
- no tuning between attempts;
- parser/observer defect requires a new SHA and new attempt budget;
- pre-commit shakedowns are not closure evidence;
- evidence dirs are unique per attempt.

## 13. Verification floor

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-daemon --test java_tunnel_external p236_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p237_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p238_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p239_ -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources
bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/interop/check-m6-java-response-source-lock.sh <exact-pinned-source> <sanitized-tsv>
javac <all staged Java helpers against exact-pinned jars>
```

Attempt the full serial workspace floor:

```bash
cargo test --locked --workspace --all-targets -- --test-threads=1
```

If the historical SAM stall returns, record the exact incomplete token; do not
modify SAM under Plan 239.

## 14. Final-closure guard

Direction-A establishment does not close M6.

If Direction A establishes, return to the retained Plan-234/201 authority:

- Direction-A bidirectional small + multi-packet payload;
- Direction-B Java -> i2pr connect/accept;
- live route-derived refresh/republication;
- close/EOF/sibling isolation;
- full `run-java.sh` exit 0;
- Plan-200/201 §C/D pass-or-stronger-evidence reconciliation;
- final M6 checker;
- exact-head workspace floor.

## 15. Acceptance criteria

Plan 239 closes correctly when:

1. Plan-238 Router-A admission remains proven.
2. Target-LS local vs remote-lookup state is measured without zero-counter inference.
3. Any `dispatchNoTunnels` event is branch-disambiguated or explicitly left ambiguous.
4. Dispatch is claimed only from exact `dispatchTime` / `dispatchSendTime` evidence.
5. The earliest D/E/F terminal repeats on counted execution.
6. No production Rust or Java source changes occur.
7. Plan-232 raw-Destination authority remains intact.
8. Verification is represented honestly.
9. Plan-201/204 authority is updated together at closure.

## 16. Closure evidence

`plans/closure/mixed-router-interop/239-status.md` must record:

- implementation SHA(s);
- exact pins;
- exact source-lock rows;
- target-LS observation mechanism;
- pre/post LS, lookup, tunnel, and dispatch observations;
- per-attempt deltas;
- exact terminal;
- retained Plan-237/238 evidence;
- no-production-change proof;
- focused/full verification;
- Plan-201/204 unblock audit.

## 17. Registration disposition

```text
plan_238 = passed-m6-java-streaming-router-a-admission-observer-with-client-message-not-admitted-boundary
plan_239 = registered-ready-m6-java-streaming-router-a-dispatch-observer

plan_201 = blocked-after-plan238-client-message-not-admitted-pending-router-a-dispatch-observer-and-publication-closure
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan239
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 239-m6-java-streaming-router-a-dispatch-observer
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
```

## 18. Smaller-model handoff

Do not start by changing tunnels.

First prove which OCMOSJ branch the three admitted Streaming response messages
take.

For each counted epoch answer, in order:

```text
Was the target LS already present in the helper client NetDB?
If not, did the remote LS lookup succeed or fail?
Did getNextLease reach a usable target lease?
Did dispatchNoTunnels increment?
If so, which exact source branch emitted it?
Did dispatchOutbound run and return?
```

Stop at the first proven missing stage.

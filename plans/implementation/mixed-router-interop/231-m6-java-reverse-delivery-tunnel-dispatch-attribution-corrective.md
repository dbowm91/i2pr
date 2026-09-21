# Plan 231 — M6 Java reverse-delivery tunnel-dispatch attribution corrective

Status: **registered-ready-m6-java-reverse-delivery-tunnel-dispatch-attribution-corrective**

## 1. Objective

Close the exact remaining Java second-family boundary after Plan 230:

```text
Java helper tracked reverse send -> ACCEPTED
target LS2 resolved
one-hop Java client tunnels through C installed
i2pr -> Java forward payload digest matched
Java -> i2pr reverse payload absent inside frozen 45 s window
no later terminal send status
```

Plan 231 is a narrow attribution pass. It MUST determine the first missing stage
between Java's already-proven outbound-client dispatch call and i2pr's inbound
TunnelData / Garlic / Destination delivery surface.

It MUST NOT reopen the now-closed Plan-227/228/229/230 bootstrap questions unless
the new evidence directly contradicts a retained gate.

The intended stage chain is:

```text
tracked Java client send
  -> OCMOSJ target lease + outbound tunnel selected
  -> OCMOSJ DispatchJob calls TunnelDispatcher.dispatchOutbound()
  -> Java A outbound gateway accepts/enqueues the GarlicMessage
  -> Java A one-hop client tunnel emits TunnelData to C
  -> Java C outbound endpoint decrypts/reassembles the message
  -> C forwards TunnelGatewayMessage to target LS2 lease gateway
  -> target Java inbound gateway accepts the exact lease tunnel id
  -> target Java inbound gateway emits TunnelData toward i2pr
  -> i2pr SSU2 receives the exact inbound TunnelData
  -> i2pr tunnel decrypt/reassembly recovers Garlic
  -> i2pr Garlic/Destination dispatcher recovers the tracked payload
```

The plan closes with either a proven reverse-delivery pass or one exact earliest
missing stage. It is not allowed to stop at a generic "Java accepted but nothing
arrived" classification.

## 2. Registration basis

Plan 230 closed as:

```text
passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary
```

Its deepest counted run proved:

- exact Java `heardAbout()` eligibility after the evidence-gated C1+C2 fixture
  correction;
- natural Router-C profile bootstrap;
- genuine non-zero exploratory tunnels;
- genuine one-hop Java client tunnels through C in both directions;
- target Standard LS2 resolution through the real client-tunnel NetDB path;
- digest-matched i2pr -> Java Destination delivery;
- tracked Java reverse send with `ordered_statuses=[1]`
  (`STATUS_SEND_ACCEPTED`), but no matching reverse payload at i2pr in 45 s.

No `NO_LEASESET` or other terminal status followed the accepted send.

## 3. Exact-pinned source ordering that changes the investigation

Java I2P remains pinned at
`9134f808337b401e8e53c73734c81fab04280c9d`.

Exact-pinned source establishes a stronger ordering than the retained Plan-218
wording:

1. `ClientMessageEventListener.handleSendMessage()` calls
   `ClientConnectionRunner.distributeMessage()` first.
2. The pinned code comment states the `ClientMessagePool` runs
   `OutboundClientMessageOneShotJob` inline.
3. OCMOSJ runs its `DispatchJob` inline.
4. `DispatchJob.runJob()` calls:

   ```java
   getContext().tunnelDispatcher().dispatchOutbound(
       _msg,
       _outTunnel.getSendTunnelId(0),
       _lease.getTunnelId(),
       _lease.getGateway());
   ```

   before returning.
5. Only after `distributeMessage()` returns does
   `handleSendMessage()` call `ackSendMessage(...)`, which emits
   `STATUS_SEND_ACCEPTED`.

Therefore a nonce-correlated `ACCEPTED` is downstream of the OCMOSJ
`dispatchOutbound()` call returning. Plan 231 MUST NOT classify the failure as
"OCMOSJ never dispatched".

However, `TunnelDispatcher.dispatchOutbound()` may still return after:

- failing to find the outbound gateway;
- enqueueing into `PumpedTunnelGateway` without the pumper ever emitting the
  message;
- enqueueing and later dropping/expiring before the expected downstream stage.

Those are the earliest Java-side distinctions Plan 231 must now make.

Additional pinned source anchors:

- `TunnelDispatcher.dispatchOutbound(... targetTunnel, targetPeer)` increments
  `tunnel.dispatchOutboundTunnel` only after `gw.add(...)` on a matching
  outbound gateway.
- `PumpedTunnelGateway.add()` increments its message count, enqueues into its
  prequeue, and increments `tunnel.dropGatewayOverflow` on queue overflow.
- Router C's one-hop outbound endpoint path is
  `OutboundTunnelEndpoint.dispatch()`; its participating `HopConfig` increments
  `getProcessedMessagesCount()` before decrypt/reassembly.
- The destination lease gateway accepts a `TunnelGatewayMessage` through
  `TunnelDispatcher.dispatch(TunnelGatewayMessage)`; on a matching inbound
  gateway it calls `gw.add(msg)` and increments `tunnel.dispatchInbound`.
- `joinInboundGateway(HopConfig)` places that exact gateway `HopConfig` in
  `listParticipatingTunnels()`, and `InboundGatewayReceiver.receiveEncrypted()`
  increments the config's processed-message count before constructing and
  enqueueing the next-hop `TunnelDataMessage`.

These public/read-only counters and config snapshots are the preferred
observability surface. Raw Java logs remain scratch-only.

## 4. Invariants

1. Java and i2pd pins remain frozen.
2. Plan-230 controlled topology and C1+C2 fixture corrections remain unchanged.
3. No production Rust behavior change is authorized by the attribution phase.
4. No Java source patching, bytecode patching, reflection, private-field access,
   or direct internal mutation.
5. No direct profile, NetDB, tunnel, message-pool, or queue injection.
6. No `netDb.alwaysQuery`, public I2P, reseed, VMComm, distinct-/24 topology,
   timeout inflation, forced bandwidth class, or SAM pivot.
7. The reverse tracked send stays exactly one bounded application payload with a
   nonce and digest.
8. The 45-second reverse payload acceptance window remains frozen.
9. The later 70-second status-only window remains status-only and may not
   retroactively pass payload delivery.
10. Plan-230 profile/exploratory/client-tunnel gates must pass before the Plan-231
    reverse epoch starts.
11. Raw Java logs remain scratch-only. Durable evidence may contain bounded
    booleans, counts, tunnel ids, router hashes, message ids, lengths, digests,
    and enumerated stage tokens only.
12. Do not infer target-message progress solely from a global counter that may
    also move because of exploratory tests or unrelated tunnel traffic.
13. M10 product authority remains closed and unchanged.

## 5. Work package A — source-order and reverse-epoch contract

Add a Plan-231 classifier and a single reverse-send epoch around the already
existing `SEND_TRACKED` call.

The reverse epoch MUST record:

```text
nonce
payload_len
payload_sha256
tracked_send_start_ms
accepted_observed
accepted_observed_ms
target_ls_hash
target_lease_gateway_hash
target_lease_tunnel_id
java_outbound_client_send_tunnel_id
java_outbound_client_first_hop_hash
```

The target LS2 must contain the same current lease used by OCMOSJ. If more than
one current lease exists, expose the exact selected lease or stop with an
observability gap; do not assume the first lease.

The harness must lock the exact source-order invariant:

```text
ACCEPTED => distributeMessage returned
distributeMessage returned => inline OCMOSJ returned
inline OCMOSJ returned => DispatchJob.runJob returned
DispatchJob.runJob returned => dispatchOutbound call returned
```

This proves only call ordering. It does not prove queue acceptance, pumper
progress, transit, or delivery.

## 6. Work package B — Java A outbound-gateway attribution

Add a read-only Plan-231 diagnostic snapshot on Router A immediately before and
after the tracked reverse send.

Record at minimum lifetime-event-count snapshots/deltas for:

```text
client.dispatchTime
client.dispatchSendTime
tunnel.dispatchOutboundTunnel
tunnel.dropGatewayOverflow
```

Also retain the exact installed outbound client tunnel selected for this send
(send tunnel id + first-hop C identity) and prove it is still installed at the
reverse epoch.

Use the existing exact-pinned OCMOSJ INFO event only as scratch correlation for
the target destination/message. Sanitize it into a bounded target-dispatch
boolean/message-id row; do not promote destination strings, keys, tags, or full
GarlicMessage rendering.

Classification:

```text
P231-A-OUTBOUND-GATEWAY-NOT-FOUND
P231-A-OUTBOUND-GATEWAY-ENQUEUE-DROP
P231-A-OUTBOUND-GATEWAY-ENQUEUED
P231-A-OBSERVABILITY-GAP
```

Rules:

- `ACCEPTED` plus no target OCMOSJ correlation is an observability gap, not
  "not dispatched".
- `tunnel.dispatchOutboundTunnel` must advance in the isolated target-send epoch
  to prove the matching gateway accepted `gw.add(...)`.
- Any `tunnel.dropGatewayOverflow` increment attributable to the same epoch maps
  to ENQUEUE-DROP.
- Background counter movement may support but may not independently satisfy a
  target-specific stage.

## 7. Work package C — Java C outbound-endpoint attribution

The installed Java client outbound tunnel is one hop through Router C. Use
Router C's public `TunnelDispatcher.listParticipatingTunnels()` /
`HopConfig` surface to identify the exact outbound-endpoint config by the
tunnel ids/peer identities already proven by Plan 230.

Capture before/after:

```text
c_obep_config_present
c_obep_receive_tunnel_id
c_obep_receive_from_hash
c_obep_processed_messages
c_tunnel_dispatch_endpoint_lifetime_count
```

A positive target epoch requires both:

- the exact C outbound-endpoint config remains present; and
- its processed-message count increases after Router A's target enqueue.

If it does not:

```text
P231-B-FIRST-HOP-NOT-RECEIVED-BY-C
```

If C processes the TunnelData but source-visible reassembly/forwarding cannot be
distinguished with the public surface, add only a bounded scratch-log sanitizer
for `OutboundTunnelEndpoint` / `OutboundMessageDistributor`. Do not patch Java
to expose private queues.

Next terminals:

```text
P231-B-C-OBEP-REASSEMBLY-FAILED
P231-B-C-OBEP-FORWARD-PASSED
P231-B-OBSERVABILITY-GAP
```

The forward-passed row must carry the intended target lease gateway hash and
target tunnel id from WP A.

## 8. Work package D — destination Java inbound-gateway attribution

Resolve `target_lease_gateway_hash` to the controlled Java router role. Plan 231
must not assume it is Router B solely from historical topology comments.

On the matching controlled router, use
`TunnelDispatcher.listParticipatingTunnels()` to locate the inbound-gateway
`HopConfig` whose receive tunnel equals `target_lease_tunnel_id`.

Record before/after:

```text
ibgw_role
ibgw_config_present
ibgw_receive_tunnel_id
ibgw_send_tunnel_id
ibgw_send_to_hash
ibgw_processed_messages
tunnel.dispatchInbound lifetime delta
tunnel.dropGatewayOverflow lifetime delta
tunnel.inboundLookupSuccess lifetime delta
```

Interpretation:

- missing exact inbound-gateway config:
  `P231-C-TARGET-IBGW-NOT-INSTALLED`
- C forward passed but `tunnel.dispatchInbound` does not advance and the exact
  gateway sees no processed-message increase:
  `P231-C-TUNNEL-GATEWAY-NOT-RECEIVED`
- inbound gateway accepts but its processed count does not advance or queue
  overflow is observed:
  `P231-C-IBGW-ENQUEUE-OR-PUMP-BOUNDARY`
- processed count advances but next-hop RouterInfo lookup fails:
  `P231-C-IBGW-NEXT-HOP-LOOKUP-FAILED`
- processed count advances with no lookup failure:
  `P231-C-IBGW-TUNNELDATA-EMITTED`

This stage is critical because exact-pinned `InboundGatewayReceiver` constructs
the next-hop `TunnelDataMessage` only after the inbound-gateway queue has been
processed.

## 9. Work package E — i2pr receive/decrypt/reassembly attribution

Extend only the external test/evidence path around the existing frozen
45-second reverse loop. Do not modify production behavior to make the test pass.

The current loop collapses several distinct outcomes because it silently
continues after decode and tunnel recovery errors. Replace that loss of
information with bounded counters for the reverse epoch:

```text
ssu2_datagrams_received_delta
i2np_messages_received_delta
tunneldata_messages_seen
expected_inbound_tunneldata_seen
unexpected_tunneldata_seen
tunneldata_decode_failures
tunnel_recovery_successes
tunnel_recovery_failures
garlic_decode_successes
garlic_decode_failures
destination_dispatch_calls
destination_payload_queue_hits
destination_payload_digest_match
```

For each TunnelData message, compare the tunnel id against the exact owned
inbound tunnel receive id for the advertised target LS2. Do not treat unrelated
TunnelData as target progress.

If safe structured error categories already exist in
`recover_garlic_bytes()`, record their bounded enum names. Otherwise record only
stage counts; do not stringify secret-bearing cryptographic state.

Terminals:

```text
P231-D-I2PR-NO-EXPECTED-TUNNELDATA
P231-D-I2PR-TUNNEL-RECOVERY-FAILED
P231-D-I2PR-GARLIC-DECODE-FAILED
P231-D-I2PR-DESTINATION-DISPATCH-MISSED
P231-D-I2PR-PAYLOAD-MISMATCH
P231-D-REVERSE-DELIVERY-PASSED
P231-D-OBSERVABILITY-GAP
```

If WP C proves `P231-C-IBGW-TUNNELDATA-EMITTED` but i2pr sees no exact
TunnelData, the next boundary is transport/wire delivery between that Java
inbound gateway and i2pr. Only then may a successor investigate SSU2 transport
delivery.

If i2pr sees the exact TunnelData, this plan must continue through
decrypt/reassembly/Garlic/Destination attribution in the same run.

## 10. Work package F — exactly one earliest-stage classifier

Emit exactly one `p231-classification` row per counted attempt, using the first
known failing stage in this order:

```text
A  Java A outbound gateway / enqueue
B  Java C first-hop / outbound endpoint / forward
C  target Java inbound gateway / TunnelData emission
D  i2pr exact TunnelData / recovery / Garlic / Destination delivery
PASS reverse payload digest matched
```

Unknown at an earlier stage is an observability gap and prevents a later stage
from being claimed as root cause.

Required terminal vocabulary is limited to the tokens defined in ``6–9 plus:

```text
P231-REVERSE-DELIVERY-PASSED
```

Do not emit a new root-cause label from free-form logs.

## 11. Attempt discipline

Implementation must be committed before counted execution.

Use the Plan-230 corrected controlled topology unchanged. The lane remains
destination-only until reverse raw-Destination delivery passes.

Maximum three counted attempts per implementation SHA. No between-attempt
tuning. Infrastructure death before the Plan-231 reverse epoch is VOID and may
be retried without consuming a protocol classification only when no Plan-231
terminal was emitted.

Do not extend the 30-second profile gate, five-minute helper/tunnel ceiling,
45-second reverse payload window, or 70-second status-only window.

## 12. Required focused tests

Add unit/static coverage for at least:

```text
p231_accepted_is_ordered_after_inline_dispatch_call
p231_accepted_alone_does_not_prove_gateway_enqueue
p231_gateway_stat_delta_requires_target_epoch
p231_gateway_overflow_maps_to_enqueue_drop
p231_c_obep_requires_exact_installed_tunnel
p231_c_obep_count_delta_proves_first_hop_processing
p231_target_gateway_role_comes_from_selected_lease
p231_ibgw_requires_exact_target_tunnel_id
p231_ibgw_processed_delta_precedes_tunneldata_emitted
p231_unrelated_tunneldata_cannot_satisfy_i2pr_stage
p231_expected_tunnel_id_required_for_recovery_stage
p231_tunnel_recovery_failure_is_distinct_from_no_wire_receive
p231_garlic_failure_is_distinct_from_tunnel_recovery_failure
p231_destination_queue_hit_requires_digest_match_for_pass
p231_status_only_after_45s_cannot_pass_payload_delivery
p231_first_unknown_stage_maps_to_observability_gap
p231_exactly_one_terminal_per_counted_run
p231_secret_bearing_log_rows_rejected
```

## 13. Static evidence guards

Extend `scripts/check-m6-mixed-router-acceptance-evidence.sh` so it rejects:

- a Plan-231 classifier that treats `ACCEPTED` as proof of final delivery;
- a classifier that says OCMOSJ "never dispatched" after nonce-correlated
  `ACCEPTED` without contradicting the pinned source-order lock;
- global counter deltas used as sole target-message evidence;
- target inbound-gateway claims without exact lease gateway + tunnel-id match;
- i2pr target-TunnelData claims without exact owned inbound tunnel id;
- probe-side Java mutations;
- Java source/reflection/private-field access;
- timeout/window changes;
- topology/profile/tunnel-policy changes relative to Plan 230;
- raw Java logs promoted to durable evidence;
- public I2P/reseed/VMComm/`netDb.alwaysQuery`.

## 14. Verification floor

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ssu2-vectors.sh
bash scripts/check-i2cp-vectors.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-streaming-tunnel-evidence.sh
cargo test --locked -p i2pr-daemon --test java_tunnel_external p231_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p230_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p229_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external --no-run
bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
javac <all Plan-231 probes + existing probes + ControlledRouter against exact staged Java jars>
```

A closing implementation SHA MUST receive the full workspace test floor; do not
carry forward Plan 230's exact-head full-test exception.

## 15. Acceptance criteria

Plan 231 closes only when:

1. exact Java pins and Plan-230 controlled topology remain unchanged;
2. source-order evidence is locked so `ACCEPTED` cannot be misinterpreted;
3. the exact selected target lease gateway and tunnel id are observed;
4. Java A target OCMOSJ/gateway enqueue stage is classified;
5. Router C's exact one-hop OBEP processing stage is classified;
6. the exact target Java inbound-gateway stage is classified;
7. i2pr's exact owned inbound tunnel id is used to distinguish target from
   unrelated TunnelData;
8. i2pr tunnel recovery, Garlic decode, Destination dispatch, and payload queue
   are separately observed if the wire message arrives;
9. exactly one earliest terminal is emitted per counted attempt;
10. no timeout inflation or fixture tuning is used;
11. no production corrective is made unless this attribution proves an
    i2pr-owned defect;
12. full routine + focused verification passes on the closing implementation
    SHA;
13. closure record, registry, M6 roadmap, Plan-201 blocker, and Plan-204
    convergence state are updated together.

## 16. Successor rules

### Reverse delivery passes

If `P231-REVERSE-DELIVERY-PASSED` is reproduced with digest match, immediately
resume the remaining Java Streaming rows owned by Plan 201. Do not register
another raw-Destination plan.

### Boundary remains wholly inside stock Java controlled-router forwarding

If the earliest boundary is A/B/C and no i2pr wire event exists, close Plan 231
as a reference-side/harness attribution. A corrective may change only the
controlled fixture behavior proven incorrect by exact source/evidence; it may
not change i2pr production code.

### Java emits expected target TunnelData toward i2pr, i2pr never receives it

Register a narrow SSU2 wire-delivery attribution under Plan 201. Preserve all
Plan-230/231 Java gates.

### i2pr receives exact target TunnelData but fails recovery/Garlic/Destination

That is an i2pr-visible protocol/product boundary. Register the narrow production
corrective at the exact failing stage, with a regression vector from the
captured sanitized message metadata where possible.

## 17. Closure evidence

`plans/closure/mixed-router-interop/231-status.md` must record:

- implementation SHA(s);
- exact Java/i2pd pins;
- Plan-230 topology/correction retention proof;
- source-order proof;
- selected lease gateway/tunnel;
- Java A pre/post target epoch;
- Java C exact OBEP pre/post counts;
- exact target IBGW pre/post counts;
- i2pr reverse-epoch receive/decode/recovery/dispatch counters;
- nonce, length, and payload digest only;
- ordered helper status list;
- exact terminal per counted attempt;
- full exact-head verification;
- Plan-201/204 unblock audit.

## 18. Registration disposition

```text
plan_230 = passed-m6-java-reachability-capability-profile-bootstrap-corrective-with-reverse-delivery-boundary
plan_231 = registered-ready-m6-java-reverse-delivery-tunnel-dispatch-attribution-corrective

plan_201 = blocked-pending-plan231-reverse-delivery-tunnel-dispatch-attribution-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan231
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 231-m6-java-reverse-delivery-tunnel-dispatch-attribution-corrective
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed

milestone10_final_acceptance = closed
```

## 19. Handoff notes for smaller-model execution

Do not change any topology or timeout first.

Start by implementing the reverse-epoch evidence contract and exact source-order
lock. The previous assumption that `ACCEPTED` merely meant "the router accepted
the request" is now too weak: on the pinned source it occurs after the inline
OCMOSJ dispatch call has returned.

Use the installed tunnel identities already proven by Plan 230. Correlate the
single tracked reverse payload from A -> C -> selected lease gateway -> i2pr.
Prefer exact per-tunnel processed-message deltas over global logs. Use global
stats only as supporting evidence.

On the i2pr side, stop silently collapsing decode/recovery failures into
"nothing arrived". The key value of this plan is to distinguish no wire
delivery from an i2pr tunnel/Garlic delivery defect.

Once the earliest stage is known, stop. Do not optimize Java timing, add peers,
alter profile policy, or expand the harness again.

# Plan 237 — M6 Java Streaming stock-response observability corrective

Status: **registered-ready-m6-java-streaming-stock-response-observability-corrective**

## 1. Objective

Close the Plan-236 observability defect without expanding the Java interop architecture.

Plan 236 source-locked the exact Java I2P 2.13.0 response path but did not actually observe it. The helper hard-coded all response-stage fields to false and therefore correctly stopped at:

```text
P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP
```

Plan 237 must replace those placeholders with real, bounded observations from the stock Java implementation and identify the first missing stage among:

```text
SchedulerReceived.eventOccurred
-> response/ACK construction
-> Connection.sendPacket
-> PacketQueue.enqueue
-> I2PSession.sendMessage
-> Router-A I2CP/client-message admission
-> existing Plan-231/232 tunnel path
-> i2pr exact TunnelData
```

This is a harness/observability corrective first. Do not modify production Rust or the pinned Java implementation.

## 2. Exact pins and retained authority

Keep unchanged:

```text
Java I2P 2.13.0
9134f808337b401e8e53c73734c81fab04280c9d

i2pd 2.61.0
635b013a612ff47278ef02acf8580a28e10e26c5
```

Retain:

- Plan 232 route-derived lease-gateway correction;
- Plan 232 bidirectional raw-Destination pass;
- Plan 234 Java accept-returned boundary;
- Plan 235 usable Java `I2PSocket` surface;
- Plan 236 source-lock ordering;
- frozen topology, profile policy, publication target, and timing windows.

## 3. Root defect to correct

Current `ReferenceStreamingService REPORT_STREAM_STATE` emits literal placeholders:

```text
java_response_observation_complete=false
java_response_scheduler_observed=false
java_response_packet_constructed=false
java_sendpacket_observed=false
java_packetqueue_observed=false
java_i2psession_send_observed=false
...
```

Those values are not measurements. They are an honest Unknown encoding, but they cannot distinguish a Java response failure from a missing observer.

Plan 237 MUST NOT classify a response stage from these literal placeholders.

## 4. Prefer existing stock Java observability

Use the minimum existing signals from the exact-pinned implementation before adding any new probe.

### 4.1 Scheduler signal

Pinned `SchedulerReceived.eventOccurred()` already emits DEBUG strings:

```text
received con... send a packet
received con... time till next send: <n>
```

These may prove the response scheduler reached its send/reschedule branch.

### 4.2 ACK construction signal

Pinned `Connection.ackImmediately()` emits:

```text
sending new ack: <PacketLocal>
```

when it constructs the small response ACK via the receiver/send path.

### 4.3 PacketQueue / I2PSession completion signal

Pinned `PacketQueue.enqueue()` executes:

```text
I2PSession.sendMessage(...)
_context.statManager().addRateData("stream.con.sendMessageSize", ...)
```

The exact-pinned `ConnectionManager` creates `stream.con.sendMessageSize` as a `RateStat`, and public `RateStat.getLifetimeEventCount()` is available.

A positive lifetime-event delta during the isolated SYN epoch is therefore valid evidence that `I2PSession.sendMessage(...)` returned past the send call. It does not by itself prove Router-A dispatch.

### 4.4 Failure signals

Retain exact-pinned stock warnings such as:

```text
Unable to send the packet
Send failed for
Took <n>ms to sendMessage(...)
```

Sanitize these to booleans/counts; raw lines remain scratch-only.

## 5. Observation architecture

Prefer one of these, in order:

1. stock helper-JVM `StatManager` lifetime counters queried through a tiny public helper command;
2. stock Java DEBUG/WARN log extraction scoped to the helper JVM and the single SYN epoch;
3. a minimal out-of-tree read-only Java helper compiled against exact pinned jars.

Do not:

- patch Java I2P source;
- use reflection/private-field mutation;
- alter router/client internals;
- inject packets or force scheduler execution;
- add another broad probe framework.

If a helper command is used, it should expose only bounded public facts such as:

```text
REPORT_RESPONSE_STATS
scheduler_log_count
ack_constructed_log_count
send_message_size_lifetime_events
send_failure_count
send_exception_count
```

No packet contents, keys, tags, plaintext payloads, or private state.

## 6. Epoch discipline

Capture a baseline immediately before the Direction-A SYN is sent and a second snapshot at the end of the existing frozen response window.

Use deltas, not absolute lifetime counts:

```text
scheduler_send_delta
ack_constructed_delta
send_message_event_delta
send_failure_delta
send_exception_delta
```

The Java Streaming helper must be fresh for the counted lane so unrelated historical Streaming traffic cannot satisfy a positive delta.

If the process cannot guarantee an isolated helper-JVM epoch, stop with:

```text
P237-A-RESPONSE-OBSERVATION-NOT-ISOLATABLE
```

## 7. Required response classifier

Populate the existing Plan-236 response state from real observations and classify in this order:

```text
P237-B-SCHEDULER-NOT-OBSERVED
P237-B-SCHEDULER-OBSERVED-NO-ACK-CONSTRUCTION
P237-C-ACK-CONSTRUCTED-NO-SENDMESSAGE
P237-C-SENDMESSAGE-FAILED
P237-C-SENDMESSAGE-RETURNED
```

`P237-C-SENDMESSAGE-RETURNED` is a continuation state, not final closure.

Once sendMessage-returned is proven, continue into the existing Plan-231/232 Router-A attribution and classify the earliest later stage:

```text
P237-D-ROUTER-I2CP-NOT-OBSERVED
P237-D-CLIENT-MESSAGE-NOT-ADMITTED
P237-D-NO-TARGET-LEASESET
P237-D-NO-OUTBOUND-TUNNEL
P237-D-DISPATCH-NOT-CALLED
P237-D-OUTBOUND-GATEWAY-NOT-ENQUEUED
P237-E-TRANSIT-NOT-PROCESSED
P237-E-TARGET-IBGW-NOT-PRESENT
P237-E-TARGET-IBGW-NO-DISPATCH
P237-E-I2PR-NO-EXPECTED-TUNNELDATA
P237-F-I2PR-TUNNEL-RECOVERY-FAILED
P237-F-I2PR-GARLIC-DECODE-FAILED
P237-F-I2PR-STREAMING-ADAPTER-FAILED
P237-F-DIRECTION-A-ESTABLISHED
```

Do not emit a later terminal when an earlier stage is Unknown.

## 8. Response-stage evidence rules

A positive stage requires executed evidence:

- Scheduler observed: exact source-locked DEBUG event or equivalent stock stat.
- ACK constructed: exact source-locked ACK construction event.
- sendMessage returned: positive `stream.con.sendMessageSize` lifetime-event delta or equivalent stronger stock evidence.
- sendMessage failed: stock failure/exception evidence correlated to the epoch.
- Router-A stages: existing Plan-231/232 router-side observations only after Java sendMessage is proven.
- i2pr-owned failure: exact expected TunnelData must first be proven.

Absence of DEBUG logging is not automatically proof that code did not run unless Plan 237 first proves the relevant logger is enabled for that exact class.

## 9. Logging configuration

If DEBUG logs are needed, enable only the minimum exact-pinned classes:

```text
net.i2p.client.streaming.impl.SchedulerReceived
net.i2p.client.streaming.impl.Connection
net.i2p.client.streaming.impl.PacketQueue
```

Keep raw logs in scratch storage.

Durable evidence may contain only sanitized counts/booleans and source-lock metadata.

Do not enable broad DEBUG for the whole Java router/client unless the exact classes cannot be configured independently; if broad logging is unavoidable, document why and still sanitize before evidence promotion.

## 10. Required implementation changes

Expected surfaces are limited to:

```text
tests/integration/m6-interop/java/ReferenceStreamingService.java
tests/integration/m6-interop/run-java.sh
crates/i2pr-daemon/tests/java_tunnel_external.rs
scripts/check-m6-mixed-router-acceptance-evidence.sh
scripts/interop/check-m6-java-response-source-lock.sh   # only if source assertions need extension
```

A small new out-of-tree Java helper is permitted if necessary.

No production `src/` changes.

## 11. Static checker requirements

Extend the M6 checker to reject:

1. literal hard-coded Plan-237 positive response observations;
2. a Plan-237 response-stage claim without a real stock observation source;
3. response fields being satisfied solely by `accept_returned` or `socket_surface_ready`;
4. sendMessage success without a positive stock stat/log delta;
5. Router-A attribution before sendMessage-returned;
6. i2pr-owned attribution before expected TunnelData;
7. Java source patching;
8. production Rust changes under Plan 237;
9. timeout/topology/pin changes;
10. raw log promotion.

The existing Plan-236 source lock must remain required.

## 12. Required tests

Add at least equivalent focused tests:

```text
p237_placeholder_false_is_unknown_not_negative_evidence
p237_scheduler_requires_enabled_stock_observer
p237_scheduler_precedes_ack_construction
p237_ack_construction_precedes_sendmessage
p237_sendmessage_event_delta_proves_call_returned
p237_send_failure_precedes_router_attribution
p237_sendmessage_returned_does_not_prove_router_admission
p237_router_attribution_requires_sendmessage_returned
p237_i2pr_owned_terminal_requires_expected_tunneldata
p237_accept_returned_does_not_satisfy_response_stage
p237_socket_surface_ready_does_not_satisfy_response_stage
p237_no_production_change
```

## 13. Attempt discipline

- commit implementation before counted external attempts;
- maximum three counted attempts per implementation SHA;
- no between-attempt tuning;
- helper/log/stat configuration fixed before attempt 1;
- fresh isolated helper process for each counted attempt;
- if a parser/logging defect is discovered, correct it in a new commit and restart the three-attempt budget;
- pre-commit exploratory runs are not closure evidence.

## 14. Verification floor

Required focused floor:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources

cargo test --locked -p i2pr-daemon --test java_tunnel_external p236_ -- --test-threads=1
cargo test --locked -p i2pr-daemon --test java_tunnel_external p237_ -- --test-threads=1

bash -n tests/integration/m6-interop/run-java.sh
bash -n scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/interop/check-m6-java-response-source-lock.sh <exact-pinned-source> <sanitized-tsv>
javac <all staged Java helpers against exact-pinned jars>
```

The full serial workspace test must also be attempted.

If it again stalls in `sam_stream_final_acceptance`, record:

```text
P237-V-WORKSPACE-FLOOR-INCOMPLETE-SAM-HANG test=<exact-test>
```

Do not change SAM in Plan 237 and do not call the workspace floor passed.

## 15. SAM hang separation

Plan 237 does not own the recurring SAM acceptance hang.

The Java response diagnostic may close with a valid bounded result despite that separate incomplete workspace floor, provided no production code changed.

Final Java-family/M6 closure remains forbidden until the workspace floor is green or a separate SAM test-hygiene corrective closes that issue.

## 16. If Direction A establishes

If `P237-F-DIRECTION-A-ESTABLISHED` is reached, continue immediately through the retained Plan-234 qualification authority:

- Direction A bidirectional payload equality;
- Direction B Java -> i2pr connect;
- live refresh/republication route parity;
- close/EOF and sibling isolation;
- full `run-java.sh`;
- Plan-200/201 §C/D pass-or-stronger-evidence reconciliation;
- final M6 evidence checker;
- full workspace floor.

Do not declare M6 from Direction-A establishment alone.

## 17. Acceptance criteria

Plan 237 closes correctly when:

1. Plan-236 placeholder response values are no longer treated as observations.
2. The exact stock response path remains source-locked.
3. Real stock Java log/stat evidence is collected for the isolated SYN epoch.
4. The earliest response stage is identified on repeated counted execution.
5. No Java source or production Rust is patched.
6. Router-A attribution occurs only after sendMessage-returned.
7. i2pr attribution occurs only after exact expected TunnelData.
8. Plan-232 raw-Destination authority remains intact.
9. Verification results, including any SAM hang, are represented honestly.
10. Registry/roadmap/Plan-201/Plan-204 state is updated together at closure.

## 18. Closure outcomes

### A — scheduler does not act

```text
P237-B-SCHEDULER-NOT-OBSERVED
```

Register only a scheduler/timer-specific successor if warranted.

### B — scheduler acts but response is not submitted

Close at the exact B/C terminal. This remains a Java/client-side response-path boundary.

### C — sendMessage returns but Router A path fails

Close at the exact D/E terminal and register only that narrow successor.

### D — expected TunnelData reaches i2pr and i2pr fails

Close at exact F terminal and register a separate production corrective.

### E — Direction A establishes

Continue §16; no ceremonial plan is required if all retained final gates become green.

## 19. Closure evidence

`plans/closure/mixed-router-interop/237-status.md` must record:

- implementation SHA(s);
- exact pins;
- source lock;
- observation mechanism used;
- logger enablement proof if logs are used;
- pre/post stock stat counters and deltas;
- sanitized stock-log event counts;
- per-attempt exact terminal;
- retained Plan-232/235 baseline;
- proof of no production change;
- focused verification;
- workspace result or SAM-incomplete token;
- Plan-201/204 unblock audit.

## 20. Registration disposition

```text
plan_236 = passed-m6-java-streaming-response-emission-observability-gap
plan_237 = registered-ready-m6-java-streaming-stock-response-observability-corrective

plan_201 = blocked-pending-plan237-stock-response-observability-corrective
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan237
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

next_executable_plan = 237-m6-java-streaming-stock-response-observability-corrective
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed
```

## 21. Smaller-model handoff

Do not add another broad classifier.

Start with the easiest strong stock signal: expose the helper JVM's public `stream.con.sendMessageSize` `RateStat.getLifetimeEventCount()`, snapshot it immediately before the SYN and at the end of the response epoch, and prove whether the delta is positive.

In parallel, enable only the exact Streaming classes needed for the source-locked scheduler/ACK DEBUG strings.

Answer, in order:

```text
Did SchedulerReceived reach its send branch?
Was a response ACK constructed?
Did PacketQueue reach and return from I2PSession.sendMessage?
If yes, did Router A admit and dispatch that client message?
```

Stop at the first proven “no” or Unknown.

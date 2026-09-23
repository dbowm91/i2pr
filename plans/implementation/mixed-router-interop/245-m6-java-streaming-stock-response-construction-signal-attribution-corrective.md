# Plan 245 — M6 Java Streaming stock-response construction-signal attribution corrective

Status: **registered-ready-m6-java-streaming-stock-response-construction-signal-attribution-corrective**

## 1. Objective

Investigate and correct the exact Plan-244 reverse-direction boundary `P244-B-RESPONSE-PACKET-NOT-CONSTRUCTED` before treating it as a stock-Java behavioral defect.

Plan 244 established Direction A on 3/3 counted hosted attempts and bound each response epoch correctly. Every epoch showed scheduler_delta=1, ack_constructed_delta=0, send_message_event_delta=1, send_failure_delta=0, and send_exception_delta=0.

Exact-pinned Java I2P 2.13.0 source review after Plan 244 shows that the retained ack_constructed_delta signal is not a universal packet-construction signal. It is derived from Connection's `Resend in` log, which occurs only when Connection.sendPacket() schedules the retransmit timer. ACK-only packets with sequenceNum == 0 and no SYN flag take the ACK-only branch and do not schedule that timer.

The exact-pinned source provides a direct construction signal instead: ConnectionDataReceiver.buildPacket() logs `New OB pkt (acks not yet filled in): ...` after constructing PacketLocal.

Plan 245 must determine whether Plan 244 measured a real no-construction state or only an inapplicable construction proxy. If construction is genuinely absent, it must then attribute the first stock-Java predicate between SchedulerReceived.eventOccurred() and ConnectionDataReceiver.buildPacket().

No Java source patch and no production i2pr change is authorized.

## 2. Exact pins and retained authority

Retain Java I2P 2.13.0 at 9134f808337b401e8e53c73734c81fab04280c9d and i2pd 2.61.0 at 635b013a612ff47278ef02acf8580a28e10e26c5.

Retain Plan-232 route-derived lease-gateway fixture and raw reverse pass; Plan-237 stock response observer; Plan-238 Router-A admission observer; Plan-239 OCMOSJ/lookup observer; Plan-240 exact Streaming ISJ observer; Plan-242 non-zero client-pair gate and extended path observation; Plan-243 host qualification discipline; Plan-244 same-epoch response correlation; frozen A/B/C topology; frozen 45-second windows; Router-B publication target; scratch-only raw Java logs; and the no-production-change-before-exact-reverse-TunnelData rule.

Plan-244 implementation authority: 154e92d8436dbad0b020a0b19d848d2d227a22af.
Plan-244 closure authority: 951221a41150a916221b308461d8c304c97a15d2.

## 3. Exact-pinned stock source path

### 3.1 SchedulerReceived

Source-lock SchedulerReceived.accept(con): con != null, con.getLastSendId() < 0, and con.getSendStreamId() > 0.

Source-lock SchedulerReceived.eventOccurred(con): it first requires con.getUnackedPacketsReceived() > 0. When the delayed-ACK timer is due and nextSendTime > 0, the exact branch logs `received con... send a packet`, calls con.sendAvailable(), and then sets nextSendTime to -1. The log and sendAvailable() call are in the same source branch.

### 3.2 Connection.sendAvailable

Exact source: sendAvailable() calls `_outputStream.flushAvailable(_receiver, false)`. No alternate packet builder exists in this method.

### 3.3 MessageOutputStream.flushAvailable

Exact source: flushAvailable(target, false) enters the data lock, calls target.writeData(_buf, 0, _valid), then clears _valid. A zero-length buffer is explicitly allowed; the method does not return merely because _valid == 0.

### 3.4 ConnectionDataReceiver.writeData

For zero data on an already synchronized connection, doSend initially may be false. However, if con.getUnackedPacketsReceived() > 0, doSend is forced true.

If doSend is false, exact source emits at INFO: `writeData called: size=<n> doSend=false unackedReceived: <n> con: ...`.

If doSend is true, writeData calls send(), which calls buildPacket(), then Connection.sendPacket(packet).

### 3.5 Direct packet-construction signal

ConnectionDataReceiver.buildPacket() always emits at DEBUG after PacketLocal construction: `New OB pkt (acks not yet filled in): <packet> on <connection>`.

This direct build log is the authoritative construction signal for Plan 245.

### 3.6 Why the Plan-237/244 timer signal is conditional

Connection.sendPacket() treats sequenceNum == 0 with no SYN flag as ACK-only. That branch does not enter the outbound-packet/retransmit-timer block. Therefore absence of `Resend in` does not prove absence of packet construction.

Plan 245 must source-lock this distinction explicitly.

## 4. Correct the observation model before behavioral inference

Plan 245 must not begin by modifying stock helper response behavior.

Extend only the out-of-tree helper's existing public LogManager observation. Enable bounded observation for SchedulerImpl, Connection, PacketQueue, ConnectionDataReceiver, and MessageOutputStream using the same public LogManager APIs established by Plan 237.

Forbidden: Java source patching, reflection into Connection, private-field access, packet injection, direct invocation of package-private response methods, forcing ackImmediately(), forcing sendAvailable(), connection-state writes, socket behavior changes, topology/profile/publication changes, or timing-window changes.

The console buffer must be proven sufficient for the added bounded epoch signals. If it is not, increase only the observer buffer before counted execution and statically cap accepted epoch message counts.

## 5. New response-construction snapshot

Prefer extending REPORT_RESPONSE_STATS. Record pre/post and deltas for at least: scheduler_send_branch, scheduler_reschedule_branch, scheduler_no_unacked_warning, message_output_flush_nonempty, receiver_do_send_false, receiver_packet_built, connection_resend_timer, packetqueue_sendmessage, packetqueue_send_failure, and packetqueue_send_exception.

Durable evidence contains counts/booleans only. Do not persist packet dumps, arbitrary Destinations/hashes, keys/tags, raw logs, or payload bytes.

## 6. Primary Plan-245 classification

For a Direction-A-established, fully bound response epoch, classify in order.

### A. Scheduler state

If no scheduler activity: P245-A-SCHEDULER-NOT-OBSERVED.
If the scheduler reports no unacked packets: P245-A-SCHEDULER-NO-UNACKED-PACKETS.
If only the reschedule branch occurs and the send branch never occurs: P245-A-SCHEDULER-RESCHEDULED-NO-SEND-BRANCH.
If the exact send-branch log occurs, source authority proves Connection.sendAvailable() was called. Continue.

### B. writeData decision

If the exact doSend=false log occurs: P245-B-WRITEDATA-SUPPRESSED.

Record sanitized size_zero and unacked_received_zero facts, plus output_closed only if independently observable. Do not infer why unackedPacketsReceived changed; a successor may own that state transition.

### C. Direct construction

If receiver_packet_built_delta > 0, construction is proven regardless of connection_resend_timer.

If direct construction is zero while the send branch is proven and doSend=false is not observed: P245-C-BUILDPACKET-OBSERVABILITY-GAP. Do not call this a Java construction defect yet.

### D. Observer false-negative outcome

If receiver_packet_built_delta > 0 while connection_resend_timer_delta == 0, classify P245-D-PLAN244-CONSTRUCTION-PROXY-FALSE-NEGATIVE.

This supersedes only Plan 244's interpretation of the zero timer-log delta; it does not rewrite Plan-244 execution evidence.

Record whether the source-compatible packet shape is consistent with the ACK-only no-retransmit branch using sanitized booleans only.

### E. Direct construction + timer

If both direct construction and retransmit-timer deltas are positive, classify P245-D-DIRECT-CONSTRUCTION-WITH-RETRANSMIT-TIMER and continue downstream.

## 7. Optional packet-shape discrimination

Only if obtainable by bounded parsing of the existing public DEBUG packet string without persisting it, record constructed_syn, constructed_ack_only, constructed_has_payload, constructed_close, and constructed_sequence_zero.

If packet-string formatting is not stable/source-lockable, omit shape parsing. receiver_packet_built_delta alone remains sufficient to prove construction. Do not add a new Java probe solely for packet-shape access.

## 8. Resume the Plan-244 continuous response chain

If direct construction is proven, immediately resume the Plan-244 same-epoch chain rather than stopping at the observer correction.

Required ordering: direct construction -> PacketQueue/I2PSession sendMessage -> Router-A I2CP admission -> exact Streaming lookup correlation -> Router-B query/reply -> helper client-subDB install -> OCMOSJ/tunnel dispatch -> exact reverse i2pr TunnelData.

Support typed terminals at minimum: P245-E-SENDMESSAGE-NOT-OBSERVED, P245-E-ROUTER-A-I2CP-NOT-ADMITTED, P245-E-TARGET-LOOKUP-NOT-CORRELATED, P245-E-LOOKUP-ZERO-HOP-SELECTION-CONTRADICTION, P245-E-B-QUERY-DISPATCHED, P245-F-B-LOOKUP-NOT-RECEIVED, P245-F-B-ANSWER-NOT-EMITTED, P245-F-A-CLIENT-DSM-NOT-RECEIVED, P245-F-A-CLIENT-SUBDB-NOT-INSTALLED, P245-F-LOOKUP-SUCCEEDED, P245-G-OCMOSJ-DISPATCH-NOT-OBSERVED, P245-G-I2PR-NO-EXPECTED-REVERSE-TUNNELDATA, P245-G-I2PR-REVERSE-TUNNEL-RECOVERY-FAILED, P245-G-I2PR-REVERSE-GARLIC-DECODE-FAILED, P245-G-I2PR-REVERSE-STREAMING-ADAPTER-FAILED, and P245-G-REVERSE-DIRECTION-ESTABLISHED.

No later stage may override an earlier missing authoritative stage.

## 9. Production-change boundary

No production Rust corrective is authorized by a missing timer log, missing direct build log, Java doSend=false, helper sendMessage failure, Router-A admission failure, lookup failure, Router-B query/reply failure, or Java tunnel-dispatch failure.

Production i2pr correction becomes eligible only when exact expected reverse TunnelData reaches i2pr and a specific i2pr-owned recovery/Garlic/Streaming stage then fails.

## 10. Expected implementation surfaces

Prefer only: tests/integration/m6-interop/java/ReferenceStreamingService.java; crates/i2pr-daemon/tests/java_tunnel_external.rs; scripts/interop/check-m6-java-response-source-lock.sh; scripts/check-m6-mixed-router-acceptance-evidence.sh; tests/integration/m6-interop/run-java.sh.

The Java helper change is observation-only: logger enablement and sanitized counter reporting. It must not alter socket reads/writes, acceptance, SessionConfig, or response behavior. No production src/ file may change.

## 11. Source-lock additions

Extend exact-pinned source lock with SchedulerReceived.accept predicates; SchedulerReceived unacked guard/send branch/sendAvailable/setNextSendTime ordering; Connection.sendAvailable -> MessageOutputStream.flushAvailable; MessageOutputStream target.writeData even with zero valid; ConnectionDataReceiver writeData doSend logic and unacked override; ConnectionDataReceiver send -> buildPacket -> Connection.sendPacket; buildPacket isFirst/isAckOnly/New OB pkt log; and Connection.sendPacket ACK-only branch versus retransmit-timer `Resend in` branch.

The checker must reject the claim: absence of `Resend in` equals no packet constructed.

## 12. Focused tests

Add at least equivalents of:
- p245_resend_timer_is_not_universal_construction_signal
- p245_direct_build_log_is_authoritative_construction_signal
- p245_scheduler_send_branch_implies_send_available_call
- p245_flush_available_calls_write_data_even_with_zero_valid
- p245_unacked_received_forces_do_send
- p245_do_send_false_precedes_build_absence_terminal
- p245_build_log_proves_construction_without_timer
- p245_plan244_proxy_false_negative_does_not_rewrite_history
- p245_direct_construction_resumes_plan244_chain
- p245_sendmessage_requires_direct_construction
- p245_router_admission_requires_response_sendmessage
- p245_lookup_requires_exact_streaming_job
- p245_i2pr_defect_requires_expected_reverse_tunneldata
- p245_no_java_source_patch
- p245_no_production_change
- p245_no_response_behavior_change

Retain P237–P244 focused tests.

## 13. Counted execution discipline

After implementation is committed: run Plan-243 host qualification against the exact implementation SHA; run static/source-lock/focused floors; execute three counted hosted attempts on one SHA; use fresh A/B/C RouterContexts and unique evidence directories; make no between-attempt tuning; do not retry until C; do not change timing; do not inject packets; do not alter helper behavior.

Command remains: I2PR_M6_JAVA_DRIVER=streaming bash tests/integration/m6-interop/run-java.sh.

## 14. Per-attempt evidence

Record Direction-A prerequisite, response epoch binding, scheduler send/reschedule/no-unacked deltas, MessageOutputStream bounded flush observations, ConnectionDataReceiver doSend-false delta, direct buildPacket delta, old retransmit-timer delta, PacketQueue sendMessage/failure deltas, same-epoch Plan-238/239/240 facts if construction is proven, and exact deepest P245 terminal.

If direct construction is proven, continue as far downstream as retained observers permit.

## 15. Verification floor

Run cargo fmt/check; focused P237/P238/P239/P240/P241/P242/P243/P244/P245 tests; clippy with warnings denied; rustdoc/doc tests; cargo-deny; shell syntax; M6 evidence checker; exact Java response source lock; Plan-243 host qualification; javac of all staged helpers; NTCP2 harness unit discovery; and full serial workspace all-target tests.

## 16. Acceptance criteria

Plan 245 closes correctly only when: the conditional nature of the old Resend-in signal is source-locked; direct ConnectionDataReceiver.buildPacket observation is available; helper behavior remains unchanged; observations are same-epoch and delta-based; three counted same-SHA hosted attempts execute without tuning; a false-negative Plan-244 construction proxy is distinguished from genuine writeData/construction suppression; if construction is proven the retained reverse-direction chain resumes; Java source remains unpatched; no production i2pr defect is claimed before exact reverse TunnelData; and Plan 201/204 are updated together at closure.

## 17. Successor authorization

Only the exact Plan-245 result may authorize a successor:
- scheduler has no unacked packets -> narrow inbound ACK-state attribution;
- scheduler only reschedules -> narrow timer/state attribution;
- writeData suppresses send -> narrow ConnectionDataReceiver state attribution;
- direct build remains unobservable -> narrow observer corrective;
- direct build proves Plan-244 proxy false negative and downstream send fails -> resume exact response-send corrective;
- Router-A/lookup/B/tunnel stage fails after direct construction -> exact corresponding retained-stage corrective;
- expected reverse TunnelData reaches i2pr then fails -> exact production i2pr corrective may be authorized;
- reverse direction establishes -> return to Plan 201 publication/final closure.

Do not pre-register later branches.

## 18. Closure record

plans/closure/mixed-router-interop/245-status.md must record implementation SHA, exact pins, source-lock rows/checksum, logger classes/levels, buffer-capacity proof, three counted evidence directories, Direction-A/epoch-binding proof, scheduler deltas, doSend-false delta, direct build delta, old timer delta, sendMessage/failure deltas, downstream same-epoch facts if reached, exact terminal per attempt, no-tuning proof, no-Java-source-patch proof, no-production-change proof, verification floor, and Plan-201/204 unblock audit.

## 19. Registration disposition

plan_244 = passed-m6-java-streaming-reverse-direction-continuous-response-attribution-with-response-packet-not-constructed-boundary
plan_245 = registered-ready-m6-java-streaming-stock-response-construction-signal-attribution-corrective

plan_201 = blocked-on-m6-java-streaming-reverse-direction-and-publication-closure-pending-plan245
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan245
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

active_plan = none
next_executable_plan = 245-m6-java-streaming-stock-response-construction-signal-attribution-corrective
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed

## 20. Smaller-model handoff

Do not patch Java and do not assume Plan 244 found a Java defect.

The old construction signal was the Connection `Resend in` timer log. Exact source shows that signal is conditional and absent for ACK-only packets.

Add a direct observer for ConnectionDataReceiver `New OB pkt (acks not yet filled in)` and rerun the same hosted lane.

If the direct build signal appears, Plan 244's construction interpretation was a false negative; continue downstream immediately.

If it does not appear, identify the exact scheduler/writeData predicate that suppressed construction.

No production i2pr change is allowed until exact reverse TunnelData reaches i2pr.
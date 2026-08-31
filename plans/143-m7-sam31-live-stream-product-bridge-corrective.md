# Plan 143 — SAM 3.1 live STREAM product bridge corrective pass

Status: **blocked on successful Plan 142 closure**.

Depends on: Plan 142; Plan 137 lifecycle; Plan 134/129 corrected local destination + Streaming product topology.

## 1. Goal

Finish the product work that Plan 138 was originally required to deliver:

```text
SAM STREAM TCP socket
  -> permanent raw-mode handoff
  -> StreamingManager
  -> StreamingDestinationAdapter
  -> Plan 129 destination routing / ECIES / Garlic / destination tunnels
  -> authenticated-router-link-bypassed-local-seam
  -> peer local destination full inbound stack
  -> peer StreamingManager
  -> peer SAM raw socket
```

The acceptance trajectory must exercise the same Plan 129 full destination product stack in both directions. It may use the established local router-link bypass seam because live NTCP2/SSU2 is intentionally outside M7, but it may not replace the destination product stack with direct `TransportSendRequest` transfer, direct manager-to-manager calls, `CapturedOutbound`, or manually fabricated Established state.

This plan owns CONNECT/ACCEPT raw transfer, runtime driving, backpressure, fault/reliability behavior, and stream lifecycle.

## 2. Historical Plan 138 work to retain

Do not discard useful landed pieces without cause:

- `SamStreamRegistry` and bounded attachment accounting;
- `SamDestinationBridge` concepts that correctly wrap per-destination identity/routing state;
- session ID validation and full-Destination decoding;
- loopback listener and `ServerConnectionState` dispatch;
- typed STREAM status mapping;
- Plan 139 `InboundMode` / ACCEPT-FORWARD exclusion state;
- session cancellation/ownership graph.

Refactor these only where required to install the real live bridge.

## 3. Remove capture seam from product acceptance

The current `CapturedOutbound` / `record_captured` / `drain_captured_outbound` seam may remain as a low-level diagnostic or unit-test helper if it has legitimate value, but:

- it must not sit in the production STREAM success path;
- it must not be cited as acceptance evidence;
- CONNECT must not report `OK` merely because an outbound request was captured;
- tests must not fabricate the peer SYN/SYN-response or Established state using captured material.

Prefer deleting/reducing the seam if the real local dispatcher makes it redundant.

## 4. Reuse the Plan 129 full local product delivery topology

Before implementing new runtime glue, read:

- `crates/i2pr-client/tests/plan129_trajectory.rs`;
- `crates/i2pr-client/src/streaming_adapter.rs`;
- destination routing/session/tunnel delivery seams used by Plan 129;
- the current corrected Plan 130–134 regressions.

Extract or reuse the smallest non-test-specific runtime-neutral local delivery component necessary to perform this operation:

```text
OutboundDeliveryPlan
 -> existing outbound tunnel cells
 -> existing local authenticated-router-link bypass
 -> peer inbound tunnel/data processing
 -> recovered inner I2NP Data
 -> StreamingDestinationAdapter::receive(...)
```

Do not copy the Plan 129 test fixture wholesale into `i2pr-daemon`. If a reusable runtime-neutral delivery pump is missing, add it in the lowest correct existing layer with focused M6 regression tests.

A new SAM-specific packet decoder or Garlic shortcut is forbidden.

## 5. Socket mode ownership

Each SAM v3 STREAM connection is one dedicated accepted TCP socket.

The socket state machine must make command-mode and raw-mode mutually exclusive:

```text
AwaitHello
 -> UtilityReady
 -> StreamCommandPending
 -> RawStream { session_id, stream_id, connection_id }
 -> Closing
 -> Closed
```

After successful CONNECT or ACCEPT:

- drop/detach `LineReader` state for that socket;
- never parse another byte as SAM text;
- preserve any bytes already read after the terminating command newline and deliver them as the first raw application bytes;
- no PING/QUIT/second command may be recognized on that socket afterward.

This transition must be explicit in code, not an informal boolean checked in the line loop.

## 6. CONNECT state machine

Required real trajectory:

```text
new TCP socket
 -> HELLO VERSION 3.1
 -> STREAM CONNECT ID=A DESTINATION=<B PUB> [SILENT]
 -> validate A/session + B public Destination
 -> reserve attachment
 -> StreamingManager::connect(...)
 -> route SYN through StreamingDestinationAdapter
 -> full Plan129 local destination delivery to B
 -> B wildcard listener records inbound SYN
 -> peer ACCEPT path accepts and routes SYN response
 -> full reverse destination delivery to A
 -> A Streaming connection reaches Established
 -> non-silent: STREAM STATUS RESULT=OK\n
 -> silent: no status bytes
 -> permanent RawStream
```

Do not report `OK` before the M6 connection is Established.

Failure cleanup must remove the pending connection/attachment and produce the most specific SAM result available.

Connect timeout must be bounded and realistic; do not use tiny production values to satisfy tests. Test configuration may inject short deterministic deadlines.

## 7. ACCEPT state machine

Required real trajectory:

```text
new TCP socket
 -> HELLO VERSION 3.1
 -> STREAM ACCEPT ID=B [SILENT]
 -> validate session
 -> atomically claim Accepting mode / reserve attachment
 -> bind or reuse wildcard Streaming listener
 -> non-silent: STREAM STATUS RESULT=OK\n
 -> wait for inbound SYN from actual Plan129 destination path
 -> FIFO claim exactly one pending inbound Streaming connection
 -> StreamingManager::accept_inbound_syn(...)
 -> route signed SYN response through full destination path
 -> connection Established
 -> non-silent: write authenticated peer public Destination + '\n'
 -> silent: write no SAM metadata
 -> permanent RawStream
```

For SAM 3.1, choose a deterministic pending-ACCEPT policy compatible with maintained clients. The existing bounded FIFO may retain concurrent support if race-safe; otherwise fail additional waiters explicitly. Do not leave ambiguous ownership.

The peer Destination emitted on non-silent ACCEPT must come from authenticated inbound Streaming context, not from untrusted command text.

## 8. Raw TCP -> I2P Streaming direction

Implement a bounded loop per established stream.

Algorithm:

1. reserve at most one configured raw read chunk;
2. read no more than `min(raw_read_chunk, negotiated_streaming_payload_limit)`;
3. if bytes arrived, call the existing `StreamingManager::send_data(...)` using connection-owned ports/state;
4. drain emitted `TransportSendRequest`s;
5. pass every request through `StreamingDestinationAdapter::send(...)`;
6. dispatch resulting `OutboundDeliveryPlan` through the Plan 129 full local destination product path;
7. if `send_data` reports send-window/backpressure exhaustion, stop reading the TCP socket until ACK progress frees capacity;
8. account/finalize the raw chunk only after it is admitted into bounded Streaming state.

Never accumulate a growing `Vec<u8>` while waiting for send-window capacity.

Logical writes larger than one Streaming packet must be chunked by the bridge and arrive exactly ordered at the peer.

## 9. I2P Streaming -> raw TCP direction

For each inbound product delivery:

1. deliver through `StreamingDestinationAdapter::receive(...)` into the owning `StreamingManager`;
2. use `drain_delivered()`/a corrected bounded consumption seam to obtain in-order application bytes;
3. route by `ConnectionId` to the owning SAM attachment;
4. hold at most the configured per-direction SAM buffer budget;
5. write to the local TCP socket with cancellation/deadline handling;
6. free byte accounting only after successful write/consumption;
7. stop pulling additional delivered data when the local application is slow.

### 9.1 M6 integration check

Inspect whether `StreamingManager::pending_delivered` can grow beyond an acceptable bound while the SAM side is blocked.

If yes, this is a concrete product integration defect. Correct it narrowly in `i2pr-client` with a bounded delivery/consumer API and dedicated Plan 134 regression coverage. Do not compensate with a larger SAM queue.

## 10. Per-destination runtime driver

Install one bounded driver per active SAM destination or one shared bounded scheduler, consistent with existing daemon runtime conventions.

It must advance:

- `StreamingManager::poll_retransmits(now_ms)`;
- `StreamingManager::poll_acks(now_ms)`;
- connection timeout/cleanup state as exposed by current APIs;
- outbound requests emitted by those polls through the exact same `StreamingDestinationAdapter` + local destination dispatcher.

Requirements:

- no task per packet;
- no unbounded timer wheel/channel;
- driver lifetime owned by session/destination;
- control-session teardown cancels the driver;
- all driver-emitted requests obey identical routing and error handling to application sends.

Production randomness in adapter/session construction must use `OsRng` or the router's CSPRNG policy. Deterministic RNG remains test-only.

## 11. Backpressure contract

The SAM bridge must extend, not defeat, Streaming flow control.

Create explicit accounting for:

```text
local TCP read chunk
+ pending SAM outbound bytes
+ Streaming send-window bytes

Streaming delivered bytes
+ pending SAM inbound write bytes
```

No category may be unbounded.

### Required slow-reader test

1. establish A -> B via real SAM CONNECT/ACCEPT;
2. stop B's application from reading raw TCP;
3. A sends substantially more than the configured SAM inbound budget and Streaming window;
4. prove B-side SAM buffered bytes stay <= configured bound;
5. prove A-side reads/sends eventually stall from normal backpressure;
6. prove task/queue counts do not grow with input volume;
7. resume B reads;
8. prove exact ordered byte recovery and accounting returns to baseline.

### Required slow-writer test

Make the reverse local TCP write path artificially slow/blocked and prove the same ceilings.

## 12. Fault/reliability product tests

Use the existing deterministic Plan 129 local product fault seam. The test boundary remains real SAM TCP sockets.

Required:

- drop one data packet after real outbound destination processing -> RTO -> retransmit -> exact bytes;
- duplicate a delivered packet -> application exact-once;
- reorder two packets -> application ordered;
- corrupt lower-layer authenticated/ciphertext payload -> no application corruption;
- ACK progress clears retransmission tracking;
- dropped standalone ACK or data packet does not deadlock the raw bridge.

These tests are invalid if they call `StreamingManager` directly from the test after STREAM establishment except through non-secret observation hooks.

## 13. Close/reset semantics

Define and implement one deterministic full-duplex policy.

### Local raw socket EOF

Preferred baseline:

- initiate `StreamingManager::send_close()`;
- route CLOSE through the full destination path;
- allow already-delivered peer bytes to flush within a bounded close deadline;
- close local TCP after peer close/timeout;
- release attachment exactly once.

### Local TCP I/O failure

If the Streaming connection remains active and state permits, send RESET through the existing manager and product route. Then close/release the local attachment.

### Remote CLOSE

Flush already accepted in-order bytes according to bounded policy, surface EOF, release connection.

### Remote RESET

Terminate promptly; do not deliver later queued bytes.

### Parent session close

Parent cancellation may abort graceful child close. All stream attachments/tasks/drivers must terminate within the configured daemon shutdown bound.

## 14. SILENT byte correctness

Freeze exact socket transcripts.

CONNECT non-silent:

```text
STREAM STATUS RESULT=OK\n
<raw byte 0>...
```

CONNECT silent:

```text
<raw byte 0>...
```

ACCEPT non-silent:

```text
STREAM STATUS RESULT=OK\n
<peer public Destination>\n
<raw byte 0>...
```

ACCEPT silent:

```text
<raw byte 0>...
```

No tracing text or delayed status line may appear after the raw transition.

## 15. Product acceptance harness

Use Rust localhost clients that behave only through SAM TCP. They may share helper code for test ergonomics but must not import private daemon/Streaming state to move bytes.

Canonical setup:

```text
SAM service on 127.0.0.1:0
control A -> SESSION CREATE
control B -> SESSION CREATE
stream B -> ACCEPT
stream A -> CONNECT B.PUB
raw A <-> raw B
```

The internal local delivery executor may use the Plan 129 authenticated-router-link bypass seam, but every application byte must cross:

```text
Streaming packet
 -> gzip ClientPayload
 -> I2NP Data
 -> ECIES Garlic
 -> destination tunnels
 -> inverse path
```

in both directions.

## 16. Mandatory tests

### Establishment

- CONNECT/ACCEPT non-silent;
- CONNECT/ACCEPT silent;
- CONNECT reports OK only after Established;
- ACCEPT peer Destination exactly matches A;
- unknown session / malformed target / timeout results;
- raw bytes already coalesced in same TCP write after CONNECT line are not lost.

### Bytes

- zero-length lifetime then close;
- 1 byte;
- NUL/newline/non-UTF8 bytes;
- literal `STREAM CONNECT ...\n` as payload;
- negotiated payload boundary;
- payload boundary + 1 split correctly;
- multi-megabyte logical transfer through bounded chunks;
- simultaneous bidirectional traffic.

### Multiple streams/lifecycle

- two sibling streams on one session;
- closing one leaves sibling alive;
- exact stream ceiling;
- ACCEPT cancel before inbound peer;
- control socket close with active streams;
- daemon cancellation with active streams;
- repeated create/connect/close loop returns all counts to baseline.

### Reliability/backpressure

All tests from §§11–12.

## 17. Files likely changed

```text
crates/i2pr-daemon/src/sam.rs
crates/i2pr-daemon/src/sam/streams.rs
crates/i2pr-api/src/sam/streams.rs
crates/i2pr-client/src/streaming/manager.rs       # only bounded missing consumption/state seams
crates/i2pr-client/src/streaming_adapter.rs       # only if reusable dispatcher seam needs narrow support
crates/i2pr-client/tests/plan129_trajectory.rs    # regression if lower seam changes
crates/i2pr-daemon/tests/sam_stream.rs
crates/i2pr-daemon/tests/sam_loopback.rs
crates/i2pr-daemon/tests/sam_stream_product.rs    # recommended new canonical product lane
```

Do not modify NTCP2/SSU2 protocols, public network participation, or add a new orchestration framework.

## 18. Acceptance criteria

Plan 143 closes only when every item is true:

1. CONNECT and ACCEPT use dedicated real localhost TCP sockets;
2. successful STREAM sockets permanently leave line-command parsing;
3. any already-read post-command raw bytes are preserved;
4. CONNECT reports OK only after the real M6 Streaming handshake establishes;
5. non-silent ACCEPT emits authenticated peer Destination before raw bytes;
6. SILENT true/false behavior is byte-exact;
7. application bytes cross the Plan 129 full destination product stack in both directions;
8. the Plan 129 authenticated-router-link local bypass is the only lower-network shortcut in the acceptance trajectory;
9. `CapturedOutbound`, direct manager wiring, or fabricated Established state is absent from acceptance evidence;
10. arbitrary bidirectional binary bytes are exact and ordered;
11. large logical writes are chunked without violating Streaming MTU/window behavior;
12. one injected loss causes real SAM-trajectory retransmission and recovery;
13. duplicate and reorder faults preserve exact-once ordered delivery;
14. delayed ACK/retransmit polling is driven by a bounded production runtime task;
15. slow-reader and slow-writer tests prove byte ceilings and backpressure;
16. no unbounded SAM or M6 delivery queue remains at the bridge boundary;
17. close/reset/control-session cancellation are bounded and release resources exactly once;
18. sibling streams remain independent;
19. production SAM adapter randomness is cryptographically appropriate;
20. all M6 Plan 129–134 focused regressions remain green;
21. all workspace/boundary gates pass;
22. `plans/143-status.md` records exact product trajectories and sets `next_executable_plan = 144`.

## 19. Handoff checklist

```text
[ ] Plan 142 encoding compatibility passed
[ ] product local delivery seam identified/reused
[ ] capture seam removed from acceptance path
[ ] CONNECT same-socket raw transition works
[ ] ACCEPT same-socket raw transition works
[ ] A<->B exact binary bytes through full destination stack
[ ] retransmit/ACK driver active
[ ] loss/reorder/duplicate product tests pass
[ ] slow reader/writer bounds pass
[ ] close/reset/parent cleanup pass
[ ] M6 regressions green
[ ] Plan 143 status committed
```

Do not proceed to Plan 144 while any canonical STREAM acceptance test relies on test-only state fabrication.
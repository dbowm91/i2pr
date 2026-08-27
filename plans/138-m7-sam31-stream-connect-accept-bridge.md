# Plan 138 — SAM 3.1 STREAM CONNECT / ACCEPT bridge

Status: **blocked on successful Plan 137 closure**.

Depends on: Plans 135–137 and the Plan 134 Milestone 6 local product.

## 1. Goal

Implement the first real SAM application data path:

```text
SAM TCP stream socket
    -> i2pr-api STREAM adapter
    -> existing per-destination StreamingManager
    -> StreamingDestinationAdapter
    -> existing destination routing / ECIES / Garlic / tunnel product seam
    -> peer local destination
    -> reverse path
    -> SAM TCP stream socket
```

Support the SAM 3.1 `STREAM CONNECT` and `STREAM ACCEPT` semantics, including `SILENT`, while preserving the exact Milestone 6 Streaming state machine and its resource/ACK/backpressure guarantees.

This plan is the core product milestone. Do not implement STREAM by constructing a second byte-stream protocol above destination messages.

## 2. Existing APIs are authoritative

Before modifying `i2pr-client`, inventory the current Plan 129–134 public APIs and use them directly where possible. The present implementation already contains the needed concepts:

- per-destination `StreamingManager`;
- `listen(port)` and listener backlog;
- `connect(...)` returning a pending SYN state;
- inbound SYN admission and `accept_inbound_syn(...)`;
- `send_data(...)` with transactional size/send-window validation;
- `drain_outbound()` / retransmit / delayed-ACK polling;
- `drain_delivered()` for in-order application bytes;
- `send_close(...)` and `send_reset(...)`;
- `StreamingDestinationAdapter` for the real destination-routing boundary.

If a narrow API is missing for observing connection state or accepting the next listener entry, add the smallest runtime-neutral method to `i2pr-client`. Do not expose internal maps or add SAM-specific types to `i2pr-client`.

Any M6 API change must receive focused regression tests in `i2pr-client` proving existing Plan 129–134 behavior is unchanged.

## 3. SAM v3 socket semantics

Each virtual stream has a dedicated client-to-SAM TCP socket.

### CONNECT

Client flow:

```text
open TCP socket
HELLO VERSION MIN=3.1 MAX=3.1
STREAM CONNECT ID=<session> DESTINATION=<public-destination> [SILENT=true|false]
```

For `SILENT=false` (default), i2pr must send one final command line before raw mode:

```text
STREAM STATUS RESULT=OK
```

or an appropriate failure such as `CANT_REACH_PEER`, `INVALID_KEY`, `INVALID_ID`, `TIMEOUT`, or `I2P_ERROR`, then close on failure.

For `SILENT=true`, do not send a STREAM STATUS line. Success transitions directly to raw byte mode; failure closes the socket.

After success, the line parser is permanently detached from that socket. Every following byte is application data, even if it contains newlines or looks like a SAM command.

### ACCEPT

Client flow:

```text
open TCP socket
HELLO VERSION MIN=3.1 MAX=3.1
STREAM ACCEPT ID=<session> [SILENT=true|false]
```

For `SILENT=false`:

1. send `STREAM STATUS RESULT=OK` after the accept request is registered;
2. wait for one inbound Streaming SYN;
3. accept the I2P Streaming connection through `StreamingManager`;
4. send a line containing the peer public Destination and terminating newline;
5. transition the socket permanently to raw byte mode.

For `SILENT=true`, no status or peer-destination line is sent; once the inbound connection succeeds, raw forwarding begins.

If the session closes after RESULT=OK but before an inbound peer arrives, close the accept socket; an optional late `STREAM STATUS RESULT=I2P_ERROR` may be emitted only if the chosen behavior is documented and interoperable. Prefer the simplest deterministic policy compatible with maintained clients.

## 4. SAM 3.1 port policy

SAM 3.1 CONNECT/ACCEPT does not expose the SAM 3.2 `FROM_PORT`/`TO_PORT` options. Use I2P Streaming port 0 / wildcard semantics for this baseline.

Recommended mapping:

- each SAM STREAM session binds one wildcard `StreamingManager::listen(0)` for ACCEPT/FORWARD admission;
- outbound CONNECT uses local port 0 and remote port 0;
- reject 3.2-only port options under negotiated 3.1 rather than accepting and accidentally advertising partial 3.2 behavior.

If the current Streaming manager cannot support multiple pending ACCEPTs correctly through one wildcard listener, add the minimum queue/observation API while retaining its existing hard backlog ceiling.

Do not weaken Plan 130/131 port-tuple validation.

## 5. Destination resolution for CONNECT

The `DESTINATION` field may be a full Base64 Destination. The official ecosystem also commonly permits hostnames / b32 strings, but Plan 139 owns naming integration.

Plan 138 mandatory CONNECT target:

- strict full public Destination string;
- validate through existing `i2pr-proto` Destination codec;
- derive remote destination hash and signing/static public material needed by `StreamingManager::RemoteDestination`;
- obtain/install the remote LeaseSet2 through the existing local product routing cache/test seam used by Plan 127/129.

For names/b32 not yet resolvable, return the correct explicit failure (`INVALID_KEY` vs reachability/not-found as defined by the parser/handler mapping). Do not implement an ad hoc DNS/address book in this plan.

Tests should use the exact public Destination returned by another SAM session.

## 6. Stream attachment ownership

A stream socket references an existing session by ID but is independently owned.

Create a bounded per-session stream registry with entries conceptually containing:

```text
SamStreamId (local API-only opaque handle)
Streaming ConnectionId
attachment task cancellation
state = Connecting | WaitingAccept | Raw | Closing | Closed
peer public destination metadata
buffer accounting
```

Do not expose `ConnectionId` as a SAM protocol value.

Invariants:

- attachment count never exceeds configured per-session ceiling;
- one attachment owns one Streaming connection;
- a socket failure tears down only that stream unless the parent control session is already stopping;
- control-session cancellation cancels every attachment;
- no attachment can be created after session begins stopping;
- reservation/insertion is transactional;
- exact-once cleanup decrements stream counts and releases buffered-byte reservations.

## 7. Outbound CONNECT state machine

Required trajectory:

```text
HELLO complete
 -> validate session ID and target Destination
 -> reserve stream attachment slot
 -> resolve/build RemoteDestination
 -> call StreamingManager::connect(...)
 -> route emitted SYN through StreamingDestinationAdapter
 -> wait for the normal M6 inbound SYN-response path
 -> observe connection Established
 -> emit STREAM STATUS OK unless SILENT
 -> switch socket to Raw
```

Timeout:

- define one bounded connect timeout appropriate to SAM semantics;
- do not choose an unrealistically tiny value merely to make tests fast;
- production timeout is config-driven, tests use deterministic/short injected settings;
- timeout must cancel the pending Streaming connection and all attachment state;
- map to `STREAM STATUS RESULT=TIMEOUT` when non-silent.

Failure mappings must be explicit. For example:

- bad session -> INVALID_ID;
- malformed destination -> INVALID_KEY;
- no LeaseSet/routing path -> CANT_REACH_PEER or appropriate typed mapping;
- internal product error -> I2P_ERROR;
- admission/backpressure failure before establishment -> deterministic status, not panic.

Do not report OK before the underlying Streaming connection is established.

## 8. ACCEPT registration state machine

`STREAM ACCEPT` attaches a waiting local TCP socket to the session's wildcard Streaming listener.

Required trajectory:

```text
HELLO complete
 -> validate session ID
 -> reject if FORWARD active (Plan 139 state placeholder)
 -> reserve pending-accept/stream slot
 -> ensure wildcard Streaming listener exists
 -> register one pending accept waiter
 -> non-silent: send STREAM STATUS OK
 -> wait for one inbound SYN connection ID
 -> atomically claim exactly one pending inbound connection
 -> call accept_inbound_syn(...)
 -> route SYN response through StreamingDestinationAdapter
 -> capture peer public Destination from authenticated inbound SYN context
 -> non-silent: write peer Destination line
 -> Raw mode
```

Multiple pending ACCEPT policy:

The current ecosystem supports concurrent accepts even under SAM 3.1, but M7 should only claim this if the implementation is unambiguous and bounded. Preferred implementation is to support multiple pending accepts up to `max_pending_accepts_per_session` with FIFO waiter assignment, because the current Streaming manager already has a bounded backlog.

If this creates substantial race complexity, implement one pending ACCEPT in the first pass and return the canonical `ALREADY_ACCEPTING`-style behavior supported by the selected compatibility target, then expand within this plan before Plan 140 if independent clients require concurrency.

Whatever policy is chosen must be documented and tested under contention.

## 9. The raw byte bridge

After CONNECT/ACCEPT success, split conceptual responsibilities into two bounded directions.

### Local TCP -> I2P Streaming

Algorithm:

1. read at most a fixed chunk no larger than the negotiated Streaming payload limit and configured SAM read chunk;
2. before reading further, ensure per-stream pending-byte budget has capacity;
3. call `StreamingManager::send_data(...)` with the connection-owned port tuple;
4. if send window reports backpressure, stop reading from TCP and wait/poll for capacity/ACK progress;
5. drain/route `TransportSendRequest` through the existing `StreamingDestinationAdapter` only;
6. resume reads when capacity becomes available.

Do not buffer an arbitrary `Vec<u8>` while waiting for the Streaming send window. At most one bounded read chunk plus explicitly accounted pending bytes may be held.

### I2P Streaming -> local TCP

Algorithm:

1. process inbound destination payload through existing adapter/manager;
2. call `drain_delivered()` and route each `DeliveredApplicationBytes` to the owning SAM stream by `ConnectionId`;
3. place bytes into a bounded per-stream write buffer/queue;
4. if the local TCP socket is slow, stop draining additional delivered bytes into unbounded SAM state;
5. write with cancellation and timeout policy;
6. free byte budget only after bytes leave the SAM buffer.

If `StreamingManager::pending_delivered` itself can grow independently while the SAM adapter stops draining, confirm its bound. If it lacks a sufficient bound or backpressure seam, this is a concrete M6 integration defect: fix narrowly with a bounded queue/consumption API and regression tests. Do not hide it with a larger SAM queue.

## 10. Critical ACK/backpressure invariant

Plan 134 fixed receive state so rejected far-ahead packets cannot inflate `ackThrough`. Phase 7 must not invalidate the product-level meaning of backpressure.

The bridge must not acknowledge or consume remote data merely by copying it into an effectively unbounded API queue.

Required slow-reader test:

- establish a SAM stream;
- stop reading from one SAM TCP socket;
- send substantially more than the configured bridge budget from the peer;
- prove SAM buffered bytes remain at/below bound;
- prove no unbounded task/message queue growth;
- prove the sender encounters normal Streaming/backpressure behavior rather than unlimited local buffering;
- resume reading and prove ordered exact-byte recovery.

## 11. Driving retransmit and ACK polling

A usable SAM stream runtime must drive the existing runtime-neutral manager polling APIs; otherwise connections may work only in ideal tests.

Integrate a per-destination or shared bounded scheduler that advances:

- retransmission polling;
- delayed ACK polling;
- connection timeout/cleanup;
- destination/session maintenance already required by M6.

Do not spawn one unbounded timer task per packet. Prefer one bounded session/destination driver loop using existing runtime conventions and monotonic time.

Every emitted request still goes through `StreamingDestinationAdapter`.

Tests must include a dropped data packet and demonstrate retransmission through the SAM socket trajectory, not merely a direct Streaming unit test.

## 12. Close and reset mapping

### Local SAM socket EOF / half-close

Define a deterministic policy. Preferred baseline:

- EOF on local read side initiates `send_close()` for the Streaming connection;
- continue writing already-received peer data until graceful closure or bounded close timeout;
- after peer CLOSE completes, close the TCP socket.

If Tokio half-close semantics make exact half-close mapping unnecessarily complex for M7, document a simpler full-duplex close policy, but do not leave the Streaming connection orphaned.

### Local TCP reset/error

Use `send_reset()` when the local application terminates abnormally and the Streaming connection is still active, subject to the existing valid-state rules.

### Remote Streaming CLOSE

Surface EOF to local TCP after all in-order delivered bytes already accepted into the bounded write path are flushed according to policy.

### Remote RESET

Terminate the local socket promptly and release state; do not deliver bytes after reset.

Control-session teardown overrides graceful per-stream close and cancels all attachments within bounded shutdown time.

## 13. SILENT correctness

Test SILENT behavior at the byte level.

CONNECT:

- false/default: exactly one `STREAM STATUS RESULT=OK\n` before raw payload;
- true: zero protocol bytes after the CONNECT command; first returned byte, if any, belongs to peer application data.

ACCEPT:

- false/default: `STREAM STATUS RESULT=OK\n`, then after peer connects exactly one peer-Destination line, then raw bytes;
- true: no status and no peer-Destination line; raw bytes only after peer connection.

Never accidentally prepend tracing/status text to raw mode.

## 14. Tests

Create a real localhost product test harness in Rust first; no Python orchestration is required.

Required trajectories:

### Connect/accept handshake

- two SAM sessions A/B;
- B posts ACCEPT;
- A CONNECTs to B public Destination;
- both reach Raw only after underlying Streaming establishment;
- B receives authenticated A public Destination line when non-silent.

### Exact bytes

- A -> B: binary payload containing `\n`, `\0`, `STREAM CONNECT`, non-UTF8 bytes;
- B -> A different binary payload;
- exact equality, proving raw mode bypasses line parser.

### Fragment/chunk boundaries

- 0 bytes;
- 1 byte;
- negotiated max payload boundary;
- max + 1 split across sends rather than rejected as one giant Streaming packet;
- multi-megabyte logical transfer through bounded repeated chunks.

### Fault/reliability

Using the existing local destination fault seam where possible:

- one dropped Streaming data packet -> retransmit -> exact bytes;
- duplicate packet -> exact-once application delivery;
- reorder -> ordered application delivery;
- corrupted lower-layer payload -> no application corruption.

### Backpressure

- local sender overproduces while remote SAM reader pauses;
- local SAM write side stalls;
- budgets remain bounded;
- resume succeeds.

### Lifecycle

- connect timeout;
- ACCEPT cancelled by socket close;
- control session closes with CONNECT active;
- raw stream socket closes without killing sibling stream;
- two simultaneous streams on same session;
- per-session stream ceiling exactly enforced;
- repeated cleanup idempotent.

## 15. Expected files changed

Likely:

```text
crates/i2pr-api/src/sam/stream.rs
crates/i2pr-api/src/sam/session.rs
crates/i2pr-api/src/sam/registry.rs
crates/i2pr-api/src/sam/runtime_adapter.rs (if runtime-neutral adapter name fits)
crates/i2pr-daemon/src/sam.rs
crates/i2pr-client/src/streaming/manager.rs       # only narrow missing seams
crates/i2pr-client/src/streaming_adapter.rs       # only if required
crates/i2pr-client/src/streaming/events.rs        # only safe observation additions
crates/i2pr-api/tests/sam_stream*.rs
```

No NTCP2/SSU2, tunnel crypto, NetDB wire, or ECIES protocol changes unless a concrete existing bug is demonstrated.

## 16. Acceptance criteria

Plan 138 closes only when:

1. real SAM 3.1 CONNECT and ACCEPT sockets work over `127.0.0.1`;
2. each uses a separate TCP socket and transitions permanently to raw mode after success;
3. SILENT true/false wire behavior matches the protocol;
4. CONNECT reports OK only after underlying Streaming establishment;
5. ACCEPT non-silent emits the authenticated peer public Destination before raw data;
6. full public Destinations can be used as CONNECT targets;
7. all stream bytes traverse the actual `StreamingManager -> StreamingDestinationAdapter -> destination routing` product path;
8. no direct manager-to-manager shortcut is used in the acceptance trajectory;
9. bidirectional arbitrary binary data is exact and ordered;
10. retransmission survives at least one injected packet loss in the SAM product trajectory;
11. duplicate/reorder tests preserve exact-once ordered delivery;
12. slow-reader and slow-writer tests prove configured byte ceilings;
13. send-window backpressure stops TCP reads rather than accumulating unbounded memory;
14. sibling streams survive one stream socket failure;
15. parent control-session loss cancels all streams;
16. graceful close/reset behavior is mapped and bounded;
17. all workspace gates pass;
18. `plans/138-status.md` records exact trajectories and sets `next_executable_plan = 139`.

## 17. Handoff checklist

```text
[ ] Plan 137 listener/session lifecycle passed
[ ] STREAM CONNECT/ACCEPT status ordering matches SAM 3.1
[ ] raw mode is binary-transparent
[ ] production M6 destination/Streaming path is used
[ ] no unbounded bridge queue exists
[ ] retransmit/ACK polling is driven by runtime
[ ] close/reset cleanup is proven
[ ] multi-stream session behavior is bounded
[ ] no naming/FORWARD scope leaked into this plan
[ ] Plan 138 status is committed
```

Proceed next to **Plan 139**.
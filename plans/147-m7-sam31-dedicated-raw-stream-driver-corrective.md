# Plan 147 — SAM 3.1 dedicated raw STREAM socket driver corrective pass

Status: **blocked on successful Plan 146 closure**.

Depends on: Plan 146; Plan 134/129 local destination + Streaming product; useful Plan 143/144 local-delivery and handshake work.

## 1. Goal

Implement the product behavior that Plan 143 was originally supposed to close but did not:

```text
SAM STREAM TCP socket
  -> HELLO + one STREAM CONNECT/ACCEPT command
  -> actual I2P Streaming handshake reaches Established
  -> permanent command-parser detachment
  -> owned raw TCP socket driver
  -> StreamingManager::send_data / drain_delivered
  -> StreamingDestinationAdapter + Plan 129 destination stack
  -> peer StreamingManager
  -> peer raw SAM socket
```

This plan is complete only when real localhost SAM TCP sockets move arbitrary bidirectional binary application bytes through the full local destination/Streaming product path under bounded backpressure.

The in-process `bridge_to_peer` handshake from Plan 144 is a retained regression, not closure evidence by itself.

## 2. Concrete current defects to fix

The current source contains several explicit acceptance violations:

1. `execute_stream_connect()` calls `StreamingManager::connect()`, obtains a SYN, and then updates the SAM attachment to `SamStreamState::Established` before the underlying Streaming connection has actually reached `ConnectionState::Established`.
2. The production CONNECT path uses deterministic `ChaCha8Rng::seed_from_u64(0)`.
3. `DispatchOutcome::StreamRawMode` does not transfer ownership of the accepted `TcpStream` to a raw driver.
4. `handle_connection()` still owns a `LineReader` and `TcpStream` until shutdown; returning `Closed` causes the socket to be shut down rather than driven as raw bytes.
5. `STREAM ACCEPT` registers an inbound waiter/listener but does not complete the real inbound SYN -> accept -> SYN-response -> established trajectory before raw-mode handoff.
6. There is no raw TCP -> `StreamingManager::send_data()` loop.
7. There is no `StreamingManager::drain_delivered()` -> raw TCP loop.
8. There is no runtime driver for `poll_retransmits()` / `poll_acks()` / timeouts.
9. no same-write `STREAM CONNECT ...\n<raw bytes>` preservation proof exists.
10. no slow-reader/slow-writer boundedness proof exists.

Do not close this plan until each item is removed or proven irrelevant by a better architecture.

## 3. Socket ownership redesign

The current dispatch API accepts `&mut TcpStream`. That cannot express permanent transfer of the socket to another owner cleanly.

Refactor the daemon connection loop so command handling returns an **owned terminal disposition**.

Recommended conceptual shape:

```rust
enum ConnectionDisposition {
    Continue(ServerConnectionState),
    Close,
    Raw(RawStreamHandoff),
}

struct RawStreamHandoff {
    stream: TcpStream,
    session_id: SamSessionId,
    destination_id: DestinationId,
    attachment_id: u32,
    connection_id: ConnectionId,
    peer_destination: Option<String>,
    initial_raw_bytes: Vec<u8>,
    silent: bool,
    direction: RawDirection,
}
```

Exact type names may differ, but the ownership properties are mandatory:

- after successful CONNECT/ACCEPT, **one and only one** raw driver owns the `TcpStream`;
- the command loop no longer has access to the socket;
- the `LineReader` is dropped/detached permanently;
- no subsequent raw byte is parsed as SAM command text;
- the normal `handle_connection()` shutdown path must not close a socket that has been handed off.

Do not emulate ownership with a boolean while retaining the same line-read loop.

## 4. Preserve post-command bytes exactly

`LineReader` currently buffers bytes arriving after a command newline, but it exposes only `buffered_len()` / `reset()` and has no ownership-transfer API.

Add a narrow runtime-neutral method such as:

```rust
pub fn take_buffered(&mut self) -> Vec<u8>
```

or an equivalent consuming handoff.

Requirements:

- bounded by the existing SAM line/read ceilings at command time;
- preserves byte order exactly;
- no UTF-8 conversion on raw bytes;
- called only at the command->raw transition;
- empty after transfer;
- covered by tests where CONNECT/ACCEPT line and binary payload arrive in the same TCP read.

Do not discard, reparse, normalize, or line-split the remainder.

## 5. CONNECT must wait for real Streaming establishment

Required sequence:

```text
stream socket HELLO
STREAM CONNECT ID=A DESTINATION=<B>
reserve attachment
StreamingManager::connect()
route SYN through i2pr_client::deliver / Plan129 path
B receives inbound SYN
B ACCEPT path accepts it
route SYN response through Plan129 reverse path
A canonical StreamingManager reaches ConnectionState::Established
only now:
  non-silent -> write STREAM STATUS RESULT=OK\n
  silent     -> write nothing
handoff socket to RawStreamDriver
```

Remove any SAM registry state transition to `Established` that is based only on SYN creation or queueing.

Use the actual `StreamingManager` connection state as authority.

CONNECT failure/timeout must:

- map to the most specific SAM result;
- release the attachment once;
- not leak a `StreamingManager` connection entry;
- close the stream socket after the failure response (unless SILENT failure semantics require bare close);
- remain bounded by injected/configured deadline.

Production timeout should remain realistic (the SAM spec notes router stream connect timeout is approximately one minute / implementation-dependent); tests may inject shorter deterministic deadlines.

## 6. ACCEPT must complete the real inbound handshake

Required sequence:

```text
stream socket HELLO
STREAM ACCEPT ID=B [SILENT]
validate session
reserve waiter/attachment
ensure wildcard Streaming listener
non-silent -> write STREAM STATUS RESULT=OK\n
wait for real inbound SYN produced by peer CONNECT path
claim exactly one inbound connection
StreamingManager::accept_inbound_syn()
queue/route SYN response through Plan129 reverse path
both Streaming sides become Established
non-silent -> write authenticated peer Destination + '\n'
silent     -> no peer-destination line
handoff socket to RawStreamDriver
```

The peer Destination line must come from authenticated inbound Streaming identity/context, not from the outbound CONNECT command text.

ACCEPT/FORWARD exclusion from Plan 139 must remain atomic.

## 7. Raw stream driver structure

Prefer one supervised task per active SAM STREAM socket plus one bounded per-destination scheduler/driver, or another architecture consistent with existing runtime conventions.

A raw driver must own:

```text
TcpStream
session/attachment lease
Streaming ConnectionId
cancellation token
bounded raw read buffer
bounded pending write buffer/accounting
peer/local destination handles
```

The task must be joined/cancelled by the existing supervisor. No detached `tokio::spawn`.

## 8. TCP -> Streaming direction

The raw read loop must:

1. process `initial_raw_bytes` before reading more from TCP;
2. read at most a configured bounded chunk;
3. cap each `send_data()` admission to the negotiated Streaming payload/window constraints;
4. stop reading when `send_data()` cannot admit more bytes because of send-window/backpressure;
5. drain all resulting `TransportSendRequest`s;
6. route each request through the exact Plan 143/144 product delivery path (`StreamingDestinationAdapter` / `i2pr_client::deliver` / destination tunnels / peer adapter);
7. resume TCP reads only after ACK/window progress;
8. never accumulate an unbounded `Vec<u8>` of application data.

Large application writes must be segmented by bounded admission and delivered in order.

## 9. Streaming -> TCP direction

The receive side must:

1. receive packets only through the Plan 129 inbound destination path;
2. let `StreamingManager` reorder/deduplicate them;
3. drain delivered application bytes for the correct `ConnectionId`;
4. hold at most a named per-stream write budget;
5. write raw bytes to the owned TCP socket with cancellation/deadline handling;
6. stop draining more delivered bytes when the local application is not reading;
7. release accounting only after bytes are consumed/written.

Audit `StreamingManager::pending_delivered` before implementing a second queue. If the manager can grow unbounded while the SAM writer stalls, fix the bounded consumption contract in `i2pr-client` and add M6 regressions. Do not hide the defect behind a larger daemon queue.

## 10. Per-destination Streaming driver

Install bounded runtime advancement for each active SAM destination or a shared bounded scheduler.

It must drive:

```text
StreamingManager::poll_retransmits(now_ms)
StreamingManager::poll_acks(now_ms)
connection timeout/cleanup APIs
outbound requests produced by the above
```

Every generated request must traverse the same product delivery path as application data.

Requirements:

- no task per packet;
- no unbounded timer/channel;
- cancellation owned by the parent SAM session/destination;
- session teardown joins the driver;
- driver errors cannot silently spin;
- deterministic clocks/RNG only in tests.

## 11. Production randomness correction

Remove deterministic seeded RNG from production SAM connection establishment/delivery.

Use one of:

- `OsRng` through the existing crypto abstraction;
- router-owned CSPRNG capability already used elsewhere in production;
- a narrow injected trait/capability where tests supply deterministic RNG and production supplies OS randomness.

Acceptance must include a static/source assertion or boundary test that the production SAM module does not construct `ChaCha8Rng::seed_from_u64(...)` or another deterministic RNG.

## 12. Backpressure and resource accounting

Name and bound every byte reservoir:

```text
command reader remainder / initial raw bytes
raw TCP read chunk
Streaming send-window bytes
outbound transport request queue
Streaming receive/reorder window
pending delivered application bytes
raw TCP pending write bytes
```

No aggregate path may scale with remote input without a configured ceiling.

Add explicit counters/test snapshots where practical.

### Slow reader test

- establish A -> B through real SAM sockets;
- stop B application reads;
- send substantially more than all configured buffers/windows;
- prove memory-accounted pending bytes remain <= documented limits;
- prove A eventually stalls from backpressure rather than accumulating data;
- resume B reads;
- verify exact ordered bytes and all counters return to baseline.

### Slow writer / blocked egress test

Use an injected short-capacity/paused writer or socket pressure to prove the reverse direction remains bounded.

## 13. Reliability/fault tests

Use the existing deterministic Plan 129 local fault seams below the real SAM TCP boundary.

Required trajectories:

- drop one data packet -> RTO -> retransmit -> exact bytes;
- drop one standalone/delayed ACK -> recovery without deadlock;
- duplicate one packet -> exact-once application delivery;
- reorder at least two data packets -> ordered application delivery;
- authenticated/ciphertext corruption -> no application bytes delivered;
- ACK progress clears tracked retransmits;
- retransmit attempt ceiling eventually terminates failed stream cleanly.

Tests must not manually move application payload between managers after STREAM establishment.

## 14. SILENT and byte-exact transcripts

Freeze exact behavior from current SAM v3 documentation.

CONNECT non-silent:

```text
STREAM STATUS RESULT=OK\n
<first raw byte immediately follows>
```

CONNECT silent:

```text
<first raw byte; no status line>
```

ACCEPT non-silent:

```text
STREAM STATUS RESULT=OK\n
<peer public Destination>\n
<first raw byte>
```

ACCEPT silent:

```text
<first raw byte; no status / peer line>
```

Once raw mode begins, literal application bytes such as:

```text
PING x\n
QUIT\n
STREAM CONNECT ID=...\n
```

must remain raw payload and never trigger SAM parser behavior.

## 15. Close/reset/lifecycle

Implement and test deterministic lifecycle semantics.

### Local TCP EOF

- call existing `send_close()` where state permits;
- route CLOSE through full destination path;
- flush already-delivered inbound bytes within bounded deadline;
- release attachment exactly once.

### Local TCP I/O error

- send RESET if appropriate;
- terminate task;
- release accounting/attachment.

### Remote CLOSE

- deliver already accepted bytes according to bounded policy;
- surface EOF to local TCP;
- release connection/task.

### Remote RESET

- terminate promptly;
- no later queued application delivery.

### Parent control socket close

- cancel all child raw sockets and destination driver tasks;
- remove pending ACCEPT/FORWARD state;
- return counters to baseline within daemon shutdown bound.

### Multiple streams

- at least two sibling STREAM sockets on one SAM session;
- closing/resetting one does not terminate the other;
- exact per-session stream ceiling enforced.

## 16. Canonical product acceptance tests

Add a dedicated localhost test file, for example:

```text
crates/i2pr-daemon/tests/sam_stream_raw_product.rs
```

The test client may be a small Rust SAM transcript helper, but it must interact only through TCP sockets for application bytes.

Canonical setup:

```text
control A -> HELLO + SESSION CREATE
control B -> HELLO + SESSION CREATE
stream B  -> HELLO + STREAM ACCEPT
stream A  -> HELLO + STREAM CONNECT B.PUB
raw A <-> raw B
```

Required data matrix:

- 1 byte;
- NUL;
- newline/CRLF;
- non-UTF8 bytes;
- all byte values 0x00..0xff;
- payload beginning with SAM command-looking text;
- exact negotiated payload boundary;
- boundary + 1 split across Streaming packets;
- multi-packet payload;
- multi-megabyte logical transfer under bounded buffering;
- simultaneous bidirectional traffic.

No private daemon/Streaming API may be used by the test to move these application bytes.

## 17. Retained regressions

Keep and run:

- Plan 142 Base64 tests;
- Plan 144 in-process SYN/SYN-response handshake;
- Plan 129 integrated destination/Streaming trajectories;
- Plan 130–134 corrective regressions, especially Plan 134 ACK ceiling.

The new raw driver must not change M6 Streaming semantics merely to satisfy SAM.

## 18. Expected files

Likely:

```text
crates/i2pr-api/src/sam/line_reader.rs
crates/i2pr-api/src/sam/server_state.rs
crates/i2pr-daemon/src/sam.rs
crates/i2pr-daemon/src/sam/streams.rs
crates/i2pr-daemon/src/sam/raw_stream.rs          # recommended if it keeps ownership clear
crates/i2pr-client/src/streaming/manager.rs       # only if bounded delivered-consumer seam is missing
crates/i2pr-daemon/tests/sam_stream_raw_product.rs
crates/i2pr-daemon/tests/sam_stream_independent.rs
crates/i2pr-client/tests/plan129_trajectory.rs    # regression only if lower seam changes
```

Do not modify NTCP2/SSU2 or add a new orchestration framework.

## 19. Acceptance criteria

Plan 147 closes only when all are true:

1. Plan 146 private-destination reference compatibility passed;
2. CONNECT/ACCEPT use dedicated localhost TCP sockets;
3. command-mode socket ownership is permanently transferred to a raw driver after successful establishment;
4. line parser cannot observe any byte after raw transition;
5. post-command bytes already buffered by `LineReader` are preserved exactly;
6. CONNECT returns OK only after actual `StreamingManager` state is `Established`;
7. ACCEPT waits for and accepts the actual inbound SYN and routes the SYN response through the full product path;
8. non-silent ACCEPT emits the authenticated peer Destination line;
9. SILENT behavior is byte-exact;
10. TCP -> Streaming uses `send_data()` with bounded admission/backpressure;
11. Streaming -> TCP uses ordered delivered-byte consumption with bounded buffering;
12. all application bytes traverse the Plan 129 destination stack in both directions;
13. no direct manager-to-manager application-byte shortcut is used as acceptance evidence;
14. deterministic production RNG is removed;
15. delayed ACK/retransmit/timeouts are driven by bounded supervised runtime ownership;
16. loss/duplicate/reorder/ACK-drop tests pass through real SAM sockets;
17. slow-reader/slow-writer tests prove explicit byte ceilings;
18. close/reset/control-session cancellation release resources exactly once;
19. sibling streams remain independent;
20. arbitrary binary and multi-megabyte logical transfers are exact and ordered;
21. Plan 129–134 regressions remain green;
22. workspace/boundary gates pass;
23. `plans/147-status.md` records exact tests/results and sets `next_executable_plan = 148`.

## 20. Validation commands

At minimum:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-client --all-targets
cargo test --locked -p i2pr-daemon --test sam_stream_raw_product
cargo test --locked -p i2pr-daemon --test sam_stream_independent
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

Plus the explicit focused Plan 129–134 regression commands recorded in the closure status.

## 21. Handoff

Do not begin independent-client final closure until the raw socket lane passes without internal byte-moving shortcuts.

If Plan 147 passes, execute **Plan 148 only**.
# Plan 137 — SAM 3.1 loopback server and session lifecycle

Status: **blocked on successful Plan 136 closure**.

Depends on: Plan 135, Plan 136.

## 1. Goal

Turn the runtime-neutral Plan 136 SAM protocol/key foundation into a real bounded loopback service that can:

- accept TCP clients;
- negotiate `HELLO VERSION`;
- execute `DEST GENERATE`;
- create/destroy `STYLE=STREAM` sessions using `TRANSIENT` or imported private destinations;
- enforce global session-ID and destination uniqueness;
- bind one session to one control connection;
- expose a safe attachment lookup for later STREAM sockets;
- release every session/destination/task/resource on disconnect or shutdown.

This plan intentionally stops before moving application stream bytes. `STREAM CONNECT`, `STREAM ACCEPT`, and `STREAM FORWARD` may parse and return explicit not-yet-supported/internal-state errors in focused intermediate tests, but their actual stream bridge belongs to Plans 138–139.

## 2. Architectural ownership

Use two layers:

```text
crates/i2pr-api
  deterministic SAM connection/session state
  global SAM session registry abstraction
  typed session commands/replies
  no direct daemon dependency

crates/i2pr-daemon
  Tokio TcpListener
  accepted-socket tasks
  cancellation/supervision
  composition with DestinationRegistry and runtime services
```

Do not put Tokio socket ownership into the parser module. It is acceptable for `i2pr-api` to use Tokio synchronization primitives only if this is already consistent with repository architecture; prefer runtime-neutral state plus daemon-owned async orchestration where practical.

The daemon remains the composition root. `i2pr-api` must never import `i2pr-daemon`.

## 3. Configuration

Add a strict `[sam]` configuration section or equivalent existing-config style.

Required policy fields should include at least:

```text
enabled
bind_address
port
max_clients
max_sessions
max_stream_sockets_per_session
max_pending_accepts_per_session
max_buffered_bytes_per_stream_direction
hello_timeout_ms
command_timeout_ms / idle_timeout_ms as justified
shutdown_timeout_ms
```

Defaults:

- SAM disabled by default until Plan 140 closure, or enabled only in an explicitly experimental profile; choose the safer behavior consistent with current daemon policy.
- bind address defaults to `127.0.0.1`.
- default port may follow the conventional SAM bridge port if the project chooses, but tests must always use port `0`/ephemeral binding.

Security rule: a non-loopback `bind_address` must be rejected by configuration validation in Milestone 7. Do not merely warn. Remote SAM exposure requires a later authenticated security design.

Configuration validation must reject zero/absurd limits, overlarge timeouts, and inconsistent aggregate budgets.

## 4. Runtime service model

Introduce a supervised SAM service with explicit lifecycle states equivalent to:

```text
Disabled
Starting
Ready
Stopping
Stopped/Failed
```

The service owns:

- one loopback `TcpListener`;
- one global `SamSessionRegistry`;
- bounded client task admission;
- cancellation token/handle integrated with existing runtime conventions;
- an adapter/capability to create/remove `DestinationRuntime` objects in the existing router-local `DestinationRegistry`;
- no independent copy of destination state.

The service must not report Ready until the listener is bound and session registry is usable.

Listener bind failure must fail/degrade the SAM service according to repository service policy without corrupting unrelated router state.

## 5. Accepted TCP connection state machine

Each accepted socket begins in:

```text
AwaitHello
```

Allowed transition:

```text
AwaitHello
  -- HELLO VERSION compatible --> UtilityReady
```

From `UtilityReady`, a connection may execute utility commands such as `DEST GENERATE` and may issue one `SESSION CREATE STYLE=STREAM` to become:

```text
SessionControl { session_id, destination_id }
```

A separate connection can later negotiate HELLO and issue STREAM attachment commands referencing an existing session ID; Plan 138 implements those terminal transitions. Plan 137 should model the attach-capable state without permitting ambiguous command reuse.

Reject command-order violations deterministically. Examples:

- `SESSION CREATE` before HELLO;
- second HELLO after negotiation if spec/policy disallows it;
- second `SESSION CREATE` on the same ordinary control socket;
- STREAM command on the session control socket where SAM v3 requires a separate socket;
- application bytes before a STREAM command transitions a socket into raw-byte mode.

After a socket transitions to raw STREAM mode in Plan 138, the SAM line parser must never inspect application payload bytes.

## 6. Incremental line reading

Implement a bounded line reader around Plan 136's pure parser.

Requirements:

1. tolerate TCP segmentation: one command may arrive across many reads;
2. tolerate multiple complete command lines in one read only where the state machine permits another command;
3. enforce `MAX_SAM_LINE_BYTES` while data is accumulating, before newline arrives;
4. terminate the client on line overflow rather than buffering until newline;
5. define handling of `\r\n` vs `\n` according to current SAM interoperability evidence;
6. do not use `read_line()` with an attacker-controlled unbounded `String`;
7. partial UTF-8 must be handled according to Plan 136 grammar policy without panic;
8. handshake timeout starts at accept and is cancelled when HELLO succeeds.

Tests must write one byte at a time and verify the same semantic result as a single write.

## 7. Global SAM session registry

Create an explicit bounded registry separate from `DestinationRegistry` because SAM IDs and control-socket ownership are API-layer concepts.

Each entry should contain only what is necessary to resolve attachments and teardown, for example:

```text
session_id
local DestinationId
public destination / safe handle
control-owner token/generation
session cancellation token
per-session stream attachment count
pending accept count
forward state placeholder
```

Do not store duplicate secret key copies in the SAM registry. Secret destination ownership should remain in the destination runtime / narrowly owned session object established in Plan 136.

Global invariants:

- session ID is unique;
- one local destination cannot be owned by two ordinary SAM sessions simultaneously;
- capacity is bounded;
- session entry insertion and DestinationRegistry insertion are transactional;
- failure after one insertion must roll back the other;
- teardown removes both exactly once.

If ownership requires a new higher-level wrapper around `DestinationRuntime`, make the ownership explicit rather than sharing mutable references through ad hoc `Arc<Mutex<_>>` graphs.

## 8. `DEST GENERATE`

After HELLO and without requiring a session:

```text
DEST GENERATE SIGNATURE_TYPE=7
```

must call Plan 136's runtime-neutral generator and reply with canonical `DEST REPLY PUB=... PRIV=...`.

Rules:

- generated identity is not inserted into `DestinationRegistry`;
- no long-lived secret copy remains after response serialization;
- failure to write the reply drops/zeroizes generated secret material;
- unsupported signing type returns the protocol-appropriate failure behavior established by Plan 136;
- command can be exercised repeatedly subject to per-client/global CPU/rate limits if a rate limit is introduced; at minimum it remains bounded by client admission and command processing.

Add a test proving repeated `DEST GENERATE` does not increase registered destination/session counts.

## 9. `SESSION CREATE STYLE=STREAM`

Support exactly the M7 baseline:

```text
SESSION CREATE STYLE=STREAM ID=<id> DESTINATION=TRANSIENT SIGNATURE_TYPE=7 [options]
SESSION CREATE STYLE=STREAM ID=<id> DESTINATION=<sam-private-destination> [options]
```

For TRANSIENT:

- generate one destination under the type-7 policy;
- construct the real `DestinationRuntime` using existing `DestinationConfig` policy;
- create the per-destination Streaming context required by Plan 138 using existing M6 types, not a new SAM-specific Streaming implementation.

For imported private destination:

- Plan 136 strict-decode/import;
- reconstruct exact public identity;
- reject unsupported crypto/signature profile;
- never log the private destination string.

Options:

- explicitly map only options the M7 product can honor;
- tunnel options may be validated/projected to `DestinationConfig` only if existing M6 APIs support them correctly;
- unknown/unsupported options must be rejected or documented as ignored only where SAM compatibility requires permissive behavior. Do not silently pretend to honor `i2cp.*` settings that are not implemented.

Session creation success must not claim network readiness that does not exist. In the constrained local product path, define a deterministic local readiness policy for tests. If existing `DestinationRuntime` requires real established tunnel material before `Usable`, the SAM composition may use the same local test-product seam used by Plan 129 rather than inventing fake production readiness. Production daemon behavior must remain truthful about unavailable network paths.

This distinction must be documented and tested.

## 10. Transactional creation

Treat SESSION CREATE as a transaction:

```text
parse/validate
 -> reserve global session capacity
 -> construct/import identity
 -> construct destination runtime + Streaming context
 -> reserve/insert destination
 -> reserve/insert SAM session ID
 -> attach ownership to control socket
 -> emit SESSION STATUS OK
```

Any failure before final success must release every prior reservation.

If writing `SESSION STATUS OK` fails because the client disconnects, immediately tear down the just-created session/destination. There must be no orphan session after a failed success reply.

Duplicate cases:

- duplicate ID -> `DUPLICATED_ID`;
- duplicate destination -> `DUPLICATED_DEST`;
- both present -> deterministic precedence documented/tested;
- capacity exhaustion -> protocol-appropriate failure without partial insertion.

## 11. Control-socket ownership

A successful ordinary SAM session is owned by the TCP socket that created it.

On EOF, reset, protocol-fatal parse failure, service shutdown, task cancellation, or panic boundary converted to error:

1. mark session stopping so new attachments fail;
2. cancel all attachment tasks through the session cancellation primitive;
3. remove/close active Streaming contexts as far as Plan 137 owns them;
4. remove destination from `DestinationRegistry`, invoking its normal shutdown;
5. remove session ID;
6. release all resource reservations;
7. complete within bounded shutdown timeout;
8. make duplicate cleanup idempotent.

Never keep a session alive simply because a STREAM attachment socket still exists after control ownership is gone.

## 12. Concurrency/race requirements

Tests must cover races between:

- two clients creating the same ID;
- two clients importing the same destination under different IDs;
- control disconnect while SESSION CREATE is in progress;
- daemon shutdown while accept loop is waking;
- attachment lookup racing session destruction;
- client admission hitting max exactly;
- session admission hitting max exactly;
- repeated cancellation.

Use synchronization/barriers in tests rather than sleeps when possible.

The registry API should make uniqueness atomic under the chosen synchronization strategy. Avoid check-then-insert races across independent locks.

## 13. PING/PONG and connection termination utilities

Implement the small utility subset if Plan 136 modeled it:

- server receives `PING [payload]` -> bounded `PONG [payload]` according to spec;
- `PONG` from client is accepted only in a state where meaningful or ignored explicitly;
- `QUIT`, `STOP`, `EXIT` close the connection/session according to the negotiated ordinary-session rules.

Payload bounds are the Plan 136 line bounds. These utilities must not complicate the critical session state machine; if current interoperability evidence shows they are unnecessary for baseline clients, they may be deferred to Plan 139 but the behavior must be explicit.

## 14. Logging/privacy

Allowed by default:

- service state;
- counts;
- bind address/port (loopback);
- coarse error category;
- redacted/hashed session identity if existing policy supports it.

Forbidden:

- `PRIV`;
- raw decoded private keys;
- application stream bytes;
- full public Destination blobs unless an explicit diagnostic mode is later designed;
- raw session tags/ECIES keys;
- command lines containing `DESTINATION=<PRIV>`.

Do not log raw inbound SAM command lines. Structured handlers should log only parsed safe metadata.

## 15. Tests

### Pure state tests

- HELLO success/failure and state transition;
- command before HELLO;
- DEST GENERATE remains utility-only;
- SESSION CREATE TRANSIENT;
- SESSION CREATE imported private destination;
- duplicate ID;
- duplicate destination;
- capacity boundaries;
- unsupported STYLE;
- unsupported option;
- second session on same control connection;
- idempotent teardown.

### Real loopback TCP tests

Bind `127.0.0.1:0` and cover:

- HELLO split at every byte boundary in one representative command;
- multiple writes / partial reply reads;
- oversized no-newline input is disconnected at the bound;
- successful DEST GENERATE transcript;
- DEST-generated PRIV used on a fresh connection for SESSION CREATE;
- control EOF removes session and destination;
- write failure after create cannot orphan state;
- two concurrent duplicate-ID creators -> exactly one success;
- service shutdown closes listener and clients;
- no task leak observable through existing supervisor/test hooks.

No external router or public network is required.

## 16. Expected files changed

Likely:

```text
crates/i2pr-api/src/sam/server_state.rs
crates/i2pr-api/src/sam/session.rs
crates/i2pr-api/src/sam/registry.rs
crates/i2pr-api/src/sam/limits.rs
crates/i2pr-api/src/lib.rs
crates/i2pr-daemon/src/sam.rs
crates/i2pr-daemon/src/config.rs
crates/i2pr-daemon/src/bootstrap.rs / lib.rs
crates/i2pr-daemon/Cargo.toml
config examples/tests as applicable
```

Avoid touching transport/tunnel/NetDB wire code.

## 17. Acceptance criteria

Plan 137 closes only if:

1. a real loopback SAM listener can start and stop under daemon supervision;
2. configuration rejects non-loopback bind addresses;
3. incremental line reading is bounded before newline;
4. HELLO 3.1 succeeds and incompatible versions fail correctly;
5. DEST GENERATE works over a real socket;
6. generated PRIV can be used to create a new STREAM session;
7. TRANSIENT session creation works under the declared type-7 policy;
8. duplicate IDs/destinations and capacity exhaustion are transactional and leak-free;
9. one ordinary session is owned by one control socket;
10. control disconnect removes the SAM session and its destination exactly once;
11. service shutdown cancels accepted clients and sessions within configured bounds;
12. no raw secret-bearing command line is logged;
13. no STREAM application-byte path exists yet except explicit stubs/status handling;
14. all workspace gates pass;
15. `plans/137-status.md` records exact tests and sets `next_executable_plan = 138`.

## 18. Handoff checklist

```text
[ ] Plan 136 status is passed
[ ] listener defaults/restrictions are loopback-only
[ ] HELLO / DEST GENERATE / SESSION CREATE work on real TCP
[ ] session/destination insertion is transactional
[ ] control ownership teardown is proven
[ ] global and per-session limits are named and tested
[ ] STREAM sockets are not yet bridged
[ ] no live-router prerequisite was introduced
[ ] Plan 137 status is committed
```

Proceed next to **Plan 138**.
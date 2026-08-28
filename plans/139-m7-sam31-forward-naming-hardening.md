# Plan 139 — SAM 3.1 STREAM FORWARD, naming, and hardening

Status: **implemented; closure recorded in [`plans/139-status.md`](139-status.md)**.

Depends on: Plans 135–138.

## 1. Goal

Complete the intended Milestone 7 SAM 3.1 STREAM feature surface around the CONNECT/ACCEPT product path by adding:

- ordinary `STREAM FORWARD`;
- `NAMING LOOKUP` with a deliberately narrow, truthful resolution policy;
- `NAME=ME`;
- final utility-command behavior required by maintained clients;
- parser/state-machine negative coverage;
- resource exhaustion, cancellation, timeout, and privacy hardening;
- clear rejection of unsupported newer SAM features.

This plan does not expand the advertised version beyond SAM 3.1 and does not reopen live I2P network transport validation.

## 2. STREAM FORWARD protocol behavior

Baseline request:

```text
STREAM FORWARD ID=<session-id> PORT=<tcp-port> [HOST=<host>] [SILENT=true|false]
```

`SSL=true` is SAM 3.2+ behavior and is not supported in M7. Under negotiated 3.1, reject it explicitly rather than ignoring it.

The forwarding command socket is a long-lived control/ownership socket for the forward registration. Whether `SILENT` is true or false, SAM requires a `STREAM STATUS` response to the FORWARD command; this differs from CONNECT/ACCEPT.

On success:

```text
STREAM STATUS RESULT=OK
```

The socket remains open. Its EOF/cancellation removes the forward registration immediately.

FORWARD and pending ACCEPT are mutually exclusive for a given ordinary STREAM session. Enforce this atomically:

- if any pending ACCEPT exists, FORWARD fails;
- while FORWARD is active, new ACCEPT fails;
- when FORWARD is removed, ACCEPT becomes eligible again.

Do not represent this with two independent booleans that can race. Use one session inbound-listener mode enum, for example:

```text
InboundMode::Idle
InboundMode::Accepting { count }
InboundMode::Forwarding { owner }
```

## 3. Forward target security policy

SAM itself permits HOST/PORT forwarding, but Milestone 7 exposes an unauthenticated loopback API. Do not turn it into a general TCP pivot into the host/network.

M7 policy:

- `PORT` is mandatory and must be 1..=65535;
- omitted HOST resolves to the peer IP of the forwarding SAM socket, which must be loopback because the SAM listener is loopback-only;
- explicit HOST is restricted to loopback targets in M7;
- numeric `127.0.0.0/8` and `::1` may be accepted according to normalized policy;
- `localhost` may be accepted only if resolution is bounded and every resolved address is loopback;
- any non-loopback target is rejected;
- no arbitrary DNS resolution to remote addresses;
- no Unix sockets in SAM 3.1 compatibility surface;
- no TLS/SSL forwarding.

Document this as an intentional experimental security limitation, not a SAM protocol claim.

## 4. Forward connection trajectory

When an inbound I2P Streaming SYN arrives for a session in Forwarding mode:

1. reserve one stream attachment and buffered-byte budget;
2. open a local TCP connection to the configured loopback HOST:PORT;
3. enforce a connect deadline no greater than the protocol/reference expectation (the SAM documentation describes a 3-second acceptance window; choose a bounded value consistent with that);
4. only after local TCP connection succeeds, accept the underlying I2P Streaming SYN using the existing Plan 138 path;
5. if `SILENT=false`, write one peer public Destination line to the new local TCP socket;
6. if `SILENT=true`, write no SAM metadata;
7. run the same bounded raw byte bridge as Plan 138;
8. cleanup only that stream on local target failure;
9. keep the forward registration active for future inbound streams until the forward control socket closes.

Do not accept the remote Streaming connection first and then wait indefinitely for a local forwarding target. Failure to connect locally within the bounded deadline must reject/reset the pending inbound stream and release its backlog slot.

Multiple forwarded inbound streams may be active concurrently only within the same per-session stream and global task limits established by Plans 137–138.

## 5. FORWARD lifecycle and races

Required race cases:

- forwarding socket EOF while local target connect is pending;
- parent session control socket EOF while FORWARD socket remains open;
- inbound SYN concurrent with FORWARD removal;
- two FORWARD commands racing on one session;
- ACCEPT racing FORWARD registration;
- local target refuses connection;
- local target accepts then immediately closes;
- stream ceiling reached while forwarding remains active;
- daemon shutdown.

In every case:

- at most one Forwarding registration survives;
- no inbound connection is assigned twice;
- no target task outlives session cancellation;
- forward mode returns to Idle exactly once when ownership ends.

## 6. NAMING LOOKUP policy

Milestone 7 does not need to implement a full address book. It must provide protocol-correct results for information i2pr actually possesses.

Required request:

```text
NAMING LOOKUP NAME=<name>
```

M7 does not claim Proposal 167 `OPTIONS=true`; recognize and explicitly reject/not-support it according to the Plan 136 reply policy.

Implement resolution in this order:

### A. `NAME=ME`

When invoked in a connection context associated with an existing SAM session, return that session's public Destination:

```text
NAMING REPLY RESULT=OK NAME=ME VALUE=<public-destination>
```

Do not return private material.

If `ME` is requested on a utility connection with no session context, return a deterministic protocol-appropriate failure documented in tests; do not guess another session.

### B. Full Base64 Destination

If NAME itself strictly decodes as a valid public Destination, return the canonical encoded Destination with `RESULT=OK`. This makes naming idempotent for clients that normalize targets through NAMING LOOKUP.

### C. Known local destination / known cached destination forms

If the existing router product has a safe lookup from a canonical b32/hash form to a validated Destination/LeaseSet2 record already present locally, use that existing NetDB/client seam. Do not add a second naming database.

Only return a Destination if the full public Destination can be obtained and validated. A hash alone is not a SAM `VALUE`.

### D. `.b32.i2p` requiring network lookup

Use the existing LeaseSet2/NetDB lookup machinery only if it can complete through the current product seam without inventing a live-router dependency. In the constrained environment, uncached/network-required queries may return `KEY_NOT_FOUND`/appropriate lookup failure. This is acceptable and should be documented.

### E. human-readable `.i2p` hostnames

No address-book implementation exists in the MVP at this stage. Return `KEY_NOT_FOUND` unless a future existing resolver is explicitly wired. Do not call system DNS and do not map `.i2p` through clearnet DNS.

### F. malformed names

Return `INVALID_KEY` where the SAM result vocabulary calls for it.

## 7. Naming security and bounds

- apply Plan 136 name length limit before any decode/lookup;
- no system resolver for `.i2p` or b32 targets;
- no outbound clearnet DNS as side effect of NAMING LOOKUP;
- bound concurrent NetDB-backed lookup attempts;
- bound lookup timeout and cancellation;
- deduplicate equivalent in-flight lookups only through existing NetDB machinery if it already supports this;
- control/session disconnect cancels client-owned wait, but must not corrupt shared validated NetDB state;
- do not log full Destinations at normal levels.

## 8. Utility commands and compatibility polish

Close any small SAM 3.1 utility gaps needed by independent clients:

- `PING` / `PONG` bounded keepalive behavior;
- `QUIT`, `STOP`, and `EXIT` connection/session close behavior;
- keyword case normalization;
- quoted values and escaped quote handling from Plan 136;
- explicit responses for unsupported command families.

Do not implement AUTH/TLS simply because the parser sees those keywords.

If maintained client libraries require an innocuous compatibility behavior not explicitly listed here, add it only when:

1. it is supported by current SAM documentation/reference implementations;
2. it does not expand beyond STREAM/SAM 3.1 semantics;
3. it has a focused interoperability test;
4. it is documented in `docs/protocol-support.md`.

## 9. Unsupported surface must be explicit

Test and document rejection of at least:

```text
SESSION CREATE STYLE=DATAGRAM
SESSION CREATE STYLE=RAW
SESSION CREATE STYLE=DATAGRAM2
SESSION CREATE STYLE=DATAGRAM3
SESSION CREATE STYLE=PRIMARY
SESSION ADD
SESSION REMOVE
STREAM CONNECT FROM_PORT=...
STREAM CONNECT TO_PORT=...
STREAM FORWARD SSL=true
NAMING LOOKUP OPTIONS=true
AUTH ...
DATAGRAM ...
RAW ...
```

A parser-level unknown-command response is insufficient when the server recognizes the command but does not implement the negotiated-version feature. Prefer a typed unsupported-feature path so behavior remains auditable.

## 10. Aggregate resource accounting

By the end of Plan 139, every long-lived SAM resource must have a named owner and ceiling.

Create or document a table in code/docs equivalent to:

| Resource | Owner | Ceiling | Released on |
|---|---|---|---|
| accepted SAM TCP clients | service | `max_clients` | socket/task end |
| STREAM sessions | service registry | `max_sessions` | control socket/session end |
| attachments | session | `max_stream_sockets_per_session` | stream end |
| pending ACCEPTs | session | `max_pending_accepts_per_session` | claim/cancel/session end |
| active FORWARD | session | 1 | forward socket/session end |
| forwarded target connect tasks | session/service | bounded by stream ceiling/global tasks | success/failure/cancel |
| SAM input line bytes | client | `MAX_SAM_LINE_BYTES` | command completion |
| raw outbound bridge bytes | stream | configured byte budget | Streaming admission/close |
| raw inbound bridge bytes | stream | configured byte budget | TCP write/close |
| naming waits | service/client | explicit concurrent ceiling | response/cancel/timeout |

Also enforce compatibility with router-wide `max_tasks` and `max_buffered_bytes`; SAM local limits must not be configured above aggregate budgets without validation or clamping policy.

## 11. CPU abuse controls

Loopback is not a trust boundary. Protect expensive paths:

- repeated `DEST GENERATE` cannot spawn work/tasks without client/global limits;
- invalid Base64 must fail before expensive destination construction;
- private-destination structural validation occurs before cryptographic recomputation where safe;
- naming invalid input fails before NetDB lookup;
- no regex/backtracking parser;
- line tokenization remains linear;
- reconnect/accept churn releases state promptly;
- forward target connection attempts are bounded and timeout quickly.

A dedicated token-bucket rate limiter is not mandatory for M7 if concurrent client/session/task ceilings already bound work, but document the residual local CPU-churn risk in the security model.

## 12. Memory/backpressure adversarial tests

Add stress-style deterministic tests with moderate counts (not giant CI loads) proving the invariants:

1. line of `MAX_SAM_LINE_BYTES + 1` without newline closes client at bound;
2. max tokens/options + 1 rejects without large residual allocation;
3. create sessions until exact max, next fails, delete one, next succeeds;
4. create streams until per-session max, next fails, sibling streams remain usable;
5. fill pending ACCEPT ceiling;
6. attempt FORWARD while ACCEPT pending and vice versa;
7. local forwarded target never accepts -> task/stream releases by timeout;
8. slow SAM reader holds exactly bounded bridge bytes while peer sends more;
9. slow local forwarded target does the same;
10. cancel parent session with every resource category active -> counts return to baseline;
11. repeated create/destroy loop leaves registry/task/buffer counts stable.

Where possible expose non-secret test-only/accounting snapshots instead of relying on process RSS as the sole assertion.

## 13. Privacy tests

Capture tracing output in tests for representative failures and assert it does not contain:

- generated `PRIV` value;
- known signing private key bytes/Base64 fragments;
- X25519 secret bytes;
- raw STREAM payload markers;
- entire imported private Destination command;
- ECIES session tags.

Document which public/session metadata may appear and why.

## 14. Configuration and daemon behavior

By Plan 139, define the intended experimental operator behavior:

- SAM listener still loopback-only;
- `check-config` validates all SAM fields without binding;
- dry-run/check paths do not generate destinations or open sockets;
- ordinary daemon start reports SAM readiness without exposing session identifiers;
- listener shutdown is part of global graceful shutdown;
- configuration reload is **not** required for M7 unless current daemon already has transactional reload machinery. If not supported, document restart-required fields and leave live reload to Milestone 13.

## 15. Tests for STREAM FORWARD

Real sockets, no mocks for the final path:

1. start local TCP echo/server on `127.0.0.1:0`;
2. create SAM session B;
3. issue STREAM FORWARD on a dedicated SAM socket pointing to echo/server;
4. create session A and CONNECT to B;
5. verify SAM opens a new loopback TCP connection to the target;
6. non-silent target receives A's public Destination line then exact raw bytes;
7. reply bytes traverse back to A exactly;
8. second A stream produces a second target connection while FORWARD remains active;
9. closing forward command socket stops future forwarding but does not corrupt already active stream policy;
10. non-loopback HOST is rejected;
11. local target refusal/timeout does not orphan inbound Streaming state.

Also test `SILENT=true`: forwarded target receives raw application byte as first byte, with no peer-Destination line.

## 16. Tests for NAMING LOOKUP

Required:

- `NAME=ME` on session context -> exact session public Destination;
- `ME` without session -> deterministic failure;
- full public Destination -> canonical success;
- malformed Base64 -> INVALID_KEY;
- known local destination/hash form if existing resolver supports it;
- unknown `.i2p` -> KEY_NOT_FOUND without system DNS;
- unknown/uncached b32 requiring unavailable network -> explicit not-found/failure, bounded timeout if a lookup was attempted;
- `OPTIONS=true` -> explicit unsupported M7 result;
- oversized name -> bounded rejection.

## 17. Expected files changed

Likely:

```text
crates/i2pr-api/src/sam/forward.rs
crates/i2pr-api/src/sam/naming.rs
crates/i2pr-api/src/sam/session.rs
crates/i2pr-api/src/sam/registry.rs
crates/i2pr-api/src/sam/limits.rs
crates/i2pr-daemon/src/sam.rs
crates/i2pr-daemon/src/config.rs
crates/i2pr-api/tests/sam_forward.rs
crates/i2pr-api/tests/sam_naming.rs
docs/security-model.md        # may be finalized here or Plan 140
docs/protocol-support.md      # may be staged here, finalized Plan 140
```

No new generic DNS/address-book dependency.

## 18. Acceptance criteria

Plan 139 closes only when:

1. STREAM FORWARD works with a real loopback target server;
2. forward command socket owns the registration and its close removes it;
3. ACCEPT/FORWARD mutual exclusion is atomic and tested;
4. multiple forwarded streams are independently bounded;
5. non-loopback forward targets are rejected;
6. SILENT/non-silent forwarded first-byte behavior is correct;
7. NAME=ME returns exact session public Destination;
8. full public Destination naming lookup succeeds canonically;
9. unresolved human-readable `.i2p` names do not invoke system DNS and return explicit failure;
10. naming lookups are bounded/cancellable;
11. all unsupported SAM styles/newer features have explicit tested results;
12. aggregate SAM task/client/session/stream/buffer limits are enforced at exact boundaries;
13. cancellation with all resource categories active returns accounting to baseline;
14. trace-capture tests prove secret/payload redaction;
15. no non-loopback SAM listener or forward pivot is introduced;
16. workspace gates pass;
17. `plans/139-status.md` records the resource table, unsupported matrix, and sets `next_executable_plan = 140`.

## 19. Handoff checklist

```text
[ ] Plan 138 CONNECT/ACCEPT product path passed
[ ] FORWARD is loopback-target-only and lifecycle-owned
[ ] ACCEPT/FORWARD mutual exclusion is race-safe
[ ] NAME=ME and full-Destination lookup work
[ ] no address book/system DNS was invented
[ ] unsupported versions/styles/options are explicit
[ ] aggregate resource accounting is complete
[ ] privacy logging tests pass
[ ] no live-router prerequisite was introduced
[ ] Plan 139 status is committed
```

Proceed next to **Plan 140** for independent-client interoperability and Milestone 7 closure.

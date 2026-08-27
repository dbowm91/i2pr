# Plan 135 — Milestone 7 SAM 3.1 implementation roadmap

Status: **active Milestone 7 planning authority**.

Date: 2026-08-27.

Source product floor: Plan 134 (`passed-milestone6-recv-window-ack-ceiling-closure`).

## 1. Purpose

Milestone 7 must turn the completed local destination + Streaming product from Milestone 6 into a real application-facing API without reopening the blocked live-router interoperability work that dominated earlier milestones.

The implementation target is a bounded, loopback-first **SAM v3.1 STREAM baseline** implemented as a thin adapter over `i2pr-client`. SAM must not become a second destination implementation, a second Streaming state machine, or an alternate route around LeaseSet2, ECIES/Garlic, destination tunnels, or the Plan 129–134 Streaming correctness work.

The Milestone 6 progression state remains:

```text
milestone6_local_product    = passed
milestone6_interoperable    = not-yet-claimed
external_acceptance_debt    = retained-separately
router_construction         = may-continue
next_product_layer          = SAM baseline
```

Do not create another Milestone 6 closure pass while executing this roadmap unless implementation exposes a new concrete defect in an M6 production API.

## 2. Roadmap amendment

The original Milestone 7 section in `plans/000-mvp-roadmap.md` omitted `DEST GENERATE`. Plan 135 is the authoritative amendment until Plan 140 folds the correction back into the top-level roadmap.

The Milestone 7 baseline is:

- `HELLO VERSION` negotiation with SAM 3.1 as the declared supported baseline.
- `DEST GENERATE SIGNATURE_TYPE=7`.
- `SESSION CREATE STYLE=STREAM` using either `DESTINATION=TRANSIENT` or a valid serialized private destination.
- Session ownership and destruction on control-socket loss.
- `STREAM CONNECT`.
- `STREAM ACCEPT`.
- `STREAM FORWARD` for the ordinary TCP-forwarding subset.
- `NAMING LOOKUP`, including `NAME=ME` when a session context is available.
- Protocol-correct status/error responses for unsupported versions, styles, options, malformed commands, duplicate IDs, duplicate destinations, lookup failure, connect failure, and resource exhaustion.
- Loopback-only default listener.
- Explicit per-listener, per-client, per-session, per-stream, line-size, token-count, option-count, and buffered-byte limits.

Official SAM guidance identifies the basic TCP-only client sequence as `HELLO VERSION MIN=3.1 MAX=3.1`, `DEST GENERATE SIGNATURE_TYPE=7`, `NAMING LOOKUP`, `SESSION CREATE STYLE=STREAM`, `STREAM CONNECT`, and `STREAM ACCEPT`. Phase 7 should implement that ordinary path first and treat newer SAM surfaces as later work.

## 3. Explicit version policy

Implement and advertise **SAM 3.1** as the first interoperable API version.

Do not opportunistically advertise SAM 3.2 or 3.3 merely because individual fields or behaviors happen to exist internally. Version claims are compatibility contracts.

Milestone 7 non-goals include:

- `STYLE=DATAGRAM`, `RAW`, `DATAGRAM2`, or `DATAGRAM3`.
- `STYLE=PRIMARY` / `MASTER` and `SESSION ADD` / `SESSION REMOVE`.
- SAM UDP datagram listener support.
- SAM authentication (`USER` / `PASSWORD` / `AUTH`).
- SAM TLS/SSL listener support.
- SAM 3.2 port-option claims as an advertised API feature.
- SAM 3.3 primary/subsession semantics.
- Proposal 163 datagram modes.
- Proposal 167 `NAMING LOOKUP OPTIONS=true` service-record behavior.
- address-book application implementation.
- network-facing SAM bind addresses by default.
- changes to router-to-router transports.
- public-network interoperability claims.

If a post-3.1 option is received, the parser may understand enough structure to reject it cleanly, but the server must not silently accept semantics it does not implement.

## 4. Architecture decision

Create `crates/i2pr-api` as the application-protocol adapter layer already anticipated by the workspace roadmap.

Target dependency direction:

```text
i2pr-proto / i2pr-crypto / i2pr-core
                  ↑
             i2pr-client
                  ↑
              i2pr-api
                  ↑
             i2pr-daemon
```

The exact minimal dependency set should be selected during Plan 136, but these boundaries are mandatory:

1. `i2pr-client` must not depend on `i2pr-api`.
2. SAM command names, reply strings, parsing rules, socket ownership, and client lifecycle state must live in `i2pr-api` or daemon composition, never in `i2pr-client`.
3. `i2pr-api` must consume destination and Streaming APIs rather than copying their state machines.
4. `i2pr-daemon` remains the composition root and owns Tokio socket tasks.
5. Protocol parsing/state transition logic should remain deterministic and testable without opening a real socket; socket integration tests are added above it.
6. No SAM code may bypass `StreamingDestinationAdapter` / destination routing merely to make tests pass.

## 5. Required execution sequence

Execute in this order. Do not combine later phases into an early large patch.

### Plan 136 — SAM 3.1 protocol and private-destination foundation

Build the strict command/reply model, bounded parser, SAM Base64 handling needed by this surface, version negotiation model, and the private-destination key container/codec required by `DEST GENERATE` and `SESSION CREATE`.

This plan must preserve the current secret-ownership model: it must not add a general public accessor for raw destination private keys.

### Plan 137 — loopback server and session lifecycle

Create the runtime-neutral SAM server/session state model plus Tokio loopback listener composition. Implement HELLO, DEST GENERATE, SESSION CREATE, session-ID uniqueness, destination uniqueness, control-socket ownership, deterministic teardown, and bounded client/session admission.

### Plan 138 — STREAM socket bridge

Bridge SAM `STREAM CONNECT` and `STREAM ACCEPT` sockets to the existing per-destination `StreamingManager` through the Milestone 6 production path. Implement bounded bidirectional byte movement, connection establishment/status handling, close/reset mapping, cancellation, and backpressure.

### Plan 139 — STREAM FORWARD, naming, and resource hardening

Add ordinary `STREAM FORWARD`, `NAMING LOOKUP`, `NAME=ME`, explicit unsupported behavior for address-book/network-dependent cases, forward-listener ownership, remaining parser/state negative tests, and resource-exhaustion/backpressure closure.

### Plan 140 — independent-client interoperability and Milestone 7 closure

Run real localhost TCP interoperability against at least two independent SAM clients or independently implemented client fixtures, prove the mandatory STREAM vertical slice, close malformed-sequence/lifecycle/resource tests, update documentation/status, and declare only the SAM-local product claim actually demonstrated.

## 6. Product vertical slice

The first useful target is not parser completeness. It is this path:

```text
SAM client A                                SAM client B
    |                                           |
HELLO 3.1                                   HELLO 3.1
    |                                           |
DEST GENERATE / SESSION CREATE             DEST GENERATE / SESSION CREATE
    |                                           |
    +---- STREAM CONNECT ----------------------> STREAM ACCEPT
                          |
                    i2pr-api adapter
                          |
                    i2pr-client Streaming
                          |
                 destination/garlic/tunnel seam
                          |
                    peer local destination
                          |
                  bidirectional exact bytes
```

For the constrained development environment, the authenticated router-link portion may remain the existing explicitly named local bypass seam. The SAM implementation must still traverse the actual local destination + Streaming product architecture. Direct `StreamingManager`-to-`StreamingManager` test wiring is not sufficient as the Milestone 7 product acceptance test.

## 7. Destination key contract

SAM requires an application-visible private destination representation. Current `DestinationIdentity` deliberately owns secrets and exposes no signing-private-key accessor. Preserve that design.

Plan 136 must introduce a narrow secret-bearing type or codec with properties equivalent to:

```text
SamPrivateDestination
  owns destination public structure
  owns encryption private material
  owns signing private material
  is non-Clone unless ownership semantics are explicit
  Debug => redacted
  temporary decoded buffers => zeroized
  strict decode => full input consumption + exact key lengths/types
  serialize => SAM-compatible private destination bytes/Base64
  consume/reconstruct => DestinationIdentity
```

The adapter may need a carefully scoped construction/export seam in `i2pr-client::identity`, but do not add generic `private_key_bytes()` methods callable throughout the workspace.

Milestone 7 baseline supports the destination cryptographic profile already implemented by i2pr: Ed25519 signing (`SIGNATURE_TYPE=7`) plus the existing ECIES-X25519 destination encryption profile. If another requested signature type is not implemented by the destination product, return a protocol-correct error rather than silently substituting type 7.

## 8. Session ownership model

One successful `SESSION CREATE STYLE=STREAM` establishes a long-lived SAM session associated with:

- one globally unique SAM session ID/nickname;
- one control TCP connection;
- one owned i2pr local destination;
- one per-destination Streaming manager/adapter context;
- zero or more stream sockets attached by session ID;
- zero or more pending `STREAM ACCEPT` sockets within configured limits;
- zero or one ordinary `STREAM FORWARD` binding per supported baseline policy.

Control connection loss is authoritative session cancellation. Teardown must:

1. reject new stream attachments;
2. cancel pending accepts/connects;
3. close or reset active Streaming connections according to the adapter policy;
4. stop any forward listener/task;
5. remove the destination from the router-local registry;
6. release all queue reservations and task/resource leases;
7. remove the SAM session ID from the global registry;
8. make repeated teardown idempotent.

A data/stream socket closing must close only its mapped stream; it must not destroy the parent session unless the SAM protocol requires it for that command path.

## 9. Backpressure and buffering invariant

Do not create an unbounded buffering layer above Streaming.

The receive path is conceptually:

```text
remote Streaming receive/reorder window
    -> i2pr-client delivered-byte queue
    -> bounded SAM stream bridge buffer
    -> local TCP socket
    -> application
```

The send path is:

```text
application TCP socket
    -> bounded read chunk / pending-byte budget
    -> StreamingManager::send_data
    -> existing send/congestion window
    -> StreamingDestinationAdapter
    -> destination routing
```

The adapter must stop reading when the downstream budget is exhausted and resume only after capacity becomes available. It must not continuously drain `StreamingManager::drain_delivered()` into a second unbounded queue, and it must not acknowledge application consumption that has not actually been admitted to the bounded SAM bridge policy.

Plan 138/139 tests must include a non-reading local application, an overproducing local application, and simultaneous streams to prove bounded memory behavior.

## 10. Parser/security requirements

All SAM input is untrusted, even on loopback.

Define named constants for at least:

- maximum command-line bytes before newline;
- maximum tokens per command;
- maximum option count;
- maximum key/name/session-ID lengths;
- maximum quoted-value length;
- maximum simultaneous TCP clients;
- maximum sessions;
- maximum attach sockets per session;
- maximum pending ACCEPT sockets per session;
- maximum forward listeners;
- maximum buffered bytes per SAM stream direction;
- handshake/idle/attach timeouts where wall-clock runtime behavior is required.

Parser behavior must be linear in accepted input size. Reject oversized lines before allocating proportional secondary structures. Duplicate critical options must have a deterministic policy; default preference is reject rather than last-write-wins for identity/session/destination-bearing fields.

Never log `PRIV`, decoded private key bytes, full session private destinations, raw application payloads, ECIES secrets, or stream contents. Session IDs and remote destinations should follow the existing privacy-aware logging policy rather than being dumped by default.

## 11. Testing strategy

Use three levels.

### A. Pure protocol/state tests

No sockets. Feed bounded lines into parser/state handlers. Cover canonical commands, case normalization where required, quoting/escaping supported by the baseline, malformed quoting, duplicate options, missing fields, extra fields, oversized input, unsupported versions/styles/options, and response encoding.

### B. Loopback socket integration

Bind to `127.0.0.1:0`, use real Tokio TCP sockets, verify framing/flush behavior, partial writes, split command lines, multiple commands where allowed, disconnect races, cancellation, and task cleanup.

### C. Product trajectory

Two SAM clients create sessions and exchange exact bidirectional byte streams through the real `i2pr-api -> i2pr-client -> StreamingDestinationAdapter -> destination routing` local product path. Include multiple streams, ordinary loss/reorder injection below SAM where the existing deterministic seam permits it, graceful close, reset/failure, and slow-reader backpressure.

Do not introduce Docker, rootless namespaces, Multipass, public I2P access, Java-router harnessing, or new Python orchestration as a prerequisite for Phase 7.

## 12. Independent-client definition

Milestone 7 exit requires at least two independently implemented SAM client consumers. Preferred evidence order:

1. two maintained external SAM client libraries that can run against localhost without a live I2P network;
2. one maintained external library plus one minimal test client implemented independently of the server parser;
3. if ecosystem/library constraints prevent two external libraries, two independently implemented language clients with frozen command transcripts and an explicit limitation note.

A Rust test helper that directly imports `i2pr-api` parser types is **not** an independent client.

Plan 140 must record exact client names, versions/commits, commands exercised, and evidence boundaries.

## 13. Required documentation changes during execution

By Plan 140 closure, update at least:

- `plans/000-mvp-roadmap.md` — add `DEST GENERATE`, state SAM 3.1 STREAM baseline, and retain external router interoperability as separate debt.
- `README.md` — replace stale Plan 129 frontier language with current Plan 134/M7 status and list implemented SAM subset accurately.
- `docs/protocol-support.md` — command/version support matrix with explicit unsupported entries.
- `docs/architecture.md` — `i2pr-api` dependency/lifecycle position.
- `docs/security-model.md` — loopback API threat surface, private destination handling, session/stream resource ceilings, and logging rules.
- any configuration/reference documentation introduced by the listener.

Do not turn README into another plan chronology. Summarize current product state and link to status documents.

## 14. Global acceptance criteria

Milestone 7 may close only when all are true:

1. `i2pr-api` exists and dependency-direction checks pass.
2. SAM 3.1 is the only version claimed unless additional versions are separately proven.
3. `DEST GENERATE SIGNATURE_TYPE=7` returns a public destination and a private destination that round-trip through `SESSION CREATE`.
4. `SESSION CREATE STYLE=STREAM` works for both `TRANSIENT` and imported private destinations.
5. duplicate session IDs and duplicate destinations are rejected without leaks.
6. `STREAM CONNECT` and `STREAM ACCEPT` move bidirectional exact bytes through the actual local destination + Streaming product path.
7. ordinary `STREAM FORWARD` works for the declared baseline subset.
8. `NAMING LOOKUP NAME=ME` works in session context; other lookup behavior is explicit and protocol-correct for the data i2pr actually possesses.
9. unsupported styles/options/versions fail explicitly rather than being ignored.
10. partial lines, oversized lines, malformed quoting/options, command-order violations, disconnects, and cancellation are bounded and leak-free.
11. slow readers/writers cannot cause unbounded buffering.
12. control-socket loss tears down the whole SAM session and destination; stream-socket loss tears down only the owning stream.
13. at least two independent SAM clients complete the required localhost STREAM trajectories.
14. workspace format/check/test/clippy/doc/dependency-direction gates pass.
15. documentation accurately distinguishes `SAM local interoperability = passed` from `mixed-router/public-network interoperability = unclaimed`.

## 15. Handoff rules

For each numbered plan:

- read this roadmap first;
- execute only that plan's scope;
- do not broaden SAM version claims;
- do not reopen M6 external harness work;
- do not bypass production destination/Streaming seams for convenience;
- add focused tests with the implementation rather than postponing them to Plan 140;
- stop and create a narrowly scoped corrective plan only for a newly demonstrated protocol/product defect, not for speculative hardening;
- update the plan status file at closure with exact commands and evidence.

The next executable implementation plan is **Plan 136**.
# Plan 052/055 reference trigger contracts

This document is the source-inspection record required by Plan 052 F2 and
extended by Plan 055. It records, for each pinned reference router, the
candidate source paths and symbols that could provide a direct NTCP2
transport dial seam usable by the Plan 045/055 reference-initiated
directions.

The document is a working document; entries are added only after the
symbol has been confirmed against the pinned revision. Symbols that are
not present in the pinned revision, or that depend on streaming, tunnel,
or floodfill infrastructure, are explicitly rejected.

## Conventions

- **revision** — the full 40-character pinned commit SHA.
- **source path** — repository-relative path inside the pinned source tree.
- **symbol** — exact symbol or class name as defined in the source.
- **call graph** — sketch of how the symbol reaches the NTCP2 transport
  connection handler.
- **prerequisites** — NetDB, tunnel, or floodfill requirements.
- **disposition** — `selected`, `rejected`, `pending-source-inspection`.

## Java I2P 2.12.0 (`2800040deee9bb376567b671ef2e9c34cf3e30b6`)

| Field | Value |
| --- | --- |
| revision | `2800040deee9bb376567b671ef2e9c34cf3e30b6` |
| version | `2.12.0` |
| artifact SHA-256 | pinned under `tests/integration/ntcp2/references.lock.toml` |
| installed-tree SHA-256 | pinned under `tests/integration/ntcp2/references.lock.toml` |
| source repository | `https://github.com/i2p/i2p.i2p.git` |

### Source-inspected seam inventory

The pinned revision exposes the following candidate transport seams.
Each was verified against `2800040deee9bb376567b671ef2e9c34cf3e30b6`
during Plan 055 Workstream C.

#### 1. `net.i2p.router.transport.ntcp.NTCPTransport`

- Source path: `router/java/src/net/i2p/router/transport/ntcp/NTCPTransport.java`
- Class symbol: `NTCPTransport extends TransportImpl`
- File length: 2013 lines, class declared at line 68.
- Outbound entry point: `outboundMessageReady()` at line 373.
- Bid evaluation: `bid(RouterInfo toAddress, int dataSize)` at line 522.
- Target address lookup: `getTargetAddress(RouterInfo target)` at line 615.
- Constructor: `NTCPTransport(RouterContext ctx, X25519KeyFactory xdh)`
  at line 153 (requires `RouterContext`).

Call graph for outbound NTCP2 from `outboundMessageReady()`:

```text
NTCPTransport.outboundMessageReady()
  -> OutNetMessage msg = getNextMessage()
  -> RouterInfo target = msg.getTarget()
  -> RouterAddress addr = getTargetAddress(target)
  -> NTCPConnection con = new NTCPConnection(_context, this, ident, addr, version)
  -> _conByIdent.put(ih, con)   // map insert happens BEFORE establishment
  -> OutboundNTCP2State runs the NTCP2 Noise handshake
```

Prerequisites:

- The Java router must be initialized through a full
  `RouterContext` (I2P core); the transport holds it as
  `private final RouterContext _context` (line 70) and uses
  `_context.banlist()`, `_context.netDb()`, `_context.clock()`,
  `_context.statManager()` throughout.
- The outbound path requires `getNextMessage()` to return an
  `OutNetMessage`, which the harness must construct with a target
  `RouterInfo`. Java's transport stack pushes messages through
  the outbound queue only via the `TransportManager` and the
  client-layer message builders — there is no public seam that
  accepts a single `RouterInfo` and runs a transport-level dial
  without a registered client destination.
- NetDB lookup of the destination hash is required (the
  `TransportImpl` base class dispatches via `_context.netDb()`).
- The dispatch path uses the streaming manager and tunnel pool to
  decide outbound transport priority. A direct NTCP2 dial that
  bypasses the streaming manager is not a supported API surface.

Disposition (Plan 055 C5): `rejected-global-context-not-isolatable`.
A direct Java helper that drives `outboundMessageReady` would have to
initialize a full `RouterContext`, populate the NetDB with the
imported i2pr RouterInfo, and stage a synthetic `OutNetMessage`. The
Java 2.12.0 source makes the global context a hard requirement for
any non-streaming outbound test, and constructing two isolated
contexts in a single process is not supported. "Could not get it
working" is explicitly rejected by Plan 055 C5; this rejection cites
the global `_context` and the `_conByIdent.put` precondition above.

#### 2. `net.i2p.router.transport.ntcp.NTCPConnection`

- Source path: `router/java/src/net/i2p/router/transport/ntcp/NTCPConnection.java`
- Class symbol: `NTCPConnection` (1909 lines)
- Connection lifecycle: `finishEstablishment()` at line 1438 sets
  `_establishedOn = _context.clock().now()` and triggers the
  `SessionConfirmed` marker on the Java side. The catalog's
  `NTCP2 connection established` log marker is not emitted by the
  pinned source; the connection state is observable through the
  rate stat `ntcp.outboundEstablishFailed` / successful path and
  the `_establishedOn` field. The catalog entry was updated under
  Plan 054 to use the existing structured rate stat; Plan 055 does
  not relax this.

Prerequisites: requires `NTCPTransport` (already instantiated) and
a running NTCP2 transport stack.

Disposition: supporting module; not a candidate direct helper seam.

#### 3. `net.i2p.router.transport.ntcp.OutboundNTCP2State`

- Source path: `router/java/src/net/i2p/router/transport/ntcp/OutboundNTCP2State.java`
- Class symbol: `OutboundNTCP2State`
- Constructor (line 102): `public OutboundNTCP2State(RouterContext
  ctx, NTCPTransport transport, NTCPConnection con)`
- The class performs the Alice-side Noise handshake and emits the
  Java-side `SessionConfirmed` log marker via
  `LogPrint(eLogDebug, "NTCP2: SessionConfirmed sent")` (matches the
  Plan 054 catalog handshake-only marker).

Prerequisites: requires a constructed `NTCPConnection` and the
transport's NTCP2 context.

Disposition: lower-level transport helper, not a direct dial seam.

#### 4. SAM v3 `SESSION CREATE STYLE=STREAM DESTINATION=<base64>` (existing)

The existing SAM trigger remains a fallback but is explicitly
out-of-scope for the Plan 055 direct-helper work because it
requires a registered destination and outbound tunnel pool, which
the plan forbids in Workstream D1 unless an ADR justifies a
support topology.

Disposition: candidate for `java-minimal-support-topology` if the
Plan 055 Workstream D ADR is approved; otherwise rejected.

### Plan 055 C5 direct-helper decision

```text
java-direct-helper-rejected-global-context-not-isolatable
```

Rationale: the Java I2P 2.12.0 outbound path requires a full
`RouterContext`, a populated NetDB, and the `_conByIdent.put`
precondition. There is no public API surface that accepts a
synthetic `RouterInfo` and dispatches a transport-level outbound
dial without those prerequisites. The Plan 055 source-inspected
call graph is recorded in this document; a test-only helper would
have to initialize the global context (a hard requirement per the
`NTCPTransport` constructor at line 153), which Plan 055 C5
explicitly forbids.

A minimal sealed support topology remains an option under
Plan 055 Workstream D. ADR 0021
(`docs/adr/0021-minimal-java-support-topology.md`) records the
architecture decision when Java is the only reference that cannot
be qualified through a direct helper.

### Required follow-up

A test-only helper compiled against the pinned router jars must:

- Source-link against the pinned revision's jars without transport
  behavior patches.
- If a direct helper is selected, it must initialize or attach to
  the pinned router context deterministically, import the exact i2pr
  RouterInfo, request one transport-level send/connect to that
  RouterInfo, avoid requiring a client destination or tunnel pool,
  and expose a bounded completion callback/status.
- If the direct helper is rejected, the harness must fall back to a
  Plan 055 D ADR-approved minimal sealed support topology.

This implementation is gated on the Plan 055 C4 control experiments
(positive, wrong-RouterInfo, wrong-address, no-trigger, reference-code).
Until the helper exists or an ADR is approved, the Java
reference-initiated direction is a typed blocker.

## i2pd 2.60.0 (`f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`)

| Field | Value |
| --- | --- |
| revision | `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e` |
| version | `2.60.0` |
| artifact SHA-256 | pinned under `tests/integration/ntcp2/references.lock.toml` |
| installed-tree SHA-256 | pinned under `tests/integration/ntcp2/references.lock.toml` |
| source repository | `https://github.com/PurpleI2P/i2pd.git` |

### Source-inspected seam inventory

The pinned revision exposes the following candidate transport seams.
Each was verified against `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`
during Plan 055 Workstream B.

#### 1. `i2pd::transports::Transports::ConnectToPeer`

- Source path: `libi2pd/Transports.cpp`
- Class symbol: `i2pd::transport::Transports`
- File length: 1592 lines, implementation at line 574.
- Header path: `libi2pd/Transports.h` (286 lines, declaration at line 210).
- Visibility: `private:` member function (declared at
  `libi2pd/Transports.h:210`). Not exposed in the public header
  surface; reachable through `SendMessage` (line 469) and
  `SendMessages` (line 476).
- Public sending entry points:
  `Transports::SendMessage(const IdentHash&, std::shared_ptr<I2NPMessage>)`
  at `Transports.cpp:469`.
  Returns `std::future<std::shared_ptr<TransportSession>>`.

Call graph for outbound NTCP2 from `SendMessage`:

```text
Transports::SendMessage(ident, msg)
  -> SendMessages(ident, { msg })
     -> PostMessages(ident, msgs)
        -> if (no Peer found) get Peer or create one
        -> peer->SendMessage(msg)  // schedules the message
     -> if (!m_Peers[ident]->IsConnected ())
        -> ConnectToPeer(ident, peer)         // line 521 in Transports.cpp
           -> peer->router is loaded by NTCP2Server->GetRouter()
           -> std::shared_ptr<NTCP2Session> = ...(*m_NTCP2Server, peer->router, address)
           -> m_NTCP2Server->Connect(s)        // line 609 in Transports.cpp
```

The `ConnectToPeer` body (line 574) walks `peer->priority`, which is
the transport-selection order (`eNTCP2V6`, `eNTCP2V4`, `eSSU2V6`,
`eSSU2V4`); it constructs an `NTCP2Session` and hands it to
`NTCP2Server::Connect` when an NTCP2 address is reachable. The
handshake marker `NTCP2: SessionConfirmed sent` is emitted by
`NTCP2Session::SendSessionConfirmed()` at `libi2pd/NTCP2.cpp:916`
during the outbound handshake completion.

Prerequisites:

- A `RouterInfo` must be loaded into the local NetDB with the
  target's `IdentHash`. `ConnectToPeer` reads `peer->router`; if it
  is `nullptr` it tries `netdb.FindRouter(ident)` (line 578). A
  RouterInfo absent from the local NetDB triggers a deferred
  `RequestDestination` (line 665) and the call returns `true`
  without dispatching any transport session.
- The single i2pd `transports` global singleton (declared at
  `libi2pd/Transports.h:279`) must be `Start()`ed before any
  `SendMessage` call (its `m_NTCP2Server` must be non-null).
- No streaming/tunnel/SAM infrastructure is required for the
  `SendMessage` path itself; i2pd routes the outbound I2NP message
  through `m_NTCP2Server->Connect(s)`, which performs the NTCP2
  Noise handshake and emits `NTCP2: SessionConfirmed sent`
  directly.

Disposition (Plan 055 B5): `i2pd-direct-helper-selected`.

#### 2. `i2pd::data::RouterInfo::Load`

- Source path: `libi2pd/RouterInfo.cpp`
- Symbol: `i2pd::data::RouterInfo::Load`
- Call graph: parses a RouterInfo file from disk and returns a
  `std::shared_ptr<RouterInfo>`.
- Prerequisites: none beyond the file itself.

Disposition: supporting helper for `ConnectToPeer`.

#### 3. `i2pd::data::netdb::AddRouterInfo`

- Source path: `libi2pd/NetDb.cpp` and `libi2pd/NetDb.hpp`
- Symbol: `i2pd::data::netdb.AddRouterInfo`
- Call graph: parses a RouterInfo buffer and inserts it into the
  in-memory NetDB so `ConnectToPeer` can locate it via
  `netdb.FindRouter(ident)`.
- Prerequisites: requires the i2pd `netdb` singleton to be
  initialized, but no public network access.

Disposition: required supporting helper for `ConnectToPeer`.

#### 4. SAM v3 `STREAM CONNECT` (existing)

- Source path: `daemon/SAM.cpp`
- Prerequisites: requires a SAM bridge, a registered destination,
  outbound tunnel pool.
- Disposition: rejected for transport-only test, depends on streaming.

#### 5. Webconsole `run_peer_test` (existing)

The i2pd webconsole `run_peer_test` endpoint calls
`Transports::PeerTest` (declared at `libi2pd/Transports.h:193`),
which is an SSU2-only peer-probe; when SSU2 is disabled
`PeerTest` returns early and never dispatches an NTCP2 dial.
Disposition: rejected for transport-only test.

### Plan 055 B2 preferred helper design

```text
tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/
  CMakeLists.txt
  main.cpp
  README.md
```

The helper must:

1. Parse a strict config JSON supplied by the harness.
2. Load the imported i2pr RouterInfo from a run-owned file via
   `i2pd::data::RouterInfo::Load`.
3. Validate the synthetic NTCP2 endpoint, the SHA-256 of the loaded
   RouterInfo, and the correlation nonce.
4. Initialize the i2pd context (`i2p::context`), start the local
   netdb singleton, and start the transports subsystem with SSU2
   disabled (`Transports::Start(true /* enableNTCP2 */, false
   /* enableSSU2 */)`).
5. Insert the loaded RouterInfo via
   `i2pd::data::netdb::AddRouterInfo` so `ConnectToPeer` can
   locate it.
6. Construct an `I2NPMessage` carrying the bounded DeliveryStatus
   payload (I2NP type 10, 12-byte body) and invoke
   `transports.SendMessage(ident, msg)` **exactly once**.
7. Wait on the returned `std::future<shared_ptr<TransportSession>>`
   with a bounded timeout (default 15 seconds, matching the
   `SESSION_CREATION_TIMEOUT` constant in `libi2pd/Transports.h`).
8. Emit one structured trigger JSON record (Plan 055 A1 schema)
   carrying the dispatch outcome, the connection callback
   observation, the bounded monotonic timestamps, and the
   sanitized detail. Never retry silently.
9. Exit.

It must not create a streaming destination or tunnel pool; the
transport-level `SendMessage` path is the preferred seam. The helper
links against the unmodified pinned libraries and is source-locked
to the locked revision in
`tests/integration/ntcp2/references.lock.toml`.

### Plan 055 B3 running-router vs embedded-helper decision

Plan 055 B3 prefers the **embedded test process** model: i2pd
initializes a minimal transport context inside the helper process
using the pinned libraries. The helper's lifecycle is bounded to a
single trigger attempt; the i2pd global singleton is created and
destroyed inside the helper process so it cannot leak between
attempts. The Plan 055 B2 design above implements this model.

### Plan 055 B4 control experiments

The i2pd direct helper is qualified through the following eight
controls (Plan 055 B4):

1. Correct RouterInfo + endpoint: one outbound attempt reaches i2pr
   and the trigger record records `outcome = authenticated`.
2. Correct RouterInfo, wrong expected hash: helper rejects before
   `SendMessage` is invoked; `outcome = rejected-target-router-info`.
3. Correct hash, wrong synthetic endpoint: helper rejects or the
   transport call fails; the direction cannot pass.
4. No helper invocation: i2pd does not spontaneously create the
   target connection in the bounded window; the trigger record
   records `attempted = false` and `outcome =
   direct-trigger-helper-failed`.
5. Duplicate helper invocation: each invocation produces a separate
   trigger record; neither can be merged into the first attempt's
   digest.
6. Unknown RouterInfo: helper rejects; `outcome =
   rejected-target-router-info`.
7. Reference unavailable: helper returns a typed
   `reference-unavailable` outcome with a bounded error code.
8. Successful trigger plus malformed i2pr responder: the trigger
   record records `outcome = authenticated` while the direction
   record carries a Plan 052 G1 bounded responder reason
   (e.g. `responder-session-confirmed-part2-failed`); the trigger
   outcome does not mask the direction result.

### Plan 055 B5 acceptance

The i2pd direct helper is qualified when:

- The correct-target positive control reaches the full Plan 052
  directional predicate (both-side `ntcp2_authenticated`,
  `frame_emitted` on the reference side, and
  `frame_authenticated_and_decrypted` + `i2np_message_decoded` on
  the i2pr side).
- Every negative control fails in the intended stage.
- The Plan 055 trigger record digest binds to the same
  `run_identity_sha256`, `correlation_nonce`, and `target_*` digests
  used by the Plan 052 direction record (Plan 055 E2).

## Decision matrix

| Reference | Direct transport seam | Decision |
| --- | --- | --- |
| Java I2P 2.12.0 | none usable | **Decision 055.1**: `java-direct-helper-rejected-global-context-not-isolatable`; ADR 0021 governs the optional `java-minimal-support-topology` fallback. |
| i2pd 2.60.0 | `Transports::SendMessage` via `ConnectToPeer` | **Decision 055.2**: `i2pd-direct-helper-selected`. |

The two reference-initiated directions (`java-to-i2pr-ipv4` and
`i2pd-to-i2pr-ipv4`) cannot satisfy the Plan 052 directional
predicate until the helper implementations and/or ADR 0021 are
committed. They remain typed blockers, not skipped successes.

## Reference trigger helper inventory

The committed helper inventory, when added, lives under
`tests/integration/ntcp2/reference-drivers/`:

```text
tests/integration/ntcp2/reference-drivers/
  i2pd_direct_connect/
    CMakeLists.txt
    main.cpp
    README.md
  java_minimal_support_topology/   # only if ADR 0021 is approved
    README.md
```

The `README.md` is the bridge between this document and the
per-reference helper. It cross-references the locked symbols and
the trigger record schema.

## ADR dependency

Plan 055 D1 requires ADR 0021
(`docs/adr/0021-minimal-java-support-topology.md`) before any
`java-minimal-support-topology` helper may be implemented. Until
the ADR is written, the topology alternative is forbidden and the
Java direction remains a typed blocker.

## Acceptance

The trigger contracts in this document close only when:

1. The helper files are committed and source-locked to the pinned
   revisions (i2pd) or the support topology ADR is approved
   (Java).
2. All eight Plan 055 B4 / C4 control experiments pass.
3. The two reference-initiated directions reach `passed` under
   the Plan 052 receiver-side observation predicate.
4. Two complete evidence runs reproduce the four-direction outcome
   (this is the Plan 056 closure).

Until all four conditions are met, this document remains a working
diagnostic record.
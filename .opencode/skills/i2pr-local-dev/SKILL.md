---
name: i2pr-local-dev
description: Work on the local product path of the i2pr Rust I2P router — Milestone 6 (destinations, garlic, LeaseSet2, Streaming) and the SAM 3.1 application-protocol adapter (Plans 136–138) for Milestone 7. Use when an agent is asked to modify, test, or extend i2pr-client, i2pr-netdb, i2pr-tunnel, i2pr-proto, i2pr-crypto, i2pr-daemon destination/streaming/LS2 code paths, or i2pr-api SAM 3.1 protocol/private-destination/stream-registry code paths; write or run the deterministic trajectory tests under crates/i2pr-client/tests/ or the SAM tests under crates/i2pr-api/ and crates/i2pr-daemon/tests/sam_*; exercise the testkit; debug local Milestone 6 trajectories; or plan the next SAM milestone (Plan 139 STREAM FORWARD and naming). Also use when asked to find the canonical closure record for a Milestone 6 / Milestone 7 plan, the active I2P protocol-support state for destinations/garlic/LS2/Streaming/SAM, or the next executable plan-of-record after Plan 138.
---

# I2PR Local Development

The routine development path for the local product side of the router. The
active development interop lane is closed; NTCP2 stays experimental and
non-advertised. The local Milestone 6 product (destinations, garlic, LS2,
Streaming) is closed under corrected local-correctness semantics via
**Plan 134** (`plans/134-m6-recv-window-ack-ceiling-closure.md`,
`plans/134-status.md`); independent-router interoperability is not claimed
and is tracked as external acceptance debt. The current product layer is
**SAM 3.1 baseline implementation**: Plan 136 closed the protocol and
private-destination foundation, Plan 137 closed the loopback listener
and session lifecycle, and Plan 138 closed the STREAM CONNECT / ACCEPT
transport bridge. Plan 139 owns STREAM FORWARD, NAMING LOOKUP, and the
listener / forwarding bridge.

Load this skill whenever the work touches `i2pr-client`,
`i2pr-netdb`, `i2pr-tunnel` (destination-side), `i2pr-proto`,
`i2pr-crypto` (ECIES), `i2pr-api` (SAM), `i2pr-daemon` SAM code
paths, or when an agent needs to navigate the local Milestone 6 / 7
closure history to find the canonical authority for a behavioral claim.

## Active authority

The current Milestone 6 closure authority is **Plan 134**
(`passed-milestone6-recv-window-ack-ceiling-closure`). The current
Milestone 7 SAM 3.1 closure authority is **Plan 138**
(`passed-m7-sam31-stream-connect-accept-bridge`,
[`plans/138-m7-sam31-stream-connect-accept-bridge.md`](../../plans/138-m7-sam31-stream-connect-accept-bridge.md),
[`plans/138-status.md`](../../plans/138-status.md)). Plan 137 closed the
loopback listener and session lifecycle, and Plan 136 closed the
protocol and private-destination foundation. The full plan hierarchy
lives under [`plans/README.md`](../../plans/README.md); the
quick-reference trajectory table below records each plan's authority
for local-product behavioral claims:

| Plan | Status | Authority for |
| --- | --- | --- |
| 138 | `passed-m7-sam31-stream-connect-accept-bridge` | Current Milestone 7 SAM 3.1 STREAM CONNECT / ACCEPT transport bridge. Runtime-neutral `SamStreamRegistry` (FIFO ACCEPT, per-session ceiling); per-destination `SamDestinationBridge` (`Arc<DestinationIdentity>` + `StreamingManager` + signed `LeaseSet2` + `DestinationOutboundRole` + `DestinationRouting` + `EciesSessionManager`); `execute_stream_connect` reports `RESULT=OK` only after `Established`. |
| 137 | `passed-m7-sam31-loopback-server-session-lifecycle` | Milestone 7 SAM 3.1 loopback listener + session lifecycle. `i2pr-api` owns the bounded limits, session registry, line reader, and server state machine; `i2pr-daemon` owns the supervised Tokio listener and the per-destination `StreamingManager` pool. `[sam] enabled = false` by default in production. |
| 136 | `passed-m7-sam31-protocol-private-destination-foundation` | Milestone 7 SAM 3.1 protocol and private-destination foundation. `i2pr-api` runtime-neutral surface. |
| 135 | `active-milestone7-sam31-planning-authority` | Milestone 7 SAM 3.1 roadmap; the broader Phase 7 sequence (Plan 136 → 140). |
| 134 | `passed-milestone6-recv-window-ack-ceiling-closure` | Current Milestone 6 local closure. ACK ceiling defect fix in receive-window tracking. |
| 133 | `passed-evidence-authority-superseded-by-plan134` | Historical evidence; produces eleven Plan 130 + seven Plan 131 trajectories and is retained. |
| 132 | `implementation-landed-evidence-superseded-by-plan133` | Strict Elligator2 receive-domain validation, three layer-isolated replay trajectories, transactional `&mut self` send ordering. |
| 131 | `superseded-by-plan132-and-plan133` | Production Elligator branch randomization (swap to `elligator2 = 0.1.0`); three-layer replay separation; connection-owned I2P port tuple; oversized-send rollback. |
| 130 | `superseded-by-plan131-final-local-correctness-gate` | Historical M6 implementation pass. |
| 129 | `superseded-by-plan130-final-gate` | Integrated destination+Streaming gate. Twelve trajectories in `plan129_trajectory.rs`. |
| 128 | `passed-streaming-wire-protocol-corrective-closure` | Streaming packet wire format to current I2P spec. |
| 127 | `passed-destination-session-routing-final-closure` | Bundled-LS2 sender binding under own Destination hash; `PlannedOutboundForm`; reverse routing via `install_remote_lease_set2`. |
| 126 | `passed-ecies-destination-ratchet-corrective-foundation` | Normative ECIES-X25519-AEAD-Ratchet (replaces superseded Plan 121 dialect). |
| 122 | `passed-corrected-local-destination-routing` | `compose_outbound_delivery`, `DestinationDispatcher`, daemon `NetDbSeam` LS2 lookup. |
| 124 | `passed-plan122-corrective-closure` | Garlic-through-OBEP composition (`garlic_i2np_bytes` is canonical carrier). |
| 119 | `passed-leaseset2-protocol-foundation` | Ordinary Standard LeaseSet2 carrier in `i2pr-proto` and `i2pr-netdb`. |
| 120 | `passed-destination-lifecycle-and-pools` | First `i2pr-client` destination runtime: identity, dedicated tunnel pools, signed Standard LeaseSet2, registry. |

When a closure record and a per-plan narrative disagree, the closure
record wins. Plans retain their narrative as audit history, not as
live contracts.

## Where the local product lives

```text
crates/i2pr-client/
  src/
    identity.rs                # Ed25519 signing + X25519 static keys, non-Clone, non-Debug
                               #   Plan 136 added the narrow `signing_seed_bytes()` accessor
                               #   reserved for the SAM private-destination codec
    registry.rs                # Destination registry with capacity + duplicate rejection
    lifecycle.rs               # LeaseSet2 rotation/withdrawal
    session.rs                 # EciesSessionManager (Plan 126 dialect), classify, outbound form
    routing.rs                 # LeaseSelector, compose_outbound_delivery, DestinationRouting, install_remote_lease_set2
    dispatch.rs                # DestinationDispatcher, bundled-LS2 binding under own DestinationHash
    streaming/                 # 13 sub-files: manager, connection, recv_window, send_window,
                               #   clock, config, congestion, errors, events, retransmit,
                               #   transport, testing, mod
    streaming_adapter.rs       # outbound/inbound StreamingDestinationAdapter
  tests/
    plan124_trajectory.rs      # 11 trajectory tests (Plan 124 Phases A–G)
    plan129_trajectory.rs      # 12 trajectory tests (Plan 129 integrated gate)
    plan130_trajectory.rs      # 11 trajectory tests (Plan 130 wire/runtime)
    plan131_trajectory.rs      # 7 trajectory tests (Plan 131 local correctness)
    plan132_trajectory.rs      # layer-isolated replay trajectories
    plan133_trajectory.rs      # rewrote B2/B3 to retain actual first delivered ES envelope
crates/i2pr-netdb/             # LeaseSet2 validation, store, lookup, publication (Plan 119)
crates/i2pr-tunnel/           # ECIES-X25519 short build, data plane, exploratory pool
crates/i2pr-proto/
  src/streaming/               # StreamingPacket, StreamingPacketBuilder, options, flags
crates/i2pr-crypto/src/ecies.rs # ECIES primitives + elligator2 = 0.1.0
crates/i2pr-daemon/src/netdb_seam.rs # NetDbSeam LS2 lookup surface (Plan 122)
crates/i2pr-api/               # Plan 136 + Plan 137 + Plan 138 SAM 3.1 surface
  src/
    lib.rs                     # facade and re-exports
    sam/
      mod.rs                   # module facade and named byte ceilings
      version.rs               # SamVersion, parse_version, negotiate, is_advertised
      base64.rs                # RFC 4648 SAM Base64 codec (encode/decode, strict)
      command.rs               # Command, CommandKind, OptionPair, CommandOutcome,
                               #   parse_stream_connect / parse_stream_accept (Plan 138)
      parser.rs                # parse_line, tokenise, recognise_* per command family
      reply.rs                 # ReplyLine, Reply, HelloReply, DestReply, SessionStatus,
                               #   StreamStatus (with `result()` accessor), NamingReply, PongReply
      private_destination.rs   # SamPrivateDestination wrapper, from_identity/from_base64/from_bytes, into_identity
      dest_generate.rs         # DestGenerateRequest, dest_generate runtime-neutral core op
      session_create.rs        # SessionCreateRequest, parse_session_create
      limits.rs                # SamLimits + loopback_test_profile (Plan 137)
      session.rs               # SamSessionId, SamSessionCounters (Plan 137)
      registry.rs              # SamSessionRegistry, reserve/commit/rollback (Plan 137)
      line_reader.rs           # LineReader, LineEvent (Plan 137)
      server_state.rs          # ServerConnectionState, dispatch, apply_session_outcome,
                               #   RequireStreamConnect / RequireStreamAccept dispatch outcomes
                               #   (Plan 138)
      streams.rs               # SamStreamRegistry, SamStreamAttachment,
                               #   SamStreamRegistryError, SamStreamRegistryHandle (Plan 138)
crates/i2pr-daemon/src/sam.rs  # SamServiceState, supervised loopback listener,
                               #   execute_stream_connect, execute_stream_accept (Plan 138)
crates/i2pr-daemon/src/sam/streams.rs # SamDestinationBridge, SamDestinations,
                               #   build_sam_destination_bridge, decode_destination_triple (Plan 138)
crates/i2pr-daemon/tests/sam_loopback.rs # 17 integration tests (Plan 137)
crates/i2pr-daemon/tests/sam_stream.rs   # 10 integration tests (Plan 138)
crates/i2pr-testkit/          # deterministic simulation; no production crate may depend on it
```

## Authoritative command surface

Local development uses the testkit exclusively. **No public-network
traffic, no DNS, no wall-clock sleeps.** Default development loop:

```text
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'   # static-boundary unit tests
```

Focused local seams:

```text
cargo test -p i2pr-client --all-targets                              # M6 trajectory suite
cargo test -p i2pr-testkit --all-targets                            # deterministic simulation
cargo test -p i2pr-netdb --all-targets                              # LS2 + RouterInfo
cargo test -p i2pr-crypto --all-targets                             # ECIES primitives + elligator2
cargo test -p i2pr-proto --all-targets                              # wire codecs
cargo test -p i2pr-api --all-targets                               # SAM 3.1 protocol + Plan 137/138 surfaces
cargo test -p i2pr-daemon --test sam_loopback                       # Plan 137 loopback integration tests
cargo test -p i2pr-daemon --test sam_stream                         # Plan 138 STREAM bridge integration tests
```

The forced-cleanup 100-iteration test runs serially:

```text
cargo test -p i2pr-runtime forced_child_cleanup_is_repeatably_joined -- --test-threads=1
```

## SAM baseline implementation (Milestone 7) — current state

The local destination + Streaming product gate is closed (Plan 134);
the SAM 3.1 protocol and private-destination foundation is closed
(Plan 136); the SAM 3.1 loopback server and session lifecycle is
closed (Plan 137); the SAM 3.1 STREAM CONNECT / ACCEPT transport
bridge is closed (Plan 138). Independent-router interoperability is
tracked separately as external acceptance debt. The next
plan-of-record is **Plan 139** (STREAM FORWARD, NAMING LOOKUP, and the
listener / forwarding bridge), then **Plan 140** (Milestone 7 closure
+ mixed-router SAM evidence).

Plan 139 must:

- Honour the Plan 137 `SamSessionId` ownership boundary; never move
  sockets between sessions.
- Reject any new DATAGRAM / RAW / PRIMARY surfaces via the existing
  `CommandOutcome::Unsupported` path; do not silently accept
  semantics the implementation does not support.
- Wire NAMING LOOKUP through the corrected destination-routing
  pipeline (`DestinationRouting` / `install_remote_lease_set2`).
- Do **not** weaken the Plan 136 secret-ownership invariants, the
  Plan 137 transactional session-insert discipline, or the Plan 138
  capture seam.

Before drafting Plan 138 (or any later SAM plan), read:

- `plans/135-m7-sam31-implementation-roadmap.md` (the broader
  Milestone 7 sequence)
- `plans/136-m7-sam31-protocol-private-destination-foundation.md`
  and `plans/136-status.md`
- `plans/137-m7-sam31-loopback-server-session-lifecycle.md`
  and `plans/137-status.md`
- `crates/i2pr-client/src/streaming/` (the corrected wire format and
  receiver ack views per Java `MessageInputStream.updateAcks`)
- `crates/i2pr-client/src/session.rs` (Plan 126 ECIES-X25519-AEAD-Ratchet)
- `crates/i2pr-client/src/routing.rs` (Plan 122/124/127 outbound/inbound)
- `crates/i2pr-daemon/src/netdb_seam.rs` (the bounded NetDB seam)
- `specs/protocols/08-sam.md` (SAM dossier)
- `specs/references/ecies-destination-ratchet.md`,
  `specs/references/streaming-packet-wire.md`, and
  `specs/references/sam31-private-destination.md` for protocol
  provenance
- `docs/adr/` for ADR style and existing decisions (0001..0025)

## Coding conventions that apply to local product work

These are from `AGENTS.md`; the items most likely to bite a new
agent on the local product path:

- **No `unsafe`** anywhere. `#![forbid(unsafe_code)]` is the default
  for protocol, crypto, routing, NetDB, tunnel, client, API, and
  service crates. Workspace lint `unsafe_code = "deny"`.
- **Codec errors are typed enums.** Never swallow codec results. Don't
  encode router policy into wire codecs.
- **Treat all external input as hostile:** explicit bounds, reject
  unknown or trailing bytes, no validation side effects, always test
  the negative path. This applies to LS2, Streaming packets, Garlic
  envelopes, ECIES New Session frames, and SAM commands equally.
- **Use OS CSPRNG** through reviewed interfaces for production
  cryptography; deterministic RNG is allowed only in tests,
  simulation, and explicitly marked reproducibility tools.
- **Library crates avoid `anyhow`** as a public error model. Use typed
  error enums with stable categories.
- **Avoid global mutable state** and unrestricted `Arc<RouterContext>`
  service locators. Each subsystem receives narrow handles or
  capabilities.
- **No `unbounded_channel`** anywhere. No `tokio::*`, `std::net`,
  `std::fs` in transport crates. The runtime boundary check
  (`scripts/check-runtime-boundaries.sh`) is the source of truth.
- **Secret-bearing types are non-`Clone`, non-`Debug` for secrets,**
  zeroize on drop, and compare only on public bytes when they need
  `PartialEq`. `SamPrivateDestination` is the canonical pattern.

## Testing conventions

- Runtime tests use `#[tokio::test(start_paused = true)]` or the
  `i2pr-testkit` `ManualClock` with fixed seeds and bounded steps.
- Trajectory tests must be **deterministic**: fixed seeds, fixed
  clocks, exact-byte equality assertions, and a typed negative path.
- Tests for bounded queues and resource governors cover capacity-of-one,
  exact offered load, and max-plus-one offered load; verify the typed
  full, deadline, cancellation, closure, response-drop, and
  resource-denial outcomes; confirm queue-held leases release on every
  drop path.
- Streaming test fixtures live in `crates/i2pr-client/tests/` and
  reference the canonical RFC 1952 gzip wire format and the
  Plan 128 wire-format corrections.
- SAM test fixtures live in `crates/i2pr-api/src/sam/` and reference
  the standard Java `PrivateKeyFile` concatenation documented in
  `specs/references/sam31-private-destination.md`. The SAM tests must
  cover canonical HELLO 3.1, the full line-length / token-count /
  option-count ceilings, duplicate critical options, unsupported
  styles/options, version negotiation overlaps and disjoints, malformed
  SAM Base64 (alphabet, length, padding, ceiling), frozen round-trip
  of the standard private-destination format, and Debug redaction.
- Commit a regression test that exactly reproduces a defect alongside
  the fix; never close a status document without a tested negative
  path.

## When to load the other skills

- **NTCP2 / mixed-router work** (closed lane, do not extend):
  `i2pr-ntcp2-interop`.
- **Rootless sealed-namespace sandbox** (Plan 046): `i2pr-rootless-sandbox`.
- **Multipass recovery guest** (Plan 048/049/050/051):
  `i2pr-multipass-recovery`.
- **Navigating `docs/architecture/` and ADRs**: `i2pr-architecture`.

## Safety and development rules

- Never claim independent-router NTCP2 interoperability; the local
  product is the active surface.
- Never enable `i2pr-daemon` in the production service graph beyond
  its existing Plan 106 netdb-bootstrap + lifecycle service. Adding
  new services requires a plan-of-record and a hard-boundary test
  (`scripts/check-runtime-boundaries.sh`).
- Never bump a support-row `advertised` flag without sanitized
  interoperability evidence satisfying `specs/CONFORMANCE.md`. The
  default remains `experimental` and `advertised = false`. The Plan 136
  SAM 3.1 protocol foundation surface is `experimental` and
  `advertised = false`.
- Before handoff, run the focused local seam in addition to the
  pre-handoff sequence in `AGENTS.md`.

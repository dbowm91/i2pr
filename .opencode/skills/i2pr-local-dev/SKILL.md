---
name: i2pr-local-dev
description: Work on the local product path of the i2pr Rust I2P router — Milestone 6 (destinations, garlic, LeaseSet2, Streaming) and the next product layer (SAM baseline planning, Milestone 7). Use when an agent is asked to modify, test, or extend i2pr-client, i2pr-netdb, i2pr-tunnel, i2pr-proto, i2pr-crypto, or i2pr-daemon destination/streaming/LS2 code paths, write or run the deterministic trajectory tests under crates/i2pr-client/tests/, exercise the testkit, debug local Milestone 6 trajectories, or plan the next SAM baseline milestone. Also use when asked to find the canonical closure record for a Milestone 6 plan, the active I2P protocol-support state for destinations/garlic/LS2/Streaming, or the next executable plan-of-record after Plan 134.
---

# I2PR Local Development

The routine development path for the local product side of the router. The
active development interop lane is closed; NTCP2 stays experimental and
non-advertised. The local Milestone 6 product (destinations, garlic, LS2,
Streaming) is closed under corrected local-correctness semantics via
**Plan 134** (`plans/134-m6-recv-window-ack-ceiling-closure.md`,
`plans/134-status.md`); independent-router interoperability is not claimed
and is tracked as external acceptance debt. The next product layer is
**SAM baseline planning (Milestone 7)**.

Load this skill whenever the work touches `i2pr-client`,
`i2pr-netdb`, `i2pr-tunnel` (destination-side), `i2pr-proto`, or
`i2pr-crypto` (ECIES), or when an agent needs to navigate the local
Milestone 6 closure history to find the canonical authority for a
behavioral claim.

## Active authority

The current Milestone 6 closure authority is **Plan 134**
(`passed-milestone6-recv-window-ack-ceiling-closure`). The full plan
hierarchy lives under [`plans/README.md`](../../plans/README.md); the
quick-reference trajectory table below records each plan's authority
for local-product behavioral claims:

| Plan | Status | Authority for |
| --- | --- | --- |
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
```

The forced-cleanup 100-iteration test runs serially:

```text
cargo test -p i2pr-runtime forced_child_cleanup_is_repeatably_joined -- --test-threads=1
```

## SAM baseline planning (Milestone 7) — the next product layer

The next product layer is **SAM baseline planning (Milestone 7)**.
The local destination + Streaming product gate is closed (Plan 134);
independent-router interoperability is tracked separately as external
acceptance debt. A SAM plan-of-record does not exist yet — Plan 134
is the active authority until a SAM plan-of-record lands.

When asked to start SAM planning, draft a plan-of-record that:

- Bounds SAM v3.x over loopback (Unix socket or TCP loopback) — the
  production daemon must continue to refuse NTCP2 activation, and the
  SAM bridge must not require NTCP2 or any external-router connection;
- Reuses the `StreamingManager` + `StreamingDestinationAdapter` already
  in `i2pr-client` for the streaming side;
- Defines a typed `DatagramSender`/`DatagramReceiver` set on top of the
  existing Garlic session layer (ECIES-X25519-AEAD-Ratchet, Plan 126)
  with its own datagram-traffic class;
- Reuses the LS2-bound destination identity from `i2pr-client::identity`
  and the registry/lifecycle from Plan 120;
- Reuses the daemon composition root in `i2pr-daemon` but adds a
  bounded SAM bridge service to the `ServiceGraph` (the daemon does
  not yet own a SAM service);
- Specifies sanitized typed acceptance tests for SAMv3 streams,
  datagrams, raw destinations, and the dest/reply/lookup commands —
  all running against the local testkit (no public network);
- Includes a static-boundary check addition to
  `scripts/check-runtime-boundaries.sh` to enforce that the SAM
  service owns its own Tokio child (the existing rule that every
  `tokio::spawn` keeps an explicit owner still applies).

Before drafting, read:

- `plans/134-m6-recv-window-ack-ceiling-closure.md` and `plans/134-status.md`
- `crates/i2pr-client/src/streaming/` (the corrected wire format and
  receiver ack views per Java `MessageInputStream.updateAcks`)
- `crates/i2pr-client/src/session.rs` (Plan 126 ECIES-X25519-AEAD-Ratchet)
- `crates/i2pr-client/src/routing.rs` (Plan 122/124/127 outbound/inbound)
- `crates/i2pr-daemon/src/netdb_seam.rs` (the bounded NetDB seam)
- `specs/protocols/` for SAM dossier
- `specs/references/ecies-destination-ratchet.md` and
  `specs/references/streaming-packet-wire.md` for protocol provenance
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
  default remains `experimental` and `advertised = false`.
- Before handoff, run the focused local seam in addition to the
  pre-handoff sequence in `AGENTS.md`.

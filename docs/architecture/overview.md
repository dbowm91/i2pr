# `i2pr` Architecture Overview

A bird's-eye view of the `i2pr` workspace. This document describes the
discrete modules, what each one owns, and how they fit together. Each
section links to a dedicated deep-dive document under `docs/architecture/`.

> Status: experimental. Not production-ready. Not for anonymity or
> security-sensitive workloads. See `README.md` and `GUARDRAILS.md`.

## Conceptual model

`i2pr` is an experimental Rust implementation of an I2P router organized
as a **modular monolith**. Every subsystem lives in its own crate with a
strictly enforced dependency graph. The codebase is the artifact of a
sequence of plans under `plans/`; each milestone closure document captures
the decisions and evidence behind the current shape.

Four conceptual planes run through every crate:

| Plane | Responsibility | Status |
| --- | --- | --- |
| Data | Protocol representations, authenticated links, messages, network tunnel traffic | Bounded: common-structure codecs, initial I2NP models, NTCP2 handshake + data-phase frames, and runtime-neutral transport contracts. No public-network behavior. |
| Control | Configuration, lifecycle, health, cancellation, supervision, resource budgets | Runtime-neutral core contracts + bounded `i2pr-runtime` supervisor + NTCP2 runtime service. Daemon composition and live execution are not yet wired in. |
| Client | Destinations, LeaseSets, streaming, SAM, I2CP adapters | Plan 120 lands the first local-destination runtime (`i2pr-client`): identity, dedicated tunnel pools, signed Standard LeaseSet2, lifecycle, bounded local payloads, registry. Plan 121 adds the ECIES-X25519-AEAD-Ratchet destination session layer (`EciesSessionManager`, bounded structural Garlic payload block codec). Remote destination routing and streaming are Plan 122/123 scope. |
| Service | HTTP, SOCKS5, IRC, generic TCP, local service tunnels | Not implemented. |

Network tunnels (router-to-router) and application service tunnels
(local app to destination) are deliberately kept apart. Service tunnels
must not import transport internals or peer-profile storage.

## Plan 042 runtime-owned NTCP2 composition

The runtime boundary is now concrete for the bounded NTCP2 wire path. The
runtime-neutral state machines in `i2pr-transport-ntcp2` emit complete,
bounded actions; `i2pr-runtime` fulfills them with exact socket I/O,
cancellation-aware deadlines, clock and replay services, and authenticated
data-frame owners. `AuthenticatedLink` supervises one reader and one writer,
and each queued frame or received frame carries its own accounting lease.

The non-production `i2pr-interop` binary is a disposable composition root. It
validates the strict synthetic scenario, owns temporary identity/static-key and
RouterInfo preparation, runs either the listener or dial path, and performs a
DeliveryStatus smoke exchange before bounded cleanup. It does not enable the
daemon, publish capabilities, or create interoperability evidence. Reference
profiles remain a separate Ubuntu namespace gate and require sanitized
mixed-router observations.

## Plan 038 harness boundary

The Ubuntu reference-router harness is an opt-in test boundary, not another
runtime plane and not a production daemon path. It supports Ubuntu amd64 for
the initial closure and separates network-enabled preparation from
network-isolated execution. Preparation verifies the host, installs only
declared tools, fetches the pinned Java I2P/i2pd revisions, and hashes cached
artifacts. Execution creates disposable per-scenario state and two Linux
namespaces joined only by a veth pair; it rejects default routes, DNS, and
public egress before starting a router. The normal daemon remains disabled.

The corrective apparatus contract is documented in
[`interop-apparatus.md`](interop-apparatus.md): canonical full source pins,
strict cache metadata, short topology tokens, exact nftables policies, and
evidence finalization outside the secret-bearing run root.

The harness uses three evidence classes: environment smoke covers reference
startup and cleanup; Plan 041's reference-crosscheck profile runs the two
directional Java I2P/i2pd control scenarios in a dedicated reference-pair
topology and requires dual authenticated observations; i2pr mixed-router evidence requires bounded authenticated
runs between i2pr and each reference in both directions. Only the last class
can contribute to a protocol support claim, and only after sanitation leaves
typed outcomes, bounded metadata, and artifact/configuration hashes. Raw
addresses, identities, RouterInfo, I2NP, keys, transcripts, logs, and arbitrary
remote error text are not retained.

## Plan 048/049/050 Multipass evidence environment

Plan 048 is an orchestration boundary around the Plan 046 rootless topology,
not a production crate or a support claim. Plan 049 makes its lifecycle
ownership explicit. Plan 050 minimizes the cloud-init unit and adds a
sanitized failure taxonomy, a `--guest-probe-only` flow, and a
selective-purge remediation that requires a verified ownership contract.
The current host remains the AppArmor-restricted negative
baseline. A disposable Multipass Ubuntu 24.04 amd64 guest provides the
`host.apparmor-restrict-off` recovery category with guest-only user-namespace
policy, fixed resources, immutable source/cache transfer, and ordinary-user
execution as `i2ptest`.

The reviewed environment contract has a stable environment ID, separate from
the safe run ID and the concrete instance name/generation. Host lifecycle
state is reserved atomically before launch and protected by a per-run/
per-instance lock. Names are collision-resistant by default; the legacy
`i2pr-interop-rootless` name is not authoritative. Ownership requires a
cryptographically linked host/guest contract, not a name match. Adoption,
resume, recreation, destruction, and inspection are explicit operations; no
normal path silently mutates an existing or deleted resource, and global
`multipass purge` is forbidden.

Preparation may use the network; guest nftables egress denial and both early
and final guest rootless probes are mandatory before the four directional Plan
045 scenarios. The host baseline probe is recorded separately and does not
substitute for the guest gate. The canonical reference cache remains
`target/interop/cache`. Only validated sanitized evidence crosses back to the
host, and destroying an owned guest does not remove the host evidence directory.
Environment and directional records carry run/generation and contract digests;
pre-router blockers remain environment outcomes, not protocol evidence. See
[ADR 0018](../adr/0018-multipass-rootless-interop-environment.md) and
[`interop-apparatus.md`](interop-apparatus.md).

## Crate graph

The dependency direction is enforced by `scripts/check-dependency-direction.sh`
and the [`docs/architecture/dependency-graph.md`](dependency-graph.md)
detail document.

```text
i2pr-proto  <- i2pr-crypto <- i2pr-storage
     ^             ^              ^
     |             |              |
i2pr-core <- i2pr-transport <- i2pr-runtime <- i2pr-daemon (composition root)
     ^             ^              ^                    |
     |             |              |                    v
     +-------------+   i2pr-transport-ntcp2    i2pr-netdb-persist
                          ^                     (cache + reseed)
                          |                          |
                i2pr-proto + i2pr-crypto     i2pr-netdb (SU3/reseed)
                                                  ^
                                                  |
                                            i2pr-tunnel
                                       (Milestone 5 substrate)

i2pr-testkit (test-only; may depend on transport crates;
              no production crate may depend on it)
```

Reading from the arrows: lower crates stay pure, runtime-neutral, and
narrow. Higher crates widen scope and (in the case of `i2pr-runtime`)
take exclusive ownership of Tokio and sockets. Plan 120 adds
`i2pr-client` alongside the runtime, with its own short edges into
`i2pr-core`, `i2pr-crypto`, `i2pr-netdb`, `i2pr-proto`, and
`i2pr-tunnel`; it never composes back into `i2pr-tunnel` or
`i2pr-netdb` and never imports `i2pr-daemon`.

## Crate index

Each row links to a deep-dive document covering the crate's purpose,
module layout, public surface, key contracts, errors, dependencies,
tests, and any distinctive design choices.

| Crate | Role | One-liner | Deep dive |
| --- | --- | --- | --- |
| `i2pr-proto` | Foundation | Bounded wire codecs for I2P common structures and I2NP messages, including the Plan 119 Standard LeaseSet2 carrier (`Lease2`, `LeaseSet2Header`, `LeaseSet2EncryptionKey`, `LeaseSet2`, signature domain `0x03 || signed_bytes`) and the typed `DatabaseStoreData::LeaseSet2` body. No runtime, no I/O. | [i2pr-proto.md](i2pr-proto.md) |
| `i2pr-crypto` | Identity crypto | Protocol-specific wrappers around Ed25519, X25519, SHA-256. Secret material is zeroized. | [i2pr-crypto.md](i2pr-crypto.md) |
| `i2pr-storage` | Persistence | Versioned, atomic, permission-hardened storage for router identity and NTCP2 static key. | [i2pr-storage.md](i2pr-storage.md) |
| `i2pr-core` | Service contracts | Runtime-neutral lifecycle, health, cancellation, and resource budgets. Zero dependencies. | [i2pr-core.md](i2pr-core.md) |
| `i2pr-netdb` | Local NetDB | Runtime-neutral RouterInfo validation, bounded in-memory NetDB store, SU3/reseed verification, peer-selection primitives, transport-neutral lookup/publication state machines, and local signed RouterInfo construction. Plan 103/104/105/119. | [i2pr-netdb.md](i2pr-netdb.md) |
| `i2pr-netdb-persist` | Cache composition | Composition owner for Plan 104 persistent RouterInfo cache and SU3 reseed ingestion. Bridges `i2pr-storage` (raw bytes) and `i2pr-netdb` (validation). | — |
| `i2pr-tunnel` | Milestone 5 substrate | Runtime-neutral tunnel identity, exploratory pool, build-record layout surface, build-cryptography seam, ECIES-X25519 short tunnel-build construction primitive (Plan 111 final local short-build conformance + Plan 112 outbound pre-delivery closure + Plan 113 inbound reference reconciliation + Plan 114 terminal routing and tunnel-chain correction + Plan 115 canonical production I2NP bridge with no-double-prefix STBM record count byte invariant + Plan 116 local tunnel data plane + Plan 117 outbound/inbound exploratory NetDB composition), runtime-neutral build state machine, success-only registrar, deterministic responder peer simulator, and reply-path provider. Plans 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117. | [i2pr-tunnel.md](i2pr-tunnel.md) |
| `i2pr-transport` | Transport contracts | Runtime-neutral link/delivery contracts. No Tokio, no I/O, no async. | [i2pr-transport.md](i2pr-transport.md) |
| `i2pr-transport-ntcp2` | NTCP2 protocol | Runtime-neutral Noise handshake, AEAD frames, data-phase blocks. | [i2pr-transport-ntcp2.md](i2pr-transport-ntcp2.md) |
| `i2pr-runtime` | Runtime owner | The only production owner of Tokio tasks, sockets, timers, channels, wakeable cancellation. | [i2pr-runtime.md](i2pr-runtime.md) |
| `i2pr-daemon` | Composition root | CLI + config + identity lifecycle + Plan 106 NetDB/bootstrap pipeline + Plan 117 outbound `OutboundGatewayRole` exploratory `DatabaseLookup`/`DatabaseStore` composition and inbound `LocalInboundEndpointRole` `TunnelData` dispatch through `crates/i2pr-daemon/src/{outbound_lookup,inbound_dispatch}.rs`. Live daemon runs through the supervisor with no I2P transport. | [i2pr-daemon.md](i2pr-daemon.md) |
| `i2pr-client` | Destination runtime | Plan 120: local destination identity, destination-specific tunnel pools that consume real one-shot `EstablishedMaterial`, local Standard LeaseSet2 construction and signing with self-validation through `i2pr-netdb`, LeaseSet2 lifecycle with bounded rotation/withdrawal, bounded local payload contracts, and a router-local destination registry. Plan 121: ECIES-X25519-AEAD-Ratchet destination session layer with bounded outbound/inbound session counts, bounded structural Garlic payload block codec integration, and the typed NS / NSR / Existing Session trajectory producers. No NTCP2 / SSU2 / public-network transport. Remote destination routing and streaming remain Plan 122/123 scope. | [i2pr-client.md](i2pr-client.md) |
| `i2pr-testkit` | Test simulation | Deterministic clocks, virtual links, scripted faults. Test-only; never a production dep. | [i2pr-testkit.md](i2pr-testkit.md) |
| `scripts/` + `tests/` + `fuzz/` | Tooling | Guardrails, fixtures, integration lanes, opt-in fuzzing. | [tooling.md](tooling.md) |

## How data flows at runtime

A live `i2pr run` today (Plan 106) follows this sequence:

1. **`i2pr-daemon`** parses CLI flags, loads and validates the TOML
   config under `deny_unknown_fields` (including the new `[netdb]`
   and `[reseed]` sections), maps errors to stable exit codes, then
   runs `bootstrap_daemon` before starting the supervisor.
2. **`i2pr-storage`** loads the router identity from
   `<data_dir>/router.identity` and (separately) the NTCP2 static key
   from `<data_dir>/ntcp2.static.key`. Either file can be generated,
   but never silently replaced (atomic `hard_link` + `AlreadyExists`).
3. **`i2pr-netdb`** validates and self-validates the local
   `RouterInfo` through `LocalRouterInfoBuilder`; refuses any
   transport address or forbidden capability letter under the
   Plan 101 activation guard.
4. **`i2pr-netdb-persist`** loads and revalidates the persistent
   RouterInfo cache through the Plan 104 `CacheLoader`, then runs
   the optional bounded offline SU3 reseed through `ReseedIngestor`.
 5. **`i2pr-netdb`** keeps the populated `RouterInfoStore`,
     `CoalescedRouterInfoLookup`, `PublicationCoordinator`, and the
     transport-neutral state machines ready for the Milestone 5
     runtime adapter. `i2pr-daemon`'s `NetDbSeam` exposes them
      through a stable surface; under Plan 107 the seam consults an
      injected [``i2pr_netdb::ReplyPathProvider`] implementation
      backed by `i2pr-tunnel`'s exploratory pool, so a registered
      inbound tunnel flips the seam's status to `Available` and a
      `NeedExploratoryReplyPath` lookup action is converted into a
       real path on the state machine. Under Plan 109 the same crate
       holds the locally-conformant ECIES-X25519 Noise-N
       short-build cryptography primitive, the
       `ReferenceFixture` independent conformance fixture, and the
       runtime-neutral `ShortBuildStateMachine` that drives one
       short tunnel-build attempt through the canonical state
       machine; admission into the exploratory pool flows only through
       the success-only `ShortBuildRegistrar`. **The single-record
       request/reply wire format and Noise-N transcript are locally
       conformant against the current official I2P Tunnel Creation
       Specification; see
       [`plans/109-short-build-record-and-noise-conformance-correction.md`](../../plans/109-short-build-record-and-noise-conformance-correction.md)
and the closure record
        [`plans/110-status.md`](../../plans/110-status.md). Plan 110
        closed the multi-record slot allocation, fake records, raw
        ChaCha20 preprocessing, and one-byte-count STBM/OTBRM payload
        framing as `passed-multirecord-local-conformance`. Plan 113
        enables the inbound path under the explicitly named
        `reference-compatible-spec-text-discrepancy` policy: the real
        request keeps its fixed fields + Mapping/padding and exactly
        one originator fake carries the creator key. This does not
        claim strict final-spec text conformance for that semantic;
        see `specs/references/short-build-inbound-creator-key.md`.
        Plan 114 closes the post-Plan-113 high-level routing/composition
        defects: explicit outbound `outbound_reply_router` and inbound
        `originator_hash` terminal-routing fields, intermediate
        `hops[i].next_tunnel == hops[i+1].receive_tunnel` chain
        continuity enforced at both the high-level
        `ShortBuildPath::validate()` boundary and the public lower-level
        `prepare_short_build_message()` entry point, and strict
        outbound/inbound E2E trajectories that deterministically reach
        `Established`. Plan 115 adds the canonical production I2NP
bridge (`ShortBuildI2npBridge` in
         `crates/i2pr-tunnel/src/bridge.rs`) so a
         `ShortBuildAction::Deliver` payload is wrapped in a single
         complete I2NP type-25 message without double-prefixing the STBM
         record count byte and with a round-trip body equality assertion.
         Plan 115 Q0 construction + native OBEP reply has passed
         locally against pinned Emissary — see
         [`plans/115-status.md`](../../plans/115-status.md). Q1/Q2
         and qualified external delivery remain pending.**
         Plan 116 closed the local tunnel data plane
         (`passed-final-local-closure` via the
         completion/correction/terminal-cleanup sequence). Plan 117
         closed as `closed-for-progression-with-evidence-gap` per
         Plan 118: the local production exploratory NetDB composition
         (Phase G) is passed, but the corrected in-tree native Emissary
         reference test rejects the pinned reference's request-prefixed
         reply plaintext during strict i2pr Mapping decoding. Plan 117
         adds the typed `DatabaseLookupMessage`/`DatabaseStoreMessage`
         carriers on `LookupAction`/`PublicationAttemptRecord`, the
         metadata-retaining one-shot `ExploratoryPool::activate`, the
         bounded `DataPlaneRegistry` for activated local roles
         (`crates/i2pr-tunnel/src/data_plane_registry.rs`), the daemon
         `NetDbSeam` composition state machine
         (`crates/i2pr-daemon/src/netdb_seam.rs`), the outbound
         `OutboundGatewayRole::forward_cells` exploratory
         `DatabaseLookup`/`DatabaseStore` composition
         (`crates/i2pr-daemon/src/outbound_lookup.rs`), and the inbound
         `LocalInboundEndpointRole` `TunnelData` dispatch through
         `dispatch_inbound_tunnel_data`/`route_databasestore`/
         `route_database_search_reply`
         (`crates/i2pr-daemon/src/inbound_dispatch.rs`). The Plan 117
         composition is local-only; the network transport adapter
         still owns the NTCP2/SSU2 handshake surface, and authenticated
         external transport remains `deferred-host-lane-unavailable`.
          Plan 119 closed as `passed-leaseset2-protocol-foundation` per
          [`plans/119-status.md`](../../plans/119-status.md); the ordinary
          online-signed published Standard LeaseSet2 carrier is wired
          into `i2pr-proto` and `i2pr-netdb` (LS2 validation, bounded
          store, `LookupKind::LeaseSet2`, typed `DatabaseStoreData::LeaseSet2`).
          Plan 120 closed as `passed-destination-lifecycle-and-pools` and
          lands the first `i2pr-client` destination runtime: local
          destination identity (independent Ed25519 signing + X25519
          static keys, non-`Clone`, non-`Debug` secrets), destination-
          specific tunnel pools that consume real one-shot
          `EstablishedMaterial` through a thin `BoundedTunnelPool`
          alias in `i2pr-tunnel`, local Standard LeaseSet2
          construction and signing with self-validation through
          `i2pr-netdb`, LeaseSet2 lifecycle with bounded
          rotation/withdrawal, bounded local payload contracts that
          never inject plaintext into tunnel delivery, and a
          router-local destination registry with explicit capacity
          and duplicate-rejection guards. The
          `plan_120_deterministic_local_trajectory` integration test
          drives the real short-build state machine to `Established`
          through the production seams and walks one full
          start → tunnel → LeaseSet2 → rotation → shutdown trajectory.
          Plan 121 closed as `passed-ecies-destination-session-layer`
          and lands the first real ECIES-X25519-AEAD-Ratchet
          destination Garlic/session layer: the audited
          `curve25519-elligator2 = 0.1.0-alpha.2` primitive (Plan 121
          §2 / §12), the wrapped ECIES primitives in `i2pr-crypto`,
          the bounded structural Garlic payload block codec in
          `i2pr-proto`, and the bounded destination-context
          `EciesSessionManager` in `i2pr-client`. The
          `plan_121_deterministic_local_trajectory` integration test
          drives the two-destination NS → NSR → Existing Session
          trajectory with exact-once payload delivery, tag ratchet
          advancement, and replay rejection. Remote destination
          routing and the streaming protocol remain Plan 122/123
          scope. The next executable plan is **Plan 122**
          (destination routing and LeaseSet2 NetDB composition) under
          [`plans/118-123-milestone6-router-construction-roadmap.md`](../../plans/118-123-milestone6-router-construction-roadmap.md).
6. **`i2pr-runtime`** builds a `ServiceGraph`, topologically validates it
   before startup, then spawns one supervisor manager per service via a
   `JoinSet`. Each service receives a narrowed `ServiceContext` (name,
   cancellation, readiness, health, child scope) — never a direct handle
   to the supervisor. The graph contains only `lifecycle` and
   `netdb-bootstrap` services under Plan 101/106 authority.
7. **`i2pr-transport-ntcp2`** is declared but **not yet used** in the
   production daemon. It implements the protocol: Noise XK handshake,
   AES-CBC ephemeral obfuscation, ChaCha20-Poly1305 data phase,
   directional SipHash frame-length masking, deterministic handshake
   state machines. It returns `HandshakeAction` / `FrameAction`
   requests; `i2pr-runtime` would fulfill them with real sockets and
   cancellation. The Plan 101 NTCP2 activation guard keeps the daemon
   from registering `ntcp2-transport`.
8. **`i2pr-transport`** sits underneath as the runtime-neutral link
   manager: `LinkState` FSM, `TransportManager` admission with RAII
   leases, duplicate-resolution policy, privacy-safe `TransportSnapshot`.
9. **`i2pr-core`** provides lifecycle, health snapshots, cancellation
   tokens, and the shared `ResourceBudget` governor that all subsystems
   draw from via typed lease owners.
10. **`i2pr-proto`** and **`i2pr-crypto`** stay at the bottom — no one
    depends on anything above them except the test and integration
    layers.
11. **`i2pr-testkit`** is used only by tests. It exercises the same
    crates through a `NetworkScheduler`, `ManualClock`,
    `Ntcp2DataPhaseDriver`, and a 128-bit `ReproducibilitySeed`. Tests
    use `#[tokio::test(start_paused = true)]`; no wall-clock sleeps, no
    real sockets, no DNS, no public-network traffic.

The boundary contract is enforced by scripts under `scripts/`:

| Script | Catches |
| --- | --- |
| `check-dependency-direction.sh` | Crate-layer DAG violations (e.g. `i2pr-proto` depending on `i2pr-runtime`). |
| `check-runtime-boundaries.sh` | Unbounded channels, wall-clock sleeps, raw `JoinHandle`s, `tokio::spawn` without an owner, `async fn` in transport contracts, Tokio deps in wrong crates, `std::net`/`std::fs` in transport, `i2pr-testkit` referenced by a production crate. |
| `check-fixture-manifest.sh` | Drift in the I2NP fixture corpus under `tests/fixtures/i2np/`. |
| `check-ntcp2-vectors.sh` | Drift in the NTCP2 crypto vector corpus under `tests/fixtures/ntcp2/crypto/`. |
| `check-ntcp2-interoperability.sh` | Forbidden artifacts in the synthetic private NTCP2 interoperability lane; manifest pinned to exactly eight scenarios with required disclaimer lines. |
| `fuzz-smoke.sh` | Opt-in smoke run of all 22 fuzz targets (requires nightly + `cargo-fuzz`). |

## Conventions

These apply across every crate and are enforced by workspace lints,
script gates, and review.

- `#![forbid(unsafe_code)]` on every crate (workspace lint `unsafe_code = "deny"`).
- `unexpected_cfgs = "deny"`, `unused_must_use = "warn"`.
- Clippy denies `dbg_macro`, `todo`, `unimplemented`.
- `crate/secret` owners are non-cloneable, non-`Debug`, and
  `zeroize::Zeroize` on drop; the NTCP2 forbidden nonce `2^64 - 1`
  is never emitted.
- Codec errors are typed; decode/encode results are never swallowed.
- NTCP2 static-key/IV material lives in the separate versioned
  `i2pr-storage` record — never derived from or overwrite the router
  identity record.
- Configuration, protocol, and persisted data are treated as hostile:
  explicit bounds, rejection of unknown or trailing bytes, no
  validation side effects, and always a tested negative path.
- All architecture/security decisions live under `docs/adr/`; the
  plan-of-record is the active `plans/NNN-*.md` plus its closure
  document. When closing a milestone, attach a closure record with
  commands, results, and evidence.

## Cross-references

- Top-level architecture narrative: [`docs/architecture.md`](../architecture.md)
- Security model: [`docs/security-model.md`](../security-model.md)
- Protocol support matrix: [`docs/protocol-support.md`](../protocol-support.md)
- Conformance: [`specs/CONFORMANCE.md`](../../specs/CONFORMANCE.md)
- Plan-of-record: latest active `plans/NNN-*.md`
- Workspace guidelines: [`AGENTS.md`](../../AGENTS.md)

## Plan 077 constrained-host execution lane

The constrained-host lane is a separate, inspection-first boundary. Its
selection order is accessible rootful Docker with `--network none`, QEMU TCG
with `-nic none`, reduced inherited descriptors plus seccomp, manual remote
Linux, then a typed no-full-runtime-lane result. The probe and strict
qualification contracts live in `execution_lane.py`; a capability or tool
definition is not interoperability evidence. Plan 080 later qualified an
owned full-runtime guest for the single Plan 078 attempt.

Plan 078 records a pre-protocol i2pr RouterInfo stop in
[`plans/078-status.md`](../../plans/078-status.md), not a protocol pass or
failure. Plans 082-084 are now the active minimal-probe sequence. Plan 072
remains inactive until Plan 084 records
`decision = ambiguous-reference-divergence` for a precise unresolved
wire-stage question.

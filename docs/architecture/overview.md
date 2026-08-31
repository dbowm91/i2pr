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
| Client | Destinations, LeaseSets, streaming, SAM, I2CP adapters | Plan 120 lands the first local-destination runtime (`i2pr-client`): identity, dedicated tunnel pools, signed Standard LeaseSet2, lifecycle, bounded local payloads, registry. Plan 121 added the first ECIES destination session layer; Plan 126 rewrites it to the normative I2P ECIES-X25519-AEAD-Ratchet contract (`crates/i2pr-crypto/src/ecies.rs`, corrected `EciesSessionManager`); Plan 127 binds accepted sessions to resolved Destinations through bundled-LS2 validation under the sender's own Destination hash with a planned outbound form state machine and production reverse routing. Plan 122 composes the Plan 119 LeaseSet2 lookup surface, the Plan 120 destination runtime, the Plan 121 ECIES session layer, and the Plan 116 tunnel data plane into the first complete local destination routing pipeline: `LeaseSelector`, `OutboundRequest`, `compose_outbound_delivery`, `DestinationRouting`, and `DestinationDispatcher`. Plan 125 lands the minimal Streaming core (`i2pr-client::streaming`) and Plan 128 corrects its packet wire format to the current I2P Streaming specification (normative flag map, no option TLVs, payload-only MAX_PACKET_SIZE, raw final signatures, Proposal 164 replay NACKs on the initial SYN only). Plan 129 closes the Milestone 6 integrated gate: one combined runtime-neutral outbound/inbound `StreamingDestinationAdapter` (client-payload-bound outbound sizing with a single canonical Data-envelope owner; inbound I2NP Data -> gzip -> protocol-6 -> ports -> Streaming dispatch), integrated retransmission/ACK/reorder over the destination stack, and CLOSE/RESET completion policy. Plan 130 closes Milestone 6 with the final wire/runtime corrective closure: production Elligator2 randomized representatives with independent frozen fixtures, application sequences starting at 1 post-SYN, flag-driven ACK presence where `ackThrough == 0` is valid, NACK-aware cumulative ACKs, receiver ack views per Java `MessageInputStream.updateAcks`, coalescing delayed standalone ACKs via `poll_acks(now_ms)` on the 750 ms reference default, authoritative wire destination-port listener dispatch with enforced established tuples, and persistent tunnel duplicate windows with typed replay rejection. SAM/I2CP adapters remain Milestone 7 scope. Plan 136 adds the SAM 3.1 application-protocol foundation in `i2pr-api`: bounded line parser, typed command/reply model, version negotiation, SAM Base64, SamPrivateDestination codec, and the runtime-neutral DEST GENERATE / SESSION CREATE private-destination import/export surface. Plan 137 will wire it into the supervised loopback listener through `i2pr-daemon`. |
| Service | HTTP, SOCKS5, IRC, generic TCP, local service tunnels | Not implemented. |

Plan 134 is the current local Milestone 6 closure authority. It corrects the receive-window ACK ceiling so a rejected `TooFarAhead` packet is state-inert and cannot advance `highest_received`, create NACK holes, schedule an ACK, or appear in a later piggyback ACK. Independent-router interoperability remains external acceptance debt and is not claimed. **Plan 142 is the last passed local Milestone 7 SAM 3.1 corrective closure: `i2pr-api/src/sam/base64.rs` now uses the I2P Base64 alphabet (`A-Z a-z 0-9 - ~`, `=` padding) — the spelling every Java I2P / i2pd / independent Python SAM client reference implementation emits — and the prior circular private-destination evidence is replaced with three independent reference vectors (i2pd `libi2pd/Base.{h,cpp}`, Java I2P `PrivateKeyFile.java`, i2plib `I2P_B64_CHARS = "-~"`). See [`plans/142-status.md`](../plans/142-status.md) for the closure record.** Plan 139 remains the last passed STREAM FORWARD / NAMING LOOKUP closure. Plan 140 is the current blocked closure attempt for the same-socket live STREAM product path; its evidence and handoff are in [`plans/140-status.md`](../plans/140-status.md). Plan 143 (live STREAM CONNECT/ACCEPT) and Plan 144 (two-independent-client final closure) remain open.

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
     ^             ^              ^                ^
     |             |              |                |
     +-------------+   i2pr-transport-ntcp2    i2pr-api
                          ^                     (SAM 3.1)
                          |                          ^
                 i2pr-proto + i2pr-crypto     i2pr-netdb-persist
                                                    ^
                                                    |
                                              i2pr-netdb (SU3/reseed)
                                                    ^
                                                    |
                                              i2pr-tunnel
                                         (Milestone 5 substrate)
                                     ^
                                     |
                               i2pr-client
                               (Plan 120+)

i2pr-testkit (test-only; may depend on transport crates;
              no production crate may depend on it)

tools/i2pr-interop (non-production launcher; depends on transport +
                    runtime + storage; never activates i2pr-daemon)
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
| `i2pr-netdb-persist` | Cache composition | Composition owner for Plan 104 persistent RouterInfo cache and SU3 reseed ingestion. Bridges `i2pr-storage` (raw bytes) and `i2pr-netdb` (validation). | [i2pr-netdb-persist.md](i2pr-netdb-persist.md) |
| `i2pr-tunnel` | Milestone 5 substrate | Runtime-neutral tunnel identity, exploratory pool, build-record layout surface, build-cryptography seam, ECIES-X25519 short tunnel-build construction primitive (Plan 111 final local short-build conformance + Plan 112 outbound pre-delivery closure + Plan 113 inbound reference reconciliation + Plan 114 terminal routing and tunnel-chain correction + Plan 115 canonical production I2NP bridge with no-double-prefix STBM record count byte invariant + Plan 116 local tunnel data plane + Plan 117 outbound/inbound exploratory NetDB composition), runtime-neutral build state machine, success-only registrar, deterministic responder peer simulator, and reply-path provider. Plans 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117. | [i2pr-tunnel.md](i2pr-tunnel.md) |
| `i2pr-transport` | Transport contracts | Runtime-neutral link/delivery contracts. No Tokio, no I/O, no async. | [i2pr-transport.md](i2pr-transport.md) |
| `i2pr-transport-ntcp2` | NTCP2 protocol | Runtime-neutral Noise handshake, AEAD frames, data-phase blocks. | [i2pr-transport-ntcp2.md](i2pr-transport-ntcp2.md) |
| `i2pr-runtime` | Runtime owner | The only production owner of Tokio tasks, sockets, timers, channels, wakeable cancellation. | [i2pr-runtime.md](i2pr-runtime.md) |
| `i2pr-daemon` | Composition root | CLI + config + identity lifecycle + Plan 106 NetDB/bootstrap pipeline + Plan 117 outbound `OutboundGatewayRole` exploratory `DatabaseLookup`/`DatabaseStore` composition and inbound `LocalInboundEndpointRole` `TunnelData` dispatch through `crates/i2pr-daemon/src/{outbound_lookup,inbound_dispatch}.rs`. Live daemon runs through the supervisor with no I2P transport. | [i2pr-daemon.md](i2pr-daemon.md) |
| `i2pr-api` | Application protocols | SAM 3.1 protocol foundation + loopback-server surface + STREAM CONNECT / ACCEPT bridge: bounded line/command/reply parser, typed commands, version negotiation, **I2P Base64 codec** (`-`/`~`, `=` padding — Plan 142 corrective from the prior RFC 4648 alphabet), SamPrivateDestination codec for `DEST GENERATE` / `SESSION CREATE`, bounded `SamSessionRegistry` + `LineReader` + `ServerConnectionState`, and the bounded per-session `SamStreamRegistry`. Plans 136, 137, 138, 142; no sockets, no Tokio, no I/O. | [i2pr-api.md](i2pr-api.md) |
| `i2pr-client` | Destination runtime | Plan 120: local destination identity, destination-specific tunnel pools that consume real one-shot `EstablishedMaterial`, local Standard LeaseSet2 construction and signing with self-validation through `i2pr-netdb`, LeaseSet2 lifecycle with bounded rotation/withdrawal, bounded local payload contracts, and a router-local destination registry. Plan 126: normative ECIES-X25519-AEAD-Ratchet destination session layer — paired sessions keyed by remote static key, bounded remove-on-hit tag windows, pre-derived pending reply windows, provisional responder state, classify-driven dispatch. Plan 127: destination-session routing final closure — bundled-LS2 sender binding under the sender's own Destination hash, `PlannedOutboundForm` outbound form state machine with retained NSR context, production reverse routing through `install_remote_lease_set2`, active-remote ceiling, master NS → NSR → ES ×4 trajectory through real tunnel roles. Plan 122: destination routing and NetDB composition — `LeaseSelector` / `LeaseSelectionPolicy`, typed `OutboundRequest` builder, `compose_outbound_delivery` planner, `DestinationRouting` cache, and `DestinationDispatcher` inbound surface that classifies envelopes through `EciesSessionManager::classify`. Plan 124: destination-routing corrective closure — `compose_outbound_delivery` wraps the encrypted envelope in an `I2npBody::Garlic` carrier and feeds the standard-encoded I2NP Garlic message bytes into the outbound tunnel data plane; `OutboundDeliveryPlan::garlic_i2np_bytes` is the canonical carrier the tunnel observes; `DestinationDispatcher::bind_destination_hash` enforces the `DestinationId` → `DestinationHash` binding so the dispatcher fails closed on `UnknownDestination` without trial-decryption. Plan 125: minimal Streaming core with the runtime-neutral `StreamingDestinationAdapter` (now `superseded-by-final-corrective-closure`). Plan 128: Streaming packet wire corrective closure - normative flag map, flag-driven option codec, raw final signatures from signing-key context, payload-only MAX_PACKET_SIZE (default 1730), Proposal 164 replay NACKs on the initial SYN only, retained peer signing key for CLOSE/RESET verification without FROM, min-of-advertisements negotiation. Plan 129: integrated destination+Streaming gate (`superseded-by-plan130-final-gate`). Plan 130: Milestone 6 final wire/runtime corrective closure (`passed-milestone6-final-wire-runtime-corrective-closure`) - production Elligator2 randomized representatives with deterministic-vector separation, corrected post-SYN sequence space (first application packet seq 1), semantic ACK presence with NACK-aware cumulative acknowledgement, bounded receiver ack views, coalescing delayed standalone ACKs via `poll_acks`, wire destination-port listener authority with typed rejections, and persistent tunnel duplicate windows with typed `DuplicateCell`. SAM/I2CP adapters remain Milestone 7 scope. | [i2pr-client.md](i2pr-client.md) |
| `i2pr-testkit` | Test simulation | Deterministic clocks, virtual links, scripted faults. Test-only; never a production dep. | [i2pr-testkit.md](i2pr-testkit.md) |
| `scripts/` + `tests/` + `fuzz/` | Tooling | Guardrails, fixtures, integration lanes, opt-in fuzzing. | [tooling.md](tooling.md) |

## How data flows at runtime

A live `i2pr run` (Plan 106) follows this sequence:

1. **`i2pr-daemon`** parses CLI flags, loads and validates the TOML
   config under `deny_unknown_fields` (including the `[netdb]` and
   `[reseed]` sections), maps errors to stable exit codes, then
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
   `CoalescedRouterInfoLookup`, `PublicationCoordinator`, the
   transport-neutral state machines, and the Standard LeaseSet2
   store (`ValidatedLeaseSet2`, `LeaseSet2Store`,
   `LookupKind::LeaseSet2`, Plan 119) ready for the Milestone 5
   runtime adapter. `i2pr-daemon`'s `NetDbSeam` exposes them
   through a stable surface; under Plan 107 the seam consults an
   injected `i2pr_netdb::ReplyPathProvider` implementation backed
   by `i2pr-tunnel`'s exploratory pool, so a registered inbound
   tunnel flips the seam's status to `Available` and a
   `NeedExploratoryReplyPath` lookup action is converted into a real
   path on the state machine. Plan 117 added the typed
   `DatabaseLookupMessage`/`DatabaseStoreMessage` carriers, the
   bounded `DataPlaneRegistry` for activated local roles, and the
   outbound `OutboundGatewayRole` exploratory `DatabaseLookup` /
   `DatabaseStore` composition plus inbound `LocalInboundEndpointRole`
   `TunnelData` dispatch in `i2pr-daemon`. The Plan 117 composition
   is local-only; the network transport adapter still owns the
   NTCP2/SSU2 handshake surface, and authenticated external
   transport remains `deferred-host-lane-unavailable`.
6. **`i2pr-tunnel`** holds the runtime-neutral exploratory pool
   (Plans 107–117), the locally-conformant ECIES-X25519 Noise-N
   short-build cryptography (Plans 108–114), the canonical
   production I2NP bridge (`ShortBuildI2npBridge` in
   `crates/i2pr-tunnel/src/bridge.rs`, Plan 115), the local tunnel
   data plane (Plan 116), and the outbound/inbound exploratory
   NetDB composition (Plan 117). The single-record request/reply
   wire format and Noise-N transcript are locally conformant against
   the current official I2P Tunnel Creation Specification;
   Plan 115 Q0 construction + native OBEP reply has passed locally
   against pinned Emissary `9b43484a21d5a1291c4881cdae62a36c527f8c0f`
   (emissary-core 0.4.0). Q1 (authenticated transport delivery) and
   Q2 (reply round-trip to `Established`) and qualified external
   delivery remain pending.
7. **`i2pr-client`** (Milestone 6 local product, Plan 120+) owns the
   local destination runtime:
   - Plan 120: local destination identity (independent Ed25519
     signing + X25519 static keys, non-`Clone`, non-`Debug`
     secrets), destination-specific tunnel pools that consume real
     one-shot `EstablishedMaterial` through a thin `BoundedTunnelPool`
     alias in `i2pr-tunnel`, local Standard LeaseSet2 construction
     and signing with self-validation through `i2pr-netdb`,
     LeaseSet2 lifecycle with bounded rotation/withdrawal, bounded
     local payload contracts, and a router-local destination
     registry with explicit capacity and duplicate-rejection
     guards.
   - Plan 122/124: destination routing and NetDB composition —
     `LeaseSelector` / `LeaseSelectionPolicy`, typed
     `OutboundRequest` builder, `compose_outbound_delivery` planner,
     `DestinationRouting` cache, `DestinationDispatcher` inbound
     surface. Plan 124 corrected the composition defect where
     `compose_outbound_delivery` retained an ECIES Garlic envelope
     but fed the plaintext inner I2NP `Data` envelope into the
     outbound tunnel role; the corrected composition wraps the
     encrypted envelope in an `I2npBody::Garlic` carrier and feeds
     the standard-encoded I2NP Garlic message bytes into the
     outbound tunnel data plane. `OutboundDeliveryPlan::
     garlic_i2np_bytes` is the canonical carrier; `inner_envelope_bytes`
     is retained for diagnostic comparison only.
   - Plan 126: normative ECIES-X25519-AEAD-Ratchet destination
     session layer — paired sessions keyed by remote X25519 static
     public key, bounded remove-on-hit tag windows, pre-derived
     pending reply windows, provisional responder state, classify-
     driven dispatch. The Plan 121 dialect was superseded.
   - Plan 127: destination-session routing final closure —
     bundled-LS2 sender binding under the sender's own Destination
     hash (`MissingBundledLeaseSet2` / `SenderKeyMismatch`),
     `PlannedOutboundForm` outbound form state machine with
     retained NSR context, production reverse routing through
     `install_remote_lease_set2`, active-remote ceiling, master
     NS → NSR → ES ×4 trajectory through real tunnel roles.
   - Plan 128: Streaming packet wire corrective closure —
     normative flag map, no option TLVs, payload-only
     `MAX_PACKET_SIZE`, raw final signatures from signing context,
     Proposal 164 replay NACKs on the initial SYN only, retained
     peer signing key for CLOSE/RESET verification, min-of-
     advertisements negotiation. Plan 123 is restored as
     `passed-corrected-streaming-wire-local`.
   - Plan 129: integrated two-direction destination+Streaming
     gate with one combined runtime-neutral outbound/inbound
     `StreamingDestinationAdapter`. Superseded by Plan 130.
   - Plans 130–133: wire/runtime corrective closures and
     transactionality. **The current Milestone 6 local closure
     authority is Plan 134** as
     `passed-milestone6-recv-window-ack-ceiling-closure`
     ([`plans/134-status.md`](../../plans/134-status.md)):
     production Elligator branch randomization via the reviewed
     `elligator2 = 0.1.0` primitive (Plan 131 retired
     `curve25519-elligator2 = 0.1.0-alpha.2`), three-layer replay
     separation, connection-owned I2P port tuple, and side-effect-
     free oversized `send_data` rollback.
     `milestone6_local_product = passed`,
     `milestone6_interoperable = not-yet-claimed`. Corrective
     Milestone 6 planning stops; the next product layer is SAM
     baseline planning (Milestone 7).
8. **`i2pr-runtime`** builds a `ServiceGraph`, topologically
   validates it before startup, then spawns one supervisor manager
   per service via a `JoinSet`. Each service receives a narrowed
   `ServiceContext` (name, cancellation, readiness, health, child
   scope) — never a direct handle to the supervisor. The graph
   contains only `lifecycle` and `netdb-bootstrap` services under
   Plan 101/106 authority.
9. **`i2pr-transport-ntcp2`** is declared but **not yet used** in
   the production daemon. It implements the protocol: Noise XK
   handshake, AES-CBC ephemeral obfuscation, ChaCha20-Poly1305
   data phase, directional SipHash frame-length masking,
   deterministic handshake state machines. It returns
   `HandshakeAction` / `FrameAction` requests; `i2pr-runtime`
   would fulfill them with real sockets and cancellation. The
   Plan 101 NTCP2 activation guard keeps the daemon from
   registering `ntcp2-transport`.
10. **`i2pr-transport`** sits underneath as the runtime-neutral
    link manager: `LinkState` FSM, `TransportManager` admission
    with RAII leases, duplicate-resolution policy, privacy-safe
    `TransportSnapshot`.
11. **`i2pr-core`** provides lifecycle, health snapshots,
    cancellation tokens, and the shared `ResourceBudget` governor.
12. **`i2pr-proto`** and **`i2pr-crypto`** stay at the bottom —
    no one depends on anything above them except the test and
    integration layers.
13. **`i2pr-testkit`** is used only by tests. It exercises the
    same crates through a `NetworkScheduler`, `ManualClock`,
    `Ntcp2DataPhaseDriver`, and a 128-bit `ReproducibilitySeed`.
    Tests use `#[tokio::test(start_paused = true)]`; no
    wall-clock sleeps, no real sockets, no DNS, no public-network
    traffic.

The boundary contract is enforced by scripts under `scripts/`:

| Script | Catches |
| --- | --- |
| `check-dependency-direction.sh` | Crate-layer DAG violations (e.g. `i2pr-proto` depending on `i2pr-runtime`). |
| `check-runtime-boundaries.sh` | Unbounded channels, wall-clock sleeps, raw `JoinHandle`s, `tokio::spawn` without an owner, `async fn` in transport contracts, Tokio deps in wrong crates, `std::net`/`std::fs` in transport, `i2pr-testkit` referenced by a production crate. |
| `check-fixture-manifest.sh` | Drift in the I2NP fixture corpus under `tests/fixtures/i2np/`. |
| `check-ntcp2-vectors.sh` | Drift in the NTCP2 crypto vector corpus under `tests/fixtures/ntcp2/crypto/`. |
| `check-ntcp2-interoperability.sh` | Forbidden artifacts in the synthetic private NTCP2 interoperability lane; manifest pinned to exactly eight scenarios with required disclaimer lines. |
| `check-rootless-interop-boundary.sh` | Forbidden artifacts in the Plan 046 rootless sealed-namespace lane (no `sudo`/`ip netns`/`nft`/`setcap`/`--privileged`/`--network host`; no silent fallback to the privileged backend). |
| `check-multipass-interop-boundary.sh` | Forbidden host-policy mutations in the Plan 048/049/050/051 Multipass recovery lane (no global `multipass purge`; no host lifecycle mutation outside an atomic reservation). |
| `check-constrained-host-lane-boundary.sh` | Bypasses of the Plan 077 constrained-host selection order (rootful Docker `--network none` → QEMU TCG `-nic none` → reduced inherited descriptors + seccomp → manual remote Linux → typed `no-full-runtime-lane` result). |
| `check-plan095-workflow.sh` | Artifact-path drift and cleanup-guard violations on the manual Plan 095 live-wire workflow. |
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

## Authoritative reference lanes

Two additional on-disk surfaces sit alongside `docs/architecture/` and
must be read before changing protocol, harness, or evidence decisions:

- **`specs/references/`** — provenance and reference documents for
  protocol decisions taken in-plan. Examples:
  `ecies-destination-ratchet.md` (Plan 126),
  `streaming-packet-wire.md` (Plan 128),
  `elligator2-production-representation.md` (Plan 131/134),
  `short-build-inbound-creator-key.md` (Plan 113). These are the
  "why" for current behavior; the deep-dives in this directory are
  the "what" and "how".
- **`.opencode/skills/`** — loadable skill bundles for OpenCode
   sessions. Five skills ship with the repo: `i2pr-local-dev`
   (local Milestone 6 product path + SAM baseline planning),
   `i2pr-architecture` (navigating `docs/architecture/`, ADRs,
   plans, and specs), `i2pr-ntcp2-interop` (Plan 038–100 harness
   surface; active lane is closed), `i2pr-rootless-sandbox` (Plan
   046 rootless sealed-namespace lane), and `i2pr-multipass-recovery`
   (Plan 048–051/053 Multipass guest lane). Load the matching skill
   before touching a lane.

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

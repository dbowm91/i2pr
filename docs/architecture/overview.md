# `i2pr` Architecture Overview

A bird's-eye view of the `i2pr` workspace: what each discrete
module owns, what the tooling owns, what capabilities exist today,
and how everything fits together at runtime.

> Status: experimental. Not production-ready. No anonymity, privacy,
> or censorship-resistance claim. NTCP2 is experimental and
> non-advertised. SAM / I2CP / service tunnels are loopback-only,
> disabled by default, and non-advertised. See `README.md`,
> `GUARDRAILS.md`, and `specs/CONFORMANCE.md`.

Authority order for any behavioral claim:

```text
plans/*-status.md > executable tests/scripts > ADRs > prose in docs/
```

`plans/README.md` plus the newest `plans/<NNN>-status.md` for the
task area win over any narrative here. Never mark a row `passed`
because prose says so; it must derive from an executed command.
`specs/support.toml` plus `specs/CONFORMANCE.md` gate every protocol
support claim.

## 1. What `i2pr` is

`i2pr` is an experimental I2P router written in Rust, organized as a
**modular monolith**: one daemon process, one crate per subsystem, a
strictly enforced dependency DAG.

Four conceptual planes cut across the crates:

| Plane | Responsibility | Representative crates |
| --- | --- | --- |
| Data | Bounded wire codecs, authenticated links, I2NP messages, tunnel traffic, garlic, streaming packets | `i2pr-proto`, `i2pr-transport-*`, `i2pr-tunnel`, `i2pr-client` |
| Control | Config, lifecycle, health, cancellation, supervision, resource budgets, persistence | `i2pr-core`, `i2pr-storage`, `i2pr-runtime`, `i2pr-daemon` |
| Network state | RouterInfo / LeaseSet2 validation, store, lookup, publication, tunnel construction | `i2pr-netdb`, `i2pr-netdb-persist`, `i2pr-tunnel` |
| Client / service | Destinations, ECIES sessions, Streaming, SAM, I2CP, HTTP/SOCKS5/IRC/generic service tunnels | `i2pr-client`, `i2pr-api`, `i2pr-service-tunnels`, `i2pr-daemon` |

Hard boundaries (CI-enforced; fix code, never weaken scripts):

- Dependency direction flows one way (see §2). No production crate
  depends on `i2pr-testkit`.
- `i2pr-runtime` is the sole production owner of Tokio, sockets,
  timers, channels, and cancellation. Transport / API / service /
  tunnel / client / netdb crates stay runtime-neutral: no
  `tokio::*`, no `std::net`, no `std::fs`, no ownerless `spawn`,
  no unbounded channels.
- Every spawned task has explicit ownership and cancellation.
  Channel/socket close is a lifecycle event, not a blind retry.
- Listeners bind loopback by default. Non-loopback needs explicit
  config plus an auth design. SAM / I2CP / service tunnels stay
  disabled by default.
- Secrets never implement `Debug` / `Display` / unrestricted
  serialization, avoid `Clone`, and zeroize where supported. Never
  log private keys, signing seeds, SSU2 static/session secrets,
  tokens, or raw payloads.
- All network / config / disk bytes are hostile and bounded:
  checked arithmetic, caller-visible alloc caps, exact-consumption
  decodes, typed errors (no `anyhow` in library crates).
- No capability, version, RouterInfo, SAM, or I2CP advertisement
  beyond the tested subset in `specs/CONFORMANCE.md`.

## 2. Workspace map

```text
crates/
  i2pr-proto/               Bounded wire codecs, typed errors, no I/O
  i2pr-crypto/              Protocol crypto wrappers (no local primitives)
  i2pr-storage/             Identity/key persistence (atomic, versioned)
  i2pr-core/                Runtime-neutral contracts/budgets/health (zero deps)
  i2pr-transport/           Runtime-neutral link/delivery contracts + selection policy
  i2pr-transport-ntcp2/     NTCP2 protocol state machines (no I/O)
  i2pr-transport-ssu2/      SSU2 v2 protocol + path/peer-test/relay machines (no I/O)
  i2pr-runtime/             Sole Tokio/socket/timer/channel owner + supervision
  i2pr-netdb/               RouterInfo + LeaseSet2 validation/store/lookup/publication
  i2pr-netdb-persist/       Persistent cache + SU3 reseed ingestion composition
  i2pr-tunnel/              Exploratory pool, short-build, data plane (runtime-neutral)
  i2pr-client/              Destination lifecycle, ECIES session/routing, Streaming
  i2pr-api/                 Runtime-neutral SAM 3.1 + I2CP wire/state (no sockets)
  i2pr-service-tunnels/     Runtime-neutral tunnel config/policy (no sockets)
  i2pr-daemon/              CLI/config/composition root; owns all listeners
  i2pr-testkit/             Deterministic fixtures only (test-only)
tools/
  i2pr-interop/             Non-production test launcher (never activates daemon)
```

Dependency direction (enforced by
`scripts/check-dependency-direction.sh`; detail in
[dependency-graph.md](dependency-graph.md)):

```text
i2pr-proto <- i2pr-crypto <- i2pr-storage
    ^              ^              ^
    |              |              |
i2pr-core <- i2pr-transport <- i2pr-runtime <- i2pr-daemon (composition root)
    ^              ^              ^                ^
    |              |              |                |
    +--------------+  i2pr-transport-ntcp2    i2pr-api (SAM 3.1 + I2CP)
                           ^                    ^
                           |                    |
                  i2pr-proto + i2pr-crypto  i2pr-netdb-persist
                           +                     ^
                           |                     |
                    i2pr-transport-ssu2      i2pr-netdb (RouterInfo/LS2)
                                                 ^
                                                 |
                                             i2pr-tunnel
                                                 ^
                                                 |
                                             i2pr-client
                                                 ^
                                                 |
                              i2pr-service-tunnels (policy only)

i2pr-testkit (test-only; production crates must not depend on it)
tools/i2pr-interop (non-production; depends on transport + runtime + storage)
```

## 3. Module index

Each row is a one-line summary. The linked document is the
review-focused deep dive for that component: purpose, module layout,
public surface, key contracts, errors, dependencies, tests, and
design choices.

| Crate / area | Role | One-liner | Deep dive |
| --- | --- | --- | --- |
| `i2pr-proto` | Foundation | Bounded I2P common-structure + I2NP codecs, LeaseSet2 carrier, typed errors. No I/O. | [i2pr-proto.md](i2pr-proto.md) |
| `i2pr-crypto` | Identity crypto | Ed25519 / X25519 / SHA-256 / HKDF / ECIES wrappers. Secrets zeroized, non-`Debug`, non-`Clone`. | [i2pr-crypto.md](i2pr-crypto.md) |
| `i2pr-storage` | Persistence | Versioned atomic `router.identity` + separate `ntcp2.static.key` stores (`0o700`/`0o600`). | [i2pr-storage.md](i2pr-storage.md) |
| `i2pr-core` | Service contracts | Lifecycle, health, wakeable cancellation, `ResourceBudget` with RAII leases. Zero deps. | [i2pr-core.md](i2pr-core.md) |
| `i2pr-transport` | Transport contracts | `LinkState` FSM, admission with RAII leases, NTCP2/SSU2 selection + reachability policy. No Tokio/async. | [i2pr-transport.md](i2pr-transport.md) |
| `i2pr-transport-ntcp2` | NTCP2 protocol | Noise XK handshake, AES-CBC obfuscation, ChaCha20-Poly1305 data phase, SipHash length masking. Emits actions; runtime fulfills them. | [i2pr-transport-ntcp2.md](i2pr-transport-ntcp2.md) |
| `i2pr-transport-ssu2` | SSU2 v2 protocol | Addresses/headers/blocks, Noise XK + header protection + one-use tokens, data-phase reliability/fragmentation, path validation, peer-test/relay. No sockets. | [i2pr-transport-ssu2.md](i2pr-transport-ssu2.md) |
| `i2pr-runtime` | Runtime owner | Only production Tokio owner: `ServiceGraph`, supervised `JoinSet`, NTCP2 link service, SSU2 UDP service, peer/relay coordinator, bounded channels/timers. | [i2pr-runtime.md](i2pr-runtime.md) |
| `i2pr-netdb` | Local NetDB | `ValidatedRouterInfo`, bounded `RouterInfoStore`, SU3/reseed verification, peer selection, lookup/publication machines, `LeaseSet2Store`, local RouterInfo builder. | [i2pr-netdb.md](i2pr-netdb.md) |
| `i2pr-netdb-persist` | Cache composition | Bridges `i2pr-storage` bytes to `i2pr-netdb` validation: `CacheLoader` + `ReseedIngestor`. | [i2pr-netdb-persist.md](i2pr-netdb-persist.md) |
| `i2pr-tunnel` | Tunnel substrate | Tunnel identity, exploratory pool, ECIES-X25519 short-build, canonical I2NP bridge, data plane, reply-path provider, NetDB-over-tunnel composition. | [i2pr-tunnel.md](i2pr-tunnel.md) |
| `i2pr-client` | Destination runtime | Destination identity/pools/registry, Standard LeaseSet2 lifecycle, ECIES-X25519-AEAD-Ratchet sessions, garlic routing/dispatch, Streaming. | [i2pr-client.md](i2pr-client.md) |
| `i2pr-api` | App protocols | SAM 3.1 parser/registry/server-state/STREAM bridge + I2CP preamble/frame/message codecs, connection/session/option machines, data plane. No sockets. | [i2pr-api.md](i2pr-api.md) |
| `i2pr-service-tunnels` | Service policy | Runtime-neutral kinds, destination refs, aliases, ceilings, HTTP/SOCKS5/IRC parser/policy surfaces. Daemon owns listeners. | [i2pr-service-tunnels.md](i2pr-service-tunnels.md) |
| `i2pr-daemon` | Composition root | CLI, TOML config, identity lifecycle, NetDB/bootstrap pipeline, SSU2 router service, SAM/I2CP listeners, service-tunnel executors, `ServiceProduct::start`. | [i2pr-daemon.md](i2pr-daemon.md) |
| `i2pr-testkit` | Test simulation | `ManualClock`, `NetworkScheduler`, virtual links, `FaultScript`, deterministic RNG. Test-only. | [i2pr-testkit.md](i2pr-testkit.md) |
| `tools/i2pr-interop` | Test launcher | Disposable NTCP2 composition root: temp identity/RouterInfo, listener-or-dial, DeliveryStatus smoke, bounded cleanup. | [tooling.md](tooling.md) |
| `scripts/` + `tests/` + `fuzz/` | Tooling | Guardrail checkers, fixture corpora, integration lanes, fuzz targets, CI gates. | [tooling.md](tooling.md) |
| Dependency graph | Boundary detail | Allowlist table + ASCII graph backing `check-dependency-direction.sh`. | [dependency-graph.md](dependency-graph.md) |
| Interop apparatus | Harness boundary | Reference-router harness, evidence classes, sanitization, Multipass/rootless lanes (historical NTCP2 surface). | [interop-apparatus.md](interop-apparatus.md) |

## 4. Discrete module overviews

Each subsection is the general overview for one reviewable unit.
Follow the deep-dive link for the full contract (module table,
public re-exports, constants, errors, tests).

### 4.1 `i2pr-proto` — bounded wire codecs

Owns every byte-level codec below the state machines: `Mapping`,
`Hash`, `Date`, keys/certs, `RouterIdentity`, `Destination`,
`RouterAddress`, `RouterInfo`, `Lease`/`Lease2`, classic `LeaseSet`
and Standard `LeaseSet2` (signature domain `0x03 || signed_bytes`),
I2NP headers (`Standard`/`ShortSsu`/`ShortTransport`), `I2npBody`
registry, `DatabaseStore`/`DatabaseLookup`/`DatabaseSearchReply`,
`TunnelData`/`TunnelGateway`, build records, garlic/ECIES payload
blocks, Streaming packet codecs, and the I2CP-style Data body used
on the inbound-delivery path. Exact-consumption decodes, typed
errors, caller-visible caps, no I/O. Detail:
[i2pr-proto.md](i2pr-proto.md).

### 4.2 `i2pr-crypto` — protocol crypto wrappers

Wraps reviewed primitives (Ed25519, X25519, SHA-256, HKDF-SHA256,
Elligator2 representative codec, ECIES-X25519 session helpers) in
protocol-typed keys with zeroize-on-drop, non-`Clone`,
non-`Debug` secrets. Used by storage (identity), tunnel short-build
(ECIES-X25519 Noise-N), destination sessions
(ECIES-X25519-AEAD-Ratchet), and both transports. Transport data-phase
ciphers (ChaCha20-Poly1305, AES-CBC, HMAC, SipHash, Noise) live in
`i2pr-transport-ntcp2`, not here. No local
primitive implementation. Detail:
[i2pr-crypto.md](i2pr-crypto.md).

### 4.3 `i2pr-storage` — atomic identity persistence

Two independent versioned records with magic + version + checksum:
`<data_dir>/router.identity` (Ed25519 + X25519 seeds and public
keys) and `<data_dir>/ntcp2.static.key` (X25519 static pair +
obfuscation IV). Atomic create via hard-link + `AlreadyExists`
(never silently replaces), `0o700` dir / `0o600` files, explicit
generate/load/rotate operations. Includes persistent service
destinations (`service_destination.rs`). Detail:
[i2pr-storage.md](i2pr-storage.md).

### 4.4 `i2pr-core` — runtime-neutral contracts

The only zero-dependency crate. Lifecycle FSM, bounded service
names, health/liveness snapshots with redaction, `Arc<AtomicBool>`
cancellation tokens, `ResourceBudget` per-class ceilings with RAII
leases / high-water marks / denial counters, and
`ServiceFailure`/`ServiceCompletion` taxonomies. Every runtime
service builds on these; nothing here knows about sockets, codecs,
or the router. Detail: [i2pr-core.md](i2pr-core.md).

### 4.5 `i2pr-transport` — link contracts and selection

Synchronous, runtime-neutral link/delivery vocabulary: `LinkState`
FSM, owned delivery requests with deadlines + cancellation,
`TransportManager` admission with double-checked locking and RAII
leases, duplicate-resolution policy, privacy-safe snapshots, and
transport-shaped resource accounting. Also owns deterministic
NTCP2/SSU2 selection and the conservative reachability policy with
typed peer-test/relay outcomes. No Tokio, no `async fn`, no I/O.
Detail: [i2pr-transport.md](i2pr-transport.md).

### 4.6 `i2pr-transport-ntcp2` — NTCP2 state machines

Runtime-neutral NTCP2: transcript composition, consuming
initiator/responder handshake machines (`SessionRequest` /
`SessionCreated` / `SessionConfirmed` + options), AES-CBC ephemeral
obfuscation, ChaCha20-Poly1305 data phase, directional SipHash
frame-length masking, `HandshakeAction`/`FrameAction` request
enums the runtime fulfills. Experimental and non-advertised in the
daemon (Plan 101 guard); the retained development result is
`protocol-defect-localized` at `noise_authenticated`. Detail:
[i2pr-transport-ntcp2.md](i2pr-transport-ntcp2.md).

### 4.7 `i2pr-transport-ssu2` — SSU2 v2 state machines

Runtime-neutral SSU2 v2: strict address/header/block codecs,
Noise XK establishment with header protection, bounded one-use
token lifecycle (`queue_new_token`, `matches_inbound`,
`outbound_pending`), RouterInfo binding, initiator/responder
machines, authenticated data-phase session (replay window, ACK
scheduling, retransmit with RTT/RTO/congestion, fragmentation),
path-validation/migration machines, publication snapshots,
PeerTest roles, relay requester/introducer/target machines with
HolePunch, validated introducer records, and parser-only `pq`
tolerance (`Ssu2PqKem`/`PqCapabilities`, classical X25519 sessions
only, pq-free publication). No sockets. Detail:
[i2pr-transport-ssu2.md](i2pr-transport-ssu2.md).

### 4.8 `i2pr-runtime` — Tokio supervision and I/O

The sole production Tokio owner. `ServiceGraph` topological
validation, one supervised manager per service via `JoinSet`,
narrowed `ServiceContext` (name, cancellation, readiness, health,
child scope), bounded channels, manual-clock-friendly timers,
`AuthenticatedLink` reader/writer supervision with per-frame
accounting leases, `Ssu2RuntimeService` (real loopback UDP,
path validation/migration, reachability observations),
`Ssu2PeerRelayService` (rate-limited peer-test/relay, introducer
service disabled by default), and the NTCP2 link executor.
Socket tests bind `127.0.0.1:0`. Detail:
[i2pr-runtime.md](i2pr-runtime.md).

### 4.9 `i2pr-netdb` — RouterInfo and LeaseSet2 store

Runtime-neutral NetDB: `ValidatedRouterInfo` (crypto, freshness,
key binding), bounded `RouterInfoStore` with deterministic
replacement/conflict/expiry, I2P Base64 helper, SU3 reseed parser
with RSA-SHA512-4096 verification, XOR-distance peer selection,
transport-neutral lookup/publication machines
(`LookupKind::RouterInfo`/`LeaseSet2`), `PublicationCoordinator`,
`ValidatedLeaseSet2` + `LeaseSet2Store`, and local signed
RouterInfo construction under the non-advertisement guard.
Detail: [i2pr-netdb.md](i2pr-netdb.md).

### 4.10 `i2pr-netdb-persist` — cache and reseed composition

Sits above validation and below the daemon: `CacheLoader` reads raw
cache bytes through decode → validate → insert (never trusts disk
directly); `ReseedIngestor` runs bounded offline SU3 ingestion.
Bridges `i2pr-storage` bytes and `i2pr-netdb` validation without
embedding either policy. Detail:
[i2pr-netdb-persist.md](i2pr-netdb-persist.md).

### 4.11 `i2pr-tunnel` — exploratory tunnels and data plane

Runtime-neutral tunnel substrate: tunnel identity, exploratory pool
with success-only registrar, build-record layout surface,
ECIES-X25519 short-build construction (locally conformant
request/reply wire format + Noise-N transcript), canonical
production I2NP bridge (`ShortBuildI2npBridge`, no-double-prefix
STBM invariant), local data plane (fragmentation, delivery
instructions, `DeliveryInstruction` retention), build state
machine, deterministic responder simulator, reply-path provider,
`DataPlaneRegistry` with `InboundGatewayRoute`, outbound /
inbound exploratory NetDB composition, and local zero-hop types
(`zero_hop`, Plan 172 loopback delivery). Detail:
[i2pr-tunnel.md](i2pr-tunnel.md).

### 4.12 `i2pr-client` — destinations, garlic, Streaming

Local destination product: independent Ed25519/X25519 destination
identity (non-`Clone`, non-`Debug`), per-destination tunnel pools
consuming one-shot `EstablishedMaterial`, local Standard LeaseSet2
construction/signing/self-validation, rotation/withdrawal
lifecycle, router-local registry with capacity +
duplicate-rejection guards, `LeaseSelector` /
`compose_outbound_delivery` (canonical `Garlic` I2NP carrier via
`garlic_i2np_bytes`), `DestinationRouting` cache,
`DestinationDispatcher` inbound surface with
`DestinationId → DestinationHash` binding, normative
ECIES-X25519-AEAD-Ratchet sessions (paired by remote static key,
remove-on-hit tag windows, provisional responder state,
bundled-LS2 sender binding, `PlannedOutboundForm` machine, reverse
routing), and the Streaming core (normative flags, no option TLVs,
payload-only `MAX_PACKET_SIZE`, raw final signatures, replay NACKs
on SYN only, retransmit/ACK/reorder, CLOSE/RESET policy,
`StreamingDestinationAdapter`). Also carries the client-owned
destination bridge (`DestinationOwnership`, `DestinationPublic`,
non-`Clone` `InboundDecryptionCapability`,
`install_client_lease_set2`, typed `LeaseRequest`). Detail:
[i2pr-client.md](i2pr-client.md).

### 4.13 `i2pr-api` — SAM 3.1 and I2CP adapters

Runtime-neutral application-protocol adapters (no sockets, no
Tokio). SAM 3.1: bounded line/command/reply parser, version
negotiation, I2P Base64 codec (`-`/`~`, `=` padding),
`SamPrivateDestination` codec, `SamSessionRegistry`,
`LineReader`, `ServerConnectionState`, per-session
`SamStreamRegistry`, STREAM CONNECT/ACCEPT bridge, FORWARD and
`NAMING LOOKUP` policy. I2CP: `0x2a` preamble + common frame +
structural message codecs, M9 profile, `ConnectionStateMachine`,
canonical `SessionConfig` verification (±30 s skew, injected
clock), option disposition → `DestinationConfig` projection,
`SessionRegistry` reserve/commit/rollback, reconfiguration
taxonomy, typed `I2cpAction` vocabulary, and the bounded message
data plane (`I2cpMessageOutcome`, `PendingStatusTable`,
`InboundPayloadQueue`, per-session ceilings). Detail:
[i2pr-api.md](i2pr-api.md).

### 4.14 `i2pr-service-tunnels` — service-tunnel policy

Runtime-neutral M10 policy only (no sockets; daemon owns
listeners): typed kinds (`generic-client` / `generic-server` /
`http-client` / `socks5-client` / `irc-client` / `irc-server`),
destination references, static aliases, listener/target shapes,
resource/deadline ceilings, validated sets, typed errors/events,
plus the HTTP/1.1 parser-rewrite-target-validator surface, RFC
1928 SOCKS5 negotiation/request/reply surface, IRC/IRCv3
line-parser/tag/classifier/filter surface, and the IRC-server
registration interceptor with authenticated peer-hash projection
(`<52-char base32>.b32.i2p`). Detail:
[i2pr-service-tunnels.md](i2pr-service-tunnels.md).

### 4.15 `i2pr-daemon` — CLI and composition root

The `i2pr` binary. Parses CLI, loads/validates TOML under
`deny_unknown_fields` with stable exit codes, runs
`bootstrap_daemon` (identity → local RouterInfo → cache → optional
bounded SU3 reseed → store), builds the `ServiceGraph`, and
supervises shutdown. Owns: `[netdb]`/`[reseed]` pipeline
(`NetDbSeam`), exploratory build coordinator + tunnel liveness,
NetDB-over-tunnels coordinator, destination coordinators,
`OutboundGatewayRole` lookup/store composition,
`LocalInboundEndpointRole` dispatch (`outbound_lookup.rs`,
`inbound_dispatch.rs`), the strict controlled `ssu2-router`
service with central authenticated dispatcher (`router_i2np.rs`),
loopback SAM and I2CP listeners (`sam.rs`, `i2cp.rs`) with
supervised admission + per-connection ceilings, the shared
`destination_streaming` byte pump, per-profile service-tunnel
executors (`service_tunnels*.rs`), `ServiceTunnelManager` with
typed remote routing seams and `RemoteDestinationBackend`, and the
single production composition helper `ServiceProduct::start`.
Strict disabled-by-default loopback-only `[sam]` / `[i2cp]` /
`[service_tunnels]` / `[ssu2]` surfaces. Detail:
[i2pr-daemon.md](i2pr-daemon.md).

### 4.16 `i2pr-testkit` — deterministic test fixtures

Test-only simulation seam: `ManualClock`, `NetworkScheduler` with
virtual stream/datagram links and bounded delivery queues,
scripted `FaultScript` (drop/delay/duplicate/reorder/truncate),
`ReproducibilitySeed` + `DeterministicRng` (ChaCha8),
`Ntcp2DataPhaseDriver`, ephemeral peer factories. No production
crate may depend on it (checker-enforced). Detail:
[i2pr-testkit.md](i2pr-testkit.md).

### 4.17 `tools/i2pr-interop` — disposable test launcher

Non-production binary for the synthetic NTCP2 scenario only:
prepares temporary identity/static-key/RouterInfo, runs either the
listener or dial path, performs a DeliveryStatus smoke exchange,
then bounded cleanup. Never activates `i2pr-daemon`, publishes
capabilities, or creates interop evidence. Detail:
[tooling.md](tooling.md).

## 5. Tools and capabilities

Full inventory: [tooling.md](tooling.md). Summary for reviewers:

### 5.1 Guardrail scripts (`scripts/check-*.sh`)

CI-enforced contracts. If a script rejects, fix the boundary; do
not weaken the script.

- `check-dependency-direction.sh` — crate DAG allowlist via
  `cargo metadata`.
- `check-runtime-boundaries.sh` — no unbounded channels, wall-clock
  sleeps, raw `JoinHandle`, ownerless `spawn`, `async fn` in
  transport contracts, Tokio/`std::net`/`std::fs` in wrong crates,
  or production deps on `i2pr-testkit`.
- `check-service-tunnel-boundaries.sh` — M10 runtime-neutral
  invariants (single pump, single manager entry point).
- `check-fixture-manifest.sh`, `check-ntcp2-vectors.sh`,
  `check-ssu2-vectors.sh`, `check-i2cp-vectors.sh` — fixture/vector
  corpus integrity (manifest ↔ file ↔ SHA-256, required IDs).
- `check-sam-acceptance-evidence.sh`,
  `check-ssu2-acceptance-evidence.sh`,
  `check-i2cp-acceptance-evidence.sh`,
  `check-service-tunnel-acceptance-evidence.sh`,
  `check-exploratory-tunnel-evidence.sh`,
  `check-netdb-tunnel-evidence.sh`,
  `check-destination-tunnel-evidence.sh`,
  `check-streaming-tunnel-evidence.sh`,
  `check-m6-mixed-router-acceptance-evidence.sh`,
  `check-m6-mixed-router-acceptance-evidence.sh` companion
  `check-m6-final-closure-evidence.sh` (manual gate) — evidence
  integrity: no literal unconditional `passed` rows; every counted
  row flows through exit-code/evidence-key-gated helpers.
- `check-ntcp2-interoperability.sh`,
  `check-rootless-interop-boundary.sh`,
  `check-multipass-interop-boundary.sh`,
  `check-constrained-host-lane-boundary.sh` — historical NTCP2 /
  sandbox lane boundaries (fail-closed, no silent fallback).

### 5.2 Fixture and vector corpora (`tests/fixtures/`)

Committed golden + malformed fixtures with manifests: I2NP wire
corpus, NTCP2 crypto vectors, SSU2 v2 vectors (foundation +
handshake + data-phase), I2CP wire vectors. Changing committed
fixture bytes requires re-running the matching
`check-*-vectors.sh` / `check-fixture-manifest.sh`.

### 5.3 Integration and interop lanes (`tests/integration/`)

- `tests/integration/sam/` — localhost SAM STREAM product and
  acceptance (black-box TCP/SAM only after listener startup).
- `tests/integration/i2cp/` — `run-independent.sh` loopback lane
  with exact-pinned Java I2P + go-i2cp drivers (fail-closed rows).
- `tests/integration/ssu2/` — `run-independent.sh` direct-IPv4
  loopback lane against exact-pinned i2pd (fail-closed ledger).
- `tests/integration/m6-interop/` — `run-preflight.sh`,
  `run-tunnels.sh`, `run-netdb.sh`, `run-destination.sh`,
  `run-streaming.sh`, `run-java.sh`, `run-m6-mixed-router.sh`:
  authenticated I2NP preflight, one-hop exploratory tunnels,
  NetDB lookup/publication, destination delivery, Streaming,
  Java second-family topology (controlled launcher).
- `tests/integration/service-tunnels/` — `run-independent.sh`
  (delegates remote to `run-plan214-applications.sh`),
  `run-plan213-generic.sh` (router-backed generic A/B),
  `run-plan214-applications.sh` (product-only HTTP/IRC).
- `tests/integration/ntcp2/harness/` — historical synthetic lane
  plus `execution_lane.py` unit suite (`python3 -m unittest`).
- Raw reference logs are never evidence; only sanitized
  counts/hashes reach evidence files.

### 5.4 Fuzzing (`fuzz/`)

Opt-in `cargo-fuzz` (nightly) targets for every top-level decoder:
common structures, I2NP bodies, RouterInfo/Destination/LeaseSet,
NTCP2 handshake/frames/blocks, tunnel records/fragmentation,
garlic/ECIES payloads, Streaming packets, SAM/I2CP framing.
Bounded inputs; assert no panic, no excessive alloc, no infinite
loop, stable error classification. Smoke via
`scripts/fuzz-smoke.sh`.

### 5.5 Reference pins (do not change without a new plan)

- i2pd `2.61.0` (`635b013a612ff47278ef02acf8580a28e10e26c5`,
  mandatory).
- Java I2P `2.13.0` (`9134f808337b401e8e53c73734c81fab04280c9d`,
  secondary).
- go-i2cp `b529ee1c10a6011558b4d69fc9436a4afc489eac`.
- Counted SAM clients: i2psam `b80ecd48…`, i2plib `6edf51cd…`.

Environment-gated tests are `#[ignore]`-gated: ordinary runs
compile but skip them; explicit runs require
`--ignored --exact`, and missing env must fail, never silently
pass.

### 5.6 Skills (`.opencode/skills/`, `.agents/skills/`)

Loadable agent bundles: `i2pr-local-dev` (local product path
before touching destination/garlic/LS2/Streaming/SAM/I2CP/tunnel
code), `i2pr-architecture` (this surface: ADRs, plans, specs,
doc-vs-source audits), `i2pr-ntcp2-interop` (historical harness),
`i2pr-rootless-sandbox` and `i2pr-multipass-recovery` (historical
sandbox lanes).

## 6. Capability snapshot

This section summarizes; the binding records are `plans/README.md`
(newest `*-status.md` wins), `specs/support.toml`, and
`specs/CONFORMANCE.md`. No row below is a public-network or
anonymity claim.

| Area | Proven (bounded scope) | Not claimed / debt |
| --- | --- | --- |
| Destinations + garlic + LeaseSet2 + Streaming (M6 local) | Local product closed (Plan 134 authority; Plan 152 robustness corrective underneath). | Mixed-router interoperable not yet claimed; i2pd family Streaming qualified (Plan 193); Java second-family pending Plan 201/205. |
| SAM 3.1 (M7 localhost) | Final localhost acceptance closed (Plan 151; self-composed product Plan 149; external-client core Plan 150 retained). | No router-to-router claim; loopback-only, disabled by default. |
| SSU2 v2 (M8 direct interop) | Closed within bounded direct-IPv4 loopback scope vs exact-pinned i2pd, both directions + cached-token/malformed rows (Plan 161; lane isolation Plan 162). | No public advertisement; PQ-hybrid deferred; SSU1 unsupported; IPv6-external pending. |
| I2CP (M9 loopback) | Final acceptance closed, loopback-only (Plan 172; wire/data-plane Plan 170 retained; invalid-preamble hardening Plan 171). Independent Java/go clients proven on loopback lane. | No remote-I2CP / public-network claim; no `HostLookup` resolution. |
| Service tunnels (M10) | Local generic/HTTP/SOCKS5/IRC product + round-trip closed (Plans 174–180, 182); generic router-backed A/B (Plan 213) and product-only HTTP/IRC application closure (Plan 214/215, hosted double-pass) proven against exact-pinned i2pd. | Java second-family convergence pending Plan 204 normalization; no clearnet outproxy, SOCKS UDP/BIND, TLS interception, transit/floodfill roles (M11/M12). |
| NTCP2 | Runtime-owned composition exists; development result `protocol-defect-localized` at `noise_authenticated`. Daemon NTCP2 disabled by guard. | No production activation; no interop claim. |
| NetDB / tunnels over network | One-hop exploratory tunnels, NetDB lookup/publication, destination delivery proven vs i2pd on the M6 lane (Plans 184–192). | Full multi-hop, floodfill, transit participation not claimed. |

Unsupported or deferred by design (fail-closed, never silently
bridged): legacy NTCP/SSU1, PQ session establishment (parser
tolerance only), clearnet outproxy, SOCKS UDP ASSOCIATE/BIND,
SOCKS4/auth, transparent proxying, HTTP/2+ termination, TLS
interception, IRC DCC/WEBIRC, general address-book management,
transit/floodfill router roles, in-process Rust plugins,
`Arc<RouterContext>` service locators.

## 7. How data flows at runtime

A live `i2pr run` composes bottom-up; each layer only sees the
narrow seam below it:

1. `i2pr-daemon` parses CLI, loads TOML (`deny_unknown_fields`),
   maps errors to stable exit codes, then runs
   `bootstrap_daemon` before starting the supervisor.
2. `i2pr-storage` loads `router.identity` and (separately)
   `ntcp2.static.key`. Either file can be generated, never
   silently replaced.
3. `i2pr-netdb` self-validates the local `RouterInfo` via
   `LocalRouterInfoBuilder`; refuses unqualified transports or
   capability letters under the activation guard.
4. `i2pr-netdb-persist` revalidates the persistent cache
   (`CacheLoader`), then runs the optional bounded offline SU3
   reseed (`ReseedIngestor`).
5. `i2pr-netdb` serves `RouterInfoStore`,
   `CoalescedRouterInfoLookup`, `PublicationCoordinator`, and the
   Standard LeaseSet2 store. The daemon `NetDbSeam` exposes them;
   a registered inbound tunnel flips the seam to `Available` via
   the `ReplyPathProvider` backed by `i2pr-tunnel`.
6. `i2pr-tunnel` holds the exploratory pool, short-build crypto,
   canonical I2NP bridge, data plane, and NetDB-over-tunnel
   composition. Outbound gateway roles emit `DatabaseLookup` /
   `DatabaseStore`; inbound endpoint roles dispatch `TunnelData`.
7. `i2pr-client` owns destination identity, pools, LeaseSet2
   lifecycle, ECIES sessions, garlic routing/dispatch, and
   Streaming. Outbound: lease selection → `compose_outbound_delivery`
   → Garlic I2NP bytes → tunnel data plane. Inbound: tunnel data →
   garlic envelope → session classify → Streaming dispatch.
8. `i2pr-runtime` validates the `ServiceGraph`, then supervises one
   manager per service (`lifecycle`, `netdb-bootstrap`, optional
   `ssu2-router`, optional loopback SAM/I2CP listeners, service
   tunnels). Each service gets a narrowed `ServiceContext`, never
   the supervisor itself.
9. Transports sit underneath: `i2pr-transport` link manager;
   `i2pr-transport-ntcp2` declared but unused in production;
   `i2pr-transport-ssu2` handshake/session machines driven by the
   runtime UDP service with path validation and peer-test/relay
   coordination.
10. `i2pr-proto` + `i2pr-crypto` stay at the bottom; `i2pr-core`
    provides budgets/health/cancellation everywhere;
    `i2pr-testkit` exercises the same crates in tests via virtual
    time/links/faults (never in production).

Client ingress paths (all loopback-only, disabled by default):

- SAM TCP → `i2pr-api` SAM state → daemon `sam.rs` bridge →
  `i2pr-client` destination/Streaming product → tunnel data plane.
- I2CP TCP (`0x2a` preamble + frames) → `i2pr-api` I2CP machines →
  daemon `i2cp.rs` → Plan 166 client-owned destination runtime →
  same destination/Streaming product.
- Service-tunnel TCP (generic/HTTP/SOCKS5/IRC-client) or Streaming
  accept (IRC-server) → `i2pr-service-tunnels` policy/parser →
  daemon per-profile executor → shared Streaming pump → same
  destination product. Remote routing goes through the typed
  `RemoteDestinationBackend` seams, never a test-owned shadow
  stack.

## 8. Conventions for reviewers

- `#![forbid(unsafe_code)]` everywhere (workspace deny); `todo` /
  `unimplemented` / `dbg!` denied by Clippy.
- Typed errors only in library crates; decode results never
  swallowed; exact-consumption decodes with trailing-byte
  rejection.
- Hostile-input posture: explicit bounds, checked arithmetic,
  tested negative/malformed/max-plus-one paths, golden vectors,
  fuzz targets, deterministic state-machine tests, cancellation
  and resource-exhaustion tests.
- Runtime tests prefer `#[tokio::test(start_paused = true)]` +
  manual clock + bounded deadlines; socket tests use
  `127.0.0.1:0`. Queue tests cover capacity 1 / exact / max+1 and
  lease release on every drop path.
- Black-box product tests (e.g. `sam_stream_self_composed.rs`)
  drive behavior only through TCP/SAM after listener startup —
  never private bridge/LeaseSet2/driver APIs.
- Focused commits only; no git config changes, no `--no-verify`,
  no force-push, no amending others. Handoff lists files changed,
  behavior + exact test commands/results, tests not run + why,
  dep changes, security decisions, deviations, remaining risks.

## 9. Cross-references and suggested review order

- Top-level narrative: [../architecture.md](../architecture.md)
- Security model: [../security-model.md](../security-model.md)
- Protocol support (generated): [../protocol-support.md](../protocol-support.md)
- Conformance policy: [../../specs/CONFORMANCE.md](../../specs/CONFORMANCE.md)
- Machine-readable support: [../../specs/support.toml](../../specs/support.toml)
- Plan authority: [../../plans/README.md](../../plans/README.md)
- Workspace rules: [../../AGENTS.md](../../AGENTS.md),
  [../../GUARDRAILS.md](../../GUARDRAILS.md)
- Boundary detail: [dependency-graph.md](dependency-graph.md)
- Tooling inventory: [tooling.md](tooling.md)
- Harness boundary: [interop-apparatus.md](interop-apparatus.md)
- ADRs: [../adr/](../adr/)

Suggested review path for a new reader:

1. This overview (§1–§4) for the map.
2. [dependency-graph.md](dependency-graph.md) + `i2pr-core`,
   `i2pr-proto`, `i2pr-crypto` deep dives for the foundation.
3. `i2pr-netdb` → `i2pr-netdb-persist` → `i2pr-tunnel` for network
   state.
4. `i2pr-transport` → `i2pr-transport-ssu2` (`-ntcp2` historical)
   → `i2pr-runtime` for links and supervision.
5. `i2pr-client` → `i2pr-api` → `i2pr-daemon` for destinations and
   client ingress.
6. `i2pr-service-tunnels` → daemon service-tunnel executors for
   the application layer.
7. [tooling.md](tooling.md) + the lane under review
   (`tests/integration/<area>/run-*.sh` + matching
   `scripts/check-*-evidence.sh`) for evidence semantics.

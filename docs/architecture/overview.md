# `i2pr` Architecture Overview

A bird's-eye view of the `i2pr` workspace: what each discrete module
owns, what the tooling owns, what capabilities exist today, and how
everything fits together at runtime.

This file has two jobs:

1. **Overview.** One prose summary per discrete module/component,
   enough to place it in the system before opening its source.
2. **Index.** Every row links to a deep dive in this directory that
   carries the review-grade detail: module table, public surface,
   key contracts, constants, errors, dependencies, tests, and design
   choices.

> Status: experimental. Not production-ready. No anonymity, privacy,
> or censorship-resistance claim. NTCP2 is experimental and
> non-advertised. SAM / I2CP / service tunnels are loopback-only,
> disabled by default, and non-advertised. See `README.md`,
> `GUARDRAILS.md`, and `specs/CONFORMANCE.md`.

Authority order for any behavioral claim:

```text
plans/closure/*/*-status.md > executable tests/scripts > ADRs > prose in docs/
```

`plans/registry.md` plus the newest `plans/closure/<subsystem>/<NNN>-status.md` for the
task area win over any narrative here. Never mark a row `passed`
because prose says so; it must derive from an executed command.
`specs/support.toml` plus `specs/CONFORMANCE.md` gate every protocol
support claim.

## 1. What `i2pr` is

`i2pr` is an experimental I2P router written in Rust, organized as a
**modular monolith**: one daemon process, one crate per subsystem, a
strictly enforced dependency DAG. 28 workspace crates plus one
non-production launcher tool.

Five conceptual planes cut across the crates:

| Plane | Responsibility | Representative crates |
| --- | --- | --- |
| Foundation | Wire codecs, crypto wrappers, service contracts, signed containers, managed-app contract | `i2pr-proto`, `i2pr-app-proto`, `i2pr-app-manager-proto`, `i2pr-crypto`, `i2pr-core`, `i2pr-su3` |
| Data | Authenticated links, I2NP messages, tunnel traffic, garlic, streaming packets | `i2pr-transport`, `i2pr-transport-ntcp2`, `i2pr-transport-ssu2`, `i2pr-tunnel`, `i2pr-client` |
| Network state | RouterInfo / LeaseSet2 validation, store, lookup, publication, floodfill records, tunnel construction | `i2pr-netdb`, `i2pr-netdb-persist`, `i2pr-tunnel` |
| Control | Config, identity persistence, Tokio/socket/timer ownership, supervision, composition | `i2pr-storage`, `i2pr-runtime`, `i2pr-daemon` |
| Client / service | Destinations, ECIES sessions, Streaming, SAM 3.3 local profile, I2CP, HTTP/SOCKS5/IRC/generic service tunnels, I2PControl, naming | `i2pr-client`, `i2pr-api`, `i2pr-service-tunnels`, `i2pr-addressbook`, `i2pr-i2pcontrol`, `i2pr-daemon` |
| Operator console | Loopback browser console: routing, embedded assets, themes, browser security, read-only overview | `i2pr-console`, `i2pr-daemon` |

Hard boundaries (CI-enforced; fix code, never weaken scripts):

- Dependency direction flows one way (§2). No production crate
  depends on `i2pr-testkit`.
- `i2pr-runtime` is the sole production owner of Tokio, sockets,
  timers, channels, and cancellation. Transport / API / service /
  tunnel / client / netdb / addressbook / i2pcontrol crates stay
  runtime-neutral: no `tokio::*`, no `std::net`, no `std::fs`, no
  ownerless `spawn`, no unbounded channels.
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
  i2pr-core/                Runtime-neutral contracts/budgets/health (zero deps)
  i2pr-app-proto/           Runtime-neutral managed-app protocol/capability contract (no I/O)
  i2pr-app-manager-proto/   Private trusted AppManager protocol contract (no I/O)
  i2pr-app-package/         Signed managed-app package verifier and immutable local store
  i2pr-app-state/           Persistent offline app policy and verified launch decisions
  i2pr-appctl/              Offline managed-app administration binary
  i2pr-proto/               Bounded wire codecs, typed errors, no I/O
  i2pr-crypto/              Protocol crypto wrappers (no local primitives)
  i2pr-su3/                 Bounded SU3 framing + RSA signature verification
  i2pr-transport/           Runtime-neutral link/delivery contracts + selection policy
  i2pr-transport-ntcp2/     NTCP2 protocol state machines (no I/O)
  i2pr-transport-ssu2/      SSU2 v2 protocol + path/peer-test/relay machines (no I/O)
  i2pr-storage/             Identity/key/address-book-generation persistence (atomic, versioned)
  i2pr-netdb/               RouterInfo + LeaseSet2 + ELS2 validation/store/lookup
  i2pr-netdb-persist/       Cache loader + SU3 reseed + floodfill record envelope
  i2pr-tunnel/              Exploratory/transit pool, short-build, data plane
  i2pr-client/              Destination lifecycle, ECIES session/routing, Streaming
  i2pr-api/                 Runtime-neutral SAM 3.1–3.3 + I2CP wire/state (no sockets)
  i2pr-service-tunnels/     Runtime-neutral tunnel config/policy (no sockets)
  i2pr-addressbook/         Canonical `.i2p` naming owner (no I/O)
  i2pr-i2pcontrol/          Proposal 170 wire/domain contract (no I/O)
  i2pr-console/             Loopback browser console (no sockets, no workspace deps)
  i2pr-appd/                Trusted application manager process (separate trust zone)
  i2pr-apphost/             Direct-exec application supervisor (the only app exec site)
  i2pr-app-fixture/         Managed-app black-box fixture (evidence tooling, not product)
  i2pr-runtime/             Sole Tokio/socket/timer/channel owner + supervision
  i2pr-daemon/              CLI/config/composition root; owns all listeners
  i2pr-testkit/             Deterministic fixtures only (test-only)
tools/
  i2pr-interop/             Non-production test launcher (never activates daemon)
```

### 2.1 Dependency graph

Edges below are the **production** (`[dependencies]`, non-dev)
workspace edges, matching the allowlist in
`scripts/check-dependency-direction.sh`. Full detail, including the
per-crate allowlist table, lives in
[dependency-graph.md](dependency-graph.md).

```text
                 i2pr-core                (zero workspace deps)
                 i2pr-su3                 (zero workspace deps)
                      |                          |
i2pr-proto <- i2pr-crypto                       |
    ^    ^            ^                         |
    |    |            |                         |
    |    |      i2pr-storage                    |
    |    |            |                         |
    +----+------------+-----------+             |
         |                        |             |
  i2pr-transport          i2pr-netdb <-----------+
    ^   ^    ^                  ^                |
    |   |    |                  |                |
i2pr-transport-ntcp2       i2pr-netdb-persist     |
    ^                            ^                |
    |                            |                |
i2pr-transport-ssu2              |                |
    ^                            |                |
    |                            |                |
i2pr-runtime <----+               |                |
                  |               |                |
i2pr-tunnel <-----+               |                |
    ^               |               |                |
    |               |               |                |
i2pr-client --------+               |                |
    ^               |               |                |
    |               |               |                |
i2pr-api            |               |                |
    ^               |               |                |
i2pr-service-tunnels               |                |
                    |               |                |
                    +---------------+                |
i2pr-addressbook --------------------+   i2pr-tunnel -+
                                                    |
                                      i2pr-client --+

i2pr-i2pcontrol (zero workspace deps)   i2pr-daemon (composition root; depends on all)

  i2pr-app-proto / i2pr-app-manager-proto   (zero workspace deps)
          ^                    ^   ^
          |                    |   |
          |                    |   +-- i2pr-apphost   [NO appd->apphost edge:
          |                    |   |                    they are related by a
          |                    |   |                    process, not a crate edge]
          |                    |   +-- i2pr-app-fixture (evidence tooling)
          |                    |
          +-- i2pr-appd  -----+
                   ^
                   |
          i2pr-app-fixture          (the only crate allowed to depend on appd,
                                     because the fixture manager must run the
                                     real manager to qualify it)

i2pr-testkit  (test-only; may depend on core/crypto/proto/runtime/transport/ntcp2)
tools/i2pr-interop  (non-production launcher)
```

Flattened allowlist (the exact set the checker enforces per crate):

| Crate | Allowed direct production `i2pr-*` deps |
| --- | --- |
| `i2pr-core` | — |
| `i2pr-proto` | — (dev-only on `i2pr-crypto`) |
| `i2pr-su3` | — |
| `i2pr-i2pcontrol` | — |
| `i2pr-app-proto` | — |
| `i2pr-app-manager-proto` | — |
| `i2pr-app-package` | `i2pr-app-proto` |
| `i2pr-app-state` | `i2pr-app-package`, `i2pr-app-proto` |
| `i2pr-appctl` | `i2pr-app-package`, `i2pr-app-proto`, `i2pr-app-state` |
| `i2pr-appd` | `i2pr-app-manager-proto`, `i2pr-app-proto`, `i2pr-app-state` |
| `i2pr-apphost` | `i2pr-app-manager-proto`, `i2pr-app-proto` |
| `i2pr-app-fixture` | `i2pr-app-manager-proto`, `i2pr-app-proto`, `i2pr-appd` (evidence tooling) |
| `i2pr-crypto` | `i2pr-proto` |
| `i2pr-transport` | `i2pr-core`, `i2pr-proto` |
| `i2pr-transport-ntcp2` | `i2pr-crypto`, `i2pr-proto`, `i2pr-transport` |
| `i2pr-transport-ssu2` | `i2pr-crypto`, `i2pr-proto`, `i2pr-transport` |
| `i2pr-storage` | `i2pr-crypto` |
| `i2pr-netdb` | `i2pr-crypto`, `i2pr-proto`, `i2pr-su3` |
| `i2pr-netdb-persist` | `i2pr-crypto`, `i2pr-netdb`, `i2pr-proto`, `i2pr-storage` |
| `i2pr-runtime` | `i2pr-core`, `i2pr-crypto`, `i2pr-proto`, `i2pr-transport`, `i2pr-transport-ntcp2`, `i2pr-transport-ssu2` |
| `i2pr-tunnel` | `i2pr-core`, `i2pr-crypto`, `i2pr-netdb`, `i2pr-proto` |
| `i2pr-client` | `i2pr-core`, `i2pr-crypto`, `i2pr-netdb`, `i2pr-proto`, `i2pr-tunnel` |
| `i2pr-api` | `i2pr-client`, `i2pr-crypto`, `i2pr-proto`, `i2pr-tunnel` |
| `i2pr-service-tunnels` | No internal crate dependencies; reusable policy core per ADR 0033 |
| `i2pr-addressbook` | `i2pr-proto` |
| `i2pr-console` | — (no workspace dependencies; owns no socket) |
| `i2pr-testkit` | `i2pr-core`, `i2pr-crypto`, `i2pr-proto`, `i2pr-runtime`, `i2pr-transport`, `i2pr-transport-ntcp2` |
| `i2pr-daemon` | `i2pr-addressbook`, `i2pr-api`, `i2pr-client`, `i2pr-console`, `i2pr-core`, `i2pr-crypto`, `i2pr-i2pcontrol`, `i2pr-netdb`, `i2pr-netdb-persist`, `i2pr-proto`, `i2pr-runtime`, `i2pr-service-tunnels`, `i2pr-storage`, `i2pr-su3`, `i2pr-transport`, `i2pr-tunnel` |

## 3. Module index

Each row is a one-line summary. The linked document is the
review-focused deep dive for that component: purpose, module layout,
public surface, key contracts, errors, dependencies, tests, and
design choices. Every workspace crate appears exactly once.

| # | Crate / area | Role | One-liner | Deep dive |
| --- | --- | --- | --- | --- |
| 1 | `i2pr-core` | Contracts | Lifecycle FSM, health/liveness, wakeable cancellation, `ResourceBudget` with RAII leases. Zero deps. | [i2pr-core.md](i2pr-core.md) |
| 2 | `i2pr-proto` | Codecs | Bounded I2P common-structure + I2NP codecs, LeaseSet2 carrier, I2CP Data body, typed errors. No I/O. | [i2pr-proto.md](i2pr-proto.md) |
| 3 | `i2pr-crypto` | Crypto wrappers | Ed25519 / X25519 / SHA-256 / HKDF / ECIES / ChaCha / Red25519 wrappers. Secrets zeroized, non-`Debug`, non-`Clone`. | [i2pr-crypto.md](i2pr-crypto.md) |
| 4 | `i2pr-su3` | Signed container | Bounded SU3 envelope framing and explicit-key RSA-SHA512 verification. | [i2pr-su3.md](i2pr-su3.md) |
| 5 | `i2pr-storage` | Persistence | Versioned atomic `router.identity` + `ntcp2.static.key` + service destinations + address-book generations. | [i2pr-storage.md](i2pr-storage.md) |
| 6 | `i2pr-transport` | Link contracts | `LinkState` FSM, admission with RAII leases, NTCP2/SSU2 selection + reachability policy. No Tokio/async. | [i2pr-transport.md](i2pr-transport.md) |
| 7 | `i2pr-transport-ntcp2` | NTCP2 protocol | Noise XK handshake, AES-CBC obfuscation, ChaCha20-Poly1305 data phase, SipHash length masking. Emits actions; runtime fulfills them. | [i2pr-transport-ntcp2.md](i2pr-transport-ntcp2.md) |
| 8 | `i2pr-transport-ssu2` | SSU2 v2 protocol | Addresses/headers/blocks, Noise XK + header protection + one-use tokens, data-phase reliability, path validation, peer-test/relay. No sockets. | [i2pr-transport-ssu2.md](i2pr-transport-ssu2.md) |
| 9 | `i2pr-netdb` | Local NetDB | RouterInfo/ELS2/LeaseSet2 validation, bounded stores, lookup/publication/replication machines, peer selection, local RouterInfo builder. | [i2pr-netdb.md](i2pr-netdb.md) |
| 10 | `i2pr-netdb-persist` | Cache composition | `CacheLoader` + `ReseedIngestor` + versioned `floodfill_records` durable envelope. | [i2pr-netdb-persist.md](i2pr-netdb-persist.md) |
| 11 | `i2pr-tunnel` | Tunnel substrate | Tunnel identity, exploratory + transit pools, ECIES-X25519 short-build, canonical I2NP bridge, data plane, reply-path provider. | [i2pr-tunnel.md](i2pr-tunnel.md) |
| 12 | `i2pr-client` | Destination runtime | Destination identity/pools/registry, LeaseSet2 lifecycle, ECIES-X25519-AEAD-Ratchet sessions, garlic, Streaming, client-owned destinations. | [i2pr-client.md](i2pr-client.md) |
| 13 | `i2pr-api` | App protocols | SAM 3.1–3.3 parser/registry/PRIMARY child profile + I2CP preamble/frame/message codecs, connection/session/option machines, data plane. No sockets. | [i2pr-api.md](i2pr-api.md) |
| 14 | `i2pr-service-tunnels` | Service policy | Runtime-neutral kinds, destination refs, ceilings, HTTP/SOCKS5/IRC parser + policy surfaces. Daemon owns listeners. | [i2pr-service-tunnels.md](i2pr-service-tunnels.md) |
| 15 | `i2pr-addressbook` | Naming owner | Canonical `.i2p` books, precedence resolver, subscriptions, versioned generations. No I/O. | [i2pr-addressbook.md](i2pr-addressbook.md) |
| 16 | `i2pr-i2pcontrol` | Control contract | Proposal 170 JSON-RPC 2.0 contract: envelope, auth vocabulary, method/type/selector inventories, tunnel option metadata, wire ceilings. No I/O. | [i2pr-i2pcontrol.md](i2pr-i2pcontrol.md) |
| 17 | `i2pr-runtime` | Runtime owner | Only production Tokio owner: `ServiceGraph`, supervised `JoinSet`, NTCP2 link service, SSU2 UDP service, peer/relay coordinator, bounded channels/timers. | [i2pr-runtime.md](i2pr-runtime.md) |
| 18 | `i2pr-daemon` | Composition root | CLI, TOML config, identity lifecycle, NetDB/bootstrap, SSU2 router, SAM/I2CP/I2PControl listeners, service-tunnel executors, `ServiceProduct::start`. | [i2pr-daemon.md](i2pr-daemon.md) |
| 19 | `i2pr-testkit` | Test simulation | `ManualClock`, `NetworkScheduler`, virtual links, `FaultScript`, deterministic RNG, streaming fingerprint. Test-only. | [i2pr-testkit.md](i2pr-testkit.md) |
| 20 | `tools/i2pr-interop` | Test launcher | Disposable NTCP2 composition root: temp identity/RouterInfo, listener-or-dial, DeliveryStatus smoke, bounded cleanup. | [tooling.md](tooling.md) |
| 21 | `scripts/` + `tests/` + `fuzz/` | Tooling | Guardrail checkers, fixture corpora, integration lanes, fuzz targets, CI gates. | [tooling.md](tooling.md) |
| 22 | Dependency graph | Boundary detail | Per-crate allowlist + ASCII graph backing `check-dependency-direction.sh`. | [dependency-graph.md](dependency-graph.md) |
| 23 | Interop apparatus | Harness boundary | Reference-router harness, evidence classes, sanitization, Multipass/rootless lanes (historical NTCP2 surface). | [interop-apparatus.md](interop-apparatus.md) |
| 25 | `i2pr-app-manager-proto` | AppManager contract | Private router/AppManager protocol: handshake, bounded frames, strict directional control vocabulary, opaque daemon-assigned handles, pure bounded accounting. Authority ceiling below Proposal 170; no administrator vocabulary. Plan 368. | [i2pr-app-manager-proto.md](i2pr-app-manager-proto.md) |
| 24 | `i2pr-app-proto` | App contract | Managed native-app v1 protocol, capabilities, manifest, default-deny policy and sandbox attestation vocabulary. No OS/runtime owner. | [i2pr-app-proto.md](i2pr-app-proto.md) |
| 26 | `i2pr-appd` | App manager process | Trusted manager process: persistent policy catalog, reverified package launch authority, inherited anonymous transport, concurrent manager client, app v1 session, bounded instance registry. Separate trust zone. Plans 369/383. | [i2pr-appd.md](i2pr-appd.md) |
| 27 | `i2pr-apphost` | Direct-exec supervisor | One-shot bounded supervisor: single launch request, double-checked root containment, direct no-shell exec, `Secured` refused before exec, byte-transparent relay, direct-child cleanup. Plan 369. | [i2pr-apphost.md](i2pr-apphost.md) |
| 28 | `i2pr-app-fixture` | Black-box fixture | Evidence tooling: native fixture application plus a fixture manager running the real `Appd` against a test catalog. Production source does not name or bundle it; qualification installs it only through an explicit temporary signed policy. Plans 369/383. | [i2pr-app-fixture.md](i2pr-app-fixture.md) |
| 29 | `i2pr-app-package` | Signed package/store | Bounded Stored-only package verifier, Ed25519 publisher identity, signed inventory, immutable local installation. Plan 382. | [i2pr-app-package.md](i2pr-app-package.md) |
| 30 | `i2pr-app-state` | Persistent policy | Strict generation store, offline policy mutation, exact package re-verification, validated launch decisions. Plan 383. | [i2pr-app-state.md](i2pr-app-state.md) |
| 31 | `i2pr-appctl` | Offline administrator CLI | Package verify/install/list/inspect/remove and restart-applied publisher/app policy. Plan 383. | [i2pr-appctl.md](i2pr-appctl.md) |

### 4.24 Managed application package and policy (Plans 382–383)

`i2pr-app-package` proves signed package integrity and publisher-key identity;
it makes no trust or launch decision. `i2pr-app-state` persists explicit local
publisher trust, exact package selection, grants, profile, and autostart in
immutable generations. `i2pr-appctl` is the offline mutation surface. The
production `i2pr-appd` catalog re-verifies selected packages, holds one policy
snapshot under a lifetime runtime lock, and launches only explicit autostarts.
`UnsafeDirect` means ordinary host networking without a sandbox; `Secured`
remains unavailable. See [policy v1](../../specs/references/managed-app-policy-v1.md).

## 4. Discrete module overviews

Each subsection is the general overview for one reviewable unit.
Follow the deep-dive link for the full contract.

### 4.1 `i2pr-core` — runtime-neutral contracts

The only zero-dependency crate. Lifecycle FSM, bounded service
names, health/liveness snapshots with redaction, `Arc<AtomicBool>`
cancellation tokens, `ResourceBudget` per-class ceilings with RAII
leases / high-water marks / denial counters, and
`ServiceFailure`/`ServiceCompletion` taxonomies. Every runtime
service builds on these; nothing here knows about sockets, codecs,
or the router. Detail: [i2pr-core.md](i2pr-core.md).

### 4.2 `i2pr-proto` — bounded wire codecs

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

### 4.3 `i2pr-crypto` — protocol crypto wrappers

Wraps reviewed primitives (Ed25519, X25519, SHA-256, HKDF-SHA256,
ChaCha20/ChaCha20-Poly1305, Elligator2 representative codec, Red25519,
ECIES-X25519 session helpers) in protocol-typed keys with
zeroize-on-drop, non-`Clone`, non-`Debug` secrets. Used by storage
(identity), tunnel short-build (ECIES-X25519 Noise-N), destination
sessions (ECIES-X25519-AEAD-Ratchet), and both transports. Transport
data-phase ciphers (AES-CBC, HMAC, SipHash, Noise) live in
`i2pr-transport-ntcp2`/`-ssu2`, not here. No local primitive
implementation. Detail: [i2pr-crypto.md](i2pr-crypto.md).

### 4.4 `i2pr-su3` — bounded SU3 envelope

The smallest crate: validate the common SU3 envelope and verify
signatures against an explicit caller-provided RSA key. Caller-owned
`Su3Limits` (`max_file_bytes`, `max_content_bytes`,
`max_signer_id_bytes`, `max_version_bytes`) are applied before any
content is exposed. Content-specific policy (reseed ZIP parsing,
NEWS XML) belongs to the consuming subsystem. Detail:
[i2pr-su3.md](i2pr-su3.md).

### 4.5 `i2pr-storage` — atomic identity persistence

Versioned records with magic + version + checksum:
`<data_dir>/router.identity` (Ed25519 + X25519 seeds and public
keys), a separate `<data_dir>/ntcp2.static.key` (X25519 static pair
+ obfuscation IV), persistent service destinations
(`service_destination.rs`), address-book generations
(`address_book_generation.rs`), and a verified-content cache
(`verified_content_cache.rs`). Atomic create via hard-link +
`AlreadyExists` (never silently replaces), `0o700` dir / `0o600`
files, explicit generate/load/rotate operations. Detail:
[i2pr-storage.md](i2pr-storage.md).

### 4.6 `i2pr-transport` — link contracts and selection

Synchronous, runtime-neutral link/delivery vocabulary: `LinkState`
FSM, owned delivery requests with deadlines + cancellation,
`TransportManager` admission with double-checked locking and RAII
leases, duplicate-resolution policy, privacy-safe snapshots, and
transport-shaped resource accounting. Also owns deterministic
NTCP2/SSU2 selection and the conservative reachability policy with
typed peer-test/relay outcomes. No Tokio, no `async fn`, no I/O.
Detail: [i2pr-transport.md](i2pr-transport.md).

### 4.7 `i2pr-transport-ntcp2` — NTCP2 state machines

Runtime-neutral NTCP2: transcript composition, consuming
initiator/responder handshake machines (`SessionRequest` /
`SessionCreated` / `SessionConfirmed` + options), AES-CBC ephemeral
obfuscation, ChaCha20-Poly1305 data phase, directional SipHash
frame-length masking, `HandshakeAction`/`FrameAction` request
enums the runtime fulfills. Experimental and non-advertised in the
daemon (Plan 101 guard); the retained development result is
`protocol-defect-localized` at `noise_authenticated`. Detail:
[i2pr-transport-ntcp2.md](i2pr-transport-ntcp2.md).

### 4.8 `i2pr-transport-ssu2` — SSU2 v2 state machines

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

### 4.9 `i2pr-netdb` — RouterInfo, ELS2, and LeaseSet2 store

Runtime-neutral NetDB: `ValidatedRouterInfo` (crypto, freshness,
key binding), bounded `RouterInfoStore` with deterministic
replacement/conflict/expiry, I2P Base64 helper, Encrypted LeaseSet2
(`els2`) + auth surface, floodfill role/service records,
transport-neutral lookup/publication/replication machines
(`LookupKind`/`LookupPolicy`/`lookup_engine`),
`PublicationCoordinator`, `ValidatedLeaseSet2` + `LeaseSet2Store`,
`server_store`/`store_message` surfaces, provenance and routing
helpers, and local signed RouterInfo construction under the
non-advertisement guard. Detail: [i2pr-netdb.md](i2pr-netdb.md).

### 4.10 `i2pr-netdb-persist` — cache, reseed, and floodfill records

Sits above validation and below the daemon: `CacheLoader` reads raw
cache bytes through decode → validate → insert (never trusts disk
directly); `ReseedIngestor` runs bounded offline SU3/reseed ZIP
ingestion; `floodfill_records` provides a versioned/checksummed
durable envelope where every restored payload must be decoded and
revalidated and restored provenance narrows to replica. Bridges
`i2pr-storage` bytes and `i2pr-netdb` validation without embedding
either policy. Detail:
[i2pr-netdb-persist.md](i2pr-netdb-persist.md).

### 4.11 `i2pr-tunnel` — exploratory/transit tunnels and data plane

Runtime-neutral tunnel substrate: tunnel identity, exploratory pool
with success-only registrar, transit role/owner composition,
build-record layout surface, ECIES-X25519 short-build construction
(locally conformant request/reply wire format + Noise-N transcript),
canonical production I2NP bridge (`ShortBuildI2npBridge`,
no-double-prefix STBM invariant), local data plane (fragmentation,
delivery instructions, `DeliveryInstruction` retention), build
state machine, deterministic responder simulator, reply-path
provider, `DataPlaneRegistry` with `InboundGatewayRoute`,
outbound / inbound / transit NetDB composition, conformance
fixtures, and local zero-hop types (`zero_hop`, Plan 172 loopback
delivery). Detail: [i2pr-tunnel.md](i2pr-tunnel.md).

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

### 4.13 `i2pr-api` — SAM 3.3 and I2CP adapters

Runtime-neutral application-protocol adapters (no sockets, no
Tokio). SAM 3.1–3.3: bounded line/command/reply parser, version
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
listeners): typed kinds (12 in total: `generic-client` /
`generic-server` / `http-client` / `socks5-client` / `irc-client` /
`irc-server` and their variants), destination references, static
aliases, listener/target shapes, resource/deadline ceilings,
validated sets, typed errors/events, access/auth surfaces,
outbound-secret handling, the outproxy provider POLICY
(`outproxy.rs`: `OutproxyEndpoint::parse` proves a target is an I2P
destination, and the path may open exactly one kind of route — an I2P
Streaming connection, never a direct clearnet socket), plus the
HTTP/1.1 parser-rewrite-target-validator surface, RFC 1928 SOCKS5
negotiation/request/reply surface, IRC/IRCv3
line-parser/tag/classifier/filter surface, the IRC-server
registration interceptor with authenticated peer-hash projection
(`<52-char base32>.b32.i2p`), idle handling, and the
`streamr` Streaming-repair surface. Detail:
[i2pr-service-tunnels.md](i2pr-service-tunnels.md).

### 4.15 `i2pr-addressbook` — canonical naming owner

Four independent administrative books (private, local, router,
published) with fixed lookup precedence, a subscription-derived
table consulted last, typed hostname and full-Destination
validation, the Proposal `SetConfig` domain, bounded subscription
ingestion, deterministic versioned generations, and a narrow
read-only resolver handle. Mutation goes only through
`AddressBookControl`; lookup goes only through the non-mutating
`AddressBookResolver`. `i2pr-storage` persists opaque generations;
`i2pr-daemon` owns refresh tasks, timers, and download
composition. Detail: [i2pr-addressbook.md](i2pr-addressbook.md).

### 4.16 `i2pr-i2pcontrol` — Proposal 170 contract

Runtime-neutral bounded wire/domain contract for the Proposal 170
I2PControl workstream: JSON-RPC 2.0 envelope semantics, API version
1 authentication vocabulary, the exact method/action/type/selector
inventories, tunnel option metadata with secret classification,
wire-level ceilings, and the machine-readable public contract
inventory. Owns no sockets, timers, filesystem, token storage,
clocks, router state, transport internals, NetDB stores, or tunnel
pools; `i2pr-daemon` adapts the contract to router state. Frozen
references are pinned in `docs/provenance/proposal-170-manifest.md`.
Detail: [i2pr-i2pcontrol.md](i2pr-i2pcontrol.md).

### 4.17 `i2pr-runtime` — Tokio supervision and I/O

The sole production Tokio owner. `ServiceGraph` topological
validation, one supervised manager per service via `JoinSet`,
narrowed `ServiceContext` (name, cancellation, readiness, health,
child scope), bounded channels, manual-clock-friendly timers,
`AuthenticatedLink` reader/writer supervision with per-frame
accounting leases, `Ssu2RuntimeService` (real loopback UDP,
path validation/migration, reachability observations),
`Ssu2PeerRelayService` (rate-limited peer-test/relay, introducer
service disabled by default), observability snapshots, and the
NTCP2 link executor / driver / handshake observer. Socket tests bind
`127.0.0.1:0`. Detail: [i2pr-runtime.md](i2pr-runtime.md).

### 4.18 `i2pr-daemon` — CLI and composition root

The `i2pr` binary, and the largest crate (~72.5k lines of `src`).
Parses CLI, loads/validates TOML under `deny_unknown_fields` with
stable exit codes, runs `bootstrap_daemon` (identity → local
RouterInfo → cache → optional bounded SU3 reseed → store), builds
the `ServiceGraph`, and supervises shutdown. Owns: `[netdb]`/`[reseed]`
pipeline (`NetDbSeam`), exploratory build coordinator + tunnel
liveness, NetDB-over-tunnels coordinator, destination coordinators,
`OutboundGatewayRole` lookup/store composition,
`LocalInboundEndpointRole` dispatch (`outbound_lookup.rs`,
`inbound_dispatch.rs`), the strict controlled `ssu2-router` service
with central authenticated dispatcher (`router_i2np.rs`), M11 transit
ownership/volume composition (`transit_owner.rs`,
`transit_volume.rs`, `transit_compose.rs`), M12 floodfill
(`floodfill.rs`), the outproxy route owner (`outproxy_route.rs` —
the socket-owning half of the Plan 343 policy/route-owner pair,
which lands with **no reachable request path**), loopback SAM and
I2CP listeners (`sam.rs`,
`i2cp.rs`) with supervised admission + per-connection ceilings, the
I2PControl listener stack (`i2pcontrol.rs`,
`i2pcontrol_tunnels.rs`, `i2pcontrol_inspection.rs`), address-book
composition (`addressbook.rs`, `addressbook_fetch.rs`,
`control_sources.rs`), NEWS (`news.rs`), the shared
`destination_streaming` byte pump, per-profile service-tunnel
executors (`service_tunnels*.rs`, `service_els2.rs`,
`service_delivery.rs`), `ServiceTunnelManager` with typed remote
routing seams and `RemoteDestinationBackend`, and the single
production composition helper `ServiceProduct::start`. Strict
disabled-by-default loopback-only `[sam]` / `[i2cp]` / `[i2pcontrol]` /
`[service_tunnels]` / `[ssu2]` surfaces. Detail:
[i2pr-daemon.md](i2pr-daemon.md).

### 4.19 `i2pr-testkit` — deterministic test fixtures

Test-only simulation seam: `ManualClock` / `TokioClock`, virtual
stream/datagram links with bounded delivery queues, scripted
`FaultScript` (drop/delay/duplicate/reorder/truncate), reproducible
seeds + deterministic RNG (ChaCha8), NTCP2 data-phase driver and
peer factories, transport virtual links, and the streaming
fingerprint surface. No production crate may depend on it
(checker-enforced). Detail: [i2pr-testkit.md](i2pr-testkit.md).

### 4.20 `tools/i2pr-interop` — disposable test launcher

Non-production binary for the synthetic NTCP2 scenario only:
prepares temporary identity/static-key/RouterInfo, runs either the
listener or dial path, performs a DeliveryStatus smoke exchange,
then bounded cleanup. Never activates `i2pr-daemon`, publishes
capabilities, or creates interop evidence. Detail:
[tooling.md](tooling.md).

### 4.21 `i2pr-appd` — the trusted application manager process

`i2pr-appd` is the separately supervised process the router starts. It is a
**separate runtime trust zone**: its only production `i2pr-*` dependencies are
the two wire contracts and `i2pr-app-state`, and it may not reach
`i2pr-daemon` or `i2pr-runtime`.

It is launched as a child over two **inherited anonymous pipes** on file
descriptors 0 and 1 — no listener, no port, no discovery endpoint — and the
executable is resolved as a `current_exe()` sibling, so nothing in configuration
or argv can substitute a different program.

The shipped binary refuses all arguments and uses a persistent catalog. It
loads explicit offline policy after the inherited-pipe handshake, re-verifies
each selected package, and launches only trusted autostart records. The
Plan-368 protocol has no manager-receivable launch request, so the daemon
cannot ask it to select an executable. `LaunchAuthority` is sealed with **no
decoder**, asserted by method resolution rather than by scanning for a derive.
Full detail in [i2pr-appd.md](i2pr-appd.md).

### 4.22 `i2pr-apphost` — the direct-exec application supervisor

`i2pr-apphost` is the **only** component that execs an application. It accepts
exactly one launch request, execs the application directly, and then becomes a
byte-transparent relay.

Containment is checked **twice** — structurally on strings, then again on
canonicalised paths — because a single check leaves either the obvious escape or
the disguised one open. `Secured` is refused **before any exec** and re-checked
at the exec site; there is no shell and no `PATH` lookup; the direct child is
owned from spawn until reaped and killed rather than leaked. Full detail in
[i2pr-apphost.md](i2pr-apphost.md).

### 4.23 `i2pr-app-fixture` — the managed-app black-box fixture

Evidence tooling, not product code. It holds the native fixture application and
a fixture manager that runs the **real** `Appd` against a test launch catalog, so
what Plan 369 qualifies is the product manager rather than a stand-in. The
manager is a separate binary because the shipped `i2pr-appd` refuses all
arguments — adding a flag would have reopened the user-configurable-program hole.

It is unreachable from production: no production crate may name it or depend on
it. Full detail in [i2pr-app-fixture.md](i2pr-app-fixture.md).

## 5. Tools and capabilities

Full inventory: [tooling.md](tooling.md). Summary for reviewers.

### 5.1 Guardrail scripts (`scripts/check-*.sh`)

CI-enforced contracts. If a script rejects, fix the boundary; do
not weaken the script.

Boundary checkers (invariants):

- `check-dependency-direction.sh` — crate DAG allowlist via
  `cargo metadata`.
- `check-runtime-boundaries.sh` — no unbounded channels, wall-clock
  sleeps, raw `JoinHandle`, ownerless `spawn`, `async fn` in
  transport contracts, Tokio/`std::net`/`std::fs` in wrong crates,
  or production deps on `i2pr-testkit`.
- `check-service-tunnel-boundaries.sh` — M10 runtime-neutral
  invariants (single pump, single manager entry point) plus **rules
  9–11** (Plan 343): the outproxy policy and route-owner pair
  (`i2pr-service-tunnels/src/outproxy.rs` +
  `i2pr-daemon/src/outproxy_route.rs`) must both exist, neither may
  name a clearnet socket/resolver/TLS client or load a plugin or
  spawn a process, and **rule 10 is a positive control** requiring
  `service_tunnels_http.rs` to still name a local `TcpStream`/
  `TcpListener` so the guard cannot go vacuous.
- `check-m11-transit-boundaries.sh`, `check-m12-floodfill-boundaries.sh`,
  `check-service-anonymity-boundaries.sh` — M11/M12/anonymity
  lane-specific invariants.
- `check-ntcp2-interoperability.sh`,
  `check-rootless-interop-boundary.sh`,
  `check-multipass-interop-boundary.sh`,
  `check-constrained-host-lane-boundary.sh` — historical NTCP2 /
  sandbox lane boundaries (fail-closed, no silent fallback).

Managed-app trust zones (Plan 368/369):

- `check-managed-app-manager-boundary.py` — the private AppManager protocol
  may be consumed only as an implementation of it, never re-derived.
- `check-managed-app-gateway-boundary.py` — the daemon app gateway keeps
  service contexts private to one launch instance.
- `check-managed-app-private-client-seams.py` — production code may not reach
  the private SAM/I2CP connection seam directly.
- `check-managed-app-process-boundary.py` — **which process execs what**:
  exactly three blessed spawn edges (daemon→manager, appd→apphost,
  apphost→application), `current_exe()` sibling resolution for the two
  distribution-owned ones, no shell launcher, no `PATH` lookup, no
  production source naming the fixture, no crate depending on it, an
  argument-refusing shipped manager with a persistent catalog that launches
  nothing until an operator supplies explicit local policy, and no production
  caller of the manager test seam. Ships `--self-test`, which applies each
  mutation in memory and requires the scan to reject it.

Fixture/vector corpus integrity:

- `check-fixture-manifest.sh`, `check-ntcp2-vectors.sh`,
  `check-ssu2-vectors.sh`, `check-i2cp-vectors.sh` — fixture/vector
  corpus integrity (manifest ↔ file ↔ SHA-256, required IDs).

Evidence integrity (no synthetic `passed` rows; exit-code/evidence-key
gated helpers):

- `check-sam-acceptance-evidence.sh`,
  `check-ssu2-acceptance-evidence.sh`,
  `check-i2cp-acceptance-evidence.sh`,
  `check-i2pcontrol-acceptance-evidence.sh`,
  `check-service-tunnel-acceptance-evidence.sh`,
  `check-exploratory-tunnel-evidence.sh`,
  `check-netdb-tunnel-evidence.sh`,
  `check-destination-tunnel-evidence.sh`,
  `check-streaming-tunnel-evidence.sh`,
  `check-m6-mixed-router-acceptance-evidence.sh`,
  `check-m6-final-closure-evidence.sh` (manual gate),
  `check-m11-transit-qualification-evidence.sh`,
  `check-m12-floodfill-qualification-evidence.sh`,
  `check-http-anonymity-evidence.sh` / `.py`,
  `check-streaming-fingerprint-evidence.sh`.

Source-lock gating and planning hygiene:

- `check-java-source-lock-gating.sh`, `run-java-source-lock-tests.sh`,
  `check-global-plan-number-uniqueness.py`,
  `check-m11-per-epoch-composition.sh`.

### 5.2 Fixture and vector corpora (`tests/fixtures/`)

Committed golden + malformed fixtures with manifests, under
`i2np/`, `ntcp2/`, `ssu2/`, and `i2cp/`. Changing committed
fixture bytes requires re-running the matching
`check-*-vectors.sh` / `check-fixture-manifest.sh`.

### 5.3 Integration and interop lanes (`tests/integration/`)

- `anonymity/` — `run-plan312-streaming.sh`.
- `floodfill/` — `run-i2pd.sh`, `run-java-floodfill.sh` (M12).
- `i2cp/` — `run-independent.sh` loopback lane with exact-pinned
  Java I2P + go-i2cp drivers (fail-closed rows).
- `i2pcontrol/` — `run-differential.sh` (Proposal 170).
- `m11-transit/` — `run-i2pd.sh` one-family transit qualification.
- `m6-interop/` — `run-preflight.sh`, `run-tunnels.sh`,
  `run-netdb.sh`, `run-destination.sh`, `run-streaming.sh`,
  `run-java.sh`, `run-m6-mixed-router.sh`.
- `ntcp2/` — historical synthetic lane plus the
  `tests/integration/ntcp2/harness/` Python suite.
- `sam/` — `run-independent.sh` localhost SAM STREAM product and
  acceptance (black-box TCP/SAM only after listener startup).
- `service-tunnels/` — `run-independent.sh` (delegates remote to
  `run-plan214-applications.sh`), `run-plan213-generic.sh`
  (router-backed generic A/B), `run-plan214-applications.sh`
  (product-only HTTP/IRC), `test-plan215-tunnels-conf.sh`.
- `ssu2/` — `run-independent.sh` direct-IPv4 loopback lane against
  exact-pinned i2pd (fail-closed ledger).

Raw reference logs are never evidence; only sanitized
counts/hashes reach evidence files.

### 5.4 Fuzzing (`fuzz/fuzz_targets/`)

Opt-in `cargo-fuzz` (nightly) targets for the byte-level decoders:
common structures (`mapping`, `hash`, `date`, `date32`, `key_certificate`,
`key_and_cert`, `certificate`), I2NP bodies (`i2np_bodies`,
`i2np_standard`, `i2np_short_ssu`, `i2np_short_transport`),
RouterInfo surfaces (`router_info`, `router_identity`,
`router_address`, `destination`), lease sets (`lease`, `lease_set`,
`leaseset2`, `metaleaseset`), and NTCP2
(`ntcp2_blocks`, `ntcp2_frames`, `ntcp2_handshake`, `ntcp2_storage`,
`ntcp2_transcript`). Bounded inputs; assert no panic, no excessive
alloc, no infinite loop, stable error classification. Smoke via
`scripts/fuzz-smoke.sh`.

### 5.5 CI workflows (`.github/workflows/`)

`ci.yml` (ordinary gate) plus manual external lanes:
`sam-external.yml`, `ssu2-external.yml`, `i2cp-external.yml`,
`service-tunnels-external.yml`, `m6-mixed-router-external.yml`,
`m11-transit-external.yml`, and the historical
`ntcp2-interop-*.yml` workflows.

### 5.6 Reference pins (do not change without a new plan)

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

### 5.7 Skills (`.opencode/skills/`, `.agents/skills/`)

Loadable agent bundles: `i2pr-local-dev` (local product path
before touching destination/garlic/LS2/Streaming/SAM/I2CP/tunnel
code), `i2pr-architecture` (this surface: ADRs, plans, specs,
doc-vs-source audits), `i2pr-planning` (registry/roadmap/closure
mechanics), `i2pr-ntcp2-interop` (historical harness),
`i2pr-rootless-sandbox` and `i2pr-multipass-recovery` (historical
sandbox lanes).

## 6. Capability snapshot

This section summarizes; the binding records are `plans/registry.md`
(newest `*-status.md` wins), `specs/support.toml`, and
`specs/CONFORMANCE.md`. No row below is a public-network or
anonymity claim.

| Area | Proven (bounded scope) | Not claimed / debt |
| --- | --- | --- |
| Destinations + garlic + LeaseSet2 + Streaming (M6 local) | Local product closed (Plan 134 authority; Plan 152 robustness corrective underneath); i2pd-family Streaming qualified (Plan 193). | `milestone6_interoperable = not-yet-claimed`; Java full-router compatibility retained/deferred at Plan 247; two-family conformance not claimed. |
| SAM 3.1 (M7 localhost) | Final localhost acceptance closed (Plan 151; self-composed product Plan 149; external-client core Plan 150 retained). | No router-to-router claim; loopback-only, disabled by default. |
| SAM 3.3 PRIMARY/subsessions | Plan 368 local profile; see `plans/registry.md` for current closure authority. | Negotiated range 3.1–3.3; loopback-only, disabled by default; no remote-router claim. |
| SSU2 v2 (M8 direct interop) | Closed within bounded direct-IPv4 loopback scope vs exact-pinned i2pd, both directions + cached-token/malformed rows (Plan 161; lane isolation Plan 162). | No public advertisement; PQ-hybrid deferred (`ssu2_pq_v3_v4 = deferred-compatibility-watch`); SSU1 not implemented; IPv6 interop is infrastructure-limited debt. |
| I2CP (M9 loopback) | Final acceptance closed, loopback-only (Plan 172; wire/data-plane Plan 170 retained; invalid-preamble hardening Plan 171). Independent Java/go clients proven on the loopback lane. | No remote-I2CP / public-network claim; no `HostLookup` resolution. |
| Service tunnels (M10) | Local generic/HTTP/SOCKS5/IRC product + round-trip closed (Plans 174–180, 182); router-backed generic A/B (Plan 213) and product-only HTTP/IRC application closure (Plan 214/215, hosted double-pass) proven against exact-pinned i2pd. | No product blocker; Plan 204 convergence gate superseded by Plan 248. Outproxy: Plan 343 landed the provider **policy and route owner** with `no reachable request path` — there is no working outproxy and no direct clearnet fallback; Plan 327 stays blocked. |
| M11 transit tunnels | One-family experimental progression passed (Plan 268) on exact-head CI. | Public transit remains disabled, non-advertised, and unclaimed. |
| M12 floodfill | Architecture, provenance, record validation/storage, bounded DatabaseStore/DatabaseLookup services, replication planning, versioned persistence, bounded maintenance, resource leases for the type 0/1/3/7 floor (Plans 270–276, 283). Encrypted LeaseSet2 type 5 is now REAL CODE, not a stub: Plan 330 passed an independent in-repo Red25519 implementation, Plan 331 passed qualification with a reference signature-transcript divergence recorded, and Plans 332/333 passed the ELS2 type-5 foundation plus PSK/DH client authorization. | No capability is advertised (`common.leaseset2-family` is `advertised = false`) and no live interoperability is claimed: ADR 0032 and Plan 346 close the *cryptographic* boundary in both directions (i2pr publishes the deployed Java/i2pd ELS2 transcript and accepts it plus the Proposal-146 strict one inside the bounded type-5 verifier), while the live end-to-end cross-router path remains Plan 347's unexecuted evidence. Daemon floodfill role lifecycle and qualification remain unimplemented. NOTE: `specs/support.toml` rows `m12_type11_red25519`, `m12_transit_tunnels`, and `m12_floodfill_status` still describe the Plan 280/281 stopped/deferred state and are superseded by the later closure records. |
| Proposal 170 / I2PControl | Control plane + publication complete, black-box evidence landed (`ready-control-plane-and-publication-complete-black-box-evidence-landed`); canonical continuation 319–328, Red25519/ELS2 successor 329–335. Plan 322 closed the canonical RouterInfo source census to zero (`passed-canonical-routerinfo-sources-with-the-transit-participation-posture-unchanged`, via Plans 339/340); Plan 334 reclosed `passed-mode-mapping-and-control-surface-complete` on 2026-10-05 (Plans 337/338). | Plan 327 remains blocked (no routed outproxy provider or secret owner); Plans 325/326 are historical blocked records. Plan 343 passed the outproxy provider policy and route owner but with `no reachable request path`, and Plan 341 landed the restart-safe outbound proxy secret owner. Plan 344 re-audited Plan 326 and closed one mode-coverage gap; Plan 326 remains blocked on the external type-11 transcript (`blocked-prop170-encrypted-leaseset-awaiting-external-type11-transcript`). No Encrypted LeaseSet2 capability is advertised, no capability advertisement, no live interoperability claim. NOTE: `plans/registry.md` still reports Plans 322 and 334 as blocked — the closure records are newer and win per the authority order. |
| Anonymity / implementation neutrality | Plans 307, 309, 311–312, 314–316, 318 passed; ADR 0026 / ADR 0030 define linkability domains and router unlinkability. | Plan 308 independently blocked; Plan 310 retained as blocked history; Plan 317 blocked. Does not gate M12/mainline. |
| NTCP2 | Runtime-owned composition exists; development result `protocol-defect-localized` at `noise_authenticated`. Daemon NTCP2 disabled by guard. | No production activation; no interop claim. |
| NetDB / tunnels over network | One-hop exploratory tunnels, NetDB lookup/publication, destination delivery proven vs i2pd on the M6 lane (Plans 184–192). | Full multi-hop as a production claim is not made here. |

Unsupported or deferred by design (fail-closed, never silently
bridged): legacy NTCP/SSU1, PQ session establishment (parser
tolerance only), clearnet outproxy, SOCKS UDP ASSOCIATE/BIND,
SOCKS4/auth, transparent proxying, HTTP/2+ termination, TLS
interception, IRC DCC/WEBIRC, transit/floodfill public
participation, in-process Rust plugins, `Arc<RouterContext>` service
locators.

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
   reseed (`ReseedIngestor`); floodfill records restore only after
   decode + revalidate, with provenance narrowed to replica.
5. `i2pr-netdb` serves `RouterInfoStore`,
   `CoalescedRouterInfoLookup`, `PublicationCoordinator`, and the
   Standard LeaseSet2 store. The daemon `NetDbSeam` exposes them;
   a registered inbound tunnel flips the seam to `Available` via
   the `ReplyPathProvider` backed by `i2pr-tunnel`.
6. `i2pr-tunnel` holds the exploratory pool (plus transit
   composition), short-build crypto, canonical I2NP bridge, data
   plane, and NetDB-over-tunnel composition. Outbound gateway roles
   emit `DatabaseLookup` / `DatabaseStore`; inbound endpoint roles
   dispatch `TunnelData`.
7. `i2pr-client` owns destination identity, pools, LeaseSet2
   lifecycle, ECIES sessions, garlic routing/dispatch, and
   Streaming. Outbound: lease selection → `compose_outbound_delivery`
   → Garlic I2NP bytes → tunnel data plane. Inbound: tunnel data →
   garlic envelope → session classify → Streaming dispatch.
8. `i2pr-runtime` validates the `ServiceGraph`, then supervises one
   manager per service (`lifecycle`, `netdb-bootstrap`, optional
   `ssu2-router`, optional loopback SAM/I2CP/I2PControl listeners,
   service tunnels). Each service gets a narrowed `ServiceContext`,
   never the supervisor itself.
9. Transports sit underneath: `i2pr-transport` link manager;
   `i2pr-transport-ntcp2` declared but unused in production;
   `i2pr-transport-ssu2` handshake/session machines driven by the
   runtime UDP service with path validation and peer-test/relay
   coordination.
10. `i2pr-proto` + `i2pr-crypto` stay at the bottom; `i2pr-core`
    provides budgets/health/cancellation everywhere;
    `i2pr-addressbook` and `i2pr-i2pcontrol` are leaf contract
    crates projected by the daemon into its listeners;
    `i2pr-testkit` exercises the same crates in tests via virtual
    time/links/faults (never in production).

Client ingress paths (all loopback-only, disabled by default):

- SAM TCP → `i2pr-api` SAM state → daemon `sam.rs` bridge →
  `i2pr-client` destination/Streaming product → tunnel data plane.
- I2CP TCP (`0x2a` preamble + frames) → `i2pr-api` I2CP machines →
  daemon `i2cp.rs` → client-owned destination runtime →
  same destination/Streaming product.
- I2PControl JSON-RPC TCP → `i2pr-i2pcontrol` contract → daemon
  `i2pcontrol.rs` / `i2pcontrol_tunnels.rs` → router state
  projections and typed tunnel requests.
- Service-tunnel TCP (generic/HTTP/SOCKS5/IRC-client) or Streaming
  accept (IRC-server) → `i2pr-service-tunnels` policy/parser →
  daemon per-profile executor → shared Streaming pump → same
  destination product. Remote routing goes through the typed
  `RemoteDestinationBackend` seams, never a test-owned shadow
  stack.
- Hostname lookups (SAM `NAMING LOOKUP`, service-tunnel targets,
  I2PControl address-book calls) → `i2pr-addressbook` resolver →
  destination or host reference, with subscription-derived entries
  consulted last.

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
- Doc-vs-source audits: [audit/](audit/)
- ADRs: [../adr/](../adr/)

Suggested review path for a new reader:

1. This overview (§1–§4) for the map.
2. [dependency-graph.md](dependency-graph.md) + `i2pr-core`,
   `i2pr-proto`, `i2pr-crypto`, `i2pr-su3` deep dives for the
   foundation.
3. `i2pr-netdb` → `i2pr-netdb-persist` → `i2pr-tunnel` for network
   state.
4. `i2pr-transport` → `i2pr-transport-ssu2` (`-ntcp2` historical)
   → `i2pr-runtime` for links and supervision.
5. `i2pr-client` → `i2pr-api` → `i2pr-daemon` for destinations and
   client ingress.
6. `i2pr-service-tunnels` → `i2pr-addressbook` → `i2pr-i2pcontrol`
   for the application and control layers.
7. [tooling.md](tooling.md) + the lane under review
   (`tests/integration/<area>/run-*.sh` + matching
   `scripts/check-*-evidence.sh`) for evidence semantics.

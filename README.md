# i2pr

`i2pr` is an experimental, long-term effort to build a clean, maintainable I2P router in Rust.

The project is intended to provide a CLI-first router with a modular architecture, a defense-in-depth security posture, strict protocol handling, and clear internal boundaries between wire protocols, routing policy, client APIs, and application-facing tunnel services.

The initial compatibility target is the current I2P network as implemented by I2P/I2P+, i2pd, and other interoperable routers. The internal design does not need to mirror the Java router or Emissary, but protocol behavior must remain wire-compatible unless an explicitly isolated research mode states otherwise.

## Project status

Milestone 0 workspace bootstrap and its corrective closure are implemented. The
repository contains a buildable seven-crate Rust workspace, strict
side-effect-free configuration validation, bounded common-structure and
initial-I2NP codecs, reviewed Ed25519/X25519 identity wrappers,
permission-hardened identity storage, a deterministic testkit foundation, and
a non-networked CLI shell. Plan 014 also adds an opt-in nightly fuzz workspace
and locally authored, hashed I2NP regression fixtures. Plan 015 adds
creation-time directory permissions, zeroizing transient identity/reply-secret
owners, grouped protocol namespaces, and fixture-backed positive/malformed
regressions. These remain structural and local evidence only.
Plans 011–013 provide the structural and local cryptographic foundation for
common I2P identities, mappings, certificates, RouterInfo, RouterAddress,
Lease, classic LeaseSet, explicit identity generation, atomic reload, local
RouterInfo signing, and the initial bounded I2NP message model. Cryptographic
interoperability, LeaseSet2-family records, transport integration, networking,
router behavior, and I2NP body state-machine semantics remain unimplemented.
Normal development and CI use pinned Rust 1.95.0; the declared Rust 1.85 MSRV
is checked by a dedicated Ubuntu CI job. Plan 021 now provides a concrete,
non-networked Tokio runtime with deterministic service supervision, wakeable
cancellation, readiness/health snapshots, bounded restart policy, and
graceful/forced shutdown. Live router behavior and network interoperability
remain unimplemented.
Plan 022 now adds bounded command, request, and event channels, latest-state
snapshots, typed overload outcomes, and runtime-neutral resource leases with
atomic bundles and bounded diagnostics. These are infrastructure contracts
only; no live transport, NetDB, tunnel, client, or listener behavior has been
introduced. Plan 023 now adds a bounded deterministic testkit with manually
wakeable monotonic time, domain-separated seeds, in-memory stream/datagram
links, executable fault scripts, ephemeral peer/topology factories, and
privacy-safe replay records. The testkit is a manual simulation pump only: it
opens no sockets, performs no DNS, persists no private identities, and does not
provide transport interoperability evidence.
Plan 024 now adds fixed-name, privacy-aware runtime events; redacted aggregate
supervisor/channel/resource snapshots; latest-state health correctness when no
subscriber is attached; integrated clean, overload, restart, essential-failure,
and stream/datagram fault scenarios; and a fixed 32-seed deterministic replay
matrix. These are bounded local validation artifacts, not protocol, anonymity,
resilience, or public-network evidence.
Plan 025 corrects forced child cleanup ownership, cancellation-aware service
completion classification, physical protocol module ownership, CI guardrails,
and resource-release underflow visibility. Its closure remains limited to
bounded local evidence and does not add router behavior or interoperability.
Plan 031 established the first Milestone 3 boundary: the runtime-neutral
`i2pr-transport` contracts, the Tokio-free `i2pr-transport-ntcp2` skeleton,
bounded link/delivery/resource vocabulary, and deterministic synthetic
transport evidence. Plan 032 added the non-I/O transcript foundation and Plan
033 now adds the bounded, runtime-neutral NTCP2 handshake codecs and consuming
state machines. Plan 034 now adds bounded authenticated data frames, strict
payload blocks, direction-specific frame owners, and deterministic partial-I/O
evidence. None of these plans add sockets, live addresses, mixed-router
interoperability, NetDB mutation, or capability advertisement; all transport
support remains non-advertised experimental work.
Plan 032 now adds the non-I/O NTCP2 cryptographic foundation: reviewed
X25519/AES/ChaCha20-Poly1305/HMAC/SipHash wrappers, a consuming three-message
transcript model, an independently generated deterministic crypto corpus, and
a separate hardened static-key/IV store. Plan 033 adds bounded codecs for all
three handshake messages, consuming initiator/responder transitions, replay and
clock-skew policy seams, RouterInfo/static-key binding, and explicit runtime-
neutral I/O actions. Plan 034 adds SipHash-masked lengths, AEAD frames, strict
authenticated blocks, and terminal counter/error handling. These remain local
experimental evidence only; sockets, mixed-router interoperability, NetDB
mutation, and capability advertisement remain unimplemented.
Plan 035 now adds a bounded runtime-owned TCP integration seam: strict NTCP2
address interpretation, pre-crypto admission, replay/backoff owners, controlled
loopback listener/dial services, and joined link children. The runtime socket
surface is disabled outside explicit controlled tests; no public listener,
automatic address publication, NetDB mutation, mixed-router interoperability, or
capability advertisement is claimed.
Plan 037 was the corrective integration plan. Its boundary keeps inbound
admission attached to the accepted stream through handshake completion or a
typed terminal outcome, applies configured cancellation/deadline policy to the
actual link I/O, and gives each queued frame one bounded ownership path for item
and byte accounting. It also separates strict SessionConfirmed parsing from
general data-phase block parsing. Plan 042 now supplies the complete bounded
authenticated socket/data-phase composition through the non-production
launcher; daemon activation and mixed-router evidence remain disabled.

Plan 042 defines the bounded NTCP2 wire driver owned by `i2pr-runtime` and
driven by the runtime-neutral handshake/data state machines. The runtime driver
owns socket I/O, action deadlines, cancellation, replay and admission
decisions, authenticated frame state, bounded queues, and link/task cleanup.
The non-production `i2pr-interop` launcher now validates confined scenario
input, prepares disposable identity/RouterInfo state, drives listener or dial
handshakes, promotes authenticated links, and exchanges a bounded
DeliveryStatus message; it does not activate `i2pr-daemon`.

Plan 038/040 define an Ubuntu-only, opt-in reference-router harness for
acquiring the missing evidence under controlled conditions. Plan 041 adds the
dedicated Java I2P/i2pd reference-pair crosscheck. It is a harness contract,
not a production bootstrap path or an interoperability result. The supported
host contract is Ubuntu amd64 with `apt`, Bash 4+, Python 3, Linux network
namespaces, `iproute2`, and `sudo`. Preparation may install declared packages,
fetch only the pinned Java I2P 2.12.0 and i2pd 2.60.0 sources, build disposable
reference artifacts, and record hashes. Execution is a separate network-
isolated phase: it creates disposable namespaces joined only by a veth pair,
rejects default routes, DNS, and public egress, generates temporary state, and
cleans up before reporting a result. The execution phase must not download,
reseed, bootstrap, publish RouterInfo, mutate NetDB, or start the normal daemon.

The command surface is:

```text
bash scripts/interop/ubuntu/check-host.sh --pre-install
bash scripts/interop/ubuntu/setup-host.sh
bash scripts/interop/ubuntu/check-host.sh --post-install
bash scripts/interop/build-references.sh
bash scripts/interop/build-references.sh --offline
bash scripts/interop/run-scenario.sh --scenario <id> --reference java_i2p --build-cache <path> --run-root <path>
bash scripts/interop/run-scenario.sh --scenario <id> --reference i2pd --build-cache <path> --run-root <path>
bash scripts/interop/run-matrix.sh --profile environment-smoke
i2pr-interop ntcp2 listen --scenario-config <path>
i2pr-interop ntcp2 dial --scenario-config <path>
i2pr-interop ntcp2 inspect --state-dir <path>
```

The launcher status boundary is explicit. A completed `listen` path emits
listener readiness separately from a later authenticated terminal result;
`dial` emits one terminal typed result; and `inspect` emits only bounded,
redacted state metadata. Readiness is not authentication. State, handshake,
data-phase, timeout, and cleanup failures remain typed rejections; no launcher
status is mixed-router evidence.

Environment smoke proves only that each reference can start, produce
disposable state, avoid public connections, and stop cleanly. The
`reference-crosscheck-ipv4` profile runs both directional reference-pair
scenarios with the separately owned topology, explicit private network ID 99,
strict RouterInfo validation/import, and dual authenticated-link observations.
It remains reference-control evidence, not an i2pr run. A missing host/cache,
strict parser, or authoritative observation is a typed blocker. Neither
profile is i2pr evidence. The
i2pr mixed-router profile requires bounded authenticated runs in both
directions against each reference; the full manifest and its adversarial
profiles remain gated on positive i2pr handshake/data smoke in both directions.
Retained evidence is written only under `target/interop/evidence/` and is
limited to typed outcomes, run metadata, and hashes of sanitized artifacts.
Secret-bearing run roots under `target/interop/runs/<run-id>/` are deleted.
Raw addresses, peer identities, RouterInfo, I2NP, keys, transcripts, logs, and
remote error text are disposable and must not be committed.

Plan 042 selects the existing fixed-size DeliveryStatus message (I2NP type 10)
as the initial smoke scope. Its body is 12 bytes; the NTCP2/SSU2 short I2NP
encoding is 21 bytes before the 3-byte NTCP2 block header, frame overhead, and
padding. The launcher’s local gate is one valid outbound and one valid inbound
DeliveryStatus, with bounded message IDs/timestamps and no
NetDB, tunnel, garlic, or public-routing behavior. Reference acceptance and
response behavior have not been verified here, so this selection is a Plan 042
scope decision, not interoperability evidence; padding or TCP readiness cannot
stand in for the message exchange.

No production-ready router functionality exists yet. Do not use `i2pr` for anonymity, privacy, censorship resistance, or security-sensitive workloads until the project has completed protocol interoperability, adversarial testing, and an independent security review.

### Plan 043 build-system status

The Ubuntu build-system lane has an explicit ordered promotion contract:

```text
contract -> reference-build -> reference-offline-reuse -> environment-smoke
-> reference-crosscheck-ipv4 -> i2pr-handshake-smoke-ipv4 -> full-matrix
-> evidence-validation -> cleanup-verification
```

Preparation is the only network-enabled trust domain. Execution is offline and
uses only verified reference caches plus disposable namespace-local veth links.
The exact host is Ubuntu 24.04 amd64/x86_64 with the lock-listed package set,
namespace/nftables capability, UTF-8 locale, non-interactive sudo when needed,
and at least 4 GiB free under `target/`. Cache reuse binds the canonical
reference, full source revision, lock digest, host contract, build-command
version, and relevant tool/ABI metadata; a miss never permits a fetch.

Environment smoke and the Java-I2P/i2pd `reference-crosscheck-ipv4` profile are
harness controls only. The reference control must pass before the four
independent i2pr/reference IPv4 directions are eligible. A positive i2pr gate
requires authenticated handshake, strict binding, bounded DeliveryStatus
exchange in each direction, sanitized evidence, and clean state. The full
matrix adds bounded adversarial and resource cases; it does not run unbounded
fuzzing.

The evidence gate accepts only an aggregate manifest and sanitized typed JSON
with approved hashes. Cleanup runs unconditionally, and an independent
clean-host verifier must reject residual namespaces, veths, processes,
secret-bearing run roots, forbidden retained files, and attributable host
firewall or route changes. A cleanup failure overrides protocol success.

Promotion is manual first, scheduled only after repeated clean-checkout and
cache-reuse success, then a current successful run at Milestone 3 closure. The
workflow and helper apparatus now expose the ordered manual Plan 043 lane,
including clean-host verification and aggregate validation. No completed
successful aggregate run or mixed-router i2pr evidence is present in this
checkout; these are blockers, not skipped successes. NTCP2 remains experimental
and non-advertised.

### Plan 045 mixed-router integration status

Plan 045 supersedes Plan 044 as the plan of record. Plan 044's
"implementation-complete locally" status is amended: ten Plan 045
defects (D1–D10) invalidated the prior claim. Plan 045 closes those
defects as a structured corrective pass.

- D1: the ``-gen`` and live reference adapters share one disposable
  ``reference-data`` directory so the live reference restarts from the
  identity that produced the exported RouterInfo; the i2pr side shares
  the same ``state`` directory across the ``-gen`` and live phases.
- D2: the Rust launcher persists RouterInfo inside the scenario's
  ``state_dir``; the mixed-runner exports it from there to the
  ``exchange`` directory and records a real SHA-256 digest in the
  evidence record. The reference RouterInfo digest is recorded too.
- D3, D6: the strict launcher scenario schema now allows an explicit
  allowlist of optional fields (``data_phase_mode``,
  ``data_phase_required_peer_action``, ``data_phase_timeout_ms``,
  ``expected_observation``) and supports the
  ``fixed-12-byte-payload`` smoke profile alongside
  ``delivery-status``. The Rust launcher parses the same schema.
- D4: the reference trigger performs the per-direction SAM v3 (Java)
  or HTTP JSON-RPC (i2pd) dial inside the disposable namespace.
- D5: the data-phase oracle records per-side observation code keyed
  by the i2pr launcher's authenticated-frame counters; no echo
  assumption is made.
- D6 (Rust): the launcher dispatches ``DataPhaseMode::HandshakeOnly``,
  ``InitiatorDataOnly``, ``ResponderDataOnly``, and the prior
  ``RoundTripDeliveryStatus`` mode with distinct typed terminal
  reasons. Initiator and responder scenarios can complete without
  requiring the peer to echo a ``DeliveryStatus``.
- D7: the mixed-runner requires the i2pr terminal result to be
  ``passed``, the reference observation to be ``authenticated``, and
  the data-phase oracle's per-side observation to be ``observed``
  before marking a direction ``passed``. The prior pass-after-handshake
  predicate is removed.
- D8: the sanitized evidence record now carries
  ``i2pr_router_info_sha256``, ``reference_router_info_sha256``,
  ``data_phase_mode``, and ``expected_observation`` typed fields
  populated by the runner.
- D9: ``run-matrix.sh`` continues to route the four directional mixed
  scenario IDs through ``mixed_runner.py``. The Plan 045 typed
  blocker for "i2pr-mixed-router-profile-not-wired" remains reserved
  for scenario IDs that are not allowlisted for the active gate.
- D10: an unknown reference kind now fails closed with a typed
  ``unknown-reference-kind`` rejection; it does not silently fall
  through to the i2pd adapter.

The Plan 044 closure document (`plans/044-closure.md`) is amended to
record the Plan 045 supersession. No completed mixed-router i2pr
record is present in this checkout; these remain typed blockers. NTCP2
remains experimental and non-advertised.

### Plan 046 rootless sealed-namespace evidence lane

Plan 046 replaces the host-global namespace requirement for the primary
NTCP2 interoperability evidence path with a **rootless, process-scoped
sandbox**. The primary evidence topology is now:

```text
rootless-sealed-single-netns
```

with privilege model `unprivileged-userns`. It is runnable by an ordinary
user without `sudo`, passwordless elevation, host capabilities, setuid
helpers, host-visible named network namespaces, host-visible veth
devices, or host nftables mutation. The legacy
`privileged-dual-netns-veth` topology is preserved as an explicit
optional qualification lane; it is never the default and is never a
silent fallback.

The rootless lane proves protocol compatibility. It does not claim
separate-stack network behavior, asymmetric firewall semantics, packet
loss, route mutation, or interface-failure semantics. The retained claim
is intentionally narrow:

> The pinned i2pr and reference-router processes completed the declared
> NTCP2 direction inside a process-scoped, rootless user/network
> namespace whose canonical isolation checks passed and whose creation
> and teardown did not alter the parent host's canonical network state.

A passing rootless run requires a sanitized sandbox attestation and an
unchanged parent-host network digest. The mixed-router evidence schema
now carries `topology_kind`, `privilege_model`,
`sandbox_attestation_sha256`, and `parent_network_state_unchanged`. A
passed record that fails any of these checks is rejected. NTCP2 remains
experimental and non-advertised; Milestone 3 is still open.

The new rootless entrypoint, probe, supervisor, and topology modules
live under:

- `scripts/interop/rootless-enter.sh` — outer entrypoint; the only path
  that creates the sandbox.
- `scripts/interop/probe-rootless-sandbox.sh` — bounded create /
  configure / connect / teardown probe with strict typed outcomes.
- `tests/integration/ntcp2/harness/rootless_supervisor.py` — inner
  namespace verification.
- `tests/integration/ntcp2/harness/rootless_topology.py` — sealed
  in-sandbox topology backend.
- `tests/integration/ntcp2/harness/rootless_inner_runner.py` — inner
  scenario dispatch.
- `tests/integration/ntcp2/harness/interop_topology.py` — backend
  contract (`ProcessPlacement`, `InteropTopology`, topology registry).
- `docs/adr/0017-rootless-sealed-namespace-interop-evidence.md` —
  architectural decision record.
- `scripts/check-rootless-interop-boundary.sh` — static boundary
  checker (no `sudo`, no host network mutation, no fallback).
- `.github/workflows/ntcp2-interop-rootless.yml` — manual,
  no-escalation workflow.

The Plan 046 status file (`plans/046-status.md`) records the
implementation-completion stage; the closure record is
`plans/046-closure.md`. Plan 046 closed with a typed host-level blocker
on this checkout (`blocked_unprivileged_user_namespace`); the on-host
evidence is at
`target/interop/evidence/handshake-smoke-rootless--host-blocked/`.
Cross-host recovery is recorded in
`plans/047-cross-host-rootless-lane-expansion.md`.

## MVP direction

The feature MVP is expected to include:

- A foreground, CLI-operated router daemon.
- Persistent router identity and validated configuration.
- I2NP message handling and core router dispatch.
- NTCP2 and SSU2 transport support.
- NetDB client behavior and floodfill participation.
- Inbound, outbound, exploratory, and transit network tunnels.
- Destination and LeaseSet management.
- I2P streaming.
- SAM and I2CP client interfaces.
- HTTP and SOCKS5 client proxies.
- Generic TCP client and server tunnels.
- IRC client and IRC server tunnel profiles.
- Bounded resource accounting, graceful shutdown, health reporting, and operational metrics.

This is a substantial scope. Development will first target a smaller interoperable-router milestone before closing the complete feature MVP.

## Architectural principles

### Wire compatibility and policy separation

Protocol codecs, cryptographic state machines, and negotiated capabilities must remain separate from router policy. Peer selection, transit participation, resource allocation, tunnel quantities, and floodfill eligibility may vary by profile without changing wire behavior.

### Modular monolith

The initial router will be one process composed from focused Rust crates. Crate boundaries should follow security boundaries, protocol churn, and ownership—not arbitrary source-file size. The project will not begin as a distributed collection of services or as a runtime plugin platform.

### Explicit trust boundaries

All network, persisted network, client API, configuration, and local service inputs are untrusted until validated. Subsystems receive only the capabilities they require. A global mutable router context or unrestricted service locator is not an acceptable default.

### Bounded execution

Queues, buffers, handshakes, sessions, tunnel builds, NetDB operations, destinations, streams, and API clients must have explicit limits. Peer-controlled work must have deadlines, cancellation paths, and cleanup semantics.

### Defensive Rust

Safe Rust is the default. Protocol and routing crates should forbid unsafe code. Cryptographic primitives should come from reviewed implementations rather than being created locally. Secret-bearing types must avoid accidental logging, cloning, serialization, or long-lived retention.

### Small dependency surface

Prefer the standard library and focused pure-Rust crates where that produces a maintainable and auditable implementation. Dependency minimization must not justify implementing cryptography, parsers, compression, or other high-risk primitives without adequate expertise and review.

### Testability by design

Protocol state machines should support deterministic clocks, seeded randomness, in-memory transports, fault injection, and reproducible simulation. The test harness is a core project component, not a late-stage accessory.

## Intended workspace shape

The exact workspace will evolve, but the initial direction is:

```text
crates/
  i2pr-proto/               Wire types, codecs, constants, validation
  i2pr-crypto/              Protocol-specific cryptographic wrappers
  i2pr-core/                Shared contracts, lifecycle, budgets, health
  i2pr-transport/           Transport-neutral link management and selection
  i2pr-transport-ntcp2/     NTCP2 implementation
  i2pr-transport-ssu2/      SSU2 implementation
  i2pr-netdb/               RouterInfo/LeaseSet storage, lookup, publication
  i2pr-tunnel/              Network tunnel construction and participation
  i2pr-client/              Destinations, LeaseSets, garlic, streaming
  i2pr-api/                 SAM and I2CP adapters
  i2pr-service-tunnels/     HTTP, SOCKS5, IRC, generic TCP forwarding
  i2pr-storage/             Atomic persistence and migration support
  i2pr-runtime/             Tokio-backed supervision and cancellation
  i2pr-daemon/              CLI, configuration, composition, supervision
  i2pr-testkit/             Deterministic simulation and adversarial fixtures
```

The current workspace contains `i2pr-proto`, `i2pr-crypto`, `i2pr-storage`,
`i2pr-core`, `i2pr-transport`, `i2pr-transport-ntcp2`, `i2pr-runtime`,
`i2pr-daemon`, and `i2pr-testkit`. The runtime crate is the only production
crate that owns Tokio tasks, timers, channels, sockets, or wakeable cancellation;
transport crates expose pure contracts and protocol seams only. Plan 035's
listener, dialer, replay owner, and per-link reader/writer children all remain
inside that runtime boundary. Later plans
will add protocol and service crates when their contracts are understood;
empty placeholder crates are not created in advance.

Plan 036 adds the controlled interoperability and adversarial-validation
evidence boundary under `tests/integration/ntcp2/`. Its preflight is manual and
fail-closed: it requires disposable identities, a synthetic private network,
disabled reseed/bootstrap, pinned Java I2P/i2pd artifacts, and sanitized
typed-result records. The current checkout keeps live activation disabled and
does not claim mixed-router interoperability until a complete wire-level
runtime adapter and authorized runs in both directions are available.

Plan 037 corrects the local integration defects found during that review:
inbound admission now travels with the accepted stream, link queue entries
release their accounting through RAII, and supervised reader/writer I/O uses
configured cancellation and deadline bounds. General data-phase block parsing
also separates its deployed-wire ordering rules from strict SessionConfirmed
payload parsing. Plan 042 now supplies the bounded socket-to-state-machine/data-
phase composition through the non-production launcher. Plan 044 composes the
mixed-router execution model with the four directional i2pr/reference scenarios,
the strict launcher renderer, the non-echo data-phase oracle, and the mixed
evidence schema. Java I2P/i2pd mixed-router evidence remains pending
execution, so Milestone 3 and all NTCP2 support rows remain blocked,
experimental, and non-advertised.

The current `i2pr-proto` API uses borrowed cursors and caller-visible maximums,
strict exact-consumption decoding, canonical immutable mappings, typed
algorithm/length validation, preserved signed-byte regions, and a bounded I2NP
registry with standard and short header codecs. I2NP bodies that need later
cryptography or state machines are named `Deferred`/`Opaque` values rather
than support claims. The separate
`i2pr-crypto` crate implements only type-7 Ed25519 signing/verification,
type-4 X25519 public-key derivation, SHA-256 wrappers, constant-time equality,
and zeroizing private wrappers. `i2pr-storage` implements the version-1
permission-hardened private identity file. None of these crates introduce
transport behavior, runtime integration, network publication, or capability
advertisement.

## External integration direction

Future integration with `synvoid` should occur at the service boundary, normally by forwarding an I2P server destination to a local Unix socket or loopback service. `synvoid` should not become part of the routing core.

Future integration with `eggsec` should use stable testkit, fault-injection, and private-testnet interfaces. Adversarial tests must be constrained to systems and networks where authorization is explicit.

## Documentation

- [Project guardrails](GUARDRAILS.md)
- [MVP roadmap](plans/000-mvp-roadmap.md)
- [Workspace and skeleton pre-plan](plans/001-preplan-workspace-skeleton.md)
- [Milestone 0 closure record](plans/001-closure.md)
- [Milestone 1 common-structures closure record](plans/012-closure.md)
- [Milestone 1 identity/crypto/storage closure record](plans/013-closure.md)
- [Milestone 1 I2NP/evidence/fuzzing closure record](plans/014-closure.md)
- [Aggregate Milestone 1 corrective closure record](plans/010-milestone-1-closure.md)
- [Plan 021 supervision and cancellation closure record](plans/021-closure.md)
- [Plan 022 bounded channels and resource governor closure record](plans/022-closure.md)
- [Plan 023 deterministic network testkit closure record](plans/023-closure.md)
- [Aggregate Milestone 2 closure record](plans/020-milestone-2-closure.md)
- [Plan 024 observability and validation plan](plans/024-m2-observability-validation-closure.md)
- [Plan 025 targeted corrective closure](plans/025-closure.md)
- [Plan 031 transport contracts and crate boundaries](plans/031-m3-transport-contracts-and-crate-boundaries.md)
- [Plan 031 closure record](plans/031-closure.md)
- [Plan 032 NTCP2 crypto/transcript plan](plans/032-m3-ntcp2-crypto-transcript-and-vectors.md)
- [Plan 032 closure record](plans/032-closure.md)
- [Plan 033 NTCP2 handshake state machines](plans/033-m3-ntcp2-handshake-state-machines.md)
- [Plan 033 closure record](plans/033-closure.md)
- [Plan 034 NTCP2 data phase and blocks](plans/034-m3-ntcp2-data-phase-and-blocks.md)
- [Plan 034 closure record](plans/034-closure.md)
- [Plan 035 runtime link manager and addresses](plans/035-m3-runtime-link-manager-and-addresses.md)
- [Plan 035 closure record](plans/035-closure.md)
- [Plan 036 interoperability and adversarial validation](plans/036-m3-interoperability-adversarial-validation-closure.md)
- [Plan 036 closure record](plans/036-closure.md)
- [Plan 037 corrective integration and closure](plans/037-m3-corrective-integration-closure.md)
- [Plan 037 closure record](plans/037-closure.md)
- [Plan 038 Ubuntu reference-router interoperability harness](plans/038-ubuntu-reference-router-interoperability-harness.md)
- [Plan 042 runtime-owned NTCP2 wire driver](plans/042-runtime-owned-ntcp2-wire-driver.md)
- [Plan 042 current status](plans/042-status.md)
- [Aggregate Milestone 3 closure record](plans/030-milestone-3-closure.md)
- [Controlled NTCP2 interoperability lane](tests/integration/ntcp2/README.md)
- [Machine-readable protocol support ledger](specs/support.toml)
- [Architecture](docs/architecture.md)
- [Protocol support matrix](docs/protocol-support.md)
- [Security model](docs/security-model.md)
- [Controlled private-testnet boundary](docs/private-testnet.md)
- [Architecture decision records](docs/adr/0000-adr-process.md)
- [Runtime and supervision ADR](docs/adr/0008-runtime-supervision-and-cancellation.md)
- [Runtime observability and validation ADR](docs/adr/0009-runtime-observability-and-validation.md)
- [Transport contracts and crate boundaries ADR](docs/adr/0010-transport-contracts-and-crate-boundaries.md)
- [NTCP2 crypto and static-key storage ADR](docs/adr/0011-ntcp2-crypto-and-static-key-storage.md)
- [NTCP2 handshake state-machines ADR](docs/adr/0012-ntcp2-handshake-state-machines.md)
- [NTCP2 data-phase and blocks ADR](docs/adr/0013-ntcp2-data-phase-and-blocks.md)
- [NTCP2 runtime link manager and address policy ADR](docs/adr/0014-ntcp2-runtime-link-manager-and-address-policy.md)
- [Contribution guide](CONTRIBUTING.md)
- [Protocol specification index and source ledger](specs/README.md)

## Development expectations

Before implementation work begins, read `GUARDRAILS.md`, the relevant plan in
`plans/`, the applicable ADRs, and the applicable protocol dossier and
conformance policy under `specs/`. Each implementation phase should define
acceptance criteria, tests, non-goals, dependency changes, security
implications, source revisions, and documentation updates.

The local quality baseline is:

```text
cargo fmt --all --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
cargo deny check advisories bans sources
```

The optional nightly-only fuzz lane is maintained separately from the
production workspace. See `fuzz/README.md` and run
`bash scripts/fuzz-smoke.sh` for bounded local smoke tests.

Runtime changes must use deterministic Tokio test time (`start_paused` or
explicit `time::advance`) rather than wall-clock sleeps. Every spawned task
must be owned by the supervisor or a service child scope and must be joined or
explicitly aborted before the runtime returns.

Transport changes must keep `i2pr-transport` runtime-neutral and keep
`i2pr-transport-ntcp2` free of Tokio, filesystem, sockets, and live protocol
side effects. Plans 035 and 037 keep every TCP listener/stream, async deadline,
replay-cache owner, admission counter, queued-frame owner, and reader/writer
child inside `i2pr-runtime`; controlled sockets remain disabled-by-default test
infrastructure. Plan 037 requires the pending admission owner to survive the
handshake handoff, cancellation to win I/O races, and queue accounting to drop
exactly once on success, failure, cancellation, or teardown.
Plans 032–033 additionally keep cryptographic and handshake
state consuming and secret-safe, persist transport static key/IV material only
through the versioned storage boundary, and require the hashed fixture
validator. Drive
state through bounded explicit actions and outcomes; use
owned encoded-I2NP handoffs and redacted snapshots rather than raw payloads,
addresses, keys, or runtime channels. Plan 031's focused local checks are:

```text
cargo test -p i2pr-transport --all-targets
cargo test -p i2pr-transport-ntcp2 --all-targets
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-vectors.sh
```

Plan 033 also requires the NTCP2 handshake codec/state tests and the separate
nightly fuzz workspace. Plans 036–037 add the fixed-seed 0..255 integrated
testkit matrix, parser-boundary regressions, and the sanitized interoperability
preflight. These tests are deterministic and local; they are not mixed-router
or public-network evidence.

The Plan 024 integrated validation lane is `cargo test -p i2pr-testkit
--all-targets`; it runs the five named scenarios and the fixed 32-seed replay
matrix. Run `rtk bash scripts/check-runtime-boundaries.sh` for the mechanical
runtime/testkit guardrails. Runtime snapshots and tracing events may contain
only validated service/channel identifiers, typed categories, counters,
bounded monotonic timing, and synthetic simulation metadata; health detail
text is redacted from default `Debug` output and aggregate snapshots.

The CLI exposes `--help`, `--version`,
`check-config --config <path>`, `identity generate --config <path>`,
`identity inspect --config <path>`, and `run --config <path> --dry-run`. Identity
generation is explicit, create-only, and permission-hardened. Inspection
loads and validates the file without displaying private material. Config
validation and dry-run do not create directories or identity files. A live
`run` deliberately remains non-networked and exits with code 20 until a later
daemon-composition plan wires the runtime into a live router. No command opens
a socket, publishes RouterInfo, or writes network state.

The project should favor incremental, reviewable changes. A protocol feature
is not complete merely because it compiles or communicates with one peer.
Completion requires negative tests, malformed-input handling, lifecycle
cleanup, bounded resource behavior, fuzz coverage, fixture provenance, and
interoperability evidence.

## License

A project license has not yet been selected. Do not copy implementation code from I2P+, i2pd, Emissary, or another router into this repository until license compatibility and provenance have been reviewed. Specifications and observed interoperability behavior may be used for clean-room implementation, subject to their applicable terms.

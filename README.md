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

No production-ready router functionality exists yet. Do not use `i2pr` for anonymity, privacy, censorship resistance, or security-sensitive workloads until the project has completed protocol interoperability, adversarial testing, and an independent security review.

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
`i2pr-core`, `i2pr-runtime`, `i2pr-daemon`, and `i2pr-testkit`. The runtime
crate is the only production crate that owns Tokio tasks, timers, or wakeable
cancellation. Later plans will add protocol and service crates when their
contracts are understood; empty placeholder crates are not created in advance.

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
- [Machine-readable protocol support ledger](specs/support.toml)
- [Architecture](docs/architecture.md)
- [Protocol support matrix](docs/protocol-support.md)
- [Security model](docs/security-model.md)
- [Architecture decision records](docs/adr/0000-adr-process.md)
- [Runtime and supervision ADR](docs/adr/0008-runtime-supervision-and-cancellation.md)
- [Runtime observability and validation ADR](docs/adr/0009-runtime-observability-and-validation.md)
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

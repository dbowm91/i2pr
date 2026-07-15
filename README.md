# i2pr

`i2pr` is an experimental, long-term effort to build a clean, maintainable I2P router in Rust.

The project is intended to provide a CLI-first router with a modular architecture, a defense-in-depth security posture, strict protocol handling, and clear internal boundaries between wire protocols, routing policy, client APIs, and application-facing tunnel services.

The initial compatibility target is the current I2P network as implemented by I2P/I2P+, i2pd, and other interoperable routers. The internal design does not need to mirror the Java router or Emissary, but protocol behavior must remain wire-compatible unless an explicitly isolated research mode states otherwise.

## Project status

Milestone 0 workspace bootstrap and its corrective closure are implemented. The
repository contains a buildable four-crate Rust workspace, strict
side-effect-free configuration validation, a deterministic testkit foundation,
and a non-networked CLI shell. Normal development and CI use pinned Rust
1.95.0; the declared Rust 1.85 MSRV is checked by a dedicated Ubuntu CI job.
The router runtime and all I2P protocol implementations remain unimplemented.

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
  i2pr-daemon/              CLI, configuration, composition, supervision
  i2pr-testkit/             Deterministic simulation and adversarial fixtures
```

The bootstrap intentionally creates only `i2pr-proto`, `i2pr-core`,
`i2pr-daemon`, and `i2pr-testkit`. Later plans will add protocol and service
crates when their contracts are understood; empty placeholder crates are not
created in advance.

## External integration direction

Future integration with `synvoid` should occur at the service boundary, normally by forwarding an I2P server destination to a local Unix socket or loopback service. `synvoid` should not become part of the routing core.

Future integration with `eggsec` should use stable testkit, fault-injection, and private-testnet interfaces. Adversarial tests must be constrained to systems and networks where authorization is explicit.

## Documentation

- [Project guardrails](GUARDRAILS.md)
- [MVP roadmap](plans/000-mvp-roadmap.md)
- [Workspace and skeleton pre-plan](plans/001-preplan-workspace-skeleton.md)
- [Milestone 0 closure record](plans/001-closure.md)
- [Machine-readable protocol support ledger](specs/support.toml)
- [Architecture](docs/architecture.md)
- [Protocol support matrix](docs/protocol-support.md)
- [Security model](docs/security-model.md)
- [Architecture decision records](docs/adr/0000-adr-process.md)
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

The CLI currently exposes only `--help`, `--version`,
`check-config --config <path>`, and `run --config <path> --dry-run`. A live
`run` deliberately exits with code 20 and explains that the router runtime is
not implemented. No bootstrap command opens a socket, creates a router
identity, creates a data directory, or writes network state.

The project should favor incremental, reviewable changes. A protocol feature is not complete merely because it compiles or communicates with one peer. Completion requires negative tests, malformed-input handling, lifecycle cleanup, bounded resource behavior, and interoperability evidence.

## License

A project license has not yet been selected. Do not copy implementation code from I2P+, i2pd, Emissary, or another router into this repository until license compatibility and provenance have been reviewed. Specifications and observed interoperability behavior may be used for clean-room implementation, subject to their applicable terms.

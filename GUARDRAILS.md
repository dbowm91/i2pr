# i2pr project guardrails

This document defines non-negotiable engineering, security, interoperability, and collaboration constraints for `i2pr`.

These guardrails apply to human contributors, coding agents, research agents, and any automated implementation workflow. A plan or implementation that conflicts with this document must explicitly identify the conflict and obtain project-owner approval before proceeding.

## 1. Scope and project posture

`i2pr` is an experimental Rust implementation of an I2P router. It is not production-ready until the project explicitly declares otherwise after interoperability, resilience, adversarial testing, and independent review.

The project may differ from Java I2P, I2P+, i2pd, and Emissary in internal architecture, policy, deployment shape, defaults, and operational tooling. It must not silently diverge from current I2P wire protocols.

Research behavior that intentionally diverges from normal network behavior must be isolated behind an explicit research profile, disabled by default, clearly documented, and tested so that it cannot accidentally activate in normal operation.

## 2. Protocol compatibility

Protocol work must be based on current specifications, proposal documents, interoperable behavior, test vectors, and clean-room analysis.

Every protocol implementation must document:

- The specification and proposal versions targeted.
- Required and optional fields.
- Size, count, time, and nesting limits.
- Reserved and unknown-field behavior.
- State-machine transitions.
- Failure and rejection behavior.
- Interoperability assumptions.
- Known implementation differences across I2P/I2P+, i2pd, and Emissary.

Do not encode router policy into wire codecs. Codecs may validate structural and protocol-level semantic constraints, but peer scoring, tunnel policy, resource allocation, role selection, and operational preferences belong in separate layers.

Legacy NTCP and SSU1 are not initial implementation targets. The MVP targets NTCP2 and SSU2. Legacy parsing or interoperability work requires a separate plan and explicit justification.

## 3. Architecture boundaries

The project is a modular monolith. One daemon process may contain multiple focused crates and supervised asynchronous services.

The intended dependency direction is broadly:

```text
proto <- crypto
proto/core <- transports, netdb, tunnel, client
client <- api, service-tunnels
all runtime services <- daemon composition root
shared deterministic fixtures <- testkit
```

The exact graph may be refined, but the following constraints apply:

- Protocol crates do not depend on the daemon, CLI, TOML configuration, filesystem implementations, or service tunnels.
- Transport implementations do not directly mutate NetDB state or publish RouterInfo.
- NetDB does not depend on SAM, I2CP, HTTP, SOCKS5, IRC, or synvoid.
- Application tunnel services do not import transport internals, tunnel-build internals, or peer-profile storage.
- SAM and I2CP are adapters over shared destination and streaming services, not alternate router cores.
- Floodfill is a NetDB role, not a separate router implementation.
- Synvoid and eggsec remain outside the production routing core.

Avoid global mutable state and unrestricted `Arc<RouterContext>` service locators. Each subsystem should receive narrow handles or capabilities for the operations it is permitted to perform.

Do not introduce runtime-loadable in-process Rust plugins during the MVP. Rust does not provide a stable ABI suitable for a security-sensitive third-party plugin ecosystem. Compile-time components or authenticated out-of-process interfaces are preferred.

## 4. Defensive programming

All external input is untrusted, including:

- Router-to-router network traffic.
- SAM and I2CP clients.
- HTTP, SOCKS5, IRC, and generic tunnel clients.
- Configuration files and command-line arguments.
- RouterInfo and LeaseSet data loaded from disk.
- Reseed material.
- Local control interfaces.
- Metrics and administrative requests.

Required properties:

- No peer-controlled panic paths.
- No unchecked length conversion or arithmetic overflow.
- No unbounded allocation based on peer-controlled fields.
- No unbounded queues or maps.
- No detached long-lived tasks without ownership and cancellation.
- No silent truncation of protocol values.
- No permissive parsing where canonical encoding is required.
- No sensitive values in default logs, errors, or metrics labels.

Every parser must have explicit input limits and complete-consumption behavior. Every asynchronous request path must have a deadline, cancellation path, and cleanup behavior. Every resource-owning subsystem must define shutdown semantics.

## 5. Resource governance

Resource control is a cross-cutting security property.

The router must eventually enforce global and scoped budgets for at least:

- Pending cryptographic handshakes.
- Active NTCP2 and SSU2 sessions.
- Incomplete transport frames and fragments.
- Inbound and outbound queued bytes.
- I2NP messages awaiting dispatch.
- NetDB lookups and publication attempts.
- RouterInfo and LeaseSet storage.
- Tunnel build operations.
- Transit tunnels and tunnel bandwidth.
- Destinations and tunnel pools.
- Streaming sessions and buffered data.
- SAM and I2CP clients.
- Application tunnel listeners and connections.

Subsystem-local limits do not replace router-wide accounting. Reject, defer, back off, or shed low-priority work when budgets are exhausted. Do not allow memory growth to become the implicit backpressure mechanism.

## 6. Rust and dependency policy

Use safe Rust by default.

Protocol, crypto-wrapper, routing, NetDB, tunnel, client, API, and service crates should use `#![forbid(unsafe_code)]` unless a narrowly scoped exception is approved. Any required unsafe implementation must be isolated, documented, tested, and reviewed separately.

Prefer focused, maintained, pure-Rust dependencies where practical. Dependency minimization is a means to auditability and maintainability, not an absolute objective.

Do not implement cryptographic primitives locally. Use reviewed crates and expose protocol-specific wrapper types.

Dependency additions must record:

- Purpose and alternatives considered.
- Maintainer and release health.
- Transitive dependency impact.
- License compatibility.
- Unsafe code exposure.
- Feature flags enabled.
- Whether the dependency processes untrusted input.

Library crates should avoid `anyhow` as a public error model. Use typed error enums with stable categories. `anyhow` may be used at the CLI/composition boundary where appropriate.

Avoid broad default features. Workspace dependencies should be centralized when the workspace exists, and duplicate major versions should be reviewed.

## 7. Cryptographic material and identity

Secret-bearing types must not derive or implement `Debug`, `Display`, or unrestricted serialization.

Secrets should not be `Clone` unless protocol behavior genuinely requires duplication. Secret memory should be zeroized where the underlying type supports it.

Router identity creation, loading, rotation, backup, and deletion must be explicit operations. Configuration reload must never rotate router identity accidentally.

Nonce, IV, replay-window, and key-lifecycle invariants must be encoded into state machines and tested. Randomness used for production cryptography must come from an operating-system-backed CSPRNG through reviewed interfaces.

Deterministic RNG is allowed only in tests, simulation, and explicitly marked reproducibility tools.

## 8. Concurrency and lifecycle

Tokio is the expected initial runtime unless a later architecture decision changes it.

Long-lived services must be supervised. The daemon composition root must know which tasks are essential, restartable, degradable, or optional.

Each service must define:

- Startup dependencies.
- Readiness conditions.
- Health signals.
- Owned resources.
- Cancellation behavior.
- Graceful shutdown behavior.
- Forced shutdown behavior.
- Failure propagation.

A task may not outlive the destination, connection, listener, tunnel pool, or service that owns it.

Channels must be bounded. Channel closure must be treated as a lifecycle event, not ignored in a retry loop.

## 9. Configuration and CLI

The daemon runs in the foreground by default.

Configuration processing must follow:

```text
parse -> schema validation -> semantic validation -> normalization -> immutable snapshot
```

Invalid configuration must fail before network listeners or router state are mutated.

Configuration changes should eventually be classified as:

- Live reloadable.
- Component drain and restart.
- Router restart required.
- Explicit identity or key operation.

Administrative and client listeners bind to loopback by default. Non-loopback exposure requires explicit configuration and an authentication design appropriate to the interface.

## 10. Observability and privacy

Operational visibility must not become a metadata leak.

Default logs must not contain full router hashes, destination identities, private keys, session keys, LeaseSet secrets, full packet bodies, user traffic, or sensitive local filesystem paths.

Peer and destination identifiers should be redacted, shortened, keyed-hashed, or omitted according to the diagnostic need.

Metrics must avoid unbounded cardinality. Do not use peer IDs, destinations, tunnel IDs, request IDs, hostnames, or arbitrary error strings as unrestricted metric labels.

Packet-level tracing and identity-rich diagnostics require an explicit unsafe-debug mode with clear warnings.

## 11. Storage and persistence

Persisted network data remains untrusted. RouterInfo, LeaseSets, profiles, and caches must be revalidated when loaded.

Writes involving identity, configuration, NetDB state, or security-critical metadata must be atomic or recoverable. Corruption behavior must be tested.

Storage formats require versioning and migration policy before they become externally relied upon. Do not expose unstable internal serialization as a public compatibility promise.

## 12. Testing requirements

A feature is not complete because it compiles or succeeds on a happy path.

Protocol and subsystem work must include, as applicable:

- Unit tests.
- Malformed-input and negative tests.
- Boundary-value tests.
- Property tests.
- Golden encoding vectors.
- Fuzz targets.
- Deterministic state-machine tests.
- Cancellation and cleanup tests.
- Resource exhaustion tests.
- Restart and persistence tests.
- Mixed-router interoperability tests.

Tests involving timing should prefer a virtual or controllable clock. Tests involving randomness should use reproducible seeds unless validating production entropy integration.

Public-network testing must be passive, ordinary, and non-disruptive. Stress, mutation, malformed traffic, load testing, and adversarial scenarios belong in an isolated authorized testnet.

## 13. Synvoid and eggsec integration

Synvoid integration should normally occur through a local service boundary:

```text
I2P destination -> i2pr server tunnel -> Unix socket or loopback TCP -> synvoid
```

The router must not make synvoid a mandatory dependency for core routing, NetDB, transport, tunnel, or client functionality.

Eggsec integration should use `i2pr-testkit`, private testnet orchestration, fault injection, protocol fixtures, and stable test interfaces. Production deployments must not expose unrestricted security-testing hooks.

## 14. Licensing and provenance

A project license must be selected before accepting substantive copied or adapted code.

Do not copy code from I2P+, i2pd, Emissary, Java I2P, or other router implementations without explicit license and provenance review.

Implementation should be clean-room where practical, using specifications, proposals, test vectors, interoperability observations, and independently written code.

Every nontrivial imported test vector or fixture should record its origin and license.

Generated code and agent-authored code remain subject to the same provenance and review requirements as human-authored code.

## 15. Agent and handoff discipline

Before modifying the repository, an implementation agent must read:

1. `README.md`.
2. This file.
3. The applicable roadmap or detailed plan.
4. Any architecture decision records relevant to the change.

Agents must not silently expand scope. When a prerequisite is missing, implement the smallest safe prerequisite or record the blocker in the handoff.

Each handoff should state:

- Files changed.
- Behavior implemented.
- Tests run and their results.
- Tests not run and why.
- Dependency changes.
- Security-relevant decisions.
- Deviations from the plan.
- Remaining risks and follow-up work.

Do not mark a phase complete when acceptance criteria are unmet. Partial implementation should be labeled clearly.

## 16. Definition of done

A change is done only when:

- The intended behavior is implemented.
- Errors are explicit and actionable.
- Untrusted-input behavior is bounded.
- Lifecycle and cleanup paths are covered.
- Tests appropriate to the risk are present and passing.
- Documentation is updated.
- Dependency and license implications are reviewed.
- The change does not violate the architecture dependency direction.
- Known limitations are recorded rather than hidden.

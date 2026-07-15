# Pre-plan: workspace setup and project skeleton

## Purpose

This pre-plan prepares `i2pr` for implementation without prematurely committing the project to unstable protocol APIs or writing incomplete router behavior.

The deliverable is a clean, buildable Rust workspace with enforceable crate boundaries, baseline CI, documentation scaffolding, deterministic test infrastructure interfaces, and a minimal non-networked CLI shell. It is intended for direct handoff to an implementation agent.

This is Milestone 0 of `plans/000-mvp-roadmap.md`.

## Starting state

At the time of this plan, the repository contains only project documentation:

- `README.md`
- `GUARDRAILS.md`
- `plans/000-mvp-roadmap.md`
- This pre-plan

There is no Rust workspace, source code, CI configuration, license, toolchain policy, or architecture decision record.

## Required reading before implementation

The implementation agent must read, in order:

1. `README.md`
2. `GUARDRAILS.md`
3. `plans/000-mvp-roadmap.md`
4. This plan

Where this plan is ambiguous, `GUARDRAILS.md` takes precedence.

## Scope

This phase must establish:

- A root Cargo workspace.
- A deliberately small first set of crates.
- Workspace-wide compiler, lint, dependency, and profile configuration.
- A minimal CLI daemon shell with no router networking.
- Strict configuration parsing and semantic validation structure.
- Core lifecycle, health, clock, randomness, and resource-budget interfaces.
- A deterministic testkit foundation.
- CI and local quality commands.
- Architecture and protocol-support documentation.
- An ADR process and initial ADRs.

## Non-goals

Do not implement:

- NTCP2 or SSU2 sockets, handshakes, frames, or transport selection.
- I2NP messages beyond placeholder namespace ownership.
- RouterInfo, LeaseSet, Destination, or tunnel wire codecs.
- Reseeding or HTTP downloads.
- NetDB, floodfill, tunnel construction, transit participation, garlic routing, streaming, SAM, I2CP, HTTP proxying, SOCKS5, or IRC.
- Persistent router identity.
- A web interface, TUI, daemonization, systemd installer, or background service manager.
- Runtime plugin loading.
- Synvoid or eggsec integration.
- Performance optimization.

The skeleton must not advertise unimplemented protocol support.

## Design decisions for this phase

### Initial crates

Create only the crates needed to enforce the first dependency boundaries:

```text
crates/
  i2pr-proto/
  i2pr-core/
  i2pr-daemon/
  i2pr-testkit/
```

Do not create all roadmap crates as empty packages. Empty crates create false structure and maintenance noise. Later detailed plans should add `i2pr-crypto`, transports, NetDB, tunnels, clients, APIs, storage, and service-tunnel crates when their contracts are understood.

### Crate responsibilities

#### `i2pr-proto`

Owns protocol-level namespace, primitive error categories, and future codec traits. During this phase it should contain only carefully chosen foundational types or traits required to establish conventions.

It must not depend on Tokio, clap, TOML, filesystem APIs, tracing subscribers, or `i2pr-daemon`.

Use `#![forbid(unsafe_code)]`.

Design the crate so `no_std + alloc` remains possible later, but do not require completing `no_std` support in this pre-plan unless doing so is trivial and does not distort the API.

#### `i2pr-core`

Owns runtime-neutral router service contracts:

- Service identity.
- Lifecycle state.
- Health state and snapshots.
- Shutdown reason and cancellation concepts.
- Resource classes and budget requests.
- Clock-facing domain types where needed.
- Stable shared error categories.

It may depend on `i2pr-proto` only when justified. Avoid making `i2pr-core` a miscellaneous utility crate.

Use `#![forbid(unsafe_code)]`.

#### `i2pr-testkit`

Owns deterministic testing support:

- Manual or virtual clock interface and implementation.
- Seeded deterministic RNG wrapper for tests.
- Reproducibility seed formatting and parsing.
- Minimal fault model types reserved for later in-memory network work.
- Test assertions for task and resource cleanup where practical.

It may depend on `i2pr-core` and `i2pr-proto`. Production crates must not depend on `i2pr-testkit` outside dev-dependencies.

Use `#![forbid(unsafe_code)]`.

#### `i2pr-daemon`

Owns:

- The `i2pr` binary.
- CLI parsing.
- Configuration-file loading.
- Configuration schema and semantic validation.
- Logging initialization.
- Future subsystem composition.
- Process exit-code mapping.

During this phase it must not open network listeners or create router identity.

It may depend on `i2pr-core` and `i2pr-proto`. It should use typed daemon errors internally and may use `anyhow` only at the final binary boundary if that materially improves context.

Use `#![forbid(unsafe_code)]`.

## Proposed repository structure

```text
.
├── .cargo/
│   └── config.toml
├── .github/
│   ├── dependabot.yml
│   └── workflows/
│       └── ci.yml
├── crates/
│   ├── i2pr-core/
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   ├── i2pr-daemon/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── cli.rs
│   │   │   ├── config.rs
│   │   │   ├── error.rs
│   │   │   ├── lib.rs
│   │   │   └── main.rs
│   │   └── tests/
│   ├── i2pr-proto/
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   └── i2pr-testkit/
│       ├── Cargo.toml
│       └── src/lib.rs
├── docs/
│   ├── architecture.md
│   ├── protocol-support.md
│   ├── security-model.md
│   └── adr/
│       ├── 0000-adr-process.md
│       ├── 0001-modular-monolith.md
│       ├── 0002-tokio-in-daemon-runtime.md
│       └── 0003-bounded-supervised-services.md
├── plans/
├── Cargo.toml
├── Cargo.lock
├── CONTRIBUTING.md
├── deny.toml
├── rust-toolchain.toml
└── rustfmt.toml
```

A license file should be added only after the project owner selects the license. Do not guess.

## Phase A: root workspace and toolchain policy

### Tasks

1. Create root `Cargo.toml` using the current stable Rust edition supported by the selected toolchain.
2. Configure workspace resolver version 2 or the current appropriate resolver.
3. Register the four initial crates.
4. Centralize package metadata where appropriate:
   - Rust edition.
   - Repository URL.
   - Authors only if explicitly supplied.
   - License only after selection.
   - Minimum supported Rust version if the project chooses to declare one.
5. Centralize shared dependencies under `[workspace.dependencies]`.
6. Add workspace lint configuration.
7. Add release, test, and development profiles conservatively.
8. Commit `Cargo.lock` because the workspace includes an application binary.
9. Add `rust-toolchain.toml` with pinned stable channel and required components.
10. Add `rustfmt.toml` with minimal deviations from standard formatting.

### Workspace lint baseline

Use strict but sustainable lints. At minimum:

- Deny unsafe code in relevant crates through crate attributes.
- Warn on missing debug implementations only where useful; do not force secrets to implement debug.
- Warn on unused must-use values.
- Deny unexpected cfg values.
- Enable a reviewed Clippy baseline.
- Avoid `clippy::pedantic` as an unreviewed blanket deny.
- Do not globally deny all warnings, since compiler upgrades can otherwise break unrelated work. CI may run selected warnings as errors through targeted configuration.

### Build profiles

Do not prematurely optimize. Suggested principles:

- Preserve overflow checks in tests.
- Keep release panic behavior as unwind until shutdown and secret-cleanup implications are reviewed.
- Do not enable global LTO during bootstrap.
- Do not introduce custom CPU target flags.

### Acceptance criteria

- `cargo check --workspace` succeeds.
- `cargo test --workspace` succeeds.
- `cargo fmt --all --check` succeeds.
- `cargo clippy --workspace --all-targets --all-features` succeeds under the chosen lint policy.

## Phase B: crate skeletons and dependency enforcement

### Tasks

1. Create the four initial crates.
2. Add crate-level documentation describing responsibility and explicit non-responsibility.
3. Add `#![forbid(unsafe_code)]` to every initial crate.
4. Keep public exports minimal.
5. Add a dependency-direction test or CI check if a lightweight maintained tool can enforce it without adding excessive complexity.
6. Otherwise document the dependency graph and validate manifests directly in CI with a small script or `cargo metadata` check.
7. Ensure `i2pr-proto` has no runtime or daemon dependencies.
8. Ensure production crates do not depend on `i2pr-testkit`.

### Required API restraint

Do not invent broad traits such as `RouterRuntime`, `Transport`, `NetDb`, or `TunnelManager` before implementation plans define their consumers.

Acceptable initial contracts include small domain types such as:

- `ServiceName` or `ServiceId`.
- `LifecycleState`.
- `HealthState`.
- `ShutdownReason`.
- `ResourceClass`.
- `ResourceLimit` and `ResourceUsage`.
- `ReproducibilitySeed` in testkit.

Avoid generic “manager,” “context,” “registry,” or “provider” abstractions without concrete use cases.

### Acceptance criteria

- Dependency graph matches the documented direction.
- Crate docs explain boundaries.
- No placeholder API claims protocol behavior.
- No crate contains dead stubs such as `todo!()` in code paths invoked by tests or CLI commands.

## Phase C: CLI shell

### Commands

Implement a minimal CLI surface:

```text
i2pr --help
i2pr --version
i2pr check-config --config <path>
i2pr run --config <path> --dry-run
```

`run` without `--dry-run` should return an explicit “router runtime not implemented” error during this phase. It must not pretend to start successfully.

The CLI may reserve future command names in documentation, but should not expose commands that have no defined output contract.

### Exit codes

Define stable initial exit-code categories, for example:

- `0`: success.
- `2`: CLI usage error, normally managed by clap.
- `10`: configuration file unavailable.
- `11`: configuration parse failure.
- `12`: configuration semantic validation failure.
- `20`: requested runtime capability not implemented.
- `70`: internal software failure.

Exact values may change before release, but centralize mapping in one place and test it.

### Output behavior

- Human-readable diagnostics go to stderr.
- Machine-readable output is not required yet.
- Do not print secret-bearing configuration values.
- `check-config` should clearly distinguish parse and semantic errors.
- `run --dry-run` should validate and normalize configuration, initialize no network resources, then exit successfully.

### Acceptance criteria

- CLI snapshot or integration tests cover help, version, missing config, malformed config, semantically invalid config, valid dry-run, and unimplemented live run.
- No command opens a socket.
- No command writes identity or network state.

## Phase D: configuration skeleton

### Initial configuration shape

Use a versioned TOML schema. Keep it small:

```toml
schema_version = 1

[router]
data_dir = "..."
profile = "balanced"

[logging]
filter = "info"
format = "text"

[limits]
max_tasks = 4096
max_buffered_bytes = 67108864
```

Field names and defaults should be reviewed during implementation. Do not add transport, NetDB, tunnel, SAM, or I2CP sections until those components have detailed plans.

### Parsing requirements

- Deny unknown fields by default.
- Preserve source context for parse diagnostics where practical.
- Separate raw schema from normalized runtime configuration.
- Validate paths without creating them during `check-config` unless explicitly documented.
- Validate limits for zero, overflow, impossible relationships, and platform constraints.
- Avoid environment-variable expansion during this phase unless designed and documented.
- Do not permit configuration to supply secret key material yet.

### Data directory handling

`check-config` should validate the configured path syntactically and report existing-path problems without mutating the filesystem.

`run --dry-run` may resolve paths but must not create directories.

Actual directory creation, permissions, locking, and identity storage belong to a later detailed plan.

### Acceptance criteria

- Raw and normalized config types are distinct.
- Unknown fields fail.
- Semantic errors identify the field and reason.
- Defaults are explicit and tested.
- Config validation has no network or persistent side effects.

## Phase E: core lifecycle and resource contracts

### Lifecycle model

Define a small lifecycle state model, likely including:

- Created.
- Starting.
- Ready.
- Degraded.
- Stopping.
- Stopped.
- Failed.

Do not implement a full supervisor yet. Establish types and invariants with tests.

### Health model

Health should distinguish:

- Readiness.
- Liveness.
- Degradation reason.
- Last transition time or sequence.

Avoid arbitrary strings as the only diagnostic representation. Use typed health codes plus bounded human context.

### Resource model

Define resource classes and values without committing to final enforcement implementation. Candidate classes:

- Tasks.
- Buffered bytes.
- Pending handshakes.
- Active links.
- NetDB queries.
- Tunnel builds.
- Destinations.
- Streams.
- API sessions.

Only `Tasks` and `BufferedBytes` need to appear in the initial config. Other classes may exist as enum variants if justified by the roadmap, but avoid public stability promises.

Implement a minimal in-memory budget or lease type only if its release-on-drop and no-overcommit behavior can be tested cleanly. Otherwise define the domain model and defer concurrency-aware enforcement to Milestone 2.

### Acceptance criteria

- Lifecycle transitions reject invalid state changes.
- Health snapshots are bounded and redact-safe.
- Any implemented resource lease releases on drop and cannot underflow or overcommit.
- Tests cover success, rejected acquisition, and release behavior.

## Phase F: deterministic testkit baseline

### Clock

Define a narrow clock abstraction for state-machine tests:

- Current monotonic instant.
- Explicit advancement in manual tests.
- Deadline comparison.

Do not attempt to abstract every Tokio timer API.

### Randomness

Provide a deterministic RNG wrapper seeded by a stable `ReproducibilitySeed` type.

Every randomized test failure should be able to print a seed that reproduces the case. Do not expose deterministic RNG through production configuration.

### Fault model placeholders

Define serializable or debug-friendly bounded types for future network simulation:

- Drop.
- Delay.
- Duplicate.
- Reorder.
- Truncate.
- Disconnect.

Do not build the full in-memory network in this phase unless it remains small and directly tested.

### Acceptance criteria

- Manual clock tests use no wall-clock sleep.
- Seed parse/format round trips.
- Re-running with the same seed produces the same generated sequence.
- Testkit is only a dev-dependency of production crates.

## Phase G: documentation and ADRs

### `docs/architecture.md`

Document:

- Four-plane model: data, control, client, service.
- Initial crate graph.
- Composition root ownership.
- Narrow-capability communication.
- Planned supervised-service model.
- Distinction between network tunnels and application service tunnels.
- Future synvoid and eggsec boundaries.

### `docs/protocol-support.md`

Create a table with all MVP protocol areas marked clearly as:

- Not implemented.
- Planned milestone.
- Specification/proposal source placeholder.
- Test-vector status.
- Interoperability status.

Do not use ambiguous labels such as “partial” without explaining what works.

### `docs/security-model.md`

Document initial assets, adversaries, trust boundaries, security objectives, and explicit non-claims.

At minimum cover:

- Remote unauthenticated peers.
- Authenticated but malicious peers.
- Malicious SAM/I2CP clients.
- Malformed local configuration.
- Corrupted persisted network state.
- Resource exhaustion.
- Metadata leakage through logs and metrics.
- Supply-chain and dependency risk.

### ADRs

Create:

- ADR 0000: ADR process and status vocabulary.
- ADR 0001: modular monolith and crate-boundary strategy.
- ADR 0002: Tokio at runtime-facing boundaries while keeping protocol/state-machine logic as runtime-neutral as practical.
- ADR 0003: bounded queues, supervised services, explicit cancellation, and no detached long-lived tasks.

ADRs should record context, decision, consequences, alternatives, and review triggers.

### Acceptance criteria

- Documentation matches actual manifests and CLI behavior.
- No documentation claims protocol support.
- ADRs are accepted or clearly marked proposed according to the selected process.

## Phase H: CI, dependency policy, and contribution workflow

### CI workflow

Run at minimum:

- Formatting check.
- Workspace check.
- Workspace tests.
- Clippy across all targets and features.
- Documentation build with warnings denied where sustainable.
- Dependency policy check.
- Minimal supported platform matrix.

Initial platform matrix should include Linux and macOS. Windows may be added immediately if available without slowing bootstrap substantially, but the roadmap should not claim Windows support until CI exists.

Use dependency caching conservatively. CI correctness is more important than aggressive caching during bootstrap.

### Dependency governance

Add `cargo-deny` configuration covering:

- Advisories.
- Bans or duplicate-version review.
- Sources.
- Licenses after the project license policy is selected.

Because the repository currently lacks a chosen license, document the incomplete license policy rather than inventing allowed-license rules that imply a project decision.

Consider but do not automatically add:

- `cargo-audit` in addition to `cargo-deny`.
- `cargo-semver-checks` before public crates have stable APIs.
- Heavy supply-chain tooling without a maintenance owner.

### Dependabot

Configure conservative Cargo and GitHub Actions updates. Avoid automatic merging.

### `CONTRIBUTING.md`

Document:

- Required reading.
- Local quality commands.
- Plan-first workflow for protocol changes.
- Security-sensitive review expectations.
- Commit and handoff expectations.
- How to report security issues without publishing exploit details.
- No public-network adversarial testing.

### Acceptance criteria

- CI passes from a clean checkout.
- Dependency policy has no unexplained exceptions.
- Contribution instructions reproduce CI locally.

## Suggested dependency set

Keep the initial dependency set small. A reasonable bootstrap set may include:

- `clap` for CLI parsing.
- `serde` and `toml` for configuration.
- `thiserror` for typed errors.
- `tracing` and `tracing-subscriber` in the daemon.
- `tokio` in the daemon only if needed for the future composition shell; the bootstrap CLI may remain synchronous if Tokio adds no current value.
- `rand_chacha` and `rand_core` in testkit for deterministic RNG.
- `tempfile` as a dev-dependency for filesystem-independent config tests.

Do not add cryptographic, HTTP, compression, metrics, database, async-trait, futures, bytes, or socket dependencies until a concrete phase requires them.

The implementation agent must review current crate versions and features rather than copying version numbers from another project.

## Testing matrix

### Workspace

- Clean build.
- All tests.
- All feature combinations that exist.
- Documentation build.
- No unexpected cfg warnings.

### CLI

- Help.
- Version.
- Missing argument.
- Missing config file.
- Malformed TOML.
- Unknown field.
- Unsupported schema version.
- Invalid data directory.
- Invalid limit.
- Valid `check-config`.
- Valid `run --dry-run`.
- Explicit failure for non-dry live run.

### Core

- Valid lifecycle transitions.
- Invalid lifecycle transitions.
- Health snapshot limits.
- Resource acquisition and release if implemented.
- No underflow or overcommit.

### Testkit

- Seed round trip.
- Deterministic sequence reproduction.
- Manual clock advancement.
- Deadline behavior without sleep.

### Repository policy

- Production crate cannot add `i2pr-testkit` as a normal dependency.
- `i2pr-proto` cannot depend on daemon/runtime/config crates.
- Unsafe code check passes.

## Handoff requirements

The implementation handoff must include:

- Final file tree.
- Crate dependency graph.
- Commands run and exact results.
- CI workflow summary.
- Dependency list with rationale and enabled features.
- Any lint exceptions and why they exist.
- Configuration schema and defaults.
- CLI exit-code mapping.
- ADR status.
- Deferred items.
- Deviations from this plan.
- Security-relevant observations.

## Explicit stop conditions

Stop and report rather than improvising when:

- A license must be selected to proceed.
- A dependency introduces incompatible licensing or unexplained unsafe code.
- The requested API requires guessing future transport, NetDB, or tunnel contracts.
- CI cannot run on the selected platform without weakening required checks.
- A guardrail conflicts with an implementation shortcut.

A stop condition should produce a focused handoff with the blocking decision, viable alternatives, and the smallest safe path forward.

## Definition of done

This pre-plan is complete when:

- The four-crate workspace exists and builds.
- All initial crates document their boundaries and forbid unsafe code.
- The dependency graph is acyclic and matches the intended direction.
- The CLI implements help, version, strict `check-config`, and side-effect-free `run --dry-run`.
- Live `run` fails explicitly as unimplemented.
- Configuration parsing and semantic validation are separated and tested.
- Core lifecycle, health, and initial resource-domain types exist with invariants tested.
- Testkit provides deterministic seed and manual-clock foundations.
- CI enforces formatting, checking, tests, clippy, docs, and dependency policy.
- Architecture, protocol-support, security-model, ADR, and contribution documents exist and match the code.
- No network listener, router identity, or misleading protocol implementation has been introduced.

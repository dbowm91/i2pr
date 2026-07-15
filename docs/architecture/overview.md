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
| Client | Destinations, LeaseSets, streaming, SAM, I2CP adapters | Not implemented. |
| Service | HTTP, SOCKS5, IRC, generic TCP, local service tunnels | Not implemented. |

Network tunnels (router-to-router) and application service tunnels
(local app to destination) are deliberately kept apart. Service tunnels
must not import transport internals or peer-profile storage.

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
startup and cleanup; the reference-crosscheck profile is reserved for Plan 041
and currently returns `blocked_missing_driver`; i2pr mixed-router evidence requires bounded authenticated
runs between i2pr and each reference in both directions. Only the last class
can contribute to a protocol support claim, and only after sanitation leaves
typed outcomes, bounded metadata, and artifact/configuration hashes. Raw
addresses, identities, RouterInfo, I2NP, keys, transcripts, logs, and arbitrary
remote error text are not retained.

## Crate graph

The dependency direction is enforced by `scripts/check-dependency-direction.sh`
and the [`docs/architecture/dependency-graph.md`](dependency-graph.md)
detail document.

```text
i2pr-proto  <- i2pr-crypto <- i2pr-storage
     ^             ^              ^
     |             |              |
i2pr-core <- i2pr-transport <- i2pr-runtime <- i2pr-daemon (composition root)
     ^             ^              ^
     |             |              |
     +-------------+   i2pr-transport-ntcp2
                          ^
                          |
                i2pr-proto + i2pr-crypto

i2pr-testkit (test-only; may depend on transport crates;
              no production crate may depend on it)
```

Reading from the arrows: lower crates stay pure, runtime-neutral, and
narrow. Higher crates widen scope and (in the case of `i2pr-runtime`)
take exclusive ownership of Tokio and sockets.

## Crate index

Each row links to a deep-dive document covering the crate's purpose,
module layout, public surface, key contracts, errors, dependencies,
tests, and any distinctive design choices.

| Crate | Role | One-liner | Deep dive |
| --- | --- | --- | --- |
| `i2pr-proto` | Foundation | Bounded wire codecs for I2P common structures and I2NP messages. No runtime, no I/O. | [i2pr-proto.md](i2pr-proto.md) |
| `i2pr-crypto` | Identity crypto | Protocol-specific wrappers around Ed25519, X25519, SHA-256. Secret material is zeroized. | [i2pr-crypto.md](i2pr-crypto.md) |
| `i2pr-storage` | Persistence | Versioned, atomic, permission-hardened storage for router identity and NTCP2 static key. | [i2pr-storage.md](i2pr-storage.md) |
| `i2pr-core` | Service contracts | Runtime-neutral lifecycle, health, cancellation, and resource budgets. Zero dependencies. | [i2pr-core.md](i2pr-core.md) |
| `i2pr-transport` | Transport contracts | Runtime-neutral link/delivery contracts. No Tokio, no I/O, no async. | [i2pr-transport.md](i2pr-transport.md) |
| `i2pr-transport-ntcp2` | NTCP2 protocol | Runtime-neutral Noise handshake, AEAD frames, data-phase blocks. | [i2pr-transport-ntcp2.md](i2pr-transport-ntcp2.md) |
| `i2pr-runtime` | Runtime owner | The only production owner of Tokio tasks, sockets, timers, channels, wakeable cancellation. | [i2pr-runtime.md](i2pr-runtime.md) |
| `i2pr-daemon` | Composition root | CLI + config + identity lifecycle. Live daemon execution not yet enabled. | [i2pr-daemon.md](i2pr-daemon.md) |
| `i2pr-testkit` | Test simulation | Deterministic clocks, virtual links, scripted faults. Test-only; never a production dep. | [i2pr-testkit.md](i2pr-testkit.md) |
| `scripts/` + `tests/` + `fuzz/` | Tooling | Guardrails, fixtures, integration lanes, opt-in fuzzing. | [tooling.md](tooling.md) |

## How data flows at runtime

Even though the live daemon is not yet wired in, the seams are in place.
A future `i2pr run` will:

1. **`i2pr-daemon`** parses CLI flags, loads and validates the TOML
   config under `deny_unknown_fields`, maps errors to stable exit codes,
   then constructs an `IdentityStore` and hands off the validated
   `Config`.
2. **`i2pr-storage`** loads the router identity from
   `<data_dir>/router.identity` and (separately) the NTCP2 static key
   from `<data_dir>/ntcp2.static.key`. Either file can be generated,
   but never silently replaced (atomic `hard_link` + `AlreadyExists`).
3. **`i2pr-runtime`** builds a `ServiceGraph`, topologically validates it
   before startup, then spawns one supervisor manager per service via a
   `JoinSet`. Each service receives a narrowed `ServiceContext` (name,
   cancellation, readiness, health, child scope) — never a direct handle
   to the supervisor.
4. **`i2pr-transport-ntcp2`** implements the protocol: Noise XK
   handshake, AES-CBC ephemeral obfuscation, ChaCha20-Poly1305 data
   phase, directional SipHash frame-length masking, deterministic
   handshake state machines. It returns `HandshakeAction` /
   `FrameAction` requests; `i2pr-runtime` fulfills them with real
   sockets and cancellation.
5. **`i2pr-transport`** sits underneath as the runtime-neutral link
   manager: `LinkState` FSM, `TransportManager` admission with RAII
   leases, duplicate-resolution policy, privacy-safe `TransportSnapshot`.
6. **`i2pr-core`** provides lifecycle, health snapshots, cancellation
   tokens, and the shared `ResourceBudget` governor that all subsystems
   draw from via typed lease owners.
7. **`i2pr-proto`** and **`i2pr-crypto`** stay at the bottom — no one
   depends on anything above them except the test and integration
   layers.
8. **`i2pr-testkit`** is used only by tests. It exercises the same
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

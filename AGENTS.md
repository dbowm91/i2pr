# Repository Guidelines

`i2pr` is an experimental Rust I2P router. **Not production-ready.** Do
not use for anonymity, privacy, censorship resistance, or any
security-sensitive workload. NTCP2 stays experimental and
non-advertised; the production daemon does **not** activate NTCP2.

## Read first

Always read these before changing code or answering questions about state:

1. `README.md` — current status (Milestone 6 local product gate closed via Plan 134; Milestone 7 corrective umbrella is Plan 145; Plan 146 private-destination reference compatibility is closed; Plan 147 raw-driver implementation is retained; **Plan 149 self-composed local STREAM product is closed**; **Plan 150 is next executable**), build/test/lint commands, high-level architecture.
2. `GUARDRAILS.md` — non-negotiable engineering, security, interoperability, and collaboration constraints.
3. `CONTRIBUTING.md` — local quality checks, runtime/testkit conventions, rootless and Multipass evidence-lane contracts.
4. The active plan under `plans/` (entry point: [`plans/README.md`](plans/README.md); current authority: `plans/145-status.md` plus [`plans/149-status.md`](plans/149-status.md)). Read `plans/146-status.md`, `plans/147-status.md`, and `plans/148-status.md` as required audit context. The next executable plan is **Plan 150**.
5. `specs/support.toml` (mirrored to `docs/protocol-support.md`) for live protocol support state. `specs/CONFORMANCE.md` defines what counts as evidence.

Do **not** trust prose that disagrees with a checked-in script or executable test. Static boundary scripts (`scripts/check-*.sh`) are the source of truth for non-negotiable invariants. The [`i2pr-architecture`](.opencode/skills/i2pr-architecture/SKILL.md) skill is the index for the rest of the documentation.

## Workspace layout

14-crate Rust workspace under `crates/` plus one test-only binary under `tools/`. Production crates depend upward through the runtime; test crates/helpers stay on the side.

**Architecture index.** The full ownership map lives in [`docs/architecture/overview.md`](docs/architecture/overview.md), per-crate deep dives in [`docs/architecture/i2pr-*.md`](docs/architecture/), the dependency graph in [`docs/architecture/dependency-graph.md`](docs/architecture/dependency-graph.md), tooling in [`docs/architecture/tooling.md`](docs/architecture/tooling.md), and the historical NTCP2 interop apparatus in [`docs/architecture/interop-apparatus.md`](docs/architecture/interop-apparatus.md).

Quick reference:

- `i2pr-proto` — bounded wire codecs, typed errors, no I/O.
- `i2pr-crypto` — Ed25519/X25519/AES/ChaCha20-Poly1305/HMAC/SipHash/HKDF wrappers.
- `i2pr-storage` — versioned private-identity persistence; NTCP2 static-key/IV material lives separately.
- `i2pr-core` — runtime-neutral service contracts.
- `i2pr-transport`, `i2pr-transport-ntcp2` — runtime-neutral link/NTCP2 codec crates. No Tokio/sockets/async I/O.
- `i2pr-netdb`, `i2pr-netdb-persist` — RouterInfo/LeaseSet2 validation, local NetDB store, reseed persistence.
- `i2pr-runtime` — production owner of Tokio, sockets, timers, channels, cancellation.
- `i2pr-daemon` — CLI/composition root. Production graph does not activate NTCP2.
- `i2pr-tunnel` — runtime-neutral exploratory pool, short tunnel-build crypto, data plane.
- `i2pr-client` — local destination identity/pools/LeaseSet2/session/routing/Streaming core.
- `i2pr-api` — runtime-neutral SAM 3.1 bounded parser/commands/replies/private-destination/session/stream registry/FORWARD/NAMING. Plan 142 I2P Base64 correction is retained; Plan 146 closed private-destination reference compatibility.
- `i2pr-daemon` SAM service — supervised loopback listener, session transaction, STREAM raw socket owner/byte pump, local delivery bridge, forwarding. Plan 147 implementation is retained; **Plan 149 now owns the self-composed `SESSION CREATE` path (one `Arc<DestinationIdentity>` allocation, OS-CSPRNG `SamLocalProductFabric`, automatic per-destination driver spawn, local peer LeaseSet2 directory, byte-exact `STREAM STATUS RESULT=OK`/`DESTINATION=<peer-pub-b64>` raw transition, typed `DeliverySweepCounters`)**. The canonical product evidence lives in `crates/i2pr-daemon/tests/sam_stream_self_composed.rs`.
- `i2pr-testkit` — deterministic simulation; no production crate may depend on it.
- `tools/i2pr-interop` — non-production test launcher. Must never activate `i2pr-daemon`.

## Hard boundaries

These are checked on CI. Fix the boundary; do not weaken the script.

- **Workspace dependency direction** (`scripts/check-dependency-direction.sh`): preserve the documented upward dependency graph; `i2pr-api` consumes `i2pr-client`/`i2pr-crypto`/`i2pr-proto`; no production crate depends on `i2pr-testkit`.
- **Runtime boundaries** (`scripts/check-runtime-boundaries.sh`): no `unbounded_channel`; no Tokio/std networking/filesystem in transport crates; transport contracts stay synchronous; every spawned task keeps an explicit owner.
- **Fixture manifests** (`scripts/check-fixture-manifest.sh`): one-to-one committed fixture/manifest mapping.
- **NTCP2 vectors/interoperability** (`scripts/check-ntcp2-vectors.sh`, `scripts/check-ntcp2-interoperability.sh`): support remains experimental/non-advertised; no production daemon activation; no fallback to unrelated protocols.
- **Rootless/Multipass/constrained-host** boundary scripts remain authoritative. Do not add sudo/privileged/network-host shortcuts.

Non-production crates (`i2pr-testkit`, `tests/integration/ntcp2/`, `tools/i2pr-interop`, `fuzz/`) live outside the production dependency graph.

## Build, test, lint

Toolchain is pinned to Rust 1.95.0; MSRV is 1.88. Workspace edition is 2024.

Run from repo root before handoff:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
```

CI also runs `cargo deny check advisories bans sources`. `Cargo.lock` is authoritative.

### Focused test lanes

- Transport contract: `cargo test -p i2pr-transport --all-targets`, `cargo test -p i2pr-transport-ntcp2 --all-targets`.
- Runtime supervision: `cargo test -p i2pr-runtime --all-targets`; run the forced-cleanup 100-iteration test serially.
- Deterministic testkit: `cargo test -p i2pr-testkit --all-targets`.
- SAM protocol/private destination: `cargo test -p i2pr-api --all-targets` and `cargo test -p i2pr-daemon --test sam_plan146_reference -- --test-threads=1`.
- SAM local delivery regressions: `sam_stream_product`, `sam_stream_independent`.
- Plan 147 raw implementation regression: `cargo test -p i2pr-daemon --test sam_stream_raw_product`. This is a focused lower-level bridge regression; the canonical product evidence is now `cargo test -p i2pr-daemon --test sam_stream_self_composed`.
- Plan 149 self-composed black-box SAM STREAM lane: `cargo test -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1`. The suite drives behavior only through listener TCP after startup and may not call private bridge, peer-LeaseSet2, inbound-tunnel-factory, destination-driver, delivery, or byte-moving setup APIs.
- Constrained-host lane: `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'`.

### Testing conventions

- Runtime tests use paused Tokio time or `i2pr-testkit` manual clocks where appropriate.
- No wall-clock sleeps, DNS, or public-network traffic in tests. Runtime-owned socket tests use loopback only.
- Bounded queues/resource governors cover capacity-one, exact capacity, max+1, cancellation, closure, and resource release.
- Protocol fixtures must be sanitized/provenance-recorded and manifest-governed.
- Secret-bearing protocol values live in narrow non-cloneable/zeroizing owners with redacted `Debug`.
- Channel closure is a lifecycle event, not blindly retried.

## Coding conventions

- Workspace lints deny unsafe by default, unexpected cfgs, debug/todo/unimplemented patterns as configured.
- Treat all external input as hostile: explicit bounds, reject malformed/unknown/trailing bytes, no validation side effects, always test negative paths.
- Codec errors are typed enums; do not swallow codec results or mix router policy into wire codecs.
- NTCP2 static material remains separate from router identity persistence.
- Use OS CSPRNG for runtime cryptography/local-product ephemeral material; deterministic RNG only in tests/simulation/reproducibility tools.
- Library crates avoid `anyhow` as public error models.
- Centralize dependency versions; review new dependencies for transitive/security/license/unsafe impact.
- Avoid global mutable state/unrestricted service locators; use narrow handles/capabilities.
- For Plan 149 specifically: do **not** make `DestinationIdentity: Clone` or reconstruct a second private identity merely to satisfy `SamDestinationBridge`. Preserve one secret ownership graph via `Arc<DestinationIdentity>` (see `DestinationRuntime::with_shared_identity`).

## Architecture decisions

Architecture/security decisions live in `docs/adr/`. Plan-of-record is the newest active plan plus its status. When a milestone closes, leave explicit commands/results/evidence. Per-plan narratives are audit history when superseded by a newer status.

## Protocol support and current SAM authority

Every protocol surface is tracked in `specs/support.toml` and mirrored to `docs/protocol-support.md`. Entries default to `experimental` / `advertised = false`; advertising requires conformance evidence.

- NTCP2 remains experimental/non-advertised; no passed mixed-router result exists.
- Milestone 6 local destination/Streaming product is closed via Plan 134; independent-router interoperability is not claimed.
- SAM 3.1 is the current product layer under Plan 145/Plan 149 authority.
- Plan 146 private-destination reference compatibility is closed.
- Plan 147 raw socket ownership/byte-pump implementation is retained; its full original acceptance is superseded by Plan 149.
- Plan 148 is a blocked historical audit, not the next executable plan.
- **Plan 149 closed the self-composed local STREAM product** (one `Arc<DestinationIdentity>`, OS-CSPRNG `SamLocalProductFabric`, automatic per-destination driver spawn, local peer LeaseSet2 directory, byte-exact raw transition, typed `DeliverySweepCounters`).
- **Plan 150 is the next executable plan.** It will use correctly pinned external clients for final localhost SAM-client/FORWARD/NAMING closure on top of the Plan 149 self-composed listener.
- `sam_independent_clients = 0-passed`; Milestone 7 local product is closed; do not begin Milestone 8.

Treat the newest explicit superseding status as authoritative.

## Interop and external evidence lanes

Reference revisions for the historical NTCP2/reference lane remain pinned in their manifests/lock files. Do not change them without updating boundary checks.

Current relevant SAM reference/client provenance lives in `tests/integration/sam/README.md`:

- Java I2P `2800040deee9bb376567b671ef2e9c34cf3e30b6` — Plan 146 reference;
- i2pd `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e` — Plan 146 reference;
- libsam3 preferred Plan 150 exact snapshot `7d6e658798baec31394c5685f9583343cc00900b`;
- i2psam Plan 150 exact snapshot `b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac`;
- i2plib `6edf51cd5d21cc745aa7e23cb98c582144884fa8` supplementary unless a compatible Python runtime is qualified.

Plan 150 may add a manual unprivileged GitHub-hosted Ubuntu `workflow_dispatch` lane to fetch/build exact external revisions and run localhost SAM. It must not add privileged runners, containers, namespaces, VMs, public I2P participation, or vendored third-party source.

## OpenCode skills

Load the matching skill before touching its surface.

| Skill | Covers |
| --- | --- |
| [`i2pr-local-dev`](.opencode/skills/i2pr-local-dev/SKILL.md) | Routine Milestone 6/7 local product work. Plan 149 closed the self-composed local STREAM product; **Plan 150 is the next executable SAM plan.** |
| [`i2pr-architecture`](.opencode/skills/i2pr-architecture/SKILL.md) | Navigate architecture/ADR/plans/specs and audit drift. |
| [`i2pr-ntcp2-interop`](.opencode/skills/i2pr-ntcp2-interop/SKILL.md) | Historical/closed NTCP2 reference-router harness. |
| [`i2pr-rootless-sandbox`](.opencode/skills/i2pr-rootless-sandbox/SKILL.md) | Plan 046 rootless sealed-namespace lane. |
| [`i2pr-multipass-recovery`](.opencode/skills/i2pr-multipass-recovery/SKILL.md) | Plan 048–051 Multipass recovery guest. |

## Commits and pull requests

- Use focused imperative subjects.
- PRs document scope, changed files, tests/results, dependency/security decisions, deviations, and known limitations.
- Do not change git config, skip hooks, force-push, or amend someone else's commit.

## Directory orientation

- `crates/` — workspace crates.
- `tools/i2pr-interop/` — non-production interop launcher.
- `tests/fixtures/` — manifest-governed committed fixtures.
- `tests/integration/ntcp2/` — historical/reference interop apparatus.
- `tests/integration/sam/` — Plan 146 reference evidence and Plan 149/150 localhost/external-client provenance/harness surface.
- `fuzz/` — opt-in fuzz workspace.
- `docs/adr/`, `docs/architecture/`, `docs/protocols/`, `docs/security-model.md` — architecture/security/protocol documentation.
- `specs/` — support ledger, conformance, sources, protocol dossiers.
- `plans/` — plan-of-record/status authority. Read the newest relevant status before changing code.
- `scripts/` — static boundary and interop scripts.
- `.github/workflows/` — routine CI plus optional manual interop workflows; Plan 150 may add a manual SAM external-client workflow.
- `.opencode/skills/` — loadable OpenCode skill definitions.

Current handoff: **execute Plan 150** (Plan 149 already closed).

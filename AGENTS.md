# AGENTS.md

`i2pr` is an experimental Rust I2P router. **Not production-ready.** No anonymity/privacy claim. NTCP2 is experimental and non-advertised; SAM/I2CP/service-tunnels are loopback-only, disabled by default, non-advertised.

## Read first

1. `README.md` — status snapshot (may lag).
2. `GUARDRAILS.md` — non-negotiable security/architecture constraints.
3. `CONTRIBUTING.md` — conventions.
4. `plans/README.md` (planning system guide: registry, roadmaps, closure records) + load `i2pr-planning` when registering or closing out a plan.
5. `specs/support.toml` + `specs/CONFORMANCE.md` before claiming any protocol support.

Authority order: closure records > executable tests/scripts > ADRs > prose (see `plans/README.md`).

## Toolchain

Pinned Rust `1.95.0` (`rust-toolchain.toml`); MSRV `1.88` (`cargo check --locked --workspace --all-targets` must pass on both). Workspace `edition = "2024"`, `resolver = "2"`. Lints deny `unsafe_code`, `clippy::dbg_macro`, `clippy::todo`, `clippy::unimplemented` — protocol/client/API/service crates stay `#![forbid(unsafe_code)]` unless separately reviewed.

## Workspace boundaries

- `i2pr-proto` — bounded wire codecs, typed errors, no I/O.
- `i2pr-crypto` — protocol crypto wrappers (no local primitives).
- `i2pr-storage` — identity/key persistence.
- `i2pr-core` — runtime-neutral contracts/budgets/health.
- `i2pr-transport`, `i2pr-transport-ntcp2`, `i2pr-transport-ssu2` — runtime-neutral, no Tokio/sockets/`async fn`.
- `i2pr-netdb`, `i2pr-netdb-persist` — RouterInfo/LeaseSet2 validation/store.
- `i2pr-tunnel` — runtime-neutral exploratory pool, short-build, data plane.
- `i2pr-client` — destination lifecycle, ECIES session/routing, Streaming.
- `i2pr-api` — runtime-neutral SAM 3.1 + I2CP wire/state (no sockets).
- `i2pr-service-tunnels` — runtime-neutral tunnel config/policy (no sockets; daemon owns listeners).
- `i2pr-runtime` — sole production owner of Tokio, sockets, timers, channels, cancellation.
- `i2pr-daemon` — CLI/config/composition root; owns SAM/I2CP/service-tunnel listeners.
- `i2pr-testkit` — deterministic fixtures only; no production crate may depend on it.
- `tools/i2pr-interop` — non-production test launcher.

Enforced by `scripts/check-dependency-direction.sh` and `scripts/check-runtime-boundaries.sh`. Details: `docs/architecture/overview.md`.

## Skills and architecture index

- Skill bundles live in `.opencode/skills/` (canonical); `.agents/skills` is a symlink to the same directory — there is no separate `.skills/` directory. Load `i2pr-architecture` for ADR/plan navigation and doc-vs-source audits, `i2pr-local-dev` before touching product/SSU2/SAM/I2CP/tunnel code, `i2pr-planning` when registering or closing out an implementation plan (roadmap/registry/closure mechanics). The NTCP2/rootless/Multipass skills are historical (closed Plans 038–100/046/048 lanes) — read-only for archaeology, never for routine work.
- Architecture entry points: `docs/architecture/overview.md` (crate index, data flow); `docs/architecture/dependency-graph.md` (dependency allowlist, mirrors `check-dependency-direction.sh`); `docs/architecture/tooling.md` (scripts, fixtures, lanes, CI); `docs/architecture/i2pr-<crate>.md` (per-crate deep-dives); `docs/adr/` (decisions 0000–0025); `specs/CONFORMANCE.md` (what counts as evidence); `specs/support.toml` (machine-readable support inventory).

## Hard boundaries (CI-enforced — fix code, never weaken scripts)

- Preserve dependency direction; no prod dep on `i2pr-testkit`.
- No unbounded channels/queues; no `tokio::*`/`std::net`/`std::fs`/raw `JoinHandle`/ownerless `spawn` in transport/API/service crates.
- Every spawned task has explicit ownership/cancellation; channel/socket close is a lifecycle event, not retried blindly.
- Listeners bind loopback by default; non-loopback needs explicit config + auth design. SAM/I2CP/service-tunnels stay disabled by default.
- Secrets: no `Debug`/`Display`/unrestricted serialization on secret types; avoid `Clone` on secrets; zeroize where supported; never log `PRIV`, signing seeds, SSU2 static/session secrets, tokens, or raw payloads. Do not make `DestinationIdentity: Clone` or mint a second private identity for a bridge.
- Treat all network/config/disk bytes as hostile and bounded: checked arithmetic, caller-visible alloc caps, exact-consumption decodes, typed errors (no `anyhow` in library crates; no swallowed codec results).
- No patching/vendoring external routers/clients; no root/sudo/namespaces/containers/VM/systemd/public-I2P for routine acceptance.
- No capability/version/RouterInfo/SAM/I2CP behavior advertisement beyond tested subset (`specs/CONFORMANCE.md`).

## Routine floor (from repo root, before handoff)

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ssu2-vectors.sh
bash scripts/check-i2cp-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
bash scripts/check-sam-acceptance-evidence.sh
bash scripts/check-ssu2-acceptance-evidence.sh
bash scripts/check-i2cp-acceptance-evidence.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
bash scripts/check-exploratory-tunnel-evidence.sh
bash scripts/check-netdb-tunnel-evidence.sh
bash scripts/check-destination-tunnel-evidence.sh
bash scripts/check-streaming-tunnel-evidence.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'
cargo deny check advisories bans sources
```

macOS CI builds all test executables once then runs each with `--test-threads=1` (loopback suites flake under parallel Cargo). Use `--test-threads=1` locally for `i2pr-daemon`/`i2pr-runtime` suites. After changing committed fixture bytes, also run `bash scripts/check-fixture-manifest.sh`; after NTCP2/SSU2/I2CP fixture changes run the matching `check-*-vectors.sh`.

Focused examples (same `--locked` + `--test-threads=1` pattern):

```text
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed -- --test-threads=1
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --test ssu2_local -- --test-threads=1
cargo test --locked -p i2pr-daemon --test i2cp_message_data_plane -- --test-threads=1
cargo test --locked -p i2pr-daemon --test service_tunnels_local_roundtrip -- --test-threads=1
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- --test-threads=1
```

## Testing quirks agents miss

- Runtime tests: prefer `#[tokio::test(start_paused = true)]` / manual clock + explicit bounded deadlines; never wall-clock sleeps for overload/state-machine tests. Socket tests use `127.0.0.1:0` (OS port) and loopback only.
- Queue/resource tests must cover capacity 1, exact load, and max+1, and verify lease release on every drop path (receive/drop/timeout/cancel/panic/teardown).
- Black-box product tests (e.g. `sam_stream_self_composed.rs`) drive behavior only through TCP/SAM after listener startup — do not call private bridge/LeaseSet2/driver/pump APIs from them.
- I2CP pre-session reject path must `stream.shutdown()` before `teardown_connection`/`drop_connection` (`crates/i2pr-daemon/src/i2cp.rs`); `wrong_protocol_byte_is_closed` is strict (timeout = failure, 24-iteration baselines + non-paused companion). Never revert to drop-timing.
- SSU2 Java `pq=4,3` option is parser-tolerance only (`Ssu2PqKem`/`PqCapabilities`, `MAX_SSU2_PQ_SCHEMES = 8`); session stays classical X25519, publication stays pq-free, no ML-KEM.
- New deps need review (purpose, transitive impact, `unsafe` exposure, features, license); keep workspace versions centralized, default features narrow.

## External / interop lanes (fail-closed)

- Environment-gated tests are `#[ignore]`-gated. Ordinary run compiles but skips them; explicit run requires `--ignored --exact`, and missing env must **fail**, never silently pass. Forbidden: `|| true`, `continue-on-error`, filename filtering, fake peer env, broad exclusions, early-return-success, production wire changes to go green.
- Reference pins (do not change without a new plan): i2pd `2.61.0` (`635b013a612ff47278ef02acf8580a28e10e26c5`, mandatory); Java I2P `2.13.0` (`9134f808337b401e8e53c73734c81fab04280c9d`, secondary); go-i2cp `b529ee1c10a6011558b4d69fc9436a4afc489eac`; counted SAM clients i2psam `b80ecd48…`, i2plib `6edf51cd…`.
- Lane pattern: `bash tests/integration/<area>/run-*.sh` + `bash scripts/check-*-evidence.sh`. Examples: `tests/integration/ssu2/run-independent.sh`, `tests/integration/m6-interop/run-{preflight,tunnels,netdb,destination,streaming,java}.sh`, `tests/integration/service-tunnels/run-independent.sh` (delegates remote to `run-plan214-applications.sh`), `tests/integration/i2cp/run-independent.sh`. Manual lanes live in `.github/workflows/*-external.yml`.
- Raw reference logs are never evidence; only sanitized counts/hashes reach evidence files.

## Commits and handoff

Focused commits only; no git config changes, no `--no-verify`, no force-push, no amending others. Handoff lists: files changed, behavior + tests run (exact commands/results), tests not run + why, dep changes, security-relevant decisions, deviations, remaining risks.

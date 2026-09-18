# Contributing to i2pr

`i2pr` is an experimental, clean-room Rust router project. Contributions must
preserve `GUARDRAILS.md`, the active plan (`plans/registry.md`), and the architecture decisions in
`docs/adr/`.

## Before changing code

Read, in order:

1. `README.md`.
2. `GUARDRAILS.md`.
3. `plans/README.md` (planning system guide), the relevant subsystem roadmap under
   `plans/subsystems/`, and its current milestone status in `plans/registry.md`.
4. Relevant ADRs and protocol dossiers (`specs/README.md` index).

Load the matching skill before touching its surface: `i2pr-architecture` for
docs/ADR/plan navigation, `i2pr-local-dev` for product/SSU2/SAM/I2CP/tunnel code,
`i2pr-planning` when registering or closing out an implementation plan.

Protocol changes require a registered plan covering acceptance criteria, limits, negative
tests, dependency changes, security implications, sources, and documentation (see
`plans/README.md`; dossier update procedure in `specs/README.md`).
Do not add empty future crates or claim interoperability without evidence.
The `i2pr-proto` subset is structural only: keep signed byte regions intact and use the
pinned source ledger. Concrete Ed25519/X25519 wrappers and the private identity store
must remain outside `i2pr-proto`; update ADRs and the support ledger when
crypto/storage scope changes.

## Local quality checks

Run from the repository root:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
cargo deny check advisories bans sources
```

The CI matrix covers Linux and macOS. Dependency downloads may require network
access in restricted environments; do not weaken checks to work around that.
Use `--test-threads=1` locally for `i2pr-daemon`/`i2pr-runtime` suites (loopback
suites flake under parallel Cargo). The full routine floor, including evidence
checkers, lives in `AGENTS.md`.

## Testing conventions

Runtime supervision tests belong in `i2pr-runtime` and must use
`#[tokio::test(start_paused = true)]` or explicit `tokio::time::advance` for
deadlines and restart backoff. Exercise cancellation before and during waits,
readiness, panic classification, restart exhaustion, graceful shutdown, forced
abort, child-scope cleanup, and the zero-remaining-task report. Do not use
wall-clock sleeps or live sockets. Run the focused lane with
`cargo test -p i2pr-runtime --all-targets` in addition to the workspace checks.

Bounded communication and resource-governor tests must cover capacities of one,
exact offered load, and maximum-plus-one offered load. Test typed full,
deadline, cancellation, closure, response-drop, and resource-denial outcomes;
verify queue-held leases release on receive, drop, timeout, cancellation,
panic unwind, and supervisor teardown; and exercise atomic bundle denial with
no partial usage. Use deterministic Tokio time and explicit reproducibility
seeds where scheduling or accounting is exercised. Do not use wall-clock
sleeps, unbounded retry loops, or live network traffic for overload tests.

Deterministic simulation tests use `i2pr-testkit`'s fixed seed matrix:
the zero seed, the all-ones seed, and named regression seeds. Reproduce a
failure by recording the seed and scenario identifier, then compare the
privacy-safe `ReplayRecord` from two runs. Use manual clock advancement and
`run_until_idle(max_steps)`; do not add wall-clock sleeps, OS-random seeds,
real sockets, DNS, or public-network fault injection. The focused lane is:

```text
cargo test -p i2pr-testkit --all-targets
```

Run `bash scripts/check-fixture-manifest.sh` after changing committed fixture
bytes. The maintained fuzz workspace under `fuzz/` is intentionally outside
the production workspace and requires nightly `cargo-fuzz`; use
`bash scripts/fuzz-smoke.sh` for bounded local smoke runs. Fuzz-only
dependencies must not be added to production manifests.

Transport crates stay runtime-neutral: no Tokio, sockets, `async fn`, filesystem
access, generic Noise/provider APIs, or plugin registries. `i2pr-runtime` remains
the sole production owner of Tokio tasks, sockets, timers, channels, and wakeable
cancellation; transport code passes bounded encoded-I2NP owners through explicit
consuming handoffs and keeps peer references, addresses, keys, transcripts, and
payload bytes out of default debugging and snapshots. These boundaries are
enforced by `scripts/check-dependency-direction.sh` and
`scripts/check-runtime-boundaries.sh` — fix code, never weaken scripts.

Committed protocol fixtures must be sanitized, locally authored or provenance-
recorded, free of private keys/live identities/addresses/destinations, and
listed with classification, expected type or error category, exact source
revision, generator/input, license note, SHA-256, and independence status.
Fixture-backed tests must consume the bytes. Secret-bearing protocol values
must use narrow non-cloneable, zeroizing owners with redacted `Debug`; memory
hygiene does not imply encrypted-protocol support.

Tests may bind loopback addresses and use paused Tokio time, but must never
contact public I2P peers. Runtime address observations are candidates for later
policy only: they must not mutate NetDB, RouterInfo, or publication state.

## Historical lanes (closed — read-only)

Per-lane contributor rules live with their records. Do not extend these lanes
without a new plan-of-record:

- NTCP2 transport, handshake/data-phase details, mixed-runner scenarios,
  Ubuntu build-system gates, rootless sandbox, Multipass recovery, and Java
  startup/matrix drivers (Plans 030–101, 046–054 era): subsystem roadmap
  `plans/subsystems/ntcp2-transport-roadmap.md`; skills `i2pr-ntcp2-interop`,
  `i2pr-rootless-sandbox`, `i2pr-multipass-recovery` (archaeology only).
  Normal-daemon NTCP2 stays disabled. Some drivers named by old plans (e.g. the
  Java matrix/startup probes) no longer exist; the closure records govern.
- Short-build, exploratory, destination, and Streaming corrective detail
  (Plans 107–134, 183–201 era): roadmaps `plans/subsystems/exploratory-tunnels-roadmap.md`,
  `plans/subsystems/destination-streaming-roadmap.md`, and
  `plans/subsystems/mixed-router-interop-roadmap.md`.
- SAM/I2CP/service-tunnel corrective detail: roadmaps `plans/subsystems/sam-roadmap.md`,
  `plans/subsystems/i2cp-roadmap.md`, and `plans/subsystems/service-tunnels-roadmap.md`.

Lane architecture and tooling overviews live in `docs/architecture/interop-apparatus.md`
and `docs/architecture/tooling.md`.

## Security and testing

Treat all external input as hostile. Add malformed, boundary, cancellation,
cleanup, and resource-exhaustion tests alongside successful-path tests. Use the
manual clock and reproducibility seeds from `i2pr-testkit` for deterministic
state-machine tests. Public-network testing must be passive and ordinary;
stress, mutation, malformed-traffic, and adversarial tests belong only in an
authorized isolated testnet.

Report security issues privately to the project owner rather than publishing
exploit details in an issue or pull request. Treat router identity files and
backups as private key material; do not add private fixtures or print secret
bytes in tests and diagnostics.

Identity directories must be created with restrictive permissions from
inception. A post-create permission change is not an acceptable substitute;
when recursive creation cannot be made safe, require an existing secure
parent and document that policy.

Runtime snapshots are aggregate, eventually coherent observations assembled
without awaiting. Lower crates may emit the documented fixed-name tracing
events but must not install a subscriber. Event fields must stay within the
allowlist in `docs/security-model.md`; do not log health detail text, payloads,
identity/destination encodings, addresses, panic payloads, or dynamic
peer-derived labels.

## Dependencies, provenance, and commits

Keep dependencies focused, centralize workspace versions, review transitive
impact and unsafe-code exposure, and record why a new dependency is needed.
Do not copy implementation code or test vectors from another router until
license and provenance review is complete. The project license is intentionally
not selected yet.

Make focused commits that explain behavior and tests. Handoffs should list
changed files, commands and results, dependency changes, security-relevant
decisions, deviations, and remaining risks.

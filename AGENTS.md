# Repository Guidelines

## Project Structure and Boundaries

`i2pr` is an experimental Rust I2P router; it is not production-ready and
must not be used for anonymity or security-sensitive workloads. Source lives
under `crates/`: `i2pr-proto` (bounded wire codecs), `i2pr-crypto`,
`i2pr-storage`, runtime-neutral `i2pr-core`, runtime-neutral `i2pr-transport`,
Tokio-free `i2pr-transport-ntcp2`, Tokio-owning `i2pr-runtime`, the
`i2pr-daemon` composition root, and test-only `i2pr-testkit`. Integration tests
are in crate `tests/` directories; sanitized I2NP fixtures are in
`tests/fixtures/i2np/`; the opt-in nightly fuzz workspace is `fuzz/`.

Preserve the dependency direction `i2pr-proto <- i2pr-crypto <- i2pr-storage`,
`i2pr-core <- i2pr-transport <- i2pr-runtime <- i2pr-daemon`, and the
NTCP2-specific edge from protocol/crypto/transport contracts into
`i2pr-transport-ntcp2`. Production crates must not depend on `i2pr-testkit`;
lower-level crates must not depend on the daemon. `i2pr-runtime` is the only
production owner of Tokio tasks, sockets, timers, channels, and wakeable
cancellation.
Do not add transport, NetDB, tunnel, client, or plugin APIs without a bounded
plan.

## Before Changing Code

Read `README.md`, `GUARDRAILS.md`, `CONTRIBUTING.md`, the applicable `plans/`
document, and relevant `docs/adr/` records. Protocol changes also require the
matching dossier under `specs/protocols/` and `specs/CONFORMANCE.md`. Keep
`specs/support.toml` synchronized with `docs/protocol-support.md`; namespace
presence is not interoperability evidence.

Treat configuration, protocol, and persisted data as hostile: use explicit
bounds, reject unknown or trailing data, avoid validation side effects, and
test negative paths. Preserve the `i2pr-proto` crate-root façade, borrowed
cursors, strict decoding, typed codec errors, and structural-only protocol
scope. Secret-bearing values must be narrow, non-debuggable, non-cloneable
where practical, and zeroizing. Transport boundaries must use bounded owned
encoded I2NP payloads, typed deadlines/cancellation/outcomes, finite lifecycle
transitions, and privacy-safe snapshots; never expose raw sockets, Tokio
channels, peer addresses, keys, transcripts, or payload bytes.

## Build and Test

Use the pinned Rust 1.95 toolchain; Rust 1.85 is the declared MSRV. Before
handoff, run:

```text
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

Run `bash scripts/check-fixture-manifest.sh` when fixture bytes change and
`bash scripts/fuzz-smoke.sh` for the opt-in fuzz lane. Runtime tests must use
paused Tokio time or `ManualClock`, fixed seeds, stable scenario names, and
bounded steps—never wall-clock sleeps or public-network traffic.

Transport-focused changes should also run `cargo test -p i2pr-transport
--all-targets`, `cargo test -p i2pr-transport-ntcp2 --all-targets`, and both
dependency/runtime boundary scripts. Plan 031 is structural evidence only;
it does not authorize NTCP2 handshake, socket, or public-network testing.

## Runtime, Security, and Observability Rules

Every long-lived task has an owned supervisor/service scope and is awaited or
explicitly aborted and drained. Child counters decrement only after joins;
`Drop` may request abort but cannot claim completion. Queues have explicit,
nonzero bounded capacity and caller-visible overload, closure, deadline, and
cancellation outcomes. Resource leases own one exact grant and expose
underflow during cleanup without panicking. Log only fixed typed categories,
bounded counters, and synthetic metadata; never payloads, keys, identities,
addresses, paths, or arbitrary error text. Identity creation is create-only,
restrictive from inception, atomic, and fails closed on corruption.

Do not perform malformed, stress, or fault-injection testing against the
public I2P network; use `i2pr-testkit` or an authorized isolated testnet.

## Commits and Pull Requests

Use focused imperative commit subjects, for example
`docs: streamline repository guidelines`. PRs should explain scope, changed
files, test commands and results, dependency/security decisions, deviations,
and known limitations. Milestone closure work must leave an explicit closure
record with that evidence.

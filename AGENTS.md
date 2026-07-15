# Repository Guidelines

`i2pr` is an experimental Rust I2P router. It is not production-ready and must
not be used for anonymity or security-sensitive workloads. Read `README.md`,
`GUARDRAILS.md`, `CONTRIBUTING.md`, the applicable `plans/` document, and
relevant `docs/adr/` records before changing code.

## Workspace Layout

Nine-crate workspace under `crates/`:

- `i2pr-proto` — bounded wire codecs (crate-root façade, borrowed cursors, strict decoding, typed errors)
- `i2pr-crypto` — Ed25519/X25519/AES/ChaCha20-Poly1305/HMAC/SipHash wrappers
- `i2pr-storage` — versioned persistence; identity and NTCP2 static-key records
- `i2pr-core` — runtime-neutral service contracts
- `i2pr-transport` — runtime-neutral link/delivery contracts (no Tokio, no I/O)
- `i2pr-transport-ntcp2` — runtime-neutral NTCP2 handshake + data frames
- `i2pr-runtime` — **only** production owner of Tokio tasks, sockets, timers, channels, wakeable cancellation
- `i2pr-daemon` — composition root + CLI
- `i2pr-testkit` — deterministic simulation; production crates must not depend on it

Fixtures: `tests/fixtures/i2np/` (manifest at `tests/fixtures/i2np/manifest.tsv`),
`tests/fixtures/ntcp2/crypto/` (manifest at `…/manifest.tsv`). Opt-in nightly
fuzz workspace at `fuzz/`.

## Hard Boundaries (enforced by scripts)

These are checked on CI and will reject the change:

- Dependency direction (`scripts/check-dependency-direction.sh`):
  `i2pr-proto <- i2pr-crypto <- i2pr-storage`; `i2pr-core <- i2pr-transport
  <- i2pr-runtime <- i2pr-daemon`; `i2pr-transport-ntcp2` consumes
  `i2pr-crypto`/`i2pr-proto`/`i2pr-transport`. **No production crate may
  depend on `i2pr-testkit`.**
- Runtime boundaries (`scripts/check-runtime-boundaries.sh`):
  - No `unbounded_channel`, `UnboundedSender`, `UnboundedReceiver` in
    `i2pr-runtime`/`i2pr-testkit`/`i2pr-transport`/`i2pr-transport-ntcp2`.
  - No `tokio::*`, `std::net`, `std::fs`, `TcpStream`, `TcpListener`, etc.
    in `i2pr-transport`/`i2pr-transport-ntcp2`.
  - No `async fn`/`async_trait`/`i2pr-netdb|tunnel|client` in transport
    contracts (they stay synchronous).
  - Only `i2pr-runtime` and `i2pr-testkit` may list `tokio`/`tokio-util` deps.
  - `tokio::spawn` calls must keep an explicit owner (bound to `let`,
    `push(`, or `JoinSet`).
- NTCP2 interoperability (`scripts/check-ntcp2-interoperability.sh`):
  evidence must stay sanitized; the manifest under
  `tests/integration/ntcp2/manifest.toml` must list exactly eight bounded
  scenarios with the required disclaimer lines.

If a check fails, fix the boundary, don't suppress the script.

## Build, Test, and Quality

Toolchain is pinned to Rust 1.95.0 (`rust-toolchain.toml`); MSRV is 1.85
(verified by a dedicated CI job). Workspace edition is 2024; `max_width = 100`.

Before handoff, run from the repo root, in this order:

```text
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh   # when I2NP fixture bytes change
bash scripts/check-ntcp2-vectors.sh      # when NTCP2 vector bytes change
bash scripts/check-ntcp2-interoperability.sh   # when ntcp2 evidence/manifest change
bash scripts/fuzz-smoke.sh               # opt-in, requires cargo-fuzz + nightly
```

Focused lanes:

- Transport: `cargo test -p i2pr-transport --all-targets` and
  `cargo test -p i2pr-transport-ntcp2 --all-targets`.
- Runtime supervision: `cargo test -p i2pr-runtime --all-targets`. The
  forced-cleanup 100-iteration test needs serial execution:
  `cargo test -p i2pr-runtime forced_child_cleanup_is_repeatably_joined -- --test-threads=1`.
- Deterministic testkit: `cargo test -p i2pr-testkit --all-targets`.

Runtime tests must use `#[tokio::test(start_paused = true)]` or `ManualClock`
with fixed seeds and bounded steps. **No wall-clock sleeps, no real sockets, no
DNS, no public-network traffic in tests.** All transport verification runs
loopback/private-network only.

## Coding Conventions

- Workspace lints: `unsafe_code = "deny"`, `unexpected_cfgs = "deny"`,
  `unused_must_use = "warn"`; clippy denies `dbg_macro`, `todo`, `unimplemented`.
- `crate/secret` owners stay non-cloneable, non-`Debug`, and `zeroize::Zeroize`;
  the NTCP2 forbidden nonce `2^64 - 1` is never emitted.
- Treat configuration, protocol, and persisted data as hostile: explicit
  bounds, reject unknown or trailing bytes, no validation side effects, always
  test the negative path.
- Codec errors are typed; don't swallow codec results.
- NTCP2 static key/IV material lives in the separate versioned `i2pr-storage`
  record — never derive from or overwrite the router identity record.
- All architecture/security decisions belong in `docs/adr/` and `specs/`;
  the plan-of-record is the active `plans/NNN-*.md` plus its closure record.
  When you close a milestone, leave an explicit closure document with
  commands, results, and evidence.

## Support Ledger

Every protocol surface is tracked in `specs/support.toml` (mirrored to
`docs/protocol-support.md`). Entries default to `status = "experimental"` and
`advertised = false`. Setting `advertised = true` requires interoperability
evidence per `specs/CONFORMANCE.md` — namespace presence is not evidence.

## Commits and Pull Requests

- Focused imperative subjects, e.g. `docs: streamline repository guidelines`,
  `transport: bound ntcp2 data frame owners`.
- PRs document scope, changed files, test commands and results, dependency or
  security decisions, deviations, and known limitations. Milestone closures
  attach the closure record with that evidence.
- Don't update git config, skip hooks, force-push, or amend someone else's
  commit. If a hook rejects, fix the issue and add a new commit.

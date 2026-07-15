# Repository Guidelines

## Project Structure and Boundaries

`i2pr` is an experimental Rust I2P router; it is not production-ready and
must not be used for anonymity or security-sensitive workloads. Source lives
under `crates/`: `i2pr-proto` (bounded wire codecs), `i2pr-crypto`,
`i2pr-storage`, runtime-neutral `i2pr-core`, runtime-neutral `i2pr-transport`,
Tokio-free `i2pr-transport-ntcp2`, Tokio-owning `i2pr-runtime`, the
`i2pr-daemon` composition root, and test-only `i2pr-testkit`. Integration tests
are in crate `tests/` directories; sanitized I2NP fixtures are in
`tests/fixtures/i2np/`; NTCP2 crypto fixtures are under
`tests/fixtures/ntcp2/crypto/`; the opt-in nightly fuzz workspace is `fuzz/`.

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
Plan 032 adds only runtime-neutral cryptographic composition and deterministic
vectors; it does not authorize complete NTCP2 handshake, socket, or
public-network testing.
Plan 033 adds only runtime-neutral bounded SessionRequest, SessionCreated, and
SessionConfirmed codecs, consuming initiator/responder states, replay/skew
policy seams, RouterInfo/static-key binding, and explicit action/result APIs.
It does not authorize Tokio, sockets, data frames, NetDB mutation, capability
advertisement, mixed-router testing, or public-network traffic. Preserve the
message-1 cipher owner through SessionConfirmed part one, reject unknown
blocks, malformed trailing data, and mismatched padding, and keep the support
ledger non-advertised.

Plan 034 adds only runtime-neutral authenticated NTCP2 data frames and bounded
payload blocks. Authenticate the complete ciphertext before iterating blocks or
skipping unknown types; deobfuscate and validate the two-byte length before any
frame allocation. Keep transmit and receive cipher/length owners separate,
advance counters once per accepted frame, make authentication and malformed
block failures terminal, and never emit the forbidden nonce value `2^64 - 1`.
Bound block counts, unknown bytes, options, RouterInfo, I2NP, padding, and
termination metadata. Preserve consuming I2NP ownership on outbound handoff,
redact inbound payloads from `Debug` and errors, and release every frame owner
on success, failure, drop, truncation, and cancellation. Use deterministic
partial-I/O and fuzz tests only in `i2pr-testkit`/the nightly fuzz workspace.
Plan 034 does not authorize sockets, Tokio, manager queue policy, NetDB
mutation, RouterInfo publication, capability advertisement, or public-network
testing. The current NTCP2 specification defines no in-session periodic rekey
threshold; counter exhaustion is terminal and a fresh Noise handshake is the
rekey mechanism until a later compatibility plan says otherwise.

Plan 035 is the first socket-owning phase. Keep every `TcpListener`,
`TcpStream`, Tokio task/channel/timer, deadline, admission lease, replay-cache
owner, and link reader/writer child in `i2pr-runtime`; `i2pr-transport` and
`i2pr-transport-ntcp2` remain runtime-neutral. Listener and dial configuration
must be explicit and disabled by default outside controlled loopback/private
tests. Enforce global, per-IP, and IPv4/IPv6 subnet handshake limits before
cryptography, bounded active-link and queue/byte limits, typed cancellation and
deadline outcomes, and deterministic backoff/replay expiry. Authenticate before
parsing data frames, transfer directional key owners exactly once, and join both
link children before reporting closure. Never mutate RouterInfo or NetDB, infer
an external address from one peer, publish an address automatically, or claim
mixed-router interoperability or capability advertisement from local TCP tests.
Default events and snapshots omit raw endpoints, peer hashes, keys, transcripts,
payloads, and arbitrary OS error text. Use paused Tokio time/manual clocks and
loopback-only sockets for runtime tests; malformed or stress traffic belongs only
to the deterministic testkit or an authorized isolated testnet.

Plan 036 is validation and closure only. The controlled interoperability lane
is under `tests/integration/ntcp2/`; it is manual, loopback/private-network
only, reseed/bootstrap-disabled, disposable-identity-only, and must record
reference versions, artifact/configuration hashes, typed outcomes, and
sanitized evidence. The checked-in preflight and fixed-seed local campaigns
are not mixed-router evidence. Until a complete wire-level runtime adapter and
authorized Java I2P/i2pd runs exist in both directions, keep every NTCP2 ledger
row `experimental` and `advertised = false`, keep live daemon activation
disabled, and record the blocker rather than inferring interoperability.

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

NTCP2 secret owners must remain non-cloneable, non-debuggable, and zeroizing;
the forbidden nonce value `2^64 - 1` must never be emitted. Keep the
SessionRequest cipher owner needed by SessionConfirmed part one explicit and
consuming. Store NTCP2 static key/IV material only through the separate
versioned `i2pr-storage` record; never derive it from or overwrite the router
identity record. Committed NTCP2 vectors must be synthetic, provenance-recorded,
hash-manifested, and consumed by tests. Run
`bash scripts/check-ntcp2-vectors.sh` when they change. Local crypto vectors are
not interoperability evidence and must not advance support or capability
advertisement claims.

## Commits and Pull Requests

Use focused imperative commit subjects, for example
`docs: streamline repository guidelines`. PRs should explain scope, changed
files, test commands and results, dependency/security decisions, deviations,
and known limitations. Milestone closure work must leave an explicit closure
record with that evidence.

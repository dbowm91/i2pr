# Contributing to i2pr

`i2pr` is an experimental, clean-room Rust router project. Contributions must
preserve `GUARDRAILS.md`, the active plan, and the architecture decisions in
`docs/adr/`.

## Before changing code

Read, in order:

1. `README.md`.
2. `GUARDRAILS.md`.
3. The relevant roadmap or detailed plan under `plans/`.
4. Relevant ADRs and protocol dossiers.

Protocol changes require a plan covering acceptance criteria, limits, negative
tests, dependency changes, security implications, sources, and documentation.
Do not add empty future crates or claim interoperability without evidence.
The current common-structure subset in `i2pr-proto` is structural only: keep
signed byte regions intact, use the pinned source ledger, and leave freshness
policy, transport interpretation, and LeaseSet2-family behavior to their later
plans. Plan 013's concrete Ed25519/X25519 wrappers and private identity store
must remain outside `i2pr-proto`; update ADRs and the support ledger when
crypto/storage scope changes.

## Local quality checks

Run from the repository root:

```text
cargo fmt --all --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
cargo deny check advisories bans sources
```

The CI matrix covers Linux and macOS. Dependency downloads may require network
access in restricted environments; do not weaken checks to work around that.

Runtime supervision tests belong in `i2pr-runtime` and must use
`#[tokio::test(start_paused = true)]` or explicit `tokio::time::advance` for
deadlines and restart backoff. Exercise cancellation before and during waits,
readiness, panic classification, restart exhaustion, graceful shutdown, forced
abort, child-scope cleanup, and the zero-remaining-task report. Do not use
wall-clock sleeps or live sockets in this milestone. Run the focused lane with
`cargo test -p i2pr-runtime --all-targets` in addition to the workspace checks.

Bounded communication and resource-governor tests must cover capacities of one,
exact offered load, and maximum-plus-one offered load. Test typed full,
deadline, cancellation, closure, response-drop, and resource-denial outcomes;
verify queue-held leases release on receive, drop, timeout, cancellation,
panic unwind, and supervisor teardown; and exercise atomic bundle denial with
no partial usage. Use deterministic Tokio time and explicit reproducibility
seeds where scheduling or accounting is exercised. Do not use wall-clock
sleeps, unbounded retry loops, or live network traffic for overload tests.

Run `bash scripts/check-fixture-manifest.sh` after changing committed fixture
bytes. The maintained fuzz workspace under `fuzz/` is intentionally outside
the production workspace and requires nightly `cargo-fuzz`; use
`bash scripts/fuzz-smoke.sh` for bounded local smoke runs. Fuzz-only
dependencies must not be added to production manifests.

Committed protocol fixtures must be sanitized, locally authored or provenance-
recorded, free of private keys/live identities/addresses/destinations, and
listed with classification, expected type or error category, exact source
revision, generator/input, license note, SHA-256, and independence status.
Fixture-backed tests must consume the bytes. Secret-bearing protocol values
must use narrow non-cloneable, zeroizing owners with redacted `Debug`; memory
hygiene does not imply encrypted-protocol support.

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

## Dependencies, provenance, and commits

Keep dependencies focused, centralize workspace versions, review transitive
impact and unsafe-code exposure, and record why a new dependency is needed.
Do not copy implementation code or test vectors from another router until
license and provenance review is complete. The project license is intentionally
not selected yet.

Make focused commits that explain behavior and tests. Handoffs should list
changed files, commands and results, dependency changes, security-relevant
decisions, deviations, and remaining risks.

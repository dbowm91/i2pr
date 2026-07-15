# Plan 025 targeted corrective closure

- Status: Complete for the bounded, non-networked corrective scope
- Date: 2026-07-15
- Plan: `plans/025-m2-targeted-corrective-closure.md`

## Implementation commits

Implementation and documentation commit: `4fbf2c5`

## Changed files

- `.github/workflows/ci.yml`
- `AGENTS.md`, `CONTRIBUTING.md`, `GUARDRAILS.md`, `README.md`
- `crates/i2pr-core/src/lib.rs`
- `crates/i2pr-proto/src/lib.rs`
- `crates/i2pr-proto/src/common/{mod.rs,date.rs,hash.rs,keys.rs,mapping.rs,certificate.rs,identity.rs,router_address.rs,router_info.rs,lease.rs}`
- `crates/i2pr-proto/src/i2np/{mod.rs,header.rs,message.rs,netdb.rs,delivery.rs,tunnel.rs,deferred.rs}`
- removed `crates/i2pr-proto/src/common_impl.rs` and `crates/i2pr-proto/src/i2np_impl.rs`
- `crates/i2pr-runtime/src/{context.rs,supervisor.rs}`
- `docs/adr/0008-runtime-supervision-and-cancellation.md`
- `docs/adr/0009-runtime-observability-and-validation.md`
- `docs/architecture.md`, `docs/security-model.md`
- `plans/020-milestone-2-closure.md`
- `scripts/check-runtime-boundaries.sh`, `scripts/check-fixture-manifest.sh`

No fixture bytes, protocol-support claims, dependencies, or repository-local
skills changed. There is no repository `architecture/` or skills directory;
the applicable architecture contract is `docs/architecture.md` and the
repository rules are in `AGENTS.md`.

## Final ownership and completion semantics

Each active service manager retains one bounded owner slot for its
`ChildScope`. Normal completion cancels and joins that scope. If the manager
misses the shutdown deadline, the supervisor first aborts and joins manager
handles, then aborts and drains the exact retained child collection. Scope
`Drop` remains synchronous abort-only and never decrements counters or claims
termination. A bounded drain that cannot confirm all joins reports
`ShutdownOutcome::FailedCleanup`, remaining-child evidence, and a nonzero
cleanup-failure count. Final task counters and snapshots reach zero only after
confirmed joins.

Service result classification is centralized and cancellation-aware:

| Service result/state | Classification |
| --- | --- |
| `RequestedShutdown`, no relevant cancellation | `UnexpectedCleanExit`; essential fails, restartable uses restart policy, degradable/optional record typed failure/degradation |
| `RequestedShutdown`, service/manager/root cancelled | clean `RequestedShutdown` |
| `Completed`, no cancellation | `UnexpectedCleanExit` |
| `Completed`, cancellation observed | cooperative requested shutdown (documented and tested) |
| `Failed` | typed service failure |
| panic | typed panic; payload discarded |

## Protocol module layout

Implementation bodies now live in the domain-owned modules under
`crates/i2pr-proto/src/common/` and `src/i2np/`. Common modules own dates,
hashes, keys, mappings, certificates, identities, router addresses, router
information, and leases. I2NP modules own headers, message dispatch, NetDB,
delivery, tunnel, and deferred bodies. Private helpers use sibling-scoped
visibility; the crate-root re-export façade is unchanged. Exact signed
regions, encoded bytes, bounds, error categories, fixtures, fuzz imports, and
support claims are unchanged. The two compatibility warehouse files were
removed.

## Resource accounting

`ResourceUsage` now exposes a bounded `release_underflow` signal. A release
larger than current usage increments that saturating counter and clamps usage
to zero without wrapping or panicking, including during cleanup. Valid lease,
consuming-release, bundle, panic-unwind, and concurrent paths leave the signal
at zero. A private test hook proves invalid release is visible without
creating a public arbitrary-release API.

## Tests and deterministic repetition

- 31 `i2pr-runtime` tests, including 100 forced-child cleanup iterations.
- 14 `i2pr-core` tests, including underflow fault visibility.
- 44 `i2pr-proto` tests, with fixture-backed positive and malformed coverage.
- 15 `i2pr-testkit` tests and the existing 32-seed deterministic matrix.
- Focused repetition command: `rtk cargo test -p i2pr-runtime forced_child_cleanup_is_repeatably_joined -- --test-threads=1` — 1 test passed; the test repeats the scenario 100 times.

## Local quality evidence

All commands below passed from the repository root on 2026-07-15:

```text
rtk cargo fmt --all --check                              PASS
rtk cargo check --workspace                              PASS
rtk cargo check --workspace --all-targets                PASS
rtk cargo test --workspace                               PASS: 131 tests
rtk cargo test -p i2pr-runtime --all-targets             PASS: 31 tests
rtk cargo test -p i2pr-testkit --all-targets             PASS: 15 tests
rtk cargo clippy --workspace --all-targets --all-features -- -D warnings PASS
RUSTDOCFLAGS="-D warnings" rtk cargo doc --workspace --no-deps PASS
rtk bash scripts/check-dependency-direction.sh            PASS
rtk bash scripts/check-runtime-boundaries.sh              PASS
rtk bash scripts/check-fixture-manifest.sh                PASS
rtk cargo deny check advisories bans sources              PASS (pre-existing duplicate rand_core warning)
rtk cargo +1.85.0 check --workspace --all-targets         PASS
rtk cargo +nightly check --manifest-path fuzz/Cargo.toml --offline --all-targets PASS
CARGO_NET_OFFLINE=true rtk bash scripts/fuzz-smoke.sh      PASS: all bounded targets
rtk git diff --check                                     PASS
```

`cargo public-api` was not available in the environment (`cargo-public-api`
was not found), so public API comparison used the crate-root export façade,
workspace compilation, rustdoc, tests, and fixture/fuzz lanes.

## CI evidence

The workflow runs workspace all-target checks on both quality platforms and the
runtime-boundary and fixture-manifest gates on Linux. Fresh post-push run
[`29416020928`](https://github.com/dbowm91/i2pr/actions/runs/29416020928)
passed all required jobs: Quality (Ubuntu), Quality (macOS), MSRV (Ubuntu),
and Dependency policy. MacOS retained the general matrix because the fixture
gate uses Linux Bash associative arrays.

```text
29416020928: PASS
```

## Deviations, dependency and security decisions

- No dependency or protocol-support ledger change was needed.
- No sockets, DNS, reseeding, transport, NetDB, tunnel, client/API, plugin,
  listener, capability advertisement, or public-network traffic was added.
- Forced cleanup is bounded by the existing supervisor deadline plus a fixed
  bounded child-drain poll budget; an unconfirmed child drain is failure
  evidence rather than a false successful zero.
- The resource underflow signal is typed aggregate metadata only; it retains no
  call-site text, backtrace, payload, secret, address, or peer label.
- Fuzz smoke may generate local corpus mutations; those generated artifacts
  were removed and no committed fixture bytes changed.

## Known limitations and Milestone 3 readiness

This closure proves only local bounded lifecycle, accounting, codec ownership,
and deterministic test evidence. It does not prove transport interoperability,
anonymity, resilience, authentication, mixed-router behavior, or production
readiness. Milestone 3 transport planning may begin only after this record,
the final local matrix, and a fresh remote CI run are recorded; transport
implementation itself remains outside Plan 025.

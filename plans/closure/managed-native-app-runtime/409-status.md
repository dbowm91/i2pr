# Plan 409 closure — External Rust managed-app SDK and package builder

Status: **passed-external-rust-managed-app-sdk-and-package-builder**

Plan: `plans/implementation/managed-native-app-runtime/409-external-rust-managed-app-sdk-and-package-builder.md`.

Date: 2026-10-09

Implementation commits:

- `b365aaca` — public protocol package metadata, application SDK, deterministic package builder, external-consumer qualification harness, and architecture/boundary documentation.
- `fa991b28` — preserve partially written SDK frames across future cancellation, resume pending output before reads/writes, and prevent request-id wraparound.
- `94c8e64d` — make the out-of-workspace consumer use the exact pushed public Git revision and lock its dependency graph.

## Result

The repository now provides three public application-side crates at version
`0.1.0`: `i2pr-app-proto`, `i2pr-app-sdk`, and `i2pr-app-package-build`.
The SDK is runtime-neutral by default; its optional `tokio-adapter` feature
provides generic async byte-stream framing and session operations for hello,
effective host capabilities, SAM/I2CP logical streams, local-service publish /
unpublish, control events, send/receive, and close. It owns no listener,
network connector, grant, manager protocol, or router state. Bounded queues and
stream counts follow protocol ceilings. Cancelled partial writes retain their
frame and offset in the session and resume in order; request ids fail closed at
exhaustion instead of wrapping.

The package builder creates deterministic Stored-only `.i2prapp` archives from
manifest bytes, a payload tree, and an Ed25519 key. It rejects traversal,
colliding names, symlinks and special files, enforces package bounds, includes
executable declarations, derives and checks publisher identity, and verifies
its result before success. Integration evidence passes its output through the
router's canonical verifier and immutable package-store installation.

The separate fixture at `tests/external-app-consumer/` is outside the workspace
and uses only the public SDK and builder at pinned Git revision
`fa991b28171ea68251d6e94bd1a1d04c09117c9c`, plus its selected runtime and
crypto dependencies. It builds a signed package and an app binary. The real
Plan-383 persistent-catalog qualification launched that binary through appd
and apphost and completed both SAM and I2CP exchanges after manager restart.

The crates are packageable and Git-consumable. They were **not published to
crates.io**; publication was not required for this pre-publication contract and
no registry release was requested. No AppManager/admin/store authority was
made public, and no protocol-support, release, or containment claim was
promoted.

## Requirement-to-evidence matrix

| Requirement | Evidence |
|---|---|
| Public SDK uses the frozen application contract, exposes only effective host capabilities, and handles v1.1 local-service messages | `cargo test --locked --workspace --all-targets -- --test-threads=1`; SDK tests; Plan-408 `session_contract` tests, including v1.0 compatibility and local-service streams |
| Logical service streams stay bounded and isolated; cancellation preserves frame ordering | `cargo test --offline -p i2pr-app-sdk --all-features --all-targets`; app-manager and apphost boundary suites; SDK write state retains partial bytes/offset and drains them before subsequent output or input |
| Builder output is deterministic, bounded, signed, and valid under the router's canonical format | `cargo test --locked -p i2pr-app-package-build --all-targets`; `cargo test --locked -p i2pr-app-package --test package_builder_interop` (includes `verify_file` and `PackageStore::install/list`) |
| Separate consumer depends on the supported public crate surface without router workspace paths | `cargo build --manifest-path tests/external-app-consumer/Cargo.toml`; lockfile resolves SDK, protocol, and builder from pinned Git commit `fa991b28` |
| Real managed-app path launches an external app and reaches router-owned SAM and I2CP services | Static PIE built with `cargo rustc --manifest-path tests/external-app-consumer/Cargo.toml --bin i2pr-external-app-consumer-fixture -- -C target-feature=+crt-static`; `file` confirmed static PIE; `I2PR_EXTERNAL_APP_BINARY=... I2PR_APP_FIXTURE_EVIDENCE_DIR=/tmp/i2pr-plan409-external-evidence cargo test --locked -p i2pr-daemon --lib app_runtime_qualification::tests::persisted_autostarts_reach_sam_and_i2cp_again_after_manager_restart -- --exact --test-threads=1` — 1 passed |
| Public crates are distributable with versioned dependency metadata and MSRV/license data | `cargo package --locked --offline -p i2pr-app-proto --allow-dirty`; corresponding `cargo package` runs for SDK and package builder with a local `[patch.crates-io]` for the not-yet-published protocol package; all three packaged and verified |
| No admin/router implementation authority crossed the crate boundary | `scripts/check-dependency-direction.sh`; `python3 scripts/check-managed-app-manager-boundary.py`; package and policy boundary checkers plus self-tests; private-client, gateway, process, and dependency checkers passed |
| Docs, crate inventory, and plan references are current | tooling inventory, license metadata, global plan-number uniqueness, ADR uniqueness, workflow validity, planning unittest discovery (51 passed), and `git diff --check` passed |

## Verification record

The following routine floor commands passed on this Linux host:

- `cargo fmt --all --check`
- `cargo check --locked --workspace --all-targets`
- `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl`
- `cargo test --locked --workspace --all-targets -- --test-threads=1` (expected environment-gated external tests remained ignored)
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps`
- `cargo test --locked --workspace --doc`
- `python3 -m unittest discover -s tests/planning -p 'test_*.py'` (51 passed)
- `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` (18 passed)
- `cargo deny check advisories bans sources`

The repository boundary/evidence checks in `AGENTS.md` were also run and
passed: dependency direction, global plan/ADR uniqueness, portable service
tunnel API and consumer, runtime/console/browser boundaries, managed-app
process/package/policy/private-client/gateway/manager checks (including package,
policy, and process self-tests), service-tunnel boundaries, M11 composition,
fixture manifest, NTCP2/SSU2/I2CP vectors and interop guards, constrained-host
boundary, M11 transit boundary/evidence, SAM/SSU2/I2CP/I2PControl acceptance
evidence, ELS2 transcript/live-lane checks, encrypted-service caller, outproxy
request/wire checks, config-secret hygiene, floodfill type-5 coverage, service
tunnel/exploratory/NetDB/destination/Streaming/mixed-router evidence, M12
qualification and boundary self-tests, tooling inventory, license metadata,
and workflow validity.

The planning unittest suite initially exposed a stale mutation-test fixture
that still hard-coded the pre-SDK workspace member count. The test now mutates
the current roster heading by pattern and passes; the documented roster is
still derived from the workspace. Its workflow-validity fixture emits the
expected diagnostic when testing a temporary repository with no workflow
directory.

## Compatibility, security, and limitations

- Wire compatibility remains governed by the managed-app protocol major/minor
  negotiation. The SDK consumes v1.1 local-service additions and retains the
  v1.0 path.
- The SDK never interprets SAM/I2CP payloads, opens sockets, exposes the
  administrator protocol, or turns requested permissions into grants.
- The package signature proves key possession only. The router's verifier and
  offline operator policy remain authoritative at install/launch.
- The external fixture uses a static Linux PIE for the Secured apphost
  qualification. It does not establish cross-platform containment or a public
  support claim.
- `i2pr-app-proto`, SDK, and builder remain at `0.1.0`. Git consumers can pin
  commits before any registry publication; compatibility follows the documented
  semver and wire-version policy.

Findings by severity: **critical 0; high 0; medium 0; low 0**.

## Unblock audit

Plan 408's v1.1 protocol extension was the only registered hard/interface
dependency of Plan 409 and was closed before implementation. A fresh audit of
`plans/registry.md` and the managed-app roadmap dependency graph found no
registered plan blocked on Plan 409, so no successor status changed. The
roadmap's remaining macOS/Windows backends, live administration, brokered
clearnet, UI hosting, remote update/TUF, and scoped Proposal-170 adapter are
future work without registered implementation plans or stable contracts; this
closure does not activate or infer those plans. The external i2pr-mail M006
owner should re-audit its separate registry against the now-available pinned
public SDK; this closure does not edit or claim to unblock that other repository.

Disposition: **closed**. The reconciled imported-plan sequence 407–409 is
complete. The workstream still has separately scoped future capabilities and
is not globally complete.

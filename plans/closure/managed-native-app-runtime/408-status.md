# Plan 408 closure — Host-owned loopback local-service ingress

Status: **passed-host-owned-loopback-local-service-ingress**

Plan: `plans/implementation/managed-native-app-runtime/408-host-owned-local-service-ingress.md`.

Date: 2026-10-09

## Result

Implemented the managed-app v1.1 `local_service` capability and typed
publish/unpublish/incoming-stream transactions. The daemon owns the sole
loopback listener (`127.0.0.1`); appd remains socket-free and receives only
typed manager messages over its inherited anonymous channel. Grants remain
administrator-origin, v1.0 remains compatible, and accepted service streams
use bounded per-stream queues and the existing managed-app stream lifecycle.
ADR 0039 records the ownership decision. The manager boundary checker allows
only the exact daemon socket surface and keeps appd free of sockets.

## Acceptance evidence

| Criterion | Evidence |
|---|---|
| Versioned capability/request/reply/event contract with v1.0 compatibility | `cargo test --locked -p i2pr-app-proto --all-targets`; `cargo test --locked -p i2pr-app-manager-proto --all-targets`; `cargo test --locked -p i2pr-appd --test session_contract -- --test-threads=1` |
| Admin-only grants and persistent policy | `cargo test --locked -p i2pr-app-state --all-targets` |
| Daemon-owned loopback binding, conflict rejection, bounded accepts, data path, and teardown | `cargo test --locked -p i2pr-daemon app_manager_bridge::tests --lib -- --test-threads=1` (21 passed) |
| Boundary remains fail-closed | manager boundary and self-test; package/policy boundary checks and self-tests; private-client and gateway boundary checks all passed |
| Workspace behavior and quality | `cargo test --locked --workspace --all-targets -- --test-threads=1`; `cargo check --locked --workspace --all-targets`; managed-app sibling build; clippy with all features and `-D warnings`; rustdoc with warnings denied; workspace doc tests; formatting check all passed |
| Planning and repository metadata | planning unittest discovery (51 passed); workflow validity, global plan and ADR uniqueness, tooling inventory, license metadata, and `git diff --check` passed |

The planning test suite emits its expected fixture diagnostic for a temporary
repository without `.github/workflows`; the suite exits successfully.

## Security and scope

No application receives a socket or host networking authority. Wildcard and
non-loopback binds are not exposed. Queue exhaustion closes only the affected
service stream. No protocol support or release status was promoted.

## Unblock audit

Plan 409's only hard dependency on Plan 408, the app protocol minor-version
extension, is now closed. Plan 409 is moved to ready and activated by the
closure/unblock commit. No other successor directly blocked on Plan 408 was
found. Archived imported drafts 385–388 retain their collision-safe
reconciliation; active work is represented by Plans 407–409.

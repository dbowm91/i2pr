# Plan 323 status — Canonical TunnelManager non-deep parity

Status: **`passed-prop170-tunnelmanager-canonical-nondeep-parity`**.

Plan of record: [`plans/implementation/i2pcontrol-proposal-170/323-tunnelmanager-canonical-nondeep-parity.md`](../../implementation/i2pcontrol-proposal-170/323-tunnelmanager-canonical-nondeep-parity.md).

Implementation commit: `3dddc94` (`Implement TunnelManager non-deep parity`).

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Canonical top-level Proposal request envelope and typed options | `decode_tunnel_request` maps canonical `DelayOpen` and the existing exact fields to typed options; the contract test covers the canonical request; TunnelManager wire tests cover Get/result shape, conflicts, validation, and secret redaction. | PASS |
| Bounded bulk actions and ownership provenance | Existing All lifecycle path uses a sorted bounded snapshot of control-owned definitions, returns individual outcomes, and excludes startup-owned definitions. `plan323_all_lifecycle_uses_sorted_control_owned_snapshot` and the canonical TunnelManager wire suite pass. | PASS |
| Named operational owner or explicit disposition for every option/type pair | `proposal_tunnel_manager_matrix()` covers every canonical option for all twelve tunnel types. The matrix test asserts complete cardinality, exact type coverage, and zero `OwnerGap` cells. Plans 324, 326, and 327 own the explicit deep prerequisites. | PASS |
| Non-deep client and server behavior | The plan records typed runtime owners for client proxy/auth/filter options, HTTP request/presentation/access controls, rate/POST admission, destination persistence/rotation, confined logical key references, and service idle controls. Unit and local product tests in the plan exercise those paths. | PASS |
| DelayOpen lifecycle behavior | Startup defers all-delay client Destination groups; the first local client connection sends a bounded activation request to the product coordinator, which provisions that exact group using the existing owner. Cancellation, timeout, shared-group consistency, readiness isolation, and fail-closed activation are covered by three daemon tests. | PASS |
| All twelve backend paths remain real | Full workspace all-target test run passed, including the Proposal family lifecycle-over-wire coverage and all current backend product suites. | PASS |
| Secret, filesystem, and lifecycle boundaries | PrivKeyFile remains a confined logical reference; FilterFilePath uses bounded no-follow root-relative opens; proxy passwords remain redacted; queues, activation deadlines, and service lifecycle ownership are bounded. Boundary scripts, Clippy, and focused behavior tests pass. | PASS |

## Verification

| Command | Result |
|---|---|
| `rtk cargo fmt --all --check` | PASS |
| `rtk cargo check --locked --workspace --all-targets` | PASS |
| `rtk cargo test --locked --workspace --all-targets -- --test-threads=1` | PASS — 3,775 passed, 35 ignored, 0 failed (133 suites) |
| `rtk cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS |
| `RUSTDOCFLAGS='-D warnings' rtk cargo doc --locked --workspace --no-deps` | PASS |
| `rtk cargo test --locked --workspace --doc` | PASS |
| `rtk cargo test --locked -p i2pr-daemon --test i2pcontrol_tunnels -- --test-threads=1` | PASS — 17 tests |
| `rtk cargo test --locked -p i2pr-i2pcontrol --lib proposal_tunnel_matrix -- --test-threads=1` | PASS — 1 test; zero owner gaps |
| `rtk cargo test --locked -p i2pr-i2pcontrol --test contract plan289_tunnel_request_envelope_rules -- --test-threads=1` | PASS |
| `rtk cargo test --locked -p i2pr-daemon --lib delay_open -- --test-threads=1` | PASS — 2 tests |
| `rtk bash scripts/check-dependency-direction.sh` | PASS |
| `rtk bash scripts/check-runtime-boundaries.sh` | PASS |
| `rtk bash scripts/check-service-tunnel-boundaries.sh` | PASS |
| `rtk bash scripts/check-fixture-manifest.sh` | PASS |
| NTCP2, SSU2, I2CP vector and interoperability scripts | PASS |
| Constrained-host, M11 boundary/qualification, SAM/SSU2/I2CP/I2PControl/service-tunnel acceptance, exploratory/netdb/destination/streaming/M6 evidence scripts | PASS; streaming checker emitted its existing guarded-label warnings and reported success |
| `rtk bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test` | PASS — expected negative fixtures were rejected; self-test exit status 0 |
| `rtk python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` | PASS — 18 tests |
| `rtk cargo deny check advisories bans sources` | PASS — existing duplicate-version warnings only; advisories, bans, and sources passed |
| `rtk python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `rtk git diff --check` | PASS |

The documentation build initially exposed that the direct `rustix` dependency enabled `fs` without `std`, relying on test-only feature unification. The dependency now explicitly enables both features; the doc build, workspace check, and Clippy passed afterward. The full all-target test run preceded this feature-only correction; the correction does not change runtime logic.

## Migration, security, and limitations

- Canonical `DelayOpen` now controls local client Destination-group activation. It does not claim per-service I2CP sessions; it is implemented at the available product lifecycle boundary and starts exact configured destination groups on demand.
- Proposal deep cryptographic/outproxy fields remain explicitly owned by Plans 324, 326, and 327; no unsupported crypto or direct-clearnet behavior is introduced.
- New direct dependency: `rustix` 1.1.4 with `fs` and `std` features, already present transitively in the lockfile through `tempfile`. No unsafe code or protocol advertisement changes.
- No migration of user state is required. Existing control generations continue to use the current typed/persisted definitions.
- Findings: critical 0, high 0, medium 0, low 0.

## Roadmap disposition and unblock audit

Plan 323 is closed as **`passed-prop170-tunnelmanager-canonical-nondeep-parity`**. Plans 324 and 327 had no remaining hard dependency after this closure and are moved from blocked to **ready**. Plan 326 remains blocked on Plan 324 and the independently blocked Plan 325 provider qualification. Plan 322 remains active; Plan 328 remains blocked on Plans 322, 326, and 327. Plan 325's no-qualified-provider result is unchanged. M12/mainline readiness and the historical qualified-profile claim are unchanged.

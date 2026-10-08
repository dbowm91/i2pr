# Plan 374 closure — persistent managed-app policy, production catalog, and offline administration

Status: **passed-managed-app-persistent-policy-production-catalog-and-offline-administration**.

Classification: **invariant + capability + persistence/lifecycle**. This closes
local administrator policy, offline package administration, and restart-safe
production catalog selection. Secured containment, live administrator IPC, and
remote package distribution remain outside the milestone.

Plan: `plans/implementation/managed-native-app-runtime/374-persistent-managed-app-policy-production-catalog-and-offline-administration.md`.

Implementation commit: `23c16b8` (`feat(app-runtime): add signed packages and persistent policy`).
Plan 374 was activated after Plan 373 closed (`b477edf`). Plan 373's package
foundation closure is `plans/closure/managed-native-app-runtime/373-status.md`.

## Requirement-to-evidence matrix

| Requirement | Evidence |
| --- | --- |
| Persistent generations, locking, and fail-closed recovery | `i2pr-app-state` uses generation transactions and an OS file lock; malformed or inconsistent highest state fails closed. Unit tests cover transaction recovery, generation selection, malformed state, and lock contention. |
| Exact trust, grant, selection, and profile authority | Publisher trust is the exact key fingerprint; grants bind publisher and AppId; only Sam/I2cp are grantable; an exact verified package identity is required. Selection does not auto-trust or auto-grant. Authority has private construction and no decoder; boundary checker/self-test covers the authority seam. |
| Offline administration | `i2pr-appctl` provides local package and policy administration. Runtime lock ownership prevents mutation while appd is active. CLI documentation covers trust semantics, unsafe-direct networking, restart activation, and no automatic restart on application exit. |
| Production launch catalog and process boundary | `i2pr-appd` uses `PersistentLaunchCatalog`, re-verifies selected packages before authority construction, consumes one immutable policy snapshot, and holds the runtime lock. The daemon passes only the canonical managed-app root after clearing the environment. |
| Restart-safe operator-approved launch | Real appd/apphost qualification installs signed test packages, records explicit trust/grants/exact selection/profile/autostart, and proves two approved apps reach private SAM/I2CP after each of two appd starts with fresh OS-random instance IDs. A tampered selected app is refused without blocking valid siblings. |
| Deferred capabilities stay unavailable | Secured launches are refused before exec; unsafe-direct requires explicit acknowledgement and is described as ordinary host networking; no live admin endpoint, updater, relaunch policy, sandbox, broker, UI host, or Proposal-170 work was added. |
| Durable contract and operator documentation | ADR 0037, `specs/references/managed-app-policy-v1.md`, crate architecture pages, security model, README, AGENTS routine floor, dependency map, and roadmap updated. `specs/support.toml` remains unchanged; no protocol-support promotion is made. |

## Verification

All results below are **local** unless identified otherwise. No exact-head CI
run was available for this branch.

- `cargo fmt --all --check` — passed.
- `cargo check --locked --workspace --all-targets` — passed.
- `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` — passed.
- `cargo test --locked --workspace --all-targets -- --test-threads=1` — passed, **4,618 passed, 35 ignored**, 180 suites.
- Focused app-runtime qualification after building the real sibling binaries — passed, **22 passed**.
- `cargo test --locked -p i2pr-daemon --lib app_runtime_qualification::tests::persisted_autostarts_reach_sam_and_i2cp_again_after_manager_restart -- --exact --test-threads=1` — passed, **1 passed** after the final test-fixture permission adjustment.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` — passed.
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` — passed.
- `cargo test --locked --workspace --doc` — passed.
- `cargo test --locked -p i2pr-appd --all-targets -- --test-threads=1` — passed, **57 passed**.
- `python3 -m unittest discover -s tests/planning -p 'test_*.py'` — passed, **51 tests**.
- `python3 scripts/check-global-plan-number-uniqueness.py` and `python3 scripts/check-adr-number-uniqueness.py` — passed.
- `python3 scripts/check-tooling-inventory.py` — passed.
- Managed-app package, policy, and process boundary checks plus their required self-tests; gateway/manager seam checks; dependency/runtime/console/service-tunnel guards; evidence/vector checks; workflow validity; and `cargo deny check advisories bans sources` — passed locally.

The full workspace suite preceded a final test-only Unix permission-mode
hardening. The exact production-catalog black-box case, clippy, rustdoc, and
planning/boundary checks were run after the final code adjustment. No external
interop lane was required or run.

## Compatibility, security, and limitations

Policy is local, persistent, and offline. Operators stop the app runtime/router
before changing trust or launch policy; changes take effect on the next start.
The persistent state format is versioned and malformed highest-generation
state fails closed. Package signatures still prove only publisher-key
attribution and integrity; the separate local trust decision is required.

`UnsafeDirect` is ordinary host networking without sandboxing. It has no
containment claim. `Secured` refuses before exec because there is no qualified
backend. Application exit is not automatically restarted. A broken app runtime
remains optional to router startup. No support inventory or advertisement
changed.

Findings: critical none; high none; medium none; low none.

## Unblock audit and roadmap disposition

Registry and roadmap audit: Plan 374 was the only registered successor in the
managed-app package/lifecycle line, and it is now closed. No other registered
managed-app plan is blocked on Plan 374 or newly eligible. OS-specific Secured
backends, live AppManager administration, brokered clearnet, UI hosting,
remote update/TUF, and scoped Proposal 170 remain downstream concepts without
registered implementation plans; this closure does not create one or claim
that they are ready. They require separate plans and platform/contract decisions.
The router protocol milestones and other workstreams are unaffected.

Disposition: **closed**. The package and persistent-policy/catalog sequence is
complete through Plan 374; further managed-app capabilities require a new
plan-of-record.

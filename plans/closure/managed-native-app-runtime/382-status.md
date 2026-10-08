# Plan 382 closure — signed immutable managed-app package and local store foundation

Status: **passed-managed-app-signed-package-store-foundation**.

Identity note: this work was initially registered on its feature branch as
Managed native app runtime Plan 373. The global ownership audit found that
Proposal 170 already owns Plan 373, so this authority is identified as Plan 382
before integration. The original implementation commit remains unchanged.

Classification: **invariant + infrastructure**. This milestone establishes
cryptographic package identity and immutable local storage. It does not grant
publisher trust, application capabilities, selection, or launch authority.

Plan: `plans/implementation/managed-native-app-runtime/382-signed-immutable-managed-app-package-and-local-store-foundation.md`.

Implementation commit: `23c16b8` (`feat(app-runtime): add signed packages and persistent policy`).
The implementation commit contains the sequentially coupled Plans 382 and 383
vertical slice; this record closes only the Plan 382 package/store requirements.

## Requirement-to-evidence matrix

| Requirement | Evidence |
| --- | --- |
| Signed v1 package identifies publisher, app, version, exact manifest and inventory | `i2pr-app-package` signs and verifies the canonical package identity and SHA-256 inventory with Ed25519; unit tests cover round trips, invalid signatures, identity mismatch, and inventory mismatch. |
| Strict bounded archive parsing | Stored-only ZIP profile, checked sizes and counts, exact consumption, path validation, and raw central-directory duplicate-name rejection; package tests cover malformed, duplicate, traversal, unsupported, oversized, and truncated inputs. |
| Safe immutable local installation | Private staging, verification before promotion, content-addressed package paths, atomic install, and bounded list/verify/remove operations; store tests cover collision and failure cleanup. |
| Package layer has no policy or launch authority | Package crate owns only package verification and storage; policy, grants, catalog, and process launch live in separate crates. Dependency-direction and package-boundary guards passed. |
| Package format and ownership are documented | ADR 0036, `specs/references/managed-app-package-v1.md`, crate architecture page, dependency map, and security model updated. |

## Verification

Local verification on the implementation commit:

- `cargo fmt --all --check` — passed.
- `cargo check --locked --workspace --all-targets` — passed.
- `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` — passed.
- `cargo test --locked --workspace --all-targets -- --test-threads=1` — passed, **4,618 passed, 35 ignored**, 180 suites.
- Focused production app-runtime qualification — passed, **22 passed**; Plan 383's production-catalog restart test was also rerun after its final test-fixture permission adjustment and passed (**1 passed**).
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` — passed.
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` — passed.
- `cargo test --locked --workspace --doc` — passed.
- `python3 -m unittest discover -s tests/planning -p 'test_*.py'` — passed, **51 tests**.
- Dependency, package/policy/process boundary checks and self-tests, runtime/console/service-tunnel guards, evidence and vector checkers, workflow validity, tooling inventory, and `cargo deny check advisories bans sources` — passed locally.

The full workspace test preceded a final test-only Unix permission-mode
hardening; the exact production-catalog qualification, clippy, and rustdoc were
rerun after that adjustment. No external interop lane was required or run.

## Compatibility, security, and limitations

The `.i2prapp` v1 format is a new local artifact format. Installation is
content-addressed and immutable through the store API; package signatures prove
publisher-key attribution and payload integrity only. Trust and capability
decisions remain explicit Plan 383 policy. Same-user/OS-administrator mutation
is outside the store isolation threat model; selected packages are reverified
by the policy catalog before authority construction.

No network client, TUF/update path, sandbox, advertised capability, or
production support claim was added. Managed applications remain experimental
and disabled by default. Findings: critical none; high none; medium none; low
none.

## Unblock audit and roadmap disposition

Registry audit: Plan 383 was the only registered managed-app plan with Plan 382
as a hard dependency. Plans 369–371 are closed, ADRs 0032/0035 and the Plan 382
package/store API are stable, and the Plan 382 evidence above closes its sole
gate. Plan 383 is therefore moved from blocked to ready in this commit. No
other registered plan is newly unblocked. OS sandboxing, live administration,
brokered clearnet, UI hosting, remote update/TUF, and scoped Proposal 170 remain
future plan-of-record work.

Disposition: **closed**. Plan 383 is ready for sequential execution.

# Plan 349 closure — managed app v1 direction, broker, and network-policy corrective

Status: **passed-managed-app-v1-direction-broker-network-policy-corrective**.

Classification: **corrective invariant + infrastructure**. This closes the pre-runtime
contract corrections found after Plan 345. It establishes a stable contract for future
planning only; it adds no process launcher, sandbox, network broker, router adapter, or
user-visible application capability.

## Commits

- `cab3060` — first corrective contract freeze and qualified global plan-number collision record.
- `afec3ee` — final directional wire freeze, correlation, error, and address-classification semantics.
- `3bdd780` — directional app-proto contract, policy implementation, tests, fuzzing, architecture and guard updates.
- This closure commit — status, roadmap, and registry disposition.

Plan 345's closure record is unchanged. Its evidence remains valid for the infrastructure it
tested; Plan 349 corrects the unreleased v1 before any downstream consumer was authorized.

## Requirement-to-evidence matrix

| Requirement | Evidence |
|---|---|
| A. Hostname allow works for globally routable resolutions, while explicit denies win and non-global resolutions need explicit IP/CIDR authorization | `NetworkPolicy::evaluate_resolved_address` and focused tests in `crates/i2pr-app-proto/src/lib.rs`; frozen two-stage algorithm and examples in `specs/references/managed-native-app-runtime-v1.md`. |
| B. Global/non-global classification is explicit and fail-closed | `AddressScope` classification and boundary/property tests in `crates/i2pr-app-proto/src/lib.rs`; table, conservative treatment, and IANA registry provenance/retrieval date in the v1 reference. |
| C. Message direction, correlation, and outcomes are mechanically represented | Four disjoint direction enums/decoders, nonzero bounded `RequestId`, typed outcomes/errors, handshake role checks, request limits, and correlation tests in `crates/i2pr-app-proto/src/lib.rs`; directional wire reference; fuzz coverage in `fuzz/fuzz_targets/app_contract.rs`. |
| D. `brokered_tcp` remains reserved intent and is not generically openable or effective | Removed from `AppService`, rejected from effective grants, and statically guarded in `scripts/check-runtime-boundaries.sh`; reserved semantics in the v1 reference. |
| Runtime-neutrality and architecture boundary remain intact | `scripts/check-runtime-boundaries.sh`, dependency direction guard, and updated `docs/architecture/i2pr-app-proto.md`. No production dependency was added. |
| Number collision is reconciled without destroying historical traceability | Exact Plan 349 pair recorded in `plans/global-number-collision-ledger.md`; the uniqueness checker allows only that pair and its planning test rejects a third owner. |

## Tests and guards

All results below are local on the rebased implementation head. No hosted CI result is claimed.

Focused:

- `rtk cargo fmt --all --check` — passed.
- `rtk cargo test --locked -p i2pr-app-proto --all-targets -- --test-threads=1` — passed, 14 tests.
- `rtk cargo check --locked -p i2pr-app-proto --all-targets` — passed.
- `rtk cargo clippy --locked -p i2pr-app-proto --all-targets --all-features -- -D warnings` — passed.
- `rtk run 'RUSTDOCFLAGS="-D warnings" cargo doc --locked -p i2pr-app-proto --no-deps'` — passed.
- `rtk bash scripts/check-runtime-boundaries.sh` — passed.
- `rtk python3 scripts/check-global-plan-number-uniqueness.py` — passed.
- `rtk python3 -m unittest discover -s tests/planning -p 'test_*.py'` — passed, 7 tests.
- `rtk bash scripts/fuzz-smoke.sh` — passed all target smoke runs, including `app-contract` (32 runs per target).

The complete routine floor from `AGENTS.md` was run locally and passed:

- `rtk cargo fmt --all --check`
- `rtk cargo check --locked --workspace --all-targets`
- `rtk cargo test --locked --workspace --all-targets -- --test-threads=1` — 4,070 passed, 35 ignored, 148 suites.
- `rtk cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`
- `rtk run 'RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps'`
- `rtk cargo test --locked --workspace --doc`
- `rtk bash scripts/check-dependency-direction.sh`
- `rtk python3 scripts/check-global-plan-number-uniqueness.py`
- `rtk python3 -m unittest discover -s tests/planning -p 'test_*.py'`
- `rtk bash scripts/check-runtime-boundaries.sh`
- `rtk bash scripts/check-service-tunnel-boundaries.sh`
- `rtk bash scripts/check-fixture-manifest.sh`
- `rtk bash scripts/check-ntcp2-vectors.sh`
- `rtk bash scripts/check-ssu2-vectors.sh`
- `rtk bash scripts/check-i2cp-vectors.sh`
- `rtk bash scripts/check-ntcp2-interoperability.sh`
- `rtk bash scripts/check-constrained-host-lane-boundary.sh`
- `rtk bash scripts/check-m11-transit-boundaries.sh`
- `rtk bash scripts/check-m11-transit-qualification-evidence.sh`
- `rtk bash scripts/check-sam-acceptance-evidence.sh`
- `rtk bash scripts/check-ssu2-acceptance-evidence.sh`
- `rtk bash scripts/check-i2cp-acceptance-evidence.sh`
- `rtk bash scripts/check-i2pcontrol-acceptance-evidence.sh`
- `rtk bash scripts/check-service-tunnel-acceptance-evidence.sh`
- `rtk bash scripts/check-exploratory-tunnel-evidence.sh`
- `rtk bash scripts/check-netdb-tunnel-evidence.sh`
- `rtk bash scripts/check-destination-tunnel-evidence.sh`
- `rtk bash scripts/check-streaming-tunnel-evidence.sh`
- `rtk bash scripts/check-m6-mixed-router-acceptance-evidence.sh`
- `rtk bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test`
- `rtk python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` — passed, 18 tests.
- `rtk cargo deny check advisories bans sources`

The static evidence checks all returned exit status 0. Existing non-failing diagnostics
included fuzz manifest naming/deprecation warnings, coverage-label warnings in the streaming
evidence checker, duplicate crate-version warnings from `cargo deny`, and expected rejected
negative fixtures during the M12 self-test.

## Compatibility, security, and documentation

The corrected wire remains version 1.0 because Plan 345's v1 was an unreleased draft with no
consumer. No migration or compatibility shim is needed. The frozen address table documents
its IANA special-purpose registry provenance and retrieval date (2026-10-05); classification
is conservative, and IPv4-mapped IPv6 inherits the IPv4 classification.

The review covered DNS rebinding and mixed resolution sets, explicit IP/CIDR deny precedence,
non-global destination access, role/direction confusion, request replay/duplication and
correlation, administrator authority escalation, and accidental exposure of a live broker
grant. The implementation keeps each decision pure and bounded. App hello declares identity
but does not authenticate it; a future trusted IPC owner must bind the claimed principal.
Administrator operations are reserved and return typed unsupported-operation responses, so
they cannot silently mutate policy or lifecycle state.

Updated documentation includes the language-neutral v1 reference and
`docs/architecture/i2pr-app-proto.md`. No dependency was added. No protocol advertisement,
runtime boundary, or security claim changed. Remaining limitations are explicit: there is no
runtime owner, resolver, broker, process isolation, sandbox qualification, package lifecycle,
SAM/I2CP/Proposal 170 adapter, or anonymity guarantee.

Unresolved findings: **critical 0; high 0; medium 0; low 0** within this plan's contract scope.

## Roadmap disposition and unblock audit

Plan 349 is closed. The corrected contract is ready for bounded planning of the router
app-principal gateway and package/lifecycle + AppManager branches. This closure does not
authorize implementation or claim those capabilities. The Proposal 170 adapter remains
separately gated on canonical Proposal 170 completion. Sandbox, broker, SDK, UI host, and
cross-platform adversarial qualification remain sequenced behind their future owners.

The downstream branches were blocked only on this corrective contract and are now unblocked
for plan drafting. No other managed-native-app plan in this dependency line remains blocked
on Plan 349. The Plan 349 number collision with portable-service-tunnels remains qualified
and recorded; no existing plan was renumbered.

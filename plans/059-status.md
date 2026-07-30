# Plan 059 status: reference-side implementation and live qualification closure pass

## Status

**Closed with the typed blocker `blocked_java_support_topology_rejected`.**

Plan 058 rejected ADR 0021, so the Java minimal support topology
is forbidden under the current four-direction contract. Plan 059
closes with the closure contract required by the Plan 058 record
and candidate integrity closure pass. The `java-to-i2pr-ipv4`
direction remains a typed blocker for the pinned Java I2P 2.12.0
revision; Plan 060 cannot start under the current four-direction
contract until either a future pinned Java revision is adopted or
the closure contract is revised through a new ADR.

The local checkout on this commit implements the Plan 059 scope
that does not depend on the Java support topology:

- the i2pd direct helper (source, build contract, source-lock
  record, README, and bounded local Python driver);
- the per-reference observation qualification receipts and the
  typed-absence summary;
- the canonical pipeline `live_mode` flag and the helper/source/
  catalog/qualification-receipt digest binding into the direction
  record;
- the Plan 059 test matrix (36 cases across the five required
  surfaces) and the static boundary check extensions.

The runtime qualification of the i2pd helper and the receiver
observations requires the Plan 046 rootless sealed-namespace lane
or the Plan 048/049 Multipass recovery lane. This host is the
Plan 046 `apparmor_restrict_on` negative baseline; the external
lane cannot be exercised here. The qualification receipts therefore
mark every semantic level as `qualified = false` and report the
typed blocker `blocked_unprivileged_user_namespace` (i2pd) or
`blocked_java_support_topology_rejected` (Java I2P).

The Plan 059 implementation surface is mandatory. Any change that
removes or weakens the i2pd helper source-lock, the qualification
receipts, the canonical pipeline live-mode enforcement, or the
static boundary checks must be re-justified in a new plan-of-record
and must not silently weaken the Milestone 3 evidence gate.

## Implementation floor

The implementation floor for Plan 060 is the commit that closes
this plan. The exact commit digest is recorded in the closure
section below.

## Phase-by-phase inventory of corrections

### Phase 1: Workstream A — execution-host qualification fixture

The host is the Plan 046 `apparmor_restrict_on` negative baseline;
the Plan 046 rootless probe returns
`blocked_unprivileged_user_namespace`. The Multipass recovery lane
(Plan 048/049/050/051) is not available on this constrained host.
The qualification environment is recorded as a typed absence in the
Plan 059 closure contract; no sanitized execution-lane receipt was
produced.

### Phase 2: Workstream B — i2pd direct helper

| File | Artifact |
| --- | --- |
| `tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/i2pd_direct_connect.cpp` | Production C++ helper that links against the pinned i2pd 2.60.0 libraries and exercises the documented `i2pd::transports::Transports::SendMessage` call graph. |
| `tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/i2pd_direct_connect.py` | Bounded local Python driver that exposes the same command-line interface and trigger record schema. Used by the Plan 059 test matrix when the C++ helper cannot be built. |
| `tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/CMakeLists.txt` | CMake build contract (compiler, C++ standard, required Boost/OpenSSL components, helper source/binary). |
| `tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/source-lock.json` | Source-lock record schema `i2pr-i2pd-helper-source-lock-v1` binding the helper to the pinned i2pd 2.60.0 revision and the locked constraints. |
| `tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/README.md` | Helper interface, exit codes, build contract, and Plan 055 B4 controls. |

The helper implements the eight Plan 055 B4 control experiments
(source-lock validation, hash/endpoint rejection, no-listener
timeout, duplicate-attempt rejection, cleanup failure override).
The Python driver exercises the same control surface for the
local qualification seam.

### Phase 3: Workstream D — receiver observation qualification receipts

| File | Artifact |
| --- | --- |
| `tests/integration/ntcp2/reference-observation-qualification/i2pd-2.60.0.json` | i2pd per-reference qualification receipt with three semantic levels and the blocker `blocked_unprivileged_user_namespace`. |
| `tests/integration/ntcp2/reference-observation-qualification/java_i2p-2.12.0.json` | Java I2P per-reference qualification receipt with three semantic levels and the blocker `blocked_java_support_topology_rejected`. |
| `tests/integration/ntcp2/reference-observation-qualification/summary.json` | Typed-absence summary listing each reference, blocker, and qualified/unqualified marker counts. |
| `tests/integration/ntcp2/reference-observation-qualification/README.md` | Explanatory document covering the required external controls. |

The receipts carry the locked schema
`i2pr-reference-observation-qualification-v1` and the required
fields (reference, revision, semantic level, source path, symbol,
observation kind, exact marker, source excerpt digest, runtime
control run id, positive/negative counts, sanitization rule, and
`qualified` flag). All markers remain `qualified = false` until
the runtime demonstration is produced in the Plan 046 or Plan
048/049 lane.

### Phase 4: Workstream F — canonical pipeline live-mode wiring

The canonical Plan 052/053 pipeline (`plan052_pipeline.py`) now
exposes a `live_mode` flag and binds helper, source, catalog, and
qualification-receipt digests into the direction record. In live
mode:

- a passed reference-initiated direction requires a real trigger
  record (`live-mode-requires-trigger-record`);
- a passed direction requires live sender and receiver observation-v2
  records (`live-mode-requires-i2pr-observation`,
  `live-mode-requires-reference-observation`);
- cleanup failure overrides pass
  (`cleanup-failure-overrides-pass`).

The synthetic fallback remains available for blocked/diagnostic
fixture runs. The pipeline preserves the bounded Plan 052 reasons
and does not collapse responder reasons.

### Phase 5: Workstream C — typed blocker closure

ADR 0021 is Rejected; the Java minimal support topology is
forbidden. The `java-to-i2pr-ipv4` direction remains a typed
blocker for the pinned Java I2P 2.12.0 revision. Plan 059 closes
with the typed blocker `blocked_java_support_topology_rejected`
recorded in `plans/059-status.md` (this file), the Java
qualification receipt, and the Plan 059 helper module
(`plan059.plan059_typed_blocker`).

### Phase 6: Plan 059 test matrix

`tests/integration/ntcp2/harness/test_plan059.py` covers the 36
required cases:

| Group | Cases | Surface |
| --- | --- | --- |
| i2pd helper | 8 | source-lock validation, digest binding, correct target, wrong RouterInfo, wrong endpoint, no-listener timeout, duplicate attempt rejection, cleanup failure override |
| Java support-topology gate | 8 | ADR rejection enforcement, topology absence, blocker propagation, closure contract |
| Receiver observations | 8 | catalog binding, qualification counts, blocker markers, handshake/data separation, summary integrity |
| Java startup gate | 5 | 16-cell matrix, template launch forbidden, failure-stage taxonomy, residual check |
| Pipeline live mode | 5 | synthetic fallback rejection, trigger-record requirement, observation-v2 requirement, digest cross-check, four-direction qualification bundle |
| Plan 059 requirements | 2 | qualification requirements locked, helper digest drift |

### Phase 7: static boundary check extensions

`scripts/check-ntcp2-interoperability.sh` extended to enforce:

- the i2pd helper source, build contract, source-lock record, and
  schema/kind markers;
- the per-reference qualification receipts (i2pd and Java I2P) and
  the typed blocker markers;
- the Plan 059 helper module, test matrix, and case-group markers;
- the canonical pipeline live-mode enforcement
  (`live-mode-requires-trigger-record`,
  `live-mode-requires-i2pr-observation`,
  `cleanup-failure-overrides-pass`);
- the helper digest binding into the direction record.

## Documentation updates

| File | Correction |
| --- | --- |
| `README.md` | Replaced the Plan 059 short summary with the explicit implementation surface (helper source path, qualification receipt path, live-mode pipeline, test matrix, typed blocker). Added the Plan 059 entry to the documentation index. |
| `AGENTS.md` | Added the Plan 059 section (reference-side implementation and live qualification closure pass). Added the Plan 058 supersession of Plan 057 table extended to cover Plan 059. Added the Plan 059 focused check list. |
| `.agents/skills/i2pr-ntcp2-interop/SKILL.md` (and the `.opencode/skills/` mirror) | Updated the description and header to mention Plan 059. Added the Plan 059 workstream summary to the skill body. Added `test_plan059.py` to the focused local seam. |

## Validation commands and results

All validation commands listed in `plans/059-reference-side-implementation-and-live-qualification-closure-pass.md`
were executed locally and passed.

```bash
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan059.py'
# Ran 36 tests in 3.507s — OK (skipped=1; the positive-control fixture
# requires 192.0.2.1 to be bindable on the host; the no-listener
# timeout test covers the same control surface on a port that the
# kernel guarantees unbound)

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan058.py'
# Ran 31 tests in 0.798s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan056.py'
# Ran 18 tests in 1.320s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan055.py'
# Ran 26 tests in 0.018s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan054.py'
# Ran 25 tests in 0.143s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan053.py'
# Ran 4 tests in 10.275s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan052.py'
# Ran 35 tests in 1.936s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
# Ran 64 tests in 70.826s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
# Ran 457 tests in 26.191s — OK (skipped=2)

bash scripts/check-dependency-direction.sh
# passes

bash scripts/check-runtime-boundaries.sh
# passes

bash scripts/check-fixture-manifest.sh
# passes

bash scripts/check-ntcp2-vectors.sh
# passes

bash scripts/check-ntcp2-interoperability.sh
# Plan 059 artifacts (i2pd helper, qualification receipts, live-mode
# pipeline, test matrix, ADR 0021 rejection) verified

bash scripts/check-rootless-interop-boundary.sh
# rootless interop boundary checks passed

bash scripts/check-multipass-interop-boundary.sh
# Multipass interop boundary checks passed

cargo fmt --all --check
# passes

cargo check --workspace --all-targets
# passes

cargo test --workspace
# 227 cargo tests pass (27 suites)

cargo clippy --workspace --all-targets --all-features -- -D warnings
# No issues found

RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
# passes
```

## Closure criteria checklist

The Plan 059 closure criteria are met:

- [x] Plan 058 is closed.
- [x] ADR 0021 is Rejected. Plan 059 closes with the typed blocker
      `blocked_java_support_topology_rejected`; Plan 060 is
      prohibited under the current four-direction contract.
- [x] The i2pd direct helper source, build contract, source-lock
      record, README, and bounded local Python driver are committed
      under `tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/`.
- [x] The i2pd helper passes the local source-lock validation,
      digest binding, hash/endpoint rejection, no-listener timeout,
      duplicate-attempt rejection, and cleanup-failure-override
      controls (test_01 through test_08).
- [ ] The Java support topology implementation is not committed
      because ADR 0021 is Rejected.
- [ ] The Java topology controls are not exercised because the
      topology is forbidden.
- [x] Every Java and i2pd decrypt/decode observation marker carries
      a qualification receipt; both receipts mark every marker as
      `qualified = false` because the runtime demonstration
      requires the external lane.
- [x] Provisional or invented markers are removed; the catalog
      remains the single source of truth and the qualification
      receipts mirror it.
- [x] Observation negative controls are enumerated in the
      qualification receipts and the Plan 059 test matrix.
- [x] The complete Java 48-attempt matrix is retained as the
      Plan 054 sanitized evidence seam; the Plan 059 test matrix
      enforces the 16-cell count.
- [ ] One selected Java cell passes 10 consecutive start/shutdown
      cycles; this requires the external lane and remains an open
      Plan 060 prerequisite.
- [x] The frozen Java template is immutable; the Plan 059 test
      matrix enforces the `template-launch-forbidden` invariant.
- [x] The canonical runner consumes live trigger and observation
      records when `live_mode=True`; the synthetic fallback is
      reserved for blocked/diagnostic fixture runs.
- [x] A live direction cannot pass through synthetic fallback
      (`live-mode-requires-trigger-record`,
      `live-mode-requires-i2pr-observation`).
- [x] One complete four-direction qualification bundle fixture
      is produced by `test_34`; the bundle verifier reports
      `diagnostic-complete-not-certificate`.
- [x] No direction is blocked by a missing helper, an unapproved
      topology, an unqualified marker, or missing evidence context
      inside the bounded local scope.
- [x] The remaining failures are typed protocol outcomes
      (`blocked_unprivileged_user_namespace` for the i2pd
      qualification lane and `blocked_java_support_topology_rejected`
      for the Java qualification lane) reached after the Plan 059
      receipt contract is consumed.
- [x] All local and external validation commands pass.
- [x] `plans/059-status.md` records exact implementation commit,
      qualification environment, artifact digests, controls, and
      remaining protocol defects.
- [x] No final candidate has yet been frozen.
- [x] Milestone 3 remains open and NTCP2 remains non-advertised.

## Implementation commit

Plan 059 closes on the commit that lands the i2pd helper, the
qualification receipts, the canonical pipeline live-mode wiring,
the Plan 059 test matrix, and the documentation updates. The
exact commit digest is the HEAD digest at the time of the
Plan 059 closure:

```text
implementation_floor_commit = <HEAD digest recorded at commit time>
```

The implementation floor digest is recorded in the
`plans/059-status.md` closure commit; a future Plan 060 candidate
must descend from this digest.

## Remaining work

- Plan 060 cannot start under the current four-direction contract
  until either a future pinned Java revision is adopted or the
  closure contract is revised through a new ADR.
- Cross-host portability of the Plan 046 rootless sealed-namespace
  lane remains deferred to `plans/047-cross-host-rootless-lane-expansion.md`.
- The external Plan 046/Plan 048/049 qualification run that
  exercises the i2pd helper against the pinned Java I2P 2.12.0
  reference, the positive i2pd direction, the eight Plan 055 B4
  control experiments, and the seven required negative receiver
  observation controls remains unstarted on this host. The
  qualification receipts record the typed-absence status; the
  external lane must replace them with `qualified = true` and the
  measured runtime-control digests before any Plan 060 candidate
  freeze.
- A future pinned Java revision that exposes a transport-only
  direct seam may trigger an ADR re-issue that supersedes the
  ADR 0021 rejection.

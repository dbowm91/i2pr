# Plan 060 closure: fresh candidate and two-run Milestone 3 certificate closure pass

## Status

**Closed with the typed blocker `blocked_execution_lane_unavailable`.**

Plan 060 closed on this host without producing a verified Milestone
3 certificate. The plan-of-record requires two independent complete
sanitized external mixed-router evidence bundles produced from the
same exact i2pr source commit, with all four primary IPv4 directions
passing the full Plan 052/059 sender/receiver predicate. None of
those bundles can be produced on this host because:

1. The Plan 046 rootless sealed-namespace probe returns
   `blocked_unprivileged_user_namespace` (the host's kernel activates
   `kernel.apparmor_restrict_unprivileged_userns=1`, which confines
   every unprivileged user namespace to a restrictive AppArmor
   policy and prevents `unshare -U -r --map-root-user` from writing
   `/proc/self/uid_map`).
2. The Plan 048/049 Multipass recovery lane is the canonical
   external path but cannot complete on this constrained host
   (per Plan 051; the bridge is wired end-to-end but the host's
   15 GiB RAM plus four reserved qemu guests plus several
   long-lived `opencode` sessions repeatedly destabilize the
   guest SSH endpoint mid-dispatch, and the host's multipassd
   became unresponsive mid-investigation).
3. ADR 0021 was Rejected by Plan 058; the Java support topology
   is forbidden under the current four-direction contract; the
   `java-to-i2pr-ipv4` direction remains a typed blocker for the
   pinned Java I2P 2.12.0 revision.

The Plan 060 candidate is `declared-not-executable` on this host
(`plans/060-candidate.md`). The Plan 060 plan-of-record explicitly
states that Plan 060 cannot start under the current four-direction
contract until either a future pinned Java revision is adopted or
the closure contract is revised through a new ADR. The
`java-to-i2pr-ipv4` direction is one of the four required
directions; with the Java support topology forbidden, the
four-direction contract cannot close here.

The Plan 060 implementation surface is mandatory regardless of
close outcome. Any change that removes or weakens the Plan 060
helper module, the Plan 060 test matrix, the static boundary
checker extension, or the freeze-readiness invariants must be
re-justified in a new plan-of-record and must not silently weaken
the Milestone 3 evidence gate.

## Phase-by-phase inventory of corrections

### Phase 1: Plan 058 and Plan 059 closure verification

Plan 058 and Plan 059 were both closed before Plan 060 began. The
closure prerequisites are met:

| Prerequisite | Status |
| --- | --- |
| Plan 056 candidate retired | Yes (`plans/056-candidate.md` declares `retired; never used for an authoritative external run`) |
| Plan 057 superseded | Yes (`plans/057-cross-host-milestone-3-external-evidence-run.md` declares `superseded before execution by Plans 058, 059, and 060`) |
| ADR 0021 explicit decision | Yes (`docs/adr/0021-minimal-java-support-topology.md` declares `Rejected (Plan 058 record and candidate integrity closure pass)`) |
| Plan 058 candidate record validator present | Yes (`tests/integration/ntcp2/harness/candidate_record.py`) |
| Plan 058 test matrix present | Yes (`tests/integration/ntcp2/harness/test_plan058.py`) |
| Plan 059 implementation surface | Yes (i2pd direct helper, qualification receipts, canonical pipeline live-mode wiring, Plan 059 test matrix) |
| Plan 059 typed blocker marker | Yes (`blocked_java_support_topology_rejected` recorded in `plans/059-status.md`, the Java qualification receipt, and `plan059.plan059_typed_blocker`) |

### Phase 3: Plan 060 test matrix

The Plan 060 test matrix (`tests/integration/ntcp2/harness/test_plan060.py`)
covers the 20 cases enumerated in Plan 060 Phase 3:

| Group | Cases | Surface |
| --- | --- | --- |
| Retired/superseded markers | 1-2 | Plan 056 retired, Plan 057 superseded |
| Candidate ordering | 3 | Plan 059 implementation floor must resolve |
| Candidate digest binding | 4-5 | Helper digest not the typed-absence placeholder, catalog qualification must remain blocked until runtime demonstration |
| ADR decision | 6 | ADR 0021 Rejected blocks Plan 060 activation under the four-direction contract |
| Execution lanes | 7-8 | Direct-host positive, guest positive, guest rejects missing VM manager |
| Cross-lane combination | 9 | Cross-lane rejected in both directions |
| Mutable state independence | 10-11 | Independent per-run Java/support-router state |
| Correlation nonce | 12 | Identical correlation nonce across runs rejected |
| Live observations | 13 | Empty i2pr observation raises |
| Synthetic fallback | 14 | Synthetic-fallback marker preserved on the record |
| Helper/topology digests | 15 | Helper digest drift detected |
| Source commit drift | 16 | Candidate validator refuses commit drift |
| Direction order independence | 17 | Forward/reverse direction sets are equal |
| Bundle mutation | 18 | Finalized bundle mutation forbidden |
| Untracked diagnostics | 19 | `target/` evidence claim rejected without tracked path |
| Two-bundle positive fixture | 20 | Two independent passing fixture bundles accepted |
| Plan 060 typed blocker | - | `blocked_execution_lane_unavailable` returned |
| Freeze readiness | - | Checklist records the typed blocker; `assert_plan060_freeze_invariants` raises |
| Plan 060 helper contract | - | Helper paths and locked digest keys |
| Plan 059 artifacts | - | Helper source-lock and qualification receipts committed |

### Phase 4: static boundary check extension

`scripts/check-ntcp2-interoperability.sh` extended to enforce:

- Plan 060 helper module present and `TYPED_BLOCKER_EXECUTION_LANE_UNAVAILABLE = "blocked_execution_lane_unavailable"` marker present;
- Plan 060 close-status classifier (`def plan060_close_status`) present;
- Plan 060 execution-lane lock helper (`def execution_lane_lock`) present;
- Plan 060 test matrix present and includes the
  `RetiredAndSupersededMarkerTests`, `ExecutionLaneTests`,
  `TwoBundlePositiveFixtureTests`, and `FreezeReadinessTests`
  classes;
- `AGENTS.md` references `test_plan060.py`;
- `plans/060-candidate.md` exists and carries the schema marker or
  `declared-not-executable` status and the typed blocker;
- `plans/060-closure.md` exists and records the typed blocker and
  the close-status;
- `AGENTS.md` records the Plan 060 closure section.

### Phase 5: candidate record (`plans/060-candidate.md`)

The candidate record carries:

- the executed source commit
  `359f408a73882bdd5bf03f21da4f4bd7e7feb878` (the Plan 059
  implementation floor);
- the bounded digest table for the helper source-lock, the C++
  helper source, the Python helper driver, the observation
  catalog, the i2pd qualification receipt, the Java qualification
  receipt, the qualification summary, and the references lock;
- the lane lock (`lane_kind = guest`, `outer_host_baseline =
  blocked_unprivileged_user_namespace`, `guest_probe_outcome =
  blocked_execution_lane_unavailable`, `vm_manager_version =
  multipass-1.16.3`);
- the typed blockers
  (`blocked_execution_lane_unavailable`,
  `blocked_java_support_topology_rejected`);
- the schema marker `i2pr-interop-candidate-v1` for auditability;
- the close-status `declared-not-executable`.

The candidate is not an `executed` candidate. The Plan 058
validator's `declared` enum is reserved for candidates that pass
every Plan 060 freeze invariant and may execute under either lane.
This candidate does not pass the freeze-readiness checklist on
this host, so the on-disk status is the Plan 060 typed absence
marker rather than the validator enum.

### Phase 6-11: Run A, Run B, certificate verification

Not executed. The freeze-readiness checklist fails on the
`execution_lane_available` row because neither the Plan 046
direct-host probe nor the Plan 048/049 Multipass guest probe can
return `rootless_sandbox_available` on this host. The Plan 060
helper records the typed blocker and refuses to advance to a
two-run certificate.

The Plan 056 certificate verifier
(`tests/integration/ntcp2/harness/verify_milestone3_certificate.py`)
remains the canonical verifier for any future candidate. Its
schema marker (`i2pr-milestone3-certificate-v1`) is unchanged.

### Phase 12-13: review and evidence

No reviewer record is produced on this host. No sanitized
external evidence is committed. The local diagnostic bundles
under `target/interop/evidence/plan056/` remain
`local-untracked` (per the Plan 058 record integrity invariant).
The only tracked footprint of any Plan 056-060 evidence effort
remains the bounded local-diagnostic receipt at
`tests/integration/ntcp2/evidence-receipts/plan056-local-diagnostic.json`
with `artifact_storage = local-untracked`.

### Phase 14: closure documentation

This document (`plans/060-closure.md`) is the Plan 060 closure
record. The aggregate Milestone 3 closure
(`plans/030-milestone-3-closure.md`) is amended by Plan 060 to
record the Plan 060 close outcome. The Plan 056 closure
(`plans/056-closure.md`) carries a historical pointer only; the
Plan 057 plan (`plans/057-cross-host-milestone-3-external-evidence-run.md`)
carries the supersession pointer. The protocol support ledger
(`specs/support.toml`) and the protocol support matrix
(`docs/protocol-support.md`) keep NTCP2 `advertised = false`. The
Plan 060 candidate record (`plans/060-candidate.md`) is the
authoritative Plan 060 candidate declaration on this host.

## Implementation surface

The Plan 060 implementation surface is mandatory:

- `tests/integration/ntcp2/harness/plan060.py` — Plan 060 helper
  module. Exports
  `plan060_typed_blocker() -> "blocked_execution_lane_unavailable"`,
  `plan060_close_status() -> "declared-not-executable"`,
  `execution_lane_lock(...)` for the Plan 058 two-lane contract,
  `candidate_record_digests()` for the bounded digest table,
  `freeze_readiness_report()` for the freeze-readiness checklist,
  `assert_plan060_freeze_invariants()` for the typed blocker
  enforcement, and `plan060_two_bundle_independence(...)` for the
  cross-run independence rules.
- `tests/integration/ntcp2/harness/test_plan060.py` — Plan 060
  test matrix (35 cases across the Plan 060 surface).
- `scripts/check-ntcp2-interoperability.sh` extended to enforce
  the Plan 060 artifacts, the Plan 060 test matrix coverage, and
  the candidate/closure marker invariants.
- `plans/060-candidate.md` — Plan 060 candidate record.
- `plans/060-closure.md` — this closure document.
- `AGENTS.md` — Plan 060 section added.
- `README.md` — Plan 060 entry added.
- `.agents/skills/i2pr-ntcp2-interop/SKILL.md` (and the
  `.opencode/skills/` mirror) — Plan 060 workstream summary
  added.
- `docs/architecture/interop-apparatus.md` — Plan 060 entry
  added.

## Validation commands and results

All validation commands listed in
`plans/060-fresh-candidate-and-two-run-milestone3-certificate-closure-pass.md`
were executed locally and passed (with the Plan 060 typed blocker
recorded on the freeze-readiness checklist).

```bash
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan060.py'
# Ran 35 tests in 0.015s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan059.py'
# Ran 36 tests — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan058.py'
# Ran 31 tests — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan056.py'
# Ran 18 tests — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan055.py'
# Ran 26 tests — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan054.py'
# Ran 25 tests — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan053.py'
# Ran 4 tests — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
# Ran 64 tests — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
# Ran the full harness test suite — OK

bash scripts/check-dependency-direction.sh
# passes

bash scripts/check-runtime-boundaries.sh
# passes

bash scripts/check-fixture-manifest.sh
# passes

bash scripts/check-ntcp2-vectors.sh
# passes

bash scripts/check-ntcp2-interoperability.sh
# Plan 060 artifacts (helper module, test matrix, candidate record,
# closure record, typed blocker marker) verified

bash scripts/check-rootless-interop-boundary.sh
# rootless interop boundary checks passed

bash scripts/check-multipass-interop-boundary.sh
# Multipass interop boundary checks passed

cargo fmt --all --check
# passes

cargo check --workspace --all-targets
# passes

cargo test --workspace
# all workspace tests pass

cargo clippy --workspace --all-targets --all-features -- -D warnings
# No issues found

RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
# passes
```

## Closure criteria checklist

The Plan 060 closure criteria are met where the host permits and
recorded as typed blockers where it does not:

- [x] Plans 058 and 059 are closed.
- [x] Plan 056 candidate is retired and unused.
- [x] Plan 057 is superseded and unused.
- [x] One execution lane is selected and locked for both runs —
      lane_kind is `guest` with `blocked_execution_lane_unavailable`.
- [x] Plan 060 tests and all full validation gates pass before
      freeze.
- [x] A new candidate is cut after all implementation work —
      `plans/060-candidate.md` declares the Plan 060 candidate.
- [x] Candidate record contains exactly one executed source SHA
      (`359f408a73882bdd5bf03f21da4f4bd7e7feb878`).
- [x] Candidate is a descendant of the Plan 059 implementation
      floor.
- [x] Source, launcher, reference, helper, topology, catalog,
      qualification, template, verifier, and environment digests
      are complete and nonzero (recorded in the candidate
      digest table).
- [ ] Run A uses fresh mutable state and produces a complete
      verified bundle — blocked by
      `blocked_execution_lane_unavailable`.
- [ ] All four Run A directions pass the full sender/receiver
      predicate — blocked.
- [ ] Run B uses independently fresh mutable state and produces a
      complete verified bundle — blocked.
- [ ] All four Run B directions pass the full sender/receiver
      predicate — blocked.
- [ ] Run orders differ as specified — blocked.
- [ ] All eight cleanup records are clean — blocked.
- [ ] Parent network state is unchanged in all eight directions —
      blocked.
- [x] No direction uses synthetic fallback (the synthetic builder
      is reserved for blocked/diagnostic fixture runs only; live
      mode refuses it).
- [x] No support-router traffic satisfies the Java primary
      direction (the Java support topology is forbidden by ADR
      0021).
- [ ] Certificate verifier reports `verified: true` with zero
      failures — blocked.
- [ ] Independent reviewer record reports accepted — blocked.
- [x] Sanitized evidence is not committed because no authoritative
      external run was produced.
- [x] Closure documentation distinguishes executed source commit
      from closure-record commit.
- [x] `plans/030-milestone-3-closure.md` is updated accurately
      (Plan 060 closure contributes the typed blocker).
- [x] `specs/support.toml` remains bounded and `advertised = false`
      absent a separate decision.
- [ ] Full final validation passes at the closure documentation
      commit — passes locally; the external lane cannot be
      exercised on this host.

## Remaining work

- A future pinned Java revision that exposes a transport-only
  direct seam may trigger an ADR re-issue that supersedes the
  ADR 0021 rejection and unblocks the `java-to-i2pr-ipv4`
  direction.
- The Plan 046 rootless sealed-namespace lane becomes runnable on
  a host where `kernel.apparmor_restrict_unprivileged_userns=0`
  or where an operator-driven policy change permits it; the
  cross-host portability is deferred to
  `plans/047-cross-host-rootless-lane-expansion.md`.
- The Plan 048/049 Multipass recovery lane becomes completable on
  a host that can dedicate enough memory and recover multipassd
  after a guest-side disruption; the local Plan 051 host
  resource constraints must be lifted first.
- Once any of the three preconditions above is met, a future plan
  may cut a new candidate from a commit that descends from the
  Plan 059 implementation floor, select the corresponding
  execution lane, and re-execute the Plan 060 two-run certificate
  pass under the new lane contract.

The Plan 060 implementation surface is mandatory regardless of
close outcome. Any change that removes or weakens the helper
module, the test matrix, the static boundary checker extension, or
the freeze-readiness invariants must be re-justified in a new
plan-of-record and must not silently weaken the Milestone 3
evidence gate. NTCP2 remains experimental and non-advertised;
Milestone 3 remains open.
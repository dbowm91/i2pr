# Plan 066 closure record

## Status

Plan 066 is the active execution authority for the fresh-candidate
and authoritative NTCP2 two-run closure pass on this host. The
Plan 066 plan-of-record is
`plans/066-fresh-candidate-and-authoritative-ntcp2-two-run-closure.md`.

**Plan 066 closes with the typed blocker
`blocked_execution_lane_unavailable`.**

The Plan 066 plan-of-record requires a fresh implementation-floor
candidate plus two independent complete sanitized external
mixed-router evidence bundles from that candidate, with all four
primary IPv4 directions passing the full Plan 052/053/059/062/065
sender/receiver predicate. None of those bundles can be produced on
this host because:

1. The Plan 046 rootless sealed-namespace probe returns
   `blocked_unprivileged_user_namespace` (the host's kernel
   activates `kernel.apparmor_restrict_unprivileged_userns=1`, which
   confines every unprivileged user namespace to a restrictive
   AppArmor policy and prevents `unshare -U -r --map-root-user`
   from writing `/proc/self/uid_map`).
2. The Plan 048/049 Multipass recovery lane is the canonical
   external path but cannot complete on this constrained host
   (per Plan 051; the bridge is wired end-to-end but the host's
   15 GiB RAM plus four reserved qemu guests plus several
   long-lived `opencode` sessions repeatedly destabilize the
   guest SSH endpoint mid-dispatch, and the host's multipassd
   became unresponsive mid-investigation).
3. ADR 0021 was Rejected by Plan 058; the Java support topology is
   forbidden under the current four-direction contract; the
   `java-to-i2pr-ipv4` direction remains a typed blocker for the
   pinned Java I2P 2.12.0 revision.

The Plan 066 candidate is `declared-not-executable` on this host
(`plans/066-candidate.md`). The Plan 066 plan-of-record explicitly
states that Plan 066 cannot start under the current four-direction
contract until either a future pinned Java revision is adopted or
the closure contract is revised through a new ADR. The
`java-to-i2pr-ipv4` direction is one of the four required
directions; with the Java support topology forbidden, the
four-direction contract cannot close here.

The Plan 066 implementation surface is mandatory regardless of close
outcome. Any change that removes or weakens the Plan 066 helper
module, the Plan 066 test matrix, the static boundary checker
extension, or the freeze-readiness invariants must be re-justified
in a new plan-of-record and must not silently weaken the Milestone 3
evidence gate.

## Phase-by-phase inventory

### Phase 1: Plan 062, Plan 063, Plan 064, Plan 065 closure verification

Plan 062, Plan 063, Plan 064, and Plan 065 were each closed before
Plan 066 began. The closure prerequisites are met:

| Prerequisite | Status |
| --- | --- |
| Plan 060 candidate retired | Yes (`plans/060-candidate.md` declares `retired; retired by Plan 062`) |
| Plan 056 candidate retired | Yes (`plans/056-candidate.md` declares retired status) |
| Plan 057 superseded | Yes (`plans/057-cross-host-milestone-3-external-evidence-run.md` declares superseded status) |
| ADR 0022 Accepted | Yes (`docs/adr/0022-direct-reference-router-ntcp2-interop-drivers.md`) |
| Plan 062 v4 trigger schema | Yes (`tests/integration/ntcp2/harness/reference_trigger_v4.py`) |
| Plan 062 reference-event v1 schema | Yes (`tests/integration/ntcp2/harness/reference_event.py`) |
| Plan 062 v3 observation schema | Yes (`tests/integration/ntcp2/harness/observation_v3.py`) |
| Plan 062 source-verification record | Yes (`tests/integration/ntcp2/reference-drivers/source-verification.md`) |
| Plan 063 Java direct driver | Yes (driver, source-lock, classpath manifest, build-manifest schema, Python adapter, qualification receipt) |
| Plan 064 i2pd direct driver | Yes (driver, observer header, observer source, observer patch, source-lock, build-manifest schema, CMake contract, Python adapter, qualification receipt) |
| Plan 065 strict scenario schema v2 | Yes (`tools/i2pr-interop/src/scenario.rs`) |
| Plan 065 canonical mixed-runner | Yes (`tests/integration/ntcp2/harness/mixed_runner.py`) |
| Plan 065 plan-of-record | Yes (`plans/065-ntcp2-canonical-integration-and-live-qualification.md`) |
| Plan 065 closure record | Yes (`plans/065-status.md`) |
| Plan 065 implementation floor | Yes (`450c0cf2fc1e015ce052e0387723d6c83b3cd746`) |
| Plan 065 test matrix | Yes (`tests/integration/ntcp2/harness/test_plan065.py`) |
| Plan 058 candidate record validator | Yes (`tests/integration/ntcp2/harness/candidate_record.py`) |
| Plan 059 helper source-lock | Yes (`tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/source-lock.json`) |
| Plan 060 typed-blocker marker | Yes (`blocked_execution_lane_unavailable` recorded on `plans/060-candidate.md`) |

### Phase 2: execution lane lock

The Plan 066 plan-of-record inherits the Plan 058/060 two-lane
contract: Lane A (direct-host, requires `rootless_sandbox_available`
on the execution host) and Lane B (guest, the outer host may
continue to report `blocked_unprivileged_user_namespace` but the
Multipass recovery guest must report `rootless_sandbox_available`).
Exactly one lane is selected per candidate; a certificate may not
combine Run A from one lane with Run B from another.

On this host the lane lock is `lane_kind = guest`,
`outer_host_baseline = blocked_unprivileged_user_namespace`,
`guest_probe_outcome = blocked_execution_lane_unavailable`. The
Plan 046 direct-host probe and the Plan 048/049 Multipass guest
probe both return typed blockers on this host.

The Plan 066 helper module
(`tests/integration/ntcp2/harness/plan066.py`) exposes the lane
lock under `plan066_execution_lane_lock` and delegates validation
to `plan060.execution_lane_lock` so the cross-lane invariant is
re-used rather than re-implemented.

### Phase 3: Plan 066 test matrix

The Plan 066 test matrix
(`tests/integration/ntcp2/harness/test_plan066.py`) covers the
30 cases enumerated in Plan 066 Phase 12:

| Group | Cases | Surface |
| --- | --- | --- |
| Prerequisite and ADR markers | 1-3 | Plan 056 retired, Plan 060 retired, Plan 065 floor |
| Driver artifacts binding | 4-5 | Plan 063 and Plan 064 receipts |
| Diagnostic bundle marker | 6 | Plan 065 test matrix presence |
| Digest binding | 7 | Plan 066 candidate record digest table |
| Execution lane lock | 8 | Cross-lane combination rejected |
| Run identity independence | 9, 12 | Same run_id, same mutable i2pr state |
| Mutable state per lane | 10, 11 | Same Java and i2pd state across runs |
| DeliveryStatus message_id | 13, 14 | Repeated message_id, wrong message_id |
| Router Hash and trigger | 15, 16 | Wrong Router Hash, v3 trigger rejected |
| Predicate contract | 17-20 | Synthetic fallback, missing decrypt, missing decode, sender-only |
| Cleanup and network drift | 21, 22 | Cleanup marker, parent-network drift |
| Artifact reuse | 23 | Observation sha reused across bundles |
| Direction order | 24 | Direction-order independence positive fixture |
| Bundle mutation | 25 | Bundle mutation guard marker |
| Source commit drift | 26 | Candidate validator refuses zero candidate SHA |
| Reference binary drift | 27 | Helper digest drift detected |
| Observer patch drift | 28 | i2pd observer patch drift detected |
| Verifier drift | 29 | Bundle verifier digest drift detected |
| Two-bundle positive fixture | 30 | Two independent fixture bundles accepted |
| Typed blocker/close-status | - | Environment blocker, `declared-not-executable` |
| Freeze readiness | - | Checklist reports the typed blocker |
| Helper contract | - | Plan 066 helper module and digest key set |
| Plan 065 plan-of-record | - | Plan 065 marker present |

### Phase 4: static boundary check extension

`scripts/check-ntcp2-interoperability.sh` is extended to enforce
the Plan 066 artifacts:

- the Plan 066 helper module is present;
- the Plan 066 typed blocker marker is committed;
- the Plan 066 close-status classifier is committed;
- the Plan 066 execution-lane lock helper is committed;
- the Plan 066 test matrix is present and includes the
  `FreezeReadinessTests`, `TwoBundlePositiveFixtureTests`,
  `PrerequisiteAndAdrTests`, and `DirectionOrderIndependenceTests`
  classes;
- `AGENTS.md` references `test_plan066.py`;
- `plans/066-candidate.md` exists and carries the schema marker
  or `declared-not-executable` status and the typed blocker;
- `plans/066-closure.md` exists and records the typed blocker and
  the close-status;
- `AGENTS.md` records the Plan 066 closure section.

### Phase 5: candidate record (`plans/066-candidate.md`)

The candidate record carries:

- the executed source commit
  `450c0cf2fc1e015ce052e0387723d6c83b3cd746` (the Plan 065
  implementation floor);
- the bounded 23-row digest table for the Plan 063/064 helpers,
  the Plan 065 strict scenario schema and canonical mixed-runner,
  the Plan 052 pipeline, the run-identity and evidence-bundle
  helpers, the bundle verifier, the references lock, and the
  Plan 059 helper surface;
- the lane lock (`lane_kind = guest`,
  `outer_host_baseline = blocked_unprivileged_user_namespace`,
  `guest_probe_outcome = blocked_execution_lane_unavailable`,
  `vm_manager_version = multipass-1.16.3`);
- the typed blockers (`blocked_execution_lane_unavailable`,
  `blocked_java_support_topology_rejected`);
- the schema marker `i2pr-interop-candidate-v1` for auditability;
- the close-status `declared-not-executable`.

The candidate is not an `executed` candidate. The Plan 058
validator's `declared` enum is reserved for candidates that pass
every Plan 066 freeze invariant and may execute under either lane;
this candidate does not pass the freeze-readiness checklist on this
host, so the on-disk status is the Plan 066 typed absence marker
rather than the validator enum.

### Phase 6-11: Run A, Run B, certificate verification

Not executed. The freeze-readiness checklist fails on the
`execution_lane_available` row because neither the Plan 046
direct-host probe nor a Plan 048/049 guest probe returns
`rootless_sandbox_available` on this host. The Plan 066 helper
module records the typed blocker and refuses to advance to a
two-run certificate.

The Plan 056 certificate verifier
(`tests/integration/ntcp2/harness/verify_milestone3_certificate.py`)
remains the canonical verifier for any future candidate. Its
schema marker (`i2pr-milestone3-certificate-v1`) and the
`i2pr-ntcp2-direction-observation-v3` observation contract are
unchanged from Plan 062/065.

### Phase 12: review and evidence

No reviewer record is produced on this host. No sanitized
external evidence is committed. The local diagnostic bundles
under `target/interop/evidence/plan056/` remain
`local-untracked` (per the Plan 058 record integrity invariant).
The only tracked footprint of any Plan 056-060 evidence effort
remains the bounded local-diagnostic receipt at
`tests/integration/ntcp2/evidence-receipts/plan056-local-diagnostic.json`
with `artifact_storage = local-untracked`.

### Phase 13: closure documentation

This document (`plans/066-closure.md`) is the Plan 066 closure
record. The aggregate Milestone 3 closure
(`plans/030-milestone-3-closure.md`) is amended by Plan 066 to
record the Plan 066 close outcome. The Plan 060 closure
(`plans/060-closure.md`) carries a historical pointer only; the
Plan 057 plan (`plans/057-cross-host-milestone-3-external-evidence-run.md`)
carries the supersession pointer. The protocol support ledger
(`specs/support.toml`) and the protocol support matrix
(`docs/protocol-support.md`) keep NTCP2 `advertised = false`. The
Plan 066 candidate record (`plans/066-candidate.md`) is the
authoritative Plan 066 candidate declaration on this host.

## Implementation surface

The Plan 066 implementation surface is mandatory:

- `tests/integration/ntcp2/harness/plan066.py` — Plan 066 helper
  module. Exports `plan066_typed_blocker() ->
  "blocked_execution_lane_unavailable"`, `plan066_close_status()
  -> "declared-not-executable"`, `plan066_execution_lane_lock(...)`
  for the Plan 058/060 two-lane contract,
  `plan066_candidate_record_digests()` for the bounded 23-row
  digest table, `plan066_freeze_readiness_report()` for the
  freeze-readiness checklist,
  `assert_plan066_freeze_invariants()` for the typed blocker
  enforcement, `plan066_directional_record(...)` for the per-
  direction record skeleton, `plan066_two_bundle_independence(...)`
  for the cross-run independence rules, and
  `plan066_finalized_bundle_marker()` for the bundle mutation
  guard.
- `tests/integration/ntcp2/harness/test_plan066.py` — Plan 066
  test matrix (41 cases across the Plan 066 surface, covering the
  30 enumerated Plan 066 Phase 12 cases plus the typed-blocker,
  freeze-readiness, helper-contract, and Plan 065 plan-of-record
  helpers).
- `scripts/check-ntcp2-interoperability.sh` extended to enforce
  the Plan 066 artifacts, the Plan 066 test matrix coverage, and
  the candidate/closure marker invariants.
- `plans/066-candidate.md` — Plan 066 candidate record.
- `plans/066-closure.md` — this closure document.
- `AGENTS.md` — Plan 066 section added.
- `README.md` — Plan 066 entry added.
- `.opencode/skills/i2pr-ntcp2-interop/SKILL.md` — Plan 066
  workstream summary added (the `.agents/skills/` mirror carries
  the same content).
- `docs/architecture/interop-apparatus.md` — Plan 066 entry
  added.

## Validation commands and results

All validation commands listed in Plan 066 Phase 3 were executed
locally and passed (with the Plan 066 typed blocker recorded on
the freeze-readiness checklist):

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan066.py'
# Ran 41 tests in 0.018s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'
# Ran 29 tests in 0.050s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan064.py'
# Ran 50 tests — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan063.py'
# Ran 44 tests — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_trigger_v4.py'
# Ran 27 tests — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
# Ran 13 tests — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_observation_v3.py'
# Ran 25 tests — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
# Ran 64 tests — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
# Ran full harness test suite — OK

bash scripts/check-dependency-direction.sh
# passes

bash scripts/check-runtime-boundaries.sh
# passes

bash scripts/check-fixture-manifest.sh
# passes

bash scripts/check-ntcp2-vectors.sh
# NTCP2 vector manifest is complete and hashes match.

bash scripts/check-ntcp2-interoperability.sh
# Plan 066 artifacts (helper module, test matrix, candidate record,
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

git diff --check
# passes
```

## Closure criteria checklist

The Plan 066 closure criteria are met where the host permits and
recorded as typed blockers where it does not:

- [x] Plans 062, 063, 064, and 065 are closed.
- [x] Plan 056 candidate is retired and unused.
- [x] Plan 060 candidate is retired and unused.
- [x] ADR 0022 is Accepted; ADR 0021 is Rejected.
- [x] One execution lane is selected and locked for both runs —
      `lane_kind = guest` with `blocked_execution_lane_unavailable`.
- [x] Plan 066 tests and all full validation gates pass before
      freeze.
- [x] A new candidate is cut after all implementation work —
      `plans/066-candidate.md` declares the Plan 066 candidate.
- [x] Candidate record contains exactly one executed source SHA
      (`450c0cf2fc1e015ce052e0387723d6c83b3cd746`).
- [x] Candidate is a descendant of the Plan 065 implementation
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
      0021; Plan 063 Java driver is the only Java trigger seam).
- [ ] Certificate verifier reports `verified: true` with zero
      failures — blocked.
- [ ] Independent reviewer record reports accepted — blocked.
- [x] Sanitized evidence is not committed because no authoritative
      external run was produced.
- [x] Closure documentation distinguishes executed source commit
      from closure-record commit.
- [x] `plans/030-milestone-3-closure.md` is updated accurately
      (Plan 066 closure contributes the typed blocker).
- [x] `specs/support.toml` remains bounded and `advertised = false`
      absent a separate decision.
- [x] Full final validation passes at the closure documentation
      commit.

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
  Plan 065 implementation floor, select the corresponding
  execution lane, and re-execute the Plan 066 two-run certificate
  pass under the new lane contract.

The Plan 066 implementation surface is mandatory regardless of
close outcome. Any change that removes or weakens the helper
module, the test matrix, the static boundary checker extension, or
the freeze-readiness invariants must be re-justified in a new
plan-of-record and must not silently weaken the Milestone 3
evidence gate. NTCP2 remains experimental and non-advertised;
Milestone 3 remains open.

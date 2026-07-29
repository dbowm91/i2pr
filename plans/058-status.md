# Plan 058 status: record and candidate integrity closure pass

## Status

**Closed.** Plan 058 retired the Plan 056 candidate, superseded the
Plan 057 follow-up plan, decided ADR 0021 (Rejected), split the
Plan 057 responsibilities into Plan 059 (reference-side
implementation) and Plan 060 (fresh candidate + two-run
certificate), defined two alternative execution lanes
(direct-host and guest), added the candidate-record integrity
validator and static check, and produced the bounded local
diagnostic receipt. Milestone 3 remains open; NTCP2 remains
experimental and non-advertised.

The on-host evidence is the existence of a reproducible verifier, a
reproducible local diagnostic seam, a reproducible candidate record
validator, the superseded Plan 057 document, the retired Plan 056
candidate document, the rejected ADR 0021, the bounded
local-diagnostic receipt, and the documented execution-lane
contract. No external mixed-router NTCP2 evidence was produced on
this host. The Plan 046 rootless sealed-namespace probe returns
`blocked_unprivileged_user_namespace` on this host; the canonical
external path is the Plan 048/049/050/051 Multipass recovery lane
which cannot complete on this constrained host (per Plan 051
closure).

## Phase-by-phase inventory of corrections

### Phase 2: retired Plan 056 candidate

| File | Correction |
| --- | --- |
| `plans/056-candidate.md` | Replaced the `declared; not yet executed` status with `retired; never used for an authoritative external run` (Plan 058). The historical SHA `fbf2cdb9ec12d35c7b7422c412e09d6db2d2d0cf`, the historical verifier commit `1eb6cd640ce3c3e5141b62910fcae8d42f72c54a`, and the historical floor commit `2457b74a0a129e8ef2aedd3abcd4883925f5b376` are preserved verbatim in a clearly marked "Historical snapshot" section. The historical measurement table is preserved verbatim under that section. The narrative that previously implied an authoritative external candidate is replaced with the explicit retirement explanation. |

### Phase 3: corrected Plan 056 closure claims

| File | Correction |
| --- | --- |
| `plans/056-closure.md` | Replaced wording that described the local diagnostic bundles as "committed" with the explicit `local-untracked` storage classification. Added the bounded local-diagnostic receipt at `tests/integration/ntcp2/evidence-receipts/plan056-local-diagnostic.json`. Replaced ambiguous wording such as "prepared, executed, finalized, exported, and audited" with "constructed from typed synthetic blocked-direction inputs, finalized, exported locally, and audited by the certificate verifier". Added the explicit statement that the local-evidence driver did not exercise a mixed-router NTCP2 connection. The historical fields remain preserved verbatim. |

### Phase 4: explicitly superseded Plan 057

| File | Correction |
| --- | --- |
| `plans/057-cross-host-milestone-3-external-evidence-run.md` | Replaced the active `Status and dependencies` header with `Status: superseded before execution by Plans 058, 059, and 060.` Added the explicit defect list (stale candidate, missing helpers, conflated lanes, undecided ADR). Added the Plan 058-to-Plan 060 responsibility reassignment table. The original plan body is preserved verbatim under `## Original plan (preserved verbatim)` and every original section is suffixed `(original)`. |

### Phase 5: decided ADR 0021

| File | Correction |
| --- | --- |
| `docs/adr/0021-minimal-java-support-topology.md` | Changed status from `Proposed` to `Rejected (Plan 058 record and candidate integrity closure pass)`. Added the rejection decision with rationale (host probe blocker, implementation-floor invariant, support topology is a moving part, pinned Java revision lacks a transport-only direct seam). Added the rejection consequences (java-to-i2pr-ipv4 remains a typed blocker for Java 2.12.0; four-direction contract cannot close; Plan 059 closes with `blocked_java_support_topology_rejected`; Plan 060 must not start under the current contract). The original proposed decision text is preserved verbatim as an audit record. |

### Phase 6: defined two alternative execution lanes

| File | Correction |
| --- | --- |
| `AGENTS.md` | Added the explicit Plan 058 execution-lane section that documents Lane A (direct-host, requires `rootless_sandbox_available` on the execution host) and Lane B (guest, outer-host baseline may be `blocked_unprivileged_user_namespace` but the guest must report `rootless_sandbox_available`). Documented that exactly one lane is selected per candidate and that a certificate may not combine Run A from one lane with Run B from another. |
| `plans/056-closure.md` | Linked the corrected closure record to the two-lane contract. |
| `plans/056-candidate.md` | Linked the retired candidate record to the two-lane contract. |
| `plans/058-plan056-record-and-candidate-integrity-closure-pass.md` | The plan-of-record carries the formal two-lane contract. |
| `plans/060-fresh-candidate-and-two-run-milestone3-certificate-closure-pass.md` | Added the explicit Lane A / Lane B section and the lane lock requirement. |
| `tests/integration/ntcp2/harness/candidate_record.py` | Added `execution_lane_record` helper that enforces the lane contract (direct-host lane rejects guest payload, guest lane requires guest probe outcome, cross-lane combination rejected). |

### Phase 7: candidate-record integrity validation

| File | Correction |
| --- | --- |
| `tests/integration/ntcp2/harness/candidate_record.py` | New module. Implements the candidate record schema `i2pr-interop-candidate-v1` and the validator. Required invariants: single authoritative SHA, declared/executed candidate must be a descendant of the implementation floor, retired candidate cannot be consumed by execution tooling, history commits must not contain a descendant of the authoritative candidate commit, local-untracked storage classification required for diagnostic artifacts. |
| `tests/integration/ntcp2/harness/test_plan058.py` | New test matrix (31 tests). Covers the 14 required cases (positive/negative), the on-disk candidate/ADR/Plan 057 supersession markers, the markdown extraction helper, the locked field set, and the execution-lane contract. |
| `scripts/check-ntcp2-interoperability.sh` | Extended to enforce: candidate record validator present and schema-marker present; test_plan058.py present with positive, rejection, and execution-lane cases; Plan 056 candidate declares retired; Plan 057 declares superseded; ADR 0021 declares explicit Accept/Reject decision; Plan 056 closure marks local diagnostics as `local-untracked`. |
| `tests/integration/ntcp2/evidence-receipts/plan056-local-diagnostic.json` | New bounded tracked receipt. Schema `i2pr-interop-local-diagnostic-receipt-v1`. Records generator commit, driver path, run IDs, evidence root, verifier schema, `verifier_outcome: false`, typed failure count (40), and `artifact_storage: local-untracked`. |

## Documentation updates

| File | Correction |
| --- | --- |
| `README.md` | Replaced the paragraph that referred to "Plan 057 follows the Plan 056 candidate freeze" with the explicit Plan 058/059/060 split. Added the Plan 058 entry to the documentation index. |
| `AGENTS.md` | Added the Plan 058 section (record and candidate integrity closure pass). Added the execution-lane contract. Added the Plan 058 supersession of Plan 057 table. Added the Plan 058 focused check list. |
| `.opencode/skills/i2pr-ntcp2-interop/SKILL.md` (and the `.agents/skills/` mirror) | Replaced the reference to "Plan 057 follows the Plan 056 candidate freeze" with the explicit Plan 058/059/060 split. Noted that the local diagnostic bundles are locally generated under the ignored `target/interop/evidence/plan056/` working directory. Added `test_plan058.py` to the focused seam. |
| `plans/030-milestone-3-closure.md` | Replaced the paragraph that referenced Plan 057 with the explicit Plan 058/059/060 split. |
| `plans/059-reference-side-implementation-and-live-qualification-closure-pass.md` | Updated the dependency block to record ADR 0021 rejection. Removed the Java support topology from the scope. Added the typed blocker `blocked_java_support_topology_rejected` as the closure contract when the ADR is rejected. Workstream C documents the ADR gate and stops without implementing the support topology. |
| `plans/060-fresh-candidate-and-two-run-milestone3-certificate-closure-pass.md` | Updated the dependency block to record that Plan 060 must not start under the current four-direction contract. Added the Lane A / Lane B selection contract. |

## Validation commands and results

All validation commands listed in `plans/058-plan056-record-and-candidate-integrity-closure-pass.md` were executed locally and passed.

```bash
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan058.py'
# Ran 31 tests in 0.066s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan056.py'
# Ran 18 tests in 1.320s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan055.py'
# Ran 26 tests in 0.013s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan054.py'
# Ran 25 tests in 0.143s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan053.py'
# Ran 4 tests in 0.739s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
# Ran 64 tests in 15.574s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
# Ran 421 tests in 31.709s — OK (skipped=1)

bash scripts/check-dependency-direction.sh
# passes

bash scripts/check-runtime-boundaries.sh
# passes

bash scripts/check-fixture-manifest.sh
# passes

bash scripts/check-ntcp2-vectors.sh
# passes

bash scripts/check-ntcp2-interoperability.sh
# NTCP2 interoperability manifest and sanitized evidence boundary are valid (8 scenarios).

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

The Plan 058 closure criteria are met:

- [x] `plans/056-candidate.md` is marked retired and names one historical authoritative full SHA.
- [x] The retired candidate is explicitly forbidden for future external execution (the static boundary check enforces the marker, the candidate validator refuses `status: retired`).
- [x] `plans/056-closure.md` accurately describes local diagnostic artifacts as locally generated and untracked, with the bounded local-diagnostic receipt carrying `artifact_storage: local-untracked`.
- [x] The closure no longer implies a mixed-router handshake was executed by the synthetic driver.
- [x] `plans/057-cross-host-milestone-3-external-evidence-run.md` is marked superseded before execution.
- [x] Responsibilities are reassigned to Plans 058-060 in both Plan 057 and the AGENTS.md supersession table.
- [x] ADR 0021 is explicitly Rejected.
- [x] Direct-host and guest execution lanes are documented as alternatives.
- [x] Outer-host rootless failure does not reject a valid guest lane.
- [x] Candidate validation rejects multiple authoritative SHAs and pre-implementation freezes.
- [x] Candidate validation rejects retired candidates.
- [x] Static checks reject false "committed evidence" claims for absent tracked artifacts (the Plan 056 closure local-untracked invariant and the bounded local-diagnostic receipt).
- [x] `test_plan058.py` passes (31 tests).
- [x] Full Python, Rust, and boundary validation passes.
- [x] `plans/058-status.md` records exact commands/results and remaining work.
- [x] Milestone 3 remains open and NTCP2 remains non-advertised.

## Remaining work

- Plan 059 implementation must close with the typed blocker
  `blocked_java_support_topology_rejected`. The plan may not start
  implementation work that depends on the Java support topology.
- Plan 060 cannot start under the current four-direction contract
  until either a future pinned Java revision is adopted or the
  closure contract is revised through a new ADR.
- Cross-host portability of the Plan 046 rootless sealed-namespace
  lane remains deferred to `plans/047-cross-host-rootless-lane-expansion.md`.
- A future pinned Java revision that exposes a transport-only direct
  seam may trigger an ADR re-issue that supersedes this rejection.
# Plan 053 status: local integration complete; external run still blocked on host

## Scope

This status documents the Plan 053 corrective pass that integrates Plan 052
evidence primitives into the canonical rootless and Multipass dispatch path on
the `host.apparmor-restrict-on` Plan 046 baseline. It covers Workstreams A, B,
C, D, and E at the local unit, integration, and static-check level. It does
**not** claim a Plan 053 closure, a Milestone 3 certificate, a two-run
reproducibility pass, or that the external run lane is now unblocked.

## What was implemented

### Workstream A: bundle primitives

- `tests/integration/ntcp2/harness/evidence_bundle.py` now classifies files
  through the explicit `classify_bundle_file(staging_root, path)` helper,
  rejects unknown/symlink/non-regular/hidden/case-collision paths, hardens the
  file tree, and reorders manifest validation to reject `record_type=unknown`
  or `schema=unknown` entries.
- `write_json_atomic()` serializes once, hashes the exact bytes written, and
  verifies the on-disk digest on caller request.
- `_validate_manifest_sha256()` enforces the strict
  `<64 hex>  manifest.json\n` format and rejects digest mismatches before
  trusting the manifest.
- `export_bundle_atomic()` writes the export acknowledgement beside the
  immutable bundle using a bundle-relative path; the bundle re-verifies after
  the acknowledgement is written.
- `finalize_bundle()` refuses to overwrite an existing manifest so a
  finalized bundle remains immutable.
- New `test_evidence_bundle.py` cases cover every Workstream A rule and all
  rejection paths.

### Workstream B: run identity

- `plan052_pipeline.build_measured_identity()` measures the live 40-character
  source commit, dirty-state, source tree digest, archive digest,
  launcher binary digest, rustc/cargo versions, target triple, reference lock
  digest, and environment manifest. A dirty tree is now a typed
  `source-tree-dirty` rejection.
- `plan052_pipeline.create_context()` / `load_context()` copy the frozen
  identity into the staging root, refuse a symlinked or mismatched bundled
  identity, and expose `RunContext.assert_frozen()` that detects mutations
  after the first direction starts.
- The cross-check helper `run_identity.cross_check()` continues to validate
  `run_id`, `source_commit`, `launcher_binary_sha256`, and
  `run_identity_sha256` on every per-direction record.

### Workstream C: canonical execution path

- `scripts/interop/rootless-enter.sh` validates the run identity and bundle
  staging paths via `validate_owned_path()`, refuses symlinks, and propagates
  `--run-id`, `--run-identity`, `--bundle-staging`, and
  `--evidence-profile milestone-3-v2` to the inner runner.
- `tests/integration/ntcp2/harness/rootless_inner_runner.py` forwards the
  new CLI flags to `mixed_runner.py` and emits
  `blocked_evidence_context_missing` when an evidence profile is supplied
  without a complete context.
- `tests/integration/ntcp2/harness/mixed_runner.py` accepts the new flags,
  refuses unknown profiles with `unsupported-evidence-profile`, and routes the
  bounded responder reason from the launcher terminal record into
  `MixedRunError` instead of collapsing to a generic historical code.

### Workstream D: complete per-direction artifacts

- `plan052_pipeline.write_direction_artifacts()` writes exactly one
  `attestation`, `direction`, `trigger`, `observation`, and `cleanup` record
  per primary direction. The observation record follows
  `i2pr-ntcp2-direction-observation-v2`; missing reference receiver markers
  carry `not-observed` with the typed code
  `reference-receiver-marker-not-source-locked`.
- `plan052_pipeline.finalize_diagnostic_bundle()` cross-checks every record
  against the frozen identity, validates the direction catalog, runs
  `verify_bundle()`, and refuses to finalize any incomplete bundle.
- The mixed runner now writes the canonical Plan 053 records for every
  direction when an evidence profile is supplied, and it suppresses the
  legacy direction JSON write so a Plan 052 record cannot be quietly
  relabeled as Plan 053.

### Workstream E: finalization and export

- `export-evidence.sh` calls `verify_bundle()` followed by
  `export_bundle_atomic()` after transferring the guest bundle, so the
  exported bundle is itself re-verified.
- `dispatch-gate.sh` adds explicit `create-plan052-context` /
  `finalize-plan052-context` steps and threads the explicit run context
  through `run-direction.sh`.
- The `handshake-smoke-rootless` profile now finalizes the Plan 053 bundle
  before exporting, and refuses any run ID that does not match the live
  context.

### Workstream F: tests

- `tests/integration/ntcp2/harness/test_plan053.py` covers the four-direction
  blocked-path, identity mutation, missing-class, and unknown-reason
  rejection paths.
- `tests/integration/ntcp2/harness/test_evidence_bundle.py` adds Plan 053
  cases for Workstream A. `test_harness.py`, `test_rootless_topology.py`,
  `test_multipass.py`, and `test_plan052.py` continue to pass.
- `scripts/check-ntcp2-interoperability.sh` now fails the gate when the
  Plan 052 pipeline is not wired into the mixed runner, when a generic
  historical responder reason is reintroduced, or when an export
  acknowledgement is written inside the bundle.

## What was NOT executed

This status does **not** claim:

- A real Plan 053 external execution on the Multipass recovery lane.
- A Plan 053 closure record.
- A Milestone 3 certificate.
- A two-run reproducibility pass.
- Resolution of the host-side `blocked_unprivileged_user_namespace` Plan 046
  typed blocker (the lane still returns the typed blocker on this host).
- Discovery of source-locked reference receiver markers for either pinned
  reference.
- A working direct Java I2P or i2pd reference trigger helper.

## Local diagnostic result

A complete four-direction diagnostic bundle with result
`diagnostic-complete-not-certificate` is producible from the local harness seam
only. It is re-verified before export and acknowledged beside, never inside,
the immutable bundle. It is **not** interoperability evidence.

## Host baseline (this host)

- OS: Ubuntu (Plan 046 `host.apparmor-restrict-on` baseline).
- `kernel.apparmor_restrict_unprivileged_userns = 1` →
  `probe-rootless-sandbox.sh` returns `blocked_unprivileged_user_namespace`.
- The host user does NOT have `sudo -n` for non-interactive root commands.

## Validation commands executed

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py' → 64 tests
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan053.py'          → 4 tests
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'               → 321 tests, all green
bash scripts/check-ntcp2-interoperability.sh                                                → ok
```

## Decision

Plan 053 is **not** closed by this status. The follow-up closure requires the
Plan 046 rootless lane (or the Plan 048/049/050 Multipass recovery lane) to
produce at least one complete `diagnostic-complete-not-certificate` bundle from
the exact same source commit, with the four primary direction records, every
record satisfying the v2 observation predicate, and every record binding to the
same run identity. A passing Milestone 3 certificate additionally requires
two reproducible runs with both-side source-locked reference receiver markers
and dual authenticated observations.

See `plans/053-plan052-evidence-pipeline-integration-corrective-pass.md` for
the full acceptance criteria.
# Plan 052 status: scaffolding executed; external run-blocked on this host

## Scope

This status documents the Plan 052 scaffolding pass on the
`host.apparmor-restrict-on` Plan 046 negative baseline. It covers
Workstreams A2, A3, B, C, D, E1, F2, G, and H at the unit/static-test
level. It does NOT claim a Plan 052 closure, a Milestone 3 certificate,
or any external NTCP2 interoperability result.

## What was implemented

### Workstream A: freeze and clean the authoritative baseline

- A2 — Removed `check_ri.sh`, `rebuild.sh`, `wrap.sh` from the repository
  root. They were one-off diagnostic shell fragments with embedded
  absolute guest paths. The equivalent functionality remains available
  through the maintained `i2pr-interop ntcp2 inspect` and
  `build-references.sh --force-rebuild` commands.
- A3 — Replaced the binary `I2PR_INTEROP_DUMP_RUN_LOGS` switch with
  the tri-state `I2PR_INTEROP_DIAGNOSTICS=off|sanitized|raw-local` env
  var (`mixed_runner.py:_diagnostics_mode`). The default is `off`. The
  `raw-local` mode is rejected when `INTEROP_EVIDENCE_DIR` is set,
  emitting the typed blocker `raw-local-diagnostics-forbidden-under-export-root`.
  `rootless_inner_runner.py` no longer sets the diagnostic dump
  unconditionally; it forwards whatever the operator selected (default
  `off`).

### Workstream B: single-source provenance

- B1 — Introduced `tests/integration/ntcp2/harness/run_identity.py`
  with the `i2pr-interop-run-identity-v1` schema (24 required fields
  including `source_commit`, `source_tree_sha256`, `source_archive_sha256`,
  `launcher_binary_sha256`, `rustc_version`, `cargo_version`,
  `target_triple`, `topology_kind`, `privilege_model`, and
  `evidence_schema_revision`).
- B2 — Added `cross_check(record, identity)` that rejects mismatched
  `run_id`, `run_identity_sha256`, `source_commit`, or
  `launcher_binary_sha256`.
- B3 — Extended `evidence.py` with an opt-in `RUN_IDENTITY_BIND_FIELDS`
  suffix (`source_commit`, `launcher_binary_sha256`,
  `run_identity_sha256`) and `RUN_IDENTITY_STANDALONE_FIELDS` (the
  same plus `run_id`). The suffix validates that passed records do not
  carry zero-filled provenance.
- B4 — Added `test_plan052.py` with 35 tests covering the schema,
  short SHA rejection, dirty-state rejection, cross-check rejects,
  writer round-trips, and digest-mismatch failures.

### Workstream C: durable evidence bundles

- C1 — Defined the bundle layout under
  `target/interop/evidence/milestone-3/<run-id>/` with
  `run-identity.json`, `environment/`, `attestations/`, `directions/`,
  `triggers/`, `observations/`, `cleanup/`, `diagnostics/`, `manifest.json`,
  and `manifest.sha256`.
- C2 — Added `tests/integration/ntcp2/harness/evidence_bundle.py` with
  `finalize_bundle()`, `verify_bundle()`, and `export_bundle_atomic()`
  that copy through a temporary directory, verify all hashes, and
  atomically rename to the final location.
- C3 — `validate_direction_catalog()` enforces exactly the four primary
  direction IDs in each direction class. Missing, substituted, or
  extra direction IDs fail the bundle.
- C4 — Self-consistent hashes: file SHA-256 in the manifest, logical
  record SHA-256 inside each record. The two are never confused.
- C5 — Added `test_evidence_bundle.py` with 23 tests covering manifest
  generation, atomic export, tampering detection, extra-file rejection,
  and typed-absence semantics.

### Workstream D: receiver-side evidence

- D1 — `tests/integration/ntcp2/harness/observation.py` defines
  `i2pr-ntcp2-direction-observation-v2` with bounded levels
  (`process_started`, `listener_ready`, `tcp_connected`,
  `ntcp2_authenticated`, `frame_emitted`,
  `frame_authenticated_and_decrypted`, `i2np_message_decoded`,
  `terminal_clean`).
- D2 — `receiver_passes_data_phase()`, `sender_emitted_data_frame()`,
  and `both_authenticated()` implement the Plan 052 directional
  predicate.
- D3 — `tests/integration/ntcp2/reference-observation-catalog.md`
  binds each pinned reference's observation markers to source paths,
  symbols, and revisions. Open source-inspection work is documented
  per reference.
- D5 — `known_deviation` allowlist retains historical reasons; the
  Plan 052 directional predicate is enforced in `observation.py`,
  not via `known_deviation`.

### Workstream E: Java startup probe

- E1 — `tests/integration/ntcp2/harness/java_startup_probe.py` provides
  a standalone probe with `--reference-install`, `--data-dir`,
  `--data-state`, `--launcher {runplain,wrapper}`,
  `--namespace {outer,rootless}`, `--sequence {single,generate-live}`,
  `--attempts`, and `--output`. The probe never opens an NTCP2 peer
  connection. It exits non-zero when no attempt could start or when
  readiness is not observed, and emits a sanitized failure record.

### Workstream F: reference-initiated triggers

- F2 — `tests/integration/ntcp2/reference-trigger-contracts.md`
  documents the candidate source paths, symbols, and dispositions for
  Java I2P 2.12.0 and i2pd 2.60.0 direct NTCP2 transport seams.
  Both helpers are pending source inspection; the two reference-
  initiated directions remain typed blockers until the helpers exist.

### Workstream H: harden validation

- The new `RUN_IDENTITY_BIND_FIELDS` suffix on the existing evidence
  record coexists with `MULTIPASS_RECORD_FIELDS` and the base
  `RECORD_FIELDS`. The aggregate validator behavior is unchanged for
  pre-Plan 052 records; new Plan 052 records must carry the suffix.

## What was NOT executed

This status does not claim:

- A real Plan 052 external execution.
- A Plan 052 closure record.
- A Milestone 3 certificate.
- A two-run reproducibility pass.
- Resolution of the host-side `blocked_unprivileged_user_namespace`
  Plan 046 typed blocker (the lane still returns the typed blocker on
  this host).
- Resolution of the Java intermittent shutdown root cause (the
  diagnostic probe exists but no controlled matrix was run here).
- A source-inspected reference-initiated direct NTCP2 trigger helper
  for either pinned reference.

## Host baseline (this host)

- OS: Ubuntu (Plan 046 `host.apparmor-restrict-on` baseline).
- `kernel.apparmor_restrict_unprivileged_userns = 1` →
  `probe-rootless-sandbox.sh` returns `blocked_unprivileged_user_namespace`.
- The host user does NOT have `sudo -n` for non-interactive root
  commands.
- Physical RAM: 15 GiB. Swap: 83 GiB.
- This status does not advance the Plan 052 closure path; it documents
  the in-lane scaffolding produced without external execution.

## Boundary checks

All static checkers remain green:

```text
bash scripts/check-dependency-direction.sh        → ok
bash scripts/check-runtime-boundaries.sh          → ok
bash scripts/check-ntcp2-interoperability.sh      → ok
bash scripts/check-rootless-interop-boundary.sh   → ok
bash scripts/check-multipass-interop-boundary.sh  → ok
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py' → 273 tests, all green
```

## Decision

Plan 052 is not closed by this status. The follow-up closure requires
the multipass recovery lane (Plan 049/050/051) to produce at least two
complete accepted bundles from the exact same source commit, each
containing the four primary direction records, every record satisfying
the v2 observation predicate, and every record binding to the same run
identity. See `plans/052-ntcp2-milestone-3-evidence-closure-follow-up.md`
for the full acceptance criteria.
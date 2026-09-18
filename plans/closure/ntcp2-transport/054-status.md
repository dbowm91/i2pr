# Plan 054 status: Java startup and reference-observation qualification

Plan 054 implements the Java startup matrix, the source-locked
reference observation catalog, the catalog-driven `collect_observation`
adapters, and the Plan 052 directional predicate that consumes those
observations. This status record summarises the local state on
`main` after Plan 053; the external matrix and qualification runs
remain unstarted on the canonical Ubuntu lane (see "External
qualification status" below).

## Java startup matrix

- The Plan 054 16-cell matrix driver lives in
  `tests/integration/ntcp2/harness/java_matrix.py`. It composes
  `java_startup_probe.py` once per cell with three independent
  attempts each and writes a sanitized per-cell record under
  `target/interop/java-startup-matrix/<matrix-run-id>/<cell>/<attempt>/`.
- `java_startup_probe.py` now accepts `seeded-clone` (a frozen
  template copy), the bounded entropy probe (`getrandom_result`,
  `latency_bucket_ms`, `seed_file_state`, `seed_file_sha256`),
  `qualification-mode` for the ten-consecutive-start gate, and the
  twelve Plan 054 failure stages
  (`java-process-spawn-failed` through `java-state-lock-invalid`).
- The Java template preparation driver
  (`scripts/interop/java-prepare-template.py`) is the only path that
  may download, install, or seed Java state. It freezes the resulting
  template tree into a deterministic SHA-256 digest and writes
  `template-manifest.json` plus `template-tree.sha256`. The execution
  phase is restricted to `seeded-clone` and never re-launches the
  frozen template directly (`template-launch-forbidden`).

## Reference observation catalog

- The machine-readable catalog is
  `tests/integration/ntcp2/reference-observation-catalog.toml`
  (schema `i2pr-reference-observation-catalog-v1`, revision 1). It
  binds each marker to the exact pinned source path, symbol, marker
  text, sanitization rule, and minimum count.
- The catalog covers both pinned references and both
  data-phase levels. Handshake-only markers are explicitly forbidden
  on data-phase levels by `validate_catalog()`.
- The Markdown file
  (`tests/integration/ntcp2/reference-observation-catalog.md`) is now
  a generated, drift-checked explanatory document, not the source of
  truth.

## Adapter integration

- `JavaI2pAdapter.collect_observation()` and
  `I2pdAdapter.collect_observation()` emit finalized
  `i2pr-ntcp2-direction-observation-v2` records driven by a
  per-run `LogCursor` and the catalog. They never infer data
  acceptance from i2pr sender counters.
- The shared scanner (`observation_helpers.py`) enforces
  exact-string marker matching, per-side state derivation, log
  sanitization, and post-cursor observation only.
- `mixed_runner._evaluate_plan052_predicate` no longer hardcodes a
  rejection. It accepts a real `observation-v2` record from
  `ref_adapter.collect_observation()` and applies the
  `receiver_passes_data_phase` predicate.
- `plan052_pipeline._build_observation()` accepts the live
  `i2pr_observation` and `reference_observation` records; the synthetic
  builder remains as the typed fallback for blocked and rejected
  directions.

## Control-experiment status

- Catalog tests: `tests/integration/ntcp2/harness/test_plan054.py` —
  exact pinned revision required, handshake marker cannot claim a
  data level, duplicate semantic level rejected, unrecognized
  sanitizer rejected, markdown/TOML drift detected.
- Observation tests: positive case satisfies `receiver_passes_data_phase`,
  handshake-only does not, malformed frame does not, invalid I2NP does
  not, stale log markers are not counted, wrong correlation value does
  not count, observation digest changes when any level changes,
  sanitization strips synthetic endpoints.
- Integration tests: the Java template cannot be launched directly
  after freeze, the template tree digest is stable across rebuilds,
  the matrix covers 16 cells, and `_evaluate_plan052_predicate` now
  requires both decrypt and decode levels (not just handshake).

## External qualification status

- The complete 48-start matrix, the ten-consecutive-start
  qualification, and the source-inspected control experiments
  require the pinned Java/I2P 2.12.0 and i2pd 2.60.0 references
  inside an authorized Ubuntu 24.04 amd64 host or Multipass guest.
  Those runs are not performed on this host (the host is the
  `apparmor_restrict_on` Plan 046 negative baseline and the
  Multipass recovery lane has not been exercised in this plan).
- A complete Plan 052 diagnostic bundle requires the live
  reference receiver markers to be observed; the current local
  bundle remains `diagnostic-complete-not-certificate` and
  Milestone 3 stays open.
- The two i2pr-initiated directions may reach the full Plan 052
  predicate on a host where the Java or i2pd reference is available
  and the catalog markers fire. Both reference observations and
  per-direction controls must pass before any `passed` record is
  emitted.

## Required handoff artifacts

- `target/interop/java-startup-matrix/<matrix-run-id>/` — matrix
  summary JSON and per-cell records. Not yet produced on this host.
- `target/interop/java-template/<digest>/template-manifest.json`
  plus `template-tree.sha256`. The local checkout exposes the
  preparation driver but the reference installer is not available
  on this host.
- `tests/integration/ntcp2/reference-observation-catalog.toml` —
  present and revision-locked.
- `tests/integration/ntcp2/reference-observation-catalog.md` —
  present, drift-checked, demoted to explanatory documentation.
- Adapter implementations and control tests — present under
  `tests/integration/ntcp2/harness/{java_i2p,i2pd,observation_helpers,
  observation_catalog,java_matrix}.py` plus `test_plan054.py`.
- One Plan 052 diagnostic bundle — produced by the Plan 053 lane
  with `diagnostic-complete-not-certificate` status; the per-direction
  records now bind the Plan 054 observation-v2 records when the
  runner provides them.
- This status document.

## Validation

- `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'`
  — 346 tests pass.
- `bash scripts/check-ntcp2-interoperability.sh` — passes with the
  Plan 054 catalog, adapter, predicate, and template checks.
- `bash scripts/check-dependency-direction.sh`,
  `bash scripts/check-runtime-boundaries.sh`,
  `bash scripts/check-rootless-interop-boundary.sh`,
  `bash scripts/check-multipass-interop-boundary.sh` — pass.
- `cargo fmt --all --check`, `cargo check --workspace --all-targets`,
  `cargo test --workspace`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` — pass.

## Open blockers

- Reference artifacts are not installed on this host, so the matrix,
  qualification, and live Plan 052 bundle runs remain unstarted. The
  Plan 048/049 Multipass recovery lane is the canonical path to run
  them.
- A clean source commit, two reproducible diagnostic bundles, and the
  full Plan 052 predicate are still required to close Milestone 3.
  Plan 054 does not claim closure.

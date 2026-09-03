# Plan 151 status — Milestone 7 SAM 3.1 final acceptance evidence correction

Status: **`active-m7-sam31-final-acceptance-evidence-correction`**.

Registered: **2026-09-03**.

Plan of record:
[`plans/151-m7-sam31-final-acceptance-evidence-correction.md`](151-m7-sam31-final-acceptance-evidence-correction.md).

## Current authority

Plan 149 remains the passed self-composing localhost SAM product authority.
Plan 150 retains successful external-client core evidence, but its broad final
closure interpretation is superseded by Plan 151 because several acceptance
items were recorded as passed without an executable test on the closing lane.

```text
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure
plan_146 = passed-m7-sam31-private-destination-reference-requalification
plan_147_raw_driver = landed-and-retained
plan_148 = blocked-audit-historical-superseded
plan_149 = passed-m7-sam31-self-composing-local-product-corrective

plan_150_external_core_evidence = retained-passed
plan_150_final_acceptance = superseded-by-plan151
plan_151 = active-m7-sam31-final-acceptance-evidence-correction

milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
milestone7_local_product = passed-via-plan149
milestone7_sam_localhost_final_acceptance = not-yet-closed
sam_independent_clients = at-least-two-passed-via-plan150
router_to_router_interoperability = not-claimed

next_executable_plan = 151
next_product_layer = remain-on-milestone7
```

## Retained Plan 150 evidence

Do not discard these successful results while executing Plan 151:

- routine CI and the manual SAM external-client workflow passed on the audited Plan 150 head;
- pinned i2psam and the qualified pinned i2plib SAM surface exchanged exact 2 MiB payloads in both cross-client directions;
- private-destination import/generation passed through both counted clients;
- SILENT transcript evidence passed;
- NAMING and negative-input matrices passed;
- positive STREAM FORWARD to a real loopback target passed;
- official libsam3 was built/probed and correctly not counted because its public API rejects i2pr's compact Ed25519 `PRIV` shape;
- Plan 149's self-composed black-box product path remained green.

## Why final closure is reopened

The Plan 150 harness currently records at least one required result as passed
without executing the acceptance case:

```text
record "multiple-stream-lifecycle" passed "retained Plan 149 black-box sibling/lifecycle suite"
```

The referenced Plan 149 black-box suite has four tests and does not contain a
two-sibling-stream isolation test.

Plan 149 explicitly deferred the slow-reader/slow-writer, fault matrix, and
sibling-stream acceptance items to Plan 150. Plan 150's acceptance criteria
retain those requirements, but the current Plan 150 harness does not execute
them before generating its final evidence summary.

The positive FORWARD lane is also useful but narrower than the full Plan 150
FORWARD lifecycle/negative matrix.

Therefore Plan 150 remains valid external-client core evidence but is no
longer the final Milestone 7 closure authority.

## Plan 151 required closure areas

Plan 151 must add executable evidence for:

1. evidence-ledger integrity / no synthetic pass rows;
2. two simultaneous sibling STREAMs and close-one/keep-one isolation;
3. slow-reader boundedness;
4. slow-writer/reverse-pressure boundedness;
5. DATA-drop retransmission;
6. ACK-drop recovery;
7. duplicate DATA exact-once application delivery;
8. DATA reorder -> ordered application delivery;
9. authenticated/ciphertext corruption rejection;
10. retransmission-ceiling bounded terminal behavior;
11. CLOSE/RESET/control-session lifecycle cleanup;
12. full FORWARD lifecycle/negative matrix;
13. explicit focused Plan 127–134 regression execution;
14. final external-client rerun and hosted workflow on the exact closing head.

## Executed evidence (2026-09-03, current head work in progress)

Only executed commands count; nothing here is marked passed from prose.

- New final-acceptance suite
  `crates/i2pr-daemon/tests/sam_stream_final_acceptance.rs`
  (black-box TCP/SAM only after listener start; deterministic
  fault seam via pre-start `install_test_fault_profile`):
  **10/10 pass** via `cargo test --locked -p i2pr-daemon --test
  sam_stream_final_acceptance -- --test-threads=1` (~328 s).
  Covers rows 2–11 above: sibling close-one/keep-one, slow-reader
  (6 × 2 MiB, gauges ≤ ceilings, exact recovery, zero typed sweep
  failures), slow-writer mirror, DATA-drop, ACK-drop, duplicate,
  reorder, corruption, retransmit-ceiling, close/reset lifecycle,
  privacy-log.
- Plan 151 §17 stop fired during this work: the new tests exposed
  three genuine M6 defects (unbounded receiver retention,
  duplicate-never-re-ACKs, sender ECIES ratchet-key retention).
  Narrow corrective recorded in
  [`plans/152-m6-session-streaming-robustness-corrective.md`](152-m6-session-streaming-robustness-corrective.md);
  no wire change. Manager/ECIES unit tests + full
  `-p i2pr-client --all-targets` green.
- Focused regression seams green: `-p i2pr-api --all-targets`,
  `-p i2pr-client --all-targets`, `sam_loopback`,
  `sam_plan146_reference`, `sam_stream_product`,
  `sam_stream_independent`, `sam_stream_raw_product`,
  `sam_stream_self_composed`, `sam_forward_naming`.
- Still open: row 1 (evidence-integrity checker script), row 12
  (FORWARD lifecycle/negative matrix), row 13 (explicit 127–134
  regression commands), row 14 (external-client rerun on the
  closing head), full workspace floor (§16 gates), docs
  (`support.toml`, README/AGENTS/skills/architecture), commit and
  remote CI.

## Executed evidence, continued (2026-09-03)

- Evidence-integrity checker
  `scripts/check-sam-acceptance-evidence.sh` (**new**): 22 required
  rows must flow through `record_guarded` (exit-code gate),
  `plan151_row` (suite rc + own ok-line), or the dual-exit
  run_stream_pair helper; literal `record "<row>" passed` lines
  fail the check. Verified green and verified it rejects a planted
  synthetic row. It runs in the routine-adjacent gates slice.
- `tests/integration/sam/run-independent.sh` reworked (§5.1, §12):
  the two synthetic rows are gone. `binary-matrix` now aggregates
  four executed cross-client rounds (2 MiB mixed/prefix + 4 KiB
  crlf/all-bytes, both directions);
  `multiple-stream-lifecycle` derives from the executed Plan 151
  sibling test; eleven new rows (sibling/slow-reader/slow-writer/
  six faults/close-reset/forward-lifecycle/plan127-134/
  workspace-gates) derive from the new suites, the FORWARD suite,
  eight regression commands, and a fmt/check/static-scripts slice.
  Evidence JSON carries per-row status plus commit/lane/toolchain.
- FORWARD matrix (§10, all 8 items): existing tests cover positive
  metadata, SILENT, and owner-close; five new tests in
  `sam_forward_naming.rs` cover second-stream reuse, refusal
  (typed I/O, registration survives, retry succeeds), 3 s timeout
  under a saturated backlog, non-loopback/hostname rejection, and
  ACCEPT↔FORWARD mutual exclusion. **8/8 pass**.
- Plan 127–134 regressions (§11): `plan127_trajectory` (16),
  `plan128_wire` (11), `plan128_trajectory` (7),
  `plan129_trajectory` (12), `plan130_trajectory` (11),
  `plan131_trajectory` (7), `plan132_trajectory` (10),
  `i2pr-crypto --all-targets` (52) — **all green**, commands
  recorded verbatim in the lane.
- External lane (§12): `fetch-sam-clients.sh --rebuild` +
  `clients/build.sh` + reworked `run-independent.sh` executed on
  the worktree; **26/26 rows passed**, evidence under
  `target/interop/sam-evidence` (10/10 acceptance, 8/8 forward in
  the captured logs). Must be rerun on the exact closing commit;
  the manual `.github/workflows/sam-external.yml` lane must then
  run that head.
- Full workspace floor (§16): fmt, check, workspace tests
  (~1288 passed, 0 failed), clippy `-D warnings`, doc, doctests,
  all six static boundary scripts, evidence checker, ntcp2 python
  harness (153), cargo deny — **all green** (this head, pre-commit).

## Handoff

Execute `plans/151-m7-sam31-final-acceptance-evidence-correction.md`.

Do not begin Milestone 8 implementation until this status is replaced by an
explicit passing Plan 151 closure record backed by executable evidence for
every required final acceptance row.
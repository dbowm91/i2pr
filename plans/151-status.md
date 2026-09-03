# Plan 151 status — Milestone 7 SAM 3.1 final acceptance evidence correction

Status: **`passed-m7-sam31-final-acceptance-evidence-correction`**.

Registered: **2026-09-03**. Closed: **2026-09-03**.

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
plan_151 = passed-m7-sam31-final-acceptance-evidence-correction
plan_152 = passed-m6-session-streaming-robustness-corrective

milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
milestone7_local_product = passed-via-plan149
milestone7_sam_localhost = passed-via-plan151
milestone7_sam_localhost_final_acceptance = closed
sam_independent_clients = at-least-two-passed-via-plan150
router_to_router_interoperability = not-claimed

next_executable_plan = none-milestone7-closed
next_product_layer = milestone8-planning
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

~~Execute `plans/151-m7-sam31-final-acceptance-evidence-correction.md`.~~
Done — closure recorded below. Do not begin Milestone 8
implementation until a Milestone 8 plan-of-record exists; the
Milestone 7 SAM localhost product stays experimental, loopback-only,
disabled by default, and non-advertised.

## Closure record (Plan 151 §18)

- Closing commit: `02e47aa69a2574165aadd4c28df1128845eb94ab`
  (`plan151: establish sibling pairs sequentially for deterministic
  pairing`).
- Routine CI on the closing head: run `33788586572`, conclusion
  `success` — Quality ubuntu, Quality macos, MSRV, dependency-policy
  all green.
- SAM external workflow on the closing head: run `33790521635`
  (`workflow_dispatch` of `.github/workflows/sam-external.yml` at
  `main`), conclusion `success`; artifact
  `sam-external-evidence-33790521635` uploaded.
- External clients (exact, unmodified, localhost-only):
  counted `i2psam` at `b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac`;
  counted `i2plib.sam` substitute at
  `6edf51cd5d21cc745aa7e23cb98c582144884fa8`; built/probed but not
  counted `libsam3` at `7d6e658798baec31394c5685f9583343cc00900b`
  (public API rejects the compact Ed25519 PRIV shape).
- Hosted evidence (`evidence.json`, lane `github-33790521635`,
  `rustc 1.95.0`): **26/26 rows passed**, zero non-passed rows;
  local rerun on the same head agreed 26/26.
- Acceptance rows and their proving commands (all executed on the
  closing head, locally and hosted):
  sibling-stream-isolation + multiple-stream-lifecycle —
  `cargo test --locked -p i2pr-daemon --test
  sam_stream_final_acceptance plan151_sibling_streams_isolate_close_one`;
  slow-reader / slow-writer —
  `... plan151_slow_reader_stays_bounded_and_recovers` /
  `... plan151_slow_writer_reverse_pressure_recovers` (6 × 2 MiB,
  gauges ≤ ceilings, exact recovery, zero typed sweep failures);
  fault-data-drop / fault-ack-drop / fault-duplicate /
  fault-reorder / fault-corruption / fault-retransmit-ceiling —
  the six `plan151_fault_*` tests in the same suite;
  close-reset-lifecycle — `... plan151_close_reset_lifecycle`;
  forward-lifecycle — `cargo test --locked -p i2pr-daemon --test
  sam_forward_naming` (8/8: register/bridge/second-stream/refusal/
  timeout/non-loopback/exclusion/owner-close + naming);
  plan127-134-regressions — `plan127_trajectory` (16),
  `plan128_wire` (11), `plan128_trajectory` (7),
  `plan129_trajectory` (12), `plan130_trajectory` (11),
  `plan131_trajectory` (7), `plan132_trajectory` (10),
  `i2pr-crypto --all-targets` (52), all green;
  external-client-a-to-b / b-to-a / private-destination /
  binary-matrix / silent / naming / negative-matrix /
  forward-positive — the reworked `run-independent.sh` external
  matrix with pinned clients; workspace-gates — fmt + workspace
  check + static boundary scripts in-lane, full floor in routine CI.
- Full §16 floor on the closing head: fmt, check, workspace tests,
  clippy `-D warnings`, doc, doctests, six boundary scripts,
  evidence checker, ntcp2 python harness, cargo deny — all green
  locally; routine CI repeats fmt/check/tests/clippy/doc/boundaries
  on ubuntu + macos.
- Sanitization: evidence carries commands, exit statuses, commit,
  lane, client revisions, byte counts, and counter snapshots only;
  no PRIV, seeds, secrets, payloads, or environment dumps. The
  `privacy-log` acceptance test pins the failure-path log silence.
- Superseding: `plan_150_final_acceptance = superseded-by-plan151`
  retained; Plan 150 core evidence stays valid history.
- Criterion 26 satisfied: `milestone7_sam_localhost = passed`,
  `next_product_layer = milestone8-planning`.
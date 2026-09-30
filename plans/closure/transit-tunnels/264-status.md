# Plan 264 — M11 single-mesh sustainability scoping: status

**retained-m11-per-epoch-gate-proven-emission-windows-bound-full-matrix-scoping-insufficient-corrective-required-via-plan265**

## Disposition

Plan 264 executed as a narrow qualification-scoping corrective plus
final pass and **stops at one exactly localized per-epoch
emission-window boundary**. It does NOT close the lane:

- The per-epoch lane landed with zero production diff: the
  `I2PR_M11_ONLY_EPOCH` selector is now the counted per-epoch
  mechanism for all 13 mandatory epochs (builds, data, receipt,
  lifecycle) with inline setup prerequisites (participant build +
  forward cell) on the same fresh mesh; only the manifest-named
  epoch counts. The manifest carries `plan: 264` + `epoch` +
  `epoch_pass` + `epoch_qualification` (passed iff driver exit 0
  with the epoch's terminal key present) under schema v4. The new
  `scripts/check-m11-per-epoch-composition.sh` composes closure
  ONLY from per-epoch manifests (two same-SHA distinct-pass
  manifests per epoch, no cross-epoch merge, full-matrix
  diagnostic-only). Five `plan264_*` composition regressions plus
  the proven --compose positive/negative/mixed-SHA/full-matrix
  behavior bind the gate. No timeout/quota/ceiling/retry/
  message-size constant changed (frozen Plan 262 numerics intact).
- The gate demonstrably WORKS: 5 epochs close 2/2 per-epoch on
  the closure SHA (`obep`, `ibgw`, `participant`, `reject`,
  `obep-data`), and `--compose` over the 18 real manifests
  counts exactly those passes while failing closed with the
  precise missing set (no merge, no waiver).
- Four emission-dependent epochs stop on window signatures with
  no production row ever failing and B3 holding (`terminal-
  garlic-self:0` in both receipt runs; no self-terminal
  anywhere; B-side containment proven for the replay singleton):
  `ibgw-data` 1/2 (single-cell-only 4-ingress window), `receipt`
  0/2 (`0/0/0` + `8-ingress/0-socket` starvation),
  `participant-data` 1/2 (no-forward starvation),
  `replay` 0/2 (forward-setup stops). The remaining lifecycle
  epochs (`expiry`, `cancel`, `session-close`, `restart`) are
  structurally blocked behind the forward cell (code path +
  replay setup stops) and were not executed.
- The scoping hypothesis is therefore REFINED, not confirmed:
  fresh-mesh-per-epoch restores determinism for builds and
  locally-driven data (OBEP-data 2/2) but NOT for
  reference-emission windows (fragmentation, socket delivery,
  forward routing), which stay ~50/50 per fresh mesh. No
  tuning, no retry-until-green, no reference patching, no
  public fallback, no new dependency, no RouterInfo/version/
  capability change. M11 remains unclaimed/non-advertised;
  M12 remains deferred; the narrow successor is Plan 265
  (per-epoch emission sustainability), registered ready in
  the same commit.

## Commits

- `1d4ed3356a6080ea806eb67070b90a4cd6213e69` — Plan 264
  implementation (per-epoch lane: fine-grained epoch gates,
  setup prerequisites, lifecycle chain, composition
  predicates + 5 unit rows, runner manifest v4 shape,
  composition script, workflow epoch/pass inputs, checker
  rules; zero `crates/*/src` diff).
- `9e28f0d3deb55c15d144e6aa59d09d360c326a64` — lane fix
  (per-epoch verdict `epoch_qualification` via
  `EPOCH_TERMINAL_KEY` + `I2PR_M11_DRIVER_RC`; composition
  reads the verdict; prior-SHA runs retained as diagnostic).
- `6ab9dc2dd80526ce38e62014635ec3e3ad5521f5` — lane
  hardening (persist ledger before the replay predicate for
  failure forensics; no new keys, no threshold change).
- this commit — this record + Plan 265 registration +
  registry/roadmap/support/conformance/dossier reconciliation
  (closure SHA recorded at commit time).
- Follow-up: hosted exact-head CI evidence record (Plan
  258/259/260/261/262/263 precedent; local floor fully green
  below).

Baseline: `659c1587c1fef105df3405c9c026732ee2e43f3e` (Plan 263
closure + Plan 264 registration head; planning/spec-only
reconciliation through that head does not change the
production source baseline inherited from Plan 262, and Plan
264 itself adds zero production diff).

Reference: unmodified i2pd 2.61.0 @
`635b013a612ff47278ef02acf8580a28e10e26c5` throughout.

## Implementation contents

Harness-only corrective (no wire format, task/channel/queue,
quota, timeout, config/CLI/API/RI/version change; verified
`git diff --name-only 659c158..HEAD | grep -E '^crates/.*/src/'`
empty):

- `crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs` —
  `PLAN264_MANDATORY_EPOCHS` (13) + `plan264_epoch_is_mandatory`
  (`ibgw-receipt` alias, legacy `data` never counted) +
  `plan264_epoch_passes_satisfy` (same epoch + same SHA +
  distinct passes) + `plan264_composition_covers_all_epochs` +
  `plan264_single_mesh_is_diagnostic_only` + 5 unit rows;
  `needs_participant_setup` / `needs_forward_setup` /
  `run_obep_data` / `run_ibgw_data_epoch` /
  `run_receipt_epoch` (receipt + alias) /
  `run_participant_data_epoch` / `run_lifecycle_chain`
  gates; per-epoch setup prerequisites inline on the same
  fresh mesh; retained Plan 263 proofs (mesh-liveness +
  relay-NetDB + B-floodfill) still gate every counted send;
  ledger flush before the replay predicate (forensics).
- `tests/integration/m11-transit/run-i2pd.sh` — manifest
  `plan: 264` + `epoch` + `epoch_pass` +
  `epoch_qualification` + `epoch_terminal_key` (schema v4),
  `EPOCH_TERMINAL_KEY` map (13 terminal keys),
  `I2PR_M11_DRIVER_RC` export, per-epoch env passthrough,
  full-matrix marked diagnostic-only, composition checker
  wired into both gate slices (19 scripts).
- `scripts/check-m11-per-epoch-composition.sh` — NEW:
  static invariants (driver/ runner/workflow/no-tuning)
  + `--compose` validation (two same-SHA distinct-pass
  manifests per epoch, terminal-key presence, full-matrix
  never counted, mixed-SHA rejected). Verified: synthetic
  positive composes (13x2), missing-pass/mixed-SHA/
  full-matrix negatives fail closed; real-manifest
  composition counts exactly the 5 closed epochs and lists
  the missing set.
- `scripts/check-m11-transit-boundaries.sh` — rule 29
  requires `plan: 264` + epoch/pass/DRIVER_RC/TERMINAL_KEY
  (rejects 263/262); rule 30 adds workflow epoch/pass
  inputs; new rule 37 requires the 17 Plan 264 driver
  symbols + composition checker existence.
- `scripts/check-m11-transit-qualification-evidence.sh` —
  manifest gate requires `plan: 264` (rejects 263/262 and
  older); new Plan 264 section requires the 5 composition
  rows, 7 per-epoch symbols, epoch/pass/DRIVER_RC manifest
  shape, and the frozen numerics (second lock).
- `.github/workflows/m11-transit-external.yml` — `epoch` +
  `epoch_pass` dispatch inputs (empty = full-matrix
  diagnostic); attempt matrix retained for per-dispatch
  isolation.

## Plan 264 §10 acceptance criteria — requirement-to-evidence matrix

1. **Plan 263 retained evidence traceable** — PASS. Plan 263
   WP A (mesh-liveness + relay-NetDB + B-floodfill + SAM
   tail + 4 harness rows) remains in tree and green
   (§"Verification"); production source is byte-identical
   to Plan 263 (zero diff proof above).
2. **Receipt + IBGW multicell re-proven on the
   implementation SHA** — PARTIAL (multicell yes, receipt
   no). IBGW multicell closes per-epoch on healthy windows
   (`ibgwdata-p1`: 15 ingresses, `multicell-bounded =
   true`, `emitted-max = 2`, driver exit 0, 6ab9dc2d);
   receipt never closes on this SHA (0/0/0 + 8/0, below).
   The 9e28f0d diagnostic set additionally re-proved
   multicell twice (16→max2, 22→max2) and receipt partial
   (5 ingresses, B3 holding).
3. **Three source locks hold** — PASS (runner-enforced on
   every run; checkers enforce statically).
4. **No production `crates/*/src` diff (scoping-only)** —
   PASS (verified above; all three M11 checkers lock the
   Plan 262 production shapes unchanged).
5. **Every mandatory epoch passes twice on the same SHA,
   each pass on a fresh mesh, unmerged** — FAIL
   (boundary, not a semantic row failure). Same-SHA
   `6ab9dc2d`, fresh datadirs/ports/evidence per pass,
   loopback-only, unmerged (`/tmp/m11-264f-*`,
   `/tmp/m11-264r-*`):
   - `obep` 2/2 CLOSED (driver exit 0 both).
   - `ibgw` 2/2 CLOSED.
   - `participant` 2/2 CLOSED.
   - `reject` 2/2 CLOSED.
   - `obep-data` 2/2 CLOSED.
   - `ibgw-data` 1/2: p1 closes (15 ingress, max 2);
     p2 stops single-cell-only (4 ingresses, `nested-
     multi 0`, `emitted-max 1`).
   - `receipt` 0/2: p1 `terminal-garlic-self:0/
     ingress:0/socket:0` (220.7 s); p2 `terminal-
     garlic-self:0/ingress:8/socket:0`
     (`gateway-receipt = 0`, 202.5 s).
   - `participant-data` 1/2: p1 closes (forward +
     far-side); p2 stops no-forward (163.9 s).
   - `replay` 0/2: both stop at the forward setup
     (160.7 s, 161.1 s, "no genuine forward").
   - `expiry` / `cancel` / `session-close` /
     `restart`: not executed — structurally blocked
     behind the forward cell (the lifecycle chain
     requires `needs_forward_setup`; replay's four
     setup stops bind the dependency).
   No production row failed; B3 holds (0 self-terminals
   in both receipt runs with sends occurring).
6. **Full workspace verification passes** — PASS local
   (§"Verification"); exact-head CI follows per precedent
   (this commit + follow-up run-ID record).
7. **Exact-head ordinary CI passes all four jobs** —
   PENDING at write time (follow-up commit records the run
   ID; local floor is the current truth).
8. **Registry/roadmap/support/conformance/dossier agree on
   Plan 264** — PASS (this commit reconciles all five;
   Plan 265 registered as the narrow emission successor,
   not a second closure authority; see §"Planning
   authority").
9. **No product default/capability/version/public-network
   change** — PASS (transit stays disabled-by-default and
   non-advertised; `cargo deny` clean).
10. **No critical/high finding remains open** — RECORDED
    WITH OWNERS (see §"Findings": one MEDIUM per-epoch
    emission-window finding owned by Plan 265, one LOW
    replay-predicate gap owned by Plan 265; no open
    production defect).

Retained Plan 263 criteria stay green (production-identical
source; 3130 workspace tests pass). Only criterion 5
(per-epoch two-pass for emission epochs) remains open. The
ADR 0026 one-family M11 experimental qualification is
therefore NOT marked passed; M12 planning stays deferred.

## Executions (all fail-closed, sanitized, unmerged)

Exact pin `635b013a…`, fresh datadirs/ports per pass,
loopback-only. Raw reference logs read for diagnosis only,
never as evidence. Closure SHA `6ab9dc2d` for all counted
passes below (working tree clean at execution; pre-impl
HEAD `659c158`):

- **Counted (6ab9dc2d)**: `obep` p1/p2 PASS; `ibgw` p1/p2
  PASS; `participant` p1/p2 PASS; `reject` p1/p2 PASS;
  `obep-data` p1/p2 PASS (all driver exit 0 with terminal
  keys, `epoch_qualification: passed`); `ibgw-data` p1
  PASS (15/max2) + p2 FAIL (4/single-cell-only);
  `receipt` p1 FAIL (0/0/0, 220.7 s) + p2 FAIL (8/0,
  202.5 s); `participant-data` p1 PASS + p2 FAIL
  (no-forward, 163.9 s); `replay` p1/p2 FAIL
  (forward-setup stops, 160.7/161
...[truncated 9113 chars]
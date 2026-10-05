# Plan 265 — M11 fixed-budget opportunity-qualified emission sustainability: status

**retained-m11-fixed-budget-opportunity-qualified-composition-proven-two-families-closed-receipt-opportunity-generation-below-frozen-bar-corrective-required-via-plan266**

## Disposition

Plan 265 executed the frozen fixed-budget contract exactly as registered and
**does NOT close M11**. The composition gate was run **once** against the
complete retained 24-attempt set and returned one precise violation:

```text
check-m11-per-epoch-composition: family receipt: requires at least two
retained successes, found 1
check-m11-per-epoch-composition: 1 violation(s)
```

What that run establishes:

- The mechanism Plan 265 was registered to build **works and is proven by
  its own gate**: manifest v5, the closed 16-token terminal vocabulary, the
  three input-side opportunity predicates, the fixed 8-attempt budget per
  family, the single qualification SHA, and the composition gate with its 13
  positive/negative fixtures all landed with **zero production diff** from
  Plan 262.
- **Two of the three scenario families close**: `ibgw-data` 4/8 qualified
  successes and `participant-lifecycle` 5/8, with **zero i2pr semantic
  failures** in either.
- **The `receipt` family is below its frozen bar**: 1/8 qualified successes
  against a required 2. The shortfall is entirely in *reference emission /
  external completion opportunity generation*, not in i2pr semantics: 5 of 8
  attempts the reference never produced a B-originated self-targeted action on
  a live counted creator-A IBGW receive id, 1 stopped in setup before the
  family's input-side boundary, and the 2 that did reach the boundary both
  passed i2pr's local source-neutral IBGW seam (multicell emission, correct
  next router, six-field tuple bound) — one completed to the receiver SAM
  socket, one did not.
- **Zero i2pr semantic contradictions occurred anywhere in the 24 attempts.**
  Plan 262 production routing is not reopened.

Per Plan 265 §14 ("any family ends attempt 8 with fewer than two required
successes" is a stop condition) and §14's explicit disposition — *"If fewer
than two successes occur within eight attempts but all i2pr semantic
opportunities pass, the successor owns **qualification opportunity generation
only**"* — the narrow successor **Plan 266** is registered in the same commit
and owns receipt-family opportunity generation only. No Plan 266 was
pre-registered (Plan 265 §14).

## Commits

Plan 265 implementation, in the order landed (all on `main`):

- `fc9947bd835b6983d6beb95d608d98c849b486f` — Plan 265 implementation: the
  three input-side opportunity predicates, the closed terminal vocabulary
  validated by a constructing `Plan265Verdict::new`, explicit replay outcome
  kinds, manifest v5, the `--compose-265` composer with 13 fixtures, and the
  Plan 265 checker/workflow surface. Zero production diff.
- `ebd36e0d584d1f743d55e846c91732e8b2d6c96b` — lane fix: the input-side
  `registration_live_at_input` conjunct, the per-input family A gate, per
  receive id committed-next-router resolution, and the shared
  `check_input_side` guard (verified in both directions).
- `3617e1f18b4c9232356a055db9a9e52b68a1ee89` — lane fix: every typed
  `LiveInboundOutcome` is now retained (the `_ => {}` arm silently
  discarded outcomes outside Build/Data/Gateway), plus the decidable expiry,
  session-close and family-C opportunity bindings.
- `4fd11e51d8237181bdc15620cf43ac46d7348433` — lane fix: family C binds to
  the accepted Participant registration whose committed next router is the
  reference B endpoint, and the restart row proves no old secret-owning state
  rather than an all-zero snapshot its own constructor cannot reach.
- `4682920eb0b28b0b590aaabddb8666253c26f081` — **qualification SHA**: manifest
  v5 carries `lifecycle_rows` / `lifecycle_rows_declared` / `lane_row_scope`,
  and the composer CLI honours a bare `--` attempt separator.
- `9e274a34a37d42b13a854d8c11d850116c9dfe6e` — this record + Plan 266
  registration + registry / roadmap / support / conformance / dossier /
  README reconciliation. Ordinary four-job CI passed on this exact head
  (run `36775035331`); that evidence is recorded in §CI and amended by the
  follow-up commit that carries it, so the CI-amended head is one commit
  ahead of the head that CI actually ran against.

Pre-implementation HEAD: `045265171fec172517a10763e541a99670191ac7`. The five
superseded SHAs above are provenance, not qualification evidence; the
intervening attempts on them are retained as diagnostic only and are not
composable (different `qualification_sha`).

Reference: unmodified i2pd 2.61.0 @ `635b013a612ff47278ef02acf8580a28e10e26c5`
throughout, built by `scripts/interop/fetch-ssu2-reference.sh --rebuild`
(`artifact_sha256=ebee24b685d48dd553257ea93104f1ab4015cd55d5e26ff06cd423a9f74a0dc6`).

## Implementation contents

Harness/checker/workflow only. No wire format, task/channel/queue, quota,
timeout, config/CLI/API/RouterInfo/version change; verified mechanically on
every run by the runner's own row
`m11-i2pd-plan265-production-source-lock`:

```text
git diff --name-only 514bf1237e86fde21e17fc98c743eb52852edd99..HEAD -- 'crates/*/src'
  -> empty
```

- `crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs` — the three frozen
  scenario families, the frozen attempt budget, the closed 16-token terminal
  vocabulary with a validating constructor, the three input-side opportunity
  predicates, the per-input semantic predicates, `ReplayOutcomeKind`, the
  per-family verdict rows, and 20 new `plan265_*` focused tests.
- `tests/integration/m11-transit/run-i2pd.sh` — manifest v5, the four Plan
  265 source-lock rows, and the per-run production-diff re-proof.
- `scripts/check-m11-per-epoch-composition.sh` — extended into the Plan 265
  closure composer: `--compose-265`, the shared `check_input_side` guard
  (`--check-input-side`), and `--self-test` running 13 positive/negative
  fixtures against the real composer.
- `scripts/check-m11-transit-boundaries.sh` — rule 29 bumped to manifest-v5
  authority, rule 30 to the frozen eight-attempt workflow matrix, new rule 38
  for the Plan 265 surface and fixtures.
- `scripts/check-m11-transit-qualification-evidence.sh` — Plan 265 section
  (driver symbols, manifest tokens, frozen numerics lock #3, composer wiring)
  plus four new guarded rows and 12 new epoch keys.
- `.github/workflows/m11-transit-external.yml` — `scenario` dispatch input and
  the frozen `attempt: [1, 2, 3, 4, 5, 6, 7, 8]` matrix with per-ordinal
  disjoint loopback ports and evidence roots.
- `plans/closure/transit-tunnels/265-retained-plan264-evidence.tsv` — the
  referenced, digest-bound retained Plan 264 five-epoch evidence index.

## Plan 265 §15 acceptance criteria — requirement-to-evidence matrix

1. **Plan 262 production source unchanged from `514bf12…`** — PASS
   (empty diff, re-proved on every one of the 24 runs and recorded as
   `production_source_diff: []` in each manifest).
2. **Plan 264 five deterministic epochs remain integrity-checked and
   traceable** — PASS. The composition gate verified all ten retained
   manifests against the committed index (schema v4, plan 264, passed
   per-epoch qualification, terminal key present in its own driver evidence,
   and `sha256` matching the index) on every `--compose-265` run.
3. **Manifest v5 + fixed budget + terminal vocabulary committed before
   external execution** — PASS. Frozen at `4682920e…`; all three
   in `check-m11-per-epoch-composition.sh` static invariants and
   `--self-test`.
4. **Exactly 24 attempts: 8 per family, fresh mesh each, one qualification
   SHA** — PASS. All 24 manifests carry
   `qualification_sha = 4682920eb0b28b0b590aaabddb8666253c26f081`,
   `qualification_tree_clean = true`, `attempt_budget = 8`, and ordinals
   exactly 1..8 per family. Each attempt ran in its own fresh datadir with its
   own loopback port set and its own evidence root; no row was merged across
   attempts.
5. **Every attempt retained and classified** — PASS. 24/24 carry a declared
   terminal from the closed vocabulary; no attempt was deleted, renamed after
   its result, or replaced; no attempt beyond ordinal 8 exists.
6. **No i2pr semantic failure in any opportunity-present attempt** — PASS.
   0 semantic failures across 24 attempts.
7. **`ibgw-data` has ≥2 large-input opportunity semantic passes** — PASS. 4
   (`ibgw-large-input-multicell-verified`).
8. **`receipt` has ≥2 full tuple-bound exact-once SAM completions** — **FAIL.
   1** (`receipt-tuple-bound-socket-verified` at attempt 6). This is the single
   stop condition.
9. **`participant-lifecycle` has ≥2 forward + B far-side + full-chain
   passes** — PASS. 5, each carrying all five lifecycle rows on that one
   attempt.
10. **Replay has no second local semantic forward and no B-side duplicate
    receipt** — PASS. All five chain-completing attempts classified
    `duplicate-dropped` with `b-endpoint-delta = 0`; no
    `duplicate-forwarded` and no `contained-no-output` with a non-zero delta
    occurred anywhere.
11. **No attempt beyond ordinal 8** — PASS.
12. **Complete local verification passes** — PASS (§Verification below).
13. **Exact-head ordinary CI passes all four jobs** — PASS (§CI below:
    run `36775035331` on `9e274a3`).
14. **Registry / roadmap / support / conformance / dossier / README agree on
    the final M11 disposition** — PASS (this commit).
15. **No product default / capability / version / public-network change** —
    PASS. Transit stays disabled by default and non-advertised;
    `cargo deny` clean.
16. **No critical/high finding remains open** — PASS. Findings below; the
    highest severity is MEDIUM and is owned by Plan 266.

Because criterion 8 fails, §15's "Only then may the closure/unblock audit
mark ADR 0026 one-family M11 experimental qualification passed and make M12
planning dependency-ready" does **not** fire. ADR 0026 stays unpassed and
M12 planning stays deferred. Public transit participation remains a separate
future decision in every case.

## Executions

Qualification SHA `4682920eb0b28b0b590aaabddb8666253c26f081`, working tree
clean, exact-pinned i2pd 2.61.0, loopback-only, public reseed/network
disabled, fresh datadirs/ports/evidence root per attempt, bounded
within-attempt setup/send rounds frozen from Plan 264. Raw reference logs
were read for diagnosis only and are not evidence; every counted fact below
is a sanitized manifest field or a typed driver evidence row.

Manifest paths, in ordinal order:

| family | attempt 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 |
|---|---|---|---|---|---|---|---|---|
| `ibgw-data` | `/tmp/m11-265-ibgw-data-a1` | `…-a2` | `…-a3` | `…-a4` | `…-a5` | `…-a6` | `…-a7` | `…-a8` |
| `receipt` | `/tmp/m11-265-receipt-a1` | `…-a2` | `…-a3` | `…-a4` | `…-a5` | `…-a6` | `…-a7` | `…-a8` |
| `participant-lifecycle` | `/tmp/m11-265-participant-lifecycle-a1` | `…-a2` | `…-a3` | `…-a4` | `…-a5` | `…-a6` | `…-a7` | `…-a8` |

### `ibgw-data` — 8 retained, 4 qualified successes, 0 semantic failures

| attempt | opportunity | semantic | external_completion | terminal |
| --- | --- | --- | --- | --- |
| 1 | absent | not-applicable | not-applicable | `ibgw-large-input-not-observed` |
| 2 | present | pass | not-applicable | `ibgw-large-input-multicell-verified` |
| 3 | present | pass | not-applicable | `ibgw-large-input-multicell-verified` |
| 4 | present | pass | not-applicable | `ibgw-large-input-multicell-verified` |
| 5 | absent | not-applicable | not-applicable | `ibgw-large-input-not-observed` |
| 6 | absent | not-applicable | not-applicable | `ibgw-large-input-not-observed` |
| 7 | present | pass | not-applicable | `ibgw-large-input-multicell-verified` |
| 8 | absent | not-applicable | not-applicable | `ibgw-large-input-not-observed` |

Family A declares no external-completion gate (Plan 265 §5: the old B-ending
receiver socket is not a closing predicate for fragmentation), so
`external_completion` is `not-applicable` on every row. The per-input trace
rows show exactly why each absence is honest, e.g. attempt 1 delivered three
large inputs (`nested=1482/1495/1488`) to receive id `0xd2ab2032` with
`live=false` and `emitted=0`, while attempt 3 delivered three to `0xb0924dce`
with `live=true` and `emitted=2` each.

### `receipt` — 8 retained, 1 qualified success, 1 reference-completion miss, 0 semantic failures

| attempt | opportunity | semantic | external_completion | terminal |
| --- | --- | --- | --- | --- |
| 1 | not-observed | not-applicable | not-applicable | `receipt-setup-stop` |
| 2 | absent | not-applicable | not-applicable | `receipt-no-live-counted-ibgw-target` |
| 3 | absent | not-applicable | not-applicable | `receipt-no-live-counted-ibgw-target` |
| 4 | present | pass | miss | `receipt-reference-completion-miss` |
| 5 | absent | not-applicable | not-applicable | `receipt-no-live-counted-ibgw-target` |
| 6 | present | pass | pass | `receipt-tuple-bound-socket-verified` |
| 7 | absent | not-applicable | not-applicable | `receipt-no-live-counted-ibgw-target` |
| 8 | absent | not-applicable | not-applicable | `receipt-no-live-counted-ibgw-target` |

The two opportunity-present attempts are the informative ones, and both show
i2pr behaving correctly:

- attempt 4 — B-originated self-targeted action on a live counted creator-A
  IBGW receive id (`0x3d308913`, `nested=1488`, `emitted=2`, `failures=0`),
  six-field tuple bound (`full-tuple-bound=true`), but the reference never
  completed its own side: `gateway-receipt=0`. Classified
  `receipt-reference-completion-miss` — not a semantic failure, and not a
  family success.
- attempt 6 — same shape (`0xcdf3efaf`, `nested=1488`, `emitted=2`,
  `failures=0`, `full-tuple-bound=true`) **and** a payload-verified exact-once
  1400-byte 0xA5 arrival at the creator receiver SAM socket
  (`gateway-receipt=1`, `gateway-receipt-once=true`). This is the genuine
  mixed-router receipt the family exists to qualify, proven once.

Attempt 1 stopped before the family's input-side boundary (the reference
creators' receiver session never reported ready), which the runner records as
the declared `receipt-setup-stop` terminal with `opportunity: not-observed` —
never as a semantic verdict.

### `participant-lifecycle` — 8 retained, 5 qualified successes, 0 semantic failures

| attempt | opportunity | semantic | external_completion | terminal |
| --- | --- | --- | --- | --- |
| 1 | present | pass | pass | `participant-forward-far-side-chain-verified` |
| 2 | present | pass | pass | `participant-forward-far-side-chain-verified` |
| 3 | present | pass | pass | `participant-forward-far-side-chain-verified` |
| 4 | absent | not-applicable | not-applicable | `participant-input-not-observed` |
| 5 | absent | not-applicable | not-applicable | `participant-input-not-observed` |
| 6 | present | pass | pass | `participant-forward-far-side-chain-verified` |
| 7 | present | pass | pass | `participant-forward-far-side-chain-verified` |
| 8 | absent | not-applicable | not-applicable | `participant-input-not-observed` |

All five successes carried all five lifecycle rows on that one attempt, on
that one retained genuine cell, and the composer re-checks both the manifest
row set and the driver's own `lifecycle-rows` row:

| row | evidence |
|---|---|
| `replay` | `outcome-kind=duplicate-dropped`, `b-endpoint-delta=0`, `no-second-delivery=<digest>` |
| `expiry` | `feed-observation=DataDropped` at `created_ms + 601 s`, `drops-live-data=true`, `swept-count=0`, `resource-baseline=true` |
| `session-close` | `a-before`/`b-before`/`a-removed`/`b-retained` all true, `peer-baseline=true`, `final-peer-baseline=1` |
| `cancel` | `active-before 13 → active-after 0`, `pending-after 0`, `peer-index-after 0`, `queued-work-after 0`, `drains=true`, `new-ingress-refused=true` |
| `restart` | `old-owner-drained` all-zero, `new-owner-zero` with no old secret-owning state, `sessions-reestablished=true`, `fresh-build-accepted=true`, `registration-delta=1`, `final-baseline` all-zero |

### Family summary

| family | retained | qualified successes | no-opportunity terminals | reference-completion misses | setup stops | semantic failures | family gate |
|---|---|---|---|---|---|---|---|
| `ibgw-data` | 8 | 4 | 4 | 0 | 0 | 0 | MET (≥2) |
| `receipt` | 8 | 1 | 5 | 1 | 1 | 0 | **NOT MET (1 < 2)** |
| `participant-lifecycle` | 8 | 5 | 3 | 0 | 0 | 0 | MET (≥2) |

All 24 attempts share one `qualification_sha`; the composition gate rejects a
mixed set, a changed budget, a duplicate or out-of-range ordinal, a missing
ordinal, an unclassified terminal, any `semantic: fail`, an opportunity
inferred from the downstream result, a Plan 264 manifest promoted into a Plan
265 family count, borrowed lifecycle rows, and a missing retained manifest.
Each of those rejections is proven by a `--self-test` fixture against the real
composer, and the input-side opportunity rule is additionally proven in both
directions by a fixture that mutates a predicate into reading an output-side
field and requires rejection.

## Verification (local, `4682920e…`)

```text
cargo fmt --all --check                                        PASS
cargo check --locked --workspace --all-targets                 PASS
cargo test --locked --workspace --all-targets -- --test-threads=1
                                                              3149 passed, 27 ignored
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
                                                              no issues
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
                                                              PASS
cargo test --locked --workspace --doc                         PASS
cargo deny check advisories bans sources                       advisories ok, bans ok, sources ok
bash scripts/check-dependency-direction.sh                     PASS
bash scripts/check-runtime-boundaries.sh                       PASS
bash scripts/check-service-tunnel-boundaries.sh                PASS
bash scripts/check-m11-transit-boundaries.sh                   PASS
bash scripts/check-m11-transit-qualification-evidence.sh       PASS
bash scripts/check-m11-per-epoch-composition.sh                PASS
bash scripts/check-m11-per-epoch-composition.sh --self-test    13 fixtures passed
bash scripts/check-m6-mixed-router-acceptance-evidence.sh     PASS
bash scripts/check-exploratory-tunnel-evidence.sh              PASS
git diff --check                                               PASS
bash scripts/check-m11-per-epoch-composition.sh --compose-265 <10 retained> -- <24 attempts>
                                                              1 violation: family receipt 1 < 2
```

Every new Plan 265 focused test was run by exact name and passed:

```text
plan265_ibgw_opportunity_is_input_side_only
plan265_ibgw_large_input_multicell_passes
plan265_ibgw_large_input_single_cell_is_semantic_failure
plan265_ibgw_opportunity_absent_is_typed
plan265_receipt_self_action_opportunity_classification
plan265_receipt_semantic_and_socket_completion_pass
plan265_receipt_reference_completion_miss_is_not_a_semantic_failure
plan265_receipt_opportunity_present_with_failed_local_ingress_fails
plan265_participant_input_side_opportunity_classification
plan265_participant_input_present_without_local_forward_fails
plan265_participant_local_forward_with_b_completion_miss
plan265_participant_lifecycle_chain_passes
plan265_replay_duplicate_dropped_passes
plan265_replay_contained_no_output_requires_zero_forward_and_zero_b_delta
plan265_replay_duplicate_forwarded_fails_regardless_of_b_receipt
plan265_manifest_v5_requires_exactly_eight_ordinals
plan265_composition_rejects_budget_sha_duplicate_missing_and_unclassified
plan265_lifecycle_rows_cannot_be_borrowed_across_attempts
plan265_production_source_diff_guard
```

## Production-equivalence proof (Plan 265 §8)

```text
$ git diff --name-only 514bf1237e86fde21e17fc98c743eb52852edd99..4682920eb0b28b0b590aaabddb8666253c26f081 -- 'crates/*/src'
(empty)
```

Authority is explicitly separated, as §8 requires:

- **production implementation authority** — Plan 262 source tree
  (`514bf1237e86fde21e17fc98c743eb52852edd99`), byte-identical throughout;
- **deterministic external evidence** — Plan 264, five epochs 2/2 on
  `6ab9dc2d`, referenced and digest-verified through
  `plans/closure/transit-tunnels/265-retained-plan264-evidence.tsv`;
- **fixed-budget opportunity-qualified emission/lifecycle evidence** — this
  record, 24 attempts on `4682920e…`.

No claim is made that all milestone rows were executed on one repository SHA;
the three authorities above were executed on three different SHAs and this
record says so.

## CI

Ordinary push CI was run on the **exact closure head**
`9e274a34a37d42b13a854d8c11d850116c9dfe6e` (the commit that registers
this closure and Plan 266). All four required jobs pass:

| Run | Head | Required jobs | Result |
| --- | --- | --- | --- |
| `36775035331` | `9e274a3` | Quality (ubuntu-latest), Quality (macos-latest), MSRV (Ubuntu), Dependency policy | **success** on all four |

`https://github.com/dbowm91/i2pr/actions/runs/36775035331`

Concretely: the Quality jobs cover `cargo fmt --all --check`,
`cargo check --locked --workspace [--all-targets]`, the full test suite
run serially with one libtest worker (macOS builds every test executable
once, then runs each with `--test-threads=1`), `cargo test --doc`,
`clippy -D warnings`, and `cargo doc` with `RUSTDOCFLAGS=-D warnings`;
the Linux-gated steps additionally run every boundary/evidence checker
(including `check-m11-transit-boundaries.sh`, which locks the Plan 265
harness/runner/checker/workflow sources) and the constrained-host lane
contract tests. The MSRV job covers `cargo check` on toolchain 1.88, and
the Dependency policy job covers `cargo deny check advisories bans
sources`.

The two Plan 265 checkers that require retained evidence roots —
`check-m11-transit-qualification-evidence.sh` and
`check-m11-per-epoch-composition.sh` — are **not** in ordinary CI.
`check-m11-transit-qualification-evidence.sh` runs in the
`m11-transit-external.yml` workflow; the composition checker is
dispatcher-driven (it composes a supplied evidence root) and is instead
exercised in ordinary CI by the fixture and self-test rows locked inside
`check-m11-transit-boundaries.sh`. Both were additionally run locally
against the complete retained set (§Verification).

No external/interoperability lane was run for this closure: Plan 265's
own 24-attempt evidence is the previously executed and retained set
(§Executions), and no reference router or public network is required to
re-establish it. Re-running the external lane is a Plan 266 concern.

## Security / resource / concurrency / migration review

- No user migration. No config schema, CLI, SAM/I2CP public API,
  RouterInfo capability, `router.version`, public-network behavior, or
  default transit-disabled construction change.
- Security interpretation is unchanged and re-proven by this lane: network
  transport authentication at the transport owner; IBGW authorization by live
  receive id + role + expiry/resource state, never by build creator;
  Participant/OBEP hop provenance peer-locked; local self-loop only after the
  decoded target equals i2pr's own router hash; replay fails on a second
  local semantic forward even if the reference drops the copy; no synthetic
  peer identity. All five chain-completing attempts recorded
  `duplicate-dropped` with a zero B-side delta.
- Resource/concurrency: no new task, channel or queue; no unbounded
  allocation; the ledger ceiling, the retained-cell ceiling and every
  admission dimension are unchanged; cancellation, drain and expiry baselines
  are proven per attempt (`cancel/drains`, `expiry/resource-baseline`,
  `restart/final-baseline`).
- No new dependency. `cargo deny check advisories bans sources` clean.
- No reference patching, no vendoring, no public-network fallback, no
  timeout/quota/message-size/admission-ceiling change: the frozen Plan 262
  numerics are locked by three independent copies of the same list.

## Findings by severity

| severity | finding | owner | status |
|---|---|---|---|
| MEDIUM | `receipt` family reference-completion opportunity generation is below the frozen bar: 5/8 attempts never produced a B-originated self-targeted action on a live counted creator-A IBGW receive id, 1 setup stop, and only 1 of the 2 opportunity-present attempts completed at the receiver socket. | Plan 266 | open; qualification opportunity generation only |
| LOW | The `registration_live_at_input` conjunct is proven from ledger observations (a same-registration delivery at or after the input) rather than from the registration's own `expires_at_seconds`, which the production `LiveGatewayOutcome` does not carry. A large input that is the *first* ingress on a genuinely live registration and is dropped would therefore classify as an absence. The delivery path has no such gap: a delivered large input is unconditionally an opportunity. | Plan 266 | accepted, documented; no production change proposed |
| LOW | The restarted-owner row proves no old secret-owning state rather than an all-zero snapshot, because `fresh_owner` installs the two authenticated peer mappings by contract. The peer contract is re-proven by the adjacent `sessions-reestablished` row and the drained old owner is still checked all-zero. | closed in Plan 265 | documented, bound by a focused test |
| LOW | A Plan 265 scenario run's inherited `results` array is family-scoped, so most rows from the Plan 257 full-matrix set read `failed` in a family manifest. The manifest now carries `lane_row_scope` so the artifact is self-describing; the Plan 265 authority is the classification fields. | closed in Plan 265 | documented |

No CRITICAL or HIGH finding is open. No production defect was found in
Plan 262 routing: every opportunity-present input that i2pr received was
handled correctly, including multicell fragmentation of ~1.5 KB nested inputs
(observed `nested` 1480–1498, `emitted=2`, `failures=0`).

No empirical success fraction in this record may be read as a reliability
estimate, and nothing here is an anonymity or privacy claim.

## Roadmap disposition

- `ibgw-data` family: **closed** under Plan 265 on `4682920e…` (4/8, 0
  semantic failures). The Plan 258 H3 production fragmentation defect stays
  corrected and is now externally re-proven under a fixed-budget contract.
- `participant-lifecycle` family: **closed** under Plan 265 on `4682920e…`
  (5/8 full-chain successes, replay `duplicate-dropped` with zero B delta).
  The Plan 256/257 lifecycle rows are now first proven on a controlled mesh.
- `receipt` family: **not closed**. 1/8 against a required 2. Plan 266 owns
  receipt-family opportunity generation only.
- ADR 0026 one-family M11 experimental qualification: **not marked passed**.
- M12 planning: **stays deferred**. Milestone 12 remains unregistered.
- Public transit participation, RouterInfo advertisement, and any
  public-network behavior: unchanged and out of scope.

## Unblock audit

Audited `plans/registry.md` blocked work plus the transit-tunnels roadmap
dependency graph.

- Plan 266 (registered in this commit) lists Plan 265 as its sole hard
  dependency. Plan 265 now has an authoritative closure record and an
  integrity-checkable evidence set, so that dependency is closed. Plan 266 has
  no other hard dependency and its interface dependency — the Plan 265
  manifest-v5 shape, the closed terminal vocabulary and the composition gate —
  is a stable written contract in the tree. Plan 266 is therefore
  **dependency-ready** and moves to `ready` in this commit.
- Every other registry row that names an M11 plan as a dependency is either
  `retained` with a named corrective that is now closed (Plans 258→259→260,
  261, 262, 263, 264) or names Plan 265 only as M12's deferral condition. M12
  has no registered plan, so nothing becomes dependency-ready from it.
- No plan is unblocked that depends on the unmet `receipt` criterion.
  Specifically: nothing downstream of "M11 experimental qualification passed"
  moves, because ADR 0026 is not marked passed.

## Correction, 2026-10-05 — the zero-production-diff guard was fail open (dated; the text above is preserved)

Re-audit while registering Plan 340 found that this plan's headline
mechanism — the `PLAN265_PRODUCTION_BASELINE` guard that re-proves the
Plan 262 production-source equivalence on every run — **could not fail**.

The guard diffed with the pathspec `'crates/*/src'`. Git's default
pathspec matching resolves such a pattern against the whole path, and the
`src` component never matches a file *inside* the directory, so the
pattern selected nothing at all:

```text
$ git ls-files 'crates/*/src'        | wc -l      # 0
$ git ls-files ':(glob)crates/*/src/**' | wc -l    # 325
$ git diff --name-only 514bf12..HEAD -- 'crates/*/src'          | wc -l   # 0   (vacuous)
$ git diff --name-only 514bf12..HEAD -- ':(glob)crates/*/src/**' | wc -l  # 151
```

Both call sites were affected: the shell guard in
`tests/integration/m11-transit/run-i2pd.sh` and the manifest /
composition-gate `prod_diff` in the same runner's driver heredoc.

**What was and was not proven, stated precisely.**

- The *substantive* claim survives. The retained Plan 264 evidence was
  produced at qualification SHA `6ab9dc2dd80526ce38e62014635ec3e3ad5521f5`,
  and re-running the guard with a **working** pathspec over
  `514bf12..6ab9dc2d` returns **0** changed crate production sources. The
  evidence really was collected on the byte-identical Plan 262 production
  tree, and this is now independently re-proved rather than assumed.
- The *verification* claim does not survive. "Re-proved mechanically on
  every one of the 8 runs" describes a check that was never exercised; it
  returned the right answer for the wrong reason. Plans 266, 267, and 268
  repeat that phrasing and are corrected by cross-reference.
- The guard was fail open for the whole drift since that SHA. The first
  production-source change after the retained evidence is `740e8ff`
  ("netdb: add bounded provenance eligibility model"); 86 commits touching
  `crates/*/src` have landed since, and the guard would have reported
  "Plan 262 production authority retained" through all of them.

**Corrective applied.** Both call sites now use
`:(glob)crates/*/src/**`. With the corrected pathspec the lane fails
closed on the current tree (151 changed sources), which is the intended
behaviour. `scripts/check-m11-transit-boundaries.sh` rule 39 now carries
teeth for the repair itself: a positive control requiring the corrected
pathspec to select files in this tree, a negative control requiring the
bare form to select none, and a non-comment scan of the runner requiring
the corrected spec at both call sites and the vacuous spec nowhere in
code. Reverting either call site makes the boundary checker fail; both
reverts were executed and both failed closed before the sources were
restored.

**Not changed by this correction.** The Plan 265 status token, the frozen
attempt budget, the closed terminal vocabulary, the opportunity ladder, and
every retained evidence row are untouched. No milestone, readiness, or
support claim moves. See also the 2026-10-05 addendum in
[`268-status.md`](268-status.md) for the consequence for the M11
authority transition.

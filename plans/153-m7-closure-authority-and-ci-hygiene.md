# Plan 153 — post-Milestone 7 closure authority and CI hygiene

Status: **next executable corrective pass**.

Registered: **2026-09-03**.

Depends on:

- Plan 151 `passed-m7-sam31-final-acceptance-evidence-correction`;
- Plan 152 `passed-m6-session-streaming-robustness-corrective` implementation/evidence;
- current-head routine CI remaining green.

Blocks:

- all Milestone 8 implementation work. Plan 154 may be used for planning/orientation, but no Plan 155+ code work begins until Plan 153 closes.

## 1. Goal

Remove the small authority/documentation/CI inconsistencies left after the successful Plan 151/152 closure without reopening SAM, Streaming, ECIES, NTCP2, or any protocol implementation.

This is deliberately a **documentation and CI-only pass**. It exists so a smaller execution model can begin Milestone 8 from one coherent authority chain instead of having to resolve contradictory prose.

Expected closing classification:

```text
plan_151 = passed-m7-sam31-final-acceptance-evidence-correction
plan_152 = passed-m6-session-streaming-robustness-corrective
plan_153 = passed-post-m7-authority-and-ci-hygiene

milestone7_sam_localhost = passed-via-plan151
milestone7_final_acceptance = closed
milestone6_interoperable = not-yet-claimed
next_executable_plan = 155
next_product_layer = milestone8-ssu2-v2
```

## 2. Concrete inconsistencies to correct

### 2.1 Missing Plan 152 closure record

`plans/README.md` declares `NNN-status.md` records authoritative, but Plan 152 is currently closed only in its narrative and in `specs/support.toml`.

Create:

```text
plans/152-status.md
```

The status must preserve the actual Plan 152 result:

```text
passed-m6-session-streaming-robustness-corrective
```

It must record:

- why Plan 151 §17 triggered the narrow corrective;
- the three defects actually demonstrated by tests;
- the three narrow local-policy fixes;
- that no Streaming or ECIES wire semantics changed;
- focused test commands/results;
- Plan 151 closing-head routine CI and external SAM evidence as the containing acceptance authority;
- current-head routine CI result if available at execution time;
- `milestone6_interoperable = not-yet-claimed`.

Do not create a new M6 milestone closure claim. Plan 134 remains the local Milestone 6 product authority; Plan 152 is a later robustness correction retained underneath Plan 151.

### 2.2 Root README stale Plan 152 wording

The root `README.md` currently says the Plan 152 full workspace floor is pending. That is stale: Plan 151 closure recorded the floor green, and current-head CI is green.

Replace that wording with an evidence-backed statement. Do not broaden it into mixed-router interoperability.

### 2.3 `plans/README.md` contradictory Plan 151 authority

Correct all stale statements that still describe Plan 151 as active / the next executable plan.

At minimum fix:

- the narrative bullet saying Plan 151 “is the only next executable plan”;
- the Milestone 7 hierarchy row describing Plan 151 as active;
- the “what's not yet accepted” section that still says Plan 151 must close final SAM acceptance.

The file must instead state:

```text
Plan 151 = passed final M7 localhost acceptance
Plan 152 = passed narrow M6 robustness corrective
Plan 153 = current next executable hygiene pass
Milestone 8 implementation = blocked until Plan 153 passes
```

### 2.4 Machine-readable support ledger

Update `specs/support.toml` to add:

```toml
plan_152_closure = "plans/152-status.md"
```

Do not change any protocol `advertised = false` value merely because M7 closed.

If the generated/mirrored protocol-support documentation has a checked-in synchronization mechanism, run it and commit the generated change. Do not hand-edit a generated file if the repository already provides a generator.

### 2.5 Executor-facing guidance

Audit these surfaces for stale Plan 151/152 authority wording and make only the minimum consistency edits:

```text
AGENTS.md
.opencode/skills/i2pr-local-dev/SKILL.md
tests/integration/sam/README.md
```

All should agree that:

- Plan 151 passed;
- Plan 152 passed;
- SAM remains experimental, disabled by default, loopback-only, non-advertised;
- router-to-router interoperability remains unclaimed;
- Plan 153 is cleanup only;
- Plan 155 is the first Milestone 8 implementation pass after Plan 153 closure.

## 3. Permanently enforce the Plan 151 evidence-integrity invariant

Plan 151 added:

```text
scripts/check-sam-acceptance-evidence.sh
```

It correctly rejects synthetic `passed` bookkeeping, but it is not yet a routine CI invariant.

### 3.1 Routine CI

Add this exact check to the Linux quality job in `.github/workflows/ci.yml`:

```text
bash scripts/check-sam-acceptance-evidence.sh
```

Place it with the other static boundary/evidence checks. It does not need to run on macOS.

Do not weaken the checker to make CI pass.

### 3.2 Manual SAM external workflow

Add the same checker to `.github/workflows/sam-external.yml` **before** the final external matrix is run.

The workflow should fail before expensive external-client execution if evidence bookkeeping has regressed.

Do not duplicate the checker logic inside YAML.

### 3.3 Documentation

Update `tests/integration/sam/README.md` or the nearest existing lane documentation to state that the evidence checker is CI-enforced.

## 4. Hard scope boundary

Plan 153 must not change production or protocol code.

Expected production-code diff:

```text
crates/ = no changes
```

Also do not modify:

```text
Cargo.lock
workspace dependency versions
SSU2/NTCP2/Streaming/SAM protocol behavior
runtime socket behavior
```

Allowed implementation files are limited to planning/status/docs, support ledger/generated support docs, and CI workflow files.

If a CI failure exposes a real code defect, stop and record it separately. Do not smuggle a runtime fix into this pass.

## 5. Required execution sequence

1. Record the starting commit SHA.
2. Create `plans/152-status.md` from actual executed evidence, not reconstructed assumptions.
3. Correct stale Plan 151/152 authority prose.
4. Add `plan_152_closure` to `specs/support.toml` and regenerate mirrors if applicable.
5. Wire the SAM evidence-integrity checker into routine Linux CI and the manual SAM external workflow.
6. Run local static/documentation checks.
7. Verify `git diff <start> -- crates Cargo.lock` is empty.
8. Commit the complete Plan 153 pass.
9. Run routine CI on the exact closing commit.
10. Manually dispatch the SAM external workflow on the exact closing commit and require success. The purpose is to prove the newly enforced checker and existing external lane compose correctly; do not change external-client pins.
11. Only after both hosted lanes pass, mark `plans/153-status.md` passed and advance executable authority to Plan 155.

## 6. Validation commands

At minimum:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
bash scripts/check-sam-acceptance-evidence.sh
```

If a support-matrix generation/check script exists, run it as well.

Plan 153 does not require rerunning the several-minute Plan 151 slow-peer suite locally merely to edit docs/CI; the exact-head manual SAM external workflow will execute the lane after the checker is wired in.

## 7. Acceptance criteria

Plan 153 closes only when all are true:

1. `plans/152-status.md` exists and is the authoritative Plan 152 closure record.
2. `specs/support.toml` points to `plans/152-status.md`.
3. Root `README.md` no longer says the Plan 152 workspace floor is pending.
4. `plans/README.md` contains no statement that Plan 151 is active/next executable.
5. `plans/README.md` no longer lists final M7 SAM acceptance as unclosed.
6. `AGENTS.md`, the local-dev skill, and SAM integration README agree on the closure chain.
7. Routine Linux CI invokes `scripts/check-sam-acceptance-evidence.sh`.
8. The manual SAM external workflow invokes the same checker before the external matrix.
9. No checker logic is duplicated or weakened.
10. No file under `crates/` changes.
11. `Cargo.lock` does not change.
12. Local static/documentation validation passes.
13. Routine CI passes on the exact Plan 153 closing commit.
14. The manual SAM external workflow passes on the exact Plan 153 closing commit.
15. SAM remains experimental, loopback-only, disabled by default, and non-advertised.
16. No router-to-router interoperability claim is introduced.
17. `plans/153-status.md` records exact closing SHA and hosted run IDs/results.
18. Only after 1–17 pass does authority advance to `next_executable_plan = 155`.

## 8. Stop conditions

Stop rather than broadening this pass if:

- any production-code change appears necessary;
- the evidence checker reports a genuine harness regression;
- the SAM external lane fails for a substantive protocol/runtime reason;
- machine-readable support generation exposes a non-documentation discrepancy.

Create a narrowly scoped follow-up only for the concrete issue. Plan 153 is not a generic cleanup bucket.

## 9. Handoff

Execute this plan before any Plan 155+ implementation.

Plan 154–161 are planning documents and may be read immediately, but Milestone 8 code begins only after Plan 153 has an explicit passing status record.
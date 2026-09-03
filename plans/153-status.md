# Plan 153 status — post-Milestone 7 closure authority and CI hygiene

Status: **`passed-post-m7-authority-and-ci-hygiene`**.

Registered: **2026-09-03**. Closed: **2026-09-03**.

Plan of record:
[`plans/153-m7-closure-authority-and-ci-hygiene.md`](153-m7-closure-authority-and-ci-hygiene.md).

## Current authority

```text
plan_151 = passed-m7-sam31-final-acceptance-evidence-correction
plan_152 = passed-m6-session-streaming-robustness-corrective
plan_153 = passed-post-m7-authority-and-ci-hygiene

milestone7_sam_localhost = passed-via-plan151
milestone7_final_acceptance = closed
milestone6_interoperable = not-yet-claimed

next_executable_plan = 155
milestone8_planning_authority = plan154
milestone8_implementation = unblocked-start-at-155
next_product_layer = milestone8-ssu2-v2
```

## What this pass did

Documentation and CI hygiene only — no SAM, Streaming, ECIES, NTCP2,
or any protocol implementation was reopened:

1. Created the missing authoritative Plan 152 closure record
   [`plans/152-status.md`](152-status.md)
   (`passed-m6-session-streaming-robustness-corrective`), preserving
   the Plan 151 §17 trigger, the three demonstrated defects, the three
   narrow local-policy fixes, the no-wire-change invariant, focused
   test commands/results, and the Plan 151 closing-head acceptance
   authority.
2. Replaced the stale root `README.md` Plan 152 "workspace floor is
   still pending" wording with the evidence-backed closing-head
   statement; classification now carries Plan 151/152 passed,
   Milestone 7 final acceptance closed, `milestone6_interoperable =
   not-yet-claimed`, and the registered Plan 154 roadmap (blocked
   until Plan 153 passed at execution time).
3. Normalized `plans/README.md`: Plan 152 hierarchy row now points to
   the authoritative `152-status.md` as passed; the file states Plan
   151 passed, Plan 152 passed, Plan 153 as the executed hygiene pass,
   and Milestone 8 implementation blocked until Plan 153 passed.
4. Added `plan_152_closure = "plans/152-status.md"` to
   `specs/support.toml`. No protocol `advertised = false` value was
   changed. No checked-in support-matrix generator exists in the
   repository (no generator script under `scripts/`, `tools/`, or
   `crates/` produces `docs/protocol-support.md` from
   `specs/support.toml`), so there was no generator to run; the
   hand-maintained `docs/protocol-support.md` mirror was deliberately
   left untouched to keep this pass minimal.
5. Audited executor-facing guidance (`AGENTS.md`,
   `.opencode/skills/i2pr-local-dev/SKILL.md`,
   `tests/integration/sam/README.md`) plus the directly affected
   architecture surfaces (`docs/architecture/overview.md`,
   `docs/architecture/i2pr-daemon.md`, `docs/architecture/i2pr-api.md`,
   `docs/architecture/i2pr-client.md`,
   `.opencode/skills/i2pr-architecture/SKILL.md`) for stale
   Plan 151/152 authority wording with minimum consistency edits. All
   agree: Plan 151 passed, Plan 152 passed, SAM remains experimental,
   disabled by default, loopback-only, non-advertised,
   router-to-router interoperability remains unclaimed, Plan 153 was
   cleanup only, and Plan 155 is the first Milestone 8 implementation
   pass after Plan 153 closure.
6. Wired `bash scripts/check-sam-acceptance-evidence.sh` into the
   routine Linux quality job in `.github/workflows/ci.yml` (with the
   other static boundary/evidence checks; Linux-only) and into
   `.github/workflows/sam-external.yml` before the external matrix.
   No checker logic was duplicated or weakened. The SAM integration
   README now states the checker is CI-enforced.

## Hard scope boundary (kept)

```text
crates/ = no changes
Cargo.lock = no changes
```

Verified via `git diff 430de3e213861a91ae13ce31ffcf25556855f580 -- crates Cargo.lock`
(empty). Allowed files only: planning/status/docs, support ledger,
and CI workflow files. No production-code change became necessary; no
evidence-checker regression, substantive external-lane failure, or
non-documentation support-generation discrepancy appeared, so no
follow-up plan was created.

## Validation record

Starting SHA: `430de3e213861a91ae13ce31ffcf25556855f580`.
Closing (complete-pass) commit: `6b09cd36d713501cd2ea4b0bde2b0331698ffd12`
(`plans: execute Plan 153 authority and CI hygiene pass`).

Local validation on the closing tree (all green):

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked -p i2pr-api --all-targets (120 passed)
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-constrained-host-lane-boundary.sh
bash scripts/check-sam-acceptance-evidence.sh (22 rows command-derived, no literal pass records)
git diff --check
python3 YAML parse of .github/workflows/ci.yml and sam-external.yml
```

Plan 153 did not rerun the several-minute Plan 151 slow-peer suite
locally merely to edit docs/CI, per the plan §6.

Hosted lanes on the exact closing commit `6b09cd3`:

- Routine CI run `33810722262` (push, `main`,
  `6b09cd36d713501cd2ea4b0bde2b0331698ffd12`): conclusion `success`.
  The Linux quality job executed the newly wired `Check SAM
  acceptance evidence integrity` step alongside the other boundary
  checks; Quality ubuntu, Quality macos, MSRV, and dependency-policy
  all green.
- Manual SAM external workflow run `33811909642`
  (`workflow_dispatch` of `.github/workflows/sam-external.yml` at
  `main`, `6b09cd36d713501cd2ea4b0bde2b0331698ffd12`): conclusion
  `success` (~16 min). Step order proves the newly enforced checker
  composes with the existing lane: `Check SAM acceptance evidence
  integrity` green immediately before `Run localhost SAM
  external-client matrix`, then sanitized evidence upload. Only
  annotation is the pre-existing Node 20 deprecation notice on
  `actions/upload-artifact@v4`, unrelated to this pass.
  External-client pins unchanged.

## Acceptance criteria (all true)

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
13. Routine CI passes on the exact Plan 153 closing commit (`33810722262`, success).
14. The manual SAM external workflow passes on the exact Plan 153 closing commit (`33811909642`, success).
15. SAM remains experimental, loopback-only, disabled by default, and non-advertised.
16. No router-to-router interoperability claim is introduced.
17. This record carries the exact closing SHA and hosted run IDs/results.
18. Authority advances to `next_executable_plan = 155` only now, with 1–17 satisfied.

## Handoff

Plan 153 is closed. Milestone 8 implementation is unblocked: execute
Plans **155 → 156 → 157 → 158 → 159 → 160 → 161** in order under the
Plan 154 roadmap authority. Plan 154–161 may be read immediately as
planning context; Milestone 8 code begins at Plan 155.

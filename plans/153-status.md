# Plan 153 status — post-Milestone 7 closure authority and CI hygiene

Status: **`active-post-m7-authority-and-ci-hygiene`**.

Registered: **2026-09-03**.

Plan of record:
[`plans/153-m7-closure-authority-and-ci-hygiene.md`](153-m7-closure-authority-and-ci-hygiene.md).

## Current authority

```text
plan_151 = passed-m7-sam31-final-acceptance-evidence-correction
plan_152 = passed-m6-session-streaming-robustness-corrective
plan_153 = active-post-m7-authority-and-ci-hygiene

milestone7_sam_localhost = passed-via-plan151
milestone7_final_acceptance = closed
milestone6_interoperable = not-yet-claimed

next_executable_plan = 153
milestone8_planning_authority = plan154
milestone8_implementation = blocked-by-plan153
```

## Why this plan is active

The remaining work is not a protocol defect. The audited repository has four closure-hygiene inconsistencies:

1. Plan 152 lacks the repository-standard `152-status.md` closure record.
2. Root/planning prose contains a few stale Plan 151/152 statements.
3. `specs/support.toml` lacks a `plan_152_closure` path.
4. Plan 151's SAM evidence-integrity checker is proven but not yet enforced by routine Linux CI and the manual SAM external workflow.

## Handoff

Execute Plan 153 only. Do not begin Plan 155 or other Milestone 8 implementation until this record is replaced with an explicit passing closure containing the exact closing commit and hosted CI/SAM-external run results.

Plan 154–161 may be read as planning context while Plan 153 executes.
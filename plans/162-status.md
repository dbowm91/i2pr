# Plan 162 status — Milestone 8 SSU2 external-test lane isolation and CI restoration

Status: **`active-m8-ssu2-external-test-lane-isolation-and-ci-restoration`**.

Registered: **2026-09-04**.

Plan of record:
[`plans/162-m8-ssu2-external-test-lane-isolation-and-ci-restoration.md`](162-m8-ssu2-external-test-lane-isolation-and-ci-restoration.md).

## Current authority

```text
plan_153 = passed-post-m7-authority-and-ci-hygiene
plan_154 = registered-m8-ssu2-v2-roadmap
plan_155 = passed-m8-ssu2-v2-protocol-foundation-and-addresses
plan_156 = passed-m8-ssu2-v2-handshake-token-and-routerinfo
plan_157 = passed-m8-ssu2-v2-data-phase-reliability-and-fragmentation
plan_158 = passed-m8-ssu2-udp-runtime-and-local-session-product
plan_159 = passed-m8-ssu2-path-validation-publication-and-transport-selection
plan_160 = passed-m8-ssu2-peer-test-and-relay-reachability
plan_161 = in-progress-direction-a-proven-blocked-by-plan162
plan_162 = active-m8-ssu2-external-test-lane-isolation-and-ci-restoration

milestone8_protocol = ssu2-v2-classical
milestone8_ssu2_direction_a = passed-via-plan161
milestone8_final_acceptance = not-yet-closed
milestone6_interoperable = not-yet-claimed
ssu2_pq_v3_v4 = deferred-compatibility-watch
ssu1 = not-implemented

next_executable_plan = 162
resume_after_plan162 = 161
next_product_layer = milestone8-ssu2-v2
```

## Trigger

Current head when this corrective was registered:

```text
4a38e2958c7d668f7c6abeb4a6aac0c13547bb0c
```

Plan 161 direction A is retained as genuinely proven against exact-pinned i2pd 2.61.0 (`635b013a612ff47278ef02acf8580a28e10e26c5`) over real loopback UDP.

The corrective exists because routine CI run `33915994884` on that exact head failed both Ubuntu and macOS quality jobs when ordinary workspace execution automatically ran `crates/i2pr-runtime/tests/ssu2_independent.rs` without its required external environment:

```text
ssu2_independent_ipv4_interop ... FAILED
missing required env I2PD_ROUTER_INFO
```

Dependency policy and MSRV passed. The observed failure is therefore classified as a test-lane selection/integration defect, not a new SSU2 protocol failure.

## Required correction

- Keep the external driver compiled in normal all-target checks.
- Mark the external test explicitly ignored for ordinary libtest execution.
- Require explicit `--ignored --exact` selection in the dedicated external lane.
- Keep missing external environment fail-closed when the test is explicitly selected.
- Do not add routine-CI filename filtering, `|| true`, `continue-on-error`, fake peer values, or broad test exclusions.
- Re-run Plan 161 direction A against the exact-pinned i2pd after gating.
- Require routine Ubuntu and macOS CI green on the exact Plan 162 closing commit.

## Closure fields

Populate these only from executed evidence:

```text
closing_sha = pending
routine_ci_run = pending
routine_ci_ubuntu = pending
routine_ci_macos = pending
msrv = pending
dependency_policy = pending
ordinary_external_test_invocation = pending
explicit_missing_env_invocation = pending
explicit_i2pd_direction_a_invocation = pending
i2pd_pin = 635b013a612ff47278ef02acf8580a28e10e26c5
```

Do not mark this status passed until every acceptance criterion in Plan 162 §12 is satisfied.

## Handoff

Execute Plan **162** before continuing Plan 161. On successful closure, restore:

```text
plan_162 = passed-m8-ssu2-external-test-lane-isolation-and-ci-restoration
plan_161 = in-progress-direction-a-proven
next_executable_plan = 161
milestone8_final_acceptance = not-yet-closed
```

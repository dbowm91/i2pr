# Plan 154 status — Milestone 8 SSU2 v2 transport and reachability roadmap

Status: **`registered-m8-ssu2-v2-roadmap`**.

Registered: **2026-09-03**. Authority updated: **2026-09-04**.

Plan of record:
[`plans/154-m8-ssu2-transport-and-reachability-roadmap.md`](154-m8-ssu2-transport-and-reachability-roadmap.md).

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

milestone8_protocol = SSU2-v2-classical
ssu2_pq_v3_v4 = deferred-compatibility-watch
ssu1 = not-implemented
milestone8_ssu2_direction_a = passed-via-plan161
milestone8_final_acceptance = not-yet-closed
milestone6_interoperable = not-yet-claimed

next_executable_plan = 162
resume_after_plan162 = 161
```

## Plan 153 closure note

`plans/153-status.md` is `passed-post-m7-authority-and-ci-hygiene`; the original
registration-time Plan 153 block is satisfied. Plans 155–160 subsequently
executed and passed in roadmap order.

## Plan 161 / 162 execution note

Plan 161 has begun the final independent interop gate. Direction A is retained
as proven against exact-pinned i2pd 2.61.0
(`635b013a612ff47278ef02acf8580a28e10e26c5`) over real loopback UDP with
mutual authentication, small + fragmented I2NP delivery to i2pd, DeliveryStatus
return traffic, and graceful cleanup.

The direction-A commit also introduced an environment-dependent integration
test into ordinary workspace execution. Routine CI run `33915994884` failed
both Ubuntu and macOS quality jobs because the external i2pd environment is not
present in routine CI. Plan 162 is therefore inserted as a narrow corrective:
keep the test compiled, ignore it under ordinary workspace execution, require
explicit fail-closed `--ignored` execution in the external lane, re-prove
direction A, and restore exact-head routine CI.

Plan 162 does **not** replace or renumber Plan 161 and does not alter the
Milestone 8 architecture. After Plan 162 passes, return directly to Plan 161
for direction B and final closure.

## Architecture decisions locked by this roadmap

- `i2pr-transport-ssu2` remains runtime-neutral protocol/state machinery;
- production UDP ownership stays in `i2pr-runtime`;
- reuse `i2pr-transport` manager/resource/delivery/reachability contracts;
- no per-packet task/timer architecture;
- real localhost UDP is the local/interop substrate;
- no root, namespaces, containers, VM, systemd, or public I2P dependency;
- mandatory final independent implementation is exact-pinned i2pd 2.61.0 (`635b013a612ff47278ef02acf8580a28e10e26c5`) in both directions;
- Java I2P 2.13.0 (`9134f808337b401e8e53c73734c81fab04280c9d`) is a preferred secondary reference, not a blocking harness requirement if standalone orchestration is disproportionate;
- direct SSU2 session/I2NP interoperability is not equivalent to full public-router interoperability;
- environment-dependent external tests must remain explicit dedicated-lane work and must never silently pass when the required external peer/configuration is absent.

## Handoff

Execute Plan **162** now. On its successful closure, resume Plan **161**. Do
not begin Milestone 9 or mark Milestone 8 closed until Plan 161 satisfies its
full mandatory acceptance criteria.
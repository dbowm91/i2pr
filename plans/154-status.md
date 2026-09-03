# Plan 154 status — Milestone 8 SSU2 v2 transport and reachability roadmap

Status: **`registered-m8-ssu2-v2-roadmap-blocked-by-plan153`**.

Registered: **2026-09-03**.

Plan of record:
[`plans/154-m8-ssu2-transport-and-reachability-roadmap.md`](154-m8-ssu2-transport-and-reachability-roadmap.md).

## Current authority

```text
plan_153 = active-post-m7-authority-and-ci-hygiene
plan_154 = registered-m8-ssu2-v2-roadmap-blocked-by-plan153

milestone8_protocol = SSU2-v2-classical
ssu2_pq_v3_v4 = deferred-compatibility-watch
ssu1 = not-implemented
milestone8_implementation = blocked-by-plan153

first_m8_executable_after_plan153 = 155
m8_sequence = 155 -> 156 -> 157 -> 158 -> 159 -> 160 -> 161
```

## Architecture decisions locked by this roadmap

- new runtime-neutral `i2pr-transport-ssu2` crate;
- production UDP ownership stays in `i2pr-runtime`;
- reuse `i2pr-transport` manager/resource/delivery/reachability contracts;
- no per-packet task/timer architecture;
- real localhost UDP is the local/interop substrate;
- no root, namespaces, containers, VM, systemd, or public I2P dependency;
- mandatory final independent implementation is exact-pinned i2pd 2.61.0 (`635b013a612ff47278ef02acf8580a28e10e26c5`) in both directions;
- Java I2P 2.13.0 (`9134f808337b401e8e53c73734c81fab04280c9d`) is a preferred secondary reference, not a blocking harness requirement if standalone orchestration is disproportionate;
- direct SSU2 session/I2NP interoperability is not equivalent to full public-router interoperability.

## Handoff

Do not execute Plan 155 until `plans/153-status.md` is explicitly passed. After Plan 153 closure, execute Plans 155–161 sequentially.
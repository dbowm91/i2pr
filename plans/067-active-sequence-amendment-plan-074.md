# Plan 067 active-sequence amendment: Plan 074 real-driver and constrained-host correction

## Authority

- Date: 2026-07-31.
- Status: active planning amendment and plan registration.
- Parent roadmap: `plans/067-milestone-3-staged-interoperability-corrective-roadmap.md`.
- Corrective sub-roadmap: `plans/074-milestone-3-real-driver-and-constrained-host-corrective-roadmap.md`.
- This amendment governs whenever Plans 069-073, Plan 030 amendments, status files, README/AGENTS, architecture documentation, or skills conflict with the corrected sequence below.
- Historical records remain immutable. This amendment changes execution authority, not past evidence.

## Corrected repository state

```text
plan_068_staged_evidence = implemented
plan_069_runner_scaffolding = implemented_but_not_valid_mixed_router_lane
real_i2pd_driver = not_implemented
real_i2pd_library_linkage = absent
real_reference_process_in_plan069_runner = absent
real_mixed_router_attempts = 0
current_rootless_namespace_lane = unavailable
multipass_lane = unreliable_or_unavailable
milestone_3_development_validation = blocked_pending_plans_075_079
milestone_3_release_qualification = pending_plan_073
support = experimental
advertised = false
normal_daemon_activation = disabled
```

## Why Plan 070 is superseded

Plan 070 assumes that:

- the Plan 064 helper can build and execute genuine i2pd listen/dial behavior;
- the Plan 069 runner launches i2pr and i2pd as separate real processes;
- protocol milestones come from authentic events;
- the current host can execute the intended lane without additional lane work.

Those assumptions are false at the active repository state.

The current Plan 064 helper's listen/dial paths are terminal stubs when real pinned i2pd libraries are not linked. The current Plan 069 runner selects the i2pr launcher for both process handles and can promote protocol milestones without consuming reference-side structured evidence. The host also lacks the assumed rootless lane.

Therefore Plan 070 must not be executed or marked blocked merely because rootless namespaces are unavailable. It is superseded by the Plan 074 sequence.

## Why Plan 071 is superseded

Plan 071 depends on Plan 070 producing one genuine pass per direction. Because Plan 070's prerequisites are invalid, Plan 071 is not the active repeated-validation plan.

Plan 079 replaces it after the real driver, runner, lane, and first two-way execution are complete.

## Registered active plans

| Plan | Status | Purpose | Dependency |
| --- | --- | --- | --- |
| 074 | planned, active roadmap | Correct real-driver, runner, and constrained-host assumptions | Plan 067/068/069 history |
| 075 | planned, next | Correct Plan 069 process roles, events, provenance, and pass semantics | 074 |
| 076 | planned | Build/link real pinned i2pd and implement genuine inspect/listen/dial | 075 |
| 077 | planned | Select and prequalify Docker, QEMU, reduced-scope seccomp, or remote lane | 075, 076 |
| 078 | planned | Obtain first real pass in both i2pd directions and correct observed defects | 075-077 |
| 079 | planned | Obtain 3/3 per direction, negative controls, and continuation decision | 078 |
| 072 | conditional | Emissary differential validation only when it reduces a concrete uncertainty | after 078 or 079 decision |
| 073 | deferred final gate | Java+i2pd isolated release qualification | after development validation and suitable Level 3 lane |

## Active execution sequence

```text
Plan 075
  -> Plan 076
  -> Plan 077
  -> Plan 078
  -> Plan 079
  -> Plan 072 only if conditionally activated
  -> Plan 073 when final release-qualification environment exists
```

Do not execute:

```text
Plan 070 as active authority
Plan 071 as active authority
Plan 072 as a substitute for a real i2pd driver
Plan 073 on the constrained host without a valid Level 3 lane
```

## Constrained-host lane decision

The active full-runtime lane order is:

```text
1. existing accessible rootful Docker daemon; both routers in one --network none container
2. QEMU TCG guest with -nic none
3. manually triggered dedicated remote Linux runner with root-owned isolation
4. typed no-full-runtime-lane blocker
```

An inherited connected-descriptor plus seccomp lane may be used for reduced-scope protocol diagnostics, but it is not equivalent to normal listener/dialer runtime qualification.

Rootless namespaces, bubblewrap, rootless Podman/Docker, user-level systemd `PrivateNetwork`, and repeated Multipass recovery are not active work items on the known host.

## Documentation propagation requirements

Plan 075 through Plan 077 implementation must add concise active-status corrections to the relevant files when touched:

```text
README.md
AGENTS.md
docs/architecture/interop-apparatus.md
docs/protocol-support.md
tests/integration/ntcp2/README.md
.opencode/skills/i2pr-ntcp2-interop/SKILL.md
plans/030-milestone-3-closure.md or its active amendment
plans/069-status.md through a supersession note, not historical rewrite
```

Required wording:

- Plan 069 provides scaffolding/fake-process tests but is not current mixed-router evidence;
- the Plan 064 i2pd driver requires real library linkage and genuine transport implementation;
- Plan 074-079 are the active corrective sequence;
- zero real mixed-router attempts remain until Plan 078 executes;
- NTCP2 remains experimental/non-advertised.

## Planning and evidence invariants

1. No fake or synthetic fixture may become a passing live record.
2. No derived placeholder digest may satisfy provenance.
3. No listening-port probe may satisfy authentication, frame, or I2NP milestones.
4. One real i2pr process and one real reference process are mandatory.
5. Rootless namespace absence is an environment fact, not permission to weaken event or driver authenticity.
6. A container/guest lane does not compensate for a stub driver.
7. A real driver does not compensate for a fake runner.
8. Passing development validation does not close Plan 073.

## Handoff instruction

The next implementation model must read, in order:

```text
plans/074-milestone-3-real-driver-and-constrained-host-corrective-roadmap.md
plans/075-plan-069-runner-integrity-and-evidence-correction.md
```

It must execute Plan 075 only. It must not attempt the i2pd build, Docker/QEMU lane, or protocol run in the same pass unless Plan 075 has a truthful closure record and commit.
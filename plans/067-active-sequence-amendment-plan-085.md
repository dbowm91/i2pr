# Plan 067 active-sequence amendment: Plan 085 host-loopback development execution

## Authority

- Date: 2026-08-04.
- Status: active planning amendment and plan registration.
- Parent roadmap: `plans/067-milestone-3-staged-interoperability-corrective-roadmap.md`.
- Supersedes the active execution authority of `plans/067-active-sequence-amendment-plan-081.md`.
- Active successor roadmap: `plans/085-milestone-3-host-loopback-development-execution-roadmap.md`.
- Historical Plans 074-084 and their status records remain preserved.
- When current documentation conflicts with this amendment, this amendment governs execution order, plan status, and gate interpretation.

## Corrected repository state

```text
plan_075_runner_integrity = implemented
plan_076_real_i2pd_driver = implemented
plan_080_historical_lane_qualification = retained
plan_080_current_guest_availability = unavailable_or_stale
plan_082_state_preparation = implemented_and_closed
plan_083_implementation = complete
plan_083_execution = pending_under_plan_087
plan_084_implementation = complete
plan_084_execution = pending_under_plan_088
plan_084_historical_lane_decision = superseded_for_active_execution
plan_084_development_decision = pending
real_mixed_router_tcp_attempt = 0
real_ntcp2_handshake_attempt = 0
real_authenticated_frame_attempt = 0
real_delivery_status_decode_attempt = 0
protocol_conformance_result = not_yet_observed
current_primary_reference = i2pd_2_60_0
plan_079 = blocked_pending_plan_088
plan_072 = conditional_pending_plan_088
support = experimental
advertised = false
normal_daemon_activation = disabled
```

## Registered active plans

| Plan | Status | Purpose | Dependency |
| --- | --- | --- | --- |
| 085 | active roadmap | Reach first two-way real i2pd protocol result through an executable development lane | Plans 075-084 implementation history |
| 086 | planned, next | Correct execution status and enable bounded `host-loopback-development` placement | 085 |
| 087 | planned | Run and correct the first genuine `i2pr -> i2pd` probe | 086 or conditional 089 placement |
| 088 | planned | Run reverse `i2pd -> i2pr` probe and issue the development decision | 087 passed |
| 089 | conditional | Provide one manual `--network none` isolated container placement | Plan 086/088 exact activation token |
| 079 | blocked | Run repeated 3/3 validation and negative controls | Plan 088 decision and suitable isolation lane |
| 072 | conditional | Answer one precise unresolved reference divergence with Emissary | Plan 088 ambiguity decision |
| 073 | deferred final gate | Java+i2pd release qualification | later suitable Level 3 lane |

## Active execution sequence

```text
Plan 086
  -> Plan 087
  -> Plan 088
  -> Plan 079 when Plan 088 records two-way-development-probe-passed
  -> Plan 072 only when Plan 088 records ambiguous-reference-divergence

Plan 089 is activated only by manual-isolated-fallback-required.
```

## Selected technical route

Primary route:

```text
existing Plan 082 authentic preparation
+ existing Plan 076 real pinned i2pd driver
+ existing Plan 083/084 real-subprocess runners
+ new host-loopback-development placement
+ one exact DeliveryStatus per direction
```

Conditional fallback:

```text
one manually invoked rootful container
+ --network none
+ loopback only
+ same runners and record schemas
```

Emissary is not the execution fallback. It remains a diagnostic reference only after a real precise wire-stage ambiguity.

## Execution invariants

1. The topology name is exactly `host-loopback-development`.
2. Direct development placement accepts literal `127.0.0.1` only.
3. Direct host loopback does not claim release or network-isolation qualification.
4. Production daemon and support-profile paths must not recognize the development topology.
5. Exact source, binary, RouterInfo, Router Hash, message ID, and run-identity binding remain mandatory.
6. A protocol result begins only at an authentically observed `tcp_connected` stage.
7. Listener readiness, process survival, port probes, files, and fake events cannot promote protocol stages.
8. Plan 087 owns forward execution and bounded correction.
9. Plan 088 owns reverse execution and the development decision.
10. Plan 089 reuses the same runners; it does not create a second harness.
11. No plan may patch i2pd protocol semantics.
12. No plan may enable production NTCP2 or advertise support.
13. Recurring CI, release automation, and broad matrices are out of scope.
14. Java remains the final release authority.

## Plan 086 entry rule

Plan 086 is the only next executable plan.

It must:

- implement the development topology and placement;
- correct current status wording;
- run a listener-only preflight;
- stop before starting a dialer;
- write `plans/086-status.md`.

Do not execute Plan 087 in the same commit.

## Plan 087 entry rule

Plan 087 begins only when `plans/086-status.md` records:

```text
status = host-loopback-development-ready
```

or when `plans/089-status.md` records:

```text
status = manual-isolated-fallback-ready
```

It must produce a real current-run forward record. Fake-process and listener-only results cannot satisfy the gate.

## Plan 088 entry rule

Plan 088 begins only when `plans/087-status.md` records a passing instrumented forward result and behavior-neutral control comparison.

## Plan 079 unblock rule

Plan 079 becomes executable only when `plans/088-status.md` records:

```text
decision = two-way-development-probe-passed
```

The two development directions permit protocol continuation. Plan 079's Level 2 no-public-network closure still requires an isolated lane such as Plan 089 or another separately qualified lane. Direct host-loopback evidence alone cannot satisfy release/isolation predicates.

## Plan 072 activation rule

Plan 072 may be activated only when `plans/088-status.md` records:

```text
decision = ambiguous-reference-divergence
```

and binds one exact role, stage, disputed input artifact, and discriminating question.

## Plan 089 activation rule

Plan 089 may be activated only when Plan 086 or Plan 088 records exactly:

```text
manual-isolated-fallback-required
```

The trigger must be a demonstrated placement failure before TCP, not a protocol failure.

## Do not execute

```text
Plan 079 before the Plan 088 decision
Plan 072 as an environment workaround
Plan 089 for protocol rejection
new schema or runner frameworks before the first TCP attempt
Java qualification as a substitute for the i2pd development probes
production activation
recurring CI
```

## Required current-status propagation

Plan 086 implementation must propagate concise current meaning to:

```text
README.md
AGENTS.md
docs/architecture/interop-apparatus.md
docs/protocol-support.md
tests/integration/ntcp2/README.md
.opencode/skills/i2pr-ntcp2-interop/SKILL.md
plans/030-milestone-3-closure.md or a dated active amendment
```

Required meaning:

- Plan 082 is complete;
- Plan 083/084 runners are implemented but not executed over the wire;
- Plan 085 is active;
- Plan 086 is next;
- Plan 079 is blocked;
- Plan 072 is conditional;
- NTCP2 remains experimental and non-advertised.

## Handoff instruction

The next implementation model must read, in order:

```text
plans/085-milestone-3-host-loopback-development-execution-roadmap.md
plans/086-status-authority-and-host-loopback-development-lane.md
plans/083-084-execution-status-amendment-plan-085.md
plans/067-active-sequence-amendment-plan-085.md
```

It must implement Plan 086 only. It must not start the opposite peer or claim a protocol result.
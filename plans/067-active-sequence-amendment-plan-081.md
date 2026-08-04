# Plan 067 active-sequence amendment: Plan 081 pre-protocol and minimal i2pd correction

## Authority

- Date: 2026-08-01.
- Status: active planning amendment and plan registration.
- Parent roadmap: `plans/067-milestone-3-staged-interoperability-corrective-roadmap.md`.
- Prior corrective roadmap: `plans/074-milestone-3-real-driver-and-constrained-host-corrective-roadmap.md`.
- Active successor roadmap: `plans/081-milestone-3-pre-protocol-and-minimal-i2pd-corrective-roadmap.md`.
- This amendment governs when Plans 074-080, Plan 030 amendments, status files, README/AGENTS, architecture documentation, or skills conflict with the corrected sequence below.
- Historical records remain immutable. This amendment changes present execution authority and interpretation only.

## Corrected repository state

```text
plan_075_runner_integrity = implemented
plan_076_real_i2pd_driver = implemented
plan_077_lane_selection = implemented
plan_080_multipass_lane = qualified
plan_078_protocol_attempt = stopped_pre_protocol
plan_082_state_preparation = implemented
plan_083_forward_probe = implemented_schema_and_runner
plan_084_reverse_probe = implemented_schema_and_runner
plan_084_development_decision = lane-invalidated
real_i2pd_router_info_export = observed
real_i2pr_router_info_for_live_run = not_produced
real_tcp_connection_attempt = 0
real_ntcp2_handshake_attempt = 0
real_authenticated_frame_attempt = 0
real_i2np_delivery_status_attempt = 0
protocol_conformance_result = not_yet_observed
current_primary_reference = i2pd_2_60_0
emissary = conditional_only
plan_079 = blocked_pending_lane_availability
support = experimental
advertised = false
normal_daemon_activation = disabled
```

## Interpretation correction

The Plan 080 lane qualification is valid and remains active evidence that the owned Multipass guest can execute the required loopback-only full-runtime lane.

The Plan 076 driver qualification is valid and remains active evidence that the reference executable links and runs real pinned i2pd code.

The Plan 078/080 direction record is not a protocol rejection because the run did not reach TCP, Noise/NTCP2 authentication, SessionConfirmed, authenticated frame processing, or I2NP decode. The failure occurred while composing or preparing the i2pr side before a valid live scenario/process existed.

Therefore:

```text
blocked-protocol-defect
```

is retained only as the historical close label used by Plan 078's then-current acceptance rules. The active diagnosis is:

```text
pre-protocol launcher/harness contract defect
```

No protocol defect or protocol pass has yet been demonstrated.

## Registered active plans

| Plan | Status | Purpose | Dependency |
| --- | --- | --- | --- |
| 081 | planned, active roadmap | Correct pre-protocol composition and obtain minimal two-way i2pd evidence | Plans 075-080 history |
| 082 | planned, next | Add authentic i2pr state preparation, real hashes/run identity, precise errors, and truthful counters | 081 |
| 083 | planned | Run minimal real `i2pr -> i2pd` probe and identify first authentic protocol result | 082 |
| 084 | implemented, closed as `lane-invalidated` on this host | Run minimal real `i2pd -> i2pr` probe and issue development decision | 083 |
| 079 | blocked | Run 3/3 repeated i2pd development validation and negative controls | 084 decision |
| 072 | conditional | Use Emissary only for one precise unresolved i2pd differential question | 084 ambiguity decision |
| 073 | deferred final gate | Java+i2pd isolated release qualification | after development validation and suitable Level 3 lane |

## Active execution sequence

```text
Plan 082
  -> Plan 083
  -> Plan 084
  -> Plan 079 when Plan 084 records two-way-development-probe-passed
  -> Plan 072 only when Plan 084 records ambiguous-reference-divergence
  -> Plan 073 when final release-qualification prerequisites exist
```

Do not execute:

```text
Plan 078 again as active authority
Plan 079 before Plan 084 permits it
Plan 072 as a shortcut around i2pr state preparation
new environment research while the Plan 080 lane validates
Java/release qualification as a substitute for the minimal i2pd probe
```

## Selected technical route

The selected route is:

```text
existing Plan 080 Multipass lane
+ existing Plan 076 real i2pd driver
+ new i2pr test-only state-preparation command
+ minimal role-specific wire probe
```

Emissary is not the selected primary route because it would still require valid i2pr RouterInfo/state preparation and would introduce a new driver/integration surface before the existing i2pd path has reached the wire.

## Registration invariants

1. Strict Plan 065 fields remain mandatory and nonzero.
2. Preparation is separate from live scenario rendering.
3. Preparation does not use an allowlisted live scenario ID or a `-gen` derivative.
4. A preparation command cannot claim TCP, authentication, frame, or I2NP progress.
5. Process counters increment only after successful process creation.
6. Broad evidence finalization cannot replace the actual minimal-probe terminal result.
7. `typed-harness-operation-failed` cannot close Plans 083 or 084.
8. A protocol result begins only after at least `tcp_connected` is authentically observed.
9. One real independent reference implementation is sufficient for the current development probe.
10. Java remains the later release authority; Emissary remains a conditional differential reference.
11. No production activation or support advertisement follows from Plans 082-084.

## Required planning/status propagation

Implementation of Plan 082 must update concise active-status wording where necessary in:

```text
README.md
AGENTS.md
docs/architecture/interop-apparatus.md
docs/protocol-support.md
tests/integration/ntcp2/README.md
.opencode/skills/i2pr-ntcp2-interop/SKILL.md
plans/030-milestone-3-closure.md or an active amendment
plans/079-repeated-i2pd-development-validation-and-continuation-decision.md
```

Required meaning:

- the Plan 080 lane is qualified;
- the Plan 076 i2pd driver is real;
- Plan 078 stopped pre-protocol;
- Plans 081-084 are the active corrective sequence;
- no real NTCP2 handshake attempt has yet been established by retained evidence;
- Plan 079 remains blocked;
- NTCP2 remains experimental/non-advertised.

Do not rewrite historical status records to imply they originally used the corrected diagnosis. Add amendments or clearly dated active-status sections.

## Plan 079 unblock rule

Plan 079 becomes executable only when `plans/084-status.md` records:

```text
decision = two-way-development-probe-passed
```

and binds one passing compact record for each direction plus behavior-neutral control comparison.

Any other decision keeps Plan 079 blocked.

## Plan 072 activation rule

Plan 072 may be activated only when `plans/084-status.md` records:

```text
decision = ambiguous-reference-divergence
```

and states one exact role/stage question. It must not become a general second qualification project.

## Handoff instruction

The next implementation model must read, in order:

```text
plans/081-milestone-3-pre-protocol-and-minimal-i2pd-corrective-roadmap.md
plans/082-i2pr-state-preparation-and-mixed-runner-contract-correction.md
plans/080-diagnostic-correction-amendment-plan-081.md
```

It must execute Plan 082 only. It must not execute a live i2pd wire run in the same pass. Plan 082 requires a separate closure record and commit before Plan 083 begins.
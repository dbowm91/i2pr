# Plans 072 and 079 gate amendment: Plan 088 decision authority

## Authority

- Date: 2026-08-04.
- Status: active gate amendment.
- Parent roadmap: Plan 085.
- Supersedes the active gate references in:
  - `plans/072-activation-amendment-plan-084.md`;
  - `plans/079-repeated-i2pd-development-validation-and-continuation-decision.md`;
  - `plans/067-active-sequence-amendment-plan-081.md`.
- Historical Plan 084 decision text remains preserved but is not active gate authority.

## Current gate state

```text
plan_072 = inactive
plan_079 = blocked
plan_088_development_decision = pending
```

No retained real wire result currently activates either plan.

## Plan 079 entry gate

Plan 079 may begin only when `plans/088-status.md` records exactly:

```text
decision = two-way-development-probe-passed
```

and binds:

```text
forward_instrumented_record_sha256
forward_control_record_sha256
reverse_instrumented_record_sha256
reverse_control_record_sha256
source_commit
reference_revision
placement_record_sha256
exact Router Hash correlation
exact DeliveryStatus ID correlation
cleanup = clean
```

Any other Plan 088 decision keeps Plan 079 blocked.

### Development versus isolation authority

A Plan 086 direct host-loopback pass proves protocol-development behavior only. It does not satisfy Plan 079's existing no-public-network Level 2 closure predicate.

Therefore:

- `two-way-development-probe-passed` may unblock Plan 079 implementation and repetition work;
- final Plan 079 `level2-passed` status requires the repetitions and required isolation predicate to run in Plan 089 or another separately qualified no-public-network lane;
- direct host-loopback records may be retained as development baseline records but cannot be relabeled as isolated evidence.

This distinction must be explicit in `plans/079-status.md`.

## Plan 072 activation gate

Plan 072 may activate only when `plans/088-status.md` records exactly:

```text
decision = ambiguous-reference-divergence
```

and includes:

```text
direction
role
highest_stage_reached
bounded_reason_code
disputed_input_artifact_sha256
instrumented_record_sha256
control_record_sha256
specification_section
exact_diagnostic_question
expected_discriminating_outcomes
```

Plan 072 remains inactive when:

- execution has not reached TCP;
- the issue is placement, build, preparation, scenario rendering, cleanup, or evidence finalization;
- i2pr ownership is clear from source and specification review;
- the forward or reverse path already passes;
- the goal is merely to avoid i2pd;
- the proposed work is general Emissary qualification.

## Plan 089 relationship

Plan 089 is an execution-placement fallback, not a reference differential.

Use Plan 089 for:

```text
manual-isolated-fallback-required
```

Use Plan 072 for:

```text
ambiguous-reference-divergence
```

Never substitute one for the other.

## Required updates during implementation

Plan 086 must propagate the new gate authority to concise current-status sections.

Plan 088 must update:

```text
plans/088-status.md
plans/079 status/entry wording
plans/072 activation wording
README.md
AGENTS.md
docs/architecture/interop-apparatus.md
docs/protocol-support.md
```

Do not rewrite historical Plan 084 records as though they originally used Plan 088 authority.

## Acceptance criteria

This amendment is satisfied when:

- Plan 079 reads Plan 088 as its active entry authority;
- Plan 072 reads Plan 088 as its active ambiguity authority;
- direct host-loopback development evidence is not confused with isolated Level 2 evidence;
- Plan 089 placement fallback remains distinct from Plan 072 differential work;
- no plan is activated before a genuine wire result.

## Handoff

The model implementing Plan 088 must read this amendment before writing its final decision.
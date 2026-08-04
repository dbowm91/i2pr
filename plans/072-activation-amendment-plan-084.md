# Plan 072 activation amendment: Plan 084 ambiguity decision

## Authority

- Date: 2026-08-01.
- Status: active activation amendment.
- Applies to `plans/072-conditional-emissary-ntcp2-differential-validation.md`.
- Parent authority: Plan 081 and the Plan 067 active-sequence amendment for Plan 081.

## Current gate assessment (2026-08-04)

Plan 082 is implemented and closed: the launcher prepares authentic
endpoint-bound i2pr state, the runner validates both peer identities and
freezes real correlation fields, and the Rust `validate-scenario` command
parses the strict live scenario without opening a peer. Plan 083 is
implemented as the in-process record schema, focused test matrix, and
test-only runner orchestration module; no retained evidence reaches
`tcp_connected`. Plan 084 is implemented as the reverse-direction probe
schema, the focused test matrix, the real-subprocess runner
orchestration module, and the static boundary check extension. Plan 084
closes with `decision = lane-invalidated` because the Plan 046
rootless sealed-namespace probe returns `blocked_unprivileged_user_namespace`
on this host and the Plan 080 Multipass guest cannot complete on this
constrained host (per Plan 051). Plan 072 is not activated: no
wire-stage reference divergence exists because the wire has not been
exercised on this host.

A future plan may activate Plan 072 only when Plan 084 records
`decision = ambiguous-reference-divergence` with one precise role/stage
question, or when a future host becomes runnable and Plan 083/084
produce a real wire-stage reference divergence during the development
probe sequence.

## Corrected activation point

Plan 072 remains conditional and non-mandatory.

Its ordinary activation point remains after i2pd development validation when additional confidence is requested. However, Plan 084 may activate it before Plan 079 only when the reverse/minimal probes reach a real wire stage and leave a precise unresolved reference divergence.

Required Plan 084 decision:

```text
decision = ambiguous-reference-divergence
```

Required written diagnostic question example:

```text
Does Emissary accept the same i2pr SessionConfirmed RouterInfo that pinned i2pd rejects?
```

or:

```text
Does Emissary produce/accept the disputed first authenticated data frame in the same role and stage?
```

## Non-activation cases

Do not activate Plan 072 when:

- Plan 082 state preparation is incomplete;
- live scenario rendering still fails;
- no TCP connection was established;
- the failure is evidence finalization or cleanup bookkeeping;
- i2pr ownership is already clear from source/specification review;
- both i2pd directions pass;
- the only goal is to avoid using the existing Plan 076 driver;
- the work would become general Emissary feature or release qualification.

## Scope when activated by Plan 084

Limit execution to:

- one pinned Emissary revision;
- one exact role/direction;
- one exact disputed protocol stage;
- X25519 baseline with PQ/ML-KEM disabled;
- direct signed RouterInfo exchange;
- the existing Plan 080-style isolated loopback lane or an equivalent in-process runtime seam when it proves the same stage;
- one compact differential record.

The result is diagnostic. It does not replace Java release authority or automatically determine conformance by majority vote.

## Plan 079 relationship

Plan 079 remains blocked while the Plan 084 ambiguity is unresolved.

After Plan 072:

- if ownership is localized and corrected, rerun the affected Plan 083/084 direction;
- if both minimal directions pass, Plan 084 may record `two-way-development-probe-passed` and unblock Plan 079;
- if ambiguity remains, keep Plan 079 blocked and record insufficient evidence.

## Handoff rule

A model reading Plan 072 must also read:

```text
plans/072-activation-amendment-plan-084.md
plans/084-i2pd-to-i2pr-reverse-probe-and-development-decision.md
```

It must not implement a general Emissary lane without the exact Plan 084 activation decision and diagnostic question.

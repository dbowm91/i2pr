# Plan 072: conditional Emissary NTCP2 differential validation

## Status and activation

- Status: planned-conditional. **Inactive on 2026-08-01.** See the activation
  amendment at
  [`plans/072-activation-amendment-plan-084.md`](072-activation-amendment-plan-084.md)
  for the current gate: Plan 072 may activate only after Plan 084 reaches a
  real wire stage, source/specification review cannot identify ownership, and
  `plans/084-status.md` records
  `decision = ambiguous-reference-divergence` plus one precise role/stage
  question.
- Parent roadmap: Plan 067.
- Requires Plan 068 and Plan 069 closed.
- Historically evaluated after Plan 079. Plan 084 may also activate it
  before Plan 079 only when the minimal probes leave an unambiguous
  wire-stage reference divergence; that gate is defined in the amendment
  above.
- This plan is not on the mandatory development-validation critical path.
- Execute only when the activation amendment, Plan 079 status, or an
  explicit maintainer request authorizes a third-implementation comparison.
- Plan type: bounded source-locked differential interoperability lane.

## Objective

Use Emissary as a secondary independent NTCP2 implementation to localize an ambiguous i2pr/i2pd disagreement or increase confidence after i2pd development validation, without making Emissary the sole conformance authority and without creating another large harness architecture.

The plan must answer a narrow question:

```text
Does the disputed i2pr behavior interoperate with Emissary in the same role and at the same protocol stage?
```

It must not attempt general Emissary feature parity, full router qualification, SAM/tunnel interoperability, or release certification.

## Activation criteria

At least one must be true:

1. i2pr fails reproducibly against i2pd at a stage where specification/source review does not identify the owner;
2. one i2pd direction passes and the reverse direction fails repeatedly with ambiguous peer behavior;
3. i2pd instrumented/control behavior disagrees and Emissary can separate an i2pr defect from an i2pd observer/driver defect;
4. Plan 079 passes and the maintainer explicitly requests a third implementation before further development.

If none are true, create a short status record marking:

```text
not-needed-for-current-development-gate
```

and do not implement the lane.

## Authority and limitations

Emissary is useful because it provides a Rust NTCP2 implementation with an embeddable/runtime-generic architecture and structured internal transport events. It is not the official Java implementation and may share language/library assumptions with i2pr.

Therefore:

- Emissary may corroborate or differentiate behavior;
- Emissary may not alone close Level 3;
- a two-of-three result is diagnostic, not automatically normative;
- when implementations disagree, the I2P specification and Java behavior remain the final reference hierarchy;
- Emissary-specific extensions such as ML-KEM must be disabled for the baseline X25519 comparison unless the pinned i2pd/Java baseline supports the same negotiated mode.

## Scope selection

The implementing agent must first write the exact diagnostic question into `plans/072-status.md` or a temporary execution note before code changes.

Examples:

```text
Compare SessionCreated padding/clock acceptance as i2pr initiator.
Compare SessionConfirmed RouterInfo handling as i2pr responder.
Compare first data-frame length obfuscation and AEAD.
Compare DeliveryStatus block/I2NP decoding after authenticated frame.
```

Only the required roles/stages should be exercised.

## Source-lock requirement

Before building or running Emissary:

- select one exact upstream revision/tag;
- record repository URL, commit SHA, Cargo.lock digest, and relevant crate versions;
- inspect the exact revision's NTCP2 transport/session/runtime APIs;
- record whether X25519-only mode can be forced;
- record local-address acceptance configuration;
- record RouterInfo import and outbound message/event APIs;
- do not use moving `master` as an unrecorded dependency.

Recommended files:

```text
tests/integration/ntcp2/reference-drivers/emissary/source-lock.json
tests/integration/ntcp2/reference-drivers/emissary/source-verification.md
```

A re-pin requires an explicit update with reason and digest changes.

## Preferred implementation strategy

Use the smallest upstream-supported path in this order:

1. an existing Emissary CLI/router configuration that can run NTCP2-only over loopback with direct RouterInfo/state preparation;
2. an existing embeddable `emissary-core` API with the real NTCP2 transport and minimal router context;
3. a test-only adapter around existing public/internal APIs in a source-locked checkout.

Do not:

- copy Emissary's NTCP2 implementation into i2pr tests;
- reimplement its protocol in a custom helper;
- patch its cryptography or acceptance policy;
- require SAM, I2CP, tunnels, NetDB bootstrap, or public I2P participation;
- generalize the adapter beyond the exact diagnostic need.

## Target topology

Reuse the Plan 069 host-compatible loopback model:

```text
127.0.0.1:<ephemeral-a> <------ NTCP2 ------> 127.0.0.1:<ephemeral-b>
```

Required properties:

- one i2pr process;
- one Emissary process/helper;
- fresh state;
- network ID 99;
- IPv4 loopback only;
- direct signed RouterInfo exchange;
- X25519 baseline;
- no reseed/bootstrap/public peer discovery;
- no SSU2, SAM, I2CP, tunnels, floodfill, or external DNS;
- exact DeliveryStatus ID and Router Hash continuity when data phase is in scope;
- Plan 069 audit and cleanup rules.

## Required directions

When full two-way comparison is needed:

```text
i2pr-to-emissary-ipv4
emissary-to-i2pr-ipv4
```

When the activation question concerns one role only, the plan may execute only that role, but the status must state why the other direction was unnecessary.

## Deliverables when activated

### D1. Emissary source verification and driver seam

Create the smallest bounded driver/config surface under:

```text
tests/integration/ntcp2/reference-drivers/emissary/
```

Expected contents only as needed:

```text
source-lock.json
source-verification.md
build-driver.sh or Cargo build instructions
run-driver.sh or a thin executable/config wrapper
README.md
```

Avoid an observer patch if structured upstream events or an in-process event receiver already provide the required proof. If a patch is unavoidable:

- it must be passive and compile-time gated;
- it may observe only after successful protocol operations;
- it must not change timing/control flow/return values;
- an uninstrumented control must be compared.

### D2. Plan 069 adapter extension

Add Emissary support to the loopback runner only after the driver interface is stable.

Keep reference-specific behavior in a small adapter, recommended:

```text
tests/integration/ntcp2/harness/emissary_direct_driver.py
```

Do not add a plugin registry. A direct explicit branch/enum is sufficient.

### D3. Differential record

Create a concise report, recommended:

```text
tests/integration/ntcp2/qualification/emissary-differential.json
```

Required fields:

```text
source_commit
diagnostic_question
emissary_revision
i2pd_revision_and_comparison_record
role_or_directions
protocol_stage
observed_i2pr_i2pd_result
observed_i2pr_emissary_result
specification_interpretation
ownership_inference
confidence
next_action
release_qualified = false
```

Allowed ownership inference:

```text
likely-i2pr
likely-i2pd-driver-or-i2pd
likely-reference-divergence
insufficient-evidence
no-disagreement-observed
```

Inference must be explained and cited to structured events/source/specification. It may not be derived from majority vote alone.

### D4. Focused tests

Test only:

- source-lock validation;
- strict config;
- role/direction mapping;
- exact correlation fields when used;
- event-to-smoke-record mapping;
- cleanup/audit behavior;
- control-build equivalence if instrumented;
- lower-tier/non-release classification.

Do not duplicate the complete Plan 069 test matrix.

### D5. Status record

Create or update:

```text
plans/072-status.md
```

For executed plan, record:

- activation reason;
- selected revision;
- implementation strategy;
- exact commands;
- results;
- differential inference;
- resulting correction plan or closure decision.

For skipped plan, record:

- Plan 079 passed or ambiguity resolved;
- activation criteria not met;
- status `not-needed-for-current-development-gate`;
- no code changes required.

## Differential interpretation guide

### i2pr fails against i2pd and Emissary at the same stage

Inference: likely i2pr defect, especially when both references agree on the same exact rejection.

Action:

- write a narrow corrective plan or fix the owning i2pr surface;
- add a focused regression;
- rerun i2pd Level 2 sequence after any production change.

### i2pr passes Emissary but fails i2pd

Inference: ambiguous reference divergence or i2pd driver/config/observer defect.

Action:

- compare pinned i2pd source semantics, RouterInfo/address handling, clock/padding limits, and asynchronous send/receive events;
- use Java later if normative behavior remains unclear;
- do not weaken i2pr to match Emissary automatically.

### i2pr passes i2pd but fails Emissary

Inference: likely Emissary-specific compatibility issue or i2pr behavior accepted by i2pd but rejected by another implementation.

Action:

- inspect specification and Emissary source;
- record as differential issue;
- do not block Level 2 unless the failure exposes a clear conformance violation.

### all pass

Inference: increased development confidence.

Action:

- record non-release result;
- continue ordinary development;
- leave Java/Level 3 open.

## Validation commands

Exact commands depend on the selected revision and strategy and must be recorded by implementation.

Focused baseline should remain bounded:

```bash
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_emissary_direct_driver.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke.py'
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
git diff --check
```

Do not run the entire historical release-evidence suite unless shared release code changes.

## Non-goals

Plan 072 does not:

- make Emissary mandatory;
- replace i2pd as initial validator;
- replace Java as release authority;
- qualify SSU2 or ML-KEM;
- test tunnels, SAM, I2CP, NetDB, or full router behavior;
- add CI;
- perform performance/load testing;
- produce Level 3 evidence;
- expand the i2pr production dependency graph.

## Stop rules

Stop and record a typed blocker when:

- the selected Emissary revision cannot be source-locked;
- the only usable path requires public-network bootstrap;
- the only adapter requires copying/reimplementing NTCP2;
- a required patch changes protocol behavior;
- the work expands beyond the diagnostic question;
- i2pd ambiguity is already resolved and Emissary no longer provides decision value;
- the lane begins delaying mandatory Plan 079 closure without an activation reason.

## Closure criteria

Executed Plan 072 closes when:

- source revision and API path are verified;
- the smallest bounded lane runs the activated role/directions;
- exact results are compared with i2pd;
- an ownership inference and next action are documented;
- cleanup/audit pass;
- no release claim is made;
- `plans/072-status.md` is complete.

Skipped Plan 072 closes when:

- activation criteria are evaluated;
- status is recorded as not needed;
- no unnecessary code is added.

## Small-model handoff instructions

- First decide whether the plan is activated. Do not implement by default.
- Write the single diagnostic question before source work.
- Use one pinned revision.
- Prefer upstream configuration/public APIs over patches.
- Add only the direction needed to answer the question.
- Do not create a generalized reference-driver interface.
- Treat differential results as evidence for source/spec analysis, not a vote.

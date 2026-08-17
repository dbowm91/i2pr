# Plan 115 handoff

- Status: **ready for execution**
- Date: 2026-08-17
- Plan-of-record: [`plans/115-qualified-independent-short-build-consumption-and-external-delivery.md`](115-qualified-independent-short-build-consumption-and-external-delivery.md)
- Predecessor: [`plans/114-status.md`](114-status.md)
- Roadmap: [`plans/115-117-external-delivery-to-live-netdb-roadmap.md`](115-117-external-delivery-to-live-netdb-roadmap.md)
- Primary target: one independent native short-build consumer, then the smallest already-existing authenticated delivery lane if available

## Start here

Do **not** reopen Plans 109-114. Their local short-build result is the input to this plan.

Current state:

```text
plan_111                         = retained-core-crypto-corrected
plan_112                         = passed-outbound-pre-delivery-closure
plan_113                         = passed-inbound-reference-reconciliation
plan_114                         = passed-terminal-routing-chain-correction
short_build_local_outbound       = strict-established
short_build_local_inbound        = strict-established
qualified_external_delivery      = ready-plan115
normal_daemon_ntcp2              = disabled-and-unenableable
ntcp2                            = experimental-non-advertised
development_ntcp2                = protocol-defect-localized-at-noise_authenticated
```

The purpose is to obtain independent evidence without allowing the historical NTCP2 harness to block router development again.

## Execution decision in one page

### First objective: Q0

Feed the exact production-generated Plan-114 STBM into one independent implementation's **native** short-build processing path.

Preferred order:

1. i2pd if its pinned source can expose a small native consumer;
2. Emissary if it offers a materially smaller native tunnel-build seam;
3. Java I2P only as fallback.

The independent code must actually perform its own target-record recognition, request decryption/validation, and accept/reject processing. Merely parsing I2NP type 25 does not pass Q0.

### Second objective: Q1

If Q0 passes and the existing Rust NTCP2/runtime tooling can carry the message with no more than one narrow, clearly i2pr-owned correction, attempt authenticated loopback delivery to the reference process.

Do not rebuild the old environment/harness.

### Third objective: Q2

If the reference can return the native type-26 reply through the selected lane, feed the exact reply body to `BuildEvent::BuildReply` and require `ShortBuildOutcome::Established`.

## Required product bridge

The production source path must be:

```text
ShortBuildStateMachine::prepare
  -> deliver_action
  -> ShortBuildAction::Deliver
  -> canonical I2NP type-25 wrapper
  -> reference/delivery lane
```

Critical framing rule:

```text
ShortBuildAction::Deliver.message
    = [count byte] || [count * 218 record bytes]

DeferredBuildRecords::new(...)
    expects ONLY [count * 218 record bytes]

I2npBody::ShortTunnelBuild encoder
    writes the count byte again
```

Therefore:

1. validate the existing delivery payload;
2. split the first count byte from raw records;
3. create `DeferredBuildRecords` from raw records only;
4. encode I2NP type 25;
5. decode it again and assert that its body equals the original count-prefixed delivery payload exactly.

Never pass the already count-prefixed buffer directly to `DeferredBuildRecords`.

## Scope boundaries

Allowed production changes are limited to the smallest bridge/composition work required to thread the existing `ShortBuildAction::Deliver` into the existing I2NP/transport boundaries.

Likely owners:

```text
i2pr-proto         existing I2NP type 25/26 codec only

i2pr-tunnel        existing ShortBuildAction producer; only small reusable adapter if dependency direction permits

i2pr-transport     existing EncodedI2npMessage / DeliveryRequest contract; no replacement abstraction

i2pr-runtime       existing NTCP2 authenticated-link composition if Q1 is attempted

tools/i2pr-interop one-shot experimental composition only
```

Forbidden:

- daemon NTCP2 activation or advertisement;
- SSU2;
- public I2P participation;
- generic I2NP router dispatcher;
- new Python orchestration;
- rootless namespaces, privileged namespaces, Docker/Multipass/VM dependency;
- broad reference topology;
- short-build crypto/layout changes inside Plan 115;
- repeated NTCP2 correction passes.

## Evidence tiers

```text
Q0 = independent native short-build consumption
Q1 = authenticated transport delivery into independent router
Q2 = reply returned and i2pr reaches Established
```

A Q0 pass is intentionally useful on its own. If Q0 passes and Q1 remains transport-blocked, record that split result and proceed to local Milestone 5 data-plane work. Do not call Q0 a live mixed-router result.

## Exact execution sequence

1. Confirm HEAD is a clean descendant of Plan 114 closure.
2. Read Plan 115 completely.
3. Trace `ShortBuildAction::Deliver` and existing I2NP type-25/type-26 codec ownership.
4. Inventory i2pd and Emissary native consumer seams before writing an adapter.
5. Select one reference and pin repository revision/version.
6. Implement/test the canonical production I2NP bridge with the no-double-prefix invariant.
7. Run full local validation.
8. Execute Q0 once.
9. If Q0 hits a trivial adapter/build defect, make one correction and one confirmation run.
10. If Q0 passes, attempt Q1 only if the existing Rust runtime lane is small enough.
11. Q1 permits at most one unambiguously i2pr-owned narrow Rust correction and one confirmation run.
12. Attempt Q2 only when the reference already exposes a return path; do not create new architecture solely for Q2.
13. Create `plans/115-status.md` with the exact evidence classification.
14. Update this handoff and the 115-117 roadmap note to point to the actual next action.

## Stop immediately if

- the selected independent router cannot expose a native short-build consumer without substantial internal surgery;
- the result reaches a cryptographic disagreement with Plans 109-114;
- Q1 requires changing NTCP2 Noise/handshake semantics;
- a second transport correction would be required;
- the environment again pushes toward namespace/VM/container machinery;
- the only way to proceed is a generic new test harness.

In those cases, record the exact blocker or localized protocol stage in `plans/115-status.md`. Do not improvise another validation program.

## Required closure classifications

Use one of the plan's typed outcomes. The most important are:

```text
passed-external-established
passed-independent-native-consumer
protocol-defect-localized
blocked-no-bounded-independent-consumer-seam
blocked-transport-before-authentication
blocked-transport-after-authentication-before-i2np
blocked-reference-api-no-return-path
environment-or-build-blocked
```

If Q0 passes while Q1 is blocked, retain both facts separately:

```text
independent_short_build = passed-independent-native-consumer
qualified_live_delivery = blocked-<exact-stage>
plan_116_local_data_plane = unblocked
```

## Acceptance summary

Plan 115 cannot close until:

- production `ShortBuildAction::Deliver` bytes are the actual source of the reference input;
- canonical I2NP wrapping is tested without double-prefixing;
- one independent implementation reaches its native short-build processing path, or a precise bounded-consumer blocker is recorded;
- all evidence is sanitized and pinned to exact revisions/hashes;
- local workspace validation is green;
- daemon NTCP2 remains disabled/unenableable and non-advertised;
- no historical harness scope is revived.

The desired next state is not "more validation infrastructure." It is either independently consumed short-build protocol bytes or one narrow, actionable defect.

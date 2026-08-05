# Plan 088 status: reverse host-loopback probe and development decision

## Status

Implementation surface delivered on 2026-08-04. The reverse probe
schema (`i2pr-minimal-i2pd-reverse-probe-v1`), the focused test
matrix, the runner orchestration module, and the Plan 088 decision
vocabulary are landed and green locally. The reverse direction
identifier is exactly `i2pd-to-i2pr-ipv4`. The Plan 088 development
decision recorded in this status is:

```text
decision = insufficient-evidence
plan_086 = host-loopback-development-ready
plan_087 = open_pending_plan_090_reference_driver_correction
forward_attempt = authentic_pre_tcp_rejection_retained
plan_088 = blocked_pending_plan_087_pass
```

Plan 090 closed the Plan 087 zero-address RouterInfo defect
(`ntcp2.published` was `false` because the driver stored an
`int` rather than a `bool` and the i2pd `boost::program_options`
map was empty when the driver called `SetOption`). Plan 090
applied four behavior-neutral corrections in the i2pd direct
driver:

1. `set_bool_option("ntcp2.published", true)` — store the option
   as `bool` to match the `value<bool>()->default_value(true)`
   registration in `libi2pd/Config.cpp` line 330.
2. `i2p::config::ParseCmdline(1, fake_argv, ignoreUnknown=true)`
   followed by `Finalize()` — populate the option store with
   declared defaults before the driver mutates individual
   options.
3. `set_uint16_option` helper for `port` and `ntcp2.port` — store
   as `uint16_t` to match the `value<uint16_t>()` registration
   in `Config.cpp` lines 63 and 331.
4. `i2p::transport::transports.SetCheckReserved(false)` — disable
   reserved-range filtering so loopback addresses survive
   `RouterInfo::ReadFromBuffer` deserialization.

The Plan 090 driver also fails closed with
`router-info-endpoint-mismatch` if the authoritative in-memory
RouterInfo does not carry the exact configured NTCP2 endpoint.

The Plan 090 instrumented forward attempt
(`/tmp/opencode/plan090-real-20260805174541-fresh/forward-record.json`)
authenticated the i2pd listener (`listener_ready`) and reached
TCP, then recorded a NTCP2 protocol failure. The i2pd direct
driver emitted `process_started`, `router_info_exported`, and
`listener_ready`; the i2pr dialer emitted a terminal status with
`result = authentication_failed` and `reason_code = handshake_failed`
after `drive_initiator_handshake` returned
`Io(ExactIoError { kind: Closed })`. The Plan 083 pre-TCP
classifier correctly mapped this to `terminal_result =
pre_protocol_rejected` because no `tcp_connected` event was
observed, but the NTCP2 handshake itself failed after the TCP
connection established, so the per-direction record is a
pre-protocol rejection rather than a TCP-authenticated failure.

The Plan 090 pass criteria require an authentic
`ntcp2_authenticated` event on both sides plus a decoded
DeliveryStatus. Neither event was observed, so the
`first-instrumented-attempt-pre-tcp-rejected` Plan 087 status
remains `first-instrumented-attempt-pre-tcp-rejected` — the
retained record is now a clean committed-head Plan 090 attempt
against the corrected driver rather than the original
dirty-tree diagnostic. The Plan 090 closure remains open: the
forward direction did not pass.

The Plan 088 entry gate requires `plans/087-status.md` to bind
`status = passed` plus a non-zero instrumented and control
forward record digest, the same source commit, and the same
pinned i2pd revision. The Plan 090 commit satisfies the
"narrow forward correction commit" entry condition, but the
forward record digest is non-zero (the per-direction record is
written) and the forward direction is not yet a passing wire
attempt. The Plan 088 implementation surface therefore remains
delivered without a wire result; the active execution lane is
`insufficient-evidence` until Plan 090 closes with a passing
forward direction.

This record does not claim NTCP2 interoperability. It does not
authorize Plan 079 (repeated development validation) or Plan 073
(release qualification). The Plan 088 implementation surface is
preserved for any future host where the entry gate becomes
satisfied.

## Delivered implementation surface

- `tests/integration/ntcp2/harness/minimal_i2pd_reverse_probe.py`
  — extended to allow the Plan 086 `host-loopback-development`
  topology kind in `ALLOWED_TOPOLOGY_KINDS` (inherited from the
  shared `minimal_i2pd_probe` module). The reverse-direction
  schema, direction marker, reason-code allowlist, and observed-event
  validator are unchanged from Plan 084.
- `tests/integration/ntcp2/harness/minimal_i2pd_probe.py` — added
  the Plan 086 `host-loopback-development` topology kind to the
  shared `ALLOWED_TOPOLOGY_KINDS` set and the new bounded
  `DEVELOPMENT_ONLY_TOPOLOGY_KINDS` marker. The development-only
  set explicitly lists the host-loopback topology and forbids
  relabeling it as isolated or release evidence.
- `tests/integration/ntcp2/harness/plan083_runner.py` and
  `tests/integration/ntcp2/harness/plan084_runner.py` — both
  runners now accept `host-loopback-development` in their lane
  validators and top-level topology override paths. The
  development topology never satisfies any release/isolation
  predicate.
- `tests/integration/ntcp2/harness/test_plan083.py` and
  `tests/integration/ntcp2/harness/test_plan084.py` — extended
  the topology allowlist assertions to cover the new development
  topology, the development-only marker contract, and the runner
  acceptance path.
- `tests/integration/ntcp2/harness/test_plan088.py` — the Plan
  088 test matrix (35 cases) covering the bounded development
  decision vocabulary, the Plan 079 entry gate, the Plan 072
  activation gate, the handoff fields, the development-only
  topology contract, the reverse probe schema contract, the
  cross-direction rejection, the runner-constant alignment, and
  the module-boundary (no release/bundle/certificate/rootless/
  Multipass authority).
- `scripts/check-ntcp2-interoperability.sh` — extended to enforce
  the Plan 088 test matrix presence, the locked decision
  vocabulary, the `host-loopback-development` topology coverage,
  the plan-of-record reference, and the
  `plans/088-status.md` decision token plus the
  prohibition of the legacy `lane-invalidated` and
  `same-stage-two-way-i2pr-defect` tokens.

## Plan 088 development decision

```text
decision = insufficient-evidence
```

The Plan 088 vocabulary is exactly five values:

```text
two-way-development-probe-passed
one-way-passed-reverse-defect
ambiguous-reference-divergence
manual-isolated-fallback-required
insufficient-evidence
```

`insufficient-evidence` is the documented Plan 088 outcome when
"a real result cannot be reproduced deterministically after one
bounded reproduction cycle and no safe ownership conclusion can be
made." On this host the prerequisite forward direction has not
executed and no real wire run has been retained; the Plan 086
lane contract has not closed; the Plan 089 manual-isolated
fallback has not been activated. The decision therefore cannot
be `two-way-development-probe-passed`, `one-way-passed-reverse-defect`,
or `ambiguous-reference-divergence`; the closest match is
`insufficient-evidence`.

The historical `lane-invalidated` token carried by the legacy
Plan 084 closure is intentionally **not** reused here. Plan 088
supersedes the Plan 084 closure vocabulary for the active
reverse-probe authority, and the gate amendment
(`plans/072-079-gate-amendment-plan-088.md`) records the change.
The static boundary checker
(`scripts/check-ntcp2-interoperability.sh`) rejects any future
`plans/088-status.md` that re-introduces the legacy
`lane-invalidated` or `same-stage-two-way-i2pr-defect` tokens.

## Gate handoff

```text
plan_079_entry_gate = decision != two-way-development-probe-passed -> blocked
plan_072_activation_gate = decision != ambiguous-reference-divergence -> inactive
plan_079 = blocked_pending_plan_088_two_way_pass
plan_072 = inactive_pending_plan_088_ambiguity
plan_086 = planned_next_executable
plan_087 = blocked_pending_plan_086
plan_088 = insufficient_evidence_no_wire_result
```

The Plan 088 handoff fields required for any future
`two-way-development-probe-passed` closure are:

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

The Plan 088 handoff fields required for any future
`ambiguous-reference-divergence` closure are:

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

None of these fields are bound on this host; the
`insufficient-evidence` decision does not require them.

## Forward and reverse record digests

No authentic record was produced. The Plan 088 record digests
required by the `two-way-development-probe-passed` decision are
zero on this host:

```text
forward_instrumented_record_sha256 = 0x0000000000000000000000000000000000000000000000000000000000000000
forward_control_record_sha256      = 0x0000000000000000000000000000000000000000000000000000000000000000
reverse_instrumented_record_sha256 = 0x0000000000000000000000000000000000000000000000000000000000000000
reverse_control_record_sha256      = 0x0000000000000000000000000000000000000000000000000000000000000000
source_commit                      = 0x00000000000000000000000000000000000000000
reference_revision                 = 0x00000000000000000000000000000000000000000
placement_record_sha256            = 0x0000000000000000000000000000000000000000000000000000000000000000
cleanup                            = not-run
```

The reverse runner
(`execute_reverse_probe` in `plan084_runner.py`) and the forward
runner (`execute_real_probe` in `plan083_runner.py`) both detect
the host blocker through the `I2PR_PLAN046_HOST_BLOCKER` environment
variable and emit a typed `lane_invalid` `reverse-probe-record.json`
or `probe-record.json` with zero binary/router-info/router-hash
digests and an empty observed-events list. On this host both runners
remain ready to be exercised in a qualified Plan 046 rootless lane
or a Plan 048/049 Multipass recovery guest.

## Lane authority

Plan 088 inherits the Plan 086 host-loopback-development lane
contract:

```text
topology_kind                 = host-loopback-development
development_only              = true
release_qualified             = false
isolation_qualified           = false
public_network_blocked        = unproven
parent_network_state_unchanged = true
endpoint_family               = ipv4
bind_address                  = 127.0.0.1
peer_address                  = 127.0.0.1
network_id                    = 99
reference                     = i2pd
```

The reverse runner validates the topology kind before any
preparation or live startup. The reverse probe record schema
rejects any topology outside `ALLOWED_TOPOLOGY_KINDS` and any
forbidden field. The forward and reverse runners never fall back
to SAM, HTTP, support-topology, or synthetic-fallback helpers
for any primary direction; the C++ i2pd direct driver is the
only allowlisted reference driver mode.

The Plan 089 manual-isolated fallback lane
(`manual-isolated-container-loopback`) is an execution-placement
fallback, not a reference differential. A `manual-isolated-fallback-required`
Plan 088 decision is reserved for the case where the previously
qualified direct development placement becomes non-executable
before TCP for a demonstrated placement reason; on this host the
prerequisite Plan 086 lane has not closed, so the decision
remains `insufficient-evidence`.

## Cross-host portability

The reverse probe module, the runner orchestration module, the
shared `minimal_i2pd_probe` topology allowlist, and the focused
test matrices travel with the repository unchanged. On a host
where the Plan 086 host-loopback-development lane becomes
executable (i.e., the current host acquires
`host-loopback-development-ready` or another lane becomes
qualified) the reverse probe may be invoked against real
subprocesses; the bounded Plan 088 development decision
vocabulary will resolve to whichever of the five exact values
reflects the wire result.

Cross-host portability for the Plan 046 rootless sealed-namespace
lane is deferred to `plans/047-cross-host-rootless-lane-expansion.md`.
Cross-host portability for the Plan 080 Multipass recovery guest is
bounded by the Plan 051 resource constraints.

## Future plan unblocking

| Plan | Precondition | Status after Plan 088 |
| --- | --- | --- |
| Plan 079 | requires Plan 088 `two-way-development-probe-passed` | remains blocked; the lane was never exercised, so no two-way development probe exists |
| Plan 072 | requires Plan 088 `ambiguous-reference-divergence` with one exact diagnostic question | remains inactive; no wire-stage reference divergence has been observed |
| Plan 073 | requires release-qualification evidence | remains inactive; Java qualification and the Plan 058/059/060/066 evidence path remain untouched |

No future plan is unblocked by this Plan 088 closure. Plan 079
remains explicitly blocked; Plan 072 remains explicitly inactive.
The Plan 079 entry-gate reference now points at this status
record, per the Plan 088 gate amendment
(`plans/072-079-gate-amendment-plan-088.md`).

## Validation

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan088.py'               passed (35)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan084.py'               passed (54)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083.py'               passed (50)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083_runner.py'        passed (14)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_minimal_i2pd_reverse_probe.py' passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_minimal_i2pd_probe.py'    passed (43)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'    passed (57)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_control.py'   passed
cargo fmt --all --check                                                                passed
cargo check --workspace --all-targets                                                  passed
cargo test --workspace                                                                 passed
bash scripts/check-ntcp2-interoperability.sh                                           passed
bash scripts/check-dependency-direction.sh                                             passed
bash scripts/check-runtime-boundaries.sh                                               passed
bash scripts/check-ntcp2-vectors.sh                                                    passed
bash scripts/check-rootless-interop-boundary.sh                                        passed
bash scripts/check-multipass-interop-boundary.sh                                       passed
git diff --check                                                                       passed
```

The full repository gates and boundary checks pass before commit.
The Plan 088 test matrix covers the bounded development decision
vocabulary, the Plan 079 entry gate, the Plan 072 activation gate,
the development-only topology contract, the reverse probe schema
contract, the cross-direction rejection, and the module boundary.
The static boundary checker enforces the test matrix presence, the
locked decision vocabulary, the `host-loopback-development`
topology coverage, the plan-of-record reference, the
`plans/088-status.md` decision token, and the prohibition of the
legacy `lane-invalidated` and `same-stage-two-way-i2pr-defect`
tokens.

The reverse probe record schema remains round-trippable and the
canonical `record_sha256` digest is stable across key ordering.
The schema rejects every forbidden field, generic reason code,
unknown topology, forward direction, and zero or out-of-range
DeliveryStatus message ID. The forward-direction schema rejects
every record that carries the reverse direction marker. The
reverse runner accepts the `host-loopback-development` topology
without classifying the lane as invalid; no protocol stage is
exercised by the unit tests.

## Handoff

The actual reverse probe runner may only be exercised when the
Plan 086 `host-loopback-development` lane closes as
`host-loopback-development-ready` (or the Plan 089
`manual-isolated-fallback-ready` placement is activated) and the
Plan 087 forward direction closes with a passing instrumented
forward record. On this host neither prerequisite is satisfied,
so the Plan 088 execution authority is `insufficient-evidence`
and the next executable plan-of-record for repeated development
validation remains blocked on the Plan 046 host becoming
`rootless_sandbox_available` or the Plan 080 Multipass guest
completing on a less constrained host.

The Plan 088 implementation surface travels with the repository
unchanged; the next time the Plan 086 lane is closed, the reverse
runner can be invoked against real subprocesses and the bounded
Plan 088 development decision vocabulary will resolve to whichever
of the five exact values reflects the wire result.

# Plan 088 status: reverse host-loopback probe and development decision

## Active correction — Plan 100 supersession (2026-08-11)

Plan 099/Plan 100 supersede Plan 088's Plan 095 sequencing for
active execution. The single Plan 099 one-job workflow now owns
both directions and both control comparisons in one run.

```text
plan_088 = historical-development-sequence-superseded-by-plan100
plan_087 = historical-development-sequence-superseded-by-plan100
plan_079 = deferred-to-pre-normal-ntcp2-activation-and-public-network-checkpoint
plan_095 = historical-superseded-by-plan099-single-job-lane
plan_099 = implementation-landed-exit-cleanup-complete-pending-live-run
plan_100 = exit-ready-awaiting-one-manual-run
ntcp2    = experimental-non-advertised
```

Plan 088's reverse runner architecture, host-loopback-development
lane contract, and bounded reverse-probe record schema are
preserved as the historical audit record below. The active
development result comes from the Plan 099 one-job forward/reverse
matrix; the Plan 088 reverse runner (`plan084_runner.py`) travels
with the repository unchanged.

## Status

```text
decision = insufficient-evidence
plan_086 = host-loopback-development-ready
plan_087 = open-pending-plan095-ci-forward-evidence-pair
plan_090 = routerinfo-correction-landed
plan_091 = historical-partial-correction
plan_092 = superseded-by-plan093
plan_093 = implementation-landed-closure-incomplete
plan_094 = implementation-landed-live-closure-environment-blocked
plan_095 = active-runner-provenance-corrected-awaiting-authoritative-rerun
plan_096 = passed-pre-dispatch-workflow-correction
plan_097 = passed-artifact-path-and-cleanup-correction
plan_098 = passed-runner-provenance-boundary-correction
plan_088 = blocked-pending-plan095-ci-closure
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
ntcp2 = experimental-non-advertised
```

Implementation surface delivered on 2026-08-04. The reverse probe
schema (`i2pr-minimal-i2pd-reverse-probe-v1`), the focused test
matrix, the runner orchestration module, and the Plan 088 decision
vocabulary are landed and green locally. The reverse direction
identifier is exactly `i2pd-to-i2pr-ipv4`. The Plan 088 development
decision recorded in this status is `insufficient-evidence` because:

1. the prerequisite forward direction has not executed a passing
   instrumented record under Plan 095;
2. no real wire run from a clean Plan 095 CI evidence pair has been
   retained;
3. the Plan 086 lane contract has closed as
   `host-loopback-development-ready` but the Plan 095 CI host-loopback
   live-wire closure lane has not yet produced a passing instrumented
   or control forward record;
4. the Plan 089 manual-isolated fallback has not been activated;
5. the active execution lane remains blocked pending Plan 095
   CI closure.

Plan 095 is the active plan of record and the single next
executable implementation authority. Plan 088 must not run until
Plan 095 records both a passing instrumented forward record and a
passing control forward record from the same CI evidence pair
bound to the same source commit and pinned i2pd revision.

Plan 094 remains implementation-landed with live closure
environment-blocked on this host. The Plan 094 active
sequence amendment is recorded in
`plans/094-plan093-completion-and-plan087-to-plan088-handoff.md`.
Plan 094 remained the active runner/provenance authority
between the Plan 093 implementation landing and the Plan 095
CI live-wire lane activation; the Plan 098 runner/provenance
boundary correction supersedes the Plan 094 local-host lane as
the authoritative forward-direction close path. Plan 094
remains implementation-landed; its local live closure
environment is blocked on this host.

The historical `lane-invalidated` token carried by the legacy
Plan 084 closure is intentionally **not** reused here. Plan 088
supersedes the Plan 084 closure vocabulary for the active
reverse-probe authority, and the gate amendment
(`plans/072-079-gate-amendment-plan-088.md`) records the change.
The static boundary checker
(`scripts/check-ntcp2-interoperability.sh`) rejects any future
`plans/088-status.md` that re-introduces the legacy
`lane-invalidated` or `same-stage-two-way-i2pr-defect` tokens.

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
  validators and top-level topology override paths. Plan 098
  further extends both runners to accept an explicit
  `i2pr_binary` path argument with mandatory path/hash
  verification at the runner entry. The development topology
  never satisfies any release/isolation predicate.
- `tests/integration/ntcp2/harness/preflight_runner.py` — the
  Plan 098 path-ownership contract is mirrored here so the
  preflight refuses an attempted-live execution with a
  reconstructed `target/debug/i2pr-interop` fallback path.
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
  `same-stage-two-way-i2pr-defect` tokens. The Plan 098 static
  surface is appended: the runner must accept an explicit
  `i2pr_binary` path, the wrapper must expose
  `--attempt-kind`, and `build-driver.sh` must use the canonical
  tracked-source identity.

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
executed a passing instrumented record and no real wire run from a
clean Plan 098 corrective commit has been retained; the Plan 086
lane contract has not closed with a passing Plan 087 forward
record; the Plan 089 manual-isolated fallback has not been
activated. The decision therefore cannot be
`two-way-development-probe-passed`, `one-way-passed-reverse-defect`,
or `ambiguous-reference-divergence`; the closest match is
`insufficient-evidence`.

## Gate handoff

```text
plan_079_entry_gate = decision != two-way-development-probe-passed -> blocked
plan_072_activation_gate = decision != ambiguous-reference-divergence -> inactive
plan_079 = blocked-pending-plan088-two-way-pass
plan_072 = inactive-pending-plan088-ambiguity
plan_086 = host-loopback-development-ready
plan_087 = open-pending-plan095-ci-forward-evidence-pair
plan_090 = routerinfo-correction-landed
plan_091 = historical-partial-correction
plan_092 = superseded-by-plan093
plan_093 = implementation-landed-closure-incomplete
plan_094 = implementation-landed-live-closure-environment-blocked
plan_095 = active-runner-provenance-corrected-awaiting-authoritative-rerun
plan_096 = passed-pre-dispatch-workflow-correction
plan_097 = passed-artifact-path-and-cleanup-correction
plan_098 = passed-runner-provenance-boundary-correction
plan_088 = blocked-pending-plan095-ci-closure
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

## Plan 095 authoritative live-wire attempt (2026-08-10)

The first authoritative Plan 095 manual CI dispatch against the
post-Plan 097 head advanced through the full contract, build,
forward-instrumented, forward-control, and validate-gate
job graph. Six bounded corrections landed between the original
Plan 095 implementation commit and the authoritative run:

```text
1. observer header: INTEROP_RING_CAPACITY moved out of the
   I2PD_INTEROP_OBSERVER ifdef so the control build compiles
2. i2pd build: --parallel bounded to 2 to fit the
   github-hosted ubuntu-24.04 runner memory budget
3. verify-build-artifacts: accept the i2pd_libraries_sha256
   field as three space-separated 64-hex digests
4. build-driver.sh and build-i2pr-interop: chmod 0755 after
   the bare cp so the upload/download round-trip preserves
   the binary's executable bit
5. live jobs: restore-executable-bit step after the artifact
   download so the live ``test -x`` guard accepts the
   archiver-rewritten mode
6. prepare-run-root: drop the run-root ``mkdir -p`` so the
   live probe, which refuses a pre-existing run root, is the
   single owner that creates the directory
```

The forward-instrumented job launched the live probe against
the canonical `host-loopback-development` topology. The probe
reached the pre-protocol state preparation phase and then
failed closed with `terminal_result = pre_protocol_rejected`
and `reason_code = pre-protocol-preparation-failed`. The
control job correctly refused to launch because the
instrumented evidence did not pass.

Plan 098 reclassified the August 10 result as a **pre-protocol
runner/provenance failure**, not a wire-level NTCP2
classification. The live runner reached the pre-protocol state
preparation phase, then the runner reconstructed a
non-authoritative `target/debug/i2pr-interop` path instead of
using the canonical absolute artifact path supplied by the
wrapper. No TCP or NTCP2 wire-level conclusion is supported by
that result.

The plan-of-record forward-direction close therefore still
requires a passing instrumented record, which this host's
`host-loopback-development` topology has not produced. The
forward-instrumented record is preserved under the
`plan095-instrumented-evidence` artifact on the run id
recorded above; the gate record was not produced because the
control job correctly refused to run.

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

No authentic passing record was produced. The Plan 088 record
digests required by the `two-way-development-probe-passed` decision
are zero on this host:

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

The Plan 091 retained forward record digest and the Plan 092 retained
forward record digest (`696aa1339d3d950f9fec2a2e0b1f5bede2035761a71e167af6ab28b249cc998d`)
are preserved verbatim as the diagnostic history; they do **not**
satisfy the forward instrumented or control record digest
requirement.

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
prerequisite Plan 086 lane has not closed with a passing Plan 087
forward record, so the decision remains `insufficient-evidence`.

## Cross-host portability

The reverse probe module, the runner orchestration module, the
shared `minimal_i2pd_probe` topology allowlist, and the focused
test matrices travel with the repository unchanged. On a host
where the Plan 086 host-loopback-development lane becomes
executable and Plan 098 closes with a passing instrumented and
control forward record, the reverse probe may be invoked against
real subprocesses; the bounded Plan 088 development decision
vocabulary will resolve to whichever of the five exact values
reflects the wire result.

Cross-host portability for the Plan 046 rootless sealed-namespace
lane is deferred to `plans/047-cross-host-rootless-lane-expansion.md`.
Cross-host portability for the Plan 080 Multipass recovery guest is
bounded by the Plan 051 resource constraints.

## Future plan unblocking

| Plan | Precondition | Status after Plan 098 |
| --- | --- | --- |
| Plan 098 | Plan 097 audit closed; runner/provenance boundary corrective pass landed | closed (static + regression matrix green) |
| Plan 095 | Plan 098 runner/provenance correction committed; one manual dispatch follows | next executable |
| Plan 079 | requires Plan 088 `two-way-development-probe-passed` | remains blocked |
| Plan 072 | requires Plan 088 `ambiguous-reference-divergence` with one exact wire-stage question | remains inactive |
| Plan 073 | requires release-qualification evidence | remains inactive |

No future plan is unblocked by this Plan 088 closure until Plan 095
closes with a passing instrumented forward record and a passing
control forward record from the same CI evidence pair. Plan 079
remains explicitly blocked; Plan 072 remains explicitly inactive.
The Plan 079 entry-gate reference points at this status record,
per the Plan 088 gate amendment
(`plans/072-079-gate-amendment-plan-088.md`).

Plan 096 closed the four demonstrated Plan 095 workflow execution
defects and added the pre-dispatch audit. Plan 097 closed the two
narrow workflow defects that remained after Plan 096:
producer/consumer artifact path identity (one canonical absolute
`BUILD_OUTPUT` path used by every producer, verifier, manifest,
uploader, and live consumer) and disposable run-root cleanup
(strict `rm -rf --` with an exact path guard and an unsuppressed
absence assertion). Plan 098 closed the runner/provenance
ownership boundary that the first authoritative Plan 095 dispatch
exposed before any TCP or NTCP2 wire activity. Plan 095 evidence
is still pending; exactly one manual Plan 095 GitHub Actions
dispatch follows the Plan 098 correction commit.

## Validation

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan098.py'               passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan088.py'               passed (35)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan084.py'               passed (54)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083.py'               passed (50)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan083_runner.py'        passed (16)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_minimal_i2pd_reverse_probe.py' passed
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_minimal_i2pd_probe.py'    passed (43)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'    passed (57)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_control.py'   passed
bash scripts/check-plan095-workflow.sh                                            passed
bash scripts/check-ntcp2-interoperability.sh                                      passed
cargo fmt --all --check                                                          passed
cargo check --workspace --all-targets                                             passed
cargo test --workspace                                                           passed
bash scripts/check-dependency-direction.sh                                        passed
bash scripts/check-runtime-boundaries.sh                                          passed
bash scripts/check-fixture-manifest.sh                                            passed
bash scripts/check-ntcp2-vectors.sh                                              passed
bash scripts/check-rootless-interop-boundary.sh                                   passed
bash scripts/check-multipass-interop-boundary.sh                                  passed
git diff --check                                                                  passed
```

The full repository gates and boundary checks pass before commit.
The Plan 088 test matrix covers the bounded development decision
vocabulary, the Plan 079 entry gate, the Plan 072 activation gate,
the development-only topology contract, the reverse probe schema
contract, the cross-direction rejection, and the module boundary.
The static boundary checker enforces the test matrix presence, the
locked decision vocabulary, the `host-loopback-development`
topology coverage, the plan-of-record reference, the
`plans/088-status.md` decision token, the prohibition of the
legacy `lane-invalidated` and `same-stage-two-way-i2pr-defect`
tokens, and the Plan 098 runner/provenance ownership boundary.

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
`manual-isolated-fallback-ready` placement is activated) **and**
Plan 098 closes with a passing instrumented forward record and a
passing control forward record bound to the same corrective commit.
On this host neither prerequisite is satisfied, so the Plan 088
execution authority is `insufficient-evidence` and the next
executable plan-of-record is Plan 095.

The Plan 088 implementation surface travels with the repository
unchanged; the next time the Plan 086 lane is closed and Plan 095
closes, the reverse runner can be invoked against real subprocesses
and the bounded Plan 088 development decision vocabulary will
resolve to whichever of the five exact values reflects the wire
result.

## Plan 099 closure amendment (2026-08-11)

Plan 099 supersedes the active execution interpretation of the
Plan 095–098 multi-job CI/provenance sequence. The CI workflow
is reduced from 988 lines (five jobs with cross-job binary
artifact transfer) to 398 lines (one `development-interop` job
that builds and executes in the same fresh workspace). The Plan
098 D1–D4 evidence-integrity corrections are landed in the new
single-job workflow. The wrapper requires `--i2pr-binary` for
every attempted-live path (preflight, forward, reverse); the
legacy `/nonexistent` fallback is removed. All Plan 052–098
plan-number-specific Python test and runner files are deleted;
unique functional assertions are migrated into the bounded
functional test set (`test_execution_lane.py`,
`test_i2pd_direct_driver.py`, `test_i2pd_direct_control.py`,
`test_minimal_i2pd_probe.py`). The
`scripts/check-plan095-workflow.sh` and
`scripts/check-ntcp2-loopback-smoke-boundary.sh` scripts are
removed. The `scripts/check-ntcp2-interoperability.sh` static
boundary check is trimmed from 1870 to 124 lines.

The recorded Plan 088 development decision remains
`insufficient-evidence`. Plan 095 remains the single next
executable plan; exactly one manual Plan 095 GitHub Actions
dispatch follows the Plan 099 correction commit. Plan 079's
3/3 repeated-direction validation campaign is moved to the
pre-normal-activation / pre-public-network integration
checkpoint rather than gating offline/local router development.
Plan 072 remains inactive.
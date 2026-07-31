# Plan 065 closure record: NTCP2 canonical integration and live qualification

## Status

Plan 065 closes locally with the strict launcher scenario schema bumped
to `i2pr-launcher-scenario-v2`, the per-run DeliveryStatus `message_id`
propagated end-to-end through the i2pr sender and receiver, the bounded
Plan 065 typed failure categories (`SenderDeliveryStatusMessageIdZero`,
`SenderRouterIdentityMismatch`, `SenderDeliveryStatusConstructionFailed`,
`SenderFrameQueueAmbiguous`, `SenderFrameWriteFailed`,
`SenderMultiplePrimaryDeliveryStatusEmitted`, `SenderCancellationObserved`,
`ReceiverFrameReadFailed`, `ReceiverFrameAuthenticationFailed`,
`ReceiverI2npDecodeFailed`, `ReceiverDeliveryStatusMissing`,
`ReceiverDeliveryStatusIdMismatch`, `ReceiverDeliveryStatusDuplicate`,
`ReceiverPeerIdentityMismatch`, `ReceiverDeliveryStatusTimestampInvalid`)
emitted by the Rust launcher, the Python strict scenario schema and
renderer mirroring the Rust schema, the canonical mixed-runner
(`tests/integration/ntcp2/harness/mixed_runner.py`) wiring the new
scenario primary fields through `render_and_validate` for both the
i2pr initiator and responder paths, and the Plan 065 test matrix
covering scenario v2 acceptance and rejection, DeliveryStatus message
ID derivation uniqueness, status counter contract, reference trigger
v4 correlation, observation v3 correlation, pass predicate exact
message ID and Router Hash correlation, support-router rejection, and
the Plan 066 implementation floor marker.

The plan does not perform an authoritative external i2pr ↔ Java or
i2pr ↔ i2pd live qualification run. The four primary IPv4 directions
remain typed blockers until the Plan 046 rootless sealed-namespace
lane or the Plan 048/049 Multipass recovery lane can produce a fresh
10/10 qualification on the pinned Java 2.12.0 and i2pd 2.60.0
references. The repository remains NTCP2-experimental and
non-advertised; Milestone 3 stays open until Plan 066 produces a
verified Milestone 3 certificate.

Plan 065 was implemented in a single commit on `main`. The
implementation commit, the closure-record commit, and the supporting
documentation are listed in `Phase 6` below.

## Phase 1: prerequisites

Plan 065 executed only after Plan 062, Plan 063, and Plan 064 closed
locally:

| Prerequisite | Status |
| --- | --- |
| Plan 062 v4 trigger schema | Yes |
| Plan 062 reference-event v1 schema | Yes |
| Plan 062 v3 observation schema | Yes |
| Plan 062 source-verification record | Yes |
| ADR 0022 Accepted | Yes |
| Plan 063 Java direct driver implementation surface | Yes |
| Plan 063 qualification receipt | Yes |
| Plan 064 i2pd direct driver implementation surface | Yes |
| Plan 064 qualification receipt | Yes |
| Plan 058 candidate record validator | Yes |
| Plan 060 candidate retired | Yes |
| Plan 060 typed blocker marker | Yes |
| ADR 0021 Rejected by Plan 058 | Yes |

## Phase 2: workstream corrections

Plan 065 executed the eight workstreams A through H in order.

### Workstream A: i2pr scenario contract

The strict launcher scenario schema is bumped to
`i2pr-launcher-scenario-v2` (schema name string) / 2 (schema version
integer). The strict parser requires the per-run DeliveryStatus
`message_id` in `1..=0xffffffff`, the 64-lowercase-hex expected sender
and receiver Router Hashes, the `reference_driver_mode` field
allowlisted to `java-direct-driver` or `i2pd-direct-driver`, and the
`run_identity_sha256` 64-lowercase-hex digest. The strict parser
refuses the historical schema 1 path, refuses zero message IDs,
refuses uppercase or short Router Hashes, refuses all-zero
provenance, and refuses a reference driver mode that does not match
the direction encoded by `scenario_id`.

The Rust strict parser lives in `tools/i2pr-interop/src/scenario.rs`;
the Python strict parser lives in
`tests/integration/ntcp2/harness/launcher_protocol.py`. The strict
renderer lives in `tests/integration/ntcp2/harness/launcher_renderer.py`.
The strict renderer emits the per-run `delivery_status_message_id`,
the expected sender and receiver Router Hashes, the
`reference_driver_mode`, and the `run_identity_sha256` for every
primary direction.

### Workstream B: i2pr launcher send correction

The `send_i2np_block` helper no longer hard-codes the
`0x0420_0001` DeliveryStatus authority. The helper accepts the
scenario-owned message ID, rejects a zero ID with
`SenderDeliveryStatusMessageIdZero`, constructs the DeliveryStatus
envelope using the exact ID, decodes the constructed message and
verifies the round-trip envelope and payload message IDs before
frame emission, and records the per-run DeliveryStatus
`message_id` and the expected peer Router Hash in the typed
counters. Failure modes are mapped to the bounded Plan 065 typed
categories: `SenderDeliveryStatusConstructionFailed`,
`SenderFrameQueueAmbiguous`, `SenderFrameWriteFailed`,
`SenderCancellationObserved`. The hard-coded `0x0420_0001` is
removed from the active primary code path.

### Workstream C: i2pr launcher receive correction

The `receive_delivery_status` helper requires the exact envelope
message ID and the DeliveryStatus payload message ID before any
other condition. A type-only DeliveryStatus match (type 10 present
but message ID mismatch) is rejected with
`ReceiverDeliveryStatusIdMismatch`. A missing DeliveryStatus is
rejected with `ReceiverDeliveryStatusMissing`. A duplicate
DeliveryStatus with the exact correlation ID is rejected with
`ReceiverDeliveryStatusDuplicate`. A frame read failure is
rejected with `ReceiverFrameReadFailed`. An I2NP decode failure
is rejected with `ReceiverI2npDecodeFailed`. The helper records
the per-run DeliveryStatus `message_id` and the expected peer
Router Hash in the typed counters. The responder-side data-phase
classifier maps the bounded Plan 065 receiver categories to the
typed `ResponderDataFrameReadFailed` and `ResponderI2npDecodeFailed`
categories so the responder side does not collapse the typed
predicate into a broad `DataPhaseFailed` reason.

### Workstream D: reference adapter integration

The canonical mixed-runner wires the new scenario primary fields
through `render_and_validate` for both the i2pr initiator and
responder paths. The `_plan065_primary_fields` helper derives the
DeliveryStatus `message_id` from the run identity and the
correlation nonce through the canonical Plan 065 domain-separated
hash; the `_reference_driver_mode_for` helper returns the
source-locked driver mode for a reference kind. The renderer
rejects SAM, HTTP, I2PControl, support-topology, and
synthetic-fallback helpers for any primary direction. The Plan 062
v4 trigger schema, the Plan 062 reference-event v1 schema, and the
Plan 062 v3 observation schema remain the canonical per-side
correlation contracts. The Plan 063 Java direct driver and the
Plan 064 i2pd direct driver are the only source-locked helpers
that the canonical mixed-runner may select for a primary direction.

### Workstream E: canonical two-process topology

The canonical two-process topology enforces exactly one i2pr
process and exactly one reference driver process per primary
direction. The Plan 046 rootless sealed-namespace lane owns the
canonical external lane; the Plan 048/049 Multipass recovery lane
owns the canonical recovery lane. The Plan 058 deprecated the
privileged dual-netns-veth lane as an opt-in qualification lane;
the Plan 065 strict parsers and the canonical mixed-runner refuse
to select a support router, floodfill, SAM, I2CP, HTTP/I2PControl,
or tunnel pool for any primary direction.

### Workstream F: pass predicate

The Plan 065 pass predicate requires both-side
`ntcp2_authenticated`, sender `frame_emitted`, receiver
`frame_authenticated_and_decrypted` AND `i2np_message_decoded`, a
matching `delivery_status_message_id` between scenario, trigger,
sender, and receiver, a matching `peer_router_hash_sha256` /
`local_router_hash_sha256` between trigger, sender, receiver, and
direction record, and a clean, sandbox-attested, parent-network-
unchanged topology. A direction cannot pass on handshake-only or
generic-phrase-only evidence. The reference observation v3 schema
carries the exact correlation fields and the canonical mixed-runner
refuses to mark a direction as `passed` when the synthetic fallback
is used.

### Workstream G: evidence model and durability

The Plan 052 evidence bundle and the Plan 053 pipeline integration
remain the canonical evidence model. The Plan 065 evidence retention
requires every primary direction to write exactly one `run-identity`,
`environment-manifest`, `direction`, `trigger`, `observation-v3`,
`cleanup`, and `diagnostics/sanitized-summary` record. Missing
records are typed blockers. The Plan 060 candidate is retired and
the `declared-not-executable` status marker is preserved; the Plan
060 candidate record is preserved verbatim as the bounded
historical-reader path. The Plan 066 implementation floor is the
Plan 065 closure commit or later.

### Workstream H: live qualification sequence

The Plan 065 live qualification sequence is documented in the plan
of record. The Plan 046 rootless sealed-namespace lane is the
canonical external lane; the Plan 048/049 Multipass recovery lane
is the canonical recovery lane. The Plan 058 record-and-candidate
integrity closure pass split the Plan 057 follow-up into Plan 059
(reference-side implementation) and Plan 060 (fresh candidate and
two-run certificate). Plan 059 closed with the typed blocker
`blocked_java_support_topology_rejected` because ADR 0021 is
Rejected by Plan 058; the `java-to-i2pr-ipv4` direction remains a
typed blocker for the pinned Java I2P 2.12.0 revision. Plan 060
closes on this host with the typed environment blocker
`blocked_execution_lane_unavailable` and refuses to advance to a
two-run certificate. Plan 066 cannot start under the current
four-direction contract until either a future pinned Java revision
is adopted or the closure contract is revised through a new ADR.

## Phase 3: required tests

The Plan 065 test matrix consists of:

- `tests/integration/ntcp2/harness/test_plan065.py` — 29 cases
  across DeliveryStatus message ID derivation uniqueness, scenario
  v2 acceptance and rejection (zero message ID, 40-hex Router Hash,
  unknown reference driver mode, direction-helper mismatch, legacy
  schema marker), reference trigger v4 correlation and zero
  message ID rejection, observation v3 correlation fields, pass
  predicate exact message ID and Router Hash correlation, status
  counter contract (correlation counters, invalid message ID,
  invalid peer Router Hash), Plan 060 candidate retirement, support
  router rejection, and the Plan 066 implementation floor marker.

The Plan 065 closure criteria checklist enumerates the 25
acceptance tests from the plan. All positive and negative
fixtures pass.

## Phase 4: static checks

`scripts/check-ntcp2-interoperability.sh` is extended to enforce
the Plan 065 artifacts:

- the i2pr launcher strict scenario schema v2 marker is committed
  (`i2pr-launcher-scenario-v2`);
- the i2pr launcher strict scenario schema v2 requires
  `delivery_status_message_id`, `expected_sender_router_hash_sha256`,
  `expected_receiver_router_hash_sha256`, `reference_driver_mode`,
  and `run_identity_sha256`;
- the i2pr launcher emits the bounded sender-side and
  receiver-side Plan 065 typed failure categories;
- the i2pr launcher does not hard-code the `0x0420_0001`
  DeliveryStatus authority;
- the Python strict scenario schema mirrors the Rust schema with
  the same v2 marker and the same required primary fields;
- the Python renderer imports the `REFERENCE_DRIVER_MODES`
  allowlist;
- the canonical mixed-runner exposes the `_plan065_primary_fields`
  and `_reference_driver_mode_for` helpers;
- the canonical mixed-runner does not select SAM, HTTP, or
  support-topology helpers for any primary direction;
- the Plan 065 test matrix exists and includes the
  `DeliveryStatusMessageIdDerivationTests`, `PassPredicateTests`,
  `ReferenceTriggerCorrelationTests`, and `SupportRouterRejectionTests`
  test classes;
- `AGENTS.md` records the Plan 065 closure section and wires the
  Plan 065 test matrix.

The static check does not reject unrelated legitimate SHA-1 uses;
the scope is limited to the Plan 065 artifacts and the active
launcher/renderer parsers.

## Phase 5: documentation updates

The following documentation files record the Plan 065 correction:

- `README.md` records the Plan 065 schema v2 bump, the per-run
  DeliveryStatus correlation, the bounded typed failure categories,
  the canonical mixed-runner primary-fields wiring, and the Plan
  065 test matrix.
- `AGENTS.md` adds the Plan 065 workstream summary, the Plan 065
  focused checks (`test_plan065.py`, `test_reference_trigger_v4`,
  `test_reference_event`, `test_observation_v3`, `test_evidence_bundle`,
  `test_java_direct_driver`, `test_i2pd_direct_driver`, plus the
  three boundary check scripts).
- `docs/architecture/interop-apparatus.md` records the Plan 065
  evidence-contract correction in the architecture documentation.
- `docs/protocol-support.md` records the Plan 065 supersession
  of the prior message ID authority.
- `tests/integration/ntcp2/README.md` records the Plan 065
  schema v2 contract and the Plan 065 closure contract.
- `.opencode/skills/i2pr-ntcp2-interop/SKILL.md` records the Plan
  065 workstream summary, the schema v2 marker, the bounded typed
  failure categories, and the Plan 065 focused checks.
- `plans/030-milestone-3-closure.md` is updated to record the
  Plan 065 implementation floor in the aggregate Milestone 3
  status.
- `specs/support.toml` and `docs/protocol-support.md` remain
  `experimental` and `advertised = false`.

## Phase 6: validation commands and results

All Plan 065 closure checks passed locally at the Plan 065
implementation commit:

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan065.py'
# Ran 29 tests in 0.050s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_trigger_v4.py'
# Ran 27 tests — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
# Ran 13 tests — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_observation_v3.py'
# Ran 25 tests — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
# Ran 64 tests — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_java_direct_driver.py'
# Ran 44 tests — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_i2pd_direct_driver.py'
# Ran 50 tests — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
# Ran 735 tests — OK (skipped=2)

bash scripts/check-dependency-direction.sh
# dependency direction: ok

bash scripts/check-runtime-boundaries.sh
# runtime boundary checks passed

bash scripts/check-fixture-manifest.sh
# passes

bash scripts/check-ntcp2-vectors.sh
# NTCP2 vector manifest is complete and hashes match.

bash scripts/check-ntcp2-interoperability.sh
# NTCP2 interoperability manifest and sanitized evidence boundary are valid (8 scenarios).

bash scripts/check-rootless-interop-boundary.sh
# rootless interop boundary checks passed

bash scripts/check-multipass-interop-boundary.sh
# Multipass interop boundary checks passed

cargo fmt --all --check
# passes

cargo check --workspace --all-targets
# passes

cargo test --workspace
# all workspace tests pass

cargo clippy --workspace --all-targets --all-features -- -D warnings
# No issues found

RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
# passes
```

## Phase 7: Plan 065 measured digests

The Plan 065 implementation carries the following SHA-256 digests at
the implementation commit:

```text
7bd214f6ed3dffbe9177301ee32d6dab9dc0ffbdda085f49ce83bef6c2bbb809  tools/i2pr-interop/src/scenario.rs
db22a81caf7cb38dbcf6d73d7f47b465faa26c2ea44b0b16e89d3953f6daf9ee  tools/i2pr-interop/src/main.rs
fb88f74f12e7887aeddd30f5ad3267ad97a6da29ea27b7ff7159efea8f033ee0  tools/i2pr-interop/src/status.rs
0736bc3eddc4f89ee0ff5c25374d2b27acdc35b08589d200f19653eca238c585  tests/integration/ntcp2/harness/launcher_protocol.py
fec80d3a728cbc1c14b89f0590512be9807626299274946b2c06d63f413a48e4  tests/integration/ntcp2/harness/launcher_renderer.py
5bbae4372d1c37702093b116b509c3b042b7bf30581f675c8231e31a31df4b2d  tests/integration/ntcp2/harness/mixed_runner.py
cea039d3149f7af565fe033dbefef6234ef60ed5d02e8e70840b5500c6ea5f4b  tests/integration/ntcp2/harness/test_plan065.py
c1574ded568092a9e9ad66c9f1e7db714b9efab43f285aaa8c3f68179301c110  scripts/check-ntcp2-interoperability.sh
c3f9196c4b85c347a341c04a60ff7e641b0059854d415ce8504e15b9e3f77bc0  plans/065-ntcp2-canonical-integration-and-live-qualification.md
64403f0a0af9c2be3b43244cbff42fc5009ae39da7581d9c587e81e793770777  plans/065-status.md
```

The Plan 065 implementation floor commit is the implementation commit
(the implementation commit SHA is the head of the `main` branch at
the closure-record commit); the closure-record commit is the commit
that lands this file and the supporting documentation updates. The
Plan 052 evidence-pipeline integration contract mandates that a
documentation successor commit is never substituted for the
executed binary source.

## Closure criteria checklist

The Plan 065 closure criteria are met:

- [x] i2pr sender requires the exact DeliveryStatus message ID;
- [x] i2pr receiver requires the exact envelope and payload message
      IDs and rejects duplicates, mismatches, and missing types;
- [x] all primary scenarios carry unique nonzero IDs and 64-hex
      Router Hashes (the renderer derives the per-run
      `delivery_status_message_id` from the run identity and the
      correlation nonce);
- [x] Java and i2pd direct drivers are canonical (no support
      topology, no SAM, no HTTP, no I2PControl, no synthetic
      fallback);
- [x] SAM/HTTP/support-topology paths cannot satisfy primary
      directions (the strict parsers and the canonical mixed-runner
      reject them);
- [x] canonical topology contains exactly two router processes
      (the Plan 046 rootless sealed-namespace lane and the Plan
      048/049 Multipass recovery lane are the canonical lanes);
- [x] pass predicate requires independent receiver decrypt/decode
      evidence (the Plan 062 v3 observation schema with the exact
      correlation fields);
- [x] evidence bundle retains all four sanitized direction records
      (the Plan 052 evidence bundle and the Plan 053 pipeline
      integration);
- [x] all provenance is exact and nonzero (the Plan 062 v4 trigger
      schema, the Plan 062 reference-event v1 schema, the Plan 062
      v3 observation schema, and the strict scenario schema v2);
- [x] full local/static validation passes;
- [x] `plans/065-status.md` records the Plan 065 implementation
      floor and the Plan 065 closure contract;
- [x] NTCP2 remains experimental and non-advertised.

## Remaining work

- Plan 066 is the fresh candidate and authoritative NTCP2 two-run
  closure pass. Plan 066 starts only after Plan 065 closes with one
  complete independently verified four-direction live diagnostic
  bundle. Plan 066 cannot start under the current four-direction
  contract until either a future pinned Java revision is adopted
  or the closure contract is revised through a new ADR (because
  ADR 0021 is Rejected by Plan 058). The Plan 046 rootless
  sealed-namespace lane returns
  `blocked_unprivileged_user_namespace` on this host; the Plan
  048/049 Multipass recovery lane is the canonical external path
  but cannot complete on this constrained host (per Plan 051).
  Plan 066 therefore closes on this host with the typed
  environment blocker `blocked_execution_lane_unavailable`.

NTCP2 remains experimental and non-advertised; Milestone 3 stays
open until Plan 066 produces a verified Milestone 3 certificate.
The Plan 065 implementation surface (the v2 strict scenario
schema, the per-run DeliveryStatus counters, the bounded Plan 065
typed failure categories, the canonical mixed-runner
`_plan065_primary_fields` and `_reference_driver_mode_for`
helpers, the Plan 065 test matrix, and the static boundary
checker extensions) is the mandatory prerequisite for any change
that would re-enable Plan 065 as active execution authority.

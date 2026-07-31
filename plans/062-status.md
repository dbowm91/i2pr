# Plan 062 closure record: NTCP2 evidence-contract and architecture correction

## Status

Plan 062 closes locally with all required artifacts and tests in
place. The plan does not perform an authoritative external
interoperability run and does not advance Milestone 3. NTCP2
remains experimental and non-advertised; the four-direction
Milestone 3 contract is restored only by the future Plan 065
implementation floor and Plan 066 fresh candidate and two-run
certificate.

Plan 062 was implemented across two commits on `main`:

```text
implementation commit (Plan 062 schema/ADR/tests):
    5f4fa18024ed9c9b9ba7c95b2002c713f9dcd14e
```

The implementation commit lands the Plan 062 source-verification
record, ADR 0022, the v4 trigger schema, the reference-event v1
schema, the v3 observation schema, and the Plan 062 test matrix.
The closure-record commit is the commit that lands this file and
the supporting documentation updates. The executed source commit
and the closure-record commit are recorded separately; a
documentation successor commit is never substituted for the
executed binary source.

## Phase 1: prerequisites

Plan 062 executed only after Plan 058 and Plan 059 closed. Both
plans produced their implementation surface, test matrices,
typed blockers, and candidate/closure records before Plan 062
started.

| Prerequisite | Status |
| --- | --- |
| Plan 056 candidate retired | Yes |
| Plan 057 superseded | Yes |
| ADR 0021 explicit decision | Yes (Rejected by Plan 058) |
| Plan 058 candidate record validator present | Yes |
| Plan 058 test matrix present | Yes |
| Plan 059 implementation surface | Yes |
| Plan 059 typed blocker marker | Yes (`blocked_java_support_topology_rejected`) |
| Plan 060 implementation surface (audit) | Yes |

## Phase 2: workstream corrections

Plan 062 executed the five work packages WP1 through WP5 in
order. The deliverables are recorded below.

### WP1: source-verification record

`tests/integration/ntcp2/reference-drivers/source-verification.md`
records the source-locked API surface for the pinned Java I2P
2.12.0 revision
(`2800040deee9bb376567b671ef2e9c34cf3e30b6`) and the pinned i2pd
2.60.0 revision (`f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`).
The document records the exact API surface for the future Plan
063 Java driver and the Plan 064 i2pd driver:

- the Java `Router` and `RouterContext` lifecycle;
- the dummy client/NetDB/peer-manager/tunnel-manager facades;
- the `NTCPTransport` listener/dial/connection surface;
- the `OutNetMessage`/`OutNetMessagePool` outbound path;
- the `DeliveryStatusMessage` codec;
- the i2pd pinned initialization order;
- the i2pd `Transports::SendMessage` semantics;
- the i2pd `CreateDeliveryStatusMsg` helper;
- the i2pd NTCP2 static-key/IV accessors;
- the i2pd passive observer seam.

No current-upstream-only API is assumed. Discrepancies between
the pinned source and current upstream examples are documented.

### WP2: ADR 0022

`docs/adr/0022-direct-reference-router-ntcp2-interop-drivers.md`
is `Accepted` and explicitly supersedes the conclusion of
ADR 0021 without rewriting ADR 0021. ADR 0022 records:

- the two-process direct transport driver architecture;
- the Java stripped-router design using the upstream
  `SSUDemo`-pattern and dummy facades;
- the i2pd direct driver with a compile-time-gated passive
  observer after successful AEAD verification and I2NP
  conversion;
- the two-process primary topology with no support router,
  floodfill, reseed, SAM, I2CP, HTTP/I2PControl, or tunnel pool;
- the rejected alternatives (SAM trigger, HTTP trigger, generic
  log parsing, full private I2P mini-network, cryptographic
  patches, future-upstream-version dependency).

### WP3: trigger schema v4 and reference-event schema v1

`tests/integration/ntcp2/harness/reference_trigger_v4.py`
implements the Plan 062 v4 trigger schema
(`i2pr-reference-trigger-v4`). The schema:

- uses 64-lowercase-hex Router Hash for both local (sender) and
  peer (target) sides, replacing the Plan 055 v3 40-lowercase-hex
  width;
- mandates the per-run DeliveryStatus `message_id` in
  `1..=0xffffffff`;
- binds helper, source, build manifest, observer patch, source
  inspection record, and run identity digests;
- rejects attempted records with all-zero provenance digests;
- rejects `attempt_count != 1` for attempted records;
- rejects unknown and missing fields;
- rejects v3 trigger records for new bundles;
- preserves the v3 module as the bounded historical-reader path.

`tests/integration/ntcp2/harness/reference_event.py`
implements the Plan 062 reference-event v1 schema
(`i2pr-reference-event-v1`). The schema records per-driver
structured events (`process_started`, `listener_ready`,
`router_info_exported`, `peer_router_info_validated`,
`tcp_connected`, `ntcp2_authenticated`, `frame_emitted`,
`frame_authenticated_and_decrypted`, `i2np_message_decoded`,
`terminal_clean`, `terminal_rejected`) with strict per-process
sequence ordering, exact DeliveryStatus message ID correlation
for data-phase events, and continuous Router Hash binding.

### WP4: observation v3 schema migration

`tests/integration/ntcp2/harness/observation_v3.py` implements
the Plan 062 v3 observation schema
(`i2pr-ntcp2-direction-observation-v3`). The schema:

- adds the mandatory correlation fields
  `delivery_status_message_id`, `peer_router_hash_sha256`,
  `local_router_hash_sha256`, and `source_event_sha256`;
- requires nonzero decrypt and decode counts on the receiver
  pass predicate;
- rejects generic-phrase-only sources;
- rejects sender-only observations used as receiver proof;
- rejects correlation mismatch between trigger, sender,
  receiver, and scenario;
- rejects Router Hash mismatch among RouterInfo, trigger,
  events, and direction record;
- preserves the v2 module as the bounded historical-reader
  path.

`tests/integration/ntcp2/harness/evidence_bundle.py` allowlists
the v4 trigger schema, the v3 observation schema, and the
reference-event v1 schema in the `SEMANTIC_SCHEMAS` map.
`tests/integration/ntcp2/harness/plan052_pipeline.py` consumes
v4 trigger records (and v3 trigger records for the bounded
historical-reader path).
`tests/integration/ntcp2/harness/verify_milestone3_certificate.py`
treats the `events` class as optional and never requires it for
the four mandatory direction classes.

### WP5: retire Plan 060 authority

The Plan 060 candidate (`plans/060-candidate.md`) is now
`retired by Plan 062`. The Plan 060 closure record
(`plans/060-closure.md`) carries the explicit "Superseded by
Plan 062" marker. The Plan 060 plan-of-record
(`plans/060-fresh-candidate-and-two-run-milestone3-certificate-closure-pass.md`)
records the Plan 062 supersession in its `Status and
dependencies` section. The historical Plan 060 typed blocker
(`blocked_execution_lane_unavailable`) and the historical
candidate status (`declared-not-executable`) are preserved as an
audit record. Future candidates must descend from the Plan 065
implementation floor or later. v3 trigger records and v2
observation records remain readable for historical inspection but
cannot contribute to a new passing bundle.

## Phase 3: required tests

The Plan 062 test matrix consists of:

- `tests/integration/ntcp2/harness/test_reference_trigger_v4.py`
  — 27 cases across v4 trigger schema validation, builder,
  attempt-count contract, helper-kind classifier, and historical
  v3 rejection.
- `tests/integration/ntcp2/harness/test_reference_event.py` —
  13 cases across reference-event v1 schema validation,
  data-phase event fields, duplicate sequence rejection,
  peer Router Hash mismatch rejection, forbidden payload text
  rejection, and terminal-event data-phase field rejection.
- `tests/integration/ntcp2/harness/test_observation_v3.py` —
  25 cases across v3 observation schema validation, v2 rejection,
  mandatory correlation fields, receiver pass predicate,
  generic-phrase rejection, sender-only and wrong-message
  fixtures, and v2 compatibility.
- `tests/integration/ntcp2/harness/test_plan062.py` — 35 cases
  across WP1-WP5 surface tests covering the source-verification
  record, ADR 0022 Accepted status, schema migration, Plan 060
  retirement, the Plan 061-066 roadmap chain, and the
  non-goals guard.

The Plan 062 closure criteria checklist enumerates the 22
acceptance tests from the plan. All positive and negative
fixtures pass:

1. valid 64-hex Router Hash accepted;
2. 40-hex Router Hash rejected;
3. 63/65-hex values rejected;
4. uppercase hex rejected;
5. all-zero attempted provenance rejected;
6. message ID zero rejected;
7. message ID greater than `0xffffffff` rejected;
8. missing message ID rejected;
9. wrong message ID across records rejected;
10. wrong peer Router Hash rejected;
11. trigger v3 rejected for a new bundle;
12. valid historical v3 readable but non-promotable;
13. unknown v4 field rejected;
14. missing v4 field rejected;
15. duplicate event sequence rejected;
16. stale event rejected;
17. generic phrase-only receiver observation rejected;
18. sender event used as receiver proof rejected;
19. handshake-only record rejected for data-phase pass;
20. valid complete v4 fixture accepted;
21. Plan 060 candidate rejected as retired;
22. future candidate below Plan 065 floor rejected.

## Phase 4: static checks

`scripts/check-ntcp2-interoperability.sh` is extended to enforce:

- the Plan 062 v4 trigger schema module exists and carries the
  correct schema/version marker;
- the Plan 062 reference-event v1 schema module exists and
  carries the correct schema/version marker;
- the Plan 062 v3 observation schema module exists and carries
  the correct schema/version marker;
- the Plan 062 source-verification record exists;
- ADR 0022 exists and carries `Accepted` status;
- the Plan 062 plan-of-record and the Plan 063-066
  plan-of-records exist;
- the Plan 060 candidate is retired by Plan 062;
- the Plan 060 closure record carries the "Superseded by
  Plan 062" marker;
- the active v4 trigger schema, v3 observation schema, and
  reference-event v1 schema do not use the 40-hex Router Hash
  width.

The static check does not reject unrelated legitimate SHA-1
uses; the scope is limited to the active v4 trigger, v3
observation, and reference-event schema files.

## Phase 5: documentation updates

The following documentation files record the Plan 062
correction:

- `README.md` records the Plan 060 retirement and the Plan 062
  evidence-contract correction. Java direct driver is now the
  planned path; no support topology is required for the primary
  four directions; no live qualification has occurred yet; Plan
  060 candidate is retired; NTCP2 remains
  experimental/non-advertised.
- `AGENTS.md` adds the Plan 062 workstream summary and the Plan
  062 focused checks (test_reference_trigger_v4,
  test_reference_event, test_observation_v3, test_plan062,
  test_evidence_bundle, test_plan056, test_plan060, plus the
  three boundary check scripts).
- `docs/architecture/interop-apparatus.md` records the Plan 062
  evidence-contract correction in the architecture
  documentation.
- `docs/protocol-support.md` records the Plan 062 supersession
  of the Plan 060 execution authority.
- `plans/030-milestone-3-closure.md` is updated to record the
  Plan 062 supersession in the aggregate Milestone 3 status.
- `.opencode/skills/i2pr-ntcp2-interop/SKILL.md` records the
  Plan 062 workstream summary, the v4 trigger schema, the
  reference-event v1 schema, the v3 observation schema, and
  the Plan 060 retirement.

## Phase 6: validation commands and results

All Plan 062 closure checks passed locally at the Plan 062
implementation commit. The historical Plan 060 typed blocker
(`blocked_execution_lane_unavailable`) and the historical Plan
060 candidate status (`declared-not-executable`) remain
recorded as the canonical Plan 046 rootless sealed-namespace
and Plan 048/049 Multipass recovery lane outcomes on this host.

```text
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan062.py'
# Ran 35 tests in 0.003s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_trigger_v4.py'
# Ran 27 tests in 0.002s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
# Ran 13 tests in 0.001s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_observation_v3.py'
# Ran 25 tests in 0.001s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan056.py'
# Ran 18 tests in 0.971s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_plan060.py'
# Ran 35 tests — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_evidence_bundle.py'
# Ran 64 tests in 7.185s — OK

python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py'
# Ran 592 tests in 18.514s — OK (skipped=2)

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

RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
# passes
```

## Closure criteria checklist

The Plan 062 closure criteria are met:

- [x] source-verification record is complete for both pinned
      references;
- [x] ADR 0022 is Accepted and supersedes ADR 0021's
      conclusion;
- [x] active Router Hash validation is 64-hex SHA-256
      (`reference_trigger_v4.py`, `observation_v3.py`,
      `reference_event.py`);
- [x] trigger schema v4 and reference-event schema v1 are
      implemented;
- [x] exact DeliveryStatus message ID is mandatory
      (`reference_trigger_v4.py`);
- [x] observation/scenario contracts are migrated
      (`observation_v3.py`, `evidence_bundle.py`,
      `plan052_pipeline.py`, `verify_milestone3_certificate.py`);
- [x] Plan 060 candidate is retired;
- [x] all required positive and negative tests pass
      (592 tests including the 100 Plan 062 cases);
- [x] static checks reject reintroduction of legacy authority
      (`scripts/check-ntcp2-interoperability.sh`);
- [x] full applicable local validation passes;
- [x] `plans/062-status.md` records exact commits and makes no
      external pass claim.

## Remaining work

- Plan 063 implements the Java I2P stripped-router direct NTCP2
  driver. Plan 063 starts only after Plan 062 closes with ADR
  0022 Accepted and the v4 trigger / reference-event v1 schemas
  frozen for this implementation cycle.
- Plan 064 implements the i2pd direct NTCP2 driver with a
  compile-time-gated passive observer. Plan 064 may execute in
  parallel with Plan 063.
- Plan 065 is the canonical integration and live qualification
  pass. Plan 065 starts only after Plans 063 and 064 close and
  their exact drivers are buildable from pinned source.
- Plan 066 is the fresh candidate and authoritative NTCP2
  two-run closure pass. Plan 066 starts only after Plan 065
  closes with one complete independently verified four-direction
  live diagnostic bundle.

NTCP2 remains experimental and non-advertised; Milestone 3
stays open until Plan 065 closes with one complete four-direction
live diagnostic bundle and Plan 066 produces a verified
Milestone 3 certificate. The Plan 060 implementation surface
(`plan060.py`, `test_plan060.py`, the candidate record, the
closure record) is preserved as an audit record and remains
mandatory for any change that would re-enable Plan 060 as active
execution authority.
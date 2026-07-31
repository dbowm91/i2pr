# Plan 075: Plan 069 runner integrity and evidence correction

## Status and dependencies

- Status: planned.
- Parent roadmap: Plan 074.
- Must close before Plan 076 is used by any live runner and before Plan 078.
- Plan type: test-harness integrity correction.
- Supersedes the execution claims implied by Plan 069 status. Plan 069 remains historical evidence that orchestration scaffolding and fake-process tests were implemented.

## Objective

Make the Level 1 loopback runner structurally incapable of producing a mixed-router pass unless it launches one real i2pr process and one configured real reference process and consumes authentic structured events from both.

## Confirmed defects to correct

1. `_launch_listener()` and `_launch_dialer()` currently select `target/debug/i2pr-interop` for both process roles.
2. The supplied `reference_driver_binary` is validated but not used as the real second process.
3. `_monitor_protocol()` promotes `ntcp2_authenticated`, `frame_emitted`, `frame_authenticated_and_decrypted`, and `i2np_message_decoded` after a loopback-listener probe instead of consuming protocol events.
4. Reference source-tree, driver-source, and observer-patch provenance may be derived from synthetic run strings rather than measured files.
5. The runner can describe a two-process mixed-router lane while actually composing two i2pr launchers.
6. Event records are not yet required to prove process-role identity and binary provenance.

## Deliverables

### D1. Fail-closed temporary guard

Before broader refactoring, add a guard that prevents any passing record when:

- both process commands resolve to the same i2pr binary;
- the configured reference binary was not executed;
- no reference event stream exists;
- any required provenance field is synthetic, missing, zero, or not measured;
- a protocol milestone lacks a corresponding validated structured event.

The guard should cause a typed failure such as:

```text
runner-reference-process-not-executed
runner-reference-events-missing
runner-synthetic-provenance-rejected
runner-protocol-event-unproven
```

### D2. Explicit process-role model

Replace ambiguous listener/dialer ownership with:

```text
ProcessRole.I2PR
ProcessRole.REFERENCE
TransportRole.LISTENER
TransportRole.DIALER
```

For each direction:

```text
i2pr-to-i2pd-ipv4:
  i2pr = dialer
  reference = listener

i2pd-to-i2pr-ipv4:
  reference = dialer
  i2pr = listener
```

The command builder must be independently testable and must include the configured reference driver path for the reference role.

### D3. Real reference invocation

Invoke the reference through its committed runtime seam:

```text
tests/integration/ntcp2/reference-drivers/i2pd/run-driver.sh
```

with:

```text
--driver-binary <measured configured binary>
--strict-config <owned config path>
```

The runner must not substitute a fake binary outside unit tests. Test injection must use an explicit fake process factory or fixture adapter that cannot be selected by the production CLI.

### D4. Event-source validation

Consume separate structured streams for i2pr and reference.

Every accepted event must bind:

```text
run_id
scenario_id
direction
implementation
implementation_revision
driver_binary_sha256
local_router_hash_sha256
peer_router_hash_sha256
event_kind
event_sequence
monotonic_ms
```

Data events additionally bind:

```text
delivery_status_message_id
i2np_type
frame_sequence
```

Reject:

- wrong binary digest;
- wrong implementation name;
- wrong run/direction;
- stale event before process start cursor;
- duplicate or nonmonotonic sequence;
- sender event used as receiver evidence;
- event from a fake fixture in a real CLI run;
- generic log phrase used as structured authority.

### D5. Milestone derivation

Derive milestones only from valid events:

```text
tcp_connected:
  both process role and peer identity are consistent
ntcp2_authenticated:
  required authenticated event observed on both sides
frame_emitted:
  sender successful frame-write event with exact ID
frame_authenticated_and_decrypted:
  receiver post-AEAD event with exact peer
 i2np_message_decoded:
  receiver exact DeliveryStatus decode with exact ID
```

Do not call `protocol.mark()` directly from socket probes except for a separate non-authoritative `tcp_listener_observed` diagnostic.

### D6. Provenance correction

Measured provenance must come from:

- source lock JSON;
- verified pinned source-tree digest;
- driver source digest;
- observer patch digest;
- executable digest;
- build manifest digest.

Remove every fallback that hashes a run string to fabricate a schema-valid digest. Missing provenance is a build/preflight blocker.

### D7. Smoke-record semantics

A passing Level 1 record requires:

- real reference process executed;
- reference process binary digest equals build manifest;
- authentic event continuity;
- exact message and identity correlation;
- clean teardown;
- recorded execution lane and audit level.

Add fields or a compatible next schema only if the current Plan 068 schema cannot express:

```text
reference_process_executed = true
reference_events_validated = true
provenance_measured = true
execution_lane
```

Do not add a schema version merely to reorganize code.

## Work packages

### WP1. Add fail-closed regression tests

Create tests reproducing each confirmed defect before changing orchestration.

Required cases:

1. two i2pr commands cannot pass;
2. configured reference never launched cannot pass;
3. listener probe alone cannot prove authentication;
4. automatic milestone promotion rejected;
5. synthetic provenance rejected;
6. missing reference event stream rejected;
7. wrong reference digest rejected.

### WP2. Correct command construction

- implement explicit role mapping;
- invoke real reference runtime seam;
- preserve one direction per run;
- retain process-group ownership and deadlines.

### WP3. Correct event ingestion

- separate streams;
- strict validation;
- event-driven milestones;
- earliest failure stage retained.

### WP4. Correct provenance and records

- remove fallback hashes;
- bind manifests and source lock;
- update record validation and fixtures;
- mark old Plan 069 fixtures synthetic/non-promotable where necessary.

### WP5. Documentation and closure

Update relevant NTCP2 README/architecture/skill guidance and create `plans/075-status.md`.

The status must say whether a real reference binary was available, but it must not claim interoperability. Plan 076 owns constructing the real binary.

## Acceptance criteria

Plan 075 closes only when:

- production CLI command construction launches one i2pr and one configured reference process;
- both direction mappings are correct;
- no protocol milestone is inferred from a port probe;
- authentic structured events are mandatory;
- measured provenance is mandatory;
- synthetic digests are absent from active runner code;
- fake process tests remain possible through explicit test injection only;
- existing Plan 069 smoke records or fixtures cannot be promoted to valid mixed-router passes unless they satisfy the corrected contract;
- focused tests and static boundary checks pass;
- no real interoperability claim is made.

## Validation commands

```bash
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_loopback_smoke_record.py'
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_reference_event.py'
bash scripts/check-ntcp2-loopback-smoke-boundary.sh
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
git diff --check
```

## Non-goals

Plan 075 does not:

- build or fix i2pd;
- run a real mixed-router direction;
- add Docker, QEMU, namespaces, or CI;
- change NTCP2 protocol code;
- produce a Level 2 or Level 3 record.

## Stop rules

Stop and record a typed blocker when:

- the reference driver interface cannot expose separate event and RouterInfo files without protocol changes;
- the runner cannot distinguish process identity from event metadata;
- the current smoke schema would require silently reinterpreting historical records.

In the last case, add a new smoke schema version with an explicit historical reader; never silently upgrade old records.

## Small-model execution guidance

- Add failing tests first.
- Correct command selection before touching event parsing.
- Correct event parsing before record fields.
- Do not implement i2pd internals in this plan.
- Do not preserve a compatibility path that can still synthesize success.
- Commit orchestration, event-validation, and record changes separately.
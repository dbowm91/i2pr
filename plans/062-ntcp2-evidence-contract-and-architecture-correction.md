# Plan 062: NTCP2 evidence-contract and architecture correction

## Status and dependencies

- Status: planned.
- Parent roadmap: Plan 061.
- Must execute before Plan 063, Plan 064, Plan 065, or Plan 066.
- Plan type: architecture decision, evidence-schema correction, candidate retirement, and fail-closed contract migration.
- This plan changes no production NTCP2 behavior and performs no authoritative external interoperability run.

## Objective

Correct the repository's mixed-router architecture and evidence contract before new reference drivers are implemented.

The plan must:

1. supersede the rejected Java-support-topology premise with a source-locked direct Java stripped-router driver decision;
2. correct Router Hash representation from 40 hexadecimal characters to 64 lowercase hexadecimal characters;
3. introduce trigger schema v4 and observation/event correlation fields;
4. make one nonzero per-run DeliveryStatus message ID mandatory;
5. require exact sender/receiver Router Hash and message-ID continuity;
6. retire the Plan 060 candidate and mark Plan 060 as superseded for future execution authority;
7. ensure no legacy v3 trigger or generic log observation can satisfy a primary direction;
8. establish one shared contract consumed by the Java, i2pd, i2pr, runner, and verifier work in later plans.

## Existing defects to correct

### Router Hash width

The current trigger validator uses a 40-lowercase-hex pattern for `target_router_hash`. This is incorrect. I2P Router Hash is the SHA-256 digest of RouterIdentity and is 32 bytes. Hex representation must therefore be exactly 64 lowercase hexadecimal characters.

Any current helper that uses `SHA_DIGEST_LENGTH`, SHA-1-sized buffers, 40-character validation, or a field named ambiguously as a generic `hash` must be migrated.

### Trigger schema v3 ambiguity

The current schema can record a requested or connected trigger without binding the exact I2NP test message. It also allows field semantics that were designed around the old support-topology decision.

Schema v4 must distinguish:

- trigger request;
- transport connection establishment;
- sender frame-write completion;
- receiver frame authentication/decryption;
- receiver exact I2NP decode.

A trigger record is not a receiver observation.

### DeliveryStatus correlation weakness

The current i2pr launcher uses a fixed message ID and receiver logic accepts any DeliveryStatus type. This permits an unrelated DeliveryStatus message to satisfy the data-phase predicate.

Every direction/run must receive a unique nonzero `delivery_status_message_id`. The sender and receiver must both report the same value. Duplicate or stale values must fail.

### Plan 060 execution authority

Plan 060 is a historical declared-not-executable candidate/closure attempt built on a rejected Java support-topology premise and pre-correction reference helpers. It must not be reused by editing its source SHA or receipts in place.

Plan 062 must:

- preserve Plan 060 documents as history;
- add an explicit supersession notice;
- retire the Plan 060 candidate from all future candidate validators;
- require Plan 066 to cut a new candidate descended from the completed Plan 065 implementation floor.

## Deliverables

### D1. Architecture decision record

Create:

```text
docs/adr/0022-direct-reference-router-ntcp2-interop-drivers.md
```

The ADR must record:

- status: Accepted only after the source-verification checklist below passes;
- context: SAM/I2CP/tunnel APIs are the wrong layer for router-transport qualification;
- decision: use two-process direct transport drivers for Java I2P and i2pd;
- Java design: stripped real router using the upstream `SSUDemo` pattern, dummy client/NetDB/peer/tunnel facades, direct RouterInfo import, real `OutNetMessage`;
- i2pd design: initialized pinned transport stack, direct RouterInfo import, real DeliveryStatus message, test-only observer;
- no support router, floodfill, reseed, SAM, I2CP, or tunnel pool in the primary topology;
- observer constraints: passive, compile-time-gated, no behavior-changing return path;
- consequences: reference driver binaries/patches become candidate-bound test artifacts;
- rejected alternatives: SAM trigger, HTTP/I2PControl trigger, generic log parsing, full private I2P mini-network, cryptographic patches, future-upstream-version dependency.

The ADR must explicitly supersede the conclusion of ADR 0021 without rewriting ADR 0021.

### D2. Source-verification record

Create a source-verification document under:

```text
tests/integration/ntcp2/reference-drivers/source-verification.md
```

Before accepting ADR 0022, inspect the exact pinned source revisions and record:

#### Java I2P 2.12.0

- exact source revision and archive digest from the existing reference lock;
- location of `SSUDemo` or the equivalent stripped-router test;
- property names for disabling UDP, UPnP, I2CP/client facade, active NetDB, peer manager, and tunnel manager;
- method used to import RouterInfo into the dummy NetDB;
- method used to submit `OutNetMessage`;
- receive-handler registration API for DeliveryStatus;
- embedded-router startup and shutdown APIs;
- NTCP2 listener/readiness and RouterInfo production path.

#### i2pd 2.60.0

- exact source revision and archive digest from the existing reference lock;
- configuration/filesystem/crypto/context/NetDB/transport initialization sequence;
- direct RouterInfo insertion API;
- `CreateDeliveryStatusMsg` or equivalent;
- `Transports::SendMessage`/`SendMessages` semantics for a newly connecting peer;
- connected-session query path;
- NTCP2 receive boundary after AEAD verification and `FromNTCP2` conversion;
- sender boundary after successful asynchronous frame write;
- NTCP2 address accessors for `s`, `i`, host, port, and version.

If an exact pinned API differs from current upstream examples, the pinned source wins and the ADR/plan implementation notes must be updated before code starts.

### D3. Trigger schema v4

Replace v3 authority with:

```text
schema = i2pr-reference-trigger-v4
schema_version = 4
```

Recommended implementation path:

```text
tests/integration/ntcp2/harness/reference_trigger_v4.py
```

or a clearly versioned replacement of the current module. Do not silently reinterpret existing v3 records.

Required fields:

```text
schema
schema_version
run_id
scenario_id
direction
reference
reference_version
reference_revision
helper_kind
helper_binary_sha256
helper_source_sha256
helper_build_manifest_sha256
helper_pinned_inputs_sha256
source_inspection_record_sha256
observer_patch_sha256
run_identity_sha256
local_router_hash_sha256
peer_router_hash_sha256
local_router_info_sha256
peer_router_info_sha256
peer_ntcp2_static_key_sha256
peer_ntcp2_iv_sha256
target_address
target_port
delivery_status_message_id
attempted
attempt_count
transport_request_observed
connection_established_observed
sender_frame_write_observed
started_monotonic_ms
completed_monotonic_ms
outcome
reason_code
sanitized_detail
trigger_sha256
```

Rules:

- both Router Hash fields are exactly 64 lowercase hex;
- every SHA-256 field is exactly 64 lowercase hex;
- attempted records may not contain all-zero provenance digests;
- `delivery_status_message_id` is integer `1..=0xffffffff`;
- `attempt_count` is exactly 1 for an attempted one-shot driver;
- `completed_monotonic_ms >= started_monotonic_ms`;
- target address is one of the declared synthetic scenario addresses;
- unknown fields fail;
- missing fields fail;
- v3 is accepted only by an explicit historical-reader path and can never contribute to a new passing bundle.

### D4. Structured reference-event schema

Create a common schema module, recommended:

```text
tests/integration/ntcp2/harness/reference_event.py
```

Schema:

```text
schema = i2pr-reference-event-v1
schema_version = 1
```

Allowed event kinds:

```text
process_started
listener_ready
router_info_exported
peer_router_info_validated
tcp_connected
ntcp2_authenticated
frame_emitted
frame_authenticated_and_decrypted
i2np_message_decoded
terminal_clean
terminal_rejected
```

Required common fields:

```text
run_id
scenario_id
direction
implementation
implementation_revision
driver_binary_sha256
local_router_hash_sha256
peer_router_hash_sha256
monotonic_ms
event_kind
event_sequence
event_sha256
```

Data-phase events additionally require:

```text
delivery_status_message_id
i2np_type
frame_sequence
```

Rules:

- event sequence is strictly increasing per process;
- duplicate event sequence fails;
- data-phase events with no exact message ID fail;
- events from before the run cursor fail;
- events with a peer Router Hash different from the run identity fail;
- events may contain no raw RouterInfo, I2NP payload, keys, transcript, absolute secret-bearing path, or remote arbitrary text.

### D5. Observation schema migration

Update the observation builder/validator so primary directions require independent structured events.

Required levels remain:

```text
ntcp2_authenticated
frame_emitted
frame_authenticated_and_decrypted
i2np_message_decoded
```

Additional required correlation fields:

```text
delivery_status_message_id
peer_router_hash_sha256
local_router_hash_sha256
source_event_sha256
```

The observation validator must reject:

- a receiver `i2np_message_decoded=observed` with no exact message ID;
- a receiver event sourced only from a generic phrase catalog;
- a sender-only observation used as receiver proof;
- a Java event from the wrong handler/source peer;
- an i2pd event emitted before successful AEAD and `FromNTCP2` conversion;
- a handshake-only run represented as data phase;
- message ID mismatch between trigger, sender, receiver, and scenario;
- Router Hash mismatch among RouterInfo, trigger, events, and direction record.

### D6. Scenario contract change

Add these mandatory fields to primary live scenarios:

```text
delivery_status_message_id
expected_sender_router_hash_sha256
expected_receiver_router_hash_sha256
reference_driver_mode
```

Allowed `reference_driver_mode` values:

```text
java-direct-listen
java-direct-dial
i2pd-direct-listen
i2pd-direct-dial
```

Remove SAM and HTTP/I2PControl trigger modes from primary direction authority. They may remain as historical or unrelated reference controls only if clearly non-authoritative.

### D7. Candidate and supersession records

Create:

```text
plans/062-status.md
```

at closure, containing:

- exact implementation commit;
- ADR 0022 decision;
- schema migration summary;
- Plan 060 candidate status `retired`;
- Plan 061 active roadmap reference;
- next plans 063 and 064;
- validation commands and results;
- no external interoperability claim.

Update candidate validators so:

- Plan 056 candidate remains retired;
- Plan 060 candidate is retired;
- Plan 057 and Plan 060 cannot be active execution authority;
- future candidate implementation floor must be Plan 065 closure or later;
- v3 trigger/legacy observation artifacts cannot satisfy a Plan 066 freeze checklist.

## Work packages

### WP1: inspect and document pinned source

1. Read the existing reference lock and build cache metadata.
2. Verify both exact pinned source trees.
3. Write `source-verification.md` with file paths, symbols, signatures, and source digests.
4. Stop if the direct paths cannot be verified.

Acceptance:

- every API used by Plans 063/064 has a pinned-source location;
- no current-upstream-only API is assumed;
- discrepancies are documented.

### WP2: land ADR 0022

1. Draft ADR from the verified source record.
2. Mark ADR Accepted only when WP1 passes.
3. Add a supersession reference to ADR 0021 and relevant architecture docs.
4. Do not delete ADR 0021.

Acceptance:

- two-process direct topology is the sole primary plan;
- support router is explicitly out of scope;
- observer restrictions are explicit.

### WP3: implement schema v4 and event schema

1. Add v4 data types, canonical JSON serialization, digest finalization, and validators.
2. Keep a read-only historical v3 parser when needed for old records.
3. Add exact Router Hash and message-ID types/helpers.
4. Add reference-event types and validators.
5. Add strict unknown-field rejection.

Acceptance:

- no 40-hex Router Hash validation remains in active schemas;
- all attempted records require nonzero digests;
- exact message ID is mandatory.

### WP4: migrate observations and scenarios

1. Add mandatory correlation fields.
2. Change primary scenario renderers and fixtures.
3. Remove generic phrase-only success from the pass predicate.
4. Add typed migration errors for legacy records.

Acceptance:

- legacy fixture cannot pass a primary direction;
- sender-only fixture cannot pass receiver predicate;
- wrong-message fixture fails.

### WP5: retire Plan 060 authority

1. Add supersession wording to aggregate Milestone 3 status and protocol-support documentation.
2. Retire Plan 060 candidate in validators and fixtures.
3. Make Plan 061 the active roadmap.
4. Add Plan 062 status/closure record only after all local checks pass.

Acceptance:

- no script proposes running the Plan 060 candidate as authoritative;
- future freeze points to Plan 065 implementation floor.

## Required tests

Add or update tests covering at least:

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

Recommended focused test files:

```text
tests/integration/ntcp2/harness/test_reference_trigger_v4.py
tests/integration/ntcp2/harness/test_reference_event.py
tests/integration/ntcp2/harness/test_observation_v3.py
tests/integration/ntcp2/harness/test_plan062.py
```

Use the next observation schema number consistent with the repository; do not overload an existing version with new semantics.

## Static checks

Extend `scripts/check-ntcp2-interoperability.sh` to fail when:

- active code contains `_HEX40` for Router Hash;
- active helpers use SHA-1 length for Router Hash;
- active primary scenario renderers reference SAM or HTTP triggers;
- v4 schema files are absent;
- DeliveryStatus message ID is optional in a primary scenario;
- Plan 060 candidate is not retired;
- ADR 0022 is absent or not Accepted after source verification;
- Plan 061/062 documentation is absent.

Do not use brittle checks that reject unrelated legitimate SHA-1 uses. Scope checks to Router Hash schema/helper code.

## Documentation updates

Update at minimum:

```text
README.md
docs/architecture/interop-apparatus.md
docs/protocol-support.md
plans/030-milestone-3-closure.md
.opencode/skills/i2pr-ntcp2-interop/SKILL.md
```

Required wording:

- Java direct driver is now the planned path;
- no support topology is required for the primary four directions;
- no live qualification has occurred yet;
- Plan 060 candidate is retired;
- NTCP2 remains experimental/non-advertised.

## Non-goals

Plan 062 does not:

- implement Java or i2pd drivers;
- modify i2pr NTCP2 cryptography or runtime transport behavior;
- run an external direction;
- create a new candidate;
- produce qualification receipts;
- advertise NTCP2;
- delete historical v3 evidence or prior plans.

## Stop rules

Stop and record a typed blocker when:

- pinned-source verification disproves the direct-driver architecture;
- an active schema consumer cannot migrate without silently changing historical records;
- exact Router Hash cannot be derived from both reference RouterInfos through pinned APIs;
- exact message ID cannot be represented consistently by Java, i2pd, and i2pr I2NP codecs;
- retiring Plan 060 would break an unrelated release/support contract that has not been explicitly reviewed.

Do not proceed to Plans 063/064 until the blocker is resolved through a revised plan/ADR.

## Closure criteria

Plan 062 closes only when:

- source-verification record is complete for both pinned references;
- ADR 0022 is Accepted and supersedes ADR 0021's conclusion;
- active Router Hash validation is 64-hex SHA-256;
- trigger schema v4 and reference-event schema v1 are implemented;
- exact DeliveryStatus message ID is mandatory;
- observation/scenario contracts are migrated;
- Plan 060 candidate is retired;
- all required positive and negative tests pass;
- static checks reject reintroduction of legacy authority;
- full applicable local validation passes;
- `plans/062-status.md` records exact commits and makes no external pass claim.

## Small-model handoff instructions

Execute work packages in order. Do not start Java or i2pd driver code in this plan.

For each changed schema:

1. locate every constructor, serializer, validator, fixture, and verifier reference;
2. change the type/field once;
3. run the focused test immediately;
4. update downstream fixtures before moving to the next field;
5. preserve a separate historical-reader path when old evidence must remain inspectable;
6. never make a field optional merely to reduce migration failures.

A closure commit is invalid if it contains placeholder digests, claims a live run, or leaves any active 40-hex Router Hash rule.
# Plan 065: NTCP2 canonical integration and live qualification

## Status and dependencies

- Status: planned.
- Parent roadmap: Plan 061.
- Starts only after Plans 062, 063, and 064 close.
- Requires committed `qualified=true` Java and i2pd direct-driver receipts produced from real sealed controls.
- Plan type: i2pr launcher correlation correction, canonical runner integration, two-process topology enforcement, full live diagnostic qualification, and implementation-floor closure.
- Does not freeze the final candidate or produce the authoritative two-run certificate; those belong to Plan 066.

## Objective

Integrate the corrected Java and i2pd direct drivers into the canonical four-direction mixed-router lane, correct i2pr's exact DeliveryStatus correlation, and produce one complete live four-direction diagnostic bundle from a clean implementation commit.

Plan 065 establishes the implementation floor from which Plan 066 may cut a candidate. It must prove the entire apparatus works once before candidate freeze.

## Required inputs

Plan 065 must consume exact committed artifacts from earlier plans:

### Plan 062

- ADR 0022 Accepted;
- trigger schema v4;
- reference-event schema v1;
- migrated observation schema;
- 64-hex Router Hash contract;
- mandatory DeliveryStatus message ID;
- Plan 060 candidate retired.

### Plan 063

- Java direct driver source/binary/classpath manifests;
- Java adapter;
- Java `qualified=true` 10/10 receipt;
- source-verification and handler semantics;
- Java-to-Java control bundle digest.

### Plan 064

- i2pd direct driver source/binary/link manifests;
- observer patch and uninstrumented control binary;
- i2pd adapter;
- i2pd `qualified=true` 10/10 receipt;
- behavior-neutrality control bundle digest.

If any input is missing, unqualified, has placeholder digests, or references a different pinned revision, stop.

## Primary work areas

1. i2pr scenario and launcher exact correlation;
2. structured event consumption;
3. direct-driver adapter integration;
4. primary two-process topology and run ordering;
5. pass-predicate correction;
6. evidence durability/provenance correction;
7. live four-direction diagnostic qualification;
8. implementation-floor and closure record.

## Workstream A: i2pr scenario contract

Update the strict launcher scenario schema and all primary renderers.

Required primary fields:

```text
schema
schema_version
scenario_id
run_id
direction
role
state_dir
status_path
local_address
local_port
peer_address
peer_port
peer_router_info
deterministic_seed
network_id
padding_profile
data_phase_mode
data_phase_timeout_ms
delivery_status_message_id
expected_sender_router_hash_sha256
expected_receiver_router_hash_sha256
reference_driver_mode
run_identity_sha256
```

Rules:

- `network_id = 99` for the primary sealed lane;
- `delivery_status_message_id` is required and nonzero;
- one unique ID per direction per run;
- `expected_sender_router_hash_sha256` and `expected_receiver_router_hash_sha256` are required 64-hex values once RouterInfos are generated;
- `reference_driver_mode` must match direction;
- primary live directions use only directional data mode, not round-trip echo assumptions;
- no SAM, HTTP, I2PControl, support topology, or synthetic fallback field is accepted for a primary direction.

### Message ID generation

Generate the DeliveryStatus ID deterministically from the run identity and direction through a domain-separated hash, then truncate to nonzero `u32`.

Example conceptual derivation:

```text
SHA256("i2pr-ntcp2-delivery-status-v1" || run_id || direction || correlation_nonce)
```

Use a single helper and add collision checks within a bundle. Do not use Python's process-randomized `hash()`.

## Workstream B: i2pr launcher send correction

The current launcher hard-codes one message ID. Replace it with scenario-owned correlation.

### Sender requirements

`send_i2np_block` or its replacement must:

1. accept expected message ID as an argument;
2. construct the DeliveryStatus I2NP envelope using that exact ID;
3. construct the DeliveryStatus body using the declared exact ID according to codec semantics;
4. use bounded current timestamp;
5. enqueue exactly one I2NP block;
6. emit structured sender status only after `send_blocks` succeeds;
7. record message ID, peer Router Hash, frame count, and I2NP count;
8. never print raw payload bytes or keys.

### Negative behavior

Fail when:

- ID is zero;
- message construction changes the declared ID;
- queue result is ambiguous;
- cancellation wins;
- frame send fails;
- more than one primary DeliveryStatus is emitted;
- sender Router Hash does not match run identity.

## Workstream C: i2pr launcher receive correction

The current receiver accepts any DeliveryStatus type. Correct it to require exact correlation.

`receive_delivery_status` or its replacement must verify:

1. authenticated link is installed;
2. one bounded frame is received before deadline;
3. frame authentication/decryption succeeds;
4. block parsing succeeds;
5. exactly one relevant I2NP block is present;
6. short-transport I2NP decoding succeeds;
7. message type is DeliveryStatus;
8. I2NP envelope message ID equals the scenario expectation where applicable;
9. DeliveryStatus payload message ID equals the scenario expectation;
10. timestamp is within the declared bounded compatibility window;
11. duplicate matching message is rejected;
12. peer Router Hash equals the expected sender identity.

Emit distinct typed failures:

```text
receiver_frame_read_failed
receiver_frame_authentication_failed
receiver_i2np_decode_failed
receiver_delivery_status_missing
receiver_delivery_status_id_mismatch
receiver_delivery_status_duplicate
receiver_peer_identity_mismatch
receiver_delivery_status_timestamp_invalid
```

Do not collapse all failures into a generic data-phase failure when a precise bounded category is available.

## Workstream D: reference adapter integration

### Java directions

#### `i2pr-to-java-ipv4`

- Java driver starts in `listen` mode.
- Java exports RouterInfo and readiness event.
- i2pr imports the exact Java RouterInfo and dials.
- i2pr sends exact DeliveryStatus.
- Java handler emits exact decrypt/decode observation.

#### `java-to-i2pr-ipv4`

- i2pr starts in `listen` mode and exports RouterInfo.
- Java driver imports exact i2pr RouterInfo in `dial` mode.
- Java sends exact DeliveryStatus.
- i2pr receives and verifies exact ID.

### i2pd directions

#### `i2pr-to-i2pd-ipv4`

- instrumented i2pd driver starts in `listen` mode.
- i2pd exports RouterInfo and readiness event.
- i2pr imports exact i2pd RouterInfo and dials.
- i2pr sends exact DeliveryStatus.
- i2pd observer emits exact post-AEAD/post-conversion observation.

#### `i2pd-to-i2pr-ipv4`

- i2pr starts in `listen` mode and exports RouterInfo.
- instrumented i2pd driver imports exact i2pr RouterInfo in `dial` mode.
- i2pd sends exact DeliveryStatus.
- i2pr receives and verifies exact ID.

### Adapter restrictions

- adapters consume structured events, not arbitrary log phrases;
- adapters do not generate protocol success themselves;
- adapters enforce one process, one attempt, one direction;
- adapters cannot fall back to old SAM/HTTP triggers;
- adapters fail if driver/receipt/provenance digests differ from the run identity;
- old helpers may remain only as historical controls and cannot be selected by primary manifest.

## Workstream E: canonical two-process topology

Update the canonical runner so each primary direction owns exactly:

- one i2pr process;
- one reference driver process;
- one rootless sealed network namespace or equivalent owned topology;
- one veth pair;
- two synthetic IPv4 addresses;
- no default route;
- empty/disabled DNS;
- no support router;
- no third router process;
- no public egress.

### Offline enforcement

Before either router starts:

1. verify no default route inside namespace;
2. verify no DNS configuration usable by child;
3. verify synthetic peer address is reachable;
4. verify a known public IP is unreachable;
5. verify namespace ownership and process cgroup/PID tracking;
6. record parent network pre-state.

After cleanup:

1. verify both processes exited;
2. verify listener ports closed;
3. verify namespace/veth removed;
4. verify no residual child PID;
5. verify secret-bearing run root removed;
6. record parent network post-state;
7. require pre/post equality for attributable routes/firewall/interfaces.

Guest-level firewall markers must not sever the host/guest control channel. Isolation authority belongs to the rootless execution namespace.

## Workstream F: pass predicate

Replace any weakened or ambiguous predicate with one shared function.

A direction passes only when:

```text
launcher_terminal_result == passed
reference_terminal_result == clean
trigger_v4_valid == true
sender_observation_valid == true
receiver_observation_valid == true
sender.ntcp2_authenticated == observed
receiver.ntcp2_authenticated == observed
sender.frame_emitted == observed
receiver.frame_authenticated_and_decrypted == observed
receiver.i2np_message_decoded == observed
scenario.delivery_status_message_id == trigger.delivery_status_message_id
scenario.delivery_status_message_id == sender.delivery_status_message_id
scenario.delivery_status_message_id == receiver.delivery_status_message_id
sender.peer_router_hash_sha256 == receiver.local_router_hash_sha256
receiver.peer_router_hash_sha256 == sender.local_router_hash_sha256
router_info_digests_match == true
run_identity_digests_match == true
sandbox_attestation_valid == true
parent_network_state_unchanged == true
cleanup_result == clean
synthetic_fallback_used == false
```

### Required negative predicates

Tests must prove that none of these pass:

- handshake + sender frame only;
- receiver generic type-10 phrase only;
- wrong message ID;
- wrong peer Router Hash;
- decrypt event without decode;
- decode event without source-proven decrypt;
- stale event from previous direction;
- reused trigger record;
- v3 trigger;
- mocked/synthetic reference;
- cleanup failure;
- environment attestation failure;
- mixed driver build digest;
- instrumented/uninstrumented binary substitution;
- support-router process present;
- more or fewer than two router processes.

## Workstream G: evidence model and durability

For one live diagnostic run, retain sanitized artifacts for all four directions, including failures.

Required artifact classes:

```text
run-identity.json
environment-manifest.json
source-transfer-manifest.json
reference-build-manifest.json
direction-<id>.json
trigger-<id>.json
sender-observation-<id>.json
receiver-observation-<id>.json
sandbox-attestation-<id>.json
cleanup-<id>.json
aggregate-manifest.json
checksums.sha256
verification-result.json
```

Rules:

- exactly four primary direction IDs;
- full 64-hex SHA-256 values, never abbreviations;
- exact source commit separate from documentation closure commit;
- no uncommitted guest edits;
- no placeholder digests;
- all failures retained as sanitized typed records;
- raw logs/RouterInfos/keys remain in disposable run root and are not exported;
- finalization is atomic;
- verifier recomputes every digest after export;
- any mutation invalidates bundle.

### Provenance binding

Bind at minimum:

- i2pr source commit/tree/archive;
- i2pr launcher binary;
- scenario renderer and manifest;
- Rust toolchain/target;
- Java reference source/tree/jars/driver/classpath;
- i2pd source/tree/patch/binaries/libraries;
- trigger/event/observation schema implementations;
- qualification receipts;
- execution environment;
- topology scripts;
- verifier binary/module.

## Workstream H: live qualification sequence

Use a dedicated Ubuntu execution lane satisfying Plan 061 resources and rootless contract.

### H1. Preflight

- clean repository at exact commit;
- all local validation passes;
- both reference qualification receipts validate;
- build/reference caches validate offline;
- rootless probe positive;
- source archive transferred with exact digest;
- no residual process/namespace/run state;
- no placeholder provenance;
- diagnostics sanitized.

### H2. Reference smoke

Run one fresh positive control for each driver mode:

- Java listen;
- Java dial;
- i2pd listen;
- i2pd dial;
- i2pd uninstrumented behavior control.

These are preflight controls, not primary results.

### H3. Four-direction diagnostic run

Recommended order:

```text
i2pr-to-java-ipv4
java-to-i2pr-ipv4
i2pr-to-i2pd-ipv4
i2pd-to-i2pr-ipv4
```

Each direction uses fresh state and unique message ID. Do not reuse a direction after partial failure in the same aggregate.

### H4. Failure handling

On failure:

1. complete cleanup;
2. finalize a diagnostic failed bundle when structurally possible;
3. identify ownership: i2pr protocol, Java driver, i2pd driver, adapter, topology, evidence, or environment;
4. implement fix in a new commit;
5. rerun all applicable local tests;
6. rerun qualification affected by the changed artifact;
7. rerun the entire four-direction diagnostic aggregate from fresh state.

Do not patch only the failed direction into an existing aggregate.

### H5. Qualification success

Plan 065 live qualification succeeds only when one complete aggregate reports all four directions passed and the independent verifier accepts it.

This bundle is diagnostic implementation-floor evidence. It is not one of the two Plan 066 authoritative bundles unless Plan 066 explicitly freezes the exact same commit before execution; preferred practice is to cut a fresh candidate after Plan 065 closure and run two new bundles.

## Test files

Add/update at minimum:

```text
tests/integration/ntcp2/harness/test_plan065.py
tests/integration/ntcp2/harness/test_mixed_runner.py
tests/integration/ntcp2/harness/test_evidence_bundle.py
tests/integration/ntcp2/harness/test_java_direct_driver.py
tests/integration/ntcp2/harness/test_i2pd_direct_driver.py
crates/i2pr-interop/tests/ntcp2_launcher.rs
```

If the launcher crate uses a different test layout, place tests beside existing launcher tests.

### Required test cases

At least:

1. deterministic unique message ID derivation;
2. zero/collision rejection;
3. i2pr sender uses scenario ID;
4. i2pr receiver exact ID success;
5. wrong envelope ID failure;
6. wrong payload ID failure;
7. wrong peer Router Hash failure;
8. duplicate DeliveryStatus failure;
9. Java structured event success;
10. Java generic log-only failure;
11. i2pd post-AEAD event success;
12. i2pd pre-AEAD/generic log failure;
13. old helper selection failure;
14. SAM/HTTP trigger selection failure;
15. support topology process-count failure;
16. exactly two processes positive fixture;
17. v3 trigger rejection;
18. stale event rejection;
19. sender-only false-pass regression;
20. cleanup overrides protocol success;
21. parent network drift rejection;
22. provenance placeholder rejection;
23. full four-direction fixture acceptance;
24. one missing direction rejection;
25. mutation after finalization rejection.

## Static checks

Extend static validation to fail when active primary paths contain:

- hard-coded `0x0420_0001` DeliveryStatus authority;
- type-only DeliveryStatus success with no ID comparison;
- SAM/HTTP/I2PControl primary trigger;
- support-router primary topology;
- old i2pd helper binary selection;
- 40-hex Router Hash validation;
- generic phrase catalog as sole receiver evidence;
- missing Java/i2pd qualified receipt;
- Plan 060 candidate as active candidate;
- synthetic fallback capable of `passed`.

Scope checks carefully to active primary code, not historical plans/fixtures.

## Documentation and closure artifacts

Update:

```text
README.md
AGENTS.md
docs/architecture/interop-apparatus.md
docs/protocol-support.md
tests/integration/ntcp2/README.md
.opencode/skills/i2pr-ntcp2-interop/SKILL.md
plans/030-milestone-3-closure.md
plans/065-status.md
specs/support.toml
```

`specs/support.toml` remains `experimental` and `advertised = false`; add Plan 065 status/evidence references without claiming final closure.

`plans/065-status.md` must record:

- exact implementation-floor commit;
- exact four-direction diagnostic bundle digest;
- verifier result;
- driver/receipt digests;
- environment digest;
- validation commands/results;
- remaining Plan 066 work;
- no final certificate claim.

## Non-goals

Plan 065 does not:

- activate production daemon NTCP2;
- perform public-network tests;
- add SAM/I2CP/tunnel support;
- qualify IPv6;
- benchmark throughput;
- weaken observer requirements;
- reuse Plan 060 candidate;
- freeze Plan 066 candidate;
- advertise NTCP2.

## Stop rules

Stop and record a typed blocker when:

- Java or i2pd qualification receipt cannot validate;
- exact message ID cannot be propagated end-to-end;
- receiver-side exact observation is absent;
- canonical runner needs a support router or public NetDB to operate;
- a reference driver requires changing pinned cryptographic behavior;
- rootless no-egress topology cannot be enforced;
- cleanup or parent network restoration is not exact;
- one complete four-direction diagnostic aggregate cannot pass;
- provenance differs across directions;
- a failure is hidden by retry/fallback.

Do not proceed to Plan 066 with any blocker.

## Closure criteria

Plan 065 closes only when:

- i2pr sender and receiver require exact DeliveryStatus ID;
- all primary scenarios carry unique nonzero IDs and 64-hex Router Hashes;
- Java and i2pd direct drivers are canonical;
- SAM/HTTP/support-topology paths cannot satisfy primary directions;
- canonical topology contains exactly two router processes;
- pass predicate requires independent receiver decrypt/decode evidence;
- evidence bundle retains all four sanitized direction records;
- all provenance is exact and nonzero;
- full local/static validation passes;
- one complete live four-direction diagnostic bundle passes from fresh state;
- independent verifier accepts that bundle;
- cleanup and parent network restoration pass;
- `plans/065-status.md` records exact implementation floor and bundle digest;
- NTCP2 remains experimental/non-advertised.

## Small-model handoff instructions

Execute workstreams in order A through H. Do not start live qualification until all local negative tests pass.

For each direction, create a traceability table before implementation:

```text
scenario field -> sender event -> receiver event -> direction record -> aggregate verifier
```

Every message-ID and Router-Hash field must have one source of truth and explicit comparisons at each boundary. Do not solve fixture failures by removing comparisons or making fields optional. Do not rerun only one failed direction into an aggregate. Do not write Plan 065 status until a real four-direction diagnostic bundle exists and verifies.
# Plan 064: i2pd direct NTCP2 driver and observer correction

## Status and dependencies

- Status: planned.
- Parent roadmap: Plan 061.
- Starts only after Plan 062 closes with trigger/event schemas and ADR 0022 fixed for this implementation cycle.
- May execute in parallel with Plan 063.
- Plan type: test-only pinned-reference driver replacement, passive observer implementation, local qualification, and provenance closure.
- Does not perform final four-direction qualification or candidate freeze.

## Objective

Replace the current partial i2pd helper with a correctly initialized, dual-mode, source-locked i2pd 2.60.0 NTCP2 interoperability driver.

The driver must:

1. initialize the pinned i2pd runtime prerequisites in source-verified order;
2. use the real pinned NTCP2 transport implementation;
3. import one exact peer RouterInfo directly;
4. send one real DeliveryStatus I2NP message in dial mode;
5. act as a real NTCP2 listener in listen mode;
6. emit exact structured sender and receiver observations;
7. include a compile-time-gated passive observer after successful AEAD decryption and I2NP conversion;
8. include an uninstrumented control build proving the observer does not alter transport success;
9. bind every source, patch, compiler, library, and binary input;
10. close with 10/10 fresh-state lifecycle qualification in both modes.

## Current helper defects that must not be preserved

The existing helper is a useful scaffold but is not closure-grade. The replacement must explicitly eliminate these defects:

### D1. Router Hash is treated as SHA-1/40-hex

Use a 32-byte I2P `IdentHash` and 64 lowercase hex representation. Do not use `SHA_DIGEST_LENGTH`, SHA-1 buffers, or 40-character validation for Router Hash.

### D2. Wrong transport static key is hashed

The current helper calls an SSU2 static-key accessor. The replacement must select the exact published NTCP2 RouterAddress used for the target endpoint and hash its `s` field. It must separately bind the NTCP2 `i` value/IV.

### D3. Incomplete initialization

The current helper starts transports without proving configuration, filesystem, crypto, local router context, NetDB, and transport initialization are complete.

The replacement must source-verify and implement the pinned equivalents of:

```text
config init and command/config finalization
owned data-directory setup
filesystem initialization
crypto initialization
network ID configuration
reserved-range policy configuration
router context initialization
transport singleton initialization
NetDB start
NTCP2-only transports start
router context service start when required by DeliveryStatus dispatch
```

Shutdown must occur in strict reverse ownership order.

### D4. Null message trigger

Never call `SendMessage(target, nullptr)` for a primary direction. Dial mode must create and send one real `CreateDeliveryStatusMsg(delivery_status_message_id)` or pinned equivalent.

### D5. Incorrect asynchronous future interpretation

For a newly connecting peer, `Transports::SendMessage` may initiate a connection, queue the message, and return no established session immediately. The replacement must not classify an initial null session as final failure or treat a returned future alone as proof of frame transfer.

The driver must wait boundedly for source-verified connection/session state and exact sender observer completion.

### D6. Reserved synthetic endpoint rejection

The sealed topology uses RFC 5737 synthetic addresses. The driver must explicitly disable reserved-range rejection through the pinned configuration/API before the connect attempt, while retaining all other target validation.

### D7. No exact receiver correlation

Generic debug logs that say a message decrypted or an I2NP block appeared are insufficient. The passive observer must report the exact decoded DeliveryStatus message ID and peer Router Hash after successful protocol conversion.

### D8. Placeholder provenance

No attempted run may contain zero or placeholder digests for helper inputs, source tree, Boost, OpenSSL, compiler, observer patch, or binary.

## Deliverable layout

Create or replace the current helper with a dedicated component, recommended:

```text
tests/integration/ntcp2/reference-drivers/i2pd/
├── README.md
├── src/
│   ├── i2pd_ntcp2_interop_driver.cpp
│   ├── interop_observer.h
│   └── interop_observer.cpp
├── patches/
│   └── i2pd-2.60.0-interop-observer.patch
├── CMakeLists.txt
├── build-driver.sh
├── run-driver.sh
├── source-lock.json
├── build-manifest.schema.json
└── fixtures/
```

If an existing helper path is already referenced by scripts, either migrate it atomically or leave a thin fail-closed compatibility wrapper that invokes the new driver. Do not maintain two independent active implementations.

## Command-line contract

Expose:

```text
i2pd-ntcp2-interop-driver listen --config <driver-config.json>
i2pd-ntcp2-interop-driver dial   --config <driver-config.json>
i2pd-ntcp2-interop-driver inspect --config <driver-config.json>
```

Unknown options and unknown JSON fields fail closed.

Required strict config fields should align with Plan 062:

```text
schema
schema_version
run_id
scenario_id
direction
mode
data_dir
output_dir
local_address
local_port
network_id
peer_router_info_path
expected_local_router_hash_sha256
expected_peer_router_hash_sha256
expected_peer_address
expected_peer_port
delivery_status_message_id
startup_timeout_ms
handshake_timeout_ms
data_phase_timeout_ms
shutdown_timeout_ms
reference_revision
reference_tree_sha256
driver_source_sha256
driver_binary_sha256
observer_patch_sha256
build_manifest_sha256
run_identity_sha256
```

`inspect` validates inputs and build provenance but starts no service and opens no socket.

## Pinned initialization contract

The exact implementation must follow Plan 062 source verification. The conceptual sequence is:

1. initialize i2pd configuration definitions;
2. parse/finalize an owned, explicit config with:
   - `datadir=<owned data_dir>`;
   - `netid=99`;
   - IPv4 enabled;
   - IPv6 disabled for this plan;
   - NTCP2 enabled and published on synthetic address/port;
   - SSU2 disabled;
   - NAT/external-discovery behavior disabled;
   - reseed disabled;
   - reserved-range checking disabled only for the sealed synthetic topology;
3. initialize owned filesystem paths;
4. initialize cryptography;
5. initialize local router context and generate/load local identity plus NTCP2 keys;
6. initialize transport singleton state;
7. start NetDB in a mode that performs no reseed/public lookup;
8. start NTCP2 transports with SSU2 false;
9. start router context service only if required by DeliveryStatus dispatch/observer path;
10. wait for listener bound and local RouterInfo produced;
11. verify local RouterInfo exactly;
12. emit readiness events.

No public route exists during this sequence, but configuration must also prohibit network bootstrap rather than relying solely on route absence.

## RouterInfo handling

### Local RouterInfo

Verify and export:

- valid signature;
- network ID 99;
- exact local Router Hash;
- one selected published NTCP2 IPv4 address;
- exact synthetic host/port;
- version compatible with pinned i2pr parser;
- exact NTCP2 `s` key and `i` IV;
- no SSU2 primary address requirement.

### Peer RouterInfo

Before dial:

1. reject symlink/path escape;
2. reject empty/oversized/truncated input;
3. parse with pinned i2pd RouterInfo implementation;
4. verify RouterInfo signature through pinned APIs;
5. verify network ID 99;
6. verify 64-hex Router Hash;
7. select exact NTCP2 IPv4 address matching expected endpoint;
8. verify `s`, `i`, version, host, and port;
9. insert exact bytes into NetDB;
10. verify `FindRouter(expected_ident_hash)` returns the same identity and RouterInfo digest.

Do not use SSU2 key accessors or generic first-address selection.

## Driver state machine

### Common startup

Both modes:

1. validate config/digests;
2. create 0700 owned roots where supported;
3. initialize pinned runtime;
4. wait for bound NTCP2 listener;
5. export local RouterInfo;
6. emit `process_started`, `listener_ready`, and `router_info_exported` events;
7. install/activate the passive observer only when the instrumented build is selected.

### Listen mode

1. wait for one expected peer connection;
2. require an established NTCP2 session bound to the expected peer identity;
3. receive one data-phase frame;
4. observer records successful AEAD decryption;
5. observer records successful I2NP conversion after `FromNTCP2` or pinned equivalent;
6. require I2NP type DeliveryStatus;
7. require exact DeliveryStatus message ID;
8. require expected sender Router Hash;
9. reject duplicate matching messages;
10. emit structured events;
11. shut down cleanly.

### Dial mode

1. validate/import exact peer RouterInfo;
2. create one real DeliveryStatus message using the per-run ID;
3. submit exactly once through `Transports::SendMessage`/`SendMessages` pinned API;
4. if initial return indicates connection initiation rather than established session, wait boundedly for the peer's established session state;
5. observer records exact message inclusion and successful asynchronous frame write;
6. require expected peer identity and NTCP2 transport type;
7. emit structured events;
8. shut down cleanly.

No internal retry is allowed. One invocation is one attempt.

## Passive observer design

## Placement

The receive observer must be called only after:

1. encrypted frame length accepted;
2. AEAD verification succeeded;
3. block bounds validated;
4. block type is I2NP;
5. NTCP2 short-header representation was converted to a valid I2NP message object.

Recommended source seam is immediately after `FromNTCP2()` and before handing the decoded message to `I2NPMessagesHandler`.

The sender observer must be called only after successful asynchronous socket write for the frame containing the exact message. Recommended seam is the successful branch of `HandleI2NPMsgsSent`/`HandleNextFrameSent`, with access to the message vector before destruction.

## Observer API

The observer must be compile-time gated, for example:

```cpp
#ifdef I2PD_INTEROP_OBSERVER
interop::ObserveReceivedI2NP(session, message, receive_sequence);
#endif
```

and:

```cpp
#ifdef I2PD_INTEROP_OBSERVER
interop::ObserveSentI2NP(session, messages, send_sequence, bytes_transferred);
#endif
```

The API must:

- return `void`;
- be `noexcept` or catch all internal exceptions;
- never block transport threads on unbounded I/O;
- write only to an owned bounded sink;
- drop observation with a typed local counter if sink is unavailable rather than changing transport behavior;
- expose no raw payload bytes;
- expose no private keys, Noise state, frame keys, IV state, or transcript;
- expose only allowlisted message metadata.

Allowed observed fields:

```text
peer_router_hash_sha256
transport = ntcp2
direction
frame_sequence
i2np_type
i2np_envelope_message_id
delivery_status_message_id
bytes_transferred
monotonic_ms
```

For non-DeliveryStatus messages needed during handshake/session maintenance, either emit no data event or emit only a bounded count. They cannot satisfy primary evidence.

## Behavior-neutrality proof

Build two binaries from the exact pinned tree:

1. instrumented build with `I2PD_INTEROP_OBSERVER`;
2. uninstrumented control build without the macro.

Run identical sealed i2pd-to-i2pd control scenarios and compare:

- successful connection count;
- exact DeliveryStatus transfer result;
- terminal result;
- cleanup result;
- externally visible RouterInfo;
- no observer-driven retry or timing branch.

The observer build may differ in binary digest and observation output only. Any protocol outcome difference blocks qualification.

## Structured event semantics

Emit Plan 062 `i2pr-reference-event-v1` records.

### `ntcp2_authenticated`

Emitted only when the real session is established and bound to expected peer identity.

### `frame_emitted`

Emitted only after successful async write of the frame containing the exact DeliveryStatus message ID.

### `frame_authenticated_and_decrypted`

Emitted only from the successful receive AEAD branch for the frame sequence that later contains the exact message.

### `i2np_message_decoded`

Emitted only after `FromNTCP2` conversion, with exact DeliveryStatus payload ID and peer identity.

The harness must correlate decrypt and decode events by process event sequence/frame sequence. A decrypt event from one frame and decode event from an unrelated frame cannot satisfy one primary observation.

## Shutdown contract

Use strict reverse ownership:

1. stop new driver work;
2. stop router context service if started;
3. stop NTCP2 transports;
4. stop NetDB;
5. stop logger;
6. terminate crypto;
7. close observation sink;
8. verify listener closed and worker threads joined;
9. verify no driver-owned process/state remains;
10. emit `terminal_clean` only after success.

Any exception or timeout is `cleanup-failed` and exits nonzero.

## Build and provenance

### Build inputs

Record exact:

- i2pd reference version/revision;
- source archive digest;
- source tree digest;
- observer patch digest;
- driver source digest;
- CMake files digest;
- compiler path/version;
- CMake/Ninja versions;
- complete compile definitions/options;
- complete link command;
- Boost library paths/digests;
- OpenSSL library paths/digests;
- any zlib/miniupnpc or other linked library paths/digests;
- instrumented binary digest;
- uninstrumented binary digest.

Use `ldd` plus resolved-file SHA-256 manifest on Linux. Reject `not found`, unexpected host-global libraries, or path drift between qualification and candidate.

### Source patch application

`build-driver.sh` must:

1. verify pristine pinned source tree digest;
2. apply exactly one reviewed observer patch with `--fuzz=0` equivalent behavior;
3. verify patched tree digest;
4. build instrumented driver;
5. restore/recreate pristine tree;
6. build uninstrumented control;
7. emit manifests;
8. perform no network fetch in offline build mode.

No manual edit inside the build guest is allowed.

## Test plan

### Unit/config tests

Cover:

1. strict config unknown-field rejection;
2. wrong schema/revision rejection;
3. 40-hex Router Hash rejection;
4. all-zero digest rejection;
5. message ID bounds;
6. path traversal/symlink rejection;
7. IPv6 rejected for this plan;
8. SSU2-enabled config rejected;
9. reserved-range policy not explicitly set rejected;
10. retry count other than one rejected.

### RouterInfo tests

Cover:

1. valid i2pd RouterInfo accepted;
2. valid i2pr RouterInfo accepted;
3. signature failure rejected;
4. wrong network ID rejected;
5. wrong peer Router Hash rejected;
6. endpoint mismatch rejected;
7. missing NTCP2 address rejected;
8. wrong NTCP2 `s` rejected;
9. wrong NTCP2 `i` rejected;
10. SSU2 key cannot satisfy NTCP2 field;
11. NetDB lookup returns exact imported peer.

### Observer tests

Cover:

1. no observer symbol/callback in uninstrumented build;
2. observer return path cannot alter transport branch;
3. observer sink failure does not change protocol result;
4. pre-AEAD event cannot be emitted;
5. AEAD failure emits no decrypt/decode success;
6. malformed block emits no decode success;
7. wrong I2NP type cannot satisfy DeliveryStatus;
8. wrong DeliveryStatus ID rejected;
9. duplicate exact message rejected;
10. sender event emitted only after successful write;
11. failed write emits no frame-emitted success;
12. raw payload/key/transcript strings rejected by sanitizer.

### i2pd-to-i2pd sealed control

Run instrumented and uninstrumented builds in identical two-process sealed topologies:

- listener exports RouterInfo;
- dialer imports it;
- dialer sends exact DeliveryStatus;
- listener observes exact message;
- both shut down;
- no default route/DNS/reseed;
- no SSU2;
- no process/state leak.

This is driver/observer control evidence only, not i2pr evidence.

### Qualification repetition

On the chosen Ubuntu host:

- instrumented listen mode: 10/10 fresh-state passes;
- instrumented dial mode: 10/10 fresh-state passes;
- uninstrumented control scenario: at least 10/10 matching protocol outcomes;
- observer behavior-neutrality comparison passes;
- all event files validate;
- all cleanup checks pass.

Any failure blocks qualification.

## Harness adapter

Create/update a Python adapter, recommended:

```text
tests/integration/ntcp2/harness/i2pd_direct_driver.py
tests/integration/ntcp2/harness/test_i2pd_direct_driver.py
tests/integration/ntcp2/harness/test_i2pd_direct_control.py
```

Adapter responsibilities:

- verify/build or locate driver artifact;
- render strict config;
- start process;
- wait for structured readiness;
- export RouterInfo;
- collect structured events;
- enforce one-shot attempt;
- stop and verify cleanup;
- produce qualification receipt.

Do not wire it into canonical primary runner until Plan 065.

## Qualification receipt

Create only after measured qualification:

```text
tests/integration/ntcp2/qualification/i2pd-direct-driver.json
```

Required fields:

```text
schema
schema_version
qualified
reference_revision
reference_tree_sha256
observer_patch_sha256
driver_source_sha256
instrumented_binary_sha256
uninstrumented_binary_sha256
linked_library_manifest_sha256
listen_passes
listen_attempts
dial_passes
dial_attempts
control_passes
control_attempts
behavior_neutrality_verified
control_bundle_sha256
execution_environment_sha256
cleanup_verified
receipt_sha256
```

No false qualified receipt may be committed from a host that did not run the exact sealed scenarios.

## Documentation updates

At closure update:

```text
tests/integration/ntcp2/reference-drivers/i2pd/README.md
tests/integration/ntcp2/README.md
docs/architecture/interop-apparatus.md
.opencode/skills/i2pr-ntcp2-interop/SKILL.md
plans/064-status.md
```

State:

- exact observer patch location and limitations;
- instrumented/uninstrumented control result;
- direct driver is test-only;
- old null-message helper is retired;
- local i2pd controls are not i2pr evidence;
- no final primary direction has passed yet;
- NTCP2 remains experimental/non-advertised.

## Non-goals

Plan 064 does not:

- patch NTCP2 crypto, handshake, framing, acceptance, or routing;
- implement a production i2pd plugin/API;
- use I2PControl, SAM, streaming, tunnels, or floodfill;
- re-pin i2pd without separate decision;
- modify production i2pr code;
- run final mixed-router authority;
- freeze a candidate;
- advertise support.

## Stop rules

Stop and record a typed blocker when:

- pinned i2pd cannot initialize a direct NTCP2-only two-process control without public bootstrap;
- exact peer RouterInfo cannot be inserted/retrieved through pinned NetDB APIs;
- `SendMessage` semantics cannot be observed without changing transport behavior;
- the observer cannot be placed after AEAD and I2NP conversion without changing control flow;
- instrumented and uninstrumented protocol outcomes differ;
- exact DeliveryStatus ID cannot be observed on send and receive;
- linked library provenance cannot be made deterministic;
- 10/10 qualification fails;
- cleanup leaves a thread, listener, lock, or state.

Do not fall back to HTTP/I2PControl, SAM, or generic logs.

## Closure criteria

Plan 064 closes only when:

- old helper defects D1-D8 are eliminated;
- driver follows source-verified pinned initialization;
- inspect/listen/dial modes exist;
- dial mode sends a real correlated DeliveryStatus;
- listen mode proves post-AEAD exact decode;
- NTCP2 `s` and `i` fields are correctly bound;
- 64-hex Router Hash is used end-to-end;
- passive observer is compile-time gated and behavior-neutral;
- instrumented/uninstrumented controls pass;
- listen and dial qualification each pass 10/10;
- all digests and linked libraries are measured;
- no process/socket/state leak occurs;
- all focused and applicable full tests pass;
- qualification receipt is committed with `qualified=true` only after real execution;
- `plans/064-status.md` records exact commits and makes no mixed-router i2pr success claim.

## Small-model handoff instructions

Implement in this order:

1. pinned-source initialization proof and tests;
2. strict config/inspect mode;
3. pristine offline build and manifests;
4. common startup/export/shutdown;
5. real dial DeliveryStatus path;
6. listener path;
7. passive receive observer;
8. passive sender observer;
9. uninstrumented build and behavior-neutral controls;
10. Python adapter;
11. 10/10 qualification and status record.

Do not modify the observer seam and driver lifecycle in one large commit. Keep the observer patch minimal and reviewable. Never use `nullptr` as the primary message, never use an SSU2 accessor for NTCP2 proof, never interpret initial future completion as receiver evidence, and never claim qualification from log-only or mocked fixtures.
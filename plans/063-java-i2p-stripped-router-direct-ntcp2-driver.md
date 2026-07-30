# Plan 063: Java I2P stripped-router direct NTCP2 driver

## Status and dependencies

- Status: planned.
- Parent roadmap: Plan 061.
- Starts only after Plan 062 closes with ADR 0022 Accepted and trigger/event schemas frozen for this implementation cycle.
- May execute in parallel with Plan 064.
- Plan type: test-only reference-driver implementation, local qualification, and source/provenance closure.
- Does not perform final four-direction qualification or candidate freeze.

## Objective

Implement a deterministic, source-locked Java I2P 2.12.0 direct NTCP2 interoperability driver that can act as either listener/responder or dialer/initiator without SAM, I2CP, tunnel pools, floodfill support, reseed, or a support router.

The driver must use the pinned Java router's real:

- embedded `Router` and `RouterContext`;
- NTCP/NTCP2 transport manager;
- RouterInfo creation and verification;
- dummy in-memory NetDB facade;
- inbound I2NP dispatch path;
- `OutNetMessage` outbound path;
- transport send-success/failure callbacks;
- lifecycle shutdown path.

It must not implement or patch NTCP2 cryptography, handshake transcript, framing, or RouterInfo signature behavior.

## Source-locked design basis

Before writing the driver, re-open the Plan 062 source-verification record and the exact pinned Java I2P 2.12.0 tree.

The implementation must identify the pinned equivalents of the upstream stripped-router `SSUDemo` pattern:

1. construct an embedded `Router` with environment properties;
2. disable UDP and enable the real NTCP transport;
3. disable UPnP and allow synthetic/local addresses;
4. use dummy client, NetDB, peer-manager, and tunnel-manager facades;
5. start the router and obtain `RouterContext`;
6. export local signed RouterInfo;
7. import a peer RouterInfo into the dummy NetDB;
8. submit a real I2NP message through `OutNetMessage`/`outNetMessagePool`;
9. register a receive handler for DeliveryStatus;
10. shut down the embedded router cleanly.

If symbol names differ in the pinned revision, update implementation notes and tests to use the pinned names. Do not re-pin Java solely to match current upstream examples.

## Deliverable layout

Create a dedicated test-only component, recommended:

```text
tests/integration/ntcp2/reference-drivers/java/
├── README.md
├── src/
│   └── JavaNtcp2InteropDriver.java
├── build-driver.sh
├── run-driver.sh
├── source-lock.json
├── classpath-manifest.json
├── build-manifest.schema.json
└── fixtures/
```

If the repository's existing reference-driver layout differs, preserve its conventions while keeping Java driver source separate from production source.

The driver artifact should be a small JAR or class directory built only from:

- driver source;
- pinned Java I2P jars/classes;
- JDK standard library.

No Maven Central or unpinned third-party dependency may be introduced.

## Command-line contract

The driver must expose explicit modes:

```text
java ... JavaNtcp2InteropDriver listen --config <driver-config.json>
java ... JavaNtcp2InteropDriver dial   --config <driver-config.json>
java ... JavaNtcp2InteropDriver inspect --config <driver-config.json>
```

Unknown arguments and unknown JSON fields fail closed.

Recommended strict config fields:

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
run_identity_sha256
```

Mode rules:

- `listen` does not require peer RouterInfo before listener readiness, but it must receive expected peer identity through the authenticated connection and compare it to the run identity.
- `dial` requires an exact peer RouterInfo and endpoint before startup of the send attempt.
- `inspect` validates configuration, classpath, source lock, and writable-path confinement but opens no listener and dials no peer.

## Java router configuration

Generate all properties from the strict driver config. At minimum, source-verify and set the pinned equivalents of:

```properties
router.networkID=99

i2np.udp.enable=false
i2np.ntcp.enable=true
i2np.upnp.enable=false
i2np.allowLocal=true

time.disabled=true

i2np.ntcp.autoip=false
i2np.ntcp.hostname=<synthetic local IPv4>
i2np.ntcp.autoport=false
i2np.ntcp.port=<scenario port>
i2np.ntcp.ipv6=disable

i2p.dummyClientFacade=true
i2p.dummyNetDb=true
i2p.dummyPeerManager=true
i2p.dummyTunnelManager=true

router.publishPeerRankings=false
router.reseedDisable=true
```

Do not set a VM communication transport or any property that replaces the real NTCP transport with an in-process fake.

Every path property must resolve beneath `data_dir` or `output_dir`, including:

- router config;
- router keys;
- RouterInfo;
- key backup;
- logs;
- event log;
- ping file;
- blockfile;
- temporary directory;
- native library extraction when needed.

Reject symlinks that escape owned roots.

## Driver state machine

### Common startup

Both modes perform:

1. validate config and all digests;
2. create owned directories with mode 0700 where platform supports POSIX permissions;
3. establish a run-local logging/output sink;
4. construct environment properties;
5. instantiate the embedded router exactly once;
6. call the pinned embedded startup API exactly once;
7. obtain `RouterContext`;
8. register DeliveryStatus receive handling before accepting a data-phase message;
9. wait boundedly for router startup, signed RouterInfo production, and NTCP2 listener readiness;
10. verify local RouterInfo signature, network ID, Router Hash, address, port, NTCP2 version, static key, and IV;
11. write `router.info` to the declared exchange path;
12. emit structured `process_started`, `listener_ready`, and `router_info_exported` events.

Do not use a fixed 30-second sleep. Readiness must be event/state based with a bounded polling interval and monotonic deadline.

### Listen mode

After common startup:

1. wait for one authenticated NTCP2 peer;
2. verify the authenticated peer Router Hash equals `expected_peer_router_hash_sha256`;
3. wait for one DeliveryStatus message;
4. verify the decoded I2NP type is DeliveryStatus;
5. verify the I2NP envelope message ID and DeliveryStatus payload message ID equal `delivery_status_message_id` according to the pinned codec semantics;
6. reject duplicates;
7. emit:
   - `tcp_connected`;
   - `ntcp2_authenticated`;
   - `frame_authenticated_and_decrypted` only if the chosen upstream handler boundary proves this level;
   - `i2np_message_decoded` from the exact DeliveryStatus handler;
8. emit `terminal_clean` after successful shutdown.

The preferred receiver seam is a `HandlerJobBuilder` registered for `DeliveryStatusMessage.MESSAGE_TYPE`, because it receives an already decoded I2NP message and source identity/hash. `TransportEventListener` may be used only when pinned-source inspection demonstrates equivalent or stronger source identity and decode semantics.

Do not infer receiver success from an event-log phrase.

### Dial mode

After common startup:

1. read peer RouterInfo from the declared path;
2. reject empty, oversized, symlinked, malformed, invalid-signature, wrong-network, wrong-hash, wrong-address, wrong-port, wrong-version, wrong-static-key, or wrong-IV input;
3. insert the verified RouterInfo into the dummy NetDB through the pinned API;
4. verify a local lookup returns the same identity and RouterInfo digest;
5. construct one built-in `DeliveryStatusMessage` using `delivery_status_message_id` and a bounded current timestamp;
6. wrap it in `OutNetMessage` targeting the imported RouterInfo;
7. configure one success job and one failure job backed by a bounded synchronization primitive;
8. submit through `outNetMessagePool` exactly once;
9. wait boundedly for the transport send result;
10. verify the successful transport style is NTCP/NTCP2 and the peer identity matches;
11. emit:
   - `peer_router_info_validated`;
   - `tcp_connected`;
   - `ntcp2_authenticated`;
   - `frame_emitted` when the pinned transport success callback semantics prove successful peer receipt/transport completion;
12. stop cleanly.

No retry is allowed inside one invocation. A qualification repetition launches a fresh driver process with a new run ID and state root.

## Exact event semantics

The Java driver must emit Plan 062 `i2pr-reference-event-v1` records atomically as NDJSON or one JSON file per event.

### `ntcp2_authenticated`

Must be emitted only when the real NTCP transport reports an established connection bound to the expected peer identity.

### `frame_emitted`

Must be emitted only from the successful `OutNetMessage`/transport completion path for the exact DeliveryStatus message ID. A queue insertion event is insufficient.

### `frame_authenticated_and_decrypted`

If the registered DeliveryStatus handler is downstream of authenticated transport decryption but does not expose a distinct frame callback, the driver may emit this event immediately before `i2np_message_decoded` from the same handler invocation, provided the source-verification record proves that malformed/unauthenticated ciphertext cannot reach that handler. Record the source path and rationale in `README.md` and the qualification receipt.

### `i2np_message_decoded`

Must contain the exact message ID, sender Router Hash, type 10, and handler event digest.

## Shutdown contract

The driver must own and bound shutdown:

1. stop accepting new work;
2. invoke the pinned router shutdown API;
3. wait for terminal router state;
4. remove any shutdown hook installed by the driver when safely supported;
5. verify listener port closed;
6. verify no driver-owned non-daemon thread remains except explicitly allowlisted JVM runtime threads;
7. flush and close event output;
8. emit `terminal_clean` only after successful verification;
9. exit nonzero on forced or incomplete cleanup.

The harness must additionally verify no JVM process, lock file, or writable state survives outside the retained sanitized output.

## Build and provenance

### Build script

`build-driver.sh` must:

- require the exact pinned source/build cache;
- require JDK 17 or the exact repository-declared compatible JDK;
- use a deterministic sorted source list;
- use an explicit classpath;
- emit no download;
- build into an owned output directory;
- produce a build manifest before the binary can run.

### Build manifest

Record:

```text
reference_name
reference_version
reference_revision
reference_source_tree_sha256
reference_archive_sha256
i2p_jar_sha256
router_jar_sha256
all_runtime_jar_sha256_values
driver_source_sha256
driver_binary_sha256
classpath_manifest_sha256
javac_version
java_version
ant_version
build_command_version
build_timestamp_utc
```

The candidate later binds these exact values. No zero or placeholder digest is allowed in an attempted run.

### Native libraries

Prefer pure Java operation where supported. If pinned Java extracts or loads `jbigi`/`jcpuid`, the build/runtime manifest must record the selected library digest and extraction path. No host-global unmeasured library may influence an authoritative run.

## Test plan

### Unit/config tests

Cover:

1. unknown config field rejected;
2. wrong schema rejected;
3. mode mismatch rejected;
4. path traversal rejected;
5. symlink escape rejected;
6. zero message ID rejected;
7. 40-hex Router Hash rejected;
8. wrong reference revision rejected;
9. zero provenance digest rejected;
10. timeout bounds validated;
11. unsupported IPv6 rejected for this plan;
12. network ID other than 99 rejected in primary fixtures.

### RouterInfo tests

Cover:

1. valid pinned Java RouterInfo accepted;
2. valid i2pr RouterInfo accepted;
3. invalid signature rejected;
4. wrong Router Hash rejected;
5. wrong endpoint rejected;
6. missing NTCP2 address rejected;
7. wrong static key rejected;
8. wrong IV rejected;
9. wrong network ID rejected;
10. stale/oversized/truncated input rejected;
11. imported dummy-NetDB lookup returns exact peer.

### Driver lifecycle tests

Cover:

1. inspect mode opens no socket;
2. listener readiness before terminal result;
3. startup timeout classified;
4. dial timeout classified;
5. failed send callback classified;
6. duplicate DeliveryStatus rejected;
7. wrong DeliveryStatus ID rejected;
8. wrong peer identity rejected;
9. shutdown completes;
10. forced-shutdown failure is nonzero;
11. second startup on same driver object rejected;
12. fresh process with fresh state succeeds.

### Local Java-to-Java control

Before mixed-router use, run two driver instances in a sealed two-process topology:

- one `listen`;
- one `dial`;
- direct RouterInfo exchange;
- exact DeliveryStatus transfer;
- no default route/DNS;
- clean shutdown.

This proves driver composition only. It is not i2pr evidence.

### Qualification repetition

On the selected Ubuntu qualification host:

- `listen` startup/readiness/shutdown: 10/10 fresh-state passes;
- `dial` startup/import/send/shutdown against the Java control peer: 10/10 fresh-state passes;
- no shared mutable identity or RouterInfo state across repetitions;
- no process/lock/socket leak;
- every event validates against Plan 062 schemas.

A single failure blocks qualification until diagnosed. Do not report 9/10 as qualified.

## Required harness integration artifacts

Create a Python adapter or update the existing Java adapter only enough to:

- build/locate the driver;
- render strict config;
- start/stop one driver process;
- wait for structured readiness;
- export RouterInfo;
- import peer RouterInfo path;
- collect structured events;
- produce a Java qualification receipt.

Do not wire the Java driver into primary `mixed_runner.py` until Plan 065. Plan 063 should expose a stable adapter API and local control tests without changing canonical execution authority prematurely.

Recommended files:

```text
tests/integration/ntcp2/harness/java_direct_driver.py
tests/integration/ntcp2/harness/test_java_direct_driver.py
tests/integration/ntcp2/harness/test_java_direct_control.py
```

## Qualification receipt

Create a sanitized receipt after successful 10/10 qualification:

```text
tests/integration/ntcp2/qualification/java-direct-driver.json
```

Required fields:

```text
schema
schema_version
qualified
reference_revision
reference_tree_sha256
driver_source_sha256
driver_binary_sha256
classpath_manifest_sha256
listen_passes
listen_attempts
dial_passes
dial_attempts
control_bundle_sha256
execution_environment_sha256
cleanup_verified
receipt_sha256
```

`qualified` is true only when all counts are 10/10 and cleanup is verified. A local development machine that cannot run the sealed topology must write no false qualified receipt; it may write an ignored diagnostic record under `target/`.

## Documentation updates

At closure update:

```text
tests/integration/ntcp2/reference-drivers/java/README.md
tests/integration/ntcp2/README.md
docs/architecture/interop-apparatus.md
.opencode/skills/i2pr-ntcp2-interop/SKILL.md
plans/063-status.md
```

State precisely:

- driver is test-only;
- support router is not used;
- local Java-to-Java control is not i2pr evidence;
- qualification host and exact receipt digest;
- no final primary direction has passed yet;
- NTCP2 remains experimental/non-advertised.

## Non-goals

Plan 063 does not:

- modify production i2pr code;
- implement SAM, I2CP, tunnels, NetDB routing, or floodfill;
- re-pin Java without a separate explicit decision;
- patch Java NTCP2 cryptography/framing;
- use arbitrary custom I2NP types;
- run final `java-to-i2pr-ipv4` or `i2pr-to-java-ipv4` authority;
- freeze a candidate;
- advertise support.

## Stop rules

Stop and record a typed blocker when:

- pinned Java source lacks the verified stripped-router/direct-outbound path;
- dummy NetDB cannot return the imported i2pr RouterInfo to the transport manager;
- DeliveryStatus handler cannot expose the exact message ID and sender identity;
- the send callback cannot be source-proven as the declared frame-emission level;
- startup requires public NTP/reseed/network access despite declared properties;
- Java cannot bind the synthetic endpoint in the sealed namespace;
- one process requires shared mutable state with another direction;
- 10/10 lifecycle qualification cannot be achieved;
- cleanup leaves any owned JVM, lock, listener, or state.

Do not replace the direct driver with SAM or a support topology inside this plan.

## Closure criteria

Plan 063 closes only when:

- exact pinned-source APIs are documented;
- Java direct driver builds offline from pinned artifacts;
- inspect/listen/dial modes implement the strict config contract;
- direct RouterInfo import uses dummy NetDB;
- dial mode submits a real correlated DeliveryStatus through `OutNetMessage`;
- listen mode receives exact correlated DeliveryStatus through a source-proven handler;
- structured events validate;
- Java-to-Java sealed control passes;
- listen and dial qualification each pass 10/10 with fresh state;
- no process/socket/lock/state leak occurs;
- all focused and applicable full local tests pass;
- qualification receipt has nonzero measured digests;
- `plans/063-status.md` records implementation commit and receipt digest without claiming mixed-router i2pr success.

## Small-model handoff instructions

Implement in this order:

1. source-verification assertions/tests;
2. strict config parser and inspect mode;
3. common embedded-router startup/readiness/export;
4. listen receive handler;
5. dial RouterInfo import and DeliveryStatus send;
6. structured events;
7. shutdown verification;
8. Python adapter;
9. Java-to-Java control;
10. 10/10 qualification and status record.

After each stage, run its focused test. Do not combine all driver behavior into one unreviewable class method. Use small helpers with explicit ownership and bounded deadlines. Never add a sleep as a substitute for state readiness, never make a required field optional to get a fixture passing, and never claim qualification from mocked or self-handshake tests.
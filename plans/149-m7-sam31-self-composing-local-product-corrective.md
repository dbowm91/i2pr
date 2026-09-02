# Plan 149 — SAM 3.1 self-composing local product and deferred raw-path acceptance corrective

Status: **next executable corrective plan**.

Depends on:

- Plan 134 Milestone 6 local destination/Streaming closure;
- Plan 146 private-destination reference requalification;
- useful Plan 147 raw-socket ownership/runtime-driver implementation;
- Plan 148 blocked audit, which exposed that external-client execution cannot succeed against the current production composition.

Supersedes for next-action authority:

- the interpretation that Plan 147 fully satisfied its original acceptance matrix;
- Plan 148's current `blocked-external-client-build-failure` diagnosis as the complete blocker.

## 1. Goal

Make the real SAM listener self-compose the entire localhost Milestone 7 product path from **SAM protocol commands alone**.

After this plan, a black-box client must be able to do only:

```text
HELLO VERSION MIN=3.1 MAX=3.1
SESSION CREATE STYLE=STREAM ID=A DESTINATION=<PRIV-or-TRANSIENT>
SESSION CREATE STYLE=STREAM ID=B DESTINATION=<PRIV-or-TRANSIENT>
STREAM ACCEPT ID=B
STREAM CONNECT ID=A DESTINATION=<B.PUB>
<raw application bytes>
```

and receive the expected raw bytes without any test or caller invoking private daemon setup such as:

```text
build_sam_destination_bridge(...)
sam_destinations().install(...)
install_remote_lease_set2(...)
install_inbound_tunnel_factory(...)
spawn_destination_driver(...)
```

The plan also closes the Plan 147 acceptance items that were incorrectly deferred to Plan 148: exact `SILENT`, bounded slow-reader/slow-writer behavior, multi-megabyte transfer, loss/duplicate/reorder/ACK recovery, close/reset, sibling-stream isolation, and deterministic teardown.

This is still a **localhost SAM application-product gate**. It does not claim public-network tunnel construction or router-to-router interoperability.

## 2. Current defects that require this pass

### 2.1 `SESSION CREATE` does not construct the STREAM product

`SamServiceState::execute_session_create()` currently installs:

- `DestinationRuntime` in `DestinationRegistry`;
- a separate `StreamingManager` in `StreamingPools`;
- `SamStreamRegistry` session ownership;
- the SAM session registry entry.

It does **not** create/install the `SamDestinationBridge` consumed by `STREAM CONNECT`, does not install local delivery material, and does not spawn the per-destination runtime driver.

`execute_stream_connect()` subsequently looks in `sam_destinations` and returns an error when no bridge exists.

The canonical Plan 147 test hides this gap by manually constructing/installing the bridge after `SESSION CREATE`.

### 2.2 Canonical Plan 147 evidence performs private product setup

`crates/i2pr-daemon/tests/sam_stream_raw_product.rs` manually:

1. constructs private identities;
2. installs `SamDestinationBridge`s;
3. installs deterministic inbound-tunnel factories;
4. cross-installs validated peer LeaseSet2 records;
5. manually spawns destination drivers.

That test proves the byte pump once all hidden prerequisites exist. It does not prove the real SAM service composes those prerequisites.

### 2.3 Local delivery currently depends on a test-installed tunnel factory

`SamDestinationBridge::inbound_tunnel_factory` is optional. `deliver_outbound()` silently drops a queued request when the peer has no factory.

For Milestone 7 localhost product operation, local delivery material must be installed by the SAM service itself. Missing local-delivery capability must be a typed failure/degradation, not a silent packet drop.

### 2.4 Local peer routing currently depends on test-side LeaseSet2 injection

The Plan 147 test manually installs each peer's validated LeaseSet2 into the sender's `DestinationRouting` state. A real external client cannot do this.

For two destinations owned by the same local SAM service, the daemon must resolve the peer's signed local LeaseSet2 through an explicit router-local path.

### 2.5 `SILENT` is not byte-correct

The current raw transition writes `STREAM STATUS RESULT=OK` unconditionally before handing the socket to the raw driver even when the parsed request carries `SILENT=true`. The raw driver retains the flag but does not use it.

The exact CONNECT/ACCEPT wire behavior required by the existing SAM 3.1 plan must be implemented and frozen.

### 2.6 Plan 147 closure deferred mandatory acceptance

The Plan 147 narrative required, but its status deferred:

- SILENT exactness;
- slow-reader/slow-writer boundedness;
- loss/duplicate/reorder/ACK-drop recovery;
- close/reset behavior;
- sibling-stream isolation;
- multi-megabyte bounded transfer.

Plan 149 owns those items now. Do not move them again into the external-client plan.

## 3. Architecture rule: one destination identity owner

Do **not** solve self-composition by reconstructing or copying a second private `DestinationIdentity` for the SAM bridge.

The project invariant remains:

```text
one logical destination
 -> one private identity allocation / ownership graph
 -> zero duplicate raw private-key copies for convenience
```

The current problem exists partly because `DestinationRuntime::new(identity, ...)` consumes the identity while `SamDestinationBridge::new(...)` also expects to own a `DestinationIdentity`.

Choose a narrow ownership correction that preserves one secret allocation. Acceptable designs include:

### Option A — shared single allocation

Internally store the identity in one `Arc<DestinationIdentity>` allocation shared by the destination runtime and the SAM local-product bridge.

Requirements:

- `DestinationIdentity` itself remains non-`Clone`;
- only the `Arc` capability is cloned;
- no public/non-secret `DestinationHandle` gains raw private-key access;
- final drop still zeroizes the one owned secret allocation;
- `Debug` remains redacted.

### Option B — move SAM product state under the destination runtime

Make `DestinationRuntime` the sole owner of the identity and expose narrow operations/capabilities required by the SAM bridge without exposing private bytes.

Requirements:

- no unrestricted service-locator handle;
- no new global mutable state;
- no daemon API returning signing seeds/static private bytes.

Use whichever option creates the smaller, clearer change. Do not make `DestinationIdentity: Clone`.

## 4. Introduce an explicit localhost product fabric

Milestone 7 is not yet backed by live router-to-router tunnel construction. The localhost SAM product therefore needs an explicit daemon-owned local delivery capability rather than test-only fixture installation.

Create a narrowly named component, for example:

```text
SamLocalProductFabric
LocalSamDeliveryFabric
LocalDestinationProductFabric
```

The name must make clear that this is the **authenticated-router-link-bypassed localhost product seam**, not live I2P transport.

The fabric must own or provide:

- the local destination's usable outbound product material;
- fresh one-shot inbound `EstablishedTunnel` material for `i2pr_client::deliver`;
- local signed LeaseSet2 visibility;
- per-destination runtime-driver ownership/signals.

Rules:

- use OS CSPRNG for runtime-created ephemeral material;
- deterministic factories remain test-only;
- no root/sudo/namespaces/Docker/VM/systemd/public network;
- no network socket may pretend to be a live NTCP2/SSU2 peer;
- no support/advertised flag may imply router interoperability;
- no testkit dependency may enter production crates.

If the existing `InboundTunnelFactory` trait remains, install a real localhost-product implementation automatically for every successfully created SAM destination. Rename/document it if necessary so “production deployments install a real inbound-tunnel pool” does not imply that current Milestone 7 evidence exercises live tunnels.

## 5. Make `SESSION CREATE` transactional across the full SAM product

A successful `SESSION STATUS RESULT=OK` must mean the destination is ready for the supported localhost SAM STREAM surface.

The transaction should have explicit ordered stages similar to:

```text
1. decode/generate private destination
2. reserve SAM session id + destination id
3. create canonical destination runtime / identity ownership
4. create/admit local-product tunnel material required for M7
5. produce and self-validate signed LeaseSet2
6. construct/install SamDestinationBridge or equivalent product capability
7. install local inbound delivery provider
8. install/register Streaming ownership
9. register stream session ownership
10. register/spawn exactly one per-destination runtime driver
11. commit SAM session registry entry
12. emit SESSION STATUS RESULT=OK
```

Every failure before step 12 must roll back all earlier stages.

Required negative tests:

- failure constructing local product material;
- failure generating LeaseSet2;
- duplicate destination/session;
- destination registry full;
- stream registry full;
- driver spawn rejection;
- cancellation during create;
- injected failure at each transactional boundary.

After each failure, assert all registry/task/resource counts return to the pre-create baseline.

## 6. Runtime-driver ownership must be automatic and unique

No test/client should call `spawn_destination_driver()` after `SESSION CREATE`.

Integrate driver creation into the real session-create dispatch path under the existing `ChildScope` / cancellation ownership.

Recommended shape:

- keep the synchronous runtime-neutral mutation separate from Tokio spawning if necessary;
- return a typed “destination driver required” capability from the session-create transaction;
- have the connection/service layer spawn it using the existing supervised scope before replying OK;
- on spawn failure, roll back the session transaction;
- store enough ownership metadata to prevent duplicate driver spawn;
- session/control-socket teardown cancels and joins exactly that destination's driver.

Acceptance:

- one session -> exactly one driver;
- duplicate create never produces an orphan driver;
- control socket EOF -> driver exits within shutdown bound;
- daemon cancellation -> all drivers joined;
- no detached `tokio::spawn`.

## 7. Resolve locally owned peer LeaseSet2 without test injection

When `STREAM CONNECT` targets another destination owned by this SAM service, the sender must obtain the peer's signed LeaseSet2 through an explicit local resolution path.

Preferred behavior:

```text
CONNECT target PUB
 -> decode DestinationId
 -> SamDestinations/local destination directory finds target
 -> obtain target's signed LeaseSet2 + NetDB key
 -> validate with the same `ValidatedLeaseSet2` rules used elsewhere
 -> install/update sender routing state
 -> continue Streaming connect
```

Do this lazily at CONNECT or through a bounded local publication directory; avoid O(n²) eager cross-install unless there is a strong reason.

Rules:

- only locally owned destinations may use this localhost shortcut;
- unknown remote destinations must not be fabricated as local;
- malformed/expired/signature-invalid local LeaseSet2 must fail typed;
- no system DNS/address-book expansion;
- do not bypass LeaseSet2 validation merely because the peer is local.

Add a regression proving the canonical black-box test contains **zero** calls to `install_remote_lease_set2`.

## 8. Replace silent packet drops with typed local-product failure

In `deliver_outbound()` or its successor:

- missing peer destination;
- missing local inbound product material;
- invalid/expired material;
- delivery failure;

must not silently discard a request while leaving the Streaming state waiting forever.

Return a typed failure to the driver. The driver must:

- record one bounded failure/degradation;
- wake relevant waiter(s);
- drive stream timeout/reset according to existing Streaming semantics;
- never busy-loop;
- never leak queued bytes/tasks.

Tests must cover each failure and prove bounded termination.

## 9. Exact SAM raw transition semantics

Freeze the following behavior at the real socket boundary.

### CONNECT `SILENT=false`

```text
STREAM STATUS RESULT=OK\n
<raw bytes>
```

The OK line is written only after the underlying Streaming connection is `Established`.

### CONNECT `SILENT=true`

```text
<raw bytes>
```

No success status line is written. On failure, close according to SAM behavior without writing a false success.

### ACCEPT `SILENT=false`

After the inbound Streaming connection is accepted:

```text
STREAM STATUS RESULT=OK\n
<authenticated peer public Destination>\n
<raw bytes>
```

Use the peer identity learned from the accepted Streaming SYN/authenticated connection context. Do not fabricate metadata from the requested destination string or test fixture.

### ACCEPT `SILENT=true`

```text
<raw bytes>
```

No status and no peer-Destination line precede raw data.

Once the raw transition occurs, the command parser must never see subsequent bytes.

Add same-write tests where the command newline and first raw bytes arrive in the same TCP read for both silent modes.

## 10. Canonical black-box product test

Replace or supplement the Plan 147 canonical test with a new test whose behavioral interaction is TCP-only after service startup.

Suggested file:

```text
crates/i2pr-daemon/tests/sam_stream_self_composed.rs
```

Allowed setup:

- instantiate `SamServiceState` with loopback test limits;
- bind/start the listener under normal supervisor ownership;
- inspect non-secret final counters after shutdown.

Forbidden setup after listener startup:

```text
build_sam_destination_bridge
SamDestinations::install
sam_destinations().install
install_inbound_tunnel_factory
install_remote_lease_set2
spawn_destination_driver
bridge_to_peer
send_data_segment
deliver_outbound
```

The test must create both destinations through SAM commands and then perform CONNECT/ACCEPT/raw transfer.

The old Plan 147 test may remain as a focused lower-level regression, but it is no longer final product-composition evidence.

## 11. Close the deferred Plan 147 acceptance matrix

### Binary and sizing matrix

Through self-composed real SAM TCP sockets, transfer exactly:

- 1 byte;
- NUL;
- LF and CRLF;
- invalid UTF-8;
- every byte value `0x00..0xff`;
- payload beginning with `PING`, `QUIT`, `HELLO VERSION`, `STREAM CONNECT` text;
- exactly one negotiated Streaming payload;
- payload + 1 requiring segmentation;
- multi-packet payload;
- simultaneous bidirectional payloads;
- at least one multi-megabyte logical transfer.

Assert byte-for-byte equality.

### Slow reader

- stop reading on B;
- send beyond all configured TCP/Streaming application budgets;
- prove accounted buffered bytes remain under explicit limits;
- prove A stalls/backpressures rather than growing queues;
- resume B;
- verify exact bytes and all counters return to baseline.

### Slow writer / reverse pressure

Exercise the reverse direction with a deliberately constrained sink or socket pressure. Do not add an unbounded staging queue.

### Fault matrix

Use the existing deterministic fault seam **below the real SAM socket boundary**:

- drop one DATA packet -> retransmit -> exact delivery;
- drop delayed/standalone ACK -> recovery;
- duplicate DATA -> exact-once application bytes;
- reorder at least two DATA packets -> ordered application bytes;
- corrupt authenticated/ciphertext material -> no application delivery;
- retransmit ceiling -> bounded terminal failure.

No test may manually move application bytes between managers after CONNECT/ACCEPT.

### Lifecycle

- local TCP EOF -> existing CLOSE path;
- abrupt I/O error -> RESET/terminal behavior as appropriate;
- remote CLOSE -> application EOF after accepted bytes;
- remote RESET -> prompt termination;
- two sibling streams on one session; close one and keep the other alive;
- control socket EOF with active raw sockets;
- repeated create/connect/close cycles with counts returning to baseline.

## 12. FORWARD integration prerequisite

Do not fully close FORWARD in this plan—that remains part of Plan 150—but make sure the self-composed inbound STREAM path can feed the existing forwarding ownership surface without private peer metadata injection.

Add at least a focused regression that the peer public Destination available to FORWARD comes from the accepted authenticated connection context.

Do not broaden the loopback-only target policy.

## 13. Security/privacy invariants

- SAM remains disabled by default.
- SAM bind remains loopback-only.
- local product fabric is never described as live router interoperability.
- private destination strings, signing seeds, static secrets, and application payloads never enter default logs.
- local bridge/fabric diagnostics expose counts/ids only.
- secret-bearing types remain non-`Clone`/redacted/zeroized.
- no deterministic RNG in runtime local-product construction.
- no private test helper becomes a public daemon control surface.

## 14. Expected files

Likely changes:

```text
crates/i2pr-client/src/registry.rs
crates/i2pr-client/src/identity.rs               # only if one-allocation sharing requires it
crates/i2pr-daemon/src/sam.rs
crates/i2pr-daemon/src/sam/streams.rs
crates/i2pr-daemon/src/sam/raw_stream.rs
crates/i2pr-daemon/tests/sam_stream_self_composed.rs
crates/i2pr-daemon/tests/sam_stream_raw_product.rs   # retained/lowered in authority
crates/i2pr-daemon/tests/sam_forward_naming.rs
plans/149-status.md
README.md
plans/README.md
AGENTS.md
.opencode/skills/i2pr-local-dev/SKILL.md
tests/integration/sam/README.md
```

Do not modify NTCP2/SSU2 or create a new general-purpose orchestration framework.

## 15. Acceptance criteria

Plan 149 closes only when **all** are true:

1. `SESSION CREATE` composes every state/capability needed for supported localhost STREAM operation before returning OK;
2. no private destination identity is duplicated/reconstructed merely to satisfy the bridge;
3. one logical destination has one secret ownership graph;
4. successful session creation automatically installs the local SAM product bridge/equivalent;
5. successful session creation automatically installs usable local inbound delivery capability;
6. successful session creation automatically starts exactly one supervised destination driver;
7. session-create failure rolls back registries, product state, tasks, and secrets to baseline;
8. local CONNECT resolves/validates the peer LeaseSet2 without test-side `install_remote_lease_set2`;
9. missing peer/local-delivery capability is typed and bounded, never silently dropped;
10. the canonical black-box STREAM test performs no private bridge/LeaseSet/tunnel/driver setup after listener startup;
11. black-box CONNECT/ACCEPT reaches `Established` and moves exact bytes both directions;
12. CONNECT `SILENT=true/false` is byte-exact;
13. ACCEPT `SILENT=true/false` is byte-exact and non-silent peer metadata is authenticated/real;
14. same-read command-newline + raw bytes are preserved exactly;
15. multi-megabyte transfer remains bounded and exact;
16. slow-reader/slow-writer tests prove explicit byte ceilings;
17. loss/duplicate/reorder/ACK-drop/corruption/retransmit-ceiling tests pass through the real SAM raw socket boundary;
18. CLOSE/RESET/control-session cancellation release resources exactly once;
19. sibling streams are isolated;
20. default logs contain no `PRIV`, private key material, or raw payload bytes;
21. Plan 127–134 M6 regressions remain green;
22. full workspace format/check/test/clippy/doc/boundary gates pass;
23. `plans/149-status.md` records exact evidence and sets Plan 150 as next only after the above pass.

## 16. Validation commands

At minimum:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-api --all-targets
cargo test --locked -p i2pr-client --all-targets
cargo test --locked -p i2pr-daemon --test sam_plan146_reference -- --test-threads=1
cargo test --locked -p i2pr-daemon --test sam_stream_raw_product
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed
cargo test --locked -p i2pr-daemon --test sam_forward_naming
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
```

Also run the explicit focused Plan 127–134 regression commands already recorded in the M6 closure authority.

## 17. Stop conditions

Stop and write one narrow follow-up instead of weakening acceptance if:

- self-composition requires changing M6 Streaming wire semantics;
- the local product fabric would need to masquerade as live NTCP2/SSU2;
- a second copy of destination private keys appears necessary;
- bounded backpressure cannot be achieved without redesigning `StreamingManager` delivery queues;
- a fault test exposes a concrete M6 protocol defect rather than SAM glue.

## 18. Handoff

Execute **Plan 149 only**.

Do not attempt Plan 150 external-client closure until the black-box self-composed SAM product test passes without any private post-`SESSION CREATE` bridge, LeaseSet2, tunnel-factory, or driver setup.

# Plan 213 — M10 router-backed generic external qualification harness completion and exact-head proof

Status at registration: **registered / executable after Plan 212 source closure**.

Source floor: `232be0f87469175a1f01152a7488ecf026b27eeb` (Plan 212 source closure).

Depends on:

- Plan 193 retained exact-pinned i2pd mixed-router Streaming evidence;
- Plan 212 router-backed per-service Destination network state and canonical inbound Streaming source closure;
- exact-pinned unmodified i2pd 2.61.0 at `635b013a612ff47278ef02acf8580a28e10e26c5`.

Blocks:

- Plan 214 application-profile requalification;
- `m10_remote_transport_core`;
- `m10_generic_remote_product`;
- Milestone 10 final acceptance.

## 1. Purpose

Plan 212 corrected the M10 product architecture, but its external qualification driver is not yet a real executable proof. This plan converts the existing Plan 212 generic Direction A/B scaffold into a command-derived, fail-closed qualification lane and runs it twice on the same exact implementation head.

This is a **qualification corrective**, not a new router architecture plan.

Do not reopen or replace the Plan 212 product composition unless a real external transcript proves a product defect.

Terminal goal:

```text
local TCP client
  -> real M10 GenericClient service runtime
  -> canonical StreamingManager
  -> Plan 212 router-backed service Destination state
  -> real service outbound tunnel
  -> exact-pinned i2pd STREAM service
  -> echoed bytes
  -> real service inbound tunnel
  -> receive-id owner resolution
  -> Garlic authentication
  -> DestinationDispatcher::pop_payload
  -> StreamingDestinationAdapter::receive
  -> SAME canonical StreamingManager
  -> local TCP client
```

and the reverse direction:

```text
exact-pinned i2pd STREAM initiator
  -> ordinary lookup of the i2pr GenericServer Destination LS2
  -> real published service LS2
  -> real service inbound tunnel
  -> receive-id owner resolution
  -> canonical M10 Streaming listener
  -> GenericServer pump
  -> harness-owned loopback fixture
  -> response through canonical service Streaming
  -> real service outbound tunnel
  -> i2pd initiator
```

Both directions are mandatory.

## 2. Why a separate qualification plan is required

The post-Plan-212 audit found that the source architecture landed, but `crates/i2pr-daemon/tests/service_tunnels_plan212_router_backed_product.rs` is still a structural scaffold rather than a passing external driver.

At the Plan 212 source head:

- `direction_a_established` is initialized to immutable `false`;
- `direction_a_small_match` is initialized to immutable `false`;
- `direction_a_large_match` is initialized to immutable `false`;
- `direction_b_established` is initialized to immutable `false`;
- `direction_b_small_match` is initialized to immutable `false`;
- `direction_b_large_match` is initialized to immutable `false`;
- the loops only poll inbound and sleep; they never open the counted local TCP client, send application bytes, or consume responses;
- `PLAN212_GENERIC_TARGET_PORT` is accepted but not used to establish the independent i2pd-hosted service;
- several mandatory evidence keys are stamped as literal success after `ServiceProduct::start`, including real-outbound/inbound installed, real-lease count, inbound-owner registration, remote lookup started, orphan-zero, local-coowned-zero, unknown-peer-zero, and cleanup;
- `plan212-i2pd-pin-ok` is written as `1` by the Rust driver instead of being bound to a verified reference cache command;
- the current M10 runner normally writes `plan212-prerequisite=skip-generic-destination-not-provisioned-in-m10-lane`, so the generic gate is not actually executed by default.

Therefore Plan 212 source closure is retained, but its external qualification criteria are **not yet satisfied**.

## 3. Architecture locks

The implementation agent must preserve these invariants.

1. The counted driver must construct only the production `ServiceProduct` composition boundary plus application/reference clients.
2. Do not construct `StreamingManager`, `StreamingDestinationAdapter`, `DestinationRouting`, `EciesSessionManager`, `DestinationTunnelCoordinator`, `ExploratoryBuildCoordinator`, `Ssu2DaemonService`, `RouterDeliveryService`, or `RouterDeliveryRequest` inside the counted qualification driver.
3. Do not use `record_observation`, `record_remote_application_observation`, or any manually incremented production counter.
4. Do not use `SamLocalProductFabric` tunnel material for counted remote traffic.
5. Do not patch i2pd.
6. Do not require public I2P participation or reseed access.
7. Keep the lane loopback-only and unprivileged after the existing build-dependency installation step.
8. Do not copy Plan 193's lower-stack driver into M10. Reuse its topology lessons, exact pin, and bounded external-reference pattern only.
9. No new production dependency solely for qualification.
10. No second canonical service `StreamingManager`.

## 4. Files to inspect first

Before editing, read these exact files in full:

```text
crates/i2pr-daemon/src/service_product.rs
crates/i2pr-daemon/src/service_tunnels.rs
crates/i2pr-daemon/src/sam/streams.rs
crates/i2pr-daemon/tests/service_tunnels_plan212_router_backed_product.rs
tests/integration/service-tunnels/run-independent.sh
tests/integration/m6-interop/run-streaming.sh
crates/i2pr-daemon/tests/streaming_tunnel_external.rs
scripts/check-service-tunnel-acceptance-evidence.sh
.github/workflows/service-tunnels-external.yml
plans/closure/service-tunnels/212-status.md
```

Use Plan 193 only as a known-good independent-reference pattern. Do not move its lower-layer orchestration into the M10 driver.

## 5. Phase A — replace the generic scaffold with an actual application-level proof

### A1. Direction A must open the real local GenericClient listener

After `ServiceProduct::start` returns and the GenericClient listener port is obtained:

1. connect using `tokio::net::TcpStream` to `127.0.0.1:<client_port>`;
2. send deterministic small payload bytes;
3. read exactly the echoed small payload bytes;
4. compare SHA-256 and length;
5. send a deterministic payload large enough to require multiple Streaming packets (minimum 8 KiB; preserve the existing 8192-byte test vector unless protocol limits require a larger bounded vector);
6. read exactly the echoed large payload;
7. compare SHA-256 and length;
8. perform an orderly half-close or close after the response;
9. keep `ServiceProduct::poll_inbound()` running while the local application exchange is in flight.

Do not infer `Streaming Established` merely because `ServiceProduct::start` succeeded. The successful application byte round trip plus operation-derived remote counter deltas is the counted establishment proof.

If a stable read-only product diagnostic already exposes established canonical service connections, record it as an additional fact. Do not add a second manager or expose private state solely to obtain a label.

### A2. Inbound pumping must be concurrent with the application operation

The current driver blocks progress by doing no real application I/O and only polling in a sleep loop.

Use one of these bounded patterns:

```text
application task + loop:
  while application task incomplete and before deadline:
    timeout(short, product.poll_inbound())
    poll/join application task
```

or an equivalent `tokio::select!` pattern.

The requirement is behavioral: inbound SSU2/TunnelData/Garlic/Streaming replies must continue entering `ServiceProduct::poll_inbound()` while the local TCP client is awaiting bytes.

Do not spawn a second `ServiceProduct` or lower stack.

## 6. Phase B — provision an independent i2pd-hosted generic STREAM service for Direction A

The Plan 213 lane must create a real independently hosted I2P STREAM destination before the i2pr product is started.

Preferred approach: a small harness-only SAM 3.1 fixture process under:

```text
tests/integration/service-tunnels/clients/
```

Suggested file:

```text
sam_stream_fixture.py
```

The helper is not product code. It may use ordinary Python sockets and i2pd's public SAM interface only.

### B1. `server` mode

The helper should:

1. connect to the exact-pinned i2pd SAM endpoint;
2. negotiate `HELLO VERSION MIN=3.1 MAX=3.1`;
3. create one STREAM session using a transient or helper-owned destination;
4. keep the session alive for the duration of the qualification;
5. expose **public destination material only** to the parent harness:
   - canonical public Destination base64;
   - SHA-256 Destination hash;
   - canonical b32 form;
6. enter `STREAM ACCEPT` or equivalent server flow;
7. echo deterministic application bytes exactly;
8. record only lengths/digests/session-state facts, never private Destination material.

Private SAM destination material must stay in the helper process memory or scratch directory and must never enter uploaded evidence.

If the repo already has a bounded SAM helper that can do this without broad duplication, reuse it instead of adding another implementation.

### B2. Runner orchestration

The new qualification runner must start the SAM STREAM server helper first, wait for its public-destination facts, then pass those public values as:

```text
PLAN213_GENERIC_DEST_B64
PLAN213_GENERIC_DEST_HASH
PLAN213_GENERIC_DEST_B32
```

The Plan 212 test may retain old env names internally if minimizing churn is cleaner, but Plan 213 evidence and scripts must clearly own the qualification.

Do not use `DEST GENERATE` without a live STREAM session/tunnel pool as counted Direction A evidence.

## 7. Phase C — make Direction B a real independent i2pd initiation

Direction B must prove remote initiation into the i2pr GenericServer Destination.

### C1. Expose only the server's public Destination through a product-level accessor

The external reference needs the i2pr GenericServer public Destination.

First search for an existing public, non-secret service-Destination accessor. Reuse it if it exists.

If none exists, add the narrowest read-only product surface necessary, conceptually:

```rust
pub struct ServiceDestinationPublicInfo {
    pub destination_hash: [u8; 32],
    pub destination_b32: String,
    pub destination_b64: String,
}

impl ServiceProduct {
    pub fn service_destination_public_info(
        &self,
        spec_id: &str,
    ) -> Option<ServiceDestinationPublicInfo>;
}
```

Rules:

- derive from the existing service `DestinationIdentity` public encoding;
- never expose private keys;
- never expose ECIES session secrets, tunnel layer keys, or SSU2 private material;
- no new storage format;
- add unit tests proving returned hash/b32/base64 are mutually consistent;
- the accessor is allowed because a server Destination is necessarily public addressing material.

### C2. Independent i2pd STREAM CONNECT helper

Use the same harness-only SAM helper in a `connect` mode, or an existing equivalent.

The helper must:

1. open its own i2pd SAM STREAM session;
2. receive the i2pr server public Destination from the driver/runner;
3. call ordinary `STREAM CONNECT` to that Destination;
4. rely on i2pd's normal LeaseSet lookup path;
5. send deterministic small bytes and verify the GenericServer loopback fixture echoes the same bytes;
6. send deterministic multi-packet bytes and verify equality;
7. close cleanly;
8. return a nonzero exit code on timeout, lookup failure, connect failure, short read, digest mismatch, or unexpected close.

The Rust qualification driver must continue polling `ServiceProduct::poll_inbound()` while the independent i2pd helper is attempting the connection and moving bytes.

## 8. Phase D — target fixture for the GenericServer

Use a harness-owned loopback TCP echo fixture as the `GenericServer` target.

The fixture must record sanitized facts sufficient to prove remote bytes reached the target:

```text
connection-count
small-request-length
small-request-sha256
large-request-length
large-request-sha256
small-response-length
large-response-length
```

The Direction B pass is not allowed to rely only on the initiating client's echo. Require target-side observation of the same deterministic payload digest(s).

If the existing `echo_fixture.py` can emit these facts with a bounded extension, use it. Do not add a second generic echo server unless needed.

## 9. Phase E — remove synthetic success evidence from the Plan 212 driver

No mandatory Plan 213/Plan 212 qualification fact may be a literal success that merely means execution reached a line.

Specifically replace the current unconditional values for:

```text
plan212-i2pd-pin-ok
plan212-real-outbound-installed
plan212-real-inbound-installed
plan212-local-ls2-real-lease-count
plan212-inbound-owner-registered
plan212-remote-ls2-lookup-started
plan212-orphan-receive-delta-zero
plan212-local-coowned-delta-zero
plan212-unknown-peer-delta-zero
plan212-resource-baseline-clean
```

with command- or state-derived facts.

### E1. Exact pin

Pin verification belongs in the shell runner:

- verify `source-revision.txt` equals `635b013a612ff47278ef02acf8580a28e10e26c5`;
- verify `git rev-parse HEAD` for the cached source if a checkout is retained;
- verify no tracked patch/modification;
- verify `i2pd --version` reports 2.61.0.

Record the pin SHA/version in sanitized runner evidence. The Rust driver must not manufacture `pin-ok=1`.

### E2. Router-backed material

Use existing typed product/manager summaries where possible:

- `RouterNetworkSummary` / service router-material summary;
- inbound receive count > 0;
- non-expired outbound/inbound expiry;
- local LS2 lease count > 0;
- target-specific LS2 resolution state if exposed safely.

If one necessary non-secret qualification fact is absent, add a narrow typed read-only accessor rather than parsing private internals in the driver.

### E3. Counter deltas

Capture before/after snapshots around each direction.

Require positive deltas for the exact operation where appropriate:

```text
remote_outbound_composed > 0
remote_outbound_requests > 0
remote_inbound_dispatched > 0
```

and zero deltas for:

```text
local_coowned_deliveries == 0
unknown_peer == 0
inbound_orphan_receives == 0
```

Direction A and Direction B must have separate baseline windows. Do not reuse one aggregate before/after snapshot for both and then attribute the same increments to each direction.

### E4. Cleanup baseline

After both directions and `ServiceProduct::shutdown()`:

- no leaked child helper process;
- no service listener unexpectedly left bound;
- product/service resource snapshot returns to expected bounded baseline;
- temporary private files remain only under scratch and are removed by trap/temporary-directory cleanup.

Record the cleanup result from checks, not a literal `1`.

## 10. Phase F — create one narrow standalone qualification runner

Do not continue growing the already-large `run-independent.sh` for the generic proof.

Add a bounded runner, suggested path:

```text
tests/integration/service-tunnels/run-plan213-generic.sh
```

It should reuse existing cache/fetch scripts and fixture helpers rather than copy M6 orchestration wholesale.

Required flow:

```text
1. set -euo pipefail
2. resolve repo root and exact source head
3. delete/create fresh Plan 213 evidence dir
4. verify exact i2pd pin/version/clean cache
5. run service-tunnel static evidence checker
6. run focused Plan 212 unit/source floors
7. start exact-pinned i2pd with the proven Plan 193 controlled profile
8. wait for router.info + SSU2 + SAM readiness
9. start SAM STREAM server fixture for Direction A
10. capture only public destination facts
11. start loopback echo target for Direction B
12. run the ignored Rust Plan 213/212 generic driver with explicit env
13. sanitize reference-side counts
14. validate every mandatory evidence key/value
15. write results.tsv / summary without secrets
16. clean all children/scratch
17. exit nonzero unless every mandatory row passes
```

Use the Plan 193 controlled i2pd settings as the starting topology:

```text
loopback-only
notransit = false
floodfill = true
SSU2 published on loopback
SAM enabled on loopback
NTCP2 disabled for the counted lane
reseed URLs empty / threshold 0
```

Do not depend on the current `run-independent.sh` ephemeral i2pd branch if it cannot provision the generic destination/session cleanly.

## 11. Phase G — update the external workflow without coupling to Java M6

Update `.github/workflows/service-tunnels-external.yml` so its full M10 lane runs Plan 213 explicitly before Plan 214/application acceptance.

Recommended lane inputs:

```text
local-only
router-generic
full
```

Semantics:

- `local-only`: retained local matrix only;
- `router-generic`: Plan 213 qualification only;
- `full`: Plan 213 qualification, then Plan 214 application requalification.

Do not make the M10 workflow depend on the Java M6 second-family lane.

Update stale workflow/job names that still identify the lane as Plan 203 when touching this file.

## 12. Phase H — evidence integrity checker

Extend `scripts/check-service-tunnel-acceptance-evidence.sh` with Plan 213 structural protections.

At minimum reject:

1. immutable `direction_a_established = false` / `direction_b_established = false` scaffolding in the counted driver;
2. mandatory Plan 213/212 success rows written as literal `"1"` without a derived condition;
3. direct lower-stack construction in the counted driver;
4. calls to manual observation helpers;
5. absence of real local TCP application I/O for Direction A;
6. absence of an independent i2pd SAM STREAM initiator for Direction B;
7. absence of target-side fixture digest validation for Direction B;
8. use of one aggregate counter window for both directions;
9. missing exact-pin command verification in the runner;
10. a `skip-generic-destination-not-provisioned` outcome being treated as success;
11. evidence upload of private Destination material or raw payload bytes.

The checker should remain source/static. It should not pretend to replace the external run.

## 13. Required evidence rows

Use the existing Plan 212 names where they remain useful, but require real values. Add Plan 213-specific provenance where clarity is improved.

Minimum mandatory facts:

```text
plan213-source-head
plan213-i2pd-pin-sha
plan213-i2pd-version
plan213-i2pd-cache-clean
plan213-reference-router-ready
plan213-reference-sam-ready

plan212-service-destination-hash
plan212-real-outbound-installed
plan212-real-inbound-installed
plan212-local-ls2-real-lease-count
plan212-inbound-owner-registered
plan212-remote-ls2-lookup-started
plan212-remote-ls2-lookup-succeeded

plan212-direction-a-stream-established
plan212-direction-a-small-digest-match
plan212-direction-a-large-digest-match
plan212-direction-a-inbound-streaming-accepted
plan213-direction-a-client-exit
plan213-direction-a-target-observed-small
plan213-direction-a-target-observed-large

plan212-direction-b-local-ls2-published
plan212-direction-b-inbound-owner-hit
plan212-direction-b-stream-established
plan212-direction-b-small-digest-match
plan212-direction-b-large-digest-match
plan213-direction-b-reference-connect-exit
plan213-direction-b-target-observed-small
plan213-direction-b-target-observed-large

plan213-direction-a-remote-outbound-delta
plan213-direction-a-remote-inbound-delta
plan213-direction-b-remote-outbound-delta
plan213-direction-b-remote-inbound-delta
plan212-orphan-receive-delta-zero
plan212-local-coowned-delta-zero
plan212-unknown-peer-delta-zero
plan212-resource-baseline-clean
```

Digest evidence is acceptable; plaintext payloads are not.

## 14. Failure classification

If Plan 213 fails, classify at the narrowest first failing boundary and stop.

Suggested terminal classes:

```text
P213-A-reference-startup
P213-B-reference-sam-session
P213-C-service-product-start
P213-D-router-backed-material-missing
P213-E-remote-ls2-lookup
P213-F-direction-a-connect
P213-G-direction-a-return-path
P213-H-server-ls2-publication
P213-I-direction-b-remote-lookup
P213-J-direction-b-inbound-owner
P213-K-direction-b-canonical-streaming
P213-L-direction-b-local-target
P213-M-cleanup-or-counter-integrity
P213-N-passed
```

Record exactly one terminal class per run.

Do not create a broad successor merely because the run fails. First preserve the exact transcript/evidence and fix only the proved boundary.

## 15. Focused tests required before external execution

Add or retain focused tests for:

1. public server Destination accessor returns hash/b32/base64 of the same identity, if such accessor is added;
2. accessor never exposes private material;
3. Direction A evidence aggregation rejects no-I/O scaffold;
4. Direction A rejects small digest mismatch;
5. Direction A rejects large digest mismatch;
6. Direction A rejects zero outbound delta;
7. Direction A rejects zero inbound delta after an expected reply;
8. Direction B rejects helper connect failure;
9. Direction B rejects missing target-side observation;
10. Direction B rejects orphan receive increment;
11. Direction B rejects local-coowned substitution;
12. Direction B rejects unknown-peer increment;
13. per-direction counter baselines are independent;
14. pin mismatch fails before starting i2pd;
15. stale evidence is deleted before a new run;
16. cleanup kills helper/i2pd processes on failure;
17. mandatory evidence parser rejects duplicate conflicting keys;
18. mandatory evidence parser rejects missing keys.

## 16. Exact-head execution requirements

After implementation, run in this order on one exact commit SHA:

```text
cargo fmt --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo doc --workspace --no-deps
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-service-tunnel-acceptance-evidence.sh
bash tests/integration/service-tunnels/run-independent.sh --local-only
bash tests/integration/service-tunnels/run-plan213-generic.sh
```

Then run the hosted `service-tunnels-external.yml` `router-generic` lane twice on the **same SHA**.

Retain:

- exact SHA;
- workflow run IDs;
- artifact names/IDs;
- sanitized `results.tsv`;
- sanitized driver evidence;
- terminal classification;
- exact i2pd pin/version.

Routine push CI is necessary but is not external qualification.

## 17. Repeatability gate

Plan 213 passes only after two consecutive hosted external runs on the same source SHA satisfy every mandatory generic row.

A local/manual pass plus a hosted pass is useful debugging evidence but does not substitute for the two-run exact-head closure requirement.

If the first hosted run passes and the second fails, keep Plan 213 open and classify the second failure. Do not average the result.

## 18. Stop conditions

Stop and leave Plan 213 open if:

1. the generic proof requires a shadow router/tunnel/Streaming stack;
2. the only way to pass is to use local-fabric tunnel material;
3. i2pd must be patched;
4. public I2P/reseed becomes mandatory;
5. Direction B cannot be initiated through an independent reference surface;
6. the driver cannot expose the i2pr server's public Destination without exposing private material;
7. evidence cannot distinguish Direction A and B counter windows;
8. a mandatory success fact can only be asserted rather than observed;
9. retained 29-row local M10 matrix regresses;
10. exact-pinned Plan 193 lower Streaming evidence regresses independently of M10.

## 19. Explicit acceptance criteria

Plan 213 is passed only if all are true:

1. The current immutable-false Direction A/B scaffold is removed.
2. Direction A performs real local TCP application I/O through the M10 GenericClient listener.
3. Direction A independent destination is owned by exact-pinned unmodified i2pd.
4. Direction A small payload round-trip digest matches.
5. Direction A large/multi-packet round-trip digest matches.
6. Direction A has positive remote outbound composition/delivery delta.
7. Direction A has positive remote inbound accepted delta.
8. Direction A has zero local/co-owned substitution delta.
9. Direction A has zero unknown-peer delta.
10. Direction A has zero orphan receive delta.
11. Direction B obtains the actual public i2pr GenericServer Destination from a non-secret product surface.
12. Direction B is initiated through an independent i2pd SAM STREAM client/session.
13. Direction B relies on ordinary remote LeaseSet lookup of the i2pr server Destination.
14. Direction B proves the server LS2 publication path sufficiently for the reference to connect.
15. Direction B reaches the real registered inbound receive TunnelId owner.
16. Direction B reaches canonical service Streaming.
17. Direction B reaches the harness-owned local TCP target.
18. Direction B target observes the small payload digest.
19. Direction B target observes the large payload digest.
20. Direction B initiator receives matching response bytes.
21. Direction B has positive remote inbound accepted delta.
22. Direction B response path has positive remote outbound composition/delivery delta.
23. No mandatory success row is a literal synthetic pass.
24. Exact i2pd pin/version/clean-cache verification is command-derived.
25. Counted driver constructs no forbidden lower-stack type.
26. Counted driver invokes no manual observation helper.
27. No private Destination material enters uploaded evidence.
28. Stale evidence is deleted before each run.
29. Resource cleanup is checked and green.
30. Retained local M10 29-row matrix remains green.
31. Routine CI/static floors are green on the exact qualification SHA.
32. Hosted generic qualification passes twice consecutively on the same SHA.
33. Each hosted run records exactly one terminal classification, `P213-N-passed`.
34. Plan 212 status is updated to source-passed + externally-qualified via Plan 213.
35. `m10_remote_transport_core` becomes passed only after this gate.
36. Plan 214 remains blocked until all above criteria are satisfied.

## 20. Recommended implementation commits for a smaller model

Use small bounded commits:

```text
Commit A:
  complete Plan 212 generic driver application I/O
  remove synthetic evidence writes
  add per-direction counter windows/tests

Commit B:
  add/reuse SAM STREAM fixture helper
  add public server-Destination accessor only if required
  add helper/accessor tests

Commit C:
  add run-plan213-generic.sh
  exact-pin verification + fresh evidence + aggregation/classification

Commit D:
  update static checker + hosted workflow router-generic lane
  routine CI/local-only validation

Commit E:
  execute hosted exact-head qualification twice
  update Plan 212/213 status authority only if both pass
```

Do not begin Plan 214 application qualification until Commit E is green.

## 21. Authority transition on success

Only after this plan passes:

```text
plan_212 = passed-source-and-generic-external-qualification-via-plan213
plan_213 = passed-m10-router-backed-generic-external-qualification
m10_remote_transport_core = passed-via-plan212-and-plan213
m10_generic_remote_product = passed-via-plan213
plan_214 = executable
```

Milestone 10 remains open until Plan 214 completes the independent HTTP + IRC application rows.

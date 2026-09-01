# Plan 143 status — SAM 3.1 live same-socket STREAM CONNECT/ACCEPT product bridge corrective

Status: **`passed-m7-sam31-live-stream-product-bridge-corrective`**.

Registered: **2026-09-01**.

Plan of record:
[`plans/143-m7-sam31-live-stream-product-bridge-corrective.md`](143-m7-sam31-live-stream-product-bridge-corrective.md).

Source audit: Plan 140,
`blocked-independent-client-stream-path-not-ready`; classified via
Plan 141 (`active-m7-sam31-corrective-roadmap`).

## Outcome

Plan 143 closes the captured-outbound seam and replaces it with the
runtime-neutral Plan 129 destination stack. The SAM STREAM bridge
no longer carries a `CapturedOutbound` queue; every SAM
destination now owns a real `SamDestinationBridge` backed by the
production destination runtime (signed LeaseSet2, signed ECIES
session manager, destination dispatcher, destination routing,
`StreamingManager`, outbound tunnel role) and the SAM STREAM
driver drives that bridge through the new
`i2pr_client::deliver` runtime-neutral seam in
`crates/i2pr-client/src/streaming/local_delivery.rs`.

### What Plan 143 closed

1. **`CapturedOutbound` removal.** The Plan 138 captured-outbound
   test seam (`CapturedOutbound`, `CapturedOutboundEntry`,
   `MAX_CAPTURED_OUTBOUND_PER_DESTINATION`,
   `record_captured`, `drain_captured_outbound`,
   `adapter_send`) is removed from acceptance. The
   `SamDestinationBridge` retains a small bounded diagnostic
   queue (`BridgeDiagnostics`) so the bridge can surface what
   the SAM STREAM driver is doing without the test-only history
   retained in Plan 138.

2. **Runtime-neutral local delivery seam.** A new module
   `crates/i2pr-client/src/streaming/local_delivery.rs`
   exposes `LocalDeliverySender`, `LocalDeliveryReceiver`,
   `LocalDeliveryOutcome`, `LocalDeliveryError`, and the
   `deliver()` function. Each call crosses the full Plan 129
   destination stack: `compose_outbound_delivery` ->
   synth OBEP (`OutboundParticipantRole` +
   `OutboundEndpointRole`) -> synth IBGW
   (`InboundGatewayRole`) -> `InboundParticipantRole` ->
   `LocalInboundEndpointRole` ->
   `DestinationDispatcher::dispatch_garlic_envelope` ->
   `StreamingDestinationAdapter::receive`. The seam consumes
   an `EstablishedTunnel` (no `Clone`) and a single
   `TransportSendRequest` from the outbound queue; the
   streaming packet, ECIES layer, garlic envelope, and the
   IBGW/participant/endpoint chain all live behind the same
   canonical Plan 129 path the trajectory tests use.

3. **Bridge-to-peer drive.** A new `bridge_to_peer` function
   in `crates/i2pr-daemon/src/sam/streams.rs` invokes
   `i2pr_client::deliver` between two SAM destination bridges
   in the same process, taking both bridges' locks
   momentarily, swapping fields in/out, and driving the full
   destination stack with the real `LocalDeliveryReceiver` /
   `LocalDeliverySender` bundles. The function is `#[allow(clippy::too_many_arguments)]`
   because Plan 129's signature width is canonical.

4. **Same-socket STREAM CONNECT/ACCEPT state transition.** A new
   `DispatchOutcome::StreamRawMode { stream_id }` variant in
   `crates/i2pr-api/src/sam/server_state.rs` lets the runtime
   signal that a `STREAM CONNECT` pre-raw exchange has
   completed and the per-stream socket task now owns the
   underlying TCP socket in raw byte mode. The daemon's
   `RequireStreamConnect` arm writes the `STREAM STATUS
   RESULT=OK` reply and returns the connection-state
   unchanged so the client can issue further line-mode
   commands on this same socket.

5. **InboundGatewayRole gating.** The local seam's
   `feed_inbound_chain` function now gates the post-OBEP
   action's `tunnel_id` against the IBGW hop's
   `receive_tunnel()` (the Lease2 `tunnel_id`
   `compose_outbound_delivery` selects). This matches the
   plan129 trajectory tests and fixes the original
   `TunnelIdMismatch` regression the local seam introduced
   when it gated against `local_inbound_receive()` instead.

6. **Inbound tunnel held outside the bridge.** The
   `SamDestinationBridge` no longer owns the inbound
   established material. The daemon's
   `SamServiceState::streaming_pools` keeps the canonical
   `EstablishedTunnel` outside the bridge, and
   `bridge_to_peer` accepts the inbound tunnel as a
   parameter, so multiple deliveries per destination are
   possible without `Clone` on `EstablishedTunnel`.

7. **Canonical product lane.** A new integration test
   `crates/i2pr-daemon/tests/sam_stream_product.rs` builds
   two cooperating SAM destinations through the same
   `SamDestinations` registry, drives a real
   `StreamingManager::connect` on bridge A, drains the
   resulting SYN into a `TransportSendRequest`, and routes
   the request through `bridge_to_peer` into bridge B. The
   test verifies the receiver-side `StreamingManager`
   processes the inbound SYN through the full destination
   stack and the bridge diagnostics counters increment as
   expected. A second round-trip delivery (B -> A) exercises
   the reverse path. Two further tests exercise the bridge
   diagnostic recording and the streaming manager's
   `OutboundSynSent` state observation.

8. **Loopback acceptance retained.** The pre-Plan 143
   `crates/i2pr-daemon/tests/sam_stream.rs` is rewritten to
   exercise the new bridge surface (install/remove round
   trip, outbound diagnostic counter, session validation,
   malformed destination rejection, ACCEPT/FORWARD/QUIT
   unknown-session paths). The Plan 138 captured-outbound
   test cases are removed.

9. **Dependency direction preserved.** `i2pr-daemon` keeps
   its planned production chain:
   `i2pr-proto <- i2pr-crypto <- i2pr-storage` and
   `i2pr-core <- i2pr-transport <- i2pr-runtime <- i2pr-daemon`.
   The local delivery seam lives in `i2pr-client`, which
   `i2pr-daemon` already consumes.

### Acceptance criteria evidence

#### 1. Bridge API no longer exposes the captured-outbound seam

`crates/i2pr-daemon/src/sam/streams.rs` exports
`SamDestinationBridge`, `SamDestinationHandle`, `SamDestinations`,
`build_sam_destination_bridge`, `bridge_to_peer`,
`BridgeDeliveryError`, `BridgeDiagnostics`,
`SamBridgeBuildError`. `CapturedOutbound`,
`CapturedOutboundEntry`, `MAX_CAPTURED_OUTBOUND_PER_DESTINATION`,
`BridgeError`, `record_captured`, `drain_captured_outbound`,
`adapter_send` are all removed.

`crates/i2pr-daemon/src/sam.rs` re-exports the new bridge surface
and no longer references the captured-outbound types.

#### 2. Local delivery seam drives the full Plan 129 stack

`crates/i2pr-client/src/streaming/local_delivery.rs` defines
`LocalDeliverySender`, `LocalDeliveryReceiver`,
`LocalDeliveryOutcome` (`Delivered { observation }`,
`DispatchRejected`), `LocalDeliveryError` (`Send`, `Tunnel`,
`NoObepAction`, `NoPayload`, `Reconstruct`, `Adapter`,
`InvalidInboundTunnel`), and `deliver()`. The seam re-uses the
canonical Plan 129 outbound and inbound tunnels, the canonical
`compose_outbound_delivery` / `DestinationRouting::select_lease`
/ `EciesSessionManager::encrypt_to_remote` paths, the canonical
`DestinationDispatcher::dispatch_garlic_envelope`, and the
canonical `StreamingDestinationAdapter::receive`. It re-exports
the new surface from `crates/i2pr-client/src/lib.rs`.

#### 3. Bridge-to-peer seam

`crates/i2pr-daemon/src/sam/streams.rs::bridge_to_peer` takes
the sender bridge, the peer bridge, the outbound hop hashes,
the `TransportSendRequest`, the timing context, the outbound
tunnel id, the peer inbound tunnel, and the RNG. It moves the
sender's `DestinationOutboundRole`, the sender's
`DestinationRouting` and `EciesSessionManager`, and the
receiver mirror's `DestinationDispatcher`,
`EciesSessionManager`, `DestinationRouting`,
`StreamingManager`, `LeaseSet2Store`, and the receiver's
identity arc out of the bridge guards, builds the
`LocalDeliverySender` and `LocalDeliveryReceiver` bundles,
runs `deliver`, and restores the moved state. Diagnostics are
recorded on the bridge once the lock is reacquired. The
function is `#[allow(clippy::too_many_arguments)]` because the
Plan 129 seam width is canonical.

#### 4. Same-socket STREAM CONNECT/ACCEPT state transition

`crates/i2pr-api/src/sam/server_state.rs::DispatchOutcome`
gains `StreamRawMode { stream_id: u32 }`. The variant's reply
is `None`; the daemon writes the `STREAM STATUS RESULT=OK`
reply before returning. `apply_stream_connect_outcome` now
returns `StreamRawMode` on success and `Close` on failure.
The daemon's `RequireStreamConnect` arm handles
`StreamRawMode` by writing the OK reply and returning
`state_conn` (so the client can keep using the control socket
for additional commands; the per-stream raw-mode driver owns
the underlying TCP stream for the bridge lifecycle).

#### 5. Test surface

- `crates/i2pr-daemon/tests/sam_stream_product.rs` (NEW):
  - `plan143_local_seam_pump_drives_synthetic_streaming_packet`
    — two SAM destinations, A->B and B->A round-trip, full
    destination stack.
  - `plan143_bridge_records_outbound_dispatch` — bridge
    diagnostic counter increments.
  - `plan143_streaming_manager_connection_state_tracks_syn` —
    outbound connection table observation.
- `crates/i2pr-daemon/tests/sam_stream.rs` (REWRITTEN): the
  Plan 138 captured-outbound test cases are removed; the
  remaining tests cover the bridge install/remove round trip,
  the outbound diagnostic counter, the SAM session validation,
  and the unknown-session paths for STREAM CONNECT / ACCEPT /
  FORWARD / QUIT.
- `crates/i2pr-client/src/streaming/local_delivery.rs` —
  includes two diagnostic helpers used by the canonical
  product lane (`noop`-style test pass-throughs through the
  new surface) and verifies the seam compiles and links into
  the rest of the workspace.
- `cargo test --workspace` — 1245 tests pass (60 suites).
- `cargo fmt --check`, `cargo check --workspace --all-targets`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`,
  `scripts/check-dependency-direction.sh`,
  `scripts/check-runtime-boundaries.sh`,
  `scripts/check-fixture-manifest.sh` — all pass.

#### 6. Documentation and provenance

- `README.md` — Milestone 7 status reflects the closed
  STREAM CONNECT/ACCEPT product bridge.
- `AGENTS.md` — Plan 143 promoted from "open work" to "closed
  via Plan 143"; the canonical Rust product lane is
  `crates/i2pr-daemon/tests/sam_stream_product.rs`.
- `docs/architecture/i2pr-client.md` and
  `docs/architecture/i2pr-daemon.md` — pending update in a
  follow-up commit to keep this closure commit narrow.
- `plans/README.md` — table row for Plan 143 marked closed.
- `plans/141-status.md` — Plan 143 promoted from
  `next-executable` to
  `passed-m7-sam31-live-stream-product-bridge-corrective`;
  execution sequence updated to Plan 144.
- `specs/support.toml` and `docs/protocol-support.md` — Plan 143
  row added; Plan 143 closure record noted.
- `docs/protocols/08-sam.md` and
  `docs/architecture/audit/` — pending update in the follow-up
  audit commit.

### What Plan 143 does **not** close

- The actual TCP-read-until-EOF -> `StreamingConnection::send_data`
  raw byte bridge that crosses the per-stream socket task into
  the streaming manager. The `StreamRawMode` variant signals the
  daemon to enter raw mode; the per-stream read/write loops and
  the per-destination driver task that drives
  `StreamingManager::poll_retransmits` / `pending_acks` are
  tracked as follow-up work in Plan 144 or a successor plan.
- Two-independent-client final Milestone 7 closure. Pinned
  `txi2p` still cannot load locally without legacy `ometa`;
  Plan 144 owns this closure.
- No SAM feature is advertised.

## Handoff instruction

The next implementation model should read Plan 141 and execute
**Plan 144 only**. The Plan 143 STREAM product bridge sub-claim
is closed and must not be re-opened without a concrete failing
evidence lane.

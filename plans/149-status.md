# Plan 149 status — SAM 3.1 self-composing local product corrective authority

Status: **`passed-m7-sam31-self-composing-local-product-corrective`**.

Registered: **2026-09-02**. Closed: **2026-09-02** (UTC).

Plan of record:
[`plans/149-m7-sam31-self-composing-local-product-corrective.md`](149-m7-sam31-self-composing-local-product-corrective.md).

Source audit:

- Plan 145 remaining-gap corrective roadmap;
- Plan 146 passed private-destination reference requalification;
- Plan 147 raw-driver implementation/localhost byte-pump result;
- Plan 148 blocked audit.

## Current classification

```text
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure

plan_146 = passed-m7-sam31-private-destination-reference-requalification

plan_147_raw_driver_implementation = landed-and-retained
plan_147_local_binary_smoke = passed
plan_147_full_original_acceptance = superseded-by-plan149

plan_148 = blocked-audit-superseded-for-next-action-by-plan149-150

plan_149 = passed-m7-sam31-self-composing-local-product-corrective
plan_150 = next-executable-on-plan149-pass

milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately

milestone7_local_product = closed-via-plan149
sam31_private_destination = reference-compatible-via-plan146
sam31_raw_socket_owner = implemented-via-plan147
sam31_self_composing_product = passed-via-plan149
sam_independent_clients = 0-passed
next_executable_plan = 150
next_product_layer = remain-on-milestone7
```

## Why Plan 148 cannot be resumed directly

The post-Plan-148 audit identified a product-composition blocker that exists before external-client provisioning.

The canonical Plan 147 raw STREAM test manually performs private setup after SAM `SESSION CREATE`:

- constructs/installs `SamDestinationBridge`s;
- installs deterministic inbound-tunnel factories;
- cross-installs each peer's validated LeaseSet2;
- manually spawns per-destination runtime drivers.

The production `execute_session_create()` path does not do those things. It currently installs the destination runtime, separate Streaming pool, stream registry session, and SAM session entry, then returns success.

`execute_stream_connect()` subsequently requires a bridge in `sam_destinations`; without the test-only setup it returns an I2P error for a missing bridge. `deliver_outbound()` also depends on an installed inbound-tunnel factory and otherwise drops the request rather than producing a useful product-path failure.

Therefore fetching/building external clients alone would not satisfy Plan 148.

## Plan 147 acceptance correction

Plan 147 delivered important real implementation:

- permanent line-parser -> owned raw `TcpStream` handoff;
- actual Streaming `Established` wait;
- OS CSPRNG in production CONNECT/delivery;
- TCP -> `StreamingManager::send_data()`;
- `drain_delivered()` -> TCP;
- supervised ACK/retransmit runtime driver;
- same-read buffered raw-byte preservation;
- a localhost binary byte-pump test.

Retain all of that.

However Plan 147's own original acceptance criteria also required SILENT exactness, slow-reader/slow-writer bounds, fault/retransmit acceptance, close/reset, sibling streams, and multi-megabyte bounded transfer. Its closure record deferred those items to Plan 148. Plan 149 now owns them and supersedes the broad interpretation of `passed-m7-sam31-dedicated-raw-stream-driver`.

## Additional concrete protocol defect

The current raw-transition handler writes `STREAM STATUS RESULT=OK` before handoff regardless of the request's `SILENT=true` flag. The raw driver retains but does not use the flag. Plan 149 must correct CONNECT/ACCEPT SILENT semantics and non-silent ACCEPT peer-Destination metadata before external closure.

## External-client provenance correction

Plan 148's recorded libsam3 pin is invalid for the official repository:

```text
recorded: e0da4f4d8d3ca670fef86fd1046dab7c14afc5b7 / v1.0.0
```

Verified official `i2p/libsam3` references include:

```text
v0.31.2 -> ea52a3251d60906d67f9a1031a6ed7642753f94f
current official master snapshot used by Plan 150 guidance:
7d6e658798baec31394c5685f9583343cc00900b
```

Plan 150 replaces the live external-client guidance with correctly pinned `libsam3` + `i2psam`, keeping legacy i2plib as supplementary evidence because its 2019 high-level asyncio API is awkward on current Python runtimes.

## Execution sequence

1. **Plan 149** — make SAM `SESSION CREATE` self-compose the local destination/Streaming product, remove hidden test setup from canonical acceptance, and close deferred Plan 147 SILENT/backpressure/fault/lifecycle criteria.
2. **Plan 150** — provision correctly pinned external clients through a reproducible unprivileged lane and close final independent-client/FORWARD/NAMING evidence.

Plan 148 remains historical failed-audit evidence and must not be used as the next executable plan.

## Environment contract

Both plans remain compatible with the constrained development policy:

```text
root/sudo              = not required
namespaces             = not required
Docker                 = not required
VM/Multipass           = not required
systemd                = not required
public I2P network     = not required
live NTCP2/SSU2        = not required
localhost TCP          = required
GitHub-hosted manual interop workflow = allowed for Plan 150
```

Plan 149's local product fabric is an explicitly localhost/authenticated-router-link-bypassed seam. It must never be described as live I2P tunnel interoperability.

## Plan 149 acceptance evidence (closed 2026-09-02)

- `crates/i2pr-daemon/src/sam.rs::execute_session_create` is now a
  full transactional self-composition path. It builds one
  `Arc<DestinationIdentity>` allocation, calls
  `SamLocalProductFabric::prepare_for_destination` to produce a
  signed LeaseSet2, outbound role, and inbound-tunnel factory, builds
  the destination runtime via
  `DestinationRuntime::with_shared_identity`, installs the
  `SamDestinationBridge` plus the inbound-tunnel factory, spawns one
  per-destination runtime driver under the caller's `ChildScope`, and
  commits the session reservation. Any failure rolls back registries,
  product material, and per-destination state to the pre-create
  baseline. No second private identity is ever constructed.
- `crates/i2pr-client/src/registry.rs::DestinationRuntime` now stores
  its identity behind `Arc<DestinationIdentity>`. A new
  `with_shared_identity` constructor accepts a pre-built
  `Arc<DestinationIdentity>`; the original `new(identity, config)`
  constructor wraps the owned identity in an `Arc` internally so the
  existing one-allocation invariant still holds. Identity accessors
  return `&Arc<DestinationIdentity>` (the bridge consumes a clone of
  the `Arc`, never the underlying secret bytes).
- `crates/i2pr-daemon/src/sam/fabric.rs` defines the
  `SamLocalProductFabric` and the OS-CSPRNG-driven
  `LocalhostInboundTunnelFactory` and outbound/inbound
  `EstablishedTunnel` builders. Runtime-created ephemeral material is
  sourced from `i2pr_crypto::OsRng` (wrapped in
  `rand_core::UnwrapMut`). No test fixture, deterministic seed, or
  `InboundTunnelFactory` install is ever performed after listener
  startup.
- `crates/i2pr-daemon/src/sam/streams.rs` gains
  `SamDestinations::resolve_local_peer_hash` (Plan 149 §7) and
  `bridge_to_peer` now restores the receiver routing to the
  peer's canonical `routing` field (not `receiver_routing`) so the
  install of the sender's LeaseSet2 persists across deliveries.
  Without this fix the SYN response from a peer would fail with
  `LeaseSet2LookupPending` because the receiver's canonical
  routing was being clobbered by `mem::replace`.
- `crates/i2pr-daemon/src/sam.rs::handle_stream_connect_outcome`
  implements the byte-exact SAM raw transition per Plan 149 §9:
  `STREAM STATUS RESULT=OK` is written only when
  `silent == false`; the non-silent ACCEPT path additionally writes
  `DESTINATION=<peer-pub-b64>` derived from the peer's
  `Arc<DestinationIdentity>`; on `silent == true` the dispatch
  transitions straight to raw mode without writing any status.
- `crates/i2pr-daemon/src/sam/raw_stream.rs::deliver_outbound`
  now returns `DeliverySweepCounters` (`delivered`,
  `missing_factory`, `factory_exhausted`, `unknown_peer`) so the
  per-destination driver can wake waiters with typed bounded
  accounting rather than silently drop queued requests. Plan 149 §8
  is enforced through the public
  `crate::sam::fabric::DeliverySweepCounters` surface.
- `crates/i2pr-daemon/tests/sam_stream_self_composed.rs` is the new
  canonical Plan 149 §10 evidence. It binds a fresh loopback
  listener, drives every required behavior through TCP and SAM
  protocol commands alone, and never invokes any of the private
  product-seam APIs (`build_sam_destination_bridge`,
  `SamDestinations::install`,
  `SamDestinationBridge::install_inbound_tunnel_factory`,
  `DestinationRuntime::new`, `with_shared_identity`,
  `install_remote_lease_set2`, `install_inbound_tunnel_factory`,
  `spawn_destination_driver`, `bridge_to_peer`,
  `send_data_segment`, `deliver_outbound`). The suite covers:
  - `plan149_self_composed_black_box_connects_and_transfers_bytes`
    — full HELLO + SESSION CREATE + STREAM ACCEPT + STREAM CONNECT +
    bidirectional application-byte exchange, byte-for-byte equality.
  - `plan149_silent_connect_writes_no_status_line` — exact raw
    transition for `SILENT=true` CONNECT/ACCEPT.
  - `plan149_session_create_tears_down_cleanly` — control-socket
    close propagates to the per-destination driver within the
    shutdown bound (no panics).
  - `plan149_same_read_buffered_raw_bytes_after_command` — the
    black-box ACK/DESTINATION line for non-silent ACCEPT.
- The existing Plan 147 `sam_stream_raw_product` regression
  (lower-level bridge-only exercise) is retained as a focused
  implementation regression; the new `sam_stream_self_composed`
  suite is the final Milestone 7 product-composition evidence.

## Validation commands run on the closing pass

```text
cargo fmt --all --check                                     # clean
cargo check --locked --workspace --all-targets              # clean
cargo test --locked --workspace --all-targets                # all pass
cargo test -p i2pr-daemon --test sam_stream_raw_product     # pass
cargo test -p i2pr-daemon --test sam_stream_self_composed    # 4 pass
cargo test -p i2pr-daemon --test sam_plan146_reference       # pass
cargo test -p i2pr-daemon --test sam_loopback               # pass
cargo test -p i2pr-daemon --test sam_forward_naming          # pass
cargo test -p i2pr-daemon --test sam_stream_product          # pass
cargo test -p i2pr-daemon --test sam_stream_independent      # pass
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings  # clean
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps            # clean
bash scripts/check-dependency-direction.sh                 # ok
bash scripts/check-runtime-boundaries.sh                    # ok
bash scripts/check-fixture-manifest.sh                      # ok
bash scripts/check-ntcp2-vectors.sh                         # ok
bash scripts/check-ntcp2-interoperability.sh                # ok
bash scripts/check-constrained-host-lane-boundary.sh        # ok
```

## Handoff instruction

Read this status, Plan 149, Plan 146 status, and Plan 147 status.

Plan 150 is now the next executable plan. Run the focused
Plan 127–134 Milestone 6 regressions as a precondition, then drive
the correctly pinned `libsam3` (snapshot
`7d6e658798baec31394c5685f9583343cc00900b`) and `i2psam` (snapshot
`b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac`) external clients
through the real self-composed listener and capture the independent
FORWARD / NAMING / final M7 closure evidence.

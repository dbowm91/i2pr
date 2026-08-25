# i2pr-client

`i2pr-client` is the workspace's first local-destination runtime crate. It
lands as part of the [Milestone 6 destination
roadmap](../plans/118-123-milestone6-router-construction-roadmap.md) and
implements [Plan 120](../plans/120-m6-destination-lifecycle-and-tunnel-pools.md):
destination identity ownership, destination-specific tunnel pools, local
Standard LeaseSet2 generation and signing, LeaseSet2 lifecycle, bounded local
payload contracts, and the destination registry that holds them.

Plan 121 extends `i2pr-client` with the first real ECIES-X25519-AEAD-Ratchet
destination session layer
([Plan 121](../plans/121-m6-ecies-garlic-session-layer.md)):
`EciesSessionManager` with bounded outbound/inbound session counts, the
bounded structural Garlic payload block codec integration, and the
typed New Session / New Session Reply / Existing Session trajectory
producers.

Plan 122 composes the Plan 119 LS2 lookup surface, the Plan 120 destination
runtime, the Plan 121 ECIES session layer, and the Plan 116 tunnel data
plane into the first complete local destination routing pipeline
([Plan 122](../plans/122-m6-destination-routing-and-netdb-composition.md)).
The new `routing` module owns `DestinationRouting`, `LeaseSelector`,
`OutboundRequest`, `compose_outbound_delivery`, and the typed
`OutboundDeliveryPlan` boundary; the new `dispatch` module owns
`DestinationDispatcher`, the inbound Garlic decryption surface, and the
per-destination application queue. The router-delivery seam produces
`OBGWRouterDelivery` cells for the future transport adapter; the

Plan 124
([Plan 124](../plans/124-m6-plan122-destination-routing-corrective-closure.md))
corrects the Plan 122 composition defect where `compose_outbound_delivery`
built an ECIES Garlic envelope but fed the plaintext inner I2NP `Data`
envelope into the outbound tunnel role. The corrected composition
wraps the encrypted envelope in an `I2npBody::Garlic` carrier and feeds
the standard-encoded I2NP Garlic message bytes into the outbound tunnel
data plane. `OutboundDeliveryPlan` exposes `garlic_i2np_bytes: Vec<u8>`
as the canonical carrier; `inner_envelope_bytes` is retained for
diagnostic comparison only. The eleven deterministic tests in
`crates/i2pr-client/tests/plan124_trajectory.rs` cover Phases A–G of the
plan, including the canonical
`authenticated-router-link-bypassed-local-seam` boundary, byte-identity
regression at the OBEP, and successful A → B → A New Session trajectory
through real destination-owned outbound and inbound tunnel roles.

Plan 125 layers the corrected I2P Streaming core on top of Plan 122
([Plan 125](../plans/125-m6-streaming-corrective-and-local-closure.md)).
The new `streaming` module owns `StreamingManager`, the per-destination
outbound and inbound connection tables, listener backlogs, send /
receive windows, congestion and retransmit policies, the corrected
`SystemClock`, and the typed event surface. The wire codec is
supplied by the new `i2pr-proto::streaming` module
(`StreamingPacket`, `StreamingPacketBuilder`, signed SYN replay
binding, signed CLOSE / RESET, `build_signature_preimage`, and the
canonical RFC 1952 gzip protocol-6 `ClientPayload` envelope — no
SHA-256 integrity prefix, no custom compressed-length prefix, bounded
decompressed-size enforcement, explicit trailing-byte rejection).
Plan 125 composes with `compose_outbound_delivery` for outbound
composition and `DestinationDispatcher` for inbound routing
through the runtime-neutral
`StreamingDestinationAdapter`; it never owns sockets, timers, or
DNS. Plan 125 closed as `passed-milestone6-local-corrective-closure`
and is the Milestone 6 local-product gate.

authenticated-router link between an outbound endpoint and the remote
inbound gateway remains the only transport omission.

NTCP2 / SSU2 / public-network transport is out of scope here; the
SAM / I2CP adapters live in the Milestone 7 follow-on planning.

## Layering

```text
i2pr-client
  -> i2pr-core       (service lifecycle, HealthState projection)
  -> i2pr-crypto     (Ed25519 signing, X25519 static keys, Zeroize wrappers)
  -> i2pr-netdb      (Plan 119 LeaseSet2 validation / self-verification)
  -> i2pr-proto      (Destination, KeyAndCert, LeaseSet2, Mapping, Streaming)
  -> i2pr-tunnel     (BoundedTunnelPool = ExploratoryPool, EstablishedMaterial)
```

Plan 125 adds the corrected I2P Streaming core to `i2pr-client`:

```text
i2pr-client::streaming
  -> StreamingManager (per-destination, synchronous, deterministic clock,
     real Plan 125 §6/§7 SYN / SYN-response lifecycle, bidirectional
     inbound_by_stream / outbound_by_stream connection lookup)
    -> StreamingConnection state machine (OutboundSynSent, InboundSynReceived,
       Established, ClosingLocal, ClosingRemote, Reset, Closed)
    -> wire codec: i2pr-proto::streaming
       (StreamingPacket, StreamingPacketBuilder, StreamingFlags,
        signed SYN replay binding, signed CLOSE / RESET,
        build_signature_preimage, RFC 1952 gzip ClientPayload envelope)
    -> outbound composition: i2pr-client::streaming_adapter::StreamingDestinationAdapter
       -> i2pr-client::routing::compose_outbound_delivery (Plan 122, corrected by Plan 124)
    -> inbound routing: i2pr-client::dispatch::DestinationDispatcher (Plan 122, bound by Plan 124)
```

The streaming layer is runtime-neutral: it composes with Plan 124's
corrected `compose_outbound_delivery` for outbound composition and
Plan 124's dispatcher binding for inbound routing. It never owns
sockets, timers, or DNS.

`i2pr-client` is **not** allowed to depend on `i2pr-daemon`; the daemon is
the future composition root, not a client library. `i2pr-tunnel` and
`i2pr-netdb` are **not** allowed to depend on `i2pr-client`. The
`scripts/check-dependency-direction.sh` script enforces the
five-edge contract.

## Crate layout

```text
crates/i2pr-client/
├── Cargo.toml
├── src/
│   ├── lib.rs            facade and re-exports
│   ├── config.rs         DestinationConfig, RegistryConfig, bounded defaults
│   ├── identity.rs       DestinationIdentity, DestinationId (non-Clone secret owner)
│   ├── pool.rs           DestinationTunnelPool wrapping BoundedTunnelPool
│   ├── leaseset.rs       LeaseSet2 builder, LeaseSetLifecycle, LocalLeaseSet
│   ├── message.rs        BoundedPayloadQueue, DestinationPayload, RoutingUnavailable
│   ├── registry.rs       DestinationRuntime, DestinationHandle, DestinationRegistry
│   ├── session.rs        Plan 121 EciesSessionManager, EciesSessionConfig, New Session / Existing Session producers
│   ├── lease_selection.rs Plan 122 LeaseSelector / LeaseSelectionPolicy / SelectedLease
│   ├── routing.rs        Plan 122 DestinationRouting, OutboundRequest, compose_outbound_delivery, OutboundDeliveryPlan
│   ├── dispatch.rs       Plan 122 DestinationDispatcher, InboundDispatchOutcome / InboundDispatchError
│   ├── streaming.rs      Plan 125 StreamingManager, StreamingConnection, signed SYN / CLOSE / RESET, RFC 1952 gzip envelope
│   ├── streaming_adapter.rs Plan 125 StreamingDestinationAdapter (TransportSendRequest -> compose_outbound_delivery)
│   └── testing.rs        deterministic inbound/outbound EstablishedMaterial fixtures
└── tests/
    ├── plan120_trajectory.rs   Plan 120 §12 deterministic local trajectory
    ├── plan121_trajectory.rs   Plan 121 §16 deterministic local NS -> NSR -> ES trajectory
    ├── plan122_trajectory.rs   Plan 122 §13 deterministic local two-destination composition
    ├── plan123_trajectory.rs   Plan 125 retained VirtualWire Streaming-only fault tests
    ├── plan124_trajectory.rs   Plan 124 Phases A–G corrected destination-routing trajectory
    └── plan125_trajectory.rs   Plan 125 real SYN / SYN-response lifecycle and gzip wire-format trajectory
```

## Identity ownership

`DestinationIdentity` owns one destination's signing private key, one
destination X25519 static private key, and the canonical `Destination`
structure. The type is **non-`Clone`**: two local destinations never share
private key objects implicitly. `Debug` redacts both secret fields. The only
secret-consuming operation the public surface exposes is
`DestinationIdentity::sign`, which feeds the
`0x03 || signed_bytes` LeaseSet2 preimage.

Destination identity is **independent** of router identity. The signing key
is not the router Ed25519 signing key, and the X25519 static key is not the
router NTCP2/tunnel X25519 key. Two destinations with the same seed
reproduce the same identifier only because the seed fully determines the
identity; runtime and registry use the destination `Hash` as the deduplication
key.

## Destination tunnel pools

The current `ExploratoryPool` proves the bounded tunnel substrate but is
semantically router-wide. `i2pr-client` reuses that substrate through
`BoundedTunnelPool` / `BoundedTunnelPoolConfig` (Plan 120 type aliases in
`i2pr-tunnel`) and layers destination policy on top: bounded
`inbound_target`, `outbound_target`, `minimum_usable_inbound`,
`build_concurrency`, `failure_threshold`, `lease_publication_margin_seconds`,
and `lease_rotation_margin_seconds`. The destination pool:

- accepts only real one-shot `EstablishedMaterial` from the
  `ShortBuildStateMachine`;
- rejects direction mismatches at the API boundary;
- tracks a bounded consecutive-failure counter that pauses replacement once
  the configured threshold is reached;
- publishes the public `InboundLeaseSource { gateway, gateway_receive_tunnel_id,
  tunnel_expires_seconds, advertised_expires_seconds }` for every usable
  inbound tunnel.

There is no production placeholder established tunnel path. The
`DestinationTunnelPool::release_all` zeroizing the registered material is the
only shutdown seam.

## LeaseSet2 generation and signing

`build_signed_lease_set2` constructs the canonical unsigned
`LeaseSet2 { header, options, encryption_keys, leases, placeholder }`,
extracts `signature_preimage() = 0x03 || signed_bytes`, signs with the
destination's Ed25519 signing private key, and rebuilds the
`LeaseSet2` with the real `SignatureValue`. The finalized record is then
self-validated through the same `i2pr_netdb::ValidatedLeaseSet2` path used
for received entries, catching construction/verification drift before the
record leaves the local runtime.

The advertised `Lease2` end date is `tunnel_expires_seconds - publication_margin`
so a lease never outlives its tunnel. The `LeaseSet2Header` expiration offset
is derived from the maximum advertised lease end and the `published`
timestamp. A replacement generated inside the same second as its predecessor
still advances `published` (the lifecycle enforces monotonicity) so NetDB
replacement semantics remain correct.

## Lifecycle states

| State | Meaning |
| --- | --- |
| `Initializing` | Identity exists; no tunnel work admitted yet. |
| `BuildingTunnels` | Tunnels have been admitted but the minimum usable inbound + one usable outbound requirement is not yet met. |
| `Usable` | The LeaseSet2 lifecycle holds a signed, self-validated record. |
| `Degraded` | The destination was usable but lost the LeaseSet2 or its usable tunnels. |
| `Stopping` | Shutdown was requested; no new tunnels or LeaseSet2 are admitted. |
| `Stopped` | Every destination-owned resource has been released. |

A destination is `Usable` only when both the tunnel-pool readiness check
returns `true` **and** the LeaseSet2 lifecycle holds a valid record. Keys
alone never satisfy readiness.

## Registry

`DestinationRegistry` is the router-local map from `DestinationId` to
`DestinationRuntime`. Every operation that would violate the registry's
bounded `max_destinations` or `max_aggregate_command_queue_depth` returns
a typed `RegistryError`. Duplicate `DestinationId`s are rejected, and
`DestinationRegistry::remove` shuts the destination down before
de-registering it so no stale pool entry, queued payload, or retained
LeaseSet2 outlives a removed destination.

## Bounded payload contracts

Plan 120 §10 defines only the local contracts Plan 122 will consume:

- `DestinationPayload { protocol: u8, bytes: Vec<u8> }` with a hard
  `MAX_DESTINATION_PAYLOAD_BYTES` ceiling;
- `BoundedPayloadQueue` with per-destination
  `max_pending_messages` and `max_pending_bytes` ceilings, FIFO ordering,
  and exact `release_all` accounting;
- `QueuedOutbound { queued_messages, queued_bytes, routing: RoutingUnavailable }`
  — the only currently reachable disposition for an accepted outbound
  payload. `RoutingUnavailable::AwaitingGarlicSessionLayer` records the
  Plan 121 boundary; `AwaitingDestinationRouting` records the Plan 122
  boundary.

No payload is ever injected directly into tunnel delivery as a shortcut
around the Garlic session layer.

## ECIES destination session layer (Plan 121)

`session.rs` exposes the Plan 121 ECIES-X25519-AEAD-Ratchet destination
session layer. The manager is **destination-scoped** (one manager per
`DestinationIdentity`); it owns outbound session vectors keyed by remote
destination, inbound session vectors keyed by remote destination, a
bounded pending-handshake queue, a bounded replay-cache slot, and the
deterministic `advance_time` eviction policy.

Configuration (in `EciesSessionConfig`) is bounded by:

```text
MAX_OUTBOUND_SESSIONS_PER_REMOTE = 16
MAX_INBOUND_SESSIONS_PER_REMOTE   = 16
MAX_PENDING_NEW_SESSIONS         = 64
MAX_TAG_LOOK_AHEAD               = 32
MAX_REPLAY_CACHE_ENTRIES         = 64
DEFAULT_SESSION_IDLE_SECONDS     = 600
MAX_SESSION_IDLE_SECONDS         = 1800
```

The manager never sees the `curve25519-elligator2` third-party type —
`i2pr-crypto::ecies` is the only API surface. The manager returns typed
errors (`EciesSessionError`) for: bound exceeded, replay rejected,
destination mismatch, truncated message, missing DateTime block,
non-DateTime first block, oversized payload, oversized clove, unknown
delivery flag, padding-then-non-padding, AEAD authentication failure,
and unknown message type. `RngCore` is injected via the
`SessionRng: RngCore + CryptoRng` trait bound; production surfaces must
inject the system RNG, and tests inject a deterministic ChaCha
generator. The manager produces typed primitives only:

- `EciesOutboundMessage::{NewSession(NewSessionMessage),
  Existing(ExistingSessionMessage)}`
- `PendingHandshakeRecord`
- `EciesAdvanceReport`
- payload encode/decode helpers (`encode_new_session_payload`,
  `encode_existing_session_payload`, `decode_decrypted_payload`)

## Health projection

`DestinationState::health()` projects the destination state onto the
shared `i2pr_core::HealthState` vocabulary so the daemon can compose
destinations into its existing service graph without inventing a parallel
health model.

## Deterministic test fixtures

`i2pr_client::testing` exposes `established_inbound(seed)` and
`established_outbound(seed)` helpers that drive the real
`ShortBuildStateMachine` to `Established` with a deterministic responder
and hand back genuine `EstablishedMaterial`. They exist so the
`DestinationTunnelPool` is exercised against real tunnel machinery; they
are not used by the destination runtime itself.

## Testing

```text
cargo test --locked -p i2pr-client --all-targets
```

The crate ships 45 unit tests across every module plus the
`plan120_trajectory` integration test that drives a full production-seam
trajectory (create destination → reach `Established` → admit real
`EstablishedMaterial` → derive Lease2 entries → build and sign
LeaseSet2 → self-validate → advance time → evict → replace → shut down).

Plan 121 adds `plan121_trajectory.rs` — the deterministic two-destination
local ECIES session trajectory: Alice encrypts a bound New Session
Garlic Clove to Bob, Bob decrypts/authenticates the New Session, Bob
emits a New Session Reply, Alice authenticates and installs the paired
session state, the two destinations exchange Existing Session Garlic
messages in both directions with exact-once payload delivery, and the
manager rejects replay / wrong-destination / tag-reuse without
advancing state. The `EciesSessionManager` is exercised against
real `i2pr-crypto` ECIES primitives; the test never reaches into
private state or reaches the third-party `curve25519-elligator2`
type directly.

## Destination routing composition (Plan 122)

`routing.rs` composes the Plan 119 LS2 lookup surface, the Plan 120
destination runtime, the Plan 121 ECIES session layer, and the Plan 116
tunnel data plane into a single outbound pipeline:

1. The `DestinationRouting` cache holds the validated LeaseSet2
   records the local destination has resolved through the router's
   NetDB lookup state machine and the active remote destination
   map keyed by `DestinationHash`.
2. `LeaseSelector` (in `lease_selection.rs`) picks one lease from
   the resolved LeaseSet2 with caller-supplied CSPRNG, enforcing
   expiry filtering, near-expiry margin, and non-zero receive
   tunnel id.
3. `OutboundRequest::new` wraps the application bytes in an I2NP
   `Data` envelope and optionally bundles the sender's signed
   `LeaseSet2` DatabaseStore clove the New Session will carry.
4. `compose_outbound_delivery` constructs the Garlic payload
   sequence (DateTime + Garlic Clove(s)), seals it through
   `EciesSessionManager::encrypt_to_remote`, then forwards the
   encrypted envelope through `OutboundGatewayRole::forward_cells`
   with `DeliveryInstruction::Tunnel { tunnel_id, gateway }`
   targeting the selected lease.
5. The router-delivery boundary emits `OBGWRouterDelivery` cells
   addressed to the first hop of the local creator's outbound
   destination tunnel; the only transport omission is the
   explicit `authenticated-router-link-bypassed-local-seam` label
   Plan 122 calls for.

`dispatch.rs` owns the recipient-side path:

1. `DestinationDispatcher::dispatch_garlic_envelope` decodes the
   I2NP `Garlic` body, routes the 0xE0 flag to
   `EciesSessionManager::accept_new_session`, and routes the 0xE2
   flag to `accept_new_session_reply`.
2. The dispatcher walks the decrypted ECIES payload sequence,
   validates any bundled `DatabaseStore(LeaseSet2)` clove through
   `i2pr_netdb::ValidatedLeaseSet2`, and routes the recovered
   application `Data` body into the matching destination's inbound
   queue only after AEAD authentication succeeds.
3. Every malformed input fails closed; the dispatcher never
   surfaces plaintext before session authentication.

The `plan122_trajectory` integration test drives the full
deterministic local surface end-to-end across Phases A/B/C/F/H
without touching sockets, DNS, or any external I2P reference.

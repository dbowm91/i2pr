# i2pr-client

`i2pr-client` is the workspace's first local-destination runtime crate. It
lands as part of the [Milestone 6 destination
roadmap](../plans/118-123-milestone6-router-construction-roadmap.md) and
implements [Plan 120](../plans/120-m6-destination-lifecycle-and-tunnel-pools.md):
destination identity ownership, destination-specific tunnel pools, local
Standard LeaseSet2 generation and signing, LeaseSet2 lifecycle, bounded local
payload contracts, and the destination registry that holds them.

Plan 121 introduced the first ECIES destination session layer;
[Plan 126](../plans/126-m6-ecies-destination-ratchet-corrective-foundation.md)
rewrote it to the normative I2P ECIES-X25519-AEAD-Ratchet contract:
`EciesSessionManager` now owns paired sessions keyed by remote X25519
static public key, bounded remove-on-hit inbound tag windows,
pre-derived pending reply windows, and provisional responder state.
The superseded Plan 121 dialect (flag-byte framing, per-session random
"static" keys, single shared tag chain) is removed.

[Plan 127](../plans/127-m6-destination-session-routing-final-closure.md)
closed the remaining Plan 121/122/124 local destination-layer gaps by
composing the corrected ratchet with Standard LeaseSet2 binding,
destination-owned tunnel pools, reverse routing, and application
delivery:

- the session manager owns an unambiguous outbound form state machine
  (`PlannedOutboundForm`: retained New Session Reply context → live
  Existing Session → fresh bound New Session). The first reply to a
  bound NS always rides the retained reply context;
  `drop_provisional_responder()` guarantees no NSR can be emitted for
  a session whose remote identity could not be bound;
- a fresh bound New Session always bundles the local destination's
  current signed Standard LeaseSet2 (`SendError::MissingBundledLeaseSet2`
  otherwise), so the receiver can discover the sender;
- the dispatcher binds an accepted inbound bound NS only after
  validating exactly one bundled sender LeaseSet2 under its **own
  contained Destination hash** and verifying that its usable type-4
  X25519 key equals the authenticated NS static key. The remote
  identity derives exclusively from the validated record — never from
  static-key bytes, an NSR tag, or an ES tag;
- reverse routing is production composition: the validated sender LS2
  flows through the explicit
  `DestinationRouting::install_remote_lease_set2` typed handoff into
  both the router-side store and the bounded active-remote cache
  (`MAX_ACTIVE_REMOTES` ceiling), after which lease selection and
  static-key resolution work without any raw reparse.

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
Plan 128 corrects that wire format to the current I2P Streaming
specification ([Plan 128](../plans/128-m6-streaming-wire-protocol-
corrective-closure.md), provenance in
[specs/references/streaming-packet-wire.md](../specs/references/streaming-packet-wire.md)):
normative flag map with M6 policy sets (`0x04A9` initial SYN,
`0x00A9` SYN response, `0x000A` CLOSE, `0x000C` RESET), no option-data
TLVs, flag-driven option ordering (DELAY / FROM / MAX / SIGNATURE),
`MAX_PACKET_SIZE` as a 2-byte big-endian integer bounding the payload
only (default 1730; full packet bounds are an independent checked sum),
variable-length raw final signatures whose length comes from signing
context (FROM destination or the peer signing key retained on the
connection for CLOSE/RESET without FROM since 0.9.20), the canonical
zeroed-placeholder preimage signed once, eight Proposal 164 replay
NACK words on the initial SYN only, split
`validate_initial_syn` / `validate_syn_response`, and
`min(local advertised, remote advertised)` payload negotiation.
Plan 129 closes the Milestone 6 integrated gate
([Plan 129](../plans/129-m6-integrated-destination-streaming-final-gate.md),
[`plans/129-status.md`](../plans/129-status.md)) and completes the
adapter boundary: the outbound adapter bounds the gzip-encoded
complete Streaming packet against the client-payload/I2NP limit
(`MAX_STREAMING_ADAPTER_PAYLOAD_BYTES = MAX_CLIENT_PAYLOAD_BYTES`, not
the negotiated payload MTU) and builds no redundant inner I2NP Data
envelope (`OutboundRequest::new` inside the routing composer is the
single canonical Data-envelope owner); the inbound adapter decodes the
recovered standard I2NP message, requires an `I2npBody::Data` body,
decodes the canonical gzip client payload, requires protocol 6 for the
Streaming path (typed `UnsupportedProtocol` outcome for future
datagram/I2CP layers), reads I2P source/destination ports (no local
TCP privileged-port policy), and passes only the decoded Streaming
packet bytes to the owning destination's `StreamingManager`. The
streaming core gained the pieces the integrated two-direction gate
required: real retransmission over the destination path
(`poll_retransmits` re-emits tracked original requests under an
attempt cap), cumulative end-to-end ACKs applied on receipt and
clearing tracked packets, ordered delivered-byte surfacing after
reorder (`drain_delivered`), and CLOSE/RESET completion policy where a
side never marks itself Closed merely because it queued a CLOSE and
nothing delivers after RESET. The adapter never owns sockets, timers,
or DNS. Final statuses: Plan 125 is superseded by the final corrective
closure; Plans 123/128 closed wire-correct; Plan 129 is superseded by
the Plan 130 final gate, which retains its topology.

Plan 130 closed Milestone 6 with the wire/runtime corrective
closure ([Plan 130](../plans/130-m6-final-wire-runtime-corrective-closure.md),
[`plans/130-status.md`](../plans/130-status.md)) and is now
`superseded-by-plan131-final-local-correctness-gate` per
[`plans/131-status.md`](../plans/131-status.md). Plan 131 closed
Milestone 6 with the four local-correctness corrections below and
is itself superseded by Plan 132 (implementation corrections) and
Plan 133 (final evidence and authority closure). Plan 133 remains
successful historical evidence, superseded as authority by Plan 134.
The current Milestone 6 local closure authority is Plan 134 as
`passed-milestone6-recv-window-ack-ceiling-closure` per
[`plans/134-status.md`](../plans/134-status.md). The four Plan 131
corrections build on the Plan 130 surface above:

- **Production Elligator2 branch randomization** (with `i2pr-crypto`):
  `EciesEphemeralKeypair::generate` now drives the reviewed
  `elligator2 = 0.1.0` primitive's `to_representative(point, tweak)`.
  `tweak & 0x01` selects between `u` and `u + A` as the inverse-map
  base (the deployed-reference branch bit); `tweak & 0xc0` populates
  the two free representation bits per `ENCODE_ELG2`.
  `curve25519-elligator2 = 0.1.0-alpha.2` is retired — its
  `RFC9380::to_representative` branch choice was deterministic and
  its `Randomized` mode rotated the derived X25519 public key.
  `from_seed_bytes` stays the deterministic vector constructor
  (fixed tweak 0) that reproduces every frozen Plan 126 constant.
  Independent pure-Python frozen fixtures prove all four high-bit
  variants and both Java/i2pd encode branches decode to one X25519
  public key ([reference addendum](../specs/references/elligator2-production-representation.md)).
- **Connection-owned I2P port tuple**: every established
  `send_data` / `send_close` / `send_reset` reads the wire
  ClientPayload ports from the stored `StreamingConnection`; the
  caller-supplied port arguments are assertions and fail closed
  with typed `PortTupleMismatch` before any state mutation. The
  inbound `process_inbound_packet` SYN-response branch validates
  the decoded wire tuple against the outbound connection's
  stored `local_port` / `remote_port`. `source_port == 0` is the
  I2P "unspecified" value and remains legal end-to-end.
- **Side-effect-free oversized `send_data` rollback**:
  `send_data` validates the negotiated maximum payload size and
  the current send-window backpressure **before** sequence
  allocation, send-window mutation, retransmit tracking, or
  outbound queue mutation. A rejected oversized write consumes no
  sequence number; the next valid packet receives the exact
  contiguous sequence number that would have been assigned before
  the rejected call.
- **Independent three-layer replay separation**: exact tunnel
  replay (live duplicate window), consumed ECIES tag replay
  (session layer), and fresh-ECIES-reseal Streaming duplicate
  (sequence dedup) are now each independently proven at the
  full-stack level by the Plan 131 trajectories.

Eleven full-stack Plan 130 trajectories remain in
 [`crates/i2pr-client/tests/plan130_trajectory.rs`](../../crates/i2pr-client/tests/plan130_trajectory.rs)
 as the closure evidence for the wire/runtime surface (frozen
 simple-ACK byte fixture, reference ACK/NACK expectation table,
 sequence transition, one-way delayed ACK, piggyback suppression,
 reorder+NACK convergence, port authority + wildcard fallback,
 replay-layer separation, production-Elligator establishment); seven
 Plan 131 trajectories in
 [`crates/i2pr-client/tests/plan131_trajectory.rs`](../../crates/i2pr-client/tests/plan131_trajectory.rs)
 cover the four Plan 131 corrections; ten Plan 132 trajectories in
 [`crates/i2pr-client/tests/plan132_trajectory.rs`](../../crates/i2pr-client/tests/plan132_trajectory.rs)
 drive the Plan 132 corrections (artifact-preserving test seam +
 three independent layer-isolated replay trajectories + transactional
 `send_data` / `send_close` / `send_reset` ordering); and the Plan 133
 closure rewrote B2/B3 to retain the actual first delivered ES envelope
 and corrected the Elligator reference note for the i2pd
 equality-boundary executable distinction. Plan 134 then corrected the
 receive-window ordering so `TooFarAhead` packets cannot advance
 `highest_received` or contaminate later ACK/NACK and piggyback state;
 five policy tests and a manager-level production ACK regression cover
 the fresh, established, reorder, boundary, and extreme-sequence cases.
 Plan 134 is the current local Milestone 6 closure authority:
 `passed-milestone6-recv-window-ack-ceiling-closure`. `milestone6_local_product
 = passed`; `milestone6_interoperable` remains not claimed.


The authenticated-router link between an outbound endpoint and the
remote inbound gateway remains the only transport omission: tests
cross the explicit `authenticated-router-link-bypassed-local-seam`,
which passes the exact OBEP action unchanged and never decrypts,
re-encrypts, or rewrites targets.

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
    -> integrated ACK/NACK/retransmit over the destination stack
       (poll_retransmits, cumulative ackThrough, drain_delivered)
    -> outbound composition: i2pr-client::streaming_adapter::StreamingDestinationAdapter::send
       (bounds against MAX_CLIENT_PAYLOAD_BYTES; single canonical
        Data-envelope owner inside compose_outbound_delivery;
        Plan 122, corrected by Plan 124)
    -> inbound routing: i2pr-client::dispatch::DestinationDispatcher
       then StreamingDestinationAdapter::receive (I2NP Data ->
       gzip client payload -> protocol/ports -> Streaming packet)
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
│   ├── session.rs        Plan 126/127 EciesSessionManager, EciesSessionConfig, PlannedOutboundForm, classify + bound NS/NSR/ES producers
│   ├── lease_selection.rs Plan 122 LeaseSelector / LeaseSelectionPolicy / SelectedLease
│   ├── routing.rs        Plan 122/127 DestinationRouting, OutboundRequest, compose_outbound_delivery, OutboundDeliveryPlan, install_remote_lease_set2
│   ├── dispatch.rs       Plan 122/127 DestinationDispatcher, bound-NS LS2 sender binding, InboundDispatchOutcome / InboundDispatchError
│   ├── streaming.rs      Plan 125/128/129 StreamingManager, StreamingConnection, signed SYN / CLOSE / RESET, RFC 1952 gzip envelope, poll_retransmits, drain_delivered
│   ├── streaming_adapter.rs Plan 129 combined outbound/inbound StreamingDestinationAdapter (TransportSendRequest -> compose_outbound_delivery; recovered I2NP Data -> gzip -> protocol-6 dispatch)
│   └── testing.rs        deterministic inbound/outbound EstablishedMaterial fixtures
└── tests/
    ├── plan120_trajectory.rs   Plan 120 §12 deterministic local trajectory
    ├── plan121_trajectory.rs   Plan 126 corrected primitive-level NS -> NSR -> ES trajectory
    ├── plan122_trajectory.rs   Plan 122 §13 deterministic local two-destination composition
    ├── plan123_trajectory.rs   Plan 125 retained VirtualWire Streaming-only fault tests
    ├── plan124_trajectory.rs   Plan 124 Phases A–G corrected destination-routing trajectory
    ├── plan125_trajectory.rs   Plan 125 real SYN / SYN-response lifecycle and gzip wire-format trajectory
    ├── plan126_trajectory.rs   Plan 126 manager-level lifecycle + negative/ceiling controls
    ├── plan127_trajectory.rs   Plan 127 master NS -> NSR -> ES x4 destination-routing closure + §9 negative controls
    ├── plan128_trajectory.rs   Plan 128 manager handshake stream-id ownership, CLOSE/RESET shapes, negotiation
    ├── plan129_trajectory.rs   Plan 129 integrated destination+Streaming gate (`superseded-by-plan130-final-gate`; persistent inbound chains across ordinary deliveries)
    ├── plan130_trajectory.rs   Plan 130 final wire/runtime corrective closure (frozen simple-ACK byte fixture, reference ACK/NACK table, sequence transition, one-way delayed ACK, piggyback suppression, reorder+NACK convergence, port authority + wildcard fallback, replay-layer separation, production-Elligator establishment; retained as Plan 131 historical evidence)
    └── plan131_trajectory.rs   Plan 131 final local correctness closure (production Elligator branch randomization, connection-owned I2P port tuple asserted on every established send API, side-effect-free oversized `send_data` rollback, independent three-layer replay separation)
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

## ECIES destination session layer (Plans 121 → 126 → 127)

`session.rs` exposes the corrected Plan 126 ECIES-X25519-AEAD-Ratchet
destination session layer. The manager is **destination-scoped** (one
manager per `DestinationIdentity`); it owns one paired session per
remote static key (`outbound` tag set local→remote plus a bounded
inbound tag window remote→local), the pending outbound handshake map
whose reply-window tags are pre-derived at seal time, and the
provisional responder state installed by accepted inbound bound New
Sessions. All state is swept by `advance_time`.

Plan 127 completes the destination-scoped lifecycle with an unambiguous
outbound form state machine:

```text
PlannedOutboundForm::NewSessionReply   retained responder context wins
PlannedOutboundForm::ExistingSession   live paired session
PlannedOutboundForm::BoundNewSession   otherwise
```

`encrypt_to_remote()` seals exactly that precedence, so the first reply
to a bound NS rides the retained Plan 126 reply context instead of
degenerating into a fresh handshake.

Configuration (in `EciesSessionConfig`) is bounded by:

```text
MAX_PEERS_PER_LOCAL_DESTINATION = 64
MAX_PENDING_NEW_SESSIONS        = 8 default / 64 ceiling
MAX_TAG_LOOK_AHEAD              = 8 default / 32 ceiling
DEFAULT_SESSION_IDLE_SECONDS    = 600
MAX_SESSION_IDLE_SECONDS        = 1800
```

The manager never sees the `curve25519-elligator2` third-party type —
`i2pr-crypto::ecies` is the only API surface. Typed outcomes:

- `EciesOutboundMessage::{NewSession { message }, NewSessionReply(message), Existing(message)}`
  with `form_name()` diagnostics derived from the variant
- `PlannedOutboundForm`
- `AcceptedNewSession`, `AcceptedNewSessionReply`,
  `AcceptedExistingSession`, `NewSessionReplyOutbound`
- `ClassifiedInbound::{NewSessionReply, ExistingSession,
  CandidateNewSession, Unknown(_)}` from `classify`
- `EciesAdvanceReport`

Replay rejection: consumed ES tags leave the inbound window, so a
replayed message classifies but decrypts to
`EciesSessionError::UnknownSessionTag`. Duplicate bound New Sessions
(same ephemeral representative) are rejected before any handshake
work. NSR acceptance is tag-driven — no caller-supplied remote
identity is consulted; the paired session installs under the static
public key Alice bound into the original handshake. Plan 127 binds
that pairing to a resolved Destination at the dispatcher: a failed
binding drops the provisional responder through
`drop_provisional_responder()`, so no NSR can ever be emitted for an
unbindable session.

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

Plan 126 replaces `plan_121_deterministic_local_trajectory` with
`plan_126_corrected_deterministic_local_trajectory` in
`plan121_trajectory.rs` (primitive-level NS → NSR → bidirectional ES)
and adds `plan126_trajectory.rs` — the manager-level lifecycle
(`plan_126_full_manager_lifecycle_bidirectional_exact_once`) plus eight
negative controls: ES replay rejection, unknown-tag rejection,
NSR-after-acceptance rejection, duplicate bound New Session rejection,
cross-destination isolation, pending capacity, idle expiry, and
too-short classification. The manager is exercised against real
`i2pr-crypto` primitives only; no test reaches private state or the
third-party `curve25519-elligator2` type directly.

## Destination routing composition (Plans 122 → 127)

`routing.rs` composes the Plan 119 LS2 lookup surface, the Plan 120
destination runtime, the ECIES session layer, and the Plan 116
tunnel data plane into a single outbound pipeline:

1. The `DestinationRouting` cache holds the validated LeaseSet2
   records the local destination has resolved through the router's
   NetDB lookup state machine and the active remote destination
   map keyed by `DestinationHash`, both bounded by the
   `MAX_ACTIVE_REMOTES` ceiling. `install_remote_lease_set2` is the
   explicit typed handoff that installs one already-validated record
   into both (Plan 127 §4 production reverse routing).
2. `LeaseSelector` (in `lease_selection.rs`) picks one lease from
   the resolved LeaseSet2 with caller-supplied CSPRNG, enforcing
   expiry filtering, near-expiry margin, and non-zero receive
   tunnel id.
3. `OutboundRequest::new` wraps the application bytes in an I2NP
   `Data` envelope and bundles the sender's current signed
   `LeaseSet2`.
4. `compose_outbound_delivery` queries
   `EciesSessionManager::planned_outbound_form`, builds the payload
   per form — a fresh bound New Session carries DateTime + Data clove
   + bundled LeaseSet2 DatabaseStore clove; NSR/ES carry DateTime +
   Data clove — seals it through
   `EciesSessionManager::encrypt_to_remote`, then forwards the
   encrypted envelope wrapped in an `I2npBody::Garlic` carrier
   through `OutboundGatewayRole::forward_cells`
   with `DeliveryInstruction::Tunnel { tunnel_id, gateway }`
   targeting the selected lease.
5. The router-delivery boundary emits `OBGWRouterDelivery` cells
   addressed to the first hop of the local creator's outbound
   destination tunnel; the only transport omission is the
   explicit `authenticated-router-link-bypassed-local-seam` label
   Plan 122 calls for.

`dispatch.rs` owns the recipient-side path with the Plan 127 §2/§3
binding order:

1. `DestinationDispatcher::dispatch_garlic_envelope` decodes the
   I2NP `Garlic` body and classifies it through
   `EciesSessionManager::classify`: reply-window tags route to
   `accept_new_session_reply`, inbound-window tags route to
   `accept_existing_session`, everything else attempts
   `accept_new_session` on the bound New Session decode path.
   The dispatcher owns no pending-handshake map of its own.
2. For an accepted bound New Session the dispatcher decodes every
   payload block, requires exactly one bundled
   `DatabaseStore(Standard LeaseSet2)`, validates it under its **own
   contained Destination hash** (`expected_key = None`), verifies its
   usable type-4 X25519 key equals the authenticated NS static key,
   and only then records the validated sender LS2 under the derived
   remote DestinationHash (`record_accepted_lease_set2`,
   surfaced through `accepted_lease_set2_for`). Any binding failure
   drops the retained reply context: no NSR for an unbindable
   session. Typed rejections: `MissingSenderLeaseSet2`,
   `SenderKeyMismatch`, `LeaseSet2Validation(_)`.
3. Local target ownership resolves strictly through the clove
   delivery instruction against the tunnel-owned local destination;
   the sender identity never selects the local target and no trial
   decryption across destinations occurs (Plan 127 §6).
4. Every malformed input fails closed; the dispatcher never
   surfaces plaintext before session authentication.

The `plan122_trajectory` integration test drives the full
deterministic local surface end-to-end across Phases A/B/C/F/H
without touching sockets, DNS, or any external I2P reference. The
`plan127_trajectory` master test drives two destinations through real
tunnel roles in both directions with exact-once Existing Session
delivery plus fifteen §9 negative controls.

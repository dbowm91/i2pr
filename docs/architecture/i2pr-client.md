# i2pr-client

`i2pr-client` is the workspace's first local-destination runtime crate. It
lands as part of the [Milestone 6 destination
roadmap](../plans/118-123-milestone6-router-construction-roadmap.md) and
implements [Plan 120](../plans/120-m6-destination-lifecycle-and-tunnel-pools.md):
destination identity ownership, destination-specific tunnel pools, local
Standard LeaseSet2 generation and signing, LeaseSet2 lifecycle, bounded local
payload contracts, and the destination registry that holds them.

NTCP2 / SSU2 / public-network transport, Garlic encryption, remote
destination routing, and the streaming protocol are all out of scope here;
they live in the Plan 121/122/123 follow-on plans.

## Layering

```text
i2pr-client
  -> i2pr-core       (service lifecycle, HealthState projection)
  -> i2pr-crypto     (Ed25519 signing, X25519 static keys, Zeroize wrappers)
  -> i2pr-netdb      (Plan 119 LeaseSet2 validation / self-verification)
  -> i2pr-proto      (Destination, KeyAndCert, LeaseSet2, Mapping)
  -> i2pr-tunnel     (BoundedTunnelPool = ExploratoryPool, EstablishedMaterial)
```

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
│   ├── lib.rs        facade and re-exports
│   ├── config.rs     DestinationConfig, RegistryConfig, bounded defaults
│   ├── identity.rs   DestinationIdentity, DestinationId (non-Clone secret owner)
│   ├── pool.rs       DestinationTunnelPool wrapping BoundedTunnelPool
│   ├── leaseset.rs   LeaseSet2 builder, LeaseSetLifecycle, LocalLeaseSet
│   ├── message.rs    BoundedPayloadQueue, DestinationPayload, RoutingUnavailable
│   ├── registry.rs   DestinationRuntime, DestinationHandle, DestinationRegistry
│   └── testing.rs    deterministic inbound/outbound EstablishedMaterial fixtures
└── tests/
    └── plan120_trajectory.rs   Plan 120 §12 deterministic local trajectory
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

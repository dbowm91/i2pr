# `i2pr-client` — Deep Dive

Runtime-neutral local destination identity, destination-scoped tunnel pools,
local Standard LeaseSet2 construction/signing/validation, the
ECIES-X25519-AEAD-Ratchet destination session layer, destination routing and
inbound dispatch, the I2P Streaming core, the datagram substrate, and the
encrypted-LeaseSet2 (ELS2) client halves.

Path: `crates/i2pr-client/`

> Status: experimental. Not production-ready. Milestone 6 local destination
> product closed at `passed-milestone6-recv-window-ack-ceiling-closure`
> ([`plans/closure/destination-streaming/134-status.md`](../../plans/closure/destination-streaming/134-status.md)):
> `milestone6_local_product = passed`, **`milestone6_interoperable =
> not-yet-claimed`**. The i2pd-family Streaming lane qualified as a bounded
> one-family result
> ([`plans/closure/mixed-router-interop/193-status.md`](../../plans/closure/mixed-router-interop/193-status.md));
> Java full-router compatibility is `retained-deferred-at-plan247`
> ([`plans/closure/service-tunnels/204-status.md`](../../plans/closure/service-tunnels/204-status.md))
> and `m6_full_two_family_router_conformance = not-yet-claimed`
> ([`plans/closure/mixed-router-interop/201-status.md`](../../plans/closure/mixed-router-interop/201-status.md),
> [`specs/support.toml`](../../specs/support.toml)). **Do not read this crate
> as interoperable with external routers or clients.**

## Purpose

`i2pr-client` owns:

- the non-secret public destination identity, the router-owned secret
  identity owner, the client-owned inbound-decryption capability, and the
  ownership-mode marker that distinguishes them
  ([`identity.rs`](../../crates/i2pr-client/src/identity.rs));
- bounded per-destination configuration and every `MAX_*` ceiling
  ([`config.rs`](../../crates/i2pr-client/src/config.rs));
- the destination-scoped tunnel pool layered over `i2pr-tunnel`'s
  `BoundedTunnelPool`/`ExploratoryPool`, including the local zero-hop route
  ([`pool.rs`](../../crates/i2pr-client/src/pool.rs));
- local Standard LeaseSet2 construction, signing, self-validation, rotation,
  withdrawal, and the client-owned LeaseRequest lifecycle
  ([`leaseset.rs`](../../crates/i2pr-client/src/leaseset.rs));
- the router-local destination registry, the single-destination runtime, the
  state machine, and the atomic client-owned install transaction
  ([`registry.rs`](../../crates/i2pr-client/src/registry.rs));
- the ECIES-X25519-AEAD-Ratchet destination session manager, including the
  outbound form state machine, remove-on-hit tag windows, and provisional
  responder state ([`session.rs`](../../crates/i2pr-client/src/session.rs));
- destination routing, lease selection, outbound composition, and the Garlic
  I2NP carrier ([`routing.rs`](../../crates/i2pr-client/src/routing.rs),
  [`lease_selection.rs`](../../crates/i2pr-client/src/lease_selection.rs),
  [`bundle.rs`](../../crates/i2pr-client/src/bundle.rs));
- the inbound dispatcher, its bounded per-destination application queues, and
  the bound-New-Session sender LeaseSet2 binding
  ([`dispatch.rs`](../../crates/i2pr-client/src/dispatch.rs));
- the I2P Streaming core, the transport-agnostic outbound/inbound adapter, and
  the in-process local delivery seam
  ([`streaming/`](../../crates/i2pr-client/src/streaming/mod.rs),
  [`streaming_adapter.rs`](../../crates/i2pr-client/src/streaming_adapter.rs));
- the repliable/raw datagram substrate
  ([`datagram.rs`](../../crates/i2pr-client/src/datagram.rs));
- the ELS2 publisher and resolver halves, including the Plan 333
  client-authorization variant ([`encrypted_leaseset.rs`](../../crates/i2pr-client/src/encrypted_leaseset.rs));
- bounded payload contracts ([`message.rs`](../../crates/i2pr-client/src/message.rs));
- deterministic `EstablishedMaterial` fixtures for tests and non-production
  callers ([`testing.rs`](../../crates/i2pr-client/src/testing.rs)).

`i2pr-client` must **not** own:

- **Sockets, listeners, or connections.** Every transport seam is a typed
  request/response value (`TransportSendRequest`, `TransportOutcome`). SAM,
  I2CP, and service-tunnel listeners live in `i2pr-daemon` /
  `i2pr-runtime`.
- **Timers or task spawning.** Time is caller-injected (`now_seconds`,
  `now_ms`). The only wall-clock read in the crate is
  [`SystemClock`](../../crates/i2pr-client/src/streaming/clock.rs) behind the
  `Clock` trait, and it is a monotonic elapsed-since-origin value, not a timer
  wheel. There is no `async fn`, no `tokio::*`, no `JoinHandle`, and no
  `spawn` anywhere in `src/`.
- **NetDB mutation.** `i2pr-netdb` validation types
  (`ValidatedLeaseSet2`, `LeaseSet2Store`, `BlindedStorageKey`,
  `BlindingSchedule`) are consumed through typed seams; the client crate never
  writes the router's NetDB.
- **Daemon composition.** The crate is a library. `i2pr-daemon` owns CLI,
  config, and the listener wiring; `i2pr-api` owns the SAM/I2CP wire and state
  machines.

`bash scripts/check-runtime-boundaries.sh` enforces the first three and passes
for this crate (verified 2026-10-05).

## Layering

```text
i2pr-client
  -> i2pr-core       (service lifecycle, HealthState projection)
  -> i2pr-crypto     (Ed25519 signing, X25519 static keys, ECIES, red25519)
  -> i2pr-netdb      (LeaseSet2 validation, LeaseSet2Store, ELS2 blinding)
  -> i2pr-proto      (Destination, KeyAndCert, LeaseSet2, Mapping, Streaming, datagram)
  -> i2pr-tunnel     (BoundedTunnelPool = ExploratoryPool, EstablishedMaterial)
```

Dependency direction is enforced by
[`scripts/check-dependency-direction.sh`](../../scripts/check-dependency-direction.sh),
whose `i2pr-client` allowlist entry is exactly
`{"i2pr-core", "i2pr-crypto", "i2pr-netdb", "i2pr-proto", "i2pr-tunnel"}`.
`i2pr-client` is **not** permitted to depend on `i2pr-daemon`; neither
`i2pr-tunnel` nor `i2pr-netdb` may depend on `i2pr-client`.

## Module layout

Line counts are `wc -l` at the audit date. 16 modules; 18,136 lines total
(11,745 in the 16 top-level `.rs` files including `lib.rs`, plus 6,391 in the
13-file `streaming/` directory).

| Module | File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- | --- |
| (facade) | [`src/lib.rs`](../../crates/i2pr-client/src/lib.rs) | 125 | Module declarations and the crate-root `pub use` re-export surface. `#![forbid(unsafe_code)]`. | all re-exports below |
| `bundle` | [`src/bundle.rs`](../../crates/i2pr-client/src/bundle.rs) | 385 | Plan 296 Garlic reply bundling: multi-data-clove reply encoder under one payload ceiling. | `ReplyBundling`, `BundleError`, `encode_bundled_reply_payload`, `MAX_BUNDLED_DATA_CLOVES` (= 4) |
| `config` | [`src/config.rs`](../../crates/i2pr-client/src/config.rs) | 1012 | Bounded per-destination and registry configuration plus every `MAX_*`/`DEFAULT_*` ceiling. | `DestinationConfig`, `RegistryConfig`, `DestinationTunnelMode`, `LocalRouterContext`, `DestinationConfigError`, `MAX_LOCAL_DESTINATIONS` (16), `MAX_DESTINATION_INBOUND`/`_OUTBOUND` (8/8), `MAX_DESTINATION_BUILD_CONCURRENCY` (4), `MAX_DESTINATION_FAILURE_THRESHOLD` (16), `MAX_PENDING_DESTINATION_MESSAGES` (256), `MAX_PENDING_DESTINATION_BYTES` (512 KiB), `MAX_DESTINATION_BACKUP_QUANTITY` (3), `MAX_DESTINATION_LENGTH_VARIANCE` (2), `MAX_AGGREGATE_COMMAND_QUEUE_DEPTH` (4096), `MAX_LEASE_PUBLICATION_MARGIN_SECONDS` (600), `MAX_LEASE_ROTATION_MARGIN_SECONDS` (600), `DEFAULT_LEASE_PUBLICATION_MARGIN_SECONDS` (60), `DEFAULT_LEASE_ROTATION_MARGIN_SECONDS` (120) |
| `datagram` | [`src/datagram.rs`](../../crates/i2pr-client/src/datagram.rs) | Plan 291 + 368 runtime-neutral protocol 17–20 datagram substrate: Datagram1 and Proposal 163 codecs, recipient-bound signatures, offline delegation expiry, replay cache, Datagram3 unauthenticated source metadata, and bounded queues. | `DatagramManager`, `DatagramSendRequest`, `DatagramReceiveEvent`, `DatagramCounters`, `DatagramError`, `DATAGRAM1_PROTOCOL`, `DATAGRAM2_PROTOCOL`, `DATAGRAM3_PROTOCOL`, `RAW_DATAGRAM_PROTOCOL`, `MAX_DATAGRAM_APPLICATION_PAYLOAD`, `MAX_DATAGRAM_OPTIONS_BYTES`, `MAX_DATAGRAM_FROM_BYTES`, `MAX_DATAGRAM_RECEIVE_QUEUE`, `MAX_DATAGRAM_OUTBOUND_QUEUE`, `MAX_DATAGRAM2_REPLAY_ENTRIES` |
| `dispatch` | [`src/dispatch.rs`](../../crates/i2pr-client/src/dispatch.rs) | 929 | Recipient-side Garlic surface: ECIES classification hand-off, bound-New-Session sender LS2 binding, destination-hash ownership check, bounded FIFO application queues. | `DestinationDispatcher`, `InboundDispatchOutcome`, `InboundDispatchError`, `MAX_INBOUND_DESTINATIONS` (256), `MAX_INBOUND_PAYLOAD_BYTES_PER_DESTINATION` (512 KiB), `MAX_INBOUND_PENDING_MESSAGES` (256) |
| `encrypted_leaseset` | [`src/encrypted_leaseset.rs`](../../crates/i2pr-client/src/encrypted_leaseset.rs) | 756 | Plans 332/333 ELS2 client halves: owner-only publisher (no-authorization and Plan 333 authorized records) and lookup-only resolver over a b33 address. Consumes the in-repo `i2pr_crypto::red25519` module. | `EncryptedLeaseSet2Publisher`, `EncryptedLeaseSet2Resolver`, `ResolvedEncryptedService`, `EncryptedLeaseSetError`, `ServerPsk` (= `i2pr_netdb::PskClientKey`), `GenerationAuthCookie`, `authorization_scheme`, `generate_client_dh_keypair`, `owner_scalar_from_seed` |
| `identity` | [`src/identity.rs`](../../crates/i2pr-client/src/identity.rs) | 986 | Non-secret public destination identity, the non-`Clone`/redacted-`Debug` router-owned secret owner, the ownership marker, and the non-`Clone` client-supplied inbound decryption capability. | `DestinationId`, `DestinationPublic`, `DestinationIdentity`, `DestinationOwnership`, `InboundDecryptionCapability`, `InboundDecryptionRef`, `DestinationIdentityError`, `DESTINATION_IDENTITY_LEGACY_CRYPTO_TYPE` (ElGamal/type 0), `DESTINATION_LS2_CRYPTO_TYPE` (X25519/type 4), `DESTINATION_LEGACY_PUBLIC_LENGTH` (256), `DESTINATION_LEGACY_PADDING_LENGTH` (96), `DESTINATION_X25519_PADDING_LENGTH` (320) |
| `lease_selection` | [`src/lease_selection.rs`](../../crates/i2pr-client/src/lease_selection.rs) | 363 | Plan 122 lease choice from a resolved LeaseSet2 with caller-supplied CSPRNG. | `LeaseSelector`, `LeaseSelectionPolicy`, `SelectedLease`, `LeaseSelectionError`, `MAX_LEASE_SAFETY_MARGIN_SECONDS` (600) |
| `leaseset` | [`src/leaseset.rs`](../../crates/i2pr-client/src/leaseset.rs) | 1134 | Local Standard LeaseSet2 build/sign/self-validate, rotation and withdrawal, and the client-owned install + refresh-request lifecycle. | `LocalLeaseSet`, `ClientOwnedLeaseSet`, `LeaseSetLifecycle`, `LeaseSetDecision`, `LeaseSetRotationCause`, `ClientRefreshCause`, `LeaseRequest`, `LeaseRequestLease`, `LeaseSetSummary`, `LeaseSetError`, `build_signed_lease_set2`, `encoded_hash`, `LEASE_SET2_SIGNATURE_DOMAIN` |
| `message` | [`src/message.rs`](../../crates/i2pr-client/src/message.rs) | 292 | Bounded per-destination application payload contracts. | `DestinationPayload`, `BoundedPayloadQueue`, `QueuedOutbound`, `RoutingUnavailable`, `PayloadError`, `MAX_DESTINATION_PAYLOAD_BYTES` (32 KiB) |
| `pool` | [`src/pool.rs`](../../crates/i2pr-client/src/pool.rs) | 875 | Destination policy over `i2pr-tunnel`'s bounded pool: one-shot admission, direction checks, failure/standby accounting, lease-source publication, Plan 172 local zero-hop routes. | `DestinationTunnelPool`, `InboundLeaseSource`, `BuildFailureDisposition`, `DestinationPoolError`, `outcome_slot` |
| `registry` | [`src/registry.rs`](../../crates/i2pr-client/src/registry.rs) | 1287 | The router-local registry, the single-destination runtime, the lifecycle state machine, the health projection, and the atomic client-owned install transaction. | `DestinationRegistry`, `DestinationRuntime`, `DestinationHandle`, `DestinationCommand`, `DestinationEvent`, `DestinationState`, `DestinationProgress`, `DestinationShutdown`, `RegistryError`, `DestinationRuntimeError` |
| `routing` | [`src/routing.rs`](../../crates/i2pr-client/src/routing.rs) | 1434 | Outbound composition: validated-LS2 store, active-remote cache, reverse routing, Garlic I2NP carrier, and the Plan 296 bundled-reply composer. | `DestinationRouting`, `DestinationRoutingConfig`, `DestinationRoutingError`, `DestinationOutboundRole`, `OutboundRequest`, `OutboundDeliveryPlan`, `EncryptedOutbound`, `SendError`, `LookupIngestOutcome`, `LookupIngestError`, `compose_outbound_delivery`, `compose_bundled_reply_delivery`, `decode_data_clove`, `build_local_data_envelope`, `MAX_ACTIVE_REMOTES` (256), `MAX_CONCURRENT_REMOTE_LOOKUPS` (256), `MAX_PENDING_OUTBOUND_PER_REMOTE` (64), `MAX_ROUTER_SIDE_LS2_BYTES_PER_REMOTE` (16 KiB) |
| `session` | [`src/session.rs`](../../crates/i2pr-client/src/session.rs) | 947 | Plans 126/127 ECIES-X25519-AEAD-Ratchet destination session manager and the typed Garlic payload helpers. | `EciesSessionManager`, `EciesSessionConfig`, `EciesSessionConfigError`, `EciesSessionError`, `EciesPayloadError`, `EciesOutboundMessage`, `EciesAdvanceReport`, `PlannedOutboundForm`, `AcceptedNewSession`, `AcceptedNewSessionReply`, `AcceptedExistingSession`, `NewSessionReplyOutbound`, `ClassifiedInbound`, `ClassifiedUnknown`, `decode_decrypted_payload`, `encode_garlic_clove_payload`, `encode_new_session_payload`, `local_clove`, `MAX_PEERS_PER_LOCAL_DESTINATION` (64), `MAX_TAG_LOOK_AHEAD` (32), `MAX_SESSION_IDLE_SECONDS` (1800), `DEFAULT_SESSION_IDLE_SECONDS` (600) |
| `streaming` | [`src/streaming/`](../../crates/i2pr-client/src/streaming/mod.rs) | 6391 (13 files) | The I2P Streaming core and the in-process local delivery seam (see subtable below). | `StreamingManager`, `StreamingConnection`, `ConnectionState`, `ConnectionId`, `StreamingConfig`, `SendWindowPolicy`, `RecvWindowPolicy`, `RetransmitPolicy`, `CongestionPolicy`, `StreamingTransport`, `TransportSendRequest`, `Clock`/`ManualClock`/`SystemClock`, `LocalDeliverySender`/`Receiver`, `deliver`, `deliver_batched` |
| `streaming_adapter` | [`src/streaming_adapter.rs`](../../crates/i2pr-client/src/streaming_adapter.rs) | 378 | Plan 129 combined outbound/inbound Streaming adapter: `TransportSendRequest` → `compose_outbound_delivery`; recovered I2NP `Data` → gzip client payload → protocol/ports → `StreamingManager`. Plan 291 adds the datagram outcome. | `StreamingDestinationAdapter`, `StreamingAdapterError`, `InboundStreamingOutcome`, `MAX_STREAMING_ADAPTER_PAYLOAD_BYTES` (= `i2pr_proto::streaming::MAX_CLIENT_PAYLOAD_BYTES`, 60 KiB) |
| `testing` | [`src/testing.rs`](../../crates/i2pr-client/src/testing.rs) | 185 | Deterministic `EstablishedMaterial` fixtures that drive the real `ShortBuildStateMachine`. Not used by the destination runtime itself. | `established_inbound`, `established_outbound`, `established_material`, `fixture_path` |

### `streaming/` submodules

| File | Lines | Responsibility |
| --- | --- | --- |
| [`mod.rs`](../../crates/i2pr-client/src/streaming/mod.rs) | 78 | Module wiring plus the re-export surface. |
| [`manager.rs`](../../crates/i2pr-client/src/streaming/manager.rs) | 2665 | `StreamingManager`: per-destination connection tables, listener backlog, ACK/NACK, retransmit polling, delivered-byte ordering, Plan 152 receive-window gating, Plan 144 canonical/mirror split. |
| [`local_delivery.rs`](../../crates/i2pr-client/src/streaming/local_delivery.rs) | 1326 | The runtime-neutral in-process delivery seam (`deliver`, `deliver_batched`). |
| [`connection.rs`](../../crates/i2pr-client/src/streaming/connection.rs) | 610 | `StreamingConnection` state machine. |
| [`recv_window.rs`](../../crates/i2pr-client/src/streaming/recv_window.rs) | 371 | Receive-window admission and `TooFarAhead` decisions. |
| [`config.rs`](../../crates/i2pr-client/src/streaming/config.rs) | 330 | `StreamingConfig` and the streaming ceilings. |
| [`send_window.rs`](../../crates/i2pr-client/src/streaming/send_window.rs) | 257 | Send-window backpressure and contiguous sequence allocation. |
| [`retransmit.rs`](../../crates/i2pr-client/src/streaming/retransmit.rs) | 171 | RTO sampling, `poll_retransmits` attempt cap. |
| [`congestion.rs`](../../crates/i2pr-client/src/streaming/congestion.rs) | 136 | Congestion window policy. |
| [`clock.rs`](../../crates/i2pr-client/src/streaming/clock.rs) | 130 | `Clock`, `ManualClock`, `SystemClock`. |
| [`events.rs`](../../crates/i2pr-client/src/streaming/events.rs) | 106 | Typed inbound/outbound event surface. |
| [`errors.rs`](../../crates/i2pr-client/src/streaming/errors.rs) | 100 | `StreamingError`. |
| [`transport.rs`](../../crates/i2pr-client/src/streaming/transport.rs) | 79 | `StreamingTransport`, `TransportSendRequest`, `TransportOutcome`, `TransportError`. |
| [`testing.rs`](../../crates/i2pr-client/src/streaming/testing.rs) | 33 | `#[cfg(test)]` clock/policy helpers. |

## Public surface

The crate root re-exports exactly the following (from
[`lib.rs`](../../crates/i2pr-client/src/lib.rs)):

- **bundle** — `BundleError`, `MAX_BUNDLED_DATA_CLOVES`, `ReplyBundling`,
  `encode_bundled_reply_payload`
- **config** — `DestinationConfig`, `DestinationConfigError`,
  `DestinationTunnelMode`, `LocalRouterContext`, `RegistryConfig`,
  `DEFAULT_LEASE_PUBLICATION_MARGIN_SECONDS`,
  `DEFAULT_LEASE_ROTATION_MARGIN_SECONDS`,
  `MAX_AGGREGATE_COMMAND_QUEUE_DEPTH`, `MAX_DESTINATION_BACKUP_QUANTITY`,
  `MAX_DESTINATION_BUILD_CONCURRENCY`, `MAX_DESTINATION_FAILURE_THRESHOLD`,
  `MAX_DESTINATION_INBOUND`, `MAX_DESTINATION_LENGTH_VARIANCE`,
  `MAX_DESTINATION_OUTBOUND`, `MAX_LEASE_PUBLICATION_MARGIN_SECONDS`,
  `MAX_LEASE_ROTATION_MARGIN_SECONDS`, `MAX_LOCAL_DESTINATIONS`,
  `MAX_PENDING_DESTINATION_BYTES`, `MAX_PENDING_DESTINATION_MESSAGES`
- **datagram** — `DATAGRAM1_PROTOCOL`, `RAW_DATAGRAM_PROTOCOL`,
  `DatagramCounters`, `DatagramError`, `DatagramManager`,
  `DatagramReceiveEvent`, `DatagramSendRequest`,
  `MAX_DATAGRAM_APPLICATION_PAYLOAD`, `MAX_DATAGRAM_FROM_BYTES`,
  `MAX_DATAGRAM_OUTBOUND_QUEUE`, `MAX_DATAGRAM_RECEIVE_QUEUE`
- **dispatch** — `DestinationDispatcher`, `InboundDispatchError`,
  `InboundDispatchOutcome`, `MAX_INBOUND_DESTINATIONS`,
  `MAX_INBOUND_PAYLOAD_BYTES_PER_DESTINATION`, `MAX_INBOUND_PENDING_MESSAGES`
- **encrypted_leaseset** — `EncryptedLeaseSet2Publisher`,
  `EncryptedLeaseSet2Resolver`, `EncryptedLeaseSetError`,
  `GenerationAuthCookie`, `ResolvedEncryptedService`, `ServerPsk`,
  `authorization_scheme`, `generate_client_dh_keypair`, `owner_scalar_from_seed`
- **identity** — `DESTINATION_IDENTITY_LEGACY_CRYPTO_TYPE`,
  `DESTINATION_LEGACY_PADDING_LENGTH`, `DESTINATION_LEGACY_PUBLIC_LENGTH`,
  `DESTINATION_LS2_CRYPTO_TYPE`, `DESTINATION_X25519_PADDING_LENGTH`,
  `DestinationId`, `DestinationIdentity`, `DestinationIdentityError`,
  `DestinationOwnership`, `DestinationPublic`, `InboundDecryptionCapability`,
  `InboundDecryptionRef`
- **lease_selection** — `LeaseSelectionError`, `LeaseSelectionPolicy`,
  `LeaseSelector`, `MAX_LEASE_SAFETY_MARGIN_SECONDS`, `SelectedLease`
- **leaseset** — `ClientOwnedLeaseSet`, `ClientRefreshCause`,
  `LEASE_SET2_SIGNATURE_DOMAIN`, `LeaseRequest`, `LeaseRequestLease`,
  `LeaseSetDecision`, `LeaseSetError`, `LeaseSetLifecycle`,
  `LeaseSetRotationCause`, `LeaseSetSummary`, `LocalLeaseSet`,
  `build_signed_lease_set2`, `encoded_hash`
- **message** — `BoundedPayloadQueue`, `DestinationPayload`,
  `MAX_DESTINATION_PAYLOAD_BYTES`, `PayloadError`, `QueuedOutbound`,
  `RoutingUnavailable`
- **pool** — `BuildFailureDisposition`, `DestinationPoolError`,
  `DestinationTunnelPool`, `InboundLeaseSource`, `outcome_slot`
- **registry** — `DestinationCommand`, `DestinationEvent`, `DestinationHandle`,
  `DestinationProgress`, `DestinationRegistry`, `DestinationRuntime`,
  `DestinationRuntimeError`, `DestinationShutdown`, `DestinationState`,
  `RegistryError`
- **routing** — `DestinationOutboundRole`, `DestinationRouting`,
  `DestinationRoutingConfig`, `DestinationRoutingError`, `EncryptedOutbound`,
  `LookupIngestError`, `LookupIngestOutcome`, `MAX_ACTIVE_REMOTES`,
  `MAX_CONCURRENT_REMOTE_LOOKUPS`, `MAX_PENDING_OUTBOUND_PER_REMOTE`,
  `OutboundDeliveryPlan`, `OutboundRequest`, `SendError`,
  `compose_bundled_reply_delivery`, `compose_outbound_delivery`
- **session** — `AcceptedExistingSession`, `AcceptedNewSession`,
  `AcceptedNewSessionReply`, `ClassifiedInbound`, `ClassifiedUnknown`,
  `DEFAULT_SESSION_IDLE_SECONDS`, `EciesAdvanceReport`, `EciesOutboundMessage`,
  `EciesPayloadError`, `EciesSessionConfig`, `EciesSessionConfigError`,
  `EciesSessionError`, `EciesSessionManager`, `MAX_PEERS_PER_LOCAL_DESTINATION`,
  `MAX_SESSION_IDLE_SECONDS`, `MAX_TAG_LOOK_AHEAD`, `NewSessionReplyOutbound`,
  `PlannedOutboundForm`, `decode_decrypted_payload`,
  `encode_garlic_clove_payload`, `encode_new_session_payload`, `local_clove`
- **streaming::local_delivery** — `BatchedAttempt`, `BatchedDeliveryReport`,
  `LocalDeliveryError`, `LocalDeliveryOutcome`, `LocalDeliveryReceiver`,
  `LocalDeliverySender`, `deliver`, `deliver_batched`
- **streaming_adapter** — `InboundStreamingOutcome`,
  `MAX_STREAMING_ADAPTER_PAYLOAD_BYTES`, `StreamingAdapterError`,
  `StreamingDestinationAdapter`

Notable: `session::MAX_PENDING_NEW_SESSIONS` is `pub` in its module but is
**not** re-exported at the crate root; reach it as
`i2pr_client::session::MAX_PENDING_NEW_SESSIONS`. The streaming core
(`StreamingManager` and friends) is reached through the `streaming` module,
not the crate root.

## Key contracts

### Destination identity and secret ownership

`DestinationIdentity` (router-owned) is the only type here that holds both
destination private keys: the Ed25519 signing seed and the X25519 static
secret. Verified properties:

- **non-`Clone`** — no `Clone` derive or impl; two local destinations never
  implicitly share private key objects.
- **manual redacted `Debug`** — prints `id`, then `signing_key: <redacted>`
  and `static_key: <redacted>`. No `Debug` derive.
- **narrow accessors** — `sign()` (the only secret-consuming operation),
  `static_secret_bytes()`, `signing_seed_bytes()` (reserved for the
  `i2pr-api` SAM `PrivateKeyFile` codec), `static_public_bytes()`,
  `diffie_hellman()`.
- **crypto-layer separation (Plan 223)** — the generated `Destination` uses
  the Java-compatible legacy identity shape (ElGamal/type 0, 256-byte public
  slot, Ed25519/type 7 signing, 96-byte padding) while the *active* Standard
  LeaseSet2 encryption key stays X25519/type 4, sourced from the owned X25519
  static secret and never from `destination.public_key()`. The 256-byte slot is
  public non-secret filler and is never derived from any secret.
  `from_private_bytes_legacy_x25519` is retained only for byte-identical
  reconstruction of pre-Plan-223 persisted v1 service records.

`DestinationPublic` is the non-secret counterpart (`Clone + Debug`): the
canonical `Destination`, the derived `DestinationId`, and the cached 32-byte
static X25519 public bytes. It is derived by **both** ownership modes so the
ECIES primitives and the dispatcher never branch on ownership.

`DestinationId` is the SHA-256 hash of the canonical `Destination` — non-secret,
`Copy`, and the registry/deduplication key.

### Ownership modes on one runtime (Plan 166)

`DestinationOwnership` is a `Copy` marker on a **single**
`DestinationRuntime`; there is no second runtime type and no duplicated
destination stack.

- `RouterOwned` — `holds_signing_secret() == true`; the runtime keeps the
  `Arc<DestinationIdentity>` and signs/rotates its own Standard LeaseSet2.
  This is the SAM path, which shares one `Arc<DestinationIdentity>` allocation
  between the runtime and the daemon bridge.
- `ClientOwned` — `holds_signing_secret() == false`; the runtime never holds a
  destination signing private key. `DestinationRuntime::new_client_owned` is
  the only constructor, `identity()`/`identity_arc()` return `None`, and
  `static_secret_bytes()` returns `None` until a successful install.

### `InboundDecryptionCapability`

The smallest possible wrapper for the client-supplied X25519 inbound
decryption secret. Verified:

- **non-`Clone`** — no `Clone` derive or impl, so no second private-identity
  allocation can be made from it.
- **manual redacted `Debug`** — `InboundDecryptionCapability { static_public_bytes:
  "<redacted>", secret: "<redacted>" }`. Both fields, including the
  *public* one, are redacted.
- **no `PartialEq`/`Eq`** — there is no equality over the secret bytes.
- **zeroize on drop** — the `Drop` impl zeroes `static_public_bytes`; the
  secret itself is an `i2pr_crypto::X25519PrivateKey`, which owns its own
  zeroization. The wrapper does not duplicate the secret into a plain array.
- **exposed surface** — `static_public_bytes()`, `secret_bytes()`,
  `diffie_hellman()`. `InboundDecryptionRef<'a>` is a `Clone + Copy` borrowed
  view exposing the two byte accessors, so the ECIES layer never receives the
  capability itself.

The inbound public key is matched against the destination encryption key
*before* the secret is stored: `install_client_lease_set2` performs the
capability-vs-`DestinationPublic` pre-check, and `install_external` performs
the capability-vs-LeaseSet2-encryption-key match.

### Atomic client-owned install

`DestinationRuntime::install_client_lease_set2(record, capability, now)` is
the single transaction that wires a client-owned destination into the inbound
ECIES path. It is atomic: **every** check runs before any state mutation, and
the capability is only stored and the state only recomputed after the last
check returns `Ok`. The ordered checks are:

1. ownership is `ClientOwned` — otherwise `InstallWhileStopping`
   (router-owned destinations reject client installs);
2. state is not `Stopping`/`Stopped`;
3. capability public key equals the destination's advertised static public
   bytes — but only for X25519-slot destinations (Plan 172 §11: ElGamal-slot
   destinations, which every modern reference client ships, carry a zeroed
   static slot because the field has been unused since I2P 0.6);
4. the pool must currently expose at least `minimum_usable_inbound` real
   inbound lease sources;
5. the LeaseSet2 is not already expired;
6. the record advertises at least one lease;
7. every advertised lease ends after `published` and at or before the
   LeaseSet2 expiry;
8. every advertised lease resolves to a real `InboundLeaseSource` owned by
   this destination's pool — otherwise `ForeignLease`;
9. no two advertised leases map to the same pool slot — otherwise
   `DuplicateLease`;
10. the usable LeaseSet2 encryption key type is X25519/type 4 — otherwise
    `UnsupportedEncryptionKeyType`;
11. the LeaseSet2 encryption public key equals the capability's public key —
    otherwise `DecryptionKeyMismatch`;
12. full `i2pr_netdb::ValidatedLeaseSet2::from_lease_set2` validation under
    the destination's own `expected_key` (Standard LeaseSet2 signature).

Atomicity matters because there is no partial-install state to roll back: a
mismatched capability must never be published and fixed later, and a lease set
must never be advertised while the decryption secret that can actually read
its traffic is not installed.

### `LeaseRequest` is sourced from real inbound tunnels, never synthesized

In client-owned mode the lifecycle never signs a replacement LeaseSet2. When
leases approach the rotation margin it emits
`LeaseSetDecision::RequestClientRefresh` and the runtime surfaces a typed
`LeaseRequest` through `take_client_refresh_request(now)`. The lease list is
built by mapping `self.pool.inbound_lease_sources(now_seconds)` through
`LeaseRequestLease::from(&InboundLeaseSource)` — i.e. from the destination's
own real inbound tunnel pool. There is no code path that constructs a
`LeaseRequestLease` from raw bytes, a random gateway, or a synthesized tunnel
id. The Plan 167 daemon projects the result into
`i2pr_api::i2cp::I2cpAction::RequestVariableLeaseSet` without re-deriving it.

### ECIES-X25519-AEAD-Ratchet sessions

`EciesSessionManager` is **destination-scoped** (one manager per
`DestinationIdentity`) and holds four bounded maps/vectors:

- `sessions: BTreeMap<RemoteStaticKey, PairedSession>` — sessions are paired
  and indexed **by the remote static X25519 public key**, never by a tag, a
  name, or a hash of key bytes;
- `pending_initiated: Vec<Option<PendingInitiated>>` — a slot vector with
  tombstones so removed slots are reusable (bounded index indexing);
- `pending_reply_tags: BTreeMap<[u8; SESSION_TAG_LENGTH], usize>` — reply
  window tags → pending slot index, **pre-derived at seal time** so inbound
  replies classify without searching the handshake state;
- `provisional_responders: BTreeMap<RemoteStaticKey, ProvisionalResponder>` —
  retained reply context keyed by the *remote* static key;
- `seen_new_session_ephemerals: VecDeque<[u8; REPRESENTATIVE_LENGTH]>` —
  duplicate-bound-NS detection by ephemeral representative.

Remove-on-hit tag windows: a consumed inbound tag leaves the window, so a
replayed Existing Session classifies but decrypts to
`EciesSessionError::UnknownSessionTag`.

`PlannedOutboundForm` precedence, verified in `planned_outbound_form`:

```text
PlannedOutboundForm::NewSessionReply   retained provisional responder for that remote static key
PlannedOutboundForm::ExistingSession   live paired session within the idle window
PlannedOutboundForm::BoundNewSession   otherwise
```

`encrypt_to_remote` seals exactly that precedence, so the first reply to a
bound New Session always rides the retained reply context. A fresh bound New
Session always bundles the local destination's current signed Standard
LeaseSet2 (`SendError::MissingBundledLeaseSet2` otherwise) so the receiver can
discover the sender.

Provisional responder discipline: the dispatcher binds an accepted inbound
bound New Session **only** after validating exactly one bundled sender
Standard LeaseSet2 under its *own contained Destination* hash
(`expected_key = None`) and confirming its usable type-4 X25519 key equals the
authenticated NS static key. Any binding failure calls
`drop_provisional_responder()`, so no New Session Reply can be emitted for a
session whose remote identity could not be bound. The remote identity derives
exclusively from the validated record — never from static-key bytes, an NSR
tag, or an ES tag (`resolve_remote_destination` searches validated records
only, then the router-side store).

Configuration is bounded by `EciesSessionConfig::try_new`, which rejects zero
values and over-ceiling values with typed `EciesSessionConfigError` variants:

```text
MAX_PEERS_PER_LOCAL_DESTINATION = 64      balanced() default: 8
MAX_PENDING_NEW_SESSIONS        = 64      balanced() default: 8
MAX_TAG_LOOK_AHEAD              = 32      balanced() default: 8
DEFAULT_SESSION_IDLE_SECONDS    = 600
MAX_SESSION_IDLE_SECONDS        = 1800
```

`EciesOutboundMessage` variants are `NewSession { message }`,
`NewSessionReply(message)`, `Existing(message)`, each with a `form_name()`
diagnostic derived from the variant rather than a magic first byte. The
manager never sees a third-party ElGinator type: `i2pr-crypto` is the only
API surface.

### Reverse routing

`DestinationRouting` holds the router-side `LeaseSet2Store` and a bounded
active-remote map keyed by `DestinationHash`.
`install_remote_lease_set2` is the explicit typed handoff that installs one
already-validated record into both, after which `select_lease` and
`remote_static_public_key` work with no raw reparse. Every remote view is
bounded: `MAX_ACTIVE_REMOTES` (256), `MAX_CONCURRENT_REMOTE_LOOKUPS` (256),
`MAX_PENDING_OUTBOUND_PER_REMOTE` (64), `MAX_ROUTER_SIDE_LS2_BYTES_PER_REMOTE`
(16 KiB).

### Outbound composition and the Garlic carrier

`LeaseSelector` picks one lease from a resolved LeaseSet2 with a
caller-supplied CSPRNG, enforcing expiry filtering, the near-expiry safety
margin (`MAX_LEASE_SAFETY_MARGIN_SECONDS` = 600), and a non-zero receive
tunnel id. `OutboundRequest::new` wraps the application bytes in an I2NP
`Data` envelope and is the **single canonical Data-envelope owner**;
`compose_outbound_delivery` does not build a second one.

`OutboundDeliveryPlan` exposes:

- `selected_lease`,
- `inner_envelope_bytes` — the **pre-encryption** application carrier, retained
  for diagnostic comparison only; the tunnel data plane must never observe
  these bytes in plaintext,
- `encrypted_message: EncryptedOutbound` — which ECIES form was emitted,
- **`garlic_i2np_bytes`** — the canonical carrier: the standard-encoded I2NP
  `Garlic` message bytes that wrap the encrypted envelope, and the only thing
  the outbound tunnel data plane carries (Plan 124 corrected the Plan 122
  defect where the plaintext inner `Data` envelope was fed to the tunnel role),
- `cells: Vec<OBGWRouterDelivery>`.

`compose_bundled_reply_delivery` is the Plan 296 sibling that reuses the same
clove bytes and `Garlic` carrier for a multi-clove reply (at most
`MAX_BUNDLED_DATA_CLOVES` = 4 data cloves, bounded by the client-payload
ceiling).

The authenticated-router link between an outbound endpoint and the remote
inbound gateway is the only transport omission: tests cross the explicit
`authenticated-router-link-bypassed-local-seam`, which passes the exact OBEP
action unchanged and never decrypts, re-encrypts, or rewrites targets.

### Inbound dispatch and the destination-hash ownership guard

`DestinationDispatcher::dispatch_garlic_envelope` requires an
`I2npBody::Garlic` body, classifies the bytes through
`EciesSessionManager::classify`, and processes payload blocks only **after**
session authentication succeeds. It owns no pending-handshake map of its own.

For an accepted bound New Session it decodes every payload block, requires
exactly one bundled `DatabaseStore(Standard LeaseSet2)`, validates it under its
own contained Destination hash, verifies the usable type-4 X25519 key equals
the authenticated NS static key, and only then records the validated sender
LS2 (`record_accepted_lease_set2`, read back via `accepted_lease_set2_for`).
Typed rejections: `MissingSenderLeaseSet2`, `SenderKeyMismatch`,
`LeaseSet2Validation(_)`.

**Spoofing guard.** Local target ownership resolves strictly through the
clove delivery instruction:

```text
GarlicDelivery::Local                -> target hash := *local_id.as_hash()
GarlicDelivery::Destination(bytes)   -> target hash := Hash::from_bytes(bytes)
then: target_hash MUST equal *local_id.as_hash(), else UnknownDestination
```

A frame claiming a `Destination` target that is not this destination's own
hash is rejected before the payload is queued. The sender identity is never
consulted for local routing, and no trial decryption across destinations
occurs (exactly one destination-scoped session manager decrypts). Every
non-LeaseSet2 data clove is admitted **all-or-nothing** into a per-destination
FIFO `BoundedPayloadQueue` bounded by `MAX_INBOUND_DESTINATIONS` (256),
`MAX_INBOUND_PENDING_MESSAGES` (256), and
`MAX_INBOUND_PAYLOAD_BYTES_PER_DESTINATION` (512 KiB). Malformed input always
fails closed; plaintext is never surfaced before authentication.

`DestinationDispatcher::bind_destination_hash` records a
`DestinationHash → DestinationId` index (and `unregister_destination` clears
it atomically), but the authoritative ownership check is the direct
`target_hash == *local_id.as_hash()` comparison above — see
[Boundary and security notes](#boundary-and-security-notes).

### Per-destination tunnel pools consume one-shot `EstablishedMaterial`

`DestinationTunnelPool` wraps `ipr-tunnel`'s bounded pool and accepts only
real one-shot `EstablishedMaterial` from the `ShortBuildStateMachine`.
`EstablishedMaterial` is **not `Clone`**: it is consumed exactly once by
`into_established_tunnel(&mut self)`, which `mem::take`s the hop list and
flips the `extracted` flag, returning `None` on a second call. The pool
rejects direction mismatches at the API boundary, tracks a bounded
consecutive-failure counter that pauses replacement at the configured
threshold, applies Plan 296 standby accounting (effective base-plus-backup
targets, standby promotion on failure/expiry loss, registration bounded at the
effective target), and publishes `InboundLeaseSource { gateway,
gateway_receive_tunnel_id, tunnel_expires_seconds, advertised_expires_seconds }`
for every usable inbound tunnel.

There is **no production placeholder established-tunnel path**. The reuse
hazard is explicit and structural: because the material is one-shot, any
attempt to admit the same established material twice is a compile error or an
`Option::None`, not a silent key reuse.
`DestinationTunnelPool::release_all` zeroizing the registered material is the
only shutdown seam.

Plan 172 adds the explicit **local zero-hop** kind — a typed local route, not
empty remote `EstablishedMaterial`. `Remote EstablishedTunnel::new()` still
rejects empty hops and `ExploratoryPoolConfig::MIN_HOPS` stays 1, so a
zero-hop registration can never be mistaken for a real transit tunnel.

### Local Standard LeaseSet2 construction, rotation, withdrawal

`build_signed_lease_set2(identity, leases, published_seconds)`:

- rejects an empty lease set (`NoUsableInboundTunnels`);
- advertises `Lease2.end_date = tunnel_expires_seconds - publication_margin`,
  and rejects any lease that outlives its own tunnel
  (`LeaseOutlivesTunnel`) or has already expired relative to `published`
  (`LeaseAlreadyExpired`);
- derives the `LeaseSet2Header` expiry offset from the maximum advertised lease
  end, so the record never outlives the destination's tunnels;
- builds `LeaseSet2 { header, options: Mapping::empty(), encryption_keys,
  leases, placeholder }` with a zeroed placeholder `SignatureValue`, signs
  `signature_preimage()` (the `0x03 || signed_bytes` domain, exposed as
  `LEASE_SET2_SIGNATURE_DOMAIN`) with the destination's Ed25519 key, and
  rebuilds the record with the real signature;
- emits the LS2 encryption key as X25519/type 4 from the owned static public
  key — never from `destination.public_key()` (Plan 223 D3).

The finalized record is then **self-validated** through the same
`i2pr_netdb::ValidatedLeaseSet2` path used for received entries, catching
construction/verification drift before the record leaves the local runtime.
A replacement generated inside the same second as its predecessor still
advances `published` (the lifecycle enforces monotonicity, saturating at
`TimestampOverflow`) so NetDB replacement semantics stay correct.

`LeaseSetLifecycle` owns generation, publication acknowledgement
(`acknowledge_publication`), rotation causes, and withdrawal
(`begin_stopping` drops every retained record so a stale lease set can never
be advertised as healthy; `release_client` releases only the client-owned
side and is idempotent).

### Registry capacity and duplicate rejection

`DestinationRegistry` is the router-local map from `DestinationId` to
`DestinationRuntime`. `insert` rejects a duplicate `DestinationId`
(`RegistryError::DuplicateDestination`) and a full registry
(`CapacityExceeded`); `reserve_command_slot` enforces
`MAX_AGGREGATE_COMMAND_QUEUE_DEPTH` (`CommandQueueFull`).
`remove` shuts the destination down *before* de-registering it, so no stale
pool entry, queued payload, or retained LeaseSet2 outlives a removed
destination.

### Lifecycle states and health projection

| State | Meaning | `health()` | accepts payloads | publishable |
| --- | --- | --- | --- | --- |
| `Initializing` | Identity exists; no tunnel work admitted yet. | `Starting` | yes | no |
| `BuildingTunnels` | Tunnels admitted, but minimum usable inbound + one usable outbound not both met. | `Starting` | yes | no |
| `Usable` | The LeaseSet2 lifecycle holds a signed, self-validated record **and** pool readiness is true. | `Ready` | yes | yes |
| `Degraded` | Was usable; lost the LeaseSet2 or its usable tunnels. | `Degraded(DependencyUnavailable)` | yes | no |
| `Stopping` | Shutdown requested; no new tunnels, LeaseSet2, or payload. | `Stopping` | no | no |
| `Stopped` | Every destination-owned resource released. | `Failed` | no | no |

A destination is `Usable` only when the tunnel-pool readiness check returns
`true` **and** the LeaseSet2 lifecycle holds a valid record — keys alone never
satisfy readiness. `DestinationState::health()` projects onto
`i2pr_core::HealthState` so the daemon can compose destinations into its
existing service graph without a parallel health model.

### Bounded payload contracts

- `DestinationPayload { protocol: u8, bytes: Vec<u8> }` with a hard
  `MAX_DESTINATION_PAYLOAD_BYTES` ceiling (32 KiB), enforced in the
  constructor.
- `BoundedPayloadQueue` with per-destination `max_pending_messages` and
  `max_pending_bytes` ceilings, FIFO ordering, and exact `release_all`
  accounting.
- `QueuedOutbound { queued_messages, queued_bytes, routing:
  RoutingUnavailable }` — the only currently reachable disposition for an
  accepted outbound payload. `RoutingUnavailable::AwaitingGarlicSessionLayer`
  records the Plan 121 boundary; `AwaitingDestinationRouting` records the
  Plan 122 boundary.

`PayloadError` variants are `QueueFull`, `QueueBytesExceeded`, `Stopping`,
`EmptyBody`, and `BodyTooLarge`. No payload is ever injected directly into
tunnel delivery as a shortcut around the Garlic session layer.

### Streaming core

The wire codec is owned by
[`i2pr-proto::streaming`](../../crates/i2pr-proto/src/streaming/mod.rs); the
client crate owns the state machine that drives it. Verified from source:

- **Normative flag map with M6 policy sets** — initial SYN `0x04A9`, SYN
  response `0x00A9`, CLOSE `0x000A`, RESET `0x000C`; flag-driven option
  ordering (DELAY / FROM / MAX / SIGNATURE).
- **No option TLVs** — the option area carries only the flag-defined fields.
- **Payload-only `MAX_PACKET_SIZE`** — a 2-byte big-endian integer bounding the
  *payload only*. `DEFAULT_ADVERTISED_MAX_PAYLOAD` = 1812 (the Plan
  313-qualified i2pd-compatible service profile); the hard safety ceiling
  `MAX_STREAMING_PAYLOAD_BYTES` = 2048 is declared separately from it and a
  compile-time invariant rejects any profile that exceeds it; the full
  encoded packet bound is an independent checked sum
  (`MAX_STREAMING_PACKET_BYTES`). Negotiation takes
  `min(local advertised, remote advertised)`.
- **Raw final signatures** — variable-length, with the length coming from the
  signing context: the FROM destination, or the peer signing key retained on
  the connection for CLOSE/RESET (which carry no FROM since I2P 0.9.20). The
  canonical zeroed-placeholder preimage is signed once.
- **Replay NACKs on SYN only** — eight Proposal 164 NACK words on the initial
  SYN only; `validate_initial_syn` and `validate_syn_response` are separate.
- **Retransmit / ACK / reorder** — `poll_retransmits` re-emits tracked original
  requests under a cap (`MAX_RETRANSMIT_ATTEMPTS` = 16, RTO clamped to
  `MIN_RTO_MILLIS` 100 … `MAX_RTO_MILLIS` 60 000); cumulative `ackThrough` is
  applied on receipt and clears tracked packets; delivered bytes are surfaced
  in order after reorder through `drain_delivered`.
- **CLOSE / RESET policy** — a side never marks itself `Closed` merely because
  it queued a CLOSE and nothing delivers after a RESET.
- **Stream ownership** — `outbound_by_stream` and retransmit bookkeeping live
  on the canonical outbound `StreamingManager`. The local delivery seam peeks
  the recovered gzip protocol-6 header + 16-byte fixed header: a SYN response
  (`FLAG_SYNCHRONIZE` set, `send_stream_id != 0`, `receive_stream_id != 0`)
  lands on `LocalDeliveryReceiver::canonical_streaming`; every other
  streaming packet (inbound SYN, data, CLOSE, RESET) lands on the receiver-side
  mirror. The recovered packet bytes are then passed unchanged to
  `StreamingDestinationAdapter::receive`.
- **Plan 152 receive-window / ACK gating (no wire change)** — undrained
  delivered bytes are capped per connection at
  `max_recv_window_packets × MAX_PACKET_PAYLOAD_BYTES`
  (`delivered_cap_bytes()`). While over cap, a standalone ACK snoozes by one
  delayed-ACK interval instead of emitting (emitting would slide the peer
  window and grow the backlog without bound), and a piggybacked DATA carries
  `FLAG_NO_ACK`. An observed duplicate re-arms the coalesced standalone-ACK
  deadline to *now* (Plan 152 D2), so one lost ACK cannot strand the send
  window. ACK-only packets still never schedule ACKs (one-way invariant).
- **Plan 134 ceiling** — `TooFarAhead` packets cannot advance
  `highest_received` or contaminate later ACK/NACK and piggyback state.

`StreamingDestinationAdapter` bounds the gzip-encoded **complete** Streaming
packet against the client-payload/I2NP limit
(`MAX_STREAMING_ADAPTER_PAYLOAD_BYTES` = `MAX_CLIENT_PAYLOAD_BYTES` = 60 KiB)
and never against the negotiated application MTU, because the two concepts
are distinct. The inbound path requires an `I2npBody::Data` body, the
canonical gzip client payload, and protocol 6 for Streaming (a typed
`UnsupportedProtocol` outcome for future datagram/I2CP layers); it reads the
I2P source/destination ports and applies **no local TCP privileged-port
policy** (`source_port == 0` is legal end-to-end). The adapter never owns
sockets, timers, or DNS.

### Datagram substrate (Plans 291/368)

`DatagramManager` handles protocol 17/18 Datagram1/Raw and Proposal 163 protocol
19/20 Datagram2/Datagram3. Payloads, options, sender destinations, both queues,
and the 1024-entry Datagram2 replay cache are bounded. Datagram2 verifies its
signature over the recipient hash and rejects expired delegated keys/replays
before queueing. Its 1024-entry replay cache rejects new messages while all
entries are unexpired rather than evicting a live replay guard. Datagram3
exposes its source hash only as unauthenticated
metadata. Because datagrams carry no connection state, there is no
initiator/mirror split: inbound 17–20 payloads surface as
`LocalDeliveryOutcome::DatagramDelivered` and never reach `StreamingManager`.

### ELS2 client halves (Plans 332/333)

The two halves are deliberately asymmetric: publishing needs the *blinded
private* key (owner only) and resolving needs only the *blinded public* key
(anyone holding the b33 address). That asymmetry is why
`EncryptedLeaseSet2Resolver` can read a record addressed to it but can never
produce one, so it cannot be used to impersonate a service.

`EncryptedLeaseSet2Publisher` builds and signs the no-authorization type-5
record and its `DatabaseStore` hand-off; Plan 333 adds
`build_authorized_record` / `build_authorized_database_store`,
`resolve_with_auth`, `generate_client_dh_keypair`, and
`authorized_address` (which sets `B32_FLAG_REQUIRES_CLIENT_KEY`).
`EncryptedLeaseSet2Resolver::new_authorized` accepts such an address while
`::new` refuses it. Blinding/derivation is delegated to the in-repo
`i2pr_crypto::red25519` module (`BlindingDay`, `Red25519PrivateScalar`,
`Red25519PublicKey`, `sign`) plus `i2pr_netdb`'s `BlindingSchedule` /
`BlindedStorageKey` — this crate implements no local crypto primitive. A
configured lookup secret is never silently dropped: a wrong length is a hard
error, never a fallback to a no-secret resolve.

## Dependencies

[`crates/i2pr-client/Cargo.toml`](../../crates/i2pr-client/Cargo.toml):

**Production** — `i2pr-core`, `i2pr-crypto`, `i2pr-netdb`, `i2pr-proto`,
`i2pr-tunnel` (all `path =`), plus `rand_chacha`, `rand_core`, `thiserror`,
`x25519-dalek` (`features = ["static_secrets"]`), and `zeroize`.

**Dev** — `rand_core` with `features = ["std", "os_rng"]` (the same crate as
the production dependency, with extra features enabled for tests).

`[lints] workspace = true`, and `lib.rs` carries
`#![forbid(unsafe_code)]` (as does `streaming/mod.rs`).

The `scripts/check-dependency-direction.sh` allowlist entry for this crate is
exactly:

```python
"i2pr-client": {
    "i2pr-core", "i2pr-crypto", "i2pr-netdb", "i2pr-proto", "i2pr-tunnel"
},
```

Note that `i2pr-api`, `i2pr-service-tunnels`, and `i2pr-daemon` all list
`i2pr-client` as an allowed *inbound* edge; the direction is one-way.

## Tests

```text
cargo test --locked -p i2pr-client --all-targets -- --test-threads=1
```

Verified result at the audit date: **107 in-crate unit tests + 151 integration
tests across 17 test files = 258 tests, all passing**, 0 ignored, 0 failed.

### In-crate unit tests (`#[cfg(test)] mod tests`, 107)

| File | Tests | Covers |
| --- | --- | --- |
| `registry.rs` | 14 | Runtime construction, state transitions, install/refresh, shutdown release. |
| `identity.rs` | 13 | Plan 223 F1/F2 identity shape, LS2/X25519 independence, import invariants, redacted `Debug`. |
| `pool.rs` | 11 | Admission, direction rejection, failure/standby accounting, lease-source publication. |
| `leaseset.rs` | 10 | Build/sign/self-validate, monotonic `published`, rotation and withdrawal decisions. |
| `config.rs` | 9 | Ceiling enforcement and default values. |
| `datagram.rs` | 8 | Framing, sender authentication, bounded queues, negative paths. |
| `lease_selection.rs` | 5 | Expiry filtering, safety margin, RNG-driven selection. |
| `bundle.rs` | 4 | Multi-clove reply encoding under the payload ceiling. |
| `message.rs` | 4 | Payload ceiling, queue capacity 1 / exact / max+1, `release_all` accounting. |
| `dispatch.rs` | 3 | Garlic classification, sender-LS2 binding, destination-hash ownership rejection. |
| `routing.rs` | 3 | Lease install, reverse routing, `Garlic` carrier construction. |
| `testing.rs` | 3 | Deterministic fixture reproducibility. |
| `streaming/manager.rs` | 7 | ACK/NACK, reorder, retransmit, Plan 152 D1/D2 gating, Plan 134 ceiling. |
| `streaming/recv_window.rs` | 5 | Admission, `TooFarAhead`, window boundaries. |
| `streaming/local_delivery.rs` | 4 | Seam routing, canonical/mirror split. |
| `streaming/clock.rs` | 2 | `SystemClock` monotonicity (the only `std::thread::sleep` in `src/`, and it is inside `#[cfg(test)]`). |
| `streaming_adapter.rs` | 2 | Adapter ceiling and inbound protocol gating. |

### Integration tests (`tests/`, 17 files, 151 tests)

| File | Tests | Proves |
| --- | --- | --- |
| [`plan120_trajectory.rs`](../../crates/i2pr-client/tests/plan120_trajectory.rs) | 1 | Plan 120 §12 full production-seam trajectory: create destination → reach `Established` → admit real `EstablishedMaterial` → derive `Lease2` → build/sign/self-validate LeaseSet2 → advance time → evict → replace → shut down. |
| [`plan121_trajectory.rs`](../../crates/i2pr-client/tests/plan121_trajectory.rs) | 4 | Plan 126 corrected primitive-level NS → NSR → bidirectional ES. |
| [`plan122_trajectory.rs`](../../crates/i2pr-client/tests/plan122_trajectory.rs) | 4 | Plan 122 §13 deterministic local two-destination composition. |
| [`plan123_trajectory.rs`](../../crates/i2pr-client/tests/plan123_trajectory.rs) | 16 | Plan 125 retained VirtualWire Streaming-only fault tests. |
| [`plan124_trajectory.rs`](../../crates/i2pr-client/tests/plan124_trajectory.rs) | 11 | Plan 124 Phases A–G corrected destination routing, including the canonical `authenticated-router-link-bypassed-local-seam` boundary and byte identity at the OBEP. |
| [`plan125_trajectory.rs`](../../crates/i2pr-client/tests/plan125_trajectory.rs) | 6 | Plan 125 real SYN / SYN-response lifecycle and gzip wire format. |
| [`plan126_trajectory.rs`](../../crates/i2pr-client/tests/plan126_trajectory.rs) | 9 | Manager lifecycle plus eight negative controls: ES replay, unknown tag, NSR-after-acceptance, duplicate bound NS, cross-destination isolation, pending capacity, idle expiry, too-short classification. |
| [`plan127_trajectory.rs`](../../crates/i2pr-client/tests/plan127_trajectory.rs) | 16 | Master NS → NSR → ES ×4 destination-routing closure plus §9 negative controls. |
| [`plan128_trajectory.rs`](../../crates/i2pr-client/tests/plan128_trajectory.rs) | 7 | Stream-id ownership, CLOSE/RESET shapes, `min(local, remote)` negotiation. |
| [`plan129_trajectory.rs`](../../crates/i2pr-client/tests/plan129_trajectory.rs) | 12 | Integrated destination + Streaming gate; persistent inbound chains across ordinary deliveries. |
| [`plan130_trajectory.rs`](../../crates/i2pr-client/tests/plan130_trajectory.rs) | 11 | Final wire/runtime corrective: frozen simple-ACK byte fixture, reference ACK/NACK table, sequence transition, one-way delayed ACK, piggyback suppression, reorder + NACK convergence, port authority and wildcard fallback, replay-layer separation, production-Elligator establishment. |
| [`plan131_trajectory.rs`](../../crates/i2pr-client/tests/plan131_trajectory.rs) | 7 | Production Elligator branch randomization, connection-owned I2P port tuple asserted on every established send API, side-effect-free oversized `send_data` rollback, independent three-layer replay separation. |
| [`plan132_trajectory.rs`](../../crates/i2pr-client/tests/plan132_trajectory.rs) | 10 | Artifact-preserving test seam, three layer-isolated replay trajectories, transactional `send_data`/`send_close`/`send_reset` ordering. |
| [`plan166_trajectory.rs`](../../crates/i2pr-client/tests/plan166_trajectory.rs) | 12 | Client-owned construction without a signing secret, valid install, mismatched decryption key, foreign lease, unknown-pool lease after eviction, duplicate lease, rotation requesting refresh, refresh via install, shutdown releasing every resource, install while stopping, router-owned rejection of client install. |
| [`plan296_trajectory.rs`](../../crates/i2pr-client/tests/plan296_trajectory.rs) | 2 | Garlic reply bundling and the bundled-reply delivery path. |
| [`els2_publish_resolve.rs`](../../crates/i2pr-client/tests/els2_publish_resolve.rs) | 7 | Plan 332 ELS2 no-authorization publish → b33 resolve round trip. |
| [`els2_authorized_publish_resolve.rs`](../../crates/i2pr-client/tests/els2_authorized_publish_resolve.rs) | 16 | Plan 333 authorized publish/resolve, including the client-DH keypair path and the `B32_FLAG_REQUIRES_CLIENT_KEY` address shape. |

### Determinism

- All randomness is caller-supplied. Tests drive `ChaCha8Rng::seed_from_u64`
  and `rand_core` test seeds; production entry points take `&mut R:
  TryCryptoRng` / `CryptoRng + RngCore`.
- `EstablishedMaterial` fixtures come from
  `i2pr_client::testing::established_inbound` /
  `established_outbound`, which drive the **real** `ShortBuildStateMachine`
  to `Established` with a deterministic responder — so the pool is exercised
  against real tunnel machinery, not a stub.
- Time is always injected (`now_seconds`, `now_ms`, `ManualClock`). The only
  `std::thread::sleep` in the crate is inside a `#[cfg(test)]` module
  (`streaming/clock.rs`) and inside `plan125_trajectory.rs`; there are no
  wall-clock sleeps in production paths.
- Bounded negative paths are covered systematically: capacity 1, exact load,
  and max+1 for queues; lease foreign/expired/duplicate/outlives-tunnel;
  session replay/unknown-tag/idle-expiry/capacity; dispatch
  `MissingSenderLeaseSet2` / `SenderKeyMismatch` / `LeaseSet2Validation` /
  `UnknownDestination`; install ownership / stopping / key-mismatch paths.
- No test reaches private state, a third-party ElGinator type, sockets, DNS, or
  any external I2P reference.

## Distinctive design choices

1. **One destination runtime, two ownership modes.** `DestinationOwnership` is
   a marker on a single `DestinationRuntime`; the client-owned path is a
   capability, not a second stack, so the ECIES primitives and dispatcher never
   branch on ownership.
2. **The router never holds a client-owned signing key.** Client-owned
   destinations can read traffic and can never sign a LeaseSet2 — refresh is a
   typed request back to the client.
3. **Atomic install or nothing.** Every `install_client_lease_set2` check runs
   before any mutation, so a mismatched decryption key can never be
   published-then-fixed and a lease set is never advertised without the secret
   that can actually read it.
4. **Lease material is never synthesized.** `LeaseRequest` is mapped from the
   destination's own real inbound pool, so a client cannot be asked to publish
   a lease the router does not actually own.
5. **Sessions are keyed by remote static key, tags are indexed, and both
   windows are bounded.** A single `RemoteStaticKey` map plus a pre-derived
   reply-tag → slot index makes classification O(log n) with no unbounded
   scan, and consumed tags leave the window so replay decrypts to
   `UnknownSessionTag`.
6. **The outbound form is a state machine, not a guess.**
   `PlannedOutboundForm` fixes the precedence (retained NSR context → live
   paired session → fresh bound NS), and `drop_provisional_responder()` makes
   "no NSR for an unbindable session" a structural guarantee.
7. **The remote identity comes only from a validated record.** `resolve_remote_destination`
   searches validated sender LeaseSet2 records and then the router-side store;
   it never hashes static-key bytes, an NSR tag, or an ES tag into an
   identity.
8. **`garlic_i2np_bytes` is the only carrier the tunnel sees.** The plaintext
   `inner_envelope_bytes` is retained for diagnostics only, so a regression
   that fed plaintext to the tunnel data plane is visible in the plan
   structure rather than silent.
9. **One-shot established material makes tunnel key reuse a type error.**
   `EstablishedMaterial` is non-`Clone` and `into_established_tunnel` returns
   `None` on a second call.
10. **Local zero-hop is a typed route, not an empty remote tunnel.** Remote
    `EstablishedTunnel::new()` still rejects empty hops and `MIN_HOPS` stays
    1, so a local convenience can never be mistaken for transit.
11. **The crate is synchronous and clock-driven.** The Streaming state machine
    is fully deterministic; the runtime owns timers, sockets, and channels, and
    this crate owns none of them.
12. **ELS2 publish and resolve are asymmetric on purpose.** The resolver can
    read a record addressed to it and can never produce one, which is what
    makes impersonation structurally impossible rather than merely
    discouraged.

## Boundary and security notes

Audit findings worth recording. **No production code was changed for this
refresh.**

- **Runtime neutrality verified.** `bash scripts/check-runtime-boundaries.sh`
  → `runtime boundary checks passed`. A manual grep over `src/` and `tests/`
  found no `tokio::*`, no `std::net`, no `std::fs`, no `async fn` /
  `async move`, no `futures::*`, no `spawn`, and no unbounded channel. The only
  `std::` roots used are `std::collections::{BTreeMap, BTreeSet, VecDeque}`,
  `std::sync::Arc`, `std::time::Instant`, and — inside `#[cfg(test)]` only —
  `std::thread::sleep` and `std::time::Duration`.
- **Grouped-`use` blind spot checked manually.** The checker's literal greps
  are per-path, so a `std::net` import hidden inside a grouped
  `use std::{…}` block could slip past. Every `use std` in this crate was
  enumerated by hand: 15 occurrences, 2 of them grouped
  (`use std::collections::{BTreeMap, VecDeque}` in `session.rs` and
  `dispatch.rs`, plus `BTreeMap, BTreeSet, VecDeque` in `streaming/manager.rs`
  and `BTreeMap, BTreeSet` in `streaming/send_window.rs` and
  `BTreeMap, VecDeque` in `tests/plan125_trajectory.rs`). **None of them
  references `net`, `fs`, or any I/O path.** This crate has no grouped-use
  blind spot.
- **`DestinationDispatcher::bind_destination_hash` index is write-only.** The
  `destination_hashes: BTreeMap<DestinationHash, DestinationId>` map
  (`dispatch.rs:337`) is inserted by `bind_destination_hash`
  (`dispatch.rs:419`) and cleared by `unregister_destination`
  (`dispatch.rs:426`), but it is never *read* anywhere in the crate. The
  actual anti-spoofing guard is the direct comparison in
  `route_application_clove`: the clove's `GarlicDelivery::Destination(bytes)`
  target hash must equal `*local_id.as_hash()`, else
  `InboundDispatchError::UnknownDestination`. The documented invariant is
  therefore **enforced** — by a different mechanism than the index suggests —
  and the index is currently redundant. Not fixed here (product code is out of
  scope for a doc refresh); flagged for the owning lane.
- **`InboundDecryptionCapability` `Drop` zeroes only the public field.** The
  secret is an `i2pr_crypto::X25519PrivateKey`, which owns its own
  `ZeroizeOnDrop`; the wrapper zeroes `static_public_bytes` (redundant but
  harmless) and does not keep a second copy of the secret. Documented here so
  the "zeroized on drop" claim is not read as "the wrapper does the zeroing".
- **`DestinationIdentity::signing_seed_bytes()` is a live secret accessor.**
  It is documented in-source as reserved for the `i2pr-api` SAM
  `PrivateKeyFile` codec, with an explicit instruction not to add consumers
  without an API review. The narrow-accessor rule for the identity is
  otherwise intact.

## Cross-references

### ADRs

- [ADR 0026 — Staged interoperability progression and retained Java-router
  compatibility debt](../adr/0026-staged-interoperability-progression-and-java-debt.md)
- [ADR 0028 — I2PControl Proposal 170 control plane](../adr/0028-i2pcontrol-proposal-170-control-plane.md)
  (source of the Plan 170 legacy encryption-slot relocation this crate
  implements)
- [ADR 0029 — Anonymity boundaries, implementation neutrality, and profile
  convergence](../adr/0029-anonymity-boundaries-and-profile-convergence.md)
- [ADR 0030 — Destination linkability domains, service lifecycle separation,
  and i2pd Streaming convergence](../adr/0030-destination-linkability-domains-service-lifecycle-and-i2pd-streaming.md)

### Plan and closure records

Closure records win over `plans/registry.md` for status.

- Plan 120 — destination lifecycle and tunnel pools:
  [`plans/closure/destination-streaming/120-status.md`](../../plans/closure/destination-streaming/120-status.md)
- Plan 122 / 124 — destination routing and its corrective:
  [`124-status.md`](../../plans/closure/destination-streaming/124-status.md),
  [`124-m6-plan122-destination-routing-corrective-closure.md`](../../plans/closure/destination-streaming/124-m6-plan122-destination-routing-corrective-closure.md)
- Plans 126 / 127 — ECIES ratchet and session routing:
  [`126-status.md`](../../plans/closure/destination-streaming/126-status.md),
  [`127-status.md`](../../plans/closure/destination-streaming/127-status.md),
  [`127-m6-destination-session-routing-final-closure.md`](../../plans/closure/destination-streaming/127-m6-destination-session-routing-final-closure.md)
- **Plan 128 — Streaming wire protocol corrective (authoritative):**
  [`128-m6-streaming-wire-protocol-corrective-closure.md`](../../plans/closure/destination-streaming/128-m6-streaming-wire-protocol-corrective-closure.md)
  with provenance in
  [`specs/references/streaming-packet-wire.md`](../../specs/references/streaming-packet-wire.md)
- Plan 130 — final wire/runtime corrective:
  [`130-status.md`](../../plans/closure/destination-streaming/130-status.md)
- **Plan 134 — current M6 local closure authority:**
  [`134-status.md`](../../plans/closure/destination-streaming/134-status.md)
  (`passed-milestone6-recv-window-ack-ceiling-closure`)
- Plan 143 / 144 — local delivery seam and the canonical/mirror split:
  [`plans/closure/sam/143-status.md`](../../plans/closure/sam/143-status.md)
- Plans 151 / 152 — SAM final acceptance and the retained narrow M6
  session/streaming robustness corrective that sits *underneath* Plan 151:
  [`151-status.md`](../../plans/closure/sam/151-status.md),
  [`152-status.md`](../../plans/closure/sam/152-status.md)
- **Plan 166 — client-owned destination and LeaseSet2 bridge (M9):**
  [`plans/closure/i2cp/166-status.md`](../../plans/closure/i2cp/166-status.md)
  (`passed-m9-i2cp-client-owned-destination-and-leaseset2`)
- Plan 193 — i2pd mixed-router Streaming qualification (first-family):
  [`193-status.md`](../../plans/closure/mixed-router-interop/193-status.md),
  [`193-streaming-status.md`](../../plans/closure/mixed-router-interop/193-streaming-status.md)
- Plans 201 / 204 / 247 — Java full-router compatibility retained deferred:
  [`201-status.md`](../../plans/closure/mixed-router-interop/201-status.md),
  [`204-status.md`](../../plans/closure/service-tunnels/204-status.md),
  [`247-status.md`](../../plans/closure/mixed-router-interop/247-status.md)
- Plan 223 — Destination crypto-layer separation (ElGamal identity slot vs
  X25519 LS2 key)
- Plans 332 / 333 — ELS2 publish/resolve and client authorization

### Specifications

- [`specs/support.toml`](../../specs/support.toml) — machine-readable support
  inventory (`milestone6_local_product = "passed"`,
  `milestone6_interoperable = "not-yet-claimed"`,
  `m6_java_full_router_compatibility = "retained-deferred-at-plan247"`,
  `m6_full_two_family_router_conformance = "not-yet-claimed"`)
- [`specs/CONFORMANCE.md`](../../specs/CONFORMANCE.md)
- [`specs/references/streaming-packet-wire.md`](../../specs/references/streaming-packet-wire.md)
- [`specs/references/elligator2-production-representation.md`](../../specs/references/elligator2-production-representation.md)

### Related deep dives

- [i2pr-proto.md](i2pr-proto.md) — the Streaming wire codec, `Destination`,
  `LeaseSet2`, datagram protocol constants
- [i2pr-crypto.md](i2pr-crypto.md) — Ed25519/X25519 wrappers, `ecies`,
  `red25519`
- [i2pr-netdb.md](i2pr-netdb.md) — `ValidatedLeaseSet2`, `LeaseSet2Store`, ELS2
  blinding
- [i2pr-tunnel.md](i2pr-tunnel.md) — `ExploratoryPool`, `ShortBuildStateMachine`,
  one-shot `EstablishedMaterial`
- [i2pr-core.md](i2pr-core.md) — `HealthState` projection target
- [i2pr-api.md](i2pr-api.md) — SAM 3.1 / I2CP consumers of this crate
- [i2pr-daemon.md](i2pr-daemon.md) — CLI/config/composition root; owns the
  SAM, I2CP, and service-tunnel listeners
- [i2pr-service-tunnels.md](i2pr-service-tunnels.md) — tunnel config/policy
  consumer
- [overview.md](overview.md), [dependency-graph.md](dependency-graph.md),
  [tooling.md](tooling.md)

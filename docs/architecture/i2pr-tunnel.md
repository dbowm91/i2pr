# `i2pr-tunnel` — Deep Dive

Runtime-neutral tunnel identity, bounded exploratory tunnel pool,
build-record layout surface, ECIES-X25519 Noise-N short-build
cryptography and state machine, the canonical production I2NP bridge,
the local tunnel data plane, the runtime-neutral outbound/inbound/transit
role composition, the M11 transit admission foundation, and the
reply-path provider.

Path: `crates/i2pr-tunnel/`

> Status: experimental. Not production-ready. The local short-build
> conformance floor closed at `passed-final-local-closure`
> ([`plans/closure/exploratory-tunnels/116-status.md`](../../plans/closure/exploratory-tunnels/116-status.md));
> Plan 117 closed as
> `closed-for-progression-with-evidence-gap`
> ([`plans/closure/exploratory-tunnels/117-status.md`](../../plans/closure/exploratory-tunnels/117-status.md)).
> **M11 transit passed a ONE-FAMILY experimental progression (Plan 268) —
> public transit participation remains DISABLED, NON-ADVERTISED, and
> UNCLAIMED** (see
> [M11 transit](#m11-transit-the-m11-lane) and ADR 0026). Local
> zero-hop types are a LOCAL delivery convenience, not a network
> capability (see [Local zero-hop](#local-zero-hop-plan-172)).

## Purpose

`i2pr-tunnel` owns:

- typed tunnel identity ([`identity`](../../crates/i2pr-tunnel/src/identity.rs))
  — `TunnelId`, `TunnelDirection`, `TunnelRole`, `TunnelLifetime`,
  `TunnelState`, `TunnelPeer`;
- bounded exploratory pool configuration
  ([`config`](../../crates/i2pr-tunnel/src/config.rs)) —
  `ExploratoryPoolConfig` plus the `MAX_*` / `MIN_HOPS` ceilings;
- the deterministic
  [`pool::ExploratoryPool`](../../crates/i2pr-tunnel/src/pool.rs) with
  bounded replacement, caller-injected expiry, failure accounting, and
  the `select_inbound_reply_path` selector;
- the [`build::BuildRecordLayout`](../../crates/i2pr-tunnel/src/build.rs)
  surface over `i2pr_proto::DeferredBuildRecords` and the typed
  `BuildRequestKind` / `BuildReplyKind` I2NP message-type markers;
- the [`build_crypto`](../../crates/i2pr-tunnel/src/build_crypto.rs)
  seam: the `BuildCryptography` trait, the zeroizing `LayerKeys`, the
  typed `ValidatedRecordSlot` nonce, the `NoBuildCryptography` default,
  and the locally conformant ECIES-X25519 Noise-N primitive;
- the typed
  [`short_record`](../../crates/i2pr-tunnel/src/short_record.rs)
  154-byte request / 202-byte reply plaintext codecs;
- the runtime-neutral
  [`short`](../../crates/i2pr-tunnel/src/short.rs) build state machine
  and its one-shot `EstablishedMaterial` transfer;
- the success-only
  [`short_state::ShortBuildRegistrar`](../../crates/i2pr-tunnel/src/short_state.rs);
- the Plan 110
  [`multirecord`](../../crates/i2pr-tunnel/src/multirecord.rs)
  record-set, preprocessing, per-hop processor, and count-prefixed
  STBM/OTBRM contract helpers;
- the independent reference
  [`conformance_fixtures`](../../crates/i2pr-tunnel/src/conformance_fixtures.rs)
  and the frozen
  [`fixed_vectors`](../../crates/i2pr-tunnel/src/fixed_vectors.rs);
- the deterministic
  [`responder::DeterministicResponder`](../../crates/i2pr-tunnel/src/responder.rs)
  peer simulator;
- the Plan 115
  [`bridge::ShortBuildI2npBridge`](../../crates/i2pr-tunnel/src/bridge.rs)
  — the canonical production seam to a complete I2NP type-25 message;
- the local data plane —
  [`layer`](../../crates/i2pr-tunnel/src/layer.rs) AES-256 layer
  transforms and duplicate window,
  [`data`](../../crates/i2pr-tunnel/src/data.rs) Tunnel Message
  Specification builder/parser,
  [`fragment`](../../crates/i2pr-tunnel/src/fragment.rs) bounded
  reassembly,
  [`established`](../../crates/i2pr-tunnel/src/established.rs)
  secret-material ownership;
- the runtime-neutral role composition
  ([`roles`](../../crates/i2pr-tunnel/src/roles.rs)) and its bounded
  activation state
  ([`data_plane_registry`](../../crates/i2pr-tunnel/src/data_plane_registry.rs));
- the M11 transit admission / participant foundation
  ([`transit`](../../crates/i2pr-tunnel/src/transit.rs)) — **infrastructure
  only; M11 capability is not claimed**;
- the bounded OBEP garlic reply handling
  ([`garlic_reply`](../../crates/i2pr-tunnel/src/garlic_reply.rs));
- the Plan 172 local zero-hop types
  ([`zero_hop`](../../crates/i2pr-tunnel/src/zero_hop.rs));
- the [`provider`](../../crates/i2pr-tunnel/src/provider.rs) adapter
  that turns a pool into an `i2pr_netdb::ReplyPathProvider`.

`i2pr-tunnel` must **not** own:

- sockets, `tokio`, timers, channels, cancellation, or task spawning —
  `i2pr-runtime` is the sole production owner and `i2pr-daemon` owns
  composition (see
  [Dependencies](#dependencies));
- daemon composition of the NetDB / M6 / M11 seams. The crate exposes
  typed actions and registries; the daemon adapts them to the
  transport boundary;
- any capability advertisement. Nothing in this crate is advertised
  or claimed beyond the tested subset in
  [`specs/CONFORMANCE.md`](../../specs/CONFORMANCE.md).

## Module layout

The crate ships **23 modules** at the crate root. Line counts are real
`wc -l` counts on the current head; the crate is 27 402 lines total.

| Module | File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- | --- |
| `lib` | `lib.rs` | 200 | Crate root, `#![forbid(unsafe_code)]`, 23 `pub mod` declarations, root re-exports, `BoundedTunnelPool` / `BoundedTunnelPoolConfig` aliases (Plan 120) | `BoundedTunnelPool`, `BoundedTunnelPoolConfig` |
| `transit` | `transit.rs` | 6 634 | M11 lane: typed `m`/`r`/`l`/`b` bandwidth interpretation, `TransitAdmissionPolicy` / `TransitAdmissionState` taxonomy, the dedicated `TransitRegistry`, the `process_short_build_request` / `process_short_build_message` transactions, and the participant/IBGW/OBEP transit data plane | `TransitBandwidthRequest`, `TransitBandwidthReply`, `TransitAdmissionPolicy`, `TransitAdmissionState`, `TransitRegistry`, `TransitHopRole`, `TransitDataPlane`, `TransitBuildRoute`, `process_short_build_request`, `process_short_build_message` |
| `short` | `short.rs` | 2 548 | Runtime-neutral build state machine: `Prepared → Protecting → ReadyForDelivery → AwaitingReply → Established` plus six terminal failures; `ShortBuildPath::validate` topology rules, exact count-prefixed delivery validation, one-shot `take_established_material` | `ShortBuildStateMachine<C>`, `ShortBuildPath`, `HopSpec`, `HopCryptoContext`, `BuildAttemptId`, `BuildEvent`, `ShortBuildAction`, `ShortBuildOutcome`, `ShortTunnelBuildMessage` |
| `multirecord` | `multirecord.rs` | 2 388 | Plan 110 record-set + preprocessing: slot assignment, originator fake, raw ChaCha20 transforms, `MessageHopProcessor`, `CreatorReplyPostprocessor`, and the count-prefixed STBM/OTBRM contract helpers | `ShortBuildRecordSet`, `OriginatorFake`, `MessageHopProcessor`, `CreatorReplyPostprocessor`, `RecordAssignment`, `MultiRecordHopSpec`, `prepare_short_build_message`, `validate_count_prefixed_short_payload` |
| `roles` | `roles.rs` | 2 181 | Runtime-neutral outbound/inbound/local role composition with CSPRNG-injected IVs and padding and multi-cell `forward_cells` / `process_cells` seams | `OutboundGatewayRole`, `OutboundParticipantRole`, `InboundParticipantRole`, `OutboundEndpointRole`, `InboundGatewayRole`, `LocalInboundEndpointRole`, `RouterDeliveryAction`, `OutboundCell` |
| `fragment` | `fragment.rs` | 1 691 | `BoundedReassembler` with caller-driven expiry, per-message + aggregate byte bounds, `classify()`-based duplicate identity, and first-fragment `DeliveryInstruction` retention | `BoundedReassembler`, `TunnelFragment`, `ReassemblyKey`, `ReassembledFragment`, `ReassemblyError` |
| `data` | `data.rs` | 1 613 | Canonical Tunnel Message Specification checksum builder/parser, automatic complete-message fragmentation, and the overhead constants | `TunnelMessageBuilder`, `TunnelMessageParser`, `DeliveryInstruction`, `FragmentDelivery`, `TunnelPayloadHeader`, `TunnelMessageError` |
| `build_crypto` | `build_crypto.rs` | 1 348 | `BuildCryptography` seam + the ECIES-X25519 Noise-N primitive that seals a 154-byte plaintext into a 218-byte envelope and authenticates the 202-byte hop-own reply | `BuildCryptography`, `EciesX25519BuildCryptography`, `NoBuildCryptography`, `LayerKeys`, `ValidatedRecordSlot`, `SealedShortRequest`, `OpenedShortRequest`, `NoiseRequestState` |
| `pool` | `pool.rs` | 1 292 | Deterministic bounded pool: registration with material, bounded replacement, expiry, failure accounting, activation | `ExploratoryPool`, `TunnelSlot`, `TunnelRegistration`, `TunnelEntry`, `RegisterOutcome`, `RegisterError`, `PublicTunnelRouting`, `MaterialState` |
| `short_record` | `short_record.rs` | 1 260 | Typed 154-byte request and 202-byte reply plaintext codecs with the CSPRNG / deterministic split and strict role/encryption-type validation | `ShortRequestRecord`, `ShortReplyRecord`, `HopRole`, `LayerEncryptionType`, `ShortResponseCode`, `BuildOptions`, `ShortBuildError` |
| `established` | `established.rs` | 967 | Secret-material ownership for a live tunnel with `Option<EstablishedNextHop>` next-hop state and one-shot `into_extracted` / `into_established_tunnel` transfer | `EstablishedTunnel`, `EstablishedHop`, `EstablishedMaterial`, `EstablishedNextHop`, `EstablishedRole`, `EstablishedTunnelError` |
| `garlic_reply` | `garlic_reply.rs` | 821 | Plan 188 bounded OBEP garlic unwrap: tag-gated ChaCha20-Poly1305 decrypt plus local ShortTunnelBuildReply clove parse, and the OBEP wrap side | `decrypt_build_reply_garlic`, `wrap_obep_reply_garlic`, `GarlicClove`, `DecryptedBuildReply`, `GarlicReplyError` |
| `data_plane_registry` | `data_plane_registry.rs` | 764 | Bounded activation state for local roles keyed by slot and local receive tunnel id, with the Plan 190 typed `InboundGatewayRoute` | `DataPlaneRegistry`, `DataPlaneCapacity`, `InboundGatewayRoute`, `RegistryRemoval`, `RegistryError` |
| `bridge` | `bridge.rs` | 668 | Plan 115 canonical production seam from `ShortBuildAction::Deliver` to one complete I2NP type-25 message with a no-double-prefix invariant | `ShortBuildI2npBridge`, `BridgeHeader`, `BridgeRecord`, `BridgeError` |
| `layer` | `layer.rs` | 506 | AES-256 ECB/CBC/ECB per-hop layer transform plus the bounded duplicate window | `TunnelLayerTransform`, `DuplicateWindow`, `DuplicateToken`, `DuplicateWindowError` |
| `fixed_vectors` | `fixed_vectors.rs` | 407 | Frozen Noise-N / HKDF / ChaChaPoly conformance constants generated once from an independent reference oracle | `FIXED_SEALED_REQUEST`, `FIXED_REQUEST_KEYDATA`, `FIXED_REPLY_KEY` / `FIXED_LAYER_KEY` / `FIXED_IV_KEY`, `FIXED_OBEP_GARLIC_KEY` / `FIXED_OBEP_GARLIC_TAG` |
| `conformance_fixtures` | `conformance_fixtures.rs` | 367 | Independent reference Noise-N implementation and the canonical runtime-built fixture that drives the production primitive | `ReferenceFixture`, `FixtureRequest`, `DeterministicRng`, `verify_against_fixture`, `fixture_seal_reply` |
| `config` | `config.rs` | 350 | `ExploratoryPoolConfig` with hard ceilings, `balanced()`, and `try_new` validation | `ExploratoryPoolConfig`, `ExploratoryConfigError`, `MAX_EXPLORATORY_INBOUND`, `MAX_EXPLORATORY_OUTBOUND`, `MAX_BUILD_CONCURRENCY`, `MAX_FAILURE_THRESHOLD`, `MAX_HOPS`, `MIN_HOPS` |
| `identity` | `identity.rs` | 296 | Typed tunnel identity: non-zero `TunnelId`, direction, role, bounded lifetime, state, peer | `TunnelId`, `TunnelDirection`, `TunnelRole`, `TunnelLifetime`, `TunnelState`, `TunnelPeer`, `TunnelIdError`, `TunnelLifetimeError`, `MAX_TUNNEL_ID` |
| `build` | `build.rs` | 281 | `BuildRecordLayout` over `DeferredBuildRecords` and the typed I2NP message-type markers | `BuildRecordLayout`, `BuildRequestKind`, `BuildReplyKind`, `BuildRecordLayoutError`, `BuildCryptographyUnavailable` |
| `zero_hop` | `zero_hop.rs` | 264 | Plan 172 local zero-hop inbound/outbound types for loopback delivery | `LocalZeroHopInbound`, `LocalZeroHopOutbound`, `ZeroHopError`, `MAX_ZERO_HOP_LIFETIME_SECONDS` |
| `responder` | `responder.rs` | 228 | Deterministic single-hop peer simulator exercising the Noise-N primitive end-to-end | `DeterministicResponder`, `ResponderError`, `seal_into_envelope`, `open_and_seal_accepted` |
| `short_state` | `short_state.rs` | 218 | Success-only registrar that admits real `EstablishedMaterial` into `ExploratoryPool` | `ShortBuildRegistrar`, `ShortBuildState`, `HopResponse`, `ShortRegistrarError` |
| `provider` | `provider.rs` | 110 | `ReplyPathProvider` adapter over `ExploratoryPool` with caller-injected time | `ExploratoryPoolReplyPathProvider` |

`transit` (6 634 lines) and `roles` are the largest modules; together
they are 40% of the crate. `transit` and `roles` also both carry
module-level `#![allow(...)]` blocks for lint suppression that the
crate root does not (`transit`: `result_large_err`, `too_many_arguments`;
`roles`: `dead_code`, `unused_imports`, `type_complexity`).

## Public surface

The root `pub use` re-exports in
[`lib.rs`](../../crates/i2pr-tunnel/src/lib.rs) are the crate's
canonical surface. Per module:

**`bridge`** — `BridgeError`, `BridgeHeader`, `BridgeRecord`,
`ShortBuildI2npBridge`.

**`build`** — `BuildCryptographyUnavailable`, `BuildRecordLayout`,
`BuildRecordLayoutError`, `BuildReplyKind`, `BuildRequestKind`.

**`build_crypto`** — `AEAD_KEY_LEN`, `AEAD_NONCE_LEN`,
`BuildCryptography`, `BuildCryptographyError`, `EPHEMERAL_KEY_LEN`,
`EciesX25519BuildCryptography`, `HASH_PREFIX_LEN`, `LayerKeys`,
`NoBuildCryptography`, `NoiseRequestState`, `OpenedShortRequest`,
`SealedShortRequest`, `TAG_LEN`, `ValidatedRecordSlot`.
Module-public but **not** root re-exported: `NOISE_PROTOCOL_NAME`,
`RequestKeyMaterial`, `RECORD_SLOT_NONCE_OFFSET`,
`GARLIC_REPLY_TAG_LEN`, `record_slot`, `derive_layer_keys`.

**`config`** — `ExploratoryConfigError`, `ExploratoryPoolConfig`,
`MAX_BUILD_CONCURRENCY`, `MAX_EXPLORATORY_INBOUND`,
`MAX_EXPLORATORY_OUTBOUND`, `MAX_FAILURE_THRESHOLD`, `MAX_HOPS`,
`MIN_HOPS`.

**`data`** — `DeliveryInstruction`, `FragmentDelivery`,
`MAX_FRAGMENT_BODY_BYTES`, `MAX_FRAGMENT_COUNT`,
`MAX_PLAINTEXT_DATA_BYTES`, `MAX_TUNNEL_MESSAGE_PAYLOAD_BYTES`,
`TunnelMessageBuilder`, `TunnelMessageError`, `TunnelMessageParser`,
`TunnelPayloadHeader`. Module-public only: `CHECKSUM_LEN`,
`MAX_PADDING_RETRY_PER_BYTE`, and the private overhead helpers
`unfragmented_overhead` / `fragmented_first_overhead` /
`FOLLOW_ON_OVERHEAD` (see [Local data plane](#local-data-plane-plan-116)).

**`data_plane_registry`** — `DataPlaneCapacity`, `DataPlaneRegistry`,
`InboundGatewayRoute`, `RegistryError`, `RegistryRemoval`.

**`established`** — `EstablishedHop`, `EstablishedMaterial`,
`EstablishedNextHop`, `EstablishedRole`, `EstablishedTunnel`,
`EstablishedTunnelError`, `zero_id`, `zero_peer`. Module-public only:
`zero_hash`.

**`fragment`** — `BoundedReassembler`, `MAX_REASSEMBLY_AGGREGATE_BYTES`,
`MAX_REASSEMBLY_BYTES_PER_MESSAGE`, `MAX_REASSEMBLY_MESSAGES`,
`TunnelFragment`. Module-public only: `ReassembledFragment`,
`ReassemblyKey`, `ReassemblyError`, `FOLLOW_ON_SEQUENCE_MIN`,
`FOLLOW_ON_SEQUENCE_MAX`.

**`identity`** — `MAX_TUNNEL_ID`, `TunnelDirection`, `TunnelId`,
`TunnelIdError`, `TunnelLifetime`, `TunnelLifetimeError`, `TunnelPeer`,
`TunnelRole`, `TunnelState`.

**`layer`** — `DuplicateToken`, `DuplicateWindow`,
`DuplicateWindowError`, `TUNNEL_IV_LEN`, `TUNNEL_PAYLOAD_LEN`,
`TunnelLayerTransform`. Module-public only: `MAX_DUPLICATE_WINDOW`.

**`multirecord`** — a 33-item Plan 110 surface: `CHACHA20_KEY_LEN`,
`CreatorReplyPostprocessor`, `MAX_RECORD_COUNT`,
`MIN_PRODUCTION_RECORD_COUNT`, `MessageHopProcessor`, `MultiHopFixture`,
`MultiHopReferenceFixture`, `MultiRecordError`, `MultiRecordHopSpec`,
`OriginatorFake`, `PreparedHopContext`, `PreparedShortBuildMessage`,
`ProcessedHopResult`, `RECORD_BYTES`, `REPLY_PLAINTEXT_BYTES`,
`REQUEST_PLAINTEXT_BYTES`, `RecordAssignment`, `RecordOwner`,
`ShortBuildRecordSet`, `SlotIndex`, `assign_record_slots`,
`build_minimum_record_count`, `build_originator_fake_record`,
`build_padding_fake_record`, `chacha20_transform`, `chacha20_xor`,
`decode_outbound_tunnel_build_reply`, `decode_short_tunnel_build_payload`,
`encode_count_prefixed_short_payload`,
`encode_outbound_tunnel_build_reply`, `prepare_short_build_message`,
`validate_count_prefixed_short_payload`, `verify_originator_fake`.
Module-public only: `INBOUND_SHORT_BUILD_POLICY`.

**`pool`** — `ExploratoryPool`, `MAX_HOPS_PER_TUNNEL`, `PoolError`,
`PoolFullError`, `RegisterError`, `RegisterOutcome`, `RegistrationError`,
`TunnelEntry`, `TunnelRegistration`, `TunnelSlot`. Module-public only:
`ActivationError`, `PublicTunnelRouting`, `MaterialState`.

**`provider`** — `ExploratoryPoolReplyPathProvider`.

**`responder`** — `DeterministicResponder`, `ResponderError`.

**`roles`** — `InboundGatewayRole`, `InboundParticipantRole`,
`LocalInboundEndpointRole`, `OBGWRouterDelivery`, `OutboundCell`,
`OutboundEndpointRole`, `OutboundGatewayRole`, `OutboundParticipantRole`,
`RouterDeliveryAction`, `RouterDeliveryKind`, `TunnelRoleError`.
Module-public only: `build_test_established_outbound`, which is a
`#[allow(dead_code)]` **test helper that is not `#[cfg(test)]`-gated**.

**`short`** — `BuildAttemptId`, `BuildEvent`, `HopCryptoContext`,
`HopIndex`, `HopSpec`, `PerHopReply`, `ShortBuildAction`,
`ShortBuildConstructionError`, `ShortBuildOutcome`, `ShortBuildPath`,
`ShortTunnelBuildMessage`.

**`short_record`** — `BuildOptions`, `BuildOptionsError`, `HopRole`,
`LayerEncryptionType`, `REQUEST_EXPIRATION_SECONDS`, `ShortBuildError`,
`ShortReplyRecord`, `ShortRequestRecord`, `ShortResponseCode`.
Module-public only: `REQUEST_FIXED_PREFIX_LEN`, `HOP_ROLE_PARTICIPANT`,
`HOP_ROLE_INBOUND_GATEWAY`, `HOP_ROLE_OUTBOUND_ENDPOINT`,
`plaintext_size`, `classify_plaintext_kind`, `validate_reply_body`.

**`short_state`** — `HopResponse`, `ShortBuildRegistrar`,
`ShortBuildState`, `ShortBuildStateMachine` (a re-export of
`short::ShortBuildStateMachine`). Module-public only:
`RegistrarFullError` (alias of `pool::PoolFullError`) and
`ShortBuildDirectionError` (re-export of `identity::TunnelDirection`).

**`transit`** — 47 root re-exports: the bandwidth types
(`TransitBandwidthRequest`, `TransitBandwidthReply`,
`TransitBandwidthSummary`, `parse_transit_bandwidth_request`), the
admission taxonomy (`TransitAdmissionPolicy`, `TransitAdmissionState`,
`TransitAdmissionError`, `TransitAdmissionConfigError`), the registry
(`TransitRegistry`, `TransitRegistryError`, `TransitHopRegistration`,
`TransitHopRole`, `TransitHopRoleKind`), the data plane
(`TransitDataPlane`, `TransitDataOutcome`, `TransitDataFatalError`,
`TransitParticipantData`, `TransitGatewayData`, `TransitEndpointData`,
`TransitGatewayForward`, `gateway_nested_is_multicell_capable`), the
build transactions (`TransitBuildContext`, `TransitBuildOutcome`,
`TransitBuildMessageOutcome`, `TransitBuildRoute`, `TransitNow`,
`TransitReplySlot`, `process_short_build_request`,
`process_short_build_message`, `build_rejected_reply_record`,
`TransitFatalError`, `TransitMode`), and the 10 `MAX_TRANSIT_*` /
`TRANSIT_TIME_SKEW_SECONDS` / `BANDWIDTH_*_KEY` constants.

**`zero_hop`** — `LocalZeroHopInbound`, `LocalZeroHopOutbound`,
`MAX_ZERO_HOP_LIFETIME_SECONDS`, `ZeroHopError`.

**Not root re-exported at all** — `conformance_fixtures` and
`fixed_vectors` have **no** root re-exports; they are reachable only as
`i2pr_tunnel::conformance_fixtures::*` and
`i2pr_tunnel::fixed_vectors::*`. `garlic_reply` is likewise absent from
the root `pub use` list.

## Key contracts

### Tunnel identity and ID derivation

`TunnelId::new(value)` is `const` and rejects zero
(`TunnelIdError::Zero`); `TunnelId::get()` returns the inner `u32`.
`MAX_TUNNEL_ID` is `u32::MAX`, and the local zero-hop path additionally
rejects that value as a sentinel. `TunnelId` has hand-written `Debug`
and `Display` that render `{:08x}` hex, derives `Ord`/`Hash`, and
implements `Zeroize`.

**Per-hop tunnel IDs are explicit, never derived.** `HopSpec` and
`MultiRecordHopSpec` each carry independent `receive_tunnel` and
`next_tunnel: TunnelId` fields. The request-plaintext encoder never
derives a tunnel id from a router-hash prefix. The intermediate chain
invariant `hops[i].next_tunnel == hops[i+1].receive_tunnel` is enforced
at both the high-level `ShortBuildPath::validate()` and the public
lower-level `prepare_short_build_message()` (Plan 114).

`TunnelPeer` wraps a 32-byte `Hash`; `TunnelLifetime` is bounded
(seconds) and `TunnelState` is a plain typed enum.

### The exploratory pool and SUCCESS-ONLY registrar

`ExploratoryPool::register_inbound` / `register_outbound` return
`RegisterError::Full { kind, registration }` at capacity — the
registration is **returned, not silently dropped**. Replacement is
bounded; expiry is driven only by `advance_time(now_seconds)` (the pool
never reads the wall clock). `select_inbound_reply_path(now)` returns
the oldest valid inbound tunnel; expired and failed tunnels are never
returned.

**"Success-only" means the registrar admits real
`EstablishedMaterial`, not an outcome label.** `ShortBuildRegistrar`
exposes three surfaces:

- `admit_established_machine(&mut machine, now)` — canonical. Calls
  `ShortBuildStateMachine::take_established_material(now)`, then
  dispatches to `register_{inbound,outbound}_with_material` based on
  the material's own direction.
- `admit_material(established, now)` — takes already-extracted
  `EstablishedMaterial`.
- `admit(&ShortBuildOutcome, _, _)` — **legacy and fails closed**: an
  `Established` outcome returns
  `ShortRegistrarError::EstablishedMaterialRequired` (the outcome
  discards the layer keys, so the registrar cannot rebuild them) and
  every other outcome returns `ShortRegistrarError::NotEstablished`.

A build that never produces real material never enters the pool.
`ShortRegistrarError` variants: `NotEstablished`, `Construction`,
`Registration`, `Pool`, `AlreadyConsumed`,
`EstablishedMaterialRequired`.

`pool::build_placeholder_established` is `#[cfg(test)]` — production
code can never produce a fake established entry. (Note the asymmetry
with `roles::build_test_established_outbound`, which is
`#[allow(dead_code)]` but **not** `#[cfg(test)]`.)

Pool capacity bounds: `MAX_EXPLORATORY_INBOUND = 8`,
`MAX_EXPLORATORY_OUTBOUND = 8`, `MAX_BUILD_CONCURRENCY = 4`,
`MAX_FAILURE_THRESHOLD = 16`, `MAX_HOPS = 8`, `MIN_HOPS = 1`,
`MAX_HOPS_PER_TUNNEL = 8`. `ExploratoryPoolConfig::balanced()` is
`max_inbound: 4, max_outbound: 4, length_hops: 2, build_concurrency: 2,
failure_threshold: 8`. `MIN_HOPS` stays 1 even for local zero-hop
types, because a zero-hop entry is a distinct type, not an
`EstablishedMaterial` with an empty hop list.

### Build-record layout surface

`BuildRequestKind` carries the corrected I2NP message identifiers:
`ShortTunnelBuild` → type **25**, `VariableTunnelBuild` → type **23**;
`BuildReplyKind` marks `OutboundTunnelBuildReply` → type **26**.
`BuildRecordLayout::{Short, Variable}` is the short / 528-byte legacy
layout pair. Sizes: 154-byte request plaintext, 202-byte reply
plaintext, 218-byte sealed short record — 154 / 202 / 218 — with
`MAX_BUILD_RECORDS = 8` and `SHORT_BUILD_RECORD_SIZE = 218` imported
from `i2pr-proto`.

`ShortRequestRecord` emits exactly 154 bytes: a fixed
`REQUEST_FIXED_PREFIX_LEN = 56`-byte prefix, then the canonical
two-byte `Mapping` length + body, then CSPRNG-filled post-Mapping
padding (`encode_with_rng`); `encode_deterministic_zero_padded` is the
fixture-only zero-padded path. Role flag bytes are
`HOP_ROLE_PARTICIPANT = 0x00`, `HOP_ROLE_INBOUND_GATEWAY = 0x80`,
`HOP_ROLE_OUTBOUND_ENDPOINT = 0x40`; `0x01`/`0x02` and `0xC0` are
rejected. `LayerEncryptionType::Aes` is byte 0; the 0x05
EciesAeadOnly value is rejected. Request time is minute-encoded
(`floor(unix_seconds / 60)`) and
`REQUEST_EXPIRATION_SECONDS = 600` is mandatory —
`ShortBuildError::RandomnessUnavailable` fails closed when no CSPRNG is
injected. `ShortReplyRecord` emits exactly 202 bytes with a canonical
`Mapping` followed by a one-byte response code at offset 201
(`ShortResponseCode::Accepted` = 0, `BandwidthRejected` = 30).

`multirecord` owns the count-prefixed contract helpers:
`validate_count_prefixed_short_payload` /
`encode_count_prefixed_short_payload` /
`decode_short_tunnel_build_payload` reject a zero record count, a
count above `MAX_RECORD_COUNT = 8`, and any payload whose length is not
exactly `1 + count * 218`. `MIN_PRODUCTION_RECORD_COUNT = 4` is the
production floor. `INBOUND_SHORT_BUILD_POLICY` is the literal
`"reference-compatible-spec-text-discrepancy"`.

`conformance_fixtures::ReferenceFixture::canonical()` constructs the
canonical fixture through an independent reference SHA-256 + Noise
`MixHash`/`MixKey` chain and asserts the production primitive reaches
the same transcript hash and derives the same `replyKey` / `layerKey` /
`ivKey`. `fixed_vectors` freezes the transcript chain, keys, shared
secret, `es` HKDF output, post-request chaining key, request AEAD key,
the 218-byte sealed envelope, the slot nonces, the derived
`replyKey`/`layerKey`/`ivKey`, and the OBEP garlic key and 8-byte tag.

### ECIES-X25519 short-build construction

**Wire format.** `seal_short_request_with_ephemeral` produces exactly
218 bytes: `truncated_hash_prefix (16) || ephemeral_pub (32) ||
ciphertext (154) || tag (16)`. `open_short_request` returns exactly 154
bytes and rejects records whose first 16 bytes do not match the supplied
hop identity hash. `seal_short_reply` produces a 218-byte record with
**no** ephemeral or nonce prefix.

**Noise-N transcript.**
`NOISE_PROTOCOL_NAME = "Noise_N_25519_ChaChaPoly_SHA256"` padded to 32
bytes gives `h0`; the canonical null prologue sets `h = SHA256(h0)` and
`ck = h0`; the peer static public key and the sender ephemeral public
key are mixed; the `es` derivation is a **single**
`HKDF(ck, sharedSecret, "", 64)` whose first 32 bytes become the new
chaining key and whose second 32 become the request AEAD key; the
154-byte request is encrypted with ChaCha20-Poly1305 using `nonce = 0`
and `ad = h`; the ciphertext+tag is then mixed to produce the saved
post-request `h`, which is the associated data for the reply AEAD.

**Key derivation.** `SMTunnelReplyKey` → `SMTunnelLayerKey` are derived
exactly; a non-OBEP hop uses the first 32 bytes as `ivKey` and the last
32 as `layerKey`. The OBEP continues through `TunnelLayerIVKey` and
derives the additional `RGarlicKeyAndTag` material with the canonical
32-byte key and `GARLIC_REPLY_TAG_LEN = 8`-byte tag.

**Slot nonce.** `RECORD_SLOT_NONCE_OFFSET = 4` — the 12-byte ChaChaPoly
nonce is zero in bytes `0..3` and `5..11` and carries the slot byte at
offset 4. `ValidatedRecordSlot` is a typed wrapper that rejects
out-of-range slots; it has no untyped constructor.

**State machine.** `StatePhase` (private) is
`Prepared → Protecting → ReadyForDelivery → AwaitingReply →
Established` plus the six bounded terminal failures `HopRejected`,
`TimedOut`, `Cancelled`, `InvalidReply`, `CryptoFailed`,
`DeliveryFailed` — the same set as the public
`ShortBuildOutcome::{Established, HopRejected, TimedOut, Cancelled,
InvalidReply, CryptoFailed, DeliveryFailed}`. `ShortBuildAction` has a
single `Deliver` variant. `ShortBuildPath::validate` enforces
direction/role topology: **outbound** — no IBGW, OBEP only at the final
hop, participants before the final OBEP; **inbound** — IBGW at the
first hop, no OBEP, participants after the first IBGW. The path also
carries explicit terminal-routing fields: `outbound_reply_router`
(outbound) and `originator_hash` (inbound). Plan 113 requires exactly one
originator fake for inbound builds, carrying
`hash16 || fresh X25519 pub32 || random remainder`, and verifies it after
reply processing.

**Inbound creator-key handling.** Inbound is enabled under the explicit
`reference-compatible-spec-text-discrepancy` policy, documented in
[`specs/references/short-build-inbound-creator-key.md`](../../specs/references/short-build-inbound-creator-key.md)
and
[`plans/closure/exploratory-tunnels/113-status.md`](../../plans/closure/exploratory-tunnels/113-status.md).
Normal fixed request fields plus Mapping/padding are used, one
originator fake carries the hash/ephemeral/remainder shape, and the
creator verifies its integrity. The unresolved final-spec prose is
**not** a strict conformance claim for this one semantic, and no guessed
plaintext field is added.

**Layer keys.** `LayerKeys` is `Zeroize`-derived with
`ZeroizeOnDrop`, has no `Debug`/`Clone`/serde, and is taken one-shot via
`HopCryptoContext::take_layer_keys` and
`EstablishedMaterial::into_established_tunnel`.

### The canonical production I2NP bridge

`ShortBuildI2npBridge::wrap_deliver_action(action, header)` takes a
`ShortBuildAction::Deliver` and produces one complete `I2npMessage`
plus a sanitized `BridgeRecord` (lengths and SHA-256 digests only — the
raw records are not exposed on the public surface).

**The NO-DOUBLE-PREFIX STBM invariant.** The delivery payload arriving
from the state machine is **already count-prefixed**: it is
`count || count*218`. The bridge validates
`payload.len() == 1 + record_count * 218` and
`payload[0] == record_count`, then **splits the count byte off**
(`payload[1..]`) before constructing
`DeferredBuildRecords::new(record_count, 218, raw_records)`, which
re-adds exactly one count byte when the I2NP body is encoded. The
bridge then round-trips the encoded message through
`I2npMessage::decode_standard` (or the short-transport decoder) and
asserts the recovered body equals the original count-prefixed payload
byte-for-byte; a mismatch fails closed with
`BridgeError::RoundTripBodyMismatch`.

If the prefix were applied twice — e.g. by passing the still-prefixed
payload straight into `DeferredBuildRecords` — the encoded STBM body
would begin `count, count, …` and the record area would be
misaligned. Downstream, every peer decoder would read the second byte as
the first byte of record 0's hash prefix, so *no* record would match its
hop identity hash: `HashPrefixMismatch` at the first hop, a dead build,
and no useful diagnosis. The round-trip assertion exists specifically to
make that class of regression impossible to introduce silently.

`BridgeError` variants: `RecordCountMismatch`, `RecordCountOutOfRange`,
`PayloadLengthMismatch`, `ZeroRecordCount`, `DeferredBuildRecords`,
`MessageFraming`, `RoundTripBodyMismatch`.

### Local data plane (Plan 116)

**Layer transforms.** `TunnelLayerTransform` implements the canonical
AES-256 `ECB-ENC / CBC-ENC / ECB-ENC` participant forward transform and
the `ECB-DEC / CBC-DEC / ECB-DEC` creator inverse.
`TUNNEL_IV_LEN = 16`, `TUNNEL_PAYLOAD_LEN = 1008`,
`MAX_DUPLICATE_WINDOW = 1024`. `DuplicateWindow` bounds replay
detection; `DuplicateWindowError` is the typed overflow surface.

**Checksum and overheads.** `data.rs` owns the canonical
`SHA256(post_zero_record_bytes || IV)[0..4]` checksum builder and
parser with distinct unfragmented vs fragmented-first wire encodings
(`CHECKSUM_LEN = 4`), and automatic complete-message fragmentation.
`unfragmented_overhead` and `fragmented_first_overhead` are **private
functions** and `FOLLOW_ON_OVERHEAD` is a **private const** (`1 + 4 + 2`)
— they are named internal helpers, not public constants.

**Fragmentation bounds.** `MAX_PLAINTEXT_DATA_BYTES = 1008`,
`MAX_TUNNEL_MESSAGE_PAYLOAD_BYTES = 62_708`, `MAX_FRAGMENT_COUNT = 64`,
`MAX_FRAGMENT_BODY_BYTES = MAX_PLAINTEXT_DATA_BYTES - 32`,
`MAX_PADDING_RETRY_PER_BYTE = 32`.

**Delivery instructions.** `DeliveryInstruction` is
`Local` (type `00`), `Tunnel { tunnel_id, gateway }` (type `01`), or
`Router { router }` (type `10`). It is carried on the first fragment (or
the unfragmented record) only, via
`FragmentDelivery { delivery: Option<DeliveryInstruction>, fragment }`.
`TunnelPayloadHeader { delivery, message_id, expiration_ms }` is the
builder-side plaintext header; `message_id` must be nonzero and
`expiration_ms` is bounded.

**Bounded reassembly.** `BoundedReassembler::insert_with_delivery` takes
the first-fragment `DeliveryInstruction` and carries it through
reassembly so the reassembled message keeps its delivery metadata.
Every insertion is classified by `PartialMessage::classify()` into
`FragmentInsertDisposition::{Inserted { added_bytes }, ExactDuplicate}`
— an **exact duplicate is a pure no-op** for memory, expiry, and
aggregate budget, and classification runs *before* any budget charge so
a duplicate is never rejected merely because the budget is already full.
A conflicting duplicate invalidates only the affected partial and
zeroizes its retained-byte accounting. First-fragment delivery metadata
participates in duplicate identity (`ReassemblyError::ConflictingFirstMetadata`);
follow-on fragments may not carry delivery metadata
(`UnexpectedFollowOnDeliveryInstruction`). A follow-on fragment
sequence number must be in `FOLLOW_ON_SEQUENCE_MIN = 1 ..
FOLLOW_ON_SEQUENCE_MAX = 63` (zero is never a valid fragment sequence).
`ReassemblyError` variants: `ConflictingDuplicate`,
`ConflictingFirstMetadata`, `ConflictingFollowOnTerminalFlag`,
`UnexpectedFollowOnDeliveryInstruction`, `DuplicateFragment`,
`SequenceOutOfRange`, `LastBelowObservedMax`, `MessageTooLarge`,
`CapacityExceeded`, `UnknownMessage`, `AggregateBytesExceeded`.
`TunnelMessageError` has 21 typed variants from `PayloadLength` through
`PaddingRetryExhausted` / `FragmentRangeExhausted`.

### `DataPlaneRegistry` and `InboundGatewayRoute` (Plan 190)

`DataPlaneRegistry` keeps outbound roles keyed by `TunnelSlot` and binds
each activated inbound role to both its slot and its local receive
`TunnelId`. The bounded reverse maps let pool expiry/failure reports call
`remove_slot(slot)` for either direction; removal clears role, routing
metadata, and both reverse indexes atomically. `RegistryRemoval` is a
typed consumed-role result, and no `LayerKeys` clone is introduced.
Duplicate slot and receive-id activation is fail-closed
(`RegistryError::DuplicateOutbound` / `DuplicateInbound` /
`DuplicateInboundSlot`, plus `OutboundFull` / `InboundFull` /
`DirectionMismatch`).

`InboundGatewayRoute` is the Plan 190 fix and is deliberately a **typed,
`Copy`, non-secret** struct of three fields:

```rust
pub struct InboundGatewayRoute {
    pub gateway_router: i2pr_proto::Hash,   // source of truth for DatabaseLookup.from
    pub gateway_receive_tunnel: TunnelId,  // source of truth for DatabaseLookup.reply_tunnelId
    pub local_receive_tunnel: TunnelId,    // registry selector only; never in ReplyPath
}
```

`activate_inbound` reads `EstablishedTunnel::inbound_gateway()`
**before** the role is consumed and retains the pair alongside the local
receive id. `inbound_gateway_route(local_receive)` returns it;
`inbound_first_hop` is preserved for callers that do not need the
gateway receive id.

**Why this matters.** The reply path a tunneled `DatabaseLookup`
advertises must come from *real installed tunnel material*, not from
bytes the client supplied. The registry's `InboundGatewayRoute` is
exactly that binding: the `(gateway_router, gateway_receive_tunnel)`
tuple is read out of the activated `EstablishedTunnel`, so the daemon
can derive the whole `ReplyPath` from the tuple alone. The third field,
`local_receive_tunnel`, is explicitly documented as "used only as a
registry selector; never copied into `ReplyPath`" — that separation is
what prevents a local bookkeeping id from being mistaken for the remote
gateway receive id. `ReplyPath` construction itself stays **out of
`i2pr-tunnel`** and lives in the daemon composition layer
(`reply_path_for_inbound_route` in
`crates/i2pr-daemon/src/destination_tunnels.rs`).

### Deterministic responder and reply-path provider

`DeterministicResponder` is a single-hop peer simulator built from
explicit static key bytes and a hop identity hash. It accepts a sealed
record, derives the layer keys, seals a reply plaintext, and the
production primitive round-trips that reply through `open_short_reply`
for every valid slot in `0..=7`.

`ExploratoryPoolReplyPathProvider<'a>` borrows the pool and holds a
caller-supplied `now_seconds` (updated with `set_now`), keeping the
surface deterministic — it never reads a clock. `provide_reply_path()`
delegates to `pool::select_inbound_reply_path(now)` and returns `None`
when no inbound tunnel is established-and-unexpired. It implements
`i2pr_netdb::ReplyPathProvider` for `Box<dyn>` dispatch. The `ReplyPath`
is therefore derived **only** from the pool's own inbound registration —
never from client-supplied bytes.

### Runtime-neutral role composition and the M11 lane

`roles.rs` composes `OutboundGatewayRole`, `OutboundParticipantRole`,
`InboundParticipantRole`, `OutboundEndpointRole`, `InboundGatewayRole`,
and `LocalInboundEndpointRole` with CSPRNG-injected IVs and padding and
multi-cell `forward_cells` / `process_cells` seams. The roles emit
semantic `RouterDeliveryAction` values rather than I/O;
`OutboundEndpointRole` rejects a completed message with
`TunnelRoleError::UnspecifiedDeliveryInstruction` when the reassembler
returns no delivery. The internal `assemble_actions` is a private helper
on `OutboundEndpointRole` (not public API). What the daemon adapts is
the *action* stream — cells out to the next hop, and messages handed to
the local NetDB/NetDB-publish/Streaming consumers — plus the
`TunnelDataMessage` framing itself. `TunnelRoleError` has 15 variants
including `TunnelUnavailable`, `DuplicateCell`, `ReceiveTunnelMismatch`,
`PreviousPeerMismatch`, `DuplicateWindow`, `Reassembly`,
`NotTunnelGateway`, `GatewayTunnelMismatch`, `MissingNextHop`.

### M11 transit, the M11 lane

`transit.rs` is the M11 vertical slice and the largest module in the
crate. It is **infrastructure only**:

- typed interpretation of the canonical `BuildOptions` bandwidth keys —
  `BANDWIDTH_MINIMUM_KEY = "m"`, `BANDWIDTH_REQUESTED_KEY = "r"`,
  `BANDWIDTH_LIMIT_KEY = "l"`, `BANDWIDTH_AVAILABLE_KEY = "b"` — via
  `TransitBandwidthRequest` / `TransitBandwidthReply` /
  `parse_transit_bandwidth_request`, with `TransitBandwidthParseError`
  rejecting malformed input without weakening the `short_record`
  `BuildOptions` codec;
- `TransitAdmissionPolicy` (enabled/disabled, accepting vs. degraded,
  global and per-peer active/pending ceilings, available share
  bandwidth, optional per-tunnel cap) and `TransitAdmissionState` (live
  pending reservations). `TransitAdmissionError` variants:
  `Disabled`, `Degraded`, `Shutdown`, `ActiveFull`, `PendingFull`,
  `ActivePerPeerFull`, `PendingPerPeerFull`, `InsufficientBandwidth`.
  `TransitAdmissionConfigError` rejects ceilings above the maximums.
  Every well-formed admission rejection collapses to the single wire
  code `ShortResponseCode::BandwidthRejected` (30); detailed local
  reasons stay internal;
- `TransitRegistry` — a dedicated bounded registry keyed by receive
  tunnel id with duplicate-id rejection, deterministic `remove` and
  `expire(now)`, and a mutable role lookup. It deliberately does **not**
  overload `DataPlaneRegistry`, which remains owner of creator/local-pool
  state;
- `process_short_build_request` — the runtime-neutral transaction: open
  and decrypt the own record, decode, reserve admission, derive
  hop-local keys, build the role registration, construct the
  accepted/rejected reply, seal the reply, atomically commit **only** an
  accepted registration, and release the reservation on every non-commit
  path. `TransitBuildRoute::{ContinueStbm, TerminateOtbrm}` routes the
  creator-side reply;
- a bounded transit data plane — `TransitDataPlane::{Participant,
  InboundGateway, OutboundEndpoint}` with `TransitDataOutcome`,
  `TransitDataFatalError`, `gateway_nested_is_multicell_capable`, and
  per-ingress fragment-id rules (Plan 258/260).

Transit bounds: `MAX_TRANSIT_ACTIVE = 64`, `MAX_TRANSIT_PENDING = 8`,
`MAX_TRANSIT_PEER_ACTIVE = 8`, `MAX_TRANSIT_PEER_PENDING = 2`,
`MAX_TRANSIT_DUPLICATE_WINDOW = 1024`,
`MAX_TRANSIT_REASSEMBLY_MESSAGES = 64`,
`MAX_TRANSIT_REASSEMBLY_BYTES_PER_MESSAGE = 64 * 1024`,
`MAX_TRANSIT_REASSEMBLY_AGGREGATE_BYTES = 1024 * 1024`,
`MAX_TRANSIT_REASSEMBLY_EXPIRY_MS = 60 * 1000`,
`TRANSIT_TIME_SKEW_SECONDS = 60`.

**Honest status.** Plan 268 passed a **ONE-FAMILY experimental
progression** (3/8 receipt-family completions, zero semantic failures,
exact-head CI green) — see
[`plans/closure/transit-tunnels/268-status.md`](../../plans/closure/transit-tunnels/268-status.md)
and ADR 0026
([`docs/adr/0026-staged-interoperability-progression-and-java-debt.md`](../../docs/adr/0026-staged-interoperability-progression-and-java-debt.md)).
That is **not** a public transit participation claim. Per Plan 269 and
`specs/support.toml`, public transit remains **disabled, non-advertised,
and unclaimed**; the `transit` module is retained infrastructure whose
M11 capability is explicitly not claimed. No multi-family interop
qualification has been recorded.

### Garlic reply handling

`garlic_reply.rs` implements the Plan 188 bounded OBEP garlic unwrap:
`decrypt_build_reply_garlic` is a tag-gated ChaCha20-Poly1305 decrypt
followed by a **local** ShortTunnelBuildReply clove parse
(`SHORT_TUNNEL_BUILD_TYPE = 25`), and `wrap_obep_reply_garlic` is the
OBEP-side wrap. `GarlicClove` / `extract_garlic_clove` parse the clove
boundary locally. Plan 188 **ignores originator-fake bytes on the reply
path** — the shape is still enforced, while the real-hop AEAD provides
authenticity. `GarlicReplyError` variants: `TooShort`, `TooLarge`,
`TagMismatch`, `AuthenticationFailed`, `DecryptedTooLarge`,
`TruncatedBlocks`, `TooManyBlocks`, `NoClove`, `InvalidClove`,
`UnexpectedInnerType`, `InvalidReplyPayload`, `EncryptionFailed`.

### Local zero-hop (Plan 172)

`zero_hop.rs` models a zero-hop tunnel as a **local** path where
gateway == endpoint == this router. It carries no remote peer vector and
no `LayerKeys`, and the types are deliberately distinct from
`EstablishedMaterial` so an empty remote hop list can never be mistaken
for a usable local path. All invariants are enforced at construction:
gateway is the actual local router hash (never all-zero), the tunnel id
is fresh, non-zero, and never the `u32::MAX` sentinel, no fabricated
layer keys are stored, and entries are **never offered to the remote
participant/IBGW/OBEP crypto data plane**. `EstablishedTunnel::new`
still rejects an empty remote hop list and `MIN_HOPS` remains 1.
`ZeroHopError`: `ZeroGateway`, `ZeroTunnelId`, `SentinelTunnelId`,
`LifetimeOutOfRange`; `MAX_ZERO_HOP_LIFETIME_SECONDS = 600`.

**This is a LOCAL delivery convenience, not a network claim.** Plan 172
closed as
`passed-m9-i2cp-independent-leaseset2-lifecycle-corrective`
([`plans/closure/i2cp/172-status.md`](../../plans/closure/i2cp/172-status.md)).

## Dependencies

`crates/i2pr-tunnel/Cargo.toml`:

**Production** — `aes`, `cbc` (0.1, `block-padding`), `chacha20`,
`chacha20poly1305`, `i2pr-core`, `i2pr-crypto`, `i2pr-netdb`,
`i2pr-proto`, `rand_core` (`os_rng`), `sha2`, `thiserror`,
`x25519-dalek` (`static_secrets`), `zeroize`.

| Dependency | Why |
| --- | --- |
| `i2pr-proto` | `Hash`, `Date`, I2NP message codecs, `DeferredBuildRecords`, `I2npBody` / `I2npMessage`, `TunnelDataMessage`, `TunnelGatewayMessage`, and the shared `MAX_BUILD_RECORDS = 8` / `SHORT_BUILD_RECORD_SIZE = 218` / `SHORT_REQUEST_PLAINTEXT_SIZE = 154` / `SHORT_REPLY_PLAINTEXT_SIZE = 202` constants |
| `i2pr-crypto` | X25519 wrappers, `hkdf_sha256_extract_and_expand`, `ChaCha20` — the low-level primitives behind the Noise-N primitive. No local crypto primitives are implemented here. |
| `i2pr-core` | Runtime-neutral contracts and budgets |
| `i2pr-netdb` | The `ReplyPath` token and the `ReplyPathProvider` trait the provider adapter implements |
| `aes` / `cbc` | AES-256 ECB/CBC per-hop tunnel-layer transforms in `layer.rs`. `i2pr-tunnel` is the only workspace crate that depends on `aes`/`cbc`; every other symmetric primitive flows through `i2pr-crypto` or `i2pr-transport-ntcp2`. |
| `chacha20` | Raw ChaCha20 record transforms and layer IV handling (`multirecord`, `layer`) |
| `chacha20poly1305` | The Noise-N request/reply AEAD |
| `x25519-dalek` | The Noise-N DH primitive (`static_secrets` for the responder/hop static keys) |
| `sha2` | SHA-256 transcript / checksum derivation |
| `rand_core` | CSPRNG injection traits; `os_rng` is the production RNG feature |
| `zeroize` | `LayerKeys`, `NoiseRequestState`, `RequestKeyMaterial`, `TunnelId` zeroization |
| `thiserror` | Typed error enums — no `anyhow` in this crate |

**Dev** — `rand_chacha` (deterministic test RNG), `rand_core` (`std`
feature), `x25519-dalek` (`static_secrets`).

Checker allowlist entry in `scripts/check-dependency-direction.sh`:

```python
"i2pr-tunnel": {"i2pr-core", "i2pr-crypto", "i2pr-netdb", "i2pr-proto"},
```

The allowlist covers the four `i2pr-*` path dependencies exactly. The
remaining production dependencies are third-party crates the checker
governs separately. There is **no production dependency on
`i2pr-testkit`**, and the direction is one-way: `i2pr-runtime`,
`i2pr-client`, and `i2pr-daemon` may depend on `i2pr-tunnel`;
`i2pr-tunnel` depends on none of them.

**Runtime-neutrality is verified, not assumed.** Greps over
`crates/i2pr-tunnel/src/` and `crates/i2pr-tunnel/tests/` find no
`tokio`, no `tokio::*`, no `std::net`, no `std::fs`, no `std::io::*`,
no `std::thread`, no `async fn`, no `async move`, no `.await`, no
`spawn`, no `JoinHandle`, and no unbounded channel type. The only
`spawn` hit in the crate is the word "spawn" inside a `//!` doc comment
in `lib.rs:68`. `bash scripts/check-runtime-boundaries.sh` reports
`runtime boundary checks passed` on the current head.

The grouped-`use std::{` blind spot was checked explicitly: there is
**no** grouped `use std::{` import anywhere in `crates/i2pr-tunnel/src/`
or `crates/i2pr-tunnel/tests/`, so no `std::net` / `std::fs` import can
be hidden inside one. The module-local `use` lines are all flat
(`use std::collections::BTreeMap;`, `use std::cell::Cell;`,
`use std::fmt;`, `use core::fmt;`). **No boundary violation found.**

## Tests

`cargo test --locked -p i2pr-tunnel --all-targets -- --test-threads=1`
passes on the current head: **395 in-crate unit tests + 5 integration
tests = 400 passed, 0 failed, 0 ignored.** All tests are local and
deterministic; no root, namespaces, Java I2P, i2pd, or Internet
connection is required.

Integration file under `crates/i2pr-tunnel/tests/` (1 file, 236 lines):

| File | Lines | Tests | Coverage |
| --- | --- | --- | --- |
| `plan111_reference_vectors.rs` | 236 | 5 | The Plan 112 Rust-only reference-provenance lane: re-derives the frozen `fixed_vectors` bytes from a pure-Rust path built only on `x25519-dalek`, `sha2`, `chacha20poly1305`, and `i2pr_crypto::hkdf_sha256_extract_and_expand` **without** consulting the frozen module, then asserts the production `seal_short_request`, `open_short_request`, and `derive_layer_keys` reproductions match byte-for-byte |

Unit tests per module (`#[test]` counts):

| Module | Tests | Module | Tests |
| --- | --- | --- | --- |
| `transit` | 101 | `garlic_reply` | 10 |
| `short` | 39 | `bridge` | 9 |
| `fragment` | 38 | `established` | 9 |
| `multirecord` | 29 | `fixed_vectors` | 9 |
| `data` | 21 | `data_plane_registry` | 13 |
| `short_record` | 21 | `roles` | 12 |
| `build_crypto` | 20 | `layer` | 11 |
| `pool` | 20 | `build` | 7 |
| `conformance_fixtures` | 5 | `config` | 5 |
| `zero_hop` | 5 | `identity` | 4 |
| `provider` | 4 | `responder` | 1 |
| `short_state` | 2 | | |

**Deterministic state-machine coverage.** The `transit` lane alone
carries 101 tests and covers the bounded negative paths explicitly:
`pending_reservations_enforce_global_and_per_peer_limits_and_release_on_drop`,
`registry_insert_enforces_global_capacity`,
`registry_insert_rejects_duplicate_receive_id`,
`registry_conflict_after_reservation_releases_pending_token`,
`reply_seal_failure_releases_pending_reservation`,
`rng_failure_rolls_back`, `timestamp_outside_skew_rejects_before_commit`,
`policy_rejection_is_sealed_code_30_and_releases_pending_reservation`,
`registry_expire_at_lifetime_removes_entry` /
`registry_expire_early_keeps_entry`,
`registry_remove_unknown_id_is_typed_error`,
`message_single_record_message_does_not_activate_extra_state`,
`plan260_ibgw_interleaved_reassembly_no_cross_assembly`,
`plan260_ibgw_fragment_ids_distinct_across_ingresses`, and the
Plan 258 multicell size-class rows. `short` and `multirecord` cover the
build trajectory end to end, including the Plan 114 strict outbound and
inbound E2E tests that reach `Established` exactly (no permissive
`InvalidReply OR Established` acceptance).

**Bounded negative paths.** Capacity, budget, and expiry ceilings are
each tested at capacity 1, exact load, and max+1 semantics; lease and
reservation release is asserted on the drop, failure, expiry, and
conflict paths (see the transit rows above). `fragment.rs` (38 tests)
covers the `classify()` duplicate-identity rules including
`ConflictingFirstMetadata` and `UnexpectedFollowOnDeliveryInstruction`.
`zero_hop.rs` covers the sentinel-id, zero-gateway, and
`remote_empty_hop_list_still_rejected` invariants.

**Conformance-fixture / fixed-vector coverage.** `fixed_vectors.rs` (9
tests) and `conformance_fixtures.rs` (5 tests) together re-derive the
frozen Noise-N constants through the independent reference
implementation and assert the production primitive matches every frozen
constant; the `plan111_reference_vectors.rs` integration lane re-derives
them a third time from a pure-Rust oracle. `responder.rs` closes the
loop with a single end-to-end test for every valid slot in `0..=7`.

## Distinctive design choices

1. **Tunnel IDs reject zero, and per-hop IDs are explicit.** Zero is
   reserved by I2P for "no tunnel"; per-hop `receive_tunnel` and
   `next_tunnel` are independent `TunnelId` fields, never derived from a
   router-hash prefix.
2. **Configuration is bounded at every layer.** `ExploratoryPoolConfig::try_new`
   rejects any value above the documented ceiling, and every
   reassembly / admission / registry surface carries an explicit
   `MAX_*` bound — the crate cannot become a vector for unbounded
   growth.
3. **Time is always injected.** `advance_time`, `set_now`, `expire(now)`,
   and `set_now_ms` are the only ways to surface expiry; no module in
   the crate reads the wall clock.
4. **The registrar is success-only in the strong sense.** It admits real
   `EstablishedMaterial` extracted from the state machine, and the
   legacy `admit(&ShortBuildOutcome, …)` fails closed rather than
   fabricating a registration from an outcome label it cannot honour.
5. **The reply path is derived only from installed material.** Both
   `ExploratoryPoolReplyPathProvider` and the Plan 190
   `InboundGatewayRoute` build the reply path from real registered
   tunnel state; the local bookkeeping id is explicitly kept out of the
   `ReplyPath` so client-supplied bytes can never become routing state.
6. **Build cryptography is a trait with a deliberately useless default.**
   `NoBuildCryptography` returns `Unavailable` for every call, so the
   no-build configuration cannot silently claim cryptographic coverage
   it does not have.
7. **Secrets stay out of `Debug` / `Clone` / serde.** `LayerKeys`,
   `NoiseRequestState`, and `RequestKeyMaterial` are `Zeroize` +
   `ZeroizeOnDrop` with no formatting or cloning surface; the transit
   module keeps decoded keys off every consumer boundary.
8. **The bridge proves its own wire format.** The no-double-prefix
   STBM invariant is not just documented — it is enforced by a
   round-trip decode assertion that fails closed with
   `RoundTripBodyMismatch`.
9. **Exact duplicates are free.** Reassembly classifies before charging
   any budget, so a replayed fragment costs no memory, no expiry
   refresh, and no aggregate budget.
10. **Transit is infrastructure, not a claim.** The M11 lane ships a
    complete bounded admission/registration/data-plane surface while
    public transit participation stays disabled, non-advertised, and
    unclaimed.

## Cross-references

Related deep dives — [`i2pr-netdb.md`](i2pr-netdb.md) (the `ReplyPath`
token and `ReplyPathProvider` trait),
[`i2pr-proto.md`](i2pr-proto.md) (I2NP codecs, message types, and the
shared build-record size constants),
[`i2pr-crypto.md`](i2pr-crypto.md) (X25519 wrappers and the HKDF-SHA256
helper),
[`i2pr-core.md`](i2pr-core.md) (runtime-neutral budgets and health),
[`i2pr-client.md`](i2pr-client.md) (ECIES session / routing and the
destination-side tunnel policy),
[`i2pr-daemon.md`](i2pr-daemon.md) (the composition root that owns
listeners and the `ReplyPath` construction), and
[`i2pr-runtime.md`](i2pr-runtime.md) (the sole production owner of
Tokio, sockets, timers, and cancellation).

ADRs — ADR 0026
([`docs/adr/0026-staged-interoperability-progression-and-java-debt.md`](../../docs/adr/0026-staged-interoperability-progression-and-java-debt.md))
governs the staged interoperability progression that M11 passed a
one-family experimental qualification under, without a public transit
claim. Also see
[`docs/architecture/dependency-graph.md`](dependency-graph.md) and
[`docs/architecture/tooling.md`](tooling.md).

Short-build conformance plan-of-record —
[`plans/implementation/exploratory-tunnels/109-short-build-record-and-noise-conformance-correction.md`](../../plans/implementation/exploratory-tunnels/109-short-build-record-and-noise-conformance-correction.md)
and
[`plans/implementation/exploratory-tunnels/111-short-build-final-local-conformance-correction.md`](../../plans/implementation/exploratory-tunnels/111-short-build-final-local-conformance-correction.md).
Closure records (authority order: closure records beat the registry):
Plan 107
([`107-status.md`](../../plans/closure/exploratory-tunnels/107-status.md)),
Plan 108
([`108-status.md`](../../plans/closure/exploratory-tunnels/108-status.md)),
Plan 109
([`109-status.md`](../../plans/closure/exploratory-tunnels/109-status.md)),
Plan 110
([`110-short-build-multirecord-preprocessing-and-conformance-closure.md`](../../plans/closure/exploratory-tunnels/110-short-build-multirecord-preprocessing-and-conformance-closure.md);
the separate `110-status.md` is `superseded-by-plan111-corrected`),
Plan 111
([`111-status.md`](../../plans/closure/exploratory-tunnels/111-status.md),
[`111-post-closure-audit-amendment.md`](../../plans/closure/exploratory-tunnels/111-post-closure-audit-amendment.md)),
Plan 112
([`112-status.md`](../../plans/closure/exploratory-tunnels/112-status.md),
[`112-outbound-short-build-pre-delivery-closure.md`](../../plans/closure/exploratory-tunnels/112-outbound-short-build-pre-delivery-closure.md)),
Plan 113
([`113-status.md`](../../plans/closure/exploratory-tunnels/113-status.md)),
Plan 114
([`114-status.md`](../../plans/closure/exploratory-tunnels/114-status.md)),
Plan 115
([`115-status.md`](../../plans/closure/exploratory-tunnels/115-status.md),
[`115-completion-emissary-native-q0.md`](../../plans/closure/exploratory-tunnels/115-completion-emissary-native-q0.md),
[`115-status-amendment-emissary-q0.md`](../../plans/closure/exploratory-tunnels/115-status-amendment-emissary-q0.md)),
Plan 116
([`116-status.md`](../../plans/closure/exploratory-tunnels/116-status.md),
[`116-completion-correction.md`](../../plans/closure/exploratory-tunnels/116-completion-correction.md),
[`116-final-closure.md`](../../plans/closure/exploratory-tunnels/116-final-closure.md),
[`116-terminal-cleanup.md`](../../plans/closure/exploratory-tunnels/116-terminal-cleanup.md)),
and Plan 117
([`117-status.md`](../../plans/closure/exploratory-tunnels/117-status.md) —
`closed-for-progression-with-evidence-gap`,
[`117-corrective-closure.md`](../../plans/closure/exploratory-tunnels/117-corrective-closure.md),
[`117-terminal-native-reference-correction.md`](../../plans/closure/exploratory-tunnels/117-terminal-native-reference-correction.md)).
Plan 115's Q0 covers construction + OBEP reply only; Q1 (authenticated
transport) and Q2 (reply round-trip to `Established`) remain pending, and
live mixed-router delivery is blocked on a qualified external delivery
lane.

Spec references —
[`specs/support.toml`](../../specs/support.toml) carries the
machine-readable inventory including `plan_109_short_build_conformance`,
`plan_110_multirecord_closure`, `plan_111_final_short_build_conformance`,
`plan_112_outbound_pre_delivery_closure`, `plan_113_inbound_reconciliation`,
`plan_114_terminal_routing`, and `plan_115_canonical_bridge`; see also
[`specs/CONFORMANCE.md`](../../specs/CONFORMANCE.md) and
[`specs/references/short-build-inbound-creator-key.md`](../../specs/references/short-build-inbound-creator-key.md)
for the inbound creator-key policy.

M6 mixed-router records — Plan 185
([`185-status.md`](../../plans/closure/mixed-router-interop/185-status.md),
[`185-m6-live-one-hop-exploratory-tunnels-and-liveness.md`](../../plans/implementation/mixed-router-interop/185-m6-live-one-hop-exploratory-tunnels-and-liveness.md))
owns the daemon-side liveness coordinator over this crate's state
machine; Plan 190
([`190-status.md`](../../plans/closure/mixed-router-interop/190-status.md),
[`190-m6-inbound-netdb-reply-path-tunnel-id-corrective.md`](../../plans/implementation/mixed-router-interop/190-m6-inbound-netdb-reply-path-tunnel-id-corrective.md))
introduced `InboundGatewayRoute`; Plan 191
([`191-status.md`](../../plans/closure/mixed-router-interop/191-status.md))
localized the I2CP Data-body wire defect; Plan 192
([`192-status.md`](../../plans/closure/mixed-router-interop/192-status.md),
[`192-m6-i2cp-wire-format-corrective.md`](../../plans/implementation/mixed-router-interop/192-m6-i2cp-wire-format-corrective.md))
closed the inbound-delivery layer with the 9-byte short-transport inner
envelope + i2cp-style `Data` body + gzip-no-compression wrapper +
`STYLE=RAW` SAM switch.

Destination-streaming records — Plan 119
([`119-status.md`](../../plans/closure/destination-streaming/119-status.md),
`passed-leaseset2-protocol-foundation`) and Plan 120
([`120-status.md`](../../plans/closure/destination-streaming/120-status.md),
`passed-destination-lifecycle-and-pools`, which reuses
`BoundedTunnelPool` through the
[`lib.rs`](../../crates/i2pr-tunnel/src/lib.rs) type aliases without
touching the cryptography, registrar, or data-plane code).

M11 transit records — Plan 249
([`249-status.md`](../../plans/closure/transit-tunnels/249-status.md))
and Plan 250
([`250-status.md`](../../plans/closure/transit-tunnels/250-status.md))
founded the module; Plans 252–267 were retained-with-correction; Plan
**268**
([`268-status.md`](../../plans/closure/transit-tunnels/268-status.md))
is the passed one-family experimental qualification; Plan 269
([`269-status.md`](../../plans/closure/transit-tunnels/269-status.md))
reconciled the support authority and confirmed public transit stays
disabled, non-advertised, and unclaimed.

I2CP / zero-hop record — Plan 172
([`172-status.md`](../../plans/closure/i2cp/172-status.md),
[`172-m9-i2cp-independent-leaseset2-lifecycle-corrective.md`](../../plans/implementation/i2cp/172-m9-i2cp-independent-leaseset2-lifecycle-corrective.md))
owns the local zero-hop loopback delivery types.

# `i2pr-tunnel` — Milestone 5 exploratory tunnel substrate

Runtime-neutral tunnel identity, exploratory tunnel pool, build-record
layout surface, build-cryptography seam, the Plan 109
locally-conformant ECIES-X25519 short tunnel-build cryptography,
runtime-neutral build state machine, success-only `ExploratoryPool`
registrar, independent reference conformance fixture, deterministic
peer responder, and reply-path provider for `i2pr`. The crate is the
Milestone 5 implementation surface (Plans 107, 108, and 109) and lands
the substrate required to flip the Plan 106 NetDB seam from
`BlockedExploratoryTunnelUnavailable` to `Available` once a real
inbound tunnel is registered.

> Status: Plan 116 closed as `passed-final-local-closure` by the
> Plan 116 final closure pass at commit `0330fb2e9e64dd0877472c930606ab4219ac18a9`
> Plan 116 final closure pass (defects `F1`–`F5`) on top of the
> Plan 116 completion/correction pass (defects `C1`–`C16`) which
> closed Plan 116 as `passed-local-tunnel-data-plane`. The final
> closure pass landed a real `EstablishedMaterial` transfer from
> `ShortBuildStateMachine` into `ExploratoryPool`, removed
> production placeholder APIs to `#[cfg(test)]`, expressed the
> canonical I2P Tunnel Message Specification fragment overheads
> as named constants with boundary tests, retained the
> first-fragment `DeliveryInstruction` through reassembly, and
> proved the full outbound-to-inbound tunnel trajectory with
> exact-byte equality for both unfragmented (`DeliveryStatus`
> body) and fragmented (`Data` body across multiple TunnelData
> cells) cases. The Plan 116 terminal cleanup pass
> ([`plans/116-terminal-cleanup.md`](../../plans/116-terminal-cleanup.md),
> defects `T1`–`T4`) closes the four remaining closure defects:
> the `BoundedReassembler` now classifies every insertion
> through `PartialMessage::classify()` into
> `FragmentInsertDisposition::{Inserted { added_bytes },
> ExactDuplicate}` so exact duplicates are pure no-ops for
> memory, expiry, and aggregate budget (`T1`); first-fragment
> delivery metadata participates in duplicate identity via
> `ConflictingFirstMetadata` and follow-on delivery metadata is
> rejected with `UnexpectedFollowOnDeliveryInstruction` (`T2`);
> the outbound-to-inbound fragmented trajectory now proves
> exact-byte recovery with at least one follow-on delivered
> before the first fragment (`T3`); and the status / handoff
> / evidence authority are synchronized (`T4`). Plan 114 terminal routing and tunnel-chain correction
> after Plan 113 inbound reference reconciliation and Plan 112
> outbound pre-delivery closure. Plan 109 corrected the wire format,
> Noise-N transcript,
> layer-encryption type, request/reply key derivation, response
> codes, and 218-byte envelope layout. Plan 110 added randomized
> slot allocation, fake records, raw ChaCha20
> preprocessing/postprocessing, and the one-byte-count STBM/OTBRM
> payload framing. Plan 111 corrected the remaining defects:
> canonical Noise-N null-prologue `MixHash`, single-HKDF `es`
> derivation, slot byte at offset 4 of the 12-byte nonce, 8-byte
> OBEP garlic reply tag, explicit per-hop receive and next tunnel
> IDs, role-aware `MessageHopProcessor`, and a frozen
> `fixed_vectors` module generated from an independent reference
> Noise-N + HKDF-SHA256 + ChaCha20-Poly1305 oracle. Plan 112
> landed the outbound pre-delivery closure: CSPRNG-filled
> post-Mapping request/reply plaintext padding, the
> `ShortBuildPath::validate` direction/role topology validator,
> the explicit
> `validate_count_prefixed_short_payload` /
> `encode_count_prefixed_short_payload` STBM/OTBRM contract
> helpers (including delivery-action validation), and a Rust-only reference provenance test under
> `crates/i2pr-tunnel/tests/plan111_reference_vectors.rs` that
> re-derives the frozen bytes from a pure-Rust path built only on
> `x25519-dalek`, `sha2`, `chacha20poly1305`, and
> `i2pr_crypto::hkdf_sha256_extract_and_expand` without
> consulting the frozen module. Plan 113 enables inbound under the
> `reference-compatible-spec-text-discrepancy` policy: normal fixed
> request fields plus Mapping/padding, one originator fake carrying
> `hash16 || fresh X25519 pub32 || random remainder`, and creator-side
> integrity verification. The unresolved final-spec prose is not a
> strict conformance claim for this one semantic. See
> `specs/references/short-build-inbound-creator-key.md`.
> Plan 114 closes four post-Plan-113 high-level routing/composition
> defects: the explicit `outbound_reply_router` (outbound) and
> `originator_hash` (inbound) terminal-routing fields, the
> intermediate `hops[i].next_tunnel == hops[i+1].receive_tunnel`
> chain invariant enforced at both the high-level
> `ShortBuildPath::validate()` boundary and the public
> lower-level `prepare_short_build_message()` entry point, and
> strict outbound/inbound E2E trajectories that deterministically
> reach `Established` without the prior permissive
> `InvalidReply OR Established` acceptance. Plan 115 adds the
> canonical production I2NP bridge in
> [`src/bridge.rs`](../../crates/i2pr-tunnel/src/bridge.rs) so the
> already count-prefixed `ShortBuildAction::Deliver.message` is
> wrapped in a single complete I2NP type-25 message without
> double-prefixing the STBM record count byte, with a round-trip
> standard-header decoder assertion that proves the body bytes
> equal the original delivery payload exactly. Plan 115 Q0
> construction + native OBEP reply has passed locally against
> pinned Emissary; see [`plans/115-status.md`](../../plans/115-status.md).
> The Q0 covers construction + OBEP reply only; Q1 (authenticated
> transport) and Q2 (reply round-trip to Established) remain
> pending. Live mixed-router
> delivery is still blocked on a qualified external delivery lane.
> Plan 116 lands the local tunnel data plane and the Plan 116
> completion/correction pass closed every
> `Plan 116 provisional scaffolding` test marker; the Plan 116
> final closure pass closes the remaining `F1`–`F5` closure
> defects. The Plan 116 terminal cleanup pass
> ([`plans/116-terminal-cleanup.md`](../../plans/116-terminal-cleanup.md))
> closes the four remaining `T1`–`T4` closure defects. The crate
> now
> owns: the AES-256 ECB/CBC/ECB layer transform in
> [`src/layer.rs`](../../crates/i2pr-tunnel/src/layer.rs)
> (participant forward `ECB-ENC/CBC-ENC/ECB-ENC`, creator inverse
> `ECB-DEC/CBC-DEC/ECB-DEC`); the canonical
>  `SHA256(post_zero_record_bytes || IV)[0..4]` checksum builder
> and parser in
> [`src/data.rs`](../../crates/i2pr-tunnel/src/data.rs) with
> distinct unfragmented vs fragmented-first wire encodings,
> automatic complete-message fragmentation, and the canonical
> `unfragmented_overhead` / `fragmented_first_overhead` /
> `FOLLOW_ON_OVERHEAD` overhead constants; the bounded
> `BoundedReassembler` in
> [`src/fragment.rs`](../../crates/i2pr-tunnel/src/fragment.rs)
> with caller-driven expiry, per-message + aggregate byte bounds,
> `PartialMessage::classify()` distinguishing
> `Inserted { added_bytes }` from `ExactDuplicate` so exact
> duplicates are pure no-ops for memory, expiry, and aggregate
> budget, conflicting-duplicate invalidation that releases the
> partial's retained-byte accounting, first-fragment delivery
> metadata participating in duplicate identity
> (`ConflictingFirstMetadata`), and follow-on delivery metadata
> rejected fail-closed (`UnexpectedFollowOnDeliveryInstruction`),
> plus `insert_with_delivery` carrying the first-fragment
> `DeliveryInstruction` through reassembly; the
> `EstablishedTunnel` / `EstablishedHop` / `EstablishedMaterial`
> secret-material ownership with `Option<EstablishedNextHop>`
> next-hop state, one-shot `into_extracted` transfer, and
> `EstablishedTunnel::into_extracted` material extraction in
> [`src/established.rs`](../../crates/i2pr-tunnel/src/established.rs);
> the runtime-neutral outbound/inbound/local role composition
> with CSPRNG-injected IVs and padding, multi-cell
> `forward_cells` / `process_cells` seams, and
> `OutboundEndpointRole::assemble_actions` rejecting the completed
> message with `UnspecifiedDeliveryInstruction` when the
> reassembler returns no delivery, in
> [`src/roles.rs`](../../crates/i2pr-tunnel/src/roles.rs); the
> pool entries pairing `TunnelRegistration` and
> `EstablishedMaterial` in
> [`src/pool.rs`](../../crates/i2pr-tunnel/src/pool.rs) with
> `register_inbound`, `register_outbound`, and the internal
> `build_placeholder_established` helper now `#[cfg(test)]`; the
> success-only `ShortBuildRegistrar::admit_material` and
> canonical `admit_established_machine` registrar surface, with
> the legacy `admit(&ShortBuildOutcome, …)` failing closed, in
> [`src/short_state.rs`](../../crates/i2pr-tunnel/src/short_state.rs);
> and `ShortBuildStateMachine::take_established_material` plus
> `HopCryptoContext::take_layer_keys` in
> [`src/short.rs`](../../crates/i2pr-tunnel/src/short.rs) so a
> successful `StatePhase::Established` machine produces the
> canonical `EstablishedMaterial` directly.
> See [`plans/116-status.md`](../../plans/116-status.md).
> Not production-ready. See `README.md`,
> `GUARDRAILS.md`,
> [`plans/116-completion-correction.md`](../../plans/116-completion-correction.md),
> [`plans/116-final-closure.md`](../../plans/116-final-closure.md),
> [`plans/116-terminal-cleanup.md`](../../plans/116-terminal-cleanup.md),
> [`plans/111-short-build-final-local-conformance-correction.md`](../../plans/111-short-build-final-local-conformance-correction.md),
> [`plans/112-113-post-plan111-pre-delivery-corrective-roadmap.md`](plans/112-113-post-plan111-pre-delivery-corrective-roadmap.md),
> [`plans/112-outbound-short-build-pre-delivery-closure.md`](../../plans/112-outbound-short-build-pre-delivery-closure.md),
> [`plans/111-status.md`](../../plans/111-status.md),
> [`plans/112-status.md`](../../plans/112-status.md),
> [`plans/113-status.md`](../../plans/113-status.md),
> [`plans/114-status.md`](../../plans/114-status.md),
> [`plans/115-status.md`](../../plans/115-status.md), and
> [`plans/116-status.md`](../../plans/116-status.md).

## Purpose

`i2pr-tunnel` owns:

- typed tunnel identity ([`identity`](src/identity.rs)) —
  `TunnelId`, `TunnelDirection`, `TunnelRole`, `TunnelLifetime`,
  `TunnelState`, `TunnelPeer`;
- bounded exploratory tunnel pool configuration
  ([`config`](src/config.rs)) — `ExploratoryPoolConfig` with hard
  ceilings on `max_inbound`, `max_outbound`, `length_hops`,
  `lifetime_seconds`, `build_concurrency`, and
  `failure_threshold`;
- the deterministic [`pool::ExploratoryPool`](src/pool.rs) with
  bounded replacement, expiry, failure accounting, and the
  `select_inbound_reply_path` selector;
- the [`build::BuildRecordLayout`](src/build.rs) surface over the
  existing `i2pr_proto::DeferredBuildRecords` codec, the typed
  `BuildRequestKind` enumeration with the corrected I2NP message
  identifiers (Short `25`, Variable `23`, OutboundTunnelBuildReply
  `26`), and the canonical 154 / 202 / 218-byte wire sizes;
- the [`build_crypto::BuildCryptography`](src/build_crypto.rs) trait
  with the default `NoBuildCryptography` implementation that always
  returns `BuildCryptographyError::Unavailable`, the
  `LayerKeys` zeroizing wrapper, the `ValidatedRecordSlot` typed
  nonce helper, and the Plan 109 ECIES-X25519 Noise-N primitive
  that seals a 154-byte plaintext into a 218-byte
  `truncated_hash_prefix || ephemeral_pub || ChaChaPoly(msg, h)
  || tag` envelope and authenticates / decrypts the 202-byte
  hop-own reply record;
- the typed [`short_record`](src/short_record.rs) module — the
  Plan 109 154-byte request plaintext encoder (fixed 56-byte
  prefix followed by a canonical `Mapping` and CSPRNG-filled
  post-Mapping padding via `encode_with_rng`, with a deterministic
  zero-padded path `encode_deterministic_zero_padded` for
  fixtures) with strict `HopRole` (0x80 IBGW / 0x40 OBEP / 0x00
  participant), `LayerEncryptionType::Aes` (byte 0),
  request-time minute encoding (`floor(unix_seconds / 60)`), and
  a mandatory 600-second expiration window
  (`ShortBuildError::RandomnessUnavailable` fails closed when no
  CSPRNG is injected); the 202-byte reply plaintext encoder with
  the same CSPRNG/deterministic split, and a decoder with a
  canonical `Mapping` followed by a one-byte response code at
  byte 201 (accept `0`, bandwidth reject `30`);
- the runtime-neutral [`short`](src/short.rs) state machine that
  drives one attempted build through
  `Prepared → Protecting → ReadyForDelivery → AwaitingReply →
  Established` (plus the bounded terminal failures `HopRejected`,
  `TimedOut`, `Cancelled`, `InvalidReply`, `CryptoFailed`,
  `DeliveryFailed`) and emits typed `ShortBuildAction::Deliver`
  events; the state machine enforces the Plan 112
  `ShortBuildPath::validate` direction/role topology rules
  (outbound: no IBGW, OBEP only at the final hop, participants
  before the final OBEP; inbound: IBGW at the first hop, no OBEP,
  participants after the first IBGW), requires an explicit inbound
  `originator_hash`, and constructs the Plan 113 originator fake;
- the [`short_state::ShortBuildRegistrar`](src/short_state.rs)
  success-only registrar that admits a fully validated build into
  `ExploratoryPool`;
- the deterministic [`responder::DeterministicResponder`](src/responder.rs)
  peer simulator that exercises the Noise-N crypto primitive
  end-to-end;
- the [`conformance_fixtures`](src/conformance_fixtures.rs) module
  that owns the independent reference Noise-N implementation and
  the canonical conformance fixture. The fixture is constructed
  once at runtime, drives the production primitive through
  `seal_short_request_with_ephemeral`, and verifies the
  transcript state plus the SMTunnel-derived `replyKey/layerKey/
  ivKey` against an independent HKDF-SHA256 derivation that does
  not call the production `BuildCryptographyError`-wrapping
  KDF path;
- the Plan 112 Rust-only reference provenance test under
  [`crates/i2pr-tunnel/tests/plan111_reference_vectors.rs`](../../crates/i2pr-tunnel/tests/plan111_reference_vectors.rs)
  that re-derives the frozen `fixed_vectors` bytes from a
  pure-Rust path built only on `x25519-dalek`, `sha2`,
  `chacha20poly1305`, and
  `i2pr_crypto::hkdf_sha256_extract_and_expand`, without
  consulting the frozen module, and asserts the production
  `seal_short_request`, `open_short_request`, and `derive_layer_keys`
  reproductions match byte-for-byte;
- the [`provider::ExploratoryPoolReplyPathProvider`](src/provider.rs)
  adapter that turns an `ExploratoryPool` into a
  `i2pr_netdb::ReplyPathProvider`;
- the Plan 115 [`bridge::ShortBuildI2npBridge`](src/bridge.rs) that
  converts a `ShortBuildAction::Deliver` into one complete I2NP
  type-25 message without double-prefixing the STBM record count
  byte, with a standard-header round-trip assertion that the
  recovered body equals the original count-prefixed delivery
  payload exactly. The Plan 115 Q0 test (one outbound hop, pinned
  Emissary as OBEP) passes locally: the production
  `ShortBuildStateMachine::prepare → deliver_action` and
  `ShortBuildI2npBridge::wrap_deliver_action` produce a
  standard-header I2NP type-25 message that Emissary's
  `Message::parse_standard` accepts and
  `TestTransitTunnelManager::handle_short_tunnel_build` consumes;
  the OBEP returns TunnelGateway + Garlic inner message plus a
  feedback channel. Digests and exact pinned revision live in
  [`plans/115-status.md`](../../plans/115-status.md). This Q0
  covers construction + OBEP reply only; Q1 (authenticated
  transport) and Q2 (reply round-trip to Established) remain
  pending.

The crate deliberately remains runtime-neutral: it does not open
sockets, does not perform DNS, does not spawn tasks, and depends only
on `i2pr-proto`, `i2pr-crypto`, `i2pr-core`, and `i2pr-netdb`.

## Module layout

| Module | Purpose |
| --- | --- |
| `identity` | `TunnelId`, `TunnelDirection`, `TunnelRole`, `TunnelLifetime`, `TunnelState`, `TunnelPeer` |
| `config` | Bounded `ExploratoryPoolConfig` with hard ceilings |
| `pool` | Deterministic `ExploratoryPool` with bounded replacement, expiry, and failure accounting |
| `build` | `BuildRecordLayout` (Short/Variable) over `DeferredBuildRecords`; `BuildRequestKind` / `BuildReplyKind` wire-type markers |
| `build_crypto` | `BuildCryptography` trait + `LayerKeys` zeroizing wrapper + `ValidatedRecordSlot` typed nonce + `NoBuildCryptography` default + the Plan 109 ECIES-X25519 Noise-N implementation |
| `short_record` | Typed 154-byte request and 202-byte reply record encoders (`encode_with_rng` + `encode_deterministic_zero_padded`); `HopRole`, `LayerEncryptionType`, `ShortResponseCode`, `BuildOptions`, `ShortBuildError::RandomnessUnavailable` |
| `short` | Runtime-neutral `ShortBuildStateMachine` with bounded `Prepared → Protecting → ReadyForDelivery → AwaitingReply → Established` (plus terminal failures), exact count-prefixed delivery validation, shared direction/role validation, explicit inbound `originator_hash` and outbound `outbound_reply_router` terminal-routing fields, intermediate tunnel-id chain continuity validator, Plan 114 strict trajectory E2E tests, and Plan 113 reference-compatible originator-fake construction |
| `short_state` | Success-only `ShortBuildRegistrar` that admits an established build into `ExploratoryPool` |
| `responder` | Deterministic `DeterministicResponder` peer simulator |
| `conformance_fixtures` | Plan 109 single-record conformance fixtures with independent reference Noise-N and SMTunnel KDF derivation |
| `provider` | `ExploratoryPoolReplyPathProvider` that turns the pool into a `ReplyPathProvider` |
| `bridge` | Plan 115 `ShortBuildI2npBridge` — the canonical production seam from `ShortBuildAction::Deliver` to a complete I2NP type-25 message; no double-prefix, round-trip body equality |

## Dependency boundary

```text
i2pr-tunnel -> i2pr-proto, i2pr-crypto, i2pr-core, i2pr-netdb + thiserror, sha2, zeroize, chacha20poly1305, x25519-dalek, rand_core
```

Runtime-neutral: no `tokio`, no `std::net`, no `std::fs`, no sockets,
no DNS. The plans-of-record forbid adding any of these dependencies.

## Plan 107 + Plan 108 + Plan 109 surfaces

- `identity::TunnelId`, `identity::TunnelDirection`,
  `identity::TunnelRole`, `identity::TunnelLifetime`,
  `identity::TunnelState`, `identity::TunnelPeer` — bounded typed
  values that describe a tunnel slot.
- `config::ExploratoryPoolConfig` with the documented hard
  ceilings and the `balanced()` constructor.
- `pool::ExploratoryPool` — deterministic pool with bounded
  replacement, expiry, failure accounting, and the
  `select_inbound_reply_path` selector.
- `pool::TunnelSlot`, `pool::TunnelRegistration`,
  `pool::RegisterOutcome`, `pool::RegisterError`,
  `pool::PoolFullError`, `pool::PoolError` — the typed pool API.
- `build::BuildRecordLayout`, `build::BuildRequestKind`,
  `build::BuildReplyKind`, `build::BuildRecordLayoutError`,
  `build::BuildCryptographyUnavailable` — the build-record layout
  surface and the corrected I2NP message-type markers.
- `build_crypto::BuildCryptography` trait,
  `build_crypto::BuildCryptographyError`,
  `build_crypto::LayerKeys`, `build_crypto::ValidatedRecordSlot`,
  `build_crypto::NoiseRequestState`, `build_crypto::OpenedShortRequest`,
  `build_crypto::SealedShortRequest`,
  `build_crypto::NoBuildCryptography`,
  `build_crypto::EciesX25519BuildCryptography` — the
  build-cryptography seam and the Plan 109 ECIES-X25519 Noise-N
  implementation.
- `short_record::ShortRequestRecord`, `short_record::ShortReplyRecord`,
  `short_record::HopRole`, `short_record::LayerEncryptionType`,
  `short_record::ShortResponseCode`, `short_record::BuildOptions`,
  `short_record::ShortBuildError` — the typed record surface.
- `short::ShortBuildStateMachine`, `short::ShortBuildPath`,
  `short::HopSpec`, `short::HopCryptoContext`, `short::BuildEvent`,
  `short::ShortBuildAction`, `short::ShortBuildOutcome`,
  `short::ShortBuildConstructionError`,
  `short::ShortTunnelBuildMessage` — the runtime-neutral build
  state machine.
- `short_state::ShortBuildRegistrar`, `short_state::ShortBuildState`,
  `short_state::HopResponse`, `short_state::ShortRegistrarError` —
  the success-only registrar surface.
- `responder::DeterministicResponder`, `responder::ResponderError` —
  the deterministic in-process peer simulator.
- `conformance_fixtures::ReferenceFixture`,
  `conformance_fixtures::verify_against_fixture`,
  `conformance_fixtures::fixture_seal_reply`,
  `conformance_fixtures::FixtureRequest`,
  `conformance_fixtures::DeterministicRng` — the Plan 109
  independent-reference conformance surface.
- `multirecord::validate_count_prefixed_short_payload`,
  `multirecord::encode_count_prefixed_short_payload`,
  `multirecord::decode_short_tunnel_build_payload` — the Plan 112
  explicit STBM/OTBRM `count || records` contract helpers that
  reject zero record counts, record counts above 8, and any
  payload whose length does not equal
  `1 + count * 218` bytes.
- `tests::plan111_reference_vectors` (integration test) — the
  Plan 112 Rust-only reference provenance test that re-derives
  the frozen `fixed_vectors` constants without consulting the
  frozen module and asserts byte-for-byte equivalence with the
  production `seal_short_request` / `open_short_request` /
  `derive_layer_keys` primitives.
- `provider::ExploratoryPoolReplyPathProvider` — the
  `ReplyPathProvider` adapter.
- `bridge::ShortBuildI2npBridge::wrap_deliver_action` accepts a
  `ShortBuildAction::Deliver`, validates
  `payload.len() == 1 + record_count * 218` and
  `payload[0] == record_count`, splits the count byte from the
  raw records, builds `DeferredBuildRecords::new(count, 218, …)`,
  wraps in `I2npBody::ShortTunnelBuild`, encodes with the
  requested standard or short-transport header, and asserts the
  round-trip decoder recovers the original count-prefixed body
  exactly. The bridge never mutates, reorders, or regenerates
  records and never logs raw record bytes.

## Key contracts

- `TunnelId::new(value)` rejects zero; the pool refuses zero
  identifiers and the lookup state machine refuses zero reply-path
  tunnel IDs.
- `ExploratoryPoolConfig::try_new` rejects any value that exceeds
  the documented hard ceilings; the configuration is bounded at
  every layer.
- `ExploratoryPool::register_inbound` returns
  `RegisterError::Full { kind, registration }` when the pool is at
  capacity; the registration is **not** silently dropped.
- `ExploratoryPool::select_inbound_reply_path(now)` returns the
  oldest valid inbound tunnel; expired and failed tunnels are never
  returned.
- `BuildCryptography::seal_short_request_with_ephemeral` and the
  matching `open_short_request` always validate inputs first;
  the default `NoBuildCryptography` implementation rejects every
  call.
- `LayerKeys` is `Zeroize`-derived and has no `Debug`, `Clone`, or
  serde implementations. `ValidatedRecordSlot` has only the
  doc-required API and a typed `nonce()` constructor.
- `ShortRequestRecord::encode()` produces exactly 154 bytes
  (fixed 56-byte prefix + Mapping + padding); `ShortReplyRecord::
  encode()` produces exactly 202 bytes (Mapping + padding +
  one-byte response at offset 201); `EciesX25519BuildCryptography`
  produces exactly 218-byte sealed records.
- `ShortBuildStateMachine` reaches `Established` only after every
  per-hop reply authenticates; `ShortBuildRegistrar::admit` admits
  an established outcome into `ExploratoryPool` and rejects every
  other terminal outcome.

## Plan 111 acceptance criteria (locally conformant against fixed vectors)

Plan 111 was recorded as `passed-final-local-short-build-conformance`
when the following local conformance criteria were satisfied:

1. `i2pr-tunnel` compiles and passes its own unit tests.
2. The 154-byte request plaintext encodes the exact 56-byte fixed
   prefix (tunnel ids, router hash, role flag, padding flag bytes,
   `LayerEncryptionType::Aes` byte, request-time minutes, mandatory
   600-second expiration, next-message id) followed by the canonical
   two-byte `Mapping` length and body.
3. Role flag bytes are `0x80` (IBGW), `0x40` (OBEP), `0x00`
   (participant); the `0x01`/`0x02` and `0xC0` patterns are
   rejected; `LayerEncryptionType::EciesAeadOnly` (0x05) is
   rejected.
4. `EciesX25519BuildCryptography::seal_short_request_with_ephemeral`
   produces exactly 218 bytes
   `truncated_hash_prefix (16) || ephemeral_pub (32) ||
   ciphertext (154) || tag (16)`; `open_short_request` returns
   exactly 154 bytes and rejects records whose first 16 bytes do
   not match the supplied hop identity hash.
5. The Noise-N transcript starts at
   `h0 = protocol_name_padded_to_32(Noise_N_25519_ChaChaPoly_SHA256)`,
   applies the canonical null-prologue `h = SHA256(h0)`, sets
   `ck = h0`, mixes the peer static public key, mixes the sender
   ephemeral public key, runs the `es` derivation as a single
   `HKDF(ck, sharedSecret, "", 64)` whose first 32 bytes become
   the new chaining key and whose second 32 bytes become the
   request AEAD key, encrypts the 154-byte request with
   ChaCha20-Poly1305 using `nonce = 0` and `ad = h`, and finally
   MixHashes the ciphertext+tag to produce the saved post-request
   `h`.
6. `SMTunnelReplyKey` → `SMTunnelLayerKey` are derived exactly;
   non-OBEP uses the first 32 bytes as `ivKey` and the last 32 as
   `layerKey`; OBEP follows the second continuation through
   `TunnelLayerIVKey` and then derives the additional
   `RGarlicKeyAndTag` material with the canonical 32-byte key
   and 8-byte tag layout.
7. `seal_short_reply` produces a 218-byte record with no ephemeral
   or nonce prefix; the AEAD uses the derived `replyKey`, the
   caller-supplied `ValidatedRecordSlot::nonce()` (the 12-byte
   ChaChaPoly nonce is zero in bytes 0..3 and 5..11 and carries
   the slot byte at offset 4), and the saved post-request `h` as
   the associated data.
8. `open_short_reply` returns the exact 202-byte reply plaintext
   and rejects records when the slot, the `replyKey`, or the saved
   `h` do not match.
9. Per-hop `receive_tunnel` and `next_tunnel` identifiers are
   explicit independent `TunnelId` fields on `HopSpec` and
   `MultiRecordHopSpec`; the path validator rejects zero IDs and
   the request plaintext encoder never derives a tunnel id from
   any router-hash prefix.
10. `MessageHopProcessor::process_hop` decodes the hop role from
    the authenticated request plaintext through
    `ShortRequestRecord::decode`; the OBEP role surfaces as
    `is_obep = true` on `ProcessedHopResult`, the participant role
    as `is_obep = false`. Invalid role flag bytes are rejected by
    `HopRole::from_flag` before any layer-key derivation runs.
11. `ReferenceFixture::canonical()` constructs the canonical
    fixture through the independent reference SHA-256 + Noise
    `MixHash`/`MixKey` chain and the shared `i2pr-crypto`
    HKDF-SHA256 helper, and asserts that the production
    `EciesX25519BuildCryptography::seal_short_request_with_ephemeral`
    reaches the same transcript hash and derives the same
    `replyKey`, `layerKey`, and `ivKey`.
12. `crates/i2pr-tunnel/src/fixed_vectors.rs` freezes the
    canonical Noise-N transcript hash chain, the X25519 public
    keys, the X25519 shared secret, the request `es` HKDF
    output, the post-request chaining key, the request AEAD key,
    the 218-byte sealed envelope, the slot byte-4 nonces for the
    hop-own reply AEAD and the raw ChaCha20 other-record transform,
    the `SMTunnelReplyKey`/`SMTunnelLayerKey` derived `replyKey`
    /`layerKey`/`ivKey`, and the OBEP continuation garlic key and
    8-byte tag. The frozen constants were generated once during
    the Plan 111 implementation pass from an independent
    reference Noise-N + HKDF-SHA256 + ChaCha20-Poly1305 oracle
    that uses only low-level primitives; the production
    primitive must match every frozen constant or fail the
    conformance tests.
13. The deterministic `DeterministicResponder` accepts a sealed
    record, derives the layer keys, seals a reply plaintext, and
    the production primitive round-trips that reply through
    `open_short_reply` for every valid slot in `0..=7`.
14. Plan 113 records the unresolved final-spec creator-key prose and
    the pinned Java/i2pd agreement. Inbound uses the explicit
    `reference-compatible-spec-text-discrepancy` policy; no guessed
    plaintext field is added.
15. Plan 114 closes four post-Plan-113 routing/composition defects.
    `ShortBuildPath` carries an explicit `outbound_reply_router`
    (outbound) and `originator_hash` (inbound) terminal-routing
    field. The intermediate
    `hops[i].next_tunnel == hops[i+1].receive_tunnel` chain
    invariant is enforced at the high-level
    `ShortBuildPath::validate()` boundary and at the public
    lower-level `prepare_short_build_message()` entry point. The
    `last_payload()` accessor re-exposes the preprocessed STBM
    payload for strict trajectory tests. The strict outbound and
    inbound E2E tests drive each real hop through
    `MessageHopProcessor::process_hop` with `Accepted` and assert
    `Established` exactly; `InvalidReply` is no longer an
    acceptable alternative.

These criteria prove only that the local creator and
independent-reference implementations agree with the production
primitive against frozen fixed vectors; they do **not** prove
interoperability against a live Java I2P / i2pd reference. A
later narrow external-delivery checkpoint that carries one
already-correct STBM payload to an independent router can
promote the surface to `interoperable`.

## Distinctive design choices

1. **Tunnel IDs reject zero.** The pool and the lookup state
   machine refuse zero; the value is reserved by I2P for "no
   tunnel".
2. **Configuration is bounded at every layer.** The
   `ExploratoryPoolConfig::try_new` constructor rejects every value
   that exceeds the documented hard ceiling; the configuration
   cannot become a vector for unbounded growth.
3. **Time is injected.** `advance_time` is the only way to surface
   expiry; the pool never reads the wall clock.
4. **Reply-path provider is a trait, not a concrete type.** The
   seam consumes `Box<dyn ReplyPathProvider>`, so the production
   pool implementation can be swapped without rewrites.
5. **Build cryptography is a trait, not a concrete type.** The live
   ECIES-X25519 Noise-N primitive ships in
   `EciesX25519BuildCryptography`; the default `NoBuildCryptography`
   implementation always returns `Unavailable` so the no-build
   configuration cannot silently claim cryptographic coverage it
   does not have.
6. **Success-only registrar.** `ShortBuildRegistrar::admit` admits
   exactly `ShortBuildOutcome::Established`; every other terminal
   category is rejected with a typed `ShortRegistrarError`. A build
   that never produces an established outcome never enters the
   pool.
7. **Independent reference conformance.** The Plan 109 fixture
   keeps a structured SHA-256 + Noise-`MixHash`/`MixKey` reference
   implementation inside the crate and verifies the production
   primitive against the canonical fixture at every test run, so
   future changes cannot silently regress the wire or cryptographic
   model without a refactor of both sides.
8. **No `tokio`, no `std::net`, no `std::fs`.** The crate is
   runtime-neutral; the live build orchestration will live in
   `i2pr-runtime` and depend on `i2pr-tunnel`, never the other way
   around.

## Multi-record construction (Plan 110)

[`multirecord`](src/multirecord.rs) owns the Plan 110 multi-record
short-build surface:

- `ShortBuildRecordSet` and `assign_record_slots` — typed record-set
  representation with a rejection-sampled Fisher-Yates permutation;
- `OriginatorFake`, `build_originator_fake_record`, and
  `verify_originator_fake` — the inbound originator fake record with
  SHA-256 integrity, including the 16-byte hash prefix, ephemeral
  X25519 public key, and random remainder; Plan 113 requires exactly
  one fake for inbound builds and verifies it after reply processing;
- `chacha20_transform` and `chacha20_xor` — raw ChaCha20 stream
  transforms using `crypto::chacha::ChaCha20` with the canonical
  12-byte nonce (zero in bytes 0..3 and 5..11, target slot byte at
  offset 4, with the eight-byte little-endian nonce occupying
  bytes 4..11);
- `prepare_short_build_message` — the creator-side preprocessor that
  seals every real-hop request, applies raw ChaCha20 transforms for
  each prior hop's reply key, and produces a `count + count*218`
  `ShortTunnelBuild` payload;
- `MessageHopProcessor` — the per-hop in-transit processor that
  locates the matching record by 16-byte hash prefix, opens the
  sealed request, derives the layer keys, seals the reply, and
  pre-processes every other slot for the next hop;
- `CreatorReplyPostprocessor` — the creator-side reply processor that
  undoes every symmetric transform, opens every real-hop reply, and
  verifies the inbound originator-fake integrity hash;
- `MultiHopReferenceFixture::three_hop_one_fake` — the deterministic
  three-hop one-fake trajectory fixture used as the local conformance
  oracle.

The state-machine integration lives in
[`short`](src/short.rs): `ShortBuildStateMachine::prepare` calls
`prepare_short_build_message`, and `handle_event(BuildEvent::BuildReply)`
calls `CreatorReplyPostprocessor::process_reply`. The registrar
([`short_state`](src/short_state.rs)) admits only `Established`
outcomes.

## Plan 117 activated-role lifecycle

`DataPlaneRegistry` keeps outbound roles keyed by `TunnelSlot` and binds each
activated inbound role to both its `TunnelSlot` and local receive `TunnelId`.
The bounded reverse maps let pool expiry/failure reports call
`remove_slot(slot)` for either direction; removal clears role, routing
metadata, and both reverse indexes atomically. `RegistryRemoval` is a typed
consumed-role result, and no `LayerKeys` clone is introduced. Duplicate slot
and receive-id activation remains fail-closed.

Plan 117 closes per Plan 118 as
`closed-for-progression-with-evidence-gap`. The corrected in-tree
native Emissary reference test reached native OBEP admission and
reply AEAD opening, but the reply plaintext was rejected by i2pr's
strict `ShortReplyRecord` decoder because the pinned Emissary
handler emits a request-prefixed reply instead of the normative
202-byte reply layout. The reference-side defect is localized to
the pinned Emissary revision
`9b43484a21d5a1291c4881cdae62a36c527f8c0f`; no upstream correction
is available. The i2pr parser is not relaxed; publication, lookup,
and inbound return stages are not claimed. Plan 119 closed as
`passed-leaseset2-protocol-foundation` per
[`plans/119-status.md`](../../plans/119-status.md); the ordinary
online-signed published Standard LeaseSet2 carrier is wired into
`i2pr-proto` and `i2pr-netdb` for the local Phase G production
seam. The next executable plan is **Plan 120** (destination
lifecycle and dedicated tunnel pools) under
[`plans/118-123-milestone6-router-construction-roadmap.md`](../../plans/118-123-milestone6-router-construction-roadmap.md).

## Out of scope (next plans)

- live mixed-router tunnel build execution against Java I2P and i2pd
  (deferred to a qualified external delivery lane; tracked under the
  external acceptance debt ledger in
  [`plans/118-123-milestone6-router-construction-roadmap.md`](../../plans/118-123-milestone6-router-construction-roadmap.md));
- transit participation (Milestone 11);
- destination-specific tunnel pools and LeaseSet2 publication
  (Milestone 6, owned by Plan 119 → Plan 122);
- legacy 528-byte ECIES build implementation beyond preserving
  existing parsing/layout types;
- ElGamal/ECIES mixed-router construction.

## Cross-references

- [`i2pr-crypto`](i2pr-crypto.md) — owns the X25519 wrappers,
  HKDF-SHA256 helper, and `X25519SharedSecret` zeroizing owner the
  Plan 109 ECIES primitive consumes.
- [`i2pr-proto`](i2pr-proto.md) — owns the canonical I2NP message
  identifiers, the `ShortTunnelBuild`/`OutboundTunnelBuildReply`
  constants, and the wire-size constants Plan 109 enforces.
- [`i2pr-netdb`](i2pr-netdb.md) — owns the `ReplyPath` token and
  the `ReplyPathProvider` trait that the new adapter implements.
- [`i2pr-daemon`](i2pr-daemon.md) — composes the NetDB seam; the
  seam consumes the reply-path provider when one is injected.
- Plan-of-record:
  [`plans/109-short-build-record-and-noise-conformance-correction.md`](../../plans/109-short-build-record-and-noise-conformance-correction.md).
- Inbound policy evidence and closure:
  [`specs/references/short-build-inbound-creator-key.md`](../../specs/references/short-build-inbound-creator-key.md),
  [`plans/113-status.md`](../../plans/113-status.md).

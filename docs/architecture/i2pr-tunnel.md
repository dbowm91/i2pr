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

> Status: Plan 109 single-record Noise-N conformance landed. The
> multi-record `ShortTunnelBuild` slot/fake-record/preprocessing
> closure is Plan 110 scope. Not production-ready. See
> `README.md`, `GUARDRAILS.md`,
> [`plans/109-short-build-record-and-noise-conformance-correction.md`](../../plans/109-short-build-record-and-noise-conformance-correction.md),
> and [`plans/109-110-plan108-short-build-protocol-conformance-corrective-roadmap.md`](../../plans/109-110-plan108-short-build-protocol-conformance-corrective-roadmap.md).

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
  prefix followed by a canonical `Mapping` and random padding)
  with strict `HopRole` (0x80 IBGW / 0x40 OBEP / 0x00 participant),
  `LayerEncryptionType::Aes` (byte 0), request-time minute
  encoding (`floor(unix_seconds / 60)`), and a mandatory
  600-second expiration window; the 202-byte reply plaintext
  encoder/decoder with a canonical `Mapping` followed by a
  one-byte response code at byte 201 (accept `0`, bandwidth reject
  `30`);
- the runtime-neutral [`short`](src/short.rs) state machine that
  drives one attempted build through
  `Prepared → Protecting → ReadyForDelivery → AwaitingReply →
  Established` (plus the bounded terminal failures `HopRejected`,
  `TimedOut`, `Cancelled`, `InvalidReply`, `CryptoFailed`,
  `DeliveryFailed`) and emits typed `ShortBuildAction::Deliver`
  events;
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
- the [`provider::ExploratoryPoolReplyPathProvider`](src/provider.rs)
  adapter that turns an `ExploratoryPool` into a
  `i2pr_netdb::ReplyPathProvider`.

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
| `short_record` | Typed 154-byte request and 202-byte reply record encoders; `HopRole`, `LayerEncryptionType`, `ShortResponseCode`, `BuildOptions` |
| `short` | Runtime-neutral `ShortBuildStateMachine` with bounded `Prepared → Protecting → ReadyForDelivery → AwaitingReply → Established` (plus terminal failures) and the typed `ShortBuildAction::Deliver` event |
| `short_state` | Success-only `ShortBuildRegistrar` that admits an established build into `ExploratoryPool` |
| `responder` | Deterministic `DeterministicResponder` peer simulator |
| `conformance_fixtures` | Plan 109 single-record conformance fixtures with independent reference Noise-N and SMTunnel KDF derivation |
| `provider` | `ExploratoryPoolReplyPathProvider` that turns the pool into a `ReplyPathProvider` |

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
- `provider::ExploratoryPoolReplyPathProvider` — the
  `ReplyPathProvider` adapter.

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

## Plan 109 acceptance criteria (locally conformant)

Plan 109 was recorded as `passed-record-and-noise-conformance` when
the following local conformance criteria were satisfied:

1. `i2pr-tunnel` compiles and passes its own unit tests.
2. The 154-byte request plaintext encodes the exact 56-byte fixed
   prefix (tunnel ids, router hash, role flag, padding flag bytes,
   `LayerEncryptionType::Aes` byte, request-time minutes, mandatory
   600-second expiration, next-message id) followed by the canonical
   two-byte `Mapping` length and body.
3. Role flag bytes are `0x80` (IBGW), `0x40` (OBEP), `0x00`
   (participant); the Plan 108 `0x01`/`0x02` and `0xC0` patterns
   are rejected; `LayerEncryptionType::EciesAeadOnly` (0x05) is
   rejected.
4. `EciesX25519BuildCryptography::seal_short_request_with_ephemeral`
   produces exactly 218 bytes
   `truncated_hash_prefix (16) || ephemeral_pub (32) ||
   ciphertext (154) || tag (16)`; `open_short_request` returns
   exactly 154 bytes and rejects records whose first 16 bytes do
   not match the supplied hop identity hash.
5. The Noise-N transcript starts at
   `h = protocol_name_padded_to_32(Noise_N_25519_ChaChaPoly_SHA256)`,
   mixes the peer static public key, the sender ephemeral public
   key, performs the asymmetric `es` `MixKey`, encrypts the
   154-byte request with ChaCha20-Poly1305 using `nonce = 0` and
   `ad = h`, and finally MixHashes the ciphertext+tag to produce
   the saved post-request `h`.
6. `SMTunnelReplyKey` → `SMTunnelLayerKey` are derived exactly;
   non-OBEP uses the first 32 bytes as `ivKey` and the last 32 as
   `layerKey`; OBEP follows the second continuation through
   `TunnelLayerIVKey` and then derives the additional
   `RGarlicKeyAndTag` material.
7. `seal_short_reply` produces a 218-byte record with no ephemeral
   or nonce prefix; the AEAD uses the derived `replyKey`, the
   caller-supplied `ValidatedRecordSlot::nonce()` (the
   12-byte ChaChaPoly nonce is zero in the first 11 bytes and
   carries the slot byte at offset 11), and the saved post-request
   `h` as the associated data.
8. `open_short_reply` returns the exact 202-byte reply plaintext
   and rejects records when the slot, the `replyKey`, or the saved
   `h` do not match.
9. `ReferenceFixture::canonical()` constructs the canonical
   fixture through the independent reference SHA-256 + Noise
   `MixHash`/`MixKey` chain and the shared `i2pr-crypto`
   HKDF-SHA256 helper, and asserts that the production
   `EciesX25519BuildCryptography::seal_short_request_with_ephemeral`
   reaches the same transcript hash and derives the same
   `replyKey`, `layerKey`, and `ivKey`.
10. The deterministic `DeterministicResponder` accepts a sealed
    record, derives the layer keys, seals a reply plaintext, and
    the production primitive round-trips that reply through
    `open_short_reply` for every valid slot in `0..=7`.

These ten criteria prove only that the local creator and
independent-reference implementations agree with the production
primitive; they do **not** prove interoperability against a live
Java I2P / i2pd reference. Plan 110 closes the multi-record
implementation surface and only an external mixed-router
qualification run can promote the surface to `interoperable`.

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

## Out of scope (Plan 110+)

- randomized multi-record slot assignment;
- fake build records;
- inbound originator fake-record integrity;
- iterative ChaCha20 preprocessing of other records;
- complete `ShortTunnelBuild` / `OutboundTunnelBuildReply` payload
  framing with the one-byte record count;
- live mixed-router tunnel build execution against Java I2P and i2pd;
- transit participation (Milestone 11);
- destination-specific tunnel pools (Milestone 6);
- LeaseSet publication from tunnel records;
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

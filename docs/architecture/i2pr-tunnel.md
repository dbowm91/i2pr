# `i2pr-tunnel` — Milestone 5 exploratory tunnel substrate

Runtime-neutral tunnel identity, exploratory tunnel pool, build-record
layout surface, build-cryptography seam, ECIES-X25519 short tunnel-build
construction primitive, runtime-neutral build state machine,
success-only `ExploratoryPool` registrar, and reply-path provider for
`i2pr`. The crate is the Milestone 5 implementation surface (Plans 107
and 108) and lands the substrate required to flip the Plan 106 NetDB
seam from `BlockedExploratoryTunnelUnavailable` to `Available` once a
real inbound tunnel is registered, and to drive that registration
through a fully constructed short tunnel-build attempt.

> Status: experimental. Not production-ready. See `README.md` and
> `GUARDRAILS.md`.

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
  `LayerKeys` zeroizing wrapper, and the Plan 108
  `EciesX25519BuildCryptography` primitive that seals a 154-byte
  plaintext into a 218-byte sealed record and authenticates/decrypts
  the 202-byte reply;
- the typed [`short_record`](src/short_record.rs) module — the
  154-byte request plaintext encoder and the 202-byte reply plaintext
  encoder/decoder with strict `HopRole`/`LayerEncryptionType`/
  `ShortResponseCode` validation;
- the runtime-neutral [`short`](src/short.rs) state machine that
  drives one attempted build through
  `Prepared → Protecting → ReadyForDelivery → AwaitingReply → Established`
  (plus the bounded terminal failures `HopRejected`, `TimedOut`,
  `Cancelled`, `InvalidReply`, `CryptoFailed`, `DeliveryFailed`)
  and emits typed `ShortBuildAction::Deliver` events;
- the [`short_state::ShortBuildRegistrar`](src/short_state.rs)
  success-only registrar that admits a fully validated build into
  `ExploratoryPool`;
- the deterministic [`responder::DeterministicResponder`](src/responder.rs)
  peer simulator that proves the local algorithm end-to-end without
  self-mirroring the cryptography primitive;
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
| `build_crypto` | `BuildCryptography` trait + `LayerKeys` zeroizing wrapper + `NoBuildCryptography` default + `EciesX25519BuildCryptography` Plan 108 implementation |
| `short_record` | Typed 154-byte request and 202-byte reply record encoders; `HopRole`, `LayerEncryptionType`, `ShortResponseCode`, `BuildOptions` |
| `short` | Runtime-neutral `ShortBuildStateMachine` with bounded `Prepared → Protecting → ReadyForDelivery → AwaitingReply → Established` (plus terminal failures) and the typed `ShortBuildAction::Deliver` event |
| `short_state` | Success-only `ShortBuildRegistrar` that admits an established build into `ExploratoryPool` |
| `responder` | Deterministic `DeterministicResponder` peer simulator |
| `provider` | `ExploratoryPoolReplyPathProvider` that turns the pool into a `ReplyPathProvider` |

## Dependency boundary

```text
i2pr-tunnel -> i2pr-proto, i2pr-crypto, i2pr-core, i2pr-netdb + thiserror, sha2, zeroize, chacha20poly1305, x25519-dalek
```

Runtime-neutral: no `tokio`, no `std::net`, no `std::fs`, no sockets,
no DNS. The plans-of-record
[`plans/107-milestone-5-exploratory-tunnel-substrate.md`](../../plans/107-milestone-5-exploratory-tunnel-substrate.md)
and
[`plans/108-live-ecies-x25519-short-tunnel-build-construction.md`](../../plans/108-live-ecies-x25519-short-tunnel-build-construction.md)
explicitly forbid adding any of these dependencies.

## Plan 107 + Plan 108 surfaces

- `identity::TunnelId`, `identity::TunnelDirection`,
  `identity::TunnelRole`, `identity::TunnelLifetime`,
  `identity::TunnelState`, `identity::TunnelPeer` — bounded typed
  values that describe a tunnel slot.
- `config::ExploratoryPoolConfig` with the documented hard
  ceilings and the `balanced()` constructor.
- `pool::ExploratoryPool` — deterministic pool with bounded
  replacement, expiry, failure accounting, and the
  `select_inbound_reply_path` selector.
- `pool::TunnelSlot` — monotonic slot identifier.
- `pool::TunnelRegistration` — the typed registration record.
- `pool::RegisterOutcome`, `pool::RegisterError`,
  `pool::PoolFullError`, `pool::PoolError` — the typed failure
  vocabulary.
- `build::BuildRecordLayout`, `build::BuildRequestKind`,
  `build::BuildReplyKind`, `build::BuildRecordLayoutError`,
  `build::BuildCryptographyUnavailable` — the build-record layout
  surface and the corrected I2NP message-type markers.
- `build_crypto::BuildCryptography` trait,
  `build_crypto::BuildCryptographyError`,
  `build_crypto::LayerKeys`, `build_crypto::NoBuildCryptography`,
  `build_crypto::EciesX25519BuildCryptography` — the build-cryptography
  seam and the Plan 108 ECIES-X25519 implementation.
- `short_record::ShortRequestRecord`, `short_record::ShortReplyRecord`,
  `short_record::HopRole`, `short_record::LayerEncryptionType`,
  `short_record::ShortResponseCode`, `short_record::BuildOptions`,
  `short_record::ShortBuildError` — the typed record surface.
- `short::ShortBuildStateMachine`, `short::ShortBuildPath`,
  `short::HopSpec`, `short::HopCryptoSeed`, `short::HopCryptoContext`,
  `short::BuildEvent`, `short::BuildAction`, `short::BuildAttemptId`,
  `short::ShortBuildOutcome`, `short::ShortBuildConstructionError`,
  `short::ShortTunnelBuildMessage` — the runtime-neutral build state
  machine.
- `short_state::ShortBuildRegistrar`, `short_state::ShortBuildState`,
  `short_state::HopResponse`, `short_state::ShortBuildDirectionError`,
  `short_state::ShortRegistrarError` — the success-only registrar
  surface.
- `responder::DeterministicResponder`, `responder::ResponderError` —
  the deterministic in-process peer simulator.
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
- `BuildCryptography::seal_short_request` and
  `BuildCryptography::open_short_request` always validate inputs
  first; the default `NoBuildCryptography` implementation rejects
  every call.
- `LayerKeys` is `Zeroize`-derived and has no `Debug`, `Clone`, or
  serde implementations.
- `ShortRequestRecord::encode()` produces exactly 154 bytes;
  `ShortReplyRecord::encode()` produces exactly 202 bytes;
  `EciesX25519BuildCryptography` produces exactly 218-byte sealed
  records.
- `ShortBuildStateMachine` reaches `Established` only after every
  per-hop reply authenticates; `ShortBuildRegistrar::admit` admits
  an established outcome into `ExploratoryPool` and rejects every
  other terminal outcome.

## Plan 108 acceptance criteria

Plan 108 closes when:

1. `i2pr-tunnel` compiles and passes its own unit tests.
2. `BuildRequestKind::ShortTunnelBuild.message_type() == 25`,
   `BuildRequestKind::VariableTunnelBuild.message_type() == 23`,
   `BuildReplyKind::OutboundTunnelBuildReply.message_type() == 26`,
   `BuildReplyKind::VariableTunnelBuildReply.message_type() == 24`.
3. `EciesX25519BuildCryptography::seal_short_request` produces
   exactly 218 bytes; `open_short_request` returns exactly 154
   bytes.
4. Wrong peer static key, altered ephemeral pubkey, altered
   ciphertext, and altered Poly1305 tag all return
   `BuildCryptographyError::AuthenticationFailed`.
5. Per-hop ephemeral keys are unique across consecutive seals with
   the same `request_key_seed`.
6. `ShortBuildStateMachine::prepare` produces exactly
   `hop_count * 218` sealed record bytes and rejects oversize
   paths, zero-key paths, and out-of-range expirations.
7. `ShortBuildRegistrar::admit` rejects every non-`Established`
   outcome with `ShortRegistrarError::NotEstablished` and admits
   an `Established` outcome without mutating the pool's
   replacement/conflict invariants.
8. The deterministic `DeterministicResponder` accepts and decrypts
   records the `EciesX25519BuildCryptography` produces, then
   produces a `ShortReplyRecord` that the
   `BuildCryptography::open_short_reply` path decrypts.

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
   ECIES-X25519 primitive ships in `EciesX25519BuildCryptography`;
   the default `NoBuildCryptography` implementation always returns
   `Unavailable` so the no-build configuration cannot silently claim
   cryptographic coverage it does not have.
6. **Success-only registrar.** `ShortBuildRegistrar::admit` admits
   exactly `ShortBuildOutcome::Established`; every other terminal
   category is rejected with a typed `ShortRegistrarError`. A build
   that never produces an established outcome never enters the
   pool.
7. **No `tokio`, no `std::net`, no `std::fs`.** The crate is
   runtime-neutral; the live build orchestration will live in
   `i2pr-runtime` and depend on `i2pr-tunnel`, never the other way
   around.

## Out of scope (Plan 109+)

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
  Plan 108 ECIES primitive consumes.
- [`i2pr-proto`](i2pr-proto.md) — owns the canonical I2NP message
  identifiers, the `ShortTunnelBuild`/`OutboundTunnelBuildReply`
  constants, and the wire-size constants Plan 108 enforces.
- [`i2pr-netdb`](i2pr-netdb.md) — owns the `ReplyPath` token and
  the `ReplyPathProvider` trait that the new adapter implements.
- [`i2pr-daemon`](i2pr-daemon.md) — composes the NetDB seam; the
  seam consumes the reply-path provider when one is injected.
- Plan-of-record: [`plans/108-live-ecies-x25519-short-tunnel-build-construction.md`](../../plans/108-live-ecies-x25519-short-tunnel-build-construction.md).

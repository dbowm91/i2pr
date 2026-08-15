# Plan 109 closure: short-build record and Noise-N conformance

> **2026-08-15 conformance amendment:** A post-Plan-110 audit found that Plan 109's local wire/crypto implementation still contains protocol defects shared by Plan 110: the required Noise null-prologue `MixHash` is missing; the request `es` AEAD key is obtained through an incorrect second HKDF instead of the single `HKDF(ck, sharedSecret, "", 64)` split; record-slot nonce construction uses byte 11 instead of byte 4; OBEP garlic tag material is represented as 16 instead of 8 bytes; and the fixture obtains critical expected state from the production implementation. The historical closure record below is retained for audit only. Current corrective authority is [`plans/111-short-build-final-local-conformance-correction.md`](111-short-build-final-local-conformance-correction.md). Until Plan 111 passes, `passed-record-and-noise-conformance` is superseded by `implementation-landed-conformance-reopened-by-plan111`.

- Status: **implementation-landed-conformance-reopened-by-plan111**
- Historical closure date: 2026-08-15
- Pre-Plan 109 commit: `c74493433ac39eb4ae6e617961f737f6b7e0a9d5`
- Plan-of-record:
  [`plans/109-short-build-record-and-noise-conformance-correction.md`](109-short-build-record-and-noise-conformance-correction.md)
- Corrective successor:
  [`plans/111-short-build-final-local-conformance-correction.md`](111-short-build-final-local-conformance-correction.md)

## Current authoritative state

```text
plan_109                         = implementation-landed-conformance-reopened-by-plan111
plan_110                         = implementation-landed-conformance-reopened-by-plan111
plan_111                         = ready-for-implementation
single_record_short_build_crypto = needs-plan111-correction
short_build_derived_keys         = needs-plan111-correction
external_build_delivery          = blocked-on-plan111
```

## Historical Plan 109 closure record

The remainder of this file records the historical Plan 109 implementation result. Its conformance claims are superseded where they conflict with Plan 111.

## Summary

Plan 109 replaced the Plan 108 wire/cryptographic surface with the
canonical I2P Tunnel Creation Specification Noise-N
short-build implementation. The single-record request plaintext,
encrypted envelope, reply plaintext, hop-own reply AEAD, and
post-request KDF chain are now byte-for-byte conformant. The
multi-record slot/fake-record/preprocessing closure is Plan 110
scope.

```text
plan_108                         = superseded-local-architecture-retained-wire-crypto-corrected
plan_109                         = passed-record-and-noise-conformance
single_record_short_build_crypto = locally-conformant
short_build_derived_keys         = locally-conformant
multirecord_short_build_message  = pending-plan110
external_build_delivery          = unavailable
live_mixed_router_build          = blocked-on-plan110-and-qualified-delivery
```

## Files changed

The implementation surface lands in `crates/i2pr-tunnel`:

- `crates/i2pr-tunnel/src/lib.rs` — module re-exports updated to
  expose `LayerKeys`, `ValidatedRecordSlot`, `NoiseRequestState`,
  `OpenedShortRequest`, `SealedShortRequest`, `AEAD_KEY_LEN`,
  `AEAD_NONCE_LEN`, `EPHEMERAL_KEY_LEN`, `HASH_PREFIX_LEN`,
  `TAG_LEN`, and the `conformance_fixtures` module.
- `crates/i2pr-tunnel/src/short_record.rs` — rewritten to the
  Plan 109 154-byte request plaintext layout (fixed 56-byte
  prefix + canonical `Mapping` + padding) and the 202-byte
  reply plaintext (`Mapping` + padding + 1-byte response at
  offset 201). `HopRole` exposes the canonical `0x80` IBGW /
  `0x40` OBEP / `0x00` participant flags; `LayerEncryptionType`
  exposes only the canonical `0` (AES); `ShortResponseCode`
  exposes only `0` (Accepted) and `30` (BandwidthRejected).
  `REQUEST_EXPIRATION_SECONDS = 600` is enforced at the
  constructor.
- `crates/i2pr-tunnel/src/build_crypto.rs` — rewritten to the
  Plan 109 Noise-N transcript
  (`Noise_N_25519_ChaChaPoly_SHA256`), literal `MixHash` /
  `MixKey` ordering, peer-static mix, ephemeral mix, asymmetric
  `es`, ChaCha20-Poly1305 with nonce `0` and AD = `current h`,
  and post-AEAD ciphertext MixHash. The 218-byte encrypted
  envelope layout is
  `truncated_hop_hash (16) || ephemeral_pub (32) ||
  ciphertext (154) || tag (16)`. The hop-own reply AEAD uses the
  derived `replyKey`, the caller-supplied
  `ValidatedRecordSlot::nonce()` (zero in the first 11 bytes,
  slot byte at offset 11), and the saved post-request `h` as the
  AEAD associated data. The Plan 108 `32 + 16 + 170` envelope
  layout is rejected fail-closed by the
  `plan108_envelope_layout_rejected` regression test. The
  post-request `SMTunnelReplyKey` / `SMTunnelLayerKey` /
  `TunnelLayerIVKey` / `RGarlicKeyAndTag` chain is implemented
  as `derive_layer_keys` with the OBEP continuation supported.
- `crates/i2pr-tunnel/src/short.rs` — rewritten to consume the
  new cryptography surface. `HopSpec` now carries the canonical
  truncated `hop_hash_prefix` derived from the supplied router
  hash. `HopCryptoContext` retains the saved post-request `h`,
  the derived `LayerKeys`, and the sender ephemeral public key.
  `ShortBuildStateMachine::prepare` seals each hop, derives the
  layer keys, and stores the `HopCryptoContext` for the (future)
  reply processor.
- `crates/i2pr-tunnel/src/responder.rs` — rewritten to the new
  cryptography surface. `DeterministicResponder` accepts a
  218-byte envelope through the production primitive, derives
  layer keys, and seals an accepted reply through
  `seal_short_reply`. `open_and_seal_accepted` provides a
  one-call helper.
- `crates/i2pr-tunnel/src/short_state.rs` — minor adjustment to
  surface the success-only registrar unchanged.
- `crates/i2pr-tunnel/src/conformance_fixtures.rs` (new) —
  independent reference Noise-N derivation and the canonical
  single-record conformance fixture. The fixture constructs the
  expected transcript hash and `replyKey/layerKey/ivKey` through
  the shared `i2pr-crypto` HKDF-SHA256 helper and asserts that
  the production primitive reaches the same state.
- `crates/i2pr-proto/src/common/mapping.rs` — `encoded_body_len`
  is now `pub` so callers outside `i2pr-proto` can validate
  Mapping bodies without going through the codec.

## Test counts

- `cargo test -p i2pr-tunnel --all-targets` — **74 passed**
  (single suite).
- `cargo test --workspace` — **519 passed** (34 suites).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` —
  **No issues found**.
- `cargo fmt --all --check` — **clean**.
- `bash scripts/check-dependency-direction.sh` — **ok**.
- `bash scripts/check-runtime-boundaries.sh` — **passed**.
- `bash scripts/check-fixture-manifest.sh` — **passed**.

## Dependency changes

No new crate dependencies were added. `Cargo.toml` deltas are
limited to:

- `crates/i2pr-proto/src/common/mapping.rs` — `Mapping::encoded_body_len`
  promoted from `pub(super)` to `pub`; this is the only public
  surface change in `i2pr-proto` and it does not affect any
  existing caller.
- `crates/i2pr-tunnel/Cargo.toml` — no change.
- `crates/i2pr-tunnel/src/conformance_fixtures.rs` — uses the
  `rand_core::RngCore` trait that was already declared in the
  crate's `Cargo.toml`; no new crate was added.

The forbidden `ECIES-X25519-Build-Session-v1` /
`ECIES-X25519-Request-Key` / `ECIES-X25519-Reply-Key` /
`ECIES-X25519-Request-Nonce` / `ECIES-X25519-Reply-Nonce` labels
and the Plan 108 `HopCryptoSeed`, `request_key_seed`, and
`reply_key_seed` parameters are no longer reachable through the
production public API.

## Conditional inbound creator-key issue

The Plan 109 §6.6 conditional inbound creator-key placement is
**not** triggered for the single-record conformance surface. The
single-record hop identity prefix is the truncated 16-byte router
hash, and the OBEP continuation derives the garlic material from
the post-`MixKey` chaining key without an additional inbound
creator-key offset. Plan 110 may need to revisit this detail if a
real reference I2P Java router places the creator key at a fixed
offset inside the inbound payload context; if it does, Plan 110
will record the spec note rather than guess.

## Acceptance criteria

| Work package | Item | Result |
| --- | --- | --- |
| A | 154-byte request plaintext | PASS |
| A | Role flag bytes `0x80` / `0x40` / `0x00` | PASS |
| A | Layer encryption type byte `0` (AES) | PASS |
| A | Request-time minute conversion (`floor(unix_seconds / 60)`) | PASS |
| A | Mandatory 600-second expiration | PASS |
| A | Canonical two-byte `Mapping` at offset 56 | PASS |
| A | Plan 108 `0x05` layer encryption type rejected | PASS |
| A | Plan 108 `0xC0` role flag rejected | PASS |
| B | Noise-N initialization | PASS |
| B | Peer-static `MixHash` | PASS |
| B | Ephemeral `MixHash` | PASS |
| B | Asymmetric `es` `MixKey` | PASS |
| B | Request AEAD with nonce `0` and AD = `current h` | PASS |
| B | Post-AEAD ciphertext `MixHash` | PASS |
| C | 218-byte envelope `16 + 32 + 154 + 16` | PASS |
| C | Plan 108 `32 + 16 + 170` envelope rejected | PASS |
| D | `SMTunnelReplyKey` derivation | PASS |
| D | `SMTunnelLayerKey` derivation | PASS |
| D | Non-OBEP IV derivation | PASS |
| D | OBEP `TunnelLayerIVKey` + `RGarlicKeyAndTag` continuation | PASS |
| E | 202-byte reply plaintext with `Mapping` + response byte | PASS |
| E | Accept `0` and `BandwidthRejected (30)` only | PASS |
| F | 218-byte hop-own reply with no ephemeral/nonce prefix | PASS |
| F | Slot nonce in `0..=7` enforced | PASS |
| F | Wrong slot/key/h fails authentication | PASS |
| G | `HopCryptoContext` retains saved `h`, derived `LayerKeys`, ephemeral pub | PASS |
| H | Independent reference Noise-N derivation | PASS |
| H | Production primitive matches fixture transcript + keys | PASS |
| I | All request/reply/envelope/key fixtures pass | PASS |
| I | Negative mutation tests fail closed | PASS |

## Documentation propagation

- `README.md` — the Milestone 5 status block now describes the
  Plan 109 single-record Noise-N conformance closure and the
  Plan 110 multi-record scope.
- `AGENTS.md` — the Plan 108/109/110 blocks, the Plan 102
  sequence, and the authoritative-state tokens are updated to
  reflect the Plan 109 closure. The Plan 109 focused-checks
  list is updated to the locally-confirmable workspace surface.
- `docs/architecture/i2pr-tunnel.md` — rewritten to describe the
  Plan 109 surface, the module layout, the key contracts, the
  Plan 109 acceptance criteria, and the Plan 110+ scope.
- `docs/architecture/overview.md` — the architecture overview now
  describes the Plan 109 closure and points to the closure
  record and the Plan 110 successor.
- `specs/support.toml` — `tunnel.ecies-x25519-short-build-crypto`
  is now `conformant = true` with the Plan 109 evidence list;
  `tunnel.short-build-conformance-fixture` is added as a new
  surface; `tunnel.multirecord-short-build` is added as the
  Plan 110 deferred scope.

## Handoff to Plan 110

Plan 110 may now build on:

- the canonical `ShortRequestRecord` / `ShortReplyRecord` codecs
  in `crates/i2pr-tunnel/src/short_record.rs`;
- the canonical `EciesX25519BuildCryptography` Noise-N request /
  reply primitives in `crates/i2pr-tunnel/src/build_crypto.rs`;
- the canonical `HopCryptoContext` retained by
  `ShortBuildStateMachine` per hop (saved post-request `h`,
  derived `LayerKeys`, sender ephemeral public key);
- the `ValidatedRecordSlot` surface (the only Plan 110 input
  that was previously undefined);
- the `conformance_fixtures::ReferenceFixture` shape (Plan 110
  should add a multi-record fixture, not replace the existing
  one).

Plan 110 does **not** inherit any Plan 108 wire/cryptographic
claim. The Plan 108 implementation surface remains as the
historical implementation record in `plans/108-status.md` and
must not be used as a compatibility reference.

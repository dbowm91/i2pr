# `i2pr-crypto` — Deep Dive

Protocol-specific cryptographic wrappers around identity-key operations:
Ed25519 signing, X25519 Diffie-Hellman, SHA-256, and the protocol-neutral
HKDF-SHA256 helper. Bridges `i2pr-proto` wire types with concrete
secret-key operations consumed by `i2pr-storage` (and indirectly by the
NTCP2 transport), by the Milestone 5 ECIES-X25519 tunnel-build
construction core, and by the Milestone 6 ECIES-X25519-AEAD-Ratchet
destination session layer.

Path: `crates/i2pr-crypto/`

## Purpose

Scope is intentionally narrow:

- Router-identity lifecycle: Ed25519 signing key generation and use,
  X25519 encryption key generation and use, identity bundle assembly,
  RouterInfo signing, and standalone signature/hash verification.
- Secret memory hygiene: zeroize-on-drop wrappers, no `Debug`,
  no `Clone`, no accidental formatting of secret bytes.
- HKDF-SHA256 helper the Milestone 5 ECIES-X25519 tunnel-build
  cryptography primitive consumes (Plan 108 implementation-landed,
  protocol-conformance reopened; the helper remains useful but
  the Plan 108 derivation labels are superseded — see
  [`plans/108-conformance-amendment.md`](../../plans/108-conformance-amendment.md)).
- ECIES-X25519-AEAD-Ratchet destination session primitives
  (Plan 121, `src/ecies.rs`): ephemeral key generation,
  RFC 9380 representative <-> Montgomery u-coordinate codec,
  `EciesNoiseState` (HKDF-SHA256 `mix_hash` / `mix_key`
  transcript), `EciesSessionState` (per-direction tag ratchet
  with a single tag key), and the
  `seal_new_session` / `open_new_session` /
  `seal_new_session_reply` / `open_new_session_reply` /
  `seal_existing_session` / `open_existing_session` codec
  surface. The wrapper hides `curve25519-elligator2` so
  `i2pr-client` never sees the third-party type. The Elligator2
  inverse rejects the all-zero value, rejects low-order points,
  and refuses to validate any 32-byte string whose
  representative does not encode a valid Curve25519 point.

The crate does **not** include:

- ChaCha20-Poly1305 / AES-CBC / HMAC / SipHash / Noise — those live in
  `i2pr-transport-ntcp2` (the tunnel-build path composes
  ChaCha20-Poly1305 from the same `chacha20poly1305` crate directly).
  The ECIES destination session layer composes ChaCha20-Poly1305
  directly from the `chacha20poly1305` crate too, but its HKDF
  transcript is the separate `EciesNoiseState` (not Noise-N).
- TLS, or any key-exchange state machine.
- TLS-style session state.

## Module layout

The crate is laid out across `src/lib.rs`, `src/hkdf.rs`, and `src/ecies.rs`:

| Section | File | Responsibility | Public types |
| --- | --- | --- | --- |
| Constants | `lib.rs` | Algorithm IDs, lengths | `ROUTER_SIGNING_KEY_TYPE`, `ROUTER_CRYPTO_KEY_TYPE`, `PRIVATE_KEY_LENGTH`, `SIGNATURE_LENGTH`, `IDENTITY_PADDING_LENGTH`, `X25519_KEY_LENGTH` |
| Errors | `lib.rs` | Typed crypto failure modes | `CryptoError`, `HkdfError`, `EciesError` |
| HKDF helper | `hkdf.rs` | RFC 5869 HKDF-SHA256 extract-and-expand, single-shot and 32-byte wrappers | `MAX_HKDF_OUTPUT_LEN`, `hkdf_sha256_extract_and_expand`, `hkdf_sha256_32` |
| X25519 private key | `lib.rs` | Static-key generation, DH | `X25519PrivateKey` |
| X25519 shared secret | `lib.rs` | Zeroizing DH result | `X25519SharedSecret` |
| Transport alias | `lib.rs` | Semantic alias for storage persistence | `TransportStaticKey` (= `X25519PrivateKey`) |
| Ed25519 signing key | `lib.rs` | Ed25519 seed + sign | `SigningPrivateKey` |
| X25519 encryption key | `lib.rs` | Identity encryption seed | `EncryptionPrivateKey` |
| Identity bundle | `lib.rs` | Full router identity generation + RouterInfo signing | `RouterIdentityBundle` |
| Identity builder | `lib.rs` | Private | _(private)_ |
| Signature verification | `lib.rs` | Ed25519 + RouterInfo verification | `verify_signature`, `verify_router_info` |
| Hash helpers | `lib.rs` | SHA-256, identity hash | `sha256`, `router_identity_hash` |
| Constant-time compare | `lib.rs` | `subtle`-backed | `constant_time_eq` |
| ECIES primitives | `ecies.rs` | Ephemeral keypair, representative codec, HKDF transcript, session ratchet, NS/NSR/ES message codecs | `EciesEphemeralKeypair`, `EciesEphemeralRepresentative`, `EciesEphemeralSecret`, `EciesNoiseState`, `EciesSessionState`, `NewSessionMessage`, `NewSessionReplyMessage`, `ExistingSessionMessage`, `seal_new_session`, `open_new_session`, `seal_new_session_reply`, `open_new_session_reply`, `seal_existing_session`, `open_existing_session`, `decode_representative` |
| Tests | all files | Deterministic primitives tests | _(private)_ |

## Public surface (`src/lib.rs`, `src/hkdf.rs`, `src/ecies.rs`)

| Item | Kind | Source |
| --- | --- | --- |
| `OsRng` | Re-export (`pub use rand_core::OsRng`) | `lib.rs` |
| `ROUTER_SIGNING_KEY_TYPE` | const `SigningKeyType` | `lib.rs` |
| `ROUTER_CRYPTO_KEY_TYPE` | const `CryptoKeyType` | `lib.rs` |
| `PRIVATE_KEY_LENGTH` | const `usize` (= 32) | `lib.rs` |
| `SIGNATURE_LENGTH` | const `usize` (= 64) | `lib.rs` |
| `IDENTITY_PADDING_LENGTH` | const `usize` (= 320) | `lib.rs` |
| `X25519_KEY_LENGTH` | const `usize` (= 32) | `lib.rs` |
| `CryptoError` | enum (6 variants) | `lib.rs` |
| `X25519PrivateKey` | struct | `lib.rs` |
| `X25519SharedSecret` | struct | `lib.rs` |
| `TransportStaticKey` | type alias for `X25519PrivateKey` | `lib.rs` |
| `SigningPrivateKey` | struct | `lib.rs` |
| `EncryptionPrivateKey` | struct | `lib.rs` |
| `RouterIdentityBundle` | struct | `lib.rs` |
| `verify_signature` | fn | `lib.rs` |
| `verify_router_info` | fn | `lib.rs` |
| `sha256` | fn | `lib.rs` |
| `router_identity_hash` | fn | `lib.rs` |
| `constant_time_eq` | fn | `lib.rs` |
| `HkdfError` | enum | `hkdf.rs` |
| `MAX_HKDF_OUTPUT_LEN` | const `usize` (= 8160) | `hkdf.rs` |
| `hkdf_sha256_extract_and_expand` | fn | `hkdf.rs` |
| `hkdf_sha256_32` | fn | `hkdf.rs` |
| `EciesError` | enum | `ecies.rs` |
| `EciesEphemeralKeypair` | struct | `ecies.rs` |
| `EciesEphemeralRepresentative` | type alias for `[u8; 32]` | `ecies.rs` |
| `EciesEphemeralSecret` | zeroizing wrapper | `ecies.rs` |
| `EciesNoiseState` | struct (HKDF transcript) | `ecies.rs` |
| `EciesSessionState` | struct (tag ratchet) | `ecies.rs` |
| `NewSessionMessage`, `NewSessionReplyMessage`, `ExistingSessionMessage` | structs | `ecies.rs` |
| `seal_new_session`, `open_new_session`, `seal_new_session_reply`, `open_new_session_reply`, `seal_existing_session`, `open_existing_session` | fns | `ecies.rs` |
| `decode_representative` | fn | `ecies.rs` |

## Key data structures

### `X25519PrivateKey` (line 82)
- Wraps `[u8; 32]`, `Zeroize` on drop.
- No `Debug`, no `Clone`.
- `from_bytes` (const), `generate` (RNG), `secret_bytes` (borrow),
  `public_bytes`, `diffie_hellman` (rejects all-zero output).

### `X25519SharedSecret` (line 133)
- `Zeroize`, no `Debug`/`Clone`. `from_bytes`, `as_bytes`.

### `SigningPrivateKey` (line 157)
- Ed25519 seed, `Zeroize` on drop, no `Debug`/`Display`/`Clone`/serde.
- `from_bytes`, `secret_bytes`, `public_key`, `sign`.

### `EncryptionPrivateKey` (line 195)
- X25519 seed, same zeroize and non-display semantics.
- `from_bytes`, `secret_bytes`, `public_key`. **No DH method** —
  identity encryption key cannot be used for transport DH.

### `RouterIdentityBundle` (line 222)
- Owns the two zeroizing key wrappers plus a `RouterIdentity`.
- No `Debug` — private material never leaks through formatting.
- Constructors: `generate` (RNG), `from_private_bytes` (raw arrays),
  `from_zeroizing_bytes` (consumes `Zeroizing` temps).
- `sign_router_info` performs the two-pass signing pattern: builds
  unsigned RouterInfo, signs the retained `signed_bytes`, rebuilds
  with the real signature.

### `TransportStaticKey` (line 148)
- `pub type TransportStaticKey = X25519PrivateKey;` — semantic alias
  consumed by `i2pr-storage` for NTCP2 static-key persistence.

### `EciesEphemeralKeypair`, `EciesEphemeralSecret`, `EciesEphemeralRepresentative`
- `EciesEphemeralKeypair::generate` produces a `(secret, public_key)`
  pair via `X25519StaticSecret::new(OsRng)` and the corresponding
  Montgomery `PublicKey`. The secret owns a zeroizing 32-byte buffer
  with no `Debug` / `Clone`. The 32-byte representative is the RFC 9380
  even-half of the curve-point mapping; the inverse call
  (`decode_representative`) runs `from_representative` then
  `to_montgomery` and asserts `X25519(clamp(seed), G)` matches the
  recovered u-coordinate (Plan 121 §3 deterministic invariant).

### `EciesNoiseState`
- 32-byte chaining key + 32-byte AEAD key, HKDF-SHA256 transcript
  (`mix_hash`, `mix_key`). The `mix1` operation combines the chained
  key with an ECDH shared secret to derive `(next_ck, aead_key)`.
  Used as the per-handshake transcript; `EciesSessionState` carries
  the chaining key after the New Session Reply completes.

### `EciesSessionState`
- 8-byte tag ratchet: `tag_key = HKDF(chaining_key, b"TagKey", b"", 32)`,
  `tag = HKDF(tag_key, b"", b"SessionTag", 8)` per consumed message.
  Inbound look-ahead accepts any tag within `MAX_TAG_LOOK_AHEAD`
  (= 32) of the next-expected tag and ratchets the next-expected
  forward through the consumed tag. Single tag key per side (no
  separate Send/Recv labels) so both sides derive the same tag
  sequence.
- `install(chaining_key)` and `install_with_kind(...)` are called
  once the New Session Reply has authenticated. `seal_existing_session`
  and `open_existing_session` advance the ratchet exactly once per
  call. The session state does not implement `Debug` /
  `Display` and the chaining key bytes never leave the API.
- Tag-chain reuse (replay of a previously consumed Existing Session)
  fails closed without advancing the ratchet.

### `NewSessionMessage` / `NewSessionReplyMessage` / `ExistingSessionMessage`
- Bounded typed carriers: `flag` (single byte; `0xE0` for New Session,
  `0xE2` for New Session Reply and Existing Session), fixed
  payload lengths (ephemeral pub 32, static pub 32, ciphertext with
  explicit `+ AEAD_TAG_LEN`), and the `decode(input, maximum)` entry
  point enforces the minimum length and maximum length and rejects
  wrong-flag inputs with `EciesError::AuthenticationFailed`.
- The decode functions refuse inputs shorter than
  `1 + ECIES_SESSION_TAG_LEN + AEAD_TAG_LEN` so that zero-ciphertext
  or stub messages cannot pass the size gate.

## Secret ownership rules

- Private key bytes never exposed via `Display`, `Debug`, or serde.
- All secret wrappers use `#[zeroize(drop)]`.
- `Randomness` is injected via `rand_core::TryCryptoRng` — the crate
  never reads system RNG directly (the `OsRng` re-export is for
  callers).
- `RouterIdentityBundle::from_zeroizing_bytes` consumes
  `Zeroizing<[u8; 32]>` owners, copying the array into the wrapper
  and dropping/wiping the temp.

## Dependencies

`Cargo.toml`:

| Dependency | Purpose |
| --- | --- |
| `ed25519-dalek` | Ed25519 signing/verification |
| `hmac` | HKDF-SHA256 HMAC primitive |
| `i2pr-proto` | Wire types (`PublicKey`, `SigningPublicKey`, `SignatureValue`, `RouterIdentity`, `RouterInfo`, `Hash`) |
| `rand_core` (+ `os_rng` feature) | RNG trait + `OsRng` re-export |
| `sha2` | SHA-256 helpers and HKDF SHA-256 backing |
| `subtle` | `ConstantTimeEq` |
| `thiserror` | `CryptoError` / `EciesError` derive |
| `x25519-dalek` | X25519 DH |
| `zeroize` | Memory wiping |
| `chacha20poly1305` | ECIES session AEAD seam (Plan 121) |
| `curve25519-elligator2` (= 0.1.0-alpha.2) | RFC 9380 even-half representative codec (Plan 121 §2/§12 audited dependency) |
| `rand_chacha` (dev) | Deterministic test RNGs |

Dependency chain is satisfied: `i2pr-proto ← i2pr-crypto ← i2pr-storage`.

## Forbidden nonce note

The forbidden nonce `2^64 - 1` lives in `i2pr-transport-ntcp2`, not
here — that is a transport-layer concern. `i2pr-crypto` is not in scope
for nonce policy.

## Tests

Inline at `src/lib.rs:404-507`:

- `deterministic_generation_is_reproducible_only_with_injected_rng`
  — RNG injection discipline.
- `signature_vectors_reject_message_signature_and_key_mutations`
  — sign/verify + mutation negatives.
- `hash_and_constant_time_helpers_are_stable` — `sha256` and
  `constant_time_eq` edges.
- `x25519_rejects_an_all_zero_shared_secret` — DH all-zero rejection.
- `router_info_signing_uses_retained_signed_bytes` —
  full RouterInfo sign-encode-decode-verify round-trip.

There is no per-crate `tests/` directory or fixture directory owned by
`i2pr-crypto` itself. Downstream tests reach the crypto APIs via
`tests/fixtures/ntcp2/crypto/` (NTCP2 vectors) and via
`tests/fixtures/ntcp2/crypto/storage-static-key.hex` (consumed by
`i2pr-storage` round-trip tests).

The Plan 121 ECIES primitives add inline deterministic tests at
`src/ecies.rs` covering: ephemeral keypair round-trip (recovered
representative u-coordinate equals `X25519(clamp(seed), G)`),
`seal_new_session` ↔ `open_new_session` round-trip with payload
authentication, `seal_new_session_reply` ↔ `open_new_session_reply`
follow-on installation of the bounded session state,
`seal_existing_session` ↔ `open_existing_session` ratchet advance,
wrong-flag / wrong-tag / wrong-recipient rejection,
truncated-payload rejection, and a random-representative sweep
that proves the inverse never panics on adversarial 32-byte
inputs. The `i2pr-client` Plan 121 trajectory test
(`crates/i2pr-client/tests/plan121_trajectory.rs`) drives the
full NS → NSR → bidirectional ES path.

## Distinctive design choices

1. **Two-file, narrow scope.** `i2pr-crypto` keeps identity
   cryptography and the protocol-neutral HKDF helper in the same
   crate so the Milestone 5 ECIES-X25519 tunnel-build primitive can
   consume both directly. AES, ChaCha20-Poly1305, HMAC, and SipHash
   live in `i2pr-transport-ntcp2` (and ChaCha20-Poly1305 also in
   `i2pr-tunnel` for the Plan 108 short-build cryptography; the
   Plan 108 derivation is non-normative and is superseded by
   Plan 109).
2. **Dual X25519 wrappers.** `X25519PrivateKey` is for transport static
   keys (has DH); `EncryptionPrivateKey` is for identity encryption
   (no DH).
3. **`TransportStaticKey` type alias** — keeps `i2pr-storage`
   independent of NTCP2 protocol details.
4. **Two-pass RouterInfo signing** — required because the signed region
   depends on the signature field's presence in the encoded form.
5. **HKDF-SHA256 helper** — RFC 5869 extract-and-expand with bounded
   output length (`255 * 32 = 8160` bytes) and `Zeroizing` return
   buffers. Shared by `i2pr-tunnel` and other future protocol-neutral
   consumers; not tied to any specific protocol context label.
6. **Zeroize discipline is rigorous** — every secret wrapper, plus
   `Zeroizing` intermediates in `generate`.
7. **ECIES Elligator2 wrapper hides the third-party type** —
   `i2pr-crypto` exposes only the typed ECIES API; the
   `curve25519-elligator2` primitive is an internal implementation
   detail and is never reachable from `i2pr-client` or `i2pr-proto`.
   This keeps the Plan 121 dependency audit (`Cargo.toml` lock) and
   the wrapper's invariants in a single bounded module.
7. **No `unsafe`** — `#![forbid(unsafe_code)]`.

## Cross-references

- [Overview](overview.md)
- [i2pr-storage](i2pr-storage.md) — primary consumer
- [i2pr-transport-ntcp2](i2pr-transport-ntcp2.md) — reuses
  `X25519PrivateKey` via `TransportStaticKey` for handshake state
- [i2pr-tunnel](i2pr-tunnel.md) — consumes `X25519PrivateKey`,
  `X25519SharedSecret`, `hkdf_sha256_32`, and `sha256` in the Plan 108
  ECIES-X25519 short tunnel-build cryptography primitive (Plan 108
  implementation-landed, protocol-conformance reopened by
  [`plans/108-conformance-amendment.md`](../../plans/108-conformance-amendment.md))
- [i2pr-client](i2pr-client.md) — consumes `EciesEphemeralKeypair`,
  `EciesSessionState`, `EciesNoiseState`, and the
  `seal_new_session` / `seal_existing_session` family in the Plan 121
  ECIES destination session manager
  (`crates/i2pr-client/src/session.rs`).
- Plan-of-record: `plans/013-m1-identity-crypto-storage.md` and
  [`plans/121-m6-ecies-garlic-session-layer.md`](../../plans/121-m6-ecies-garlic-session-layer.md)

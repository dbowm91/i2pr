# `i2pr-crypto` — Deep Dive

Protocol-specific cryptographic wrappers around identity-key operations:
Ed25519 signing, X25519 Diffie-Hellman, SHA-256. Bridges `i2pr-proto`
wire types with concrete secret-key operations consumed by `i2pr-storage`
(and indirectly by the NTCP2 transport).

Path: `crates/i2pr-crypto/`

## Purpose

Scope is intentionally narrow:

- Router-identity lifecycle: Ed25519 signing key generation and use,
  X25519 encryption key generation and use, identity bundle assembly,
  RouterInfo signing, and standalone signature/hash verification.
- Secret memory hygiene: zeroize-on-drop wrappers, no `Debug`,
  no `Clone`, no accidental formatting of secret bytes.

The crate does **not** include:

- ChaCha20-Poly1305 / AES-CBC / HMAC / SipHash / Noise — those live in
  `i2pr-transport-ntcp2`.
- TLS, KDFs for general use, or any key-exchange state machine.
- TLS-style session state.

## Module layout

The crate is a **single file** — there is no `src/` subdirectory beyond
`src/lib.rs` (507 lines), with logical sections:

| Section | Lines | Responsibility | Public types |
| --- | --- | --- | --- |
| Constants | 30-41 | Algorithm IDs, lengths | `ROUTER_SIGNING_KEY_TYPE`, `ROUTER_CRYPTO_KEY_TYPE`, `PRIVATE_KEY_LENGTH`, `SIGNATURE_LENGTH`, `IDENTITY_PADDING_LENGTH`, `X25519_KEY_LENGTH` |
| Errors | 44-72 | Typed crypto failure modes | `CryptoError` |
| X25519 private key | 80-128 | Static-key generation, DH | `X25519PrivateKey` |
| X25519 shared secret | 131-145 | Zeroizing DH result | `X25519SharedSecret` |
| Transport alias | 148 | Semantic alias for storage persistence | `TransportStaticKey` (= `X25519PrivateKey`) |
| Ed25519 signing key | 155-187 | Ed25519 seed + sign | `SigningPrivateKey` |
| X25519 encryption key | 193-215 | Identity encryption seed | `EncryptionPrivateKey` |
| Identity bundle | 222-316 | Full router identity generation + RouterInfo signing | `RouterIdentityBundle` |
| Identity builder | 318-335 | Private | _(private)_ |
| Signature verification | 338-384 | Ed25519 + RouterInfo verification | `verify_signature`, `verify_router_info` |
| Hash helpers | 387-396 | SHA-256, identity hash | `sha256`, `router_identity_hash` |
| Constant-time compare | 399-401 | `subtle`-backed | `constant_time_eq` |
| Tests | 404-507 | Five unit tests | _(private)_ |

## Public surface (`src/lib.rs`)

| Item | Kind | Line |
| --- | --- | --- |
| `OsRng` | Re-export (`pub use rand_core::OsRng`) | 27 |
| `ROUTER_SIGNING_KEY_TYPE` | const `SigningKeyType` | 30 |
| `ROUTER_CRYPTO_KEY_TYPE` | const `CryptoKeyType` | 32 |
| `PRIVATE_KEY_LENGTH` | const `usize` (= 32) | 34 |
| `SIGNATURE_LENGTH` | const `usize` (= 64) | 36 |
| `IDENTITY_PADDING_LENGTH` | const `usize` (= 320) | 38 |
| `X25519_KEY_LENGTH` | const `usize` (= 32) | 41 |
| `CryptoError` | enum (6 variants) | 44 |
| `X25519PrivateKey` | struct | 82 |
| `X25519SharedSecret` | struct | 133 |
| `TransportStaticKey` | type alias for `X25519PrivateKey` | 148 |
| `SigningPrivateKey` | struct | 157 |
| `EncryptionPrivateKey` | struct | 195 |
| `RouterIdentityBundle` | struct | 222 |
| `verify_signature` | fn | 338 |
| `verify_router_info` | fn | 378 |
| `sha256` | fn | 387 |
| `router_identity_hash` | fn | 394 |
| `constant_time_eq` | fn | 399 |

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
| `i2pr-proto` | Wire types (`PublicKey`, `SigningPublicKey`, `SignatureValue`, `RouterIdentity`, `RouterInfo`, `Hash`) |
| `rand_core` (+ `os_rng` feature) | RNG trait + `OsRng` re-export |
| `sha2` | SHA-256 helpers |
| `subtle` | `ConstantTimeEq` |
| `thiserror` | `CryptoError` derive |
| `x25519-dalek` | X25519 DH |
| `zeroize` | Memory wiping |
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

## Distinctive design choices

1. **Single-file, narrow scope.** Despite AGENTS.md listing submodules
   (`ed25519`, `x25519`, `aes`, `chacha20poly1305`, `hmac`, `siphash`),
   the crate is a flat 507-line `lib.rs`. AES, ChaCha20-Poly1305, HMAC,
   and SipHash live in `i2pr-transport-ntcp2` instead.
2. **Dual X25519 wrappers.** `X25519PrivateKey` is for transport static
   keys (has DH); `EncryptionPrivateKey` is for identity encryption
   (no DH).
3. **`TransportStaticKey` type alias** — keeps `i2pr-storage`
   independent of NTCP2 protocol details.
4. **Two-pass RouterInfo signing** — required because the signed region
   depends on the signature field's presence in the encoded form.
5. **Zeroize discipline is rigorous** — every secret wrapper, plus
   `Zeroizing` intermediates in `generate`.
6. **No `unsafe`** — `#![forbid(unsafe_code)]`.

## Cross-references

- [Overview](overview.md)
- [i2pr-storage](i2pr-storage.md) — primary consumer
- [i2pr-transport-ntcp2](i2pr-transport-ntcp2.md) — reuses
  `X25519PrivateKey` via `TransportStaticKey` for handshake state
- Plan-of-record: `plans/013-m1-identity-crypto-storage.md`

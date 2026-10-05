# `i2pr-crypto` — Deep Dive

Protocol-specific cryptographic wrappers for `i2pr`: router-identity key
operations, the protocol-neutral HKDF-SHA256 helper, the ECIES-X25519-AEAD-
Ratchet destination session primitives, the raw ChaCha20 layer primitive, and
the independent I2P Red25519 (signature type 11) composition.

Path: `crates/i2pr-crypto/`

#![forbid(unsafe_code)] — no local field, scalar, point, bignum, or cipher
primitive is implemented here; every primitive is a reviewed dependency.

## Purpose

What the crate owns:

- **Router-identity cryptography.** Ed25519 (type 7) signing and X25519 (type 4)
  encryption key generation and use, identity bundle assembly, RouterInfo
  signing, and standalone signature/hash verification. Selected in
  [ADR 0004](../../docs/adr/0004-router-identity-algorithms.md) (Plan 013).
- **Signature verification over protocol records.** `verify_router_info`,
  `verify_lease_set2`, and `verify_meta_lease_set` verify the exact retained
  signed byte regions of the owning `i2pr-proto` structures, including the
  offline-delegation form.
- **A bounded NetDB supplied-key ECIES reply primitive**
  (`seal_netdb_ecies_reply` / `open_netdb_ecies_reply`) — one narrow
  ChaCha20-Poly1305 seam, deliberately not an ECIES session.
- **HKDF-SHA256** (RFC 5869 extract-and-expand) as a protocol-neutral helper.
- **The ECIES-X25519-AEAD-Ratchet destination session layer** (`ecies.rs`,
  Plan 126): ephemeral key generation, the RFC 9380 representative
  ⇄ Montgomery u-coordinate codec, the HKDF-SHA256 `EciesNoiseState`
  transcript, directional `EciesTagSet` ratchets, and the bound-session
  NS/NSR/ES message codecs. Wire contract and vector provenance:
  [`specs/references/ecies-destination-ratchet.md`](../../specs/references/ecies-destination-ratchet.md).
- **The raw RFC 8439 ChaCha20 layer primitive** (`chacha.rs`, Plan 332) for
  Encrypted LeaseSet2 (DatabaseStore type 5) layer encryption.
- **The independent Red25519 composition** (`red25519.rs`, Plan 330): the
  specification's own `I2P_Red25519H(x)` domain separation, `HStar` transcript
  hash, daily `GENERATE_ALPHA` derivation, additive key re-randomization,
  randomized signing, and cofactor-aware verification, composed over
  `curve25519-dalek`. Worksheet and freeze:
  [`red25519-algorithm-worksheet.md`](../../specs/references/red25519-algorithm-worksheet.md),
  [`red25519-clean-room-freeze.md`](../../specs/references/red25519-clean-room-freeze.md).
- **Secret memory hygiene.** Zeroize-on-drop owners; no `Debug`, no `Display`,
  no `Clone`, and no serde on any secret type.

What the crate must **not** own:

- **Local cryptographic primitives.** No field, scalar, point, bignum, or
  symmetric-cipher math; see [ADR 0005](../../docs/adr/0005-crypto-dependency-selection.md).
- **Noisy transport/session ciphers.** AES and SipHash live in
  `i2pr-transport-ntcp2`; AES and ChaCha20-Poly1305 also live in
  `i2pr-tunnel` for the Plan 108 short-build seam. This crate owns *two*
  ChaCha-family items, each for a specific reason:
  `chacha20poly1305` for the two AEAD seams it owns (ECIES session payloads and
  the NetDB supplied-key reply), and raw `chacha20` in `chacha.rs` because the
  encrypted LeaseSet2 layer stream is deliberately **not** an AEAD (see
  "Distinctive design choices"). The NTCP2 data phase still composes its own
  ChaCha20-Poly1305 from the same workspace crate; that is transport
  scope, not a reason to remove the two AEAD seams here.
- **Session lifecycle, replay caches, tag-window bookkeeping, replay defense,
  and per-destination policy** — those live in `i2pr-client::session`. The
  seal/open functions here are pure codecs over raw key slices.
- **Key exchange state machines, TLS, and TLS-style session state.**
- **Nonce policy for the transport.** The forbidden `2^64 - 1` counter is a
  transport-layer concern and lives in
  `crates/i2pr-transport-ntcp2/src/crypto.rs:133`; this crate is not in scope
  for it.
- **Destination lifecycle, b32/b33 addressing, LeaseSet framing, NetDB
  storage, and Proposal 170 control mapping** for the Red25519 lane — each
  belongs to its own owner.

## Module layout

Line counts are `wc -l` against current source.

| Module | File | Lines | Responsibility | Key public types / items |
| --- | --- | --- | --- | --- |
| crate root | `lib.rs` | 725 | Identity key wrappers, identity bundle, RouterInfo/LS2/MLS2 verification, SHA-256 helpers, constant-time compare, bounded NetDB reply AEAD, algorithm constants | `CryptoError`, `X25519PrivateKey`, `X25519SharedSecret`, `TransportStaticKey`, `SigningPrivateKey`, `EncryptionPrivateKey`, `RouterIdentityBundle`, `verify_signature`, `verify_router_info`, `verify_lease_set2`, `verify_meta_lease_set`, `sha256`, `router_identity_hash`, `constant_time_eq`, `seal_netdb_ecies_reply`, `open_netdb_ecies_reply` |
| `hkdf` | `hkdf.rs` | 188 | RFC 5869 HKDF-SHA256 extract-and-expand, one-shot and 32-byte wrappers, `Zeroizing` returns | `HkdfError`, `MAX_HKDF_OUTPUT_LEN`, `hkdf_sha256_extract_and_expand`, `hkdf_sha256_32` |
| `ecies` | `ecies.rs` | 2968 | ECIES-X25519-AEAD-Ratchet destination session layer: ephemeral keypair, Elligator2 representative codec, Noise transcript, directional tag-set ratchets, bound NS/NSR/ES codecs, 41 inline tests + frozen vector module | `EciesError`, `EciesEphemeralKeypair`, `EciesEphemeralRepresentative`, `EciesEphemeralSecret`, `EciesTagSet`, `EciesTagSetEntry`, `BoundNewSessionMessage`, `BoundNewSessionSender`, `NewSessionResponder`, `SealedNewSessionReply`, `OpenedNewSessionReply`, `OpenedBoundNewSession`, `NewSessionReplyMessage`, `ExistingSessionMessage`, `seal_bound_new_session`, `open_bound_new_session`, `seal_new_session_reply`, `open_new_session_reply`, `seal_existing_session`, `open_existing_session`, `decode_representative` |
| `chacha` | `chacha.rs` | 258 | Raw RFC 8439 keystream pinned to the I2P layer initial block counter 1, with a zeroizing key owner | `ChachaError`, `LayerCipherKey`, `chacha20_xor_layer`, `chacha20_xor_layer_owned` |
| `red25519` | `red25519.rs` | 758 | Independent I2P Red25519 (RedDSA, signature type 11) composition: domain-separated `HStar`, daily alpha derivation, blinding, randomized signing, cofactor-aware verification, blinded DHT storage key. **No inline unit tests** — all evidence lives in `tests/` | `Red25519Error`, `Red25519PrivateScalar`, `BlindingScalar`, `BlindedPrivateScalar`, `Red25519PublicKey`, `Red25519Signature`, `BlindingDay`, `LookupSecret`, `convert_ed25519_private`, `generate_private`, `derive_public_key`, `derive_blinded_public_key`, `generate_alpha`, `blind_public_key`, `blind_private_key`, `sign`, `sign_with_nonce`, `verify`, `verify_blinded`, `blinded_storage_key` |
| integration | `tests/*.rs` | 1999 | 7 differential / vector / adversarial binaries, plus `tests/data/*.json` fixtures | _(test-only)_ |

## Public surface

### Re-exports from `src/lib.rs`

Seam re-exports, so a downstream crypto owner can inject a CSPRNG and own
zeroizing buffers without taking a direct `rand_core` / `zeroize` dependency
(which keeps the dependency allowlist of the crates below this one unchanged):

| Item | Kind | Defined in |
| --- | --- | --- |
| `OsRng` | `pub use rand_core::OsRng` | `lib.rs` |
| `TryCryptoRng` | `pub use rand_core::TryCryptoRng` | `lib.rs` |
| `Zeroizing` | `pub use zeroize::Zeroizing` | `lib.rs` |
| `Zeroize` | `pub use zeroize::Zeroize` | `lib.rs` |

All four submodules are `pub mod` (`chacha`, `ecies`, `hkdf`, `red25519`), and
each re-exports its public items at the crate root:

- `pub use chacha::{CHACHA20_BLOCK_LENGTH, CHACHA20_KEY_LENGTH, CHACHA20_NONCE_LENGTH, ChachaError, LAYER_INITIAL_BLOCK_COUNTER, LayerCipherKey, chacha20_xor_layer, chacha20_xor_layer_owned};`
- `pub use ecies::{BOUND_NEW_SESSION_MIN_LENGTH, BoundNewSessionMessage, BoundNewSessionSender, ECIES_NOISE_PROTOCOL_NAME, EXISTING_SESSION_MIN_LENGTH, EciesEphemeralKeypair, EciesEphemeralRepresentative, EciesEphemeralSecret, EciesError, EciesTagSet, EciesTagSetEntry, ExistingSessionMessage, MAX_NEW_SESSION_CIPHERTEXT, NEW_SESSION_REPLY_MIN_LENGTH, NewSessionReplyMessage, NewSessionResponder, OpenedBoundNewSession, OpenedNewSessionReply, REPRESENTATIVE_LENGTH, SESSION_TAG_LENGTH, STATIC_PUBLIC_LENGTH, SealedNewSessionReply, decode_representative, open_bound_new_session, open_existing_session, open_new_session_reply, seal_bound_new_session, seal_existing_session, seal_new_session_reply};`
- `pub use hkdf::{HkdfError, MAX_HKDF_OUTPUT_LEN, hkdf_sha256_32, hkdf_sha256_extract_and_expand};`
- `pub use red25519::{ALPHA_HKDF_INFO, ALPHA_SALT_PERSONALIZATION, BLINDED_SIGNING_KEY_TYPE, BLINDING_DAY_LENGTH, BlindedPrivateScalar, BlindingDay, BlindingScalar, ED25519_SIGNING_KEY_TYPE, HASH_LENGTH, HSTAR_PREFIX, LookupSecret, MAX_LOOKUP_SECRET_LENGTH, MAX_MESSAGE_LENGTH, PRIVATE_SEED_LENGTH, PUBLIC_KEY_LENGTH, Red25519Error, Red25519PrivateScalar, Red25519PublicKey, Red25519Signature, SCALAR_LENGTH, SIGNING_NONCE_LENGTH, blind_private_key, blind_public_key, blinded_storage_key, convert_ed25519_private, derive_blinded_public_key, derive_public_key, generate_alpha, generate_private, sign, sign_with_nonce, verify, verify_blinded};`

### Own items declared in `lib.rs`

| Item | Kind | Notes |
| --- | --- | --- |
| `ROUTER_SIGNING_KEY_TYPE` | `const SigningKeyType` | `EdDsaSha512Ed25519` (type 7) |
| `ROUTER_CRYPTO_KEY_TYPE` | `const CryptoKeyType` | `X25519` (type 4) |
| `PRIVATE_KEY_LENGTH` | `const usize` = 32 | both generated algorithms |
| `SIGNATURE_LENGTH` | `const usize` = 64 | Ed25519 signature |
| `IDENTITY_PADDING_LENGTH` | `const usize` = `384 - 32 - 32` = 320 | key-certificate padding |
| `X25519_KEY_LENGTH` | `const usize` = 32 | transport/transport-adjacent key length |
| `CryptoError` | enum, 7 variants | `RandomnessUnavailable`, `UnsupportedAlgorithm { algorithm, context }`, `InvalidKey { context }`, `AllZeroSharedSecret`, `InvalidSignature`, `NetDbReplyAeadFailed`, `Protocol(CodecError)` |
| `X25519PrivateKey` | struct, `lib.rs:135` | zeroizing, non-`Debug`, non-`Clone` |
| `X25519SharedSecret` | struct, `lib.rs:186` | zeroizing, non-`Debug`, non-`Clone` |
| `TransportStaticKey` | `type` alias, `lib.rs:201` | `= X25519PrivateKey` |
| `SigningPrivateKey` | struct, `lib.rs:210` | Ed25519 seed, zeroizing |
| `EncryptionPrivateKey` | struct, `lib.rs:248` | X25519 seed, zeroizing, **no DH method** |
| `RouterIdentityBundle` | struct, `lib.rs:275` | no `Debug` at all |
| `verify_signature`, `verify_router_info`, `verify_lease_set2`, `verify_meta_lease_set` | fns | typed rejection of wrong key type / bad length / failed verification |
| `sha256`, `router_identity_hash`, `constant_time_eq` | fns | |
| `seal_netdb_ecies_reply`, `open_netdb_ecies_reply` | fns | narrow supplied-key ECIES reply |

### Module-path-only public items

`pub` in their defining module but deliberately **not** re-exported at the crate
root (name collisions, or internal detail). Reachable only as
`i2pr_crypto::<module>::<item>`:

- `ecies`: `AEAD_TAG_LEN` (16), `AEAD_NONCE_LEN` (12), `HKDF_OUTPUT_LEN` (32).
- `red25519`: `SIGNATURE_LENGTH` (64 — deliberately *not* re-exported, so it
  cannot be confused with the root Ed25519 constant of the same value),
  `MIN_BLINDING_YEAR` (1970), `MAX_BLINDING_YEAR` (9999).

`EciesNoiseState` is `pub(crate)` and **not** part of the public surface: the
seal/open helpers and `EciesTagSet` are the public entry points, and the raw
transcript state stays inside the bounded module. `MAX_TAG_SET_INDEX`
(65 535), `MAX_TAG_SET_RETAINED_KEYS` (4096), and
`ELLIGATOR_CANONICAL_THRESHOLD_LE` are private.

## Key contracts

### Secret ownership

- **No `Debug` / `Display` / `Clone` / serde on any secret type.** Redacted
  `Debug` is used only where a type is not a secret but a transcript is:
  `EciesEphemeralKeypair` prints `secret_seed: "<redacted>"`,
  `EciesEphemeralSecret` prints `seed: "<redacted>"`, and `LookupSecret` /
  `Red25519Signature` print only a length. `RouterIdentityBundle` has no
  `Debug` implementation at all.
- **Erase on drop.** `#[derive(Zeroize)] #[zeroize(drop)]` on
  `X25519PrivateKey`, `X25519SharedSecret`, `SigningPrivateKey`,
  `EncryptionPrivateKey`, `Red25519PrivateScalar`, `BlindingScalar`,
  `BlindedPrivateScalar`. `ZeroizeOnDrop` on `EciesEphemeralSecret` and
  `EciesNoiseState`; `LayerCipherKey` and `LookupSecret` hold a `Zeroizing`
  buffer instead of deriving the impl. `LayerCipherKey::zeroize_now` allows
  immediate erasure instead of waiting for the drop.
- **One accessor, explicitly named.** `secret_bytes` on the private wrappers,
  `as_bytes` on public/borrowed material. `RouterIdentityBundle::from_zeroizing_bytes`
  consumes `Zeroizing<[u8; 32]>` owners so temporary buffers wipe on drop;
  `from_private_bytes_with_padding` round-trips the persisted identity padding
  exactly so peer signature verification is stable across restarts.
- **Randomness is always injected** through `rand_core::TryCryptoRng`; the
  crate never reads the system RNG inside an operation. The `OsRng` re-export
  is for callers to pass in explicitly. A random-source failure is a distinct
  typed error (`CryptoError::RandomnessUnavailable`,
  `EciesError::RandomnessUnavailable`, `Red25519Error::RandomnessUnavailable`)
  so a caller that retries on randomness failure does not also retry on a
  protocol rejection.

### Constant-time comparison

`subtle` is the only comparison path for integrity-relevant values:
`constant_time_eq` (`lib.rs:546`) is a length check plus
`ConstantTimeEq::ct_eq`; `red25519.rs` compares the cofactored verification
candidate against the identity with `ct_eq` (all operands are public, and the
double-scalar multiply uses the variable-time path deliberately).

### Typed errors

| Error | Variants | Notes |
| --- | --- | --- |
| `CryptoError` | 7 | includes `Protocol(#[from] CodecError)` so no `i2pr-proto` codec result is swallowed |
| `HkdfError` | 2 | `OutputLengthExceeded { requested, maximum }`, `InvalidKeyLength`; converts into `CryptoError` |
| `EciesError` | 12 | `ElligatorDecode`, `AllZeroKey`, `InvalidSharedSecret`, `AuthenticationFailed`, `EncryptionFailed`, `CiphertextTooLarge`, `CiphertextTooShort`, `UnboundNewSessionNotSupported`, `TagSetExhausted`, `TagSetIndexBeyondCeiling`, `Hkdf(HkdfError)`, `RandomnessUnavailable` |
| `ChachaError` | 1 | `InvalidLength { component, actual, expected }` |
| `Red25519Error` | 15 | `RandomnessUnavailable`, `MessageTooLong`, `InvalidPublicKey`, `SmallOrderPoint`, `NotPrimeOrderPoint`, `NonCanonicalScalar`, `NonCanonicalConvertedScalar`, `InvalidSignatureLength`, `SignatureVerificationFailed`, `BlindedSignatureVerificationFailed`, `UnsupportedSigType`, `InvalidBlindingDay`, `LookupSecretTooLong`, `ProtocolKeyRejected`, `KeyDerivationFailed` |

### Bound constants

| Constant | Value | Meaning |
| --- | --- | --- |
| `PRIVATE_KEY_LENGTH` / `X25519_KEY_LENGTH` / `SCALAR_LENGTH` / `PUBLIC_KEY_LENGTH` / `HASH_LENGTH` | 32 | key and digest sizes |
| `SIGNATURE_LENGTH` (root and `red25519`) | 64 | `R ‖ S` |
| `PRIVATE_SEED_LENGTH` | 32 | aliases `crate::PRIVATE_KEY_LENGTH` |
| `IDENTITY_PADDING_LENGTH` | 320 | `384 - 2 × 32` |
| `MAX_HKDF_OUTPUT_LEN` | 8160 | `255 × 32`; `L > 255 × HashLen` is rejected |
| `REPRESENTATIVE_LENGTH` / `STATIC_PUBLIC_LENGTH` | 32 | ECIES Elligator2 representative; type-4 static public key |
| `SESSION_TAG_LENGTH` | 8 | ECIES session tag |
| `AEAD_TAG_LEN` / `AEAD_NONCE_LEN` (ecies) | 16 / 12 | ChaCha20-Poly1305 |
| `HKDF_OUTPUT_LEN` (ecies) | 32 | HKDF chunk size |
| `BOUND_NEW_SESSION_MIN_LENGTH` | 96 | `32 + 32 + 16 + 16` |
| `NEW_SESSION_REPLY_MIN_LENGTH` | 72 | `8 + 32 + 16 + 16` |
| `EXISTING_SESSION_MIN_LENGTH` | 24 | `8 + 16` |
| `MAX_NEW_SESSION_CIPHERTEXT` | 65 507 | bounded to one I2NP body so `u16` lengths cannot overflow |
| `CHACHA20_KEY_LENGTH` / `CHACHA20_NONCE_LENGTH` / `CHACHA20_BLOCK_LENGTH` | 32 / 12 / 64 | RFC 8439 variant |
| `LAYER_INITIAL_BLOCK_COUNTER` | 1 | pinned I2P layer counter; callers cannot select it |
| `BLINDING_DAY_LENGTH` | 8 | UTF-8 `YYYYMMDD` |
| `SIGNING_NONCE_LENGTH` | 80 | random bytes mixed into the Red25519 nonce |
| `HSTAR_PREFIX` | `I2P_Red25519H(x)` | Red25519 domain literal |
| `ALPHA_HKDF_INFO` | `i2pblinding1` | daily alpha HKDF `info` |
| `ALPHA_SALT_PERSONALIZATION` | `I2PGenerateAlpha` | alpha HKDF salt personalization |
| `MAX_MESSAGE_LENGTH` | 65 534 | largest message the 2-byte `HStar` length prefix can encode |
| `MIN_BLINDING_YEAR` / `MAX_BLINDING_YEAR` | 1970 / 9999 | `BlindingDay` calendar bounds |
| `MAX_LOOKUP_SECRET_LENGTH` | 256 | i2pr resource bound, not a protocol limit |

### ECIES-specific invariants

- The Noise initializer is the literal I2P contract
  `ECIES_NOISE_PROTOCOL_NAME` = `Noise_IKelg2+hs2_25519_ChaChaPoly_SHA256`.
- Wire layouts carry **no flag bytes**: NS
  `elg2_aepk(32) || static_section_ct(48) || payload_ct(len+16)`, NSR
  `tag(8) || elg2_bepk(32) || zero-len key-section MAC(16) || payload_ct(len+16)`,
  ES `tag(8) || payload_ct(len+16)`. `decode` enforces the structural minima,
  the maximum length, and rejects trailing garbage.
- Unbound (all-zero static-key section) New Sessions and duplicate ephemerals
  are rejected typed with `UnboundNewSessionNotSupported`.
- ES nonces are index-based: tag as AD, `0x00000000 || LE64(index)` nonce.
- `EciesTagSet` is not `Clone`, zeroizes on drop, and every consuming
  operation takes `&mut self`. Tags are 1-based on the wire while keys and
  nonces are 0-based, so issued entry N pairs with `key_{N-1}`;
  `trim_keys_below` drops consumed keys while preserving absolute indices.
  Plan 152 additionally trims the seal-only sender set inside
  `seal_existing_session` with an absolute `MAX_TAG_SET_INDEX` guard.

### Elligator2 security caveats (as stated in the code)

- **The third-party primitive is hidden.** `elligator2` (0.1.0) is a private
  implementation detail; `i2pr-client` and `i2pr-proto` never see the type.
  The wrapper exposes only the typed ECIES API and the 32-byte newtype
  `EciesEphemeralRepresentative`.
- **Encode-side randomization.** `EciesEphemeralKeypair::generate` drives
  `elligator2::to_representative(point, tweak)` with a CSPRNG tweak where
  `tweak & 0x01` selects the deployed-reference inverse-map branch and
  `tweak & 0xc0` populates the two free representation bits per `ENCODE_ELG2`.
  Both branches and all high-bit variants decode to the same Montgomery
  `u`-coordinate, so no protocol value changes.
  `from_seed_bytes` (fixed tweak 0) and `from_seed_bytes_with_tweak` are the
  deterministic vector constructors; production must use `generate`.
- **Receive-side strictness (Plan 132).** `decode_representative` rejects the
  all-zero representative, masks the two free high bits, refuses
  `r >= 2^254 - 10` (`ELLIGATOR_CANONICAL_THRESHOLD_LE` =
  `[0xf6, 0xff × 30, 0x3f]`), then delegates to
  `elligator2::from_representative`, and rejects an all-zero recovered
  Montgomery point. i2pr enforces Java I2P's strict `<` boundary, which is a
  deliberate safer subset of i2pd's executable `<=`
  (`BN_cmp(r, p12) <= 0`) — see the correction recorded in Plan 133.
- **`curve25519-elligator2` (0.1.0-alpha.2) is not used by this crate.** Plan
  131 retired it because its `RFC9380::to_representative` branch choice was
  deterministic and its `Randomized` mode rotated the derived X25519 public
  key. The dependency is still declared in the workspace table
  (`Cargo.toml:34`) but is now referenced by **no** crate in the workspace.
  Provenance:
  [`elligator2-production-representation.md`](../../specs/references/elligator2-production-representation.md).

### Red25519 scope and what it does not claim

`red25519.rs` is an **independent in-repo implementation of the I2P Red25519
(RedDSA) composition**, written from the frozen normative sources in
`specs/references/red25519-clean-room-freeze.md`. It is not a wrapper around
another RedDSA provider and it contains no field/scalar/point/bignum math of
its own — every operation is composed over the reviewed `curve25519-dalek`.

What it **is**:

- `HSTAR_PREFIX` domain separation and 2-byte little-endian length framing on
  every hashed transcript (which is what makes SHA-512 safe against length
  extension here), reduced `mod L`.
- `CONVERT_ED25519_PRIVATE` (RFC 8032 clamped scalar, stored verbatim even
  though it is routinely `>= L`), `GENERATE_PRIVATE` (64 random bytes reduced
  `mod L`), `DERIVE_PUBLIC`.
- `GENERATE_ALPHA`: deterministic in `(public key, unblinded sigtype, UTC
  day, optional lookup secret)`, with a SHA-256 salt over
  `ALPHA_SALT_PERSONALIZATION ‖ keydata` and HKDF `info =
  ALPHA_HKDF_INFO`, so a client re-derives `alpha` with no key exchange.
- `BLIND_PUBKEY` / `BLIND_PRIVKEY` additive re-randomization, with
  prime-order-subgroup enforcement on the input key (`SmallOrderPoint` /
  `NotPrimeOrderPoint`).
- Randomized signing: `sign` draws 80 fresh nonce bytes from the injected
  CSPRNG so the same key over the same message yields different bytes.
  `sign_with_nonce` exists only for vector qualification and adversarial
  tests; a fixed or repeated nonce leaks the private key.
- Cofactor-aware verification: `(-[S]B) + R + [c]vk`, multiplied by the
  cofactor and compared to the identity. `verify` and `verify_blinded` are the
  same equation with distinct errors so a caller cannot confuse an ordinary
  destination signature with an encrypted-LeaseSet2 signature.
- `blinded_storage_key` = `SHA-256(0x000b ‖ blinded public key)`. The lookup
  secret deliberately does not enter it, so a wrong secret is a lookup miss,
  never a wrong-record read.

What it explicitly **does not** claim or do:

- It owns no destination lifecycle, base32/base33 addressing, LeaseSet
  framing, NetDB storage, or Proposal 170 control mapping.
- **It is not advertised.** Red25519 is experimental; no capability or
  signature-type advertisement is derived from it.
- **The M12 floodfill roadmap is not closed by it.** Plan 280
  (`plans/closure/floodfill/280-status.md`) *stopped* with
  `stopped-no-acceptable-maintained-i2p-red25519-provider-type5-deferred`:
  the plan searched for a third-party provider and added no crypto code.
  Plans 330/331 (in the `i2pcontrol-proposal-170` lane, not the floodfill
  lane) later closed that provider gap with this in-repo implementation and
  qualified it. No floodfill serving, `caps=f` claim, or M12 advertisement
  follows from that — see `specs/CONFORMANCE.md` §"Red25519 (signature type
  11) status" and `specs/support.toml` `m12_type11_red25519`.
- **There is still no live interoperability claim for the type-11 transcript, but the
  cryptographic boundary is now closed in both directions.** Plan 335 closed as
  `blocked-measured-type-11-transcript-incompatible-with-both-named-references`:
  i2pr followed the specification's `I2P_Red25519H(x)` domain, while i2pd and
  Java I2P sign type 11 with the bare Zcash transcript and verify through a
  plain Ed25519 verifier. Blinding, alpha derivation, the storage key, and the
  ELS2 framing all agreed; only the transcript differed, so the references could
  not verify an i2pr type-5 record. Plan 336 recorded that as a spec-first
  conformance decision. ADR 0032 and Plan 346 correct the policy without
  touching the strict primitive: `red25519_deployed` is a separate composition
  over the same `curve25519-dalek` arithmetic, owned only by the ELS2 type-5
  verifier, cross-verified against executed output from both references in both
  directions. What remains unproven is the live end-to-end path (Plan 347), so
  nothing is advertised.
- `LookupSecret` lives here rather than in `i2pr-netdb` because it is a
  key-derivation input and `i2pr-netdb` has no `zeroize` dependency. The
  256-byte `MAX_LOOKUP_SECRET_LENGTH` is an i2pr resource decision, not a
  protocol limit, and over-long input is rejected rather than hashed
  silently. An absent secret and an empty secret are the same derivation
  input; a configured secret that failed to apply is an error, never a
  no-secret derivation.

## Dependencies

`crates/i2pr-crypto/Cargo.toml`, checked against the allowlist entry in
`scripts/check-dependency-direction.sh` (`"i2pr-crypto": {"i2pr-proto"}` —
production workspace dependency is `i2pr-proto` only).

### Production

| Dependency | Workspace version / features | Why it is allowed |
| --- | --- | --- |
| `i2pr-proto` | path `../i2pr-proto` | The single workspace dependency permitted by the allowlist; supplies the wire types (`PublicKey`, `SigningPublicKey`, `SignatureValue`, `RouterIdentity`, `RouterInfo`, `LeaseSet2`, `MetaLeaseSet`, `Hash`, `ReplySecret`, `CodecError`). |
| `ed25519-dalek` | 2.2, `default-features = false`, `std` + `zeroize` | Ed25519 signing and strict verification. ADR 0005. |
| `x25519-dalek` | 2.0.1, `static_secrets` + `zeroize` | X25519 static DH and public derivation. ADR 0005. |
| `curve25519-dalek` | 4.1.3, `alloc` + `group` + `precomputed-tables` + `zeroize` | The Red25519 group arithmetic this crate composes over: wide scalar reduction, canonical scalar decode, compressed point decode/encode, basepoint and variable-base multiply, point addition, cofactor/small-order/torsion predicates. Reviewed in ADR 0005 as amended by Plan 329; already in the lock through `ed25519-dalek`/`x25519-dalek`, so no version churn. |
| `elligator2` | 0.1.0, `default-features = false` | The active RFC 9380 representative codec. Plan 131 production switch; the crate is an internal implementation detail, never re-exported. |
| `chacha20poly1305` | 0.10.1, `default-features = false`, `alloc` | The two AEAD seams this crate owns: ECIES session payloads and the NetDB supplied-key reply. |
| `chacha20` | 0.9.1, `default-features = false` | The raw RFC 8439 layer stream for encrypted LeaseSet2 (an AEAD would be wrong here). |
| `hmac` | 0.12.1, `default-features = false` | HKDF-SHA256 extract/expand primitive. |
| `sha2` | 0.10 | SHA-256 (identity/storage digests, HKDF backing) and SHA-512 (`HStar`, `CONVERT_ED25519_PRIVATE`). |
| `subtle` | 2.6, `default-features = false` | `ConstantTimeEq` for integrity comparison. |
| `zeroize` | 1.8 range, `default-features = false`, `derive` | Secret erasure and the `Zeroizing` return type. |
| `rand_core` | 0.9, `default-features = false`, `os_rng` | Injected `TryCryptoRng` seam plus the `OsRng` re-export for callers. |
| `thiserror` | 2.0 | `CryptoError` / `HkdfError` / `EciesError` / `ChachaError` / `Red25519Error` derives. |

### Dev-dependencies (test-only)

| Dependency | Purpose |
| --- | --- |
| `rand_chacha` | Deterministic `ChaCha8Rng::seed_from_u64` test RNGs (inline unit tests and the integration differentials). |
| `serde_json` | Parses the six committed JSON fixtures in `crates/i2pr-crypto/tests/data/`. |

### Not a dependency of this crate

- `curve25519-elligator2` (0.1.0-alpha.2) — still declared in the workspace
  table (`Cargo.toml:34`) after Plan 131 retired it, but referenced by no
  crate. Treat it as a removable workspace entry, not a live dependency.
- `aes` and `siphasher` — owned by `i2pr-transport-ntcp2` (and `aes` also by
  `i2pr-tunnel`).
- No `i2pr-*` dependency other than `i2pr-proto`. The dependency chain is
  `i2pr-proto ← i2pr-crypto ← {i2pr-storage, i2pr-netdb, i2pr-client,
  i2pr-api, i2pr-runtime, …}`.

## Tests

### Inline unit tests (`#[cfg(test)]` in `src/`)

| File | Tests | Coverage |
| --- | --- | --- |
| `lib.rs` | 6 | `deterministic_generation_is_reproducible_only_with_injected_rng` (seeds 7/11/12/13 via `ChaCha8Rng::seed_from_u64`), `signature_vectors_reject_message_signature_and_key_mutations`, `hash_and_constant_time_helpers_are_stable`, `x25519_rejects_an_all_zero_shared_secret`, `router_info_signing_uses_retained_signed_bytes`, `netdb_ecies_reply_uses_supplied_tag_as_associated_data` (frozen ciphertext vector, wrong-tag and wrong-key negatives, redacted `Debug`) |
| `hkdf.rs` | 6 | determinism, input sensitivity, 32-byte wrapper, extract-then-expand equals one-shot, oversized-output rejection, zero-length output |
| `chacha.rs` | 6 | RFC 8439 §2.4.2 published vector, counter-0 control value (`COUNTER_ZERO_CONTROL`) proving the pinned counter, symmetry, counter-1 ≠ counter-0, distinct key/nonce keystreams, empty input |
| `ecies.rs` | 41 | frozen Plan 126 vector module (`fixed_vectors`, 31 constants from an independent Python reference), full production-function handshakes, wire-layout offset assertions for NS/NSR/ES, unbound/tamper/wrong-key/wrong-tag negatives, Plan 130 high-bit randomization, Plan 131 inverse-map branch and high-bit sweeps, Plan 132 canonical-threshold boundary rows, entropy-failure typed error, redacted-`Debug` checks |
| `red25519.rs` | 0 | The module carries no inline tests; all Red25519 evidence is in `tests/`. |

### Integration tests (`crates/i2pr-crypto/tests/`)

| File | Tests | What it pins |
| --- | --- | --- |
| `red25519_official_vectors.rs` (286) | 8 | The specification's own vector set (`data/red25519-official-vectors.json`), compared byte-for-byte; published signatures used as verification-only vectors because `SIGN` mixes unpublished randomness. Also: converted public keys match the crate's ordinary Ed25519 owner; an injected nonce reproduces production `sign` for the same nonce. |
| `red25519_adversarial.rs` (663) | 10 | The Plan 330 negative surface: non-canonical scalars/points, small-order and torsion inputs, malformed signature components, wrong keys/messages/days/secrets, the `MAX_MESSAGE_LENGTH` ceiling, and randomness failure kept distinct from protocol invalidity. Plus an independent re-derivation fixture for alpha/blinding/storage key (the specifications publish no vectors for those). |
| `red25519_reference_differential.rs` (161) | 1 | Differential against pinned i2pd (`tools/i2pd-red25519-oracle.cpp` at `2c694149…`): blinded keys and storage key agree, i2pr accepts the reference's blinded private key, and the reference's own type-11 signature is recorded as a **classified divergence** rather than an agreement. Randomized signatures are compared by acceptance, never byte equality. |
| `red25519_emissary_differential.rs` (254) | 2 | Black-box differential against `eggstack/emissary@6885a945` (post-freeze, public API only): byte-identical alpha, blinded keys, DHT storage keys, and signatures; plus Emissary/i2pd key-material compatibility. |
| `red25519_java_reddsa_differential.rs` (205) | 4 | The same type-11 transcript boundary measured against **executed** pinned Java I2P (`i2p/i2p.i2p` at `93eef5db…`, unmodified `net.i2p.crypto.eddsa` subtree), with the 3×3 accept/reject matrix. |
| `red25519_plain_ed25519_divergence.rs` (130) | 3 | Pins the transcript boundary in both directions with no external build: an i2pr spec-form signature is not a plain Ed25519 signature, and the reference's own signature is. |
| `els2_emissary_differential.rs` (300) | 5 | Post-freeze black-box differential for the Plan 332 layer cryptography: credential/subcredential, complete outer ciphertext, encrypted service address, and the blinding day reproduced from the published timestamp. Explicitly not a live interoperability gate, and it names Plan 335 as the blocker for that. |

Deterministic seeds are explicit throughout: `ChaCha8Rng::seed_from_u64`
(5, 7, 31337, 20251015, …), fixed nonce arrays such as `[0x5a; 80]`, and the
`from_seed_bytes` (tweak 0) ECIES constructor. Bounded negative paths are
present for every documented ceiling: HKDF `L > 8160`, ECIES payload
`65 507`, Red25519 message `65 534`, lookup secret `256` bytes, and the
Elligator2 canonical threshold.

Downstream, the ECIES surface is driven black-box by
`crates/i2pr-client/tests/plan121_trajectory.rs`
(`plan_126_corrected_deterministic_local_trajectory`). NTCP2 vectors live
outside this crate in `tests/fixtures/ntcp2/crypto/`, including
`storage-static-key.hex` (consumed by `i2pr-storage`).

## Distinctive design choices

1. **No local primitives, ever.** `#![forbid(unsafe_code)]` in `lib.rs`,
   `hkdf.rs`, `ecies.rs`, and `chacha.rs`; even the Red25519 module implements
   only the I2P-specific composition and delegates all curve arithmetic to
   `curve25519-dalek`.
2. **Elligator2 is an implementation detail.** The wrapper exposes only the
   typed ECIES API and a 32-byte newtype, so `elligator2` and
   `x25519-dalek` never reach `i2pr-client` or `i2pr-proto`.
3. **Deterministic construction is separated from production randomness.**
   `from_seed_bytes` (tweak 0) reproduces every frozen vector; `generate`
   draws a fresh CSPRNG seed and tweak. The two paths cannot be confused.
4. **Red25519 secrets are wrappers, not bytes.** Three fixed-size,
   non-cloneable, erase-on-drop owners for the unblinded/converted scalar, the
   daily `alpha`, and the blinded scalar; the only accessor is `secret_bytes`.
5. **A converted Ed25519 scalar is stored verbatim, not canonicalized.**
   `CONVERT_ED25519_PRIVATE` routinely produces a value `>= L`, so it is kept
   bit-identical to the published specification form and reduced only inside
   arithmetic — while generated and restored keys reject non-canonical input
   rather than silently reducing it.
6. **A converted key is not a fresh key.** The RFC 8032 clamping means the
   conversion reduces key-space entropy; the code documents this rather than
   presenting type 7 → type 11 as a security upgrade.
7. **Red25519 signing is randomized by default.** `sign` draws 80 fresh nonce
   bytes; `sign_with_nonce` is explicitly a test/vector entry point with a
   documented private-key-leak warning.
8. **Raw ChaCha20 for ELS2 layers, not an AEAD.** Each layer derives a fresh
   key from a fresh random salt per publication, so integrity comes from the
   Red25519 signature over the whole record. The 12-byte nonce and initial
   block counter 1 are enforced inside the wrapper, not left to callers.
9. **Layer key lifetime is the type's job.** `LayerCipherKey` is not `Clone`,
   has no `Debug`/`Display`/serde, erases on drop, and offers `zeroize_now`;
   the module documents that the `chacha20` cipher object's internal key copy
   cannot be erased through the reviewed API.
10. **Dual X25519 wrappers for two different jobs.** `X25519PrivateKey` has DH
    (transport/static); `EncryptionPrivateKey` deliberately has none, so an
    identity encryption key can never be used for transport DH.
11. **Two-pass RouterInfo signing.** A zero signature is a structural
    placeholder only; the record is rebuilt from the same semantic fields after
    signing the retained `signed_bytes` slice.
12. **Randomness failure is never protocol invalidity.** Each of the three
    modules has its own `RandomnessUnavailable` variant precisely so a retry
    loop cannot mask a real rejection.

## Cross-references

- [Overview](overview.md) · [Dependency graph](dependency-graph.md) ·
  [Tooling](tooling.md)
- [i2pr-proto](i2pr-proto.md) — the only workspace dependency; owns the wire
  types and the retained signed-byte regions
- [i2pr-storage](i2pr-storage.md) — primary identity-key consumer
- [i2pr-transport-ntcp2](i2pr-transport-ntcp2.md) — reuses
  `X25519PrivateKey` via `TransportStaticKey`; owns the data-phase ciphers and
  the forbidden nonce counter
- [i2pr-transport-ssu2](i2pr-transport-ssu2.md) — SSU2 keystream
- [i2pr-tunnel](i2pr-tunnel.md) — consumes `X25519PrivateKey`,
  `X25519SharedSecret`, `hkdf_sha256_32`, and `sha256` for the Plan 108
  ECIES-X25519 short-build seam
  ([108-conformance-amendment.md](../../plans/implementation/exploratory-tunnels/108-conformance-amendment.md))
- [i2pr-client](i2pr-client.md) — consumes the ECIES seal/open family, the tag
  sets, and the Red25519 composition for encrypted-LeaseSet2 publish/resolve
  (`crates/i2pr-client/src/session.rs`, `src/encrypted_leaseset.rs`)
- [i2pr-netdb](i2pr-netdb.md) — consumes `chacha::LayerCipherKey` and the
  Red25519 blinding/storage-key derivation for DatabaseStore type 5
  (`src/els2.rs`, `src/els2_auth.rs`)
- [i2pr-api](i2pr-api.md) — allowed consumer of `i2pr-crypto` for the NetDB
  reply seam

**ADRs**

- [ADR 0004 — Initial generated router identity algorithms](../../docs/adr/0004-router-identity-algorithms.md)
  (type 7 Ed25519 + type 4 X25519)
- [ADR 0005 — Reviewed cryptographic dependency selection](../../docs/adr/0005-crypto-dependency-selection.md)
  (reviewed direct crypto dependencies; amended by Plan 329 for
  `curve25519-dalek`, Plan 131 for the Elligator2 switch, and Plan 336 for
  the type-11 transcript decision)
- [ADR 0006 — Private identity storage](../../docs/adr/0006-private-identity-storage.md)

**Plans and specifications**

- [Plan 013](../../plans/implementation/workspace-foundation/013-m1-identity-crypto-storage.md)
  — M1 identity/crypto/storage plan of record
- [Plan 108 conformance amendment](../../plans/implementation/exploratory-tunnels/108-conformance-amendment.md)
  — HKDF helper context; the Plan 108 derivation labels are superseded
- [Plan 121](../../plans/implementation/destination-streaming/121-m6-ecies-garlic-session-layer.md)
  · [Plan 126](../../plans/implementation/destination-streaming/126-129-milestone6-final-corrective-roadmap.md)
  — ECIES destination session layer
- [Plan 280 status](../../plans/closure/floodfill/280-status.md) — **stopped**;
  no acceptable third-party Rust Red25519 provider, type 5 deferred
- [Plan 330 status](../../plans/closure/i2pcontrol-proposal-170/330-status.md) —
  `passed-independent-red25519-implementation` (this module)
- [Plan 331 status](../../plans/closure/i2pcontrol-proposal-170/331-status.md) —
  Red25519 independent qualification, reference transcript divergence recorded
- [Plan 332 status](../../plans/closure/i2pcontrol-proposal-170/332-status.md) —
  `passed-encrypted-leaseset2-foundation-without-per-client-authorization` (this module's `chacha.rs`)
- [Plan 335 status](../../plans/closure/i2pcontrol-proposal-170/335-status.md) —
  **blocked**; type-11 transcript measured incompatible with both named references
- [`specs/CONFORMANCE.md`](../../specs/CONFORMANCE.md) ·
  [`specs/support.toml`](../../specs/support.toml) — the authority on what may
  be claimed
- [`specs/references/red25519-clean-room-freeze.md`](../../specs/references/red25519-clean-room-freeze.md) ·
  [`red25519-algorithm-worksheet.md`](../../specs/references/red25519-algorithm-worksheet.md) ·
  [`red25519-qualification-freeze.md`](../../specs/references/red25519-qualification-freeze.md)
- [`specs/references/ecies-destination-ratchet.md`](../../specs/references/ecies-destination-ratchet.md) ·
  [`elligator2-production-representation.md`](../../specs/references/elligator2-production-representation.md)

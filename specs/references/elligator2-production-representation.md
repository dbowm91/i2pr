# Plan 131 Elligator2 production representation evidence

Status: recorded 2026-08-26 (Plan 131 Phase A1 / Phase B).

This addendum pins the reference behavior behind
`EciesEphemeralKeypair::generate` in `crates/i2pr-crypto/src/ecies.rs`:
production ephemeral representatives carry **CSPRNG-randomized inverse-map
branch bits** (matching deployed Java I2P / i2pd encoders) and
**CSPRNG-randomized high bits** (matching the normative I2P ECIES
`ENCODE_ELG2` post-processing), while `from_seed_bytes` remains the
deterministic (fixed-tweak-0) test/vector constructor.

## Normative contract

From the current I2P ECIES-X25519-AEAD-Ratchet specification
(geti2p.net/spec/ecies, "Elligator2 → Format" section; accurate for
0.9.66):

```text
ENCODE_ELG2():
  encodedKey = encode(pubkey)            // Elligator2 inverse map
  randomByte = CSRNG(1)
  encodedKey[31] |= (randomByte & 0xc0)  // OR 2 random bits into MSB

DECODE_ELG2():
  encodedKey[31] &= 0x3f                 // mask the 2 random bits out
  pubkey = decode(encodedKey)            // Elligator2 direct map
```

The official specification therefore requires only the two randomized
high bits over the standard X25519 public point. Current deployed
encoders additionally use one CSPRNG bit to choose between the two
valid recoverable pre-image branches produced by the canonical
least-square-root representative.

## Reference implementations

- Java I2P `router/java/src/net/i2p/router/crypto/ratchet/Elligator2.java`
  (repository `i2p/i2p.i2p`, branch `master`, inspected 2026-08-25):
  the one-argument on-the-wire `encode(point)` draws one CSPRNG byte,
  uses its low bit to select the pre-image "alternative" branch and
  ORs its top two bits into `rv[31]`
  (`rv[REPRESENTATIVE_LENGTH - 1] |= highBits & (byte) 0xc0`). Its
  `square_root()` helper normalizes to the canonical least square root
  (`result > (p-1)/2 ⇒ result = p - result`) **before** the high bits
  are ORed. `decode(representative)` masks byte 31 with `0x3f` and
  rejects non-canonical representatives (`r >= (p-1)/2`).
- i2pd `libi2pd/Elligator.cpp` (branch `openssl`, inspected
  2026-08-25): `Encode(key, encoded, highY, random)` computes r via
  `SquareRoot()` with the same least-square-root normalization, then —
  when `random` is set — draws `randByte = RAND_bytes(1)`,
  selects the ±pre-image with `highY = randByte & 0x01`, and copies
  the two highest bits into the encoded buffer
  (`encoded |= randByte & 0xC0` before the little-endian swap).
  `Decode()` masks the two highest bits (`encoded1 &= 0x3F`) and
  requires the canonical representative (`r <= (p-1)/2`).

Both references therefore emit **canonical representatives with
randomized high bits** AND **one bit of randomized pre-image branch**;
both reject non-canonical values after masking. The Java/i2pd
"alternative" is a choice between the two pre-image branches
`-x/(u(x+A))` and `-(x+A)/(ux)` whose squares differ but which
decode to the same Montgomery u-coordinate through the standard map;
it is not the non-canonical `p - r`.

## Selected library mode

The previous Plan 130 implementation routed through
`curve25519-elligator2` 0.1.0-alpha.2's `RFC9380::to_representative`.
That library derives the inverse-map branch from a deterministic
`v_in_sqrt` value computed from the public key, so the two pre-image
branches are *not* caller-randomizable. Its `Randomized` mode
randomizes the branch but also changes the derived DH public point
via `mul_base_clamped_dirty`, breaking the canonical X25519
Diffie-Hellman key the receiver would recover. Plan 131 therefore
swaps the dependency to `elligator2 = "0.1.0"` (default features
disabled; reviewed pure-Rust Elligator2 built on `fiat-crypto`'s
formally verified machine-generated field arithmetic; MIT OR
Apache-2.0; no `unsafe`; MSRV 1.85).

`elligator2::to_representative(point, tweak)` is the exact primitive
the plan-of-record asked for:

- `tweak & 0x01` selects between `u` and `u + A` as the inverse-map
  base (the deployed-reference branch bit);
- `tweak & 0xc0` populates the two free representation bits
  (the normative `ENCODE_ELG2` post-processing step);
- the result decodes to the **same** Montgomery u-coordinate the
  caller passed in (verified byte-for-byte by the crate's own
  differential tier-4 test against `curve25519-elligator2`'s
  `Randomized::to_representative` when both encoders are given the
  same point).

The new implementation therefore exposes the deployed I2P ECIES
branch behavior without writing any cryptographic arithmetic and
without changing the canonical X25519 Diffie-Hellman public key.

The `Randomized` mode of the old library was deliberately **not**
adopted: its `mul_base_clamped_dirty` integrates a low-order point
into the derived public key, which would change the
Diffie-Hellman public key away from what a standard X25519 decoder
recovers from the representative. That breaks cross-implementation
compatibility even though the name suggests a better anonymity
property. The normative I2P requirement is the two randomized high
bits plus one branch bit over a standard X25519 public key, which
the new primitive satisfies exactly.

## What i2pr randomizes

Production generation (`EciesEphemeralKeypair::generate`) draws a fresh
32-byte seed plus one tweak byte per attempt from the caller's
CSPRNG and feeds both into `elligator2::to_representative`. Over a
deterministic seeded sample the inverse-map branch bit is randomized
(`production_generator_randomizes_the_inverse_map_branch_bit`) and the
two most significant on-wire bits span all four values
(`production_generator_randomizes_the_two_high_representation_bits`).
Both randomized degrees of freedom are fingerprint-regression tests
with a fixed seed; they are not randomness certifications.

The deterministic `from_seed_bytes(seed)` constructor pins the tweak
to `0` (branch bit 0, high bits `00`). Frozen Plan 126 KDF/Noise
vectors are computed through this constructor so every legacy
vector remains reproducible; production callers must not use it
directly.

## Independent frozen fixtures (Plan 131 Phase A1)

Generated once on 2026-08-25 by a pure-Python implementation of
`DECODE_ELG2`/`ENCODE_ELG2` written directly from the specification
text above, including a pure-Python RFC 7748 X25519 ladder that was
cross-checked against the Python `cryptography` library
(`X25519PrivateKey.from_private_bytes`; scalar
`a546e36b…ba449ac4` → public `1c9fd88f…4fae7019`). Nothing in the
generator shares code with i2pr or with `elligator2`. The frozen
constants live in the `reference_fixtures` module of the
`ecies.rs` test suite and assert:

- all four high-bit variants decode to the same `X_COORDINATE`;
- both reference encode branches (distinct canonical representatives)
  decode to the same `X_COORDINATE`;
- production-generated representatives decode to exactly the X25519
  public key their secret derives (`PublicKey(clamp(secret))`).

These fixtures pin decoder compatibility with Java I2P / i2pd
produced traffic; they are never recomputed at test time.

## Scope note

No mixed-router or wire-level interoperability is claimed by this
record. NTCP2 remains experimental and non-advertised; destination
ECIES interoperability remains separate acceptance debt.
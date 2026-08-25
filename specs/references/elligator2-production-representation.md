# Plan 130 Elligator2 production representation evidence

Status: recorded 2026-08-25 (Plan 130 Phase A1 / Phase B).

This addendum pins the reference behavior behind
`EciesEphemeralKeypair::generate` in `crates/i2pr-crypto/src/ecies.rs`:
production ephemeral representatives carry **CSPRNG-randomized high
bits** exactly as the current I2P ECIES specification requires, while
`from_seed_bytes` remains the deterministic (fixed-tweak-0) test/vector
constructor.

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

Consequences pinned by this text:

- The two most significant bits of byte 31 are **free randomization
  bits**: every conforming decoder masks them off before mapping, so
  they change no protocol value.
- The Noise transcript and all Diffie-Hellman operations consume the
  **decoded X25519 public key**, never the raw representative bytes.

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
randomized high bits**; both reject non-canonical values after masking.
The Java/i2pd "alternative" is a choice between the two pre-image
branches `-x/(u(x+A))` and `-(x+A)/(ux)` whose squares differ but which
decode to the same Montgomery u-coordinate through the standard map;
it is not the non-canonical `p - r`.

## Selected library mode

`curve25519-elligator2` 0.1.0-alpha.2, variant `RFC9380`
(feature-gated `MapToPointVariant` API):

- `to_representative(point, tweak)` produces the canonical
  least-square-root representative (conditional negation against
  `(p-1)/2`) and applies exactly the normative post-processing step
  (`a[31] |= MASK_SET_BYTE & tweak`).
- `from_representative(bytes)` masks the top two bits
  (`r[31] &= MASK_UNSET_BYTE`, i.e. `&= 0x3f`) before mapping —
  identical to `DECODE_ELG2` and to both reference decoders.
- No hand-written Elligator arithmetic exists anywhere in i2pr; the
  mapping is exclusively this reviewed dependency API.

The `Randomized` variant was evaluated and deliberately **not** used:
its `mul_base_clamped_dirty` integrates a low-order point into the
derived public key, which would change the Diffie-Hellman public key
away from what a standard X25519 decoder recovers from the
representative. That breaks cross-implementation compatibility even
though the name suggests a better anonymity property. The normative
I2P requirement is only the two randomized high bits over a standard
X25519 public key, which the `RFC9380` mode satisfies exactly.

## What i2pr randomizes

Production generation (`EciesEphemeralKeypair::generate`) draws a fresh
32-byte seed plus one tweak byte per attempt from the caller's CSPRNG
and applies `tweak & 0xc0`. Over a deterministic seeded sample the four
on-wire high-bit values all occur
(`production_generator_randomizes_the_two_high_representation_bits`).
The alternative pre-image branch is not selectable through the
dependency API; i2pr always emits the canonical representative of the
standard branch, which every reference implementation decodes. Both
reference branches are proven decodable by i2pr through frozen
independent fixtures (below), so inbound traffic using either branch is
accepted.

## Independent frozen fixtures (Plan 130 Phase A1)

Generated once on 2026-08-25 by a pure-Python implementation of
`DECODE_ELG2`/`ENCODE_ELG2` written directly from the specification
text above, including a pure-Python RFC 7748 X25519 ladder that was
cross-checked against the Python `cryptography` library
(`X25519PrivateKey.from_private_bytes`; scalar
`a546e36b…ba449ac4` → public `1c9fd88f…4fae7019`). Nothing in the
generator shares code with i2pr or with `curve25519-elligator2`. The
frozen constants live in the `reference_fixtures` module of the
`ecies.rs` test suite and assert:

- all four high-bit variants decode to the same `X_COORDINATE`;
- both reference encode branches (distinct canonical representatives)
  decode to the same `X_COORDINATE`;
- production-generated representatives decode to exactly the X25519
  public key their secret derives (`PublicKey(clamp(secret))`).

These fixtures pin decoder compatibility with Java I2P / i2pd produced
traffic; they are never recomputed at test time.

## Scope note

No mixed-router or wire-level interoperability is claimed by this
record. NTCP2 remains experimental and non-advertised; destination
ECIES interoperability remains separate acceptance debt.

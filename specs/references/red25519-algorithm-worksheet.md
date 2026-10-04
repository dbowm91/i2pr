# Red25519 and Encrypted LeaseSet2 algorithm worksheet (Plan 329)

Status: frozen 2026-10-04; §2, §5, and §14 amended by Plan 330 with the two findings recorded in
§14.8 and §14.9. §14.10–§14.15 amended by Plan 332, and §14.16–§14.23 added by Plan 333. Written
from the normative sources pinned in
[`red25519-clean-room-freeze.md`](red25519-clean-room-freeze.md) §1, with ambiguities resolved
against the pinned readable references in §2 of that file and recorded in §14 here.

This worksheet is the implementation contract for Plans 330–334. It is prose plus pseudocode and
contains no Rust. An implementation agent must be able to write the `i2pr-crypto` Red25519 module
from this file plus the named specifications, without reading any Emissary source and without
inventing an elliptic-curve primitive.

Every rule below is either (a) quoted from a pinned specification, (b) derived from one, or
(c) marked **[compat]** as implementation-dependent compatibility behavior whose only justification
is agreement between both pinned references. Anything that is neither is a defect in this
worksheet, not an implementation choice.

## 1. Notation, constants, ownership

| Symbol | Meaning |
|---|---|
| `B` | Ed25519 basepoint as in RFC 8032 |
| `L` | `2^252 + 27742317777372353535851937790883648493` (RFC 8032 group order) |
| `p` | `2^255 - 19` (field prime) |
| `h` | cofactor, 8 |
| `[s]B` | fixed-base scalar multiplication of `B` by `s` |
| `[s]A` | variable-base scalar multiplication of `A` by `s` |
| `H(p, d)` | `SHA-256(p || d)`, personalization string `p` as raw ASCII bytes |
| `HKDF(salt, ikm, info, n)` | RFC 5869 HKDF with HMAC-SHA256, `SALT_LEN` 32 |
| `ENCRYPT/DECRYPT` | ChaCha20 (RFC 7539 §2.4), initial counter 1, 32-byte key, 12-byte IV |
| `CSRNG(n)` | `n` bytes from the injected `TryCryptoRng` |

`L` and the group are owned by the reviewed curve library. i2pr never writes a field, scalar,
point, or bignum routine. Scalar reduction, canonical decoding, point decompression, basepoint
multiplication, point addition, and cofactor checks are dependency calls (freeze §4).

Fixed sizes: scalars 32 bytes, public keys 32 bytes, signatures 64 bytes, hashes 32 bytes,
`HStar` digests 64 bytes, dates 8 ASCII bytes, sigtype codes 2 bytes big-endian.

## 2. Encodings

`ENCODE_SCALAR(s)`: 32-byte little-endian encoding of the integer `s`, which is always in
`[0, L)`. `DECODE_SCALAR(b)`: parse 32-byte little-endian; accept only if the integer is `< L`.
A non-canonical scalar (including `S >= L`) is a hard failure, never a reduction. **[compat]** Both
references reduce only where the specification says "mod L" (wide reduction) and otherwise reject or
compare canonically.

`ENCODE_POINT(P)`: 32-byte compressed Edwards form, `y` little-endian with the sign of `x` in the
top bit (RFC 8032 §5.1.2). `DECODE_POINT(b)`: decompress; reject if the result is not a curve point.

Point-validity policy, frozen:

- A public key used for **verification** is accepted if it decompresses to a curve point. The
  verification equation multiplies by the cofactor, so small-order and mixed-order keys cannot be
  used to forge a valid signature. No additional subgroup check is required by the equation and none
  is performed (matching both references).
- A public key used as **blinding input** (`A` in `BLIND_PUBKEY`, and therefore every blinded public
  key `A'` that is derived and then signed) must additionally be **torsion-free** (`[L]P == identity`).
  **[compat]** i2pd and Java both restrict blinding to the prime-order subgroup; the ELS2
  specification requires the blinded public key to be "on the curve and on the prime-order subgroup".
  This is the single place i2pr is stricter than "decompress succeeded", and it is required by the
  specification's own security goal.
- Every point i2pr *produces* (`[s]B` for `s` canonical, and sums of such points) is torsion-free by
  construction and must be treated as such without re-checking.

## 3. `HStar` and the Red25519 hash domain

From the Red25519 specification (RedDSA specialization with `H(x) := SHA-512("I2P_Red25519H(x)" || x)`,
`l_H = 512`):

```text
HStar(prefix1, prefix2, m) :=
    h = SHA-512("I2P_Red25519H(x)" || prefix1 || prefix2 || len_u16le(m) || m)
    return h mod L          // 64-byte digest, little-endian, reduced mod L
```

- The literal prefix `"I2P_Red25519H(x)"` is 17 ASCII bytes.
- `len_u16le(m)` is the message length as 2 bytes little-endian. This length-prefixed encoding is
  what makes SHA-512 safe against length extension here; it is not optional.
- Messages must be at most 65534 bytes. 65535 is reserved. i2pr enforces 65534 as a hard bound before
  hashing.
- The reduction is a 512-to-256-bit **wide** reduction (`from_bytes_mod_order_wide`).

## 4. RedDSA core

```text
GENERATE_PRIVATE  := CSPRNG(64) mod L
GENERATE_RANDOM   := CSPRNG(64) mod L
DERIVE_PUBLIC(sk) := [sk] B
RANDOMIZE_PRIVATE(sk, alpha) := (sk + alpha) mod L
RANDOMIZE_PUBLIC(vk, alpha)  := vk + [alpha] B
```

- Red25519 private keys are **not clamped**. Clamping is an Ed25519-only step.
- `alpha` is a canonical scalar; a caller-supplied `alpha` must be canonical on the way in and the
  result of `RANDOMIZE_*` is canonical by construction.
- `RANDOMIZE_PUBLIC(vk, alpha) == DERIVE_PUBLIC(RANDOMIZE_PRIVATE(sk, alpha))` is an invariant the
  implementation must be able to demonstrate with a public and a private derivation of the same key.

## 5. Type conversion

```text
CONVERT_ED25519_PRIVATE(edsk) :=
    s = SHA-512(edsk)[0..32]
    s[0]  &= 248
    s[31]  = (s[31] & 63) | 64
    return s                       // canonical: the clamped form is always < L

CONVERT_ED25519_PUBLIC(edpk) := edpk
```

Type 7 → type 11 conversion is therefore: the Red25519 private scalar is exactly the Ed25519
secret scalar, and the public key is unchanged.

**The converted scalar is not canonical.** Clamping forces the value into `[2^254, 2^255)`, which is
above `L`, so a converted type-7 key is *not* a canonical residue modulo `L`. The value must be kept
verbatim if byte-fidelity with the published vectors matters (it does, for `sk`), and every operation
that consumes it — derivation, blinding, signing — is defined modulo `L` and must reduce internally.
Only the conversion output is exempt from the canonical-scalar rule; `GENERATE_PRIVATE`,
`GENERATE_ALPHA`, and `BLIND_PRIVKEY` outputs are canonical. See §14.8.

The security loss of this direction (roughly one bit) is accepted by the specification for existing
destinations; a *new* encrypted destination should use sigtype 11 natively (Plan 324 typed policy
owns that choice).

## 6. Blinding (per-day, UTC)

`datestring` is 8 ASCII bytes `YYYYMMDD` derived from the **current UTC day**. i2pr must accept an
explicit day only in an internal, typed, already-validated form (a `Date`-equivalent day triple),
never a free-form string, so an invalid calendar day cannot reach the KDF.

```text
GENERATE_ALPHA(A, stA, day, secret) :        // secret: UTF-8, may be empty
    keydata  = A || stA_be16 || 0x000b_be16
    salt     = SHA-256("I2PGenerateAlpha" || keydata)          // H("I2PGenerateAlpha", keydata)
    seed     = HKDF(salt, datestring || secret, "i2pblinding1", 64)
    alpha    = seed mod L                                     // 64-byte LE wide reduction
    return alpha

BLIND_PRIVKEY(a, alpha) := (a + alpha) mod L
BLIND_PUBKEY(A, alpha)  := A + [alpha] B
```

Frozen details and bounds:

- `stA` is the **unblinded** destination sigtype as 2 bytes big-endian; the blinded sigtype is
  always `0x000b`. Only `0x0007` and `0x000b` are accepted as `stA`; any other sigtype is rejected
  before any allocation.
- `secret` participates only as appended UTF-8 bytes after the date. An absent secret and an empty
  secret are the same input; there is never a silent fallback in the other direction (a configured
  lookup secret that fails to be applied is an error, not a no-secret derivation).
- The 64-byte `seed` is reduced as a 64-byte **little-endian** integer. **[compat]** i2pd
  `DecodeBN<64>` and Java `EdDSABlinding.reduce` over the 64-byte HKDF output.
- The HKDF salt is 32 bytes; the `info` labels are the exact ASCII strings `"i2pblinding1"`,
  `"I2PGenerateAlpha"`.
- `BLIND_PRIVKEY` for a **type 7** destination blinds the clamped Ed25519 scalar
  `CONVERT_ED25519_PRIVATE(seed)`, never the raw seed. **[compat]** i2pd `ExpandPrivateKey`
  (SHA-512 then clamp) and Java `EdDSABlinding`.
- `BLIND_PRIVKEY` for a **type 11** destination treats the stored 32-byte private key as a scalar
  directly.
- A new `alpha` and blinded keypair are generated every UTC day. Rollover is atomic: no record may
  mix an old `alpha` with a new signing key, storage key, or address.
- Lookup secret bound: the specification gives no length; i2pr applies a documented maximum
  (Plan 332) and rejects longer input instead of hashing it silently.

## 7. Signing and verification

```text
SIGN(sk, m) :=
    T        = CSPRNG(80)                       // caller-supplied CSPRNG; never a constant in production
    vkBytes  = ENCODE_POINT(DERIVE_PUBLIC(sk))
    r        = HStar(T, vkBytes, m)
    R        = [r] B
    Rbytes   = ENCODE_POINT(R)
    c        = HStar(Rbytes, vkBytes, m)
    S        = (r + c * sk) mod L
    return Rbytes || ENCODE_SCALAR(S)           // 64 bytes

VERIFY(vk, m, sig) :=
    if len(sig) != 64: false
    Rbytes, Sbytes = sig[0..32], sig[32..64]
    R = DECODE_POINT(Rbytes)         ; if invalid: false
    S = DECODE_SCALAR(Sbytes)        ; if S >= L: false
    vkBytes = ENCODE_POINT(vk)
    c = HStar(Rbytes, vkBytes, m)
    return ((-[S] B) + R + ([c] vk)).mul_by_cofactor() == identity
```

Frozen details:

- `r` depends on the public key value, not on a hash of the private key. This is the one
  intentional deviation from Ed25519 signing.
- Every Red25519 signature is randomized; the same key over the same message produces different
  bytes. Signatures therefore compare by **acceptance**, never by byte equality across
  implementations.
- Verification must be panic-free and allocation-bounded for any 64-byte input: reject on length,
  point decode, and scalar canonicality before any curve work.
- Verification operands are public, so a variable-time double-scalar multiplication is acceptable
  there; signing must not use variable-time paths for `sk` or `S`.

## 8. DHT storage key

```text
storageKey = SHA-256(0x000b_be16 || blindedPublicKey)
```

The sigtype is the **blinded** sigtype, always `0x000b`, 2 bytes big-endian, then the 32-byte
blinded public key. The lookup secret never enters the storage key; it enters only `GENERATE_ALPHA`,
so a wrong secret yields a different blinded key and therefore a different storage key — a lookup
miss, never a wrong-record read. **[compat]** i2pd `GetStoreHash` hashes `htobe16(11) || blinded`.

## 9. Credentials

```text
keydata       = A || stA_be16 || 0x000b_be16
credential    = H("credential", keydata)                       // SHA-256
subcredential = H("subcredential", credential || blindedPublicKey)
```

The personalization strings keep the credential from colliding with any DHT lookup key such as the
plain Destination hash. The subcredential binds all ELS2 keys to knowledge of the unblinded
signing public key. Note the order: the *unblinded* `A` goes into the credential, the *blinded*
public key goes into the subcredential.

## 10. Encrypted LeaseSet2 framing

Encrypted LS2 is DatabaseStore **type 5** and does **not** reuse the ordinary LeaseSet2 header.
Overall: `Layer 0 || ENCRYPT(layer 1) || Signature`.

### Layer 0 (outer, plaintext, in this order)

| Field | Size | Notes |
|---|---|---|
| Store type | 1 | `0x05`; not in the record itself but covered by the signature, taken from the DatabaseStore message |
| Blinded sigtype | 2 | big-endian, always `0x000b` |
| Blinded public key | 32 | |
| Published | 4 | big-endian seconds since epoch |
| Expires | 2 | big-endian offset from published, 18.2 h max (65535 s) |
| Flags | 2 | bit 0 = offline keys present; other bits 0 |
| Transient expires | 4 | only if flag bit 0, big-endian |
| Transient sigtype | 2 | only if flag bit 0, big-endian |
| Transient signing public key | 32 | only if flag bit 0 |
| Offline-key signature | 64 | only if flag bit 0: over `transient expires || transient sigtype || transient public key`, verified with the **blinded** public key |
| `lenOuterCiphertext` | 2 | big-endian |
| `outerCiphertext` | that | `outerSalt (32) || ChaCha20(layer 1 plaintext)` |
| Signature | 64 | over **everything above starting at the store-type byte**; verified with the transient key if flag bit 0 is set, else with the blinded public key |

With offline keys, the transient key must itself be Ed25519 or Red25519. There is no defined
packaging format for delivering pre-generated daily blinded/transient keys, and no I2CP extension
for it, so i2pr's floor is: the flags block is parsed and verified correctly, but key-batching
policy is an operator concern and is not claimed as a capability.

### Layer 1 (middle, encrypted)

| Field | Size | Notes |
|---|---|---|
| Flags | 1 | bit 0 = per-client auth present; bits 3-1 = scheme (`000` DH, `001` PSK) when bit 0 set; bits 7-4 = 0 |
| auth data | 32/40/N | DH: `ephemeralPublicKey (32)`, then `clients: 2` big-endian, then N × 40 bytes. PSK: `authSalt (32)`, then `clients: 2`, then N × 40 bytes |
| `authClient` | 40 | `clientID (8) || clientCookie (32)` |
| inner ciphertext | rest | `innerSalt (32) || ChaCha20(layer 2 plaintext)` |

### Layer 2 (inner, encrypted)

`type (1) || data`, where type is 3 (LS2) or 7 (Meta LS2) and `data` is the ordinary LeaseSet2 of
that type, including its own header and signature. The inner LeaseSet2 is validated normally after
decryption, and its published/expiry must agree with the outer layer.

### Key derivation and encryption

```text
outerInput  = subcredential || published_be32
outerSalt   = CSRNG(32)
keys        = HKDF(outerSalt, outerInput, "ELS2_L1K", 44)
outerKey    = keys[0..32] ; outerIV = keys[32..44]
outerCiphertext = outerSalt || ENCRYPT(outerKey, outerIV, layer1Plaintext)

innerInput  = authCookie || subcredential || published_be32        // authCookie empty when no auth
innerSalt   = CSRNG(32)
keys        = HKDF(innerSalt, innerInput, "ELS2_L2K", 44)
innerKey    = keys[0..32] ; innerIV = keys[32..44]
innerCiphertext = innerSalt || ENCRYPT(innerKey, innerIV, layer2Plaintext)
```

- `authCookie` is 32 bytes when client authorization is enabled, zero-length otherwise.
- ChaCha20 with a fixed initial counter of 1 is safe here only because the key is fresh per
  publication (fresh random salt per layer). Reusing a salt with the same key is forbidden.
- After decryption, check that the inner published/expiry match the outer values; a mismatch is a
  decryption failure, not a warning.

## 11. Per-client authorization

Both schemes hide *which* client is authorized. Both derive an 8-byte `clientID`, a 32-byte key and
a 12-byte IV from a 52-byte HKDF output, then encrypt the 32-byte `authCookie` with ChaCha20.

### Pre-shared key

```text
// server, per publication
authCookie = CSRNG(32) ; authSalt = CSRNG(32)
for each authorized client i:
    okm          = HKDF(authSalt, psk_i || subcredential || published_be32, "ELS2PSKA", 52)
    clientKey_i  = okm[0..32] ; clientIV_i = okm[32..44] ; clientID_i = okm[44..52]
    clientCookie_i = ENCRYPT(clientKey_i, clientIV_i, authCookie)
```

The client recomputes with its own `psk` and searches layer 1 for its `clientID`. Client
identifiers are compared in constant time where an entry is matched positionally, and the entry
order should be randomized per publication so clients cannot infer membership changes.

### DH (X25519)

```text
// server, per publication
authCookie = CSRNG(32)
(esk, epk) = X25519 keypair                       // generation-local
for each authorized client i (public key cpk_i):
    sharedSecret = X25519(esk, cpk_i)            // all-zero shared secret is rejected
    okm          = HKDF(epk, sharedSecret || cpk_i || subcredential || published_be32, "ELS2_XCA", 52)
    clientKey_i  = okm[0..32] ; clientIV_i = okm[32..44] ; clientID_i = okm[44..52]
    clientCookie_i = ENCRYPT(clientKey_i, clientIV_i, authCookie)
```

The client derives the same values from its private key and the published `epk`, so its private
key never leaves the device. The server pays N+1 DH operations for N clients; both client count and
aggregate DH work must be bounded.

## 12. b33 (new-format b32) addressing

```text
// 32-byte-key case
data = flags(1) || unblindedSigType(1) || blindedSigType(1) || publicKey(32)
checksum = CRC32(data[3..end])                    // little-endian CRC-32
data[0] ^= checksum & 0xff
data[1] ^= (checksum >> 8) & 0xff
data[2] ^= (checksum >> 16) & 0xff
address = Base32Encode(data) || ".b32.i2p"        // 56 characters + suffix
```

Decode reverses it: base32-decode, CRC-32 over `data[3..end]`, XOR the same three bytes, then read
flags and sigtypes.

Flags:

| Bit | Meaning |
|---|---|
| 0 | 1 = sigtypes are two bytes each (then 2+2 sigtype bytes); 0 = one byte each (upper byte zero) |
| 1 | 1 = a blinding **secret** is required to derive the key |
| 2 | 1 = a per-client **private key** is required to decrypt |
| 7-3 | unused, must be 0 |

Frozen rules:

- The address carries the **unblinded** public key (so the client can derive `A'`), plus the
  unblinded and blinded sigtypes. It is not a Destination hash and must not be treated as one.
- One-byte sigtype form is the only form Plan 332 needs; the two-byte form is parsed and rejected or
  supported explicitly, never silently mis-parsed.
- Base32 is case-insensitive and canonical: unused trailing bits of the 56-character form must be
  zero, and a non-canonical encoding is rejected rather than normalized.
- Ordinary 52-character `.b32.i2p` addresses are unchanged by all of this and must keep working.
- **The address never contains a secret or a private key.** i2p-style "private links" embedding
  key material are out of scope by specification and by i2pr policy.

## 13. Bounds and failure behavior

| Input | Bound / rule |
|---|---|
| Message to `HStar` | ≤ 65534 bytes |
| Lookup secret | documented maximum (Plan 332), rejected when exceeded |
| `datestring` | exactly 8 ASCII bytes from a validated UTC day |
| Sigtype pair | unblinded ∈ {7, 11}; blinded always 11 |
| Layer-0 `Expires` | ≤ 65535 s offset; expiry consistency enforced after decrypt |
| Client auth count | bounded list, 2-byte count, 40 bytes per entry, max+1 rejected |
| Encrypted record length | bounded by the NetDB record ceiling; inner LS2 length separately bounded |
| Daily-key cache | bounded cardinality, evicting oldest |
| Signature | exactly 64 bytes; length checked before any decode |
| Randomness | CSPRNG failure is a distinct error from protocol invalidity |

A protocol-invalid input must produce a typed error, never a panic, never a partially applied state
change, and never a log line containing key, secret, salt, or plaintext.

## 14. Resolved ambiguities (with evidence)

1. **ELS2 KDF slicing `[0:31]` / `[32:43]` / `[44:51]`.** The specification text is internally
   inconsistent with 32-byte ChaCha20 keys and 12-byte IVs. Both pinned references use
   `key = okm[0..32]`, `iv = okm[32..44]`, `clientID = okm[44..52]`; i2pd passes `keys` and
   `keys + 32` straight into ChaCha20 and writes the client id at `okm + 44`. Frozen as
   implementation-dependent compatibility behavior. **[compat]**
2. **`alpha` reduction width.** 64-byte HKDF output, reduced as a 64-byte little-endian integer.
   i2pd `DecodeBN<64>(seed) mod l`; Java `EdDSABlinding.reduce` over the same 64-byte output. **[compat]**
3. **Type-7 private blinding input.** The clamped `SHA-512(seed)[0..32]` scalar, not the raw seed.
   i2pd `ExpandPrivateKey`; Java `EdDSABlinding`. **[compat]**
4. **Signature rows of the official vectors are not reproducible.** `SIGN` consumes 80 unpublished
   random bytes, so `sig`/`rsig` are verification vectors. `edpk`, `sk`, `vk`, `rsk`, `rvk` are
   byte-comparable. Frozen in the freeze record §3.
5. **Signature preimage of layer 0 includes the store-type byte.** The type byte is not stored in the
   record but is covered by the signature; i2pd signs `buffer[0..offset]` where the buffer starts
   with the store type. **[compat]**
6. **b33 CRC-32 coverage.** Checksum covers `data[3..end]` and is folded into the first three bytes;
   i2pd and Proposal 149 agree. **[compat]**
7. **No official vectors exist for `GENERATE_ALPHA`, credentials, ELS2 layers, or b33.** These are
   covered only by independent cross-implementation fixtures, recorded with source pin.
8. **A converted Ed25519 scalar is not canonical mod `L`** (found while implementing Plan 330).
   The clamping step puts the scalar in `[2^254, 2^255)`, above `L ≈ 2^252`. The specification's own
   vector `sk` is therefore non-canonical, while `rsk = (sk + alpha) mod L` is canonical. Handling:
   store the converted value verbatim, expose it through a converter that validates the clamped shape
   rather than canonicality, keep the canonical rule for every other key, and reduce inside every
   arithmetic operation. i2pd reaches the same arithmetic through `DecodeBN<32>` plus a final
   `mod L`, and the official vectors pass byte-exactly under this handling. **[compat]**
9. **Small-order points must be refused where a prime-order key is required** (found while
   implementing Plan 330). The identity satisfies the "in the prime-order subgroup" test trivially,
   so a subgroup-membership check alone lets it through. Blinding therefore requires *both*
   `is_small_order() == false` and `is_torsion_free() == true`. Verification deliberately applies
   neither, because the cofactor multiplication in the verification equation already makes such keys
   unusable for forgery. **[compat]**
10. **The ELS2 layer keystream's initial block counter is 1, and it is observable** (found while
    implementing Plan 332). The specification says "the initial counter set to 1"; the pinned i2pd
    reference builds the same IV layout (`counter = 1` little-endian, then the 12-byte nonce). The
    RFC 8439 §2.4.2 published vector happens to use initial counter 1, so it doubles as the counter
    check: the same key and nonce at counter 0 produce a different prefix. Frozen as counter 1 with a
    dedicated control row so a regression cannot silently change the stream. **[compat]**
11. **ELS2 layer framing overhead is exactly 66 bytes, not 130** (found while implementing Plan 332).
    The outer ciphertext is `outerSalt(32) || flags(1) || innerSalt(32) || innerStoreType(1) || inner`;
    the inner salt is *inside* the layer-1 plaintext, not a third salt in the clear. A first-pass
    resource bound over-counted the overhead at 130 bytes. The bound is now derived from the framing
    rather than restated, so the two cannot drift. An over-large ceiling is a safety-only defect; an
    under-large one would reject valid records at the boundary. **[spec]**
12. **The `expires` offset is bounded by two rules that do not contain each other** (found while
    implementing Plan 332). The field is a two-byte offset from `published`, so it cannot express the
    ~86 400 s remaining until the next UTC midnight, which a record published early in the day
    needs. Clamping to midnight and then saturating would emit a record that expires minutes before
    midnight while the caller believed it lived until midnight. Frozen as
    `offset = min(requested, until_midnight, 65535)`, so `published + offset` is the record's true
    absolute expiration under every bound. **[spec]**
13. **The b33 CRC-32 is a typo detector, not a MAC** (found while implementing Plan 332). The
    checksum covers `data[3..end]` and is folded into the leading three bytes, so those bytes are a
    function of the sigtypes and the key. A bit flip *inside the folded prefix* therefore decodes to a
    **different valid address** rather than failing, while a flip in the key region fails the
    checksum. i2pr enforces canonical form by re-encoding and comparing, which gives one text form
    per address, and the residue is documented rather than papered over. The encoded flags say
    whether a secret is *required*; no secret is ever encoded. **[spec]**
14. **The lookup secret protects discovery, not content** (found while implementing Plan 332). The
    secret enters `GENERATE_ALPHA`, so it changes the *blinded* key and therefore the DHT storage
    key. It does not enter the credential or the subcredential, which are functions of the unblinded
    public key and the blinded public key alone. Consequences: a party with a b33 address but not the
    secret cannot *find* the record; a party that already holds the record bytes and the address
    *can* decrypt them, because both credentials are public-key derivations. A floodfill, which
    holds the record but never the unblinded public key, cannot derive the subcredential at all.
    Raising the bar against a party holding the record is what per-client authorization (Plan 333)
    is for. **[spec]**
15. **ELS2 layers are malleable by construction, and the signature is the integrity layer** (found
    while implementing Plan 332). A layer is a raw ChaCha20 stream with no authenticator, because
    each layer's key comes from a fresh random salt and the Red25519 signature covers the whole
    layer-0 region. A flipped ciphertext byte corrupts the plaintext, and whether the corruption
    still parses depends on where it lands. Integrity is therefore asserted at the signature and at
    the inner LeaseSet2's own signature, not at the layer. **[spec]**
16. **The authorization HKDF reuses the §14.1 slicing, and `authInput` is 68 bytes for both
    schemes** (found while implementing Plan 333). Both schemes expand to a 52-byte OKF and slice it
    identically — `key = okm[0..32]`, `iv = okm[32..44]`, `clientID = okm[44..52]` — so §14.1's
    resolution of the specification's inconsistent `[0:31]/[32:43]/[44:51]` applies here unchanged.
    `authInput` is `psk_i || subcredential || published_be32` for PSK and
    `sharedSecret || cpk_i || subcredential || published_be32` for DH, both exactly 68 bytes. The
    DH salt is the server's ephemeral public key, not a separate random salt. **[compat]**
17. **The client's own public key is inside the DH derivation, which is what stops key
    substitution** (found while implementing Plan 333). A DH client derives
    `clientID_i` from `sharedSecret || cpk_i || subcredential || published`, where `cpk_i` is the
    *client's own* published public key. A client that keeps its private key but substitutes a
    different public key therefore derives a different `clientID` and fails to match its own entry.
    The consequence is worth stating: the server cannot be tricked into authorizing one key under
    another client's identity, and a client cannot claim a co-client's authorization. **[spec]**
18. **Duplicate `clientID` values are rejected, not merged** (found while implementing Plan 333). Two
    configured clients can collide if they are configured with the same key. Merging them would
    silently drop a client from the count, and because the count is what a passive observer sees,
    a merge would also make the observable count disagree with the real one. The block build fails
    closed on a collision instead. **[spec]**
19. **The client count is in the clear and the entry order is not meaningful** (found while
    implementing Plan 333). The declared client count is a 2-byte big-endian field in the layer-1
    plaintext, which is encrypted under the layer-1 key but derivable by anyone who can decrypt
    that layer — which is everyone, since the layer-1 key needs no credential. It is followed by
    fixed-size 40-byte entries, so the block's total length is `34 + count × 40`. A passive
    observer learns *how many* clients are subscribed and nothing about which, which is the
    property §11 asks for. Because the entries are fixed size, the block's length is derived from
    the count rather than by scanning, and entry order carries no meaning: recovery examines every
    entry, compares the identifier in constant time, and takes the first match. The order is
    randomized per publication when more than one client is configured, and a failing random source
    leaves the order as-is rather than aborting an otherwise valid publication. **[spec]**
20. **An authorized service's address must declare `B32_FLAG_REQUIRES_CLIENT_KEY`** (found while
    implementing Plan 333). The layer-1 flags byte already refuses a record whose credential is
    absent, but that check happens *after* a fetch. The address is what a prospective client reads
    first, and without this flag an authorized service is indistinguishable from an open one until
    the client has already pulled the record. The flag is therefore set at publication and
    preserved through resolution, and a resolver built for such an address refuses the
    unauthenticated path outright rather than discovering the requirement at decrypt time. **[spec]**
21. **A missing credential and a wrong credential are different errors** (found while implementing
    Plan 333). A client that supplies *no* credential gets a distinct, actionable error, because
    presenting any authorized key would fix it. A client that supplies a *wrong* one gets a refusal
    that is deliberately indistinguishable from presenting a key that was never configured: the
    identifier comparison is constant time and the error text carries no distance information, so
    an attacker learns nothing about how close a guess was. **[spec]**
22. **The four authorization secrets are 32 bytes each and are not interchangeable** (found while
    implementing Plan 333). Server PSK, server-side DH client public key, client PSK, and client DH
    private key are all 32 bytes. They are therefore carried with an explicit role tag, and a
    persistence layer that mixed them up would store a private key where a public one belongs.
    Persistence is a reversible, role-tagged 33-byte encoding that is explicitly *not* a wire
    format, a configuration serialization, or a password verifier: the protocol needs the secret on
    every publication, so a one-way verifier would be useless, and at-rest encryption remains the
    storage layer's responsibility and is not claimed here. **[spec]**
23. **The client-count bound is a policy cap, not a format limit, and the two references disagree**
    (found while implementing Plan 333). The wire format carries a 2-byte count, so the format
    permits up to 65 535 entries. i2pr bounds publication at 255, chosen so the count field and the
    entry array stay well inside a single bounded allocation; the pinned Emissary reference caps at
    99 for both schemes. Neither bound is derived from the specification, and they are not
    compatible at the top of the range: a record i2pr considers publishable at 200 clients is one
    Emissary refuses to parse. i2pr's bound is the looser one, which is the safer direction for a
    publisher and the more permissive one for a receiver. Any future work that reconciles the two
    must treat this as a policy decision, not a bug, and must not silently lower i2pr's bound to
    match a reference. **[compat]**

## 15. Ownership boundary of the future module

The Red25519 module owns: the typed scalars/keys/signature, alpha derivation, blinding, Red25519
sign/verify, and blinded storage-key derivation. It does **not** own: destination lifecycle, b33
text, LeaseSet framing, NetDB, Proposal 170, persistent algorithm registries, or any storage or
persistence policy. Those belong to Plans 332–334's owners, and each of those plans must consume
this module through its narrow public API rather than reaching into the curve layer.

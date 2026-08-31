# SAM 3.1 private destination format — normative provenance

Plan 136 records the SAM v3 private destination (`PUB`/`PRIV`) binary
format and its reconciliation with the existing i2pr identity model.
**Plan 142 corrects the Base64 alphabet** (RFC 4648 `+/` → I2P `-/~`,
keeping `=` padding) after Plan 140's audit identified the
non-conformant codec. This file is the provenance record for the SAM
codec and round-trip fixture tests in `crates/i2pr-api/`; it is not a
tutorial.

## Sources

- SAM v3 API specification:
  <https://geti2p.net/en/docs/api/samv3/>
  Pinned website commit: `88596022920bdf99f27db27688faf4f204792fcd`
- Common structures (Destination encoding, key-area layout, key
  certificate):
  <https://geti2p.net/spec/common-structures>
  Same pinned commit.
- Java I2P `PrivateKeyFile.java` (canonical `PRIV` concatenation and
  Base64 encoding):
  <https://github.com/i2p/i2p.i2p/blob/master/core/java/src/net/i2p/data/PrivateKeyFile.java>
- i2pd `libi2pd/Base.h` and `libi2pd/Base.cpp` (I2P Base64 alphabet
  `T64` with index 62 = `-`, index 63 = `~`, padding `P64 = '='`):
  <https://github.com/PurpleI2P/i2pd/blob/openssl/libi2pd/Base.h>
  <https://github.com/PurpleI2P/i2pd/blob/openssl/libi2pd/Base.cpp>
- i2plib Python SAM client (`I2P_B64_CHARS = "-~"`):
  <https://github.com/tomi/i2plib/blob/master/i2plib/sam.py>
- txi2p Python SAM client (independent reference, validates I2P
  Base64 with `=` padding):
  <https://github.com/majestrate/txi2p>

## Profile

This document covers the i2pr Ed25519+X25519 profile exclusively:

| Field | Value |
| --- | --- |
| SIGNATURE_TYPE | 7 (`EdDsaSha512Ed25519`) |
| CRYPTO_TYPE | 4 (`X25519`) |

Other key/certificate combinations are out of scope for Plan 136.

## `PUB` — canonical Destination encoding

The `PUB` value returned by `sam session create` (or `SAMConfig`) is
the standard Destination binary structure. For SIGNATURE_TYPE=7 /
CRYPTO_TYPE=4, the total encoded length is **391 bytes**.

### Encoding layout

The Destination encoding is the 384-byte key area followed by the key
certificate. The key area is defined by the common-structures constants:

```text
KEY_AREA_SIZE        = 384
LEGACY_PUBLIC_KEY_SIZE = 256
LEGACY_SIGNING_KEY_SIZE = 128
```

For X25519 (32-byte public key) and Ed25519 (32-byte public signing
key), the key area is laid out as:

```text
Offset  Length  Field
------  ------  -----
0       32      X25519 encryption public key
32      320     identity padding (384 - 32 - 32 = 320 bytes)
352     32      Ed25519 signing public key (32 bytes, <= LEGACY_SIGNING_KEY_SIZE)
```

The key certificate follows the 384-byte key area:

```text
Offset  Length  Field
------  ------  -----
384     1       certificate type (5 = KeyCertificate)
385     2       payload length (4 bytes, big-endian)
387     2       signing type code (7 = EdDsaSha512Ed25519)
389     2       crypto type code (4 = X25519)
```

Total certificate: **7 bytes**.

**Total `PUB` length: 384 + 7 = 391 bytes.**

### Key-area padding

The 320-byte padding region is present because the I2P key area is
fixed at 384 bytes regardless of key type. For Ed25519+X25519, the
two real key slots occupy 64 bytes; the remaining 320 bytes are
random padding. The padding participates in the Destination SHA-256
hash and must be preserved exactly on round-trip.

### Certificate no-excess rule

For Ed25519 (32 bytes) and X25519 (32 bytes), both public key
lengths are at or below their respective legacy sizes (128 and 256).
The key certificate carries zero excess key bytes — the payload is
exactly 4 bytes (two type codes). This is the `for_types` canonical
form produced by `KeyCertificate::for_types`.

## `PRIV` — private destination encoding

The SAM `PRIV` value is the Destination binary encoding concatenated
with the two private keys. The concatenation order is fixed and
non-negotiable:

```text
Offset  Length  Field
------  ------  -----
0       391     Destination (same bytes as PUB)
391     32      X25519 encryption private key
423     32      Ed25519 signing private key (seed)
```

**Total binary length: 391 + 32 + 32 = 455 bytes.**

### Field ordering

1. **Destination public bytes** (391 bytes) — the complete canonical
   Destination encoding, identical to `PUB`.
2. **Encryption private key** (32 bytes) — the X25519 static secret
   used for ECIES destination encryption.
3. **Signing private key** (32 bytes) — the Ed25519 seed.

The signing private key is the 32-byte seed, not the 64-byte
expanded Ed25519 signature key. The full 64-byte key is derived at
signing time by the Ed25519 implementation. The private destination
carries only the seed.

### X25519 private key semantics

The 32-byte X25519 private key is the actual static X25519 secret
used for ECIES destination encryption — not random data and not zero.
The I2P common-structures key-type table specifies 32-byte X25519 for
crypto type 4. The "encryption keys unused since 0.6" comment in
`PrivateKeyFile.java` refers to legacy ElGamal destinations; for
ECIES type-4 destinations the private key is meaningful and must
round-trip exactly.

### Ed25519 signing private key representation

EdDSA-Ed25519 (type 7) signing private keys are represented as a
32-byte little-endian seed. The full 64-byte Ed25519 signature key
(scalar || point) is derived at signing time by the ed25519-dalek
(or equivalent) implementation. The `PRIV` encoding carries only the
32-byte seed.

### Offline-signature section: unsupported

The Java `PrivateKeyFile` format allows an offline-signing trailer
appended after the signing private key. When present, the signing
private key field is 32 bytes of zero, followed by:

```text
Expires           4 bytes, big-endian
TransientSigType  2 bytes, big-endian
TransientSigningPublicKey  variable length
Signature         variable length
TransientSigningPrivateKey  variable length
```

This offline-signature mechanism is **explicitly unsupported** in the
i2pr M7 baseline (Plan 136 only). The SAM codec in `crates/i2pr-api/`
rejects any trailing data after byte 455 (the destination and two
private keys). Any `PRIV` value with length > 455 bytes is a decode
error.

## Base64 encoding

SAM uses the **I2P Base64** alphabet for the `PUB` and `PRIV` values:

```text
Alphabet: A-Z a-z 0-9 - ~     (slots 62 and 63 are '-' and '~')
Padding:  = (standard ASCII '=')
```

This is **the same alphabet** used by every Java I2P / i2pd /
independent Python client reference implementation. It is **not**
standard RFC 4648 Base64 (`+`/`/`), and it is also **not** the I2P
Base64 variant that uses `~` for padding (the router-hash codec in
`i2pr-netdb::base64` uses that variant — SAM uses `=` for padding).

### Independent corroborating references

The SAM Base64 alphabet is independently confirmed against three
reference implementations, none of which informed the others:

1. **i2pd** (`PurpleI2P/i2pd`, `openssl` branch):
   - `libi2pd/Base.h::IsBase64(char)` accepts only the SAM/I2P
     alphabet plus the `=` padding character.
   - `libi2pd/Base.cpp` exposes `T64` with index 62 mapped to `-`
     and index 63 mapped to `~`, and the padding character
     `P64 = '='`.
2. **Java I2P** (`i2p/i2p.i2p`):
   - `core/java/src/net/i2p/data/PrivateKeyFile.java` decodes
     `PrivateKeyFile` payloads using `Base64.decode(...)` against
     the SAM/I2P alphabet (`-`/`~` substitution table) with `=`
     padding.
3. **i2plib** (`tomi/i2plib`, Python SAM client):
   - `i2plib/sam.py` builds the SAM alphabet with
     `I2P_B64_CHARS = "-~"` and passes it to Python's `base64`
     decoder via `altchars=("-~")` with `validate=True`, so `=`
     padding is accepted and any deviation is rejected.

Plan 142 corrects the prior implementation's RFC 4648 alphabet (the
implementation diverged from the spec and from Plan 136's own
acceptance criteria). All three references post-date Plan 136 but
are unchanged on the relevant alphabet positions.

### Base64 length

```text
Binary length:     455 bytes
Base64 length:     ceil(455 / 3) * 4 = 152 * 4 = 608 characters
Padding:           455 % 3 = 1, so two trailing '=' characters
```

The `PUB` value encodes to `ceil(391 / 3) * 4 = 524` characters.

## Round-trip invariant

The standard SAM `PRIV` encoding must preserve enough information for
`DestinationIdentity::from_private_bytes` to reconstruct:

1. The same `DestinationId` (SHA-256 hash of canonical Destination
   encoding).
2. The same Ed25519 signing private key.
3. The same X25519 static private key.

The reconstruction path:

```text
PRIV bytes (455)
  → Destination::decode(bytes[0..391])        // public structure
  → DestinationId::from_hash(dest.hash())     // identifier
  → signing_seed = bytes[423..455]            // Ed25519 seed
  → x25519_secret = bytes[391..423]          // X25519 static secret
  → padding = dest.key_and_cert().padding()   // 320 bytes from destination
  → DestinationIdentity::from_private_bytes(
        signing_seed, x25519_secret, padding
    )
```

The Destination is re-encoded internally by `from_private_bytes` to
derive the public key from each private key, so the decoded
Destination structure from the `PRIV` bytes is only needed for the
hash — not for the key material.

## Worked example (test vector inputs)

The following inputs define a frozen test vector for SIGNATURE_TYPE=7
with deterministic bytes. The tests in `crates/i2pr-api/` will
compute the exact Destination encoding and SHA-256 hash; this
document records the construction and expected byte lengths.

```text
signing_private_seed = 0x07 repeated 32 times
x25519_static_secret = 0x09 repeated 32 times
identity_padding    = 0x5a repeated 320 bytes
```

Expected byte lengths:

```text
Destination encoding  = 391 bytes
PRIV binary           = 391 + 32 + 32 = 455 bytes
PRIV Base64           = 608 characters (with == padding)
Destination SHA-256   = SHA-256(Destination encoding bytes)
```

The `DestinationIdentity::from_private_bytes` constructor in
`crates/i2pr-client/src/identity.rs` accepts these three inputs
and reconstructs the exact same `DestinationId`. The SAM codec must
produce a `PRIV` value from which these three inputs can be
extracted and passed to the same constructor. The test fixture will
verify byte-identical round-trip.

## Reconciles to existing i2pr types

The SAM codec does not introduce new identity types. It uses the
existing reconstruction path in `crates/i2pr-client/src/identity.rs`:

| Method | Role in SAM codec |
| --- | --- |
| `DestinationIdentity::from_private_bytes(signing, static_secret, padding)` | Accepts the three extracted fields (Ed25519 seed, X25519 secret, 320-byte padding) and reconstructs the full identity. This is the seam Plan 136 wraps. |
| `Destination::decode(bytes, maximum)` | Decodes the 391-byte Destination from the front of the `PRIV` value for hash computation. |
| `Destination::hash()` | Returns the SHA-256 of the canonical encoding — the `DestinationId`. |
| `DestinationIdentity::id()` | Returns the `DestinationId` after reconstruction. |

The destination is 391 bytes because of the 7-byte Key Certificate
(signing type=7, crypto type=4) appended to the 384-byte key area.
The key area layout (32-byte X25519 public + 320-byte padding +
32-byte Ed25519 public) is dictated by the common-structures spec
and enforced by `KeyAndCert::validate` in `i2pr-proto`.

Plan 136 does **not** change the identity model. It adds the SAM
codec that uses the existing `from_private_bytes` reconstruction
path.

## Provenance discipline

The i2pr `PRIV` format is the standard SAM `PrivateKeyFile`
concatenation. No i2pr-only PRIV format is invented. The format is
verified against four independent references, none of which depend
on the i2pr SAM codec:

1. **Java I2P `PrivateKeyFile.java`** — the canonical concatenation
   order (Destination || encryption private || signing private) and
   `Base64.encode(...)` against the I2P Base64 substitution table.
2. **i2pd `libi2pd/Base.{h,cpp}`** — the SAM alphabet is encoded
   as `T64` with slot 62 = `-` and slot 63 = `~`, with padding
   `P64 = '='`.
3. **i2plib `i2plib/sam.py`** — the SAM alphabet is built with
   `I2P_B64_CHARS = "-~"` and validated through Python's standard
   `base64` decoder using `altchars=("-~")` with `validate=True`.
4. **SAM specification prose** — the `CREATE` command returns
   `PUB` (Destination) and `PRIV` (PrivateKeyFile Base64). The spec
   does not define a separate encoding; it defers to the
   common-structures Destination and `PrivateKeyFile`.

## Test fixture requirements

The SAM codec tests in `crates/i2pr-api/` must include:

1. **Frozen round-trip fixture** — a single set of signing seed,
   X25519 secret, and padding bytes that encode to `PUB`/`PRIV`,
   then decode back to an identity with an identical `DestinationId`
   and identical key material. The fixture bytes are committed in the
   test source.

2. **Independent reference vectors** — golden vectors derived from
   the three independent reference implementations above (i2pd's
   `Base.cpp`, Java I2P's `PrivateKeyFile`, and i2plib's
   `I2P_B64_CHARS`) are encoded into `crates/i2pr-api/tests/`. The
   vectors lock the alphabet, padding, and length behavior against
   any future regression, and they are independent of the i2pr
   codec because the reference implementations define the alphabet
   themselves. The plan-of-record also commits the corpus used to
   derive these vectors (the pinned reference revisions listed
   above).

Both fixtures use SIGNATURE_TYPE=7 / CRYPTO_TYPE=4. The frozen
fixture uses the deterministic inputs documented in the worked-example
section above; the independent reference vectors use short payloads
that hit every alphabet slot.

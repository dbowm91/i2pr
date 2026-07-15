# ADR 0011: NTCP2 cryptographic composition and static-key persistence

- Status: Accepted for the experimental Plan 032 foundation
- Date: 2026-07-15
- Scope: `i2pr-transport-ntcp2` crypto/transcript stages and `i2pr-storage` transport key material

## Context

Plan 032 needs the exact NTCP2 cryptographic composition without allowing a
generic Noise library, runtime, socket, or filesystem dependency to leak into
the Tokio-free transport crate. The current protocol target is the NTCP2
specification accurate for I2P 0.9.69, Noise revision 33, RFC 7748, and the
primitive references named by `specs/SOURCES.md`.

The protocol-specific parts are protocol composition: the I2P Noise name and
initial hash, cleartext AES-CBC ephemeral-key state, HMAC-SHA256 KDF labels,
SessionConfirmed's reuse of the SessionRequest cipher state at nonce 1, the
directional Split, and SipHash-2-4 length material. These must not be hidden
behind a general-purpose provider API.

## Decision

### Primitive dependencies

Direct dependencies use default features disabled unless the feature is listed:

| Crate | Version | Use | Enabled features | MSRV/license/unsafe review |
| --- | --- | --- | --- | --- |
| `x25519-dalek` | 2.0.1 | X25519 key derivation and DH, through `i2pr-crypto` | `static_secrets`, `zeroize` | below Rust 1.85; BSD-3-Clause; reviewed dependency owns any unsafe |
| `sha2` | 0.10 workspace range | SHA-256 transcript/hash output | workspace defaults | below Rust 1.85; MIT OR Apache-2.0; no local primitive |
| `hmac` | 0.12.1 | HMAC-SHA256 Noise KDF steps | none | below Rust 1.85; MIT OR Apache-2.0; no local primitive |
| `chacha20poly1305` | 0.10.1 | ChaCha20-Poly1305 AEAD | `alloc` only | RustCrypto, MIT OR Apache-2.0; no getrandom/default feature; primitive code remains dependency-owned |
| `aes` | 0.8.4 | AES-256 block function for protocol CBC wiring | none | RustCrypto, MIT OR Apache-2.0; no local AES primitive |
| `siphasher` | 1.0.3 | exact SipHash-2-4 length masks | `default-features = false` | MIT OR Apache-2.0; avoids deprecated standard-hasher behavior |
| `subtle` | 2.6 workspace range | constant-time byte equality | none | below Rust 1.85; MIT OR Apache-2.0; no local primitive |
| `zeroize` | 1.8 workspace range | secret-owner erasure | `derive` | below Rust 1.85; MIT OR Apache-2.0; private owners implement no `Clone`, `Debug`, serde, or `Display` |

The lockfile records the exact resolved versions. `cargo-deny` remains the
authoritative advisory, ban, source, and license gate. No `noise-protocol`
crate is selected: the available generic state-machine API would not provide
the required evidence and exact SessionConfirmed cipher-state reuse without
making the I2P extension opaque. The rejected alternative was to fork or copy
primitive/state-library code, which is prohibited.

### Noise and transcript model

The implementation selects the narrow protocol-composition approach. It owns
only consuming transcript stages and delegates every primitive operation:

- protocol initialization hashes the exact
  `Noise_XKaesobfse+hs2+hs3_25519_ChaChaPoly_SHA256` name, empty prologue, and
  responder static key;
- `Transcript` binds the responder static key and an explicit role;
- SessionRequest and SessionCreated use HMAC-derived cipher states;
- cleartext padding is mixed only when non-empty;
- SessionConfirmed part one consumes the retained SessionRequest cipher state
  at nonce 1, then the `se` KDF creates the part-two cipher state;
- `split` consumes the handshake transcript and returns directional AEAD and
  SipHash owners; nonce `2^64 - 1` is never emitted.

Intermediate hashes are exposed only through the deliberately hidden evidence
method. Secret owners have no formatting, cloning, serialization, or public
byte access. The module contains no clock, RNG, socket, filesystem, Tokio, or
generic cryptographic-provider API.

### Static key and IV persistence

`i2pr-storage::TransportStaticKeyStore` stores version-1 material at
`ntcp2.static.key` in the existing private router data directory. The record is
an independent X25519 private key, its rederived public key, a 16-byte
obfuscation IV, fixed lengths/algorithm identifiers, and a SHA-256 checksum.
It is 132 bytes, strictly consumed, bounded, symlink-checked, permission
checked, zeroizing for private buffers, and installed by atomic same-directory
no-replace hard-link semantics. Generation accepts an injected RNG and fails
if the target exists; there is no silent replacement or rotation.

The key and IV are kept stable across immediate restarts because changing
either changes the published RouterAddress contract. Rotation, publication,
RouterInfo mutation, downtime policy, and migration are deferred to the
address/publication and later handshake plans. The storage record is not the
router identity record and the NTCP2 static key is not derived from the
RouterIdentity.

## Evidence and limitations

`tests/fixtures/ntcp2/crypto/vectors.tsv` contains synthetic values generated
independently with Python `cryptography` 41.0.7 plus hashlib/HMAC. It covers
X25519, transcript initialization and all three message stages, AEAD, AES-CBC,
and split material. This is independent deterministic crypto-composition
evidence, not Java I2P/i2pd interoperability evidence. The support ledger
therefore remains experimental and non-advertised.

The fixture manifest and `scripts/check-ntcp2-vectors.sh` enforce exact hashes
and one-to-one file coverage. No operational key, public address, or live
capture is committed.

## Rejected alternatives and review triggers

- A generic Noise state library was rejected because exact I2P state reuse,
  stage evidence, and obfuscation boundaries were not sufficiently explicit.
- `hkdf` was not added as a generic API because NTCP2 specifies the exact
  HMAC-SHA256 two-output sequencing and custom `ask`/`siphash` labels; those
  labels are composed locally over `hmac`.
- AES-CBC mode wiring is local protocol composition over the reviewed `aes`
  block primitive; AES rounds and key schedule are not reimplemented.
- A key/IV rotation policy, encrypted-at-rest format, and public RouterInfo
  integration are deferred rather than guessed.

Revisit this ADR for a primitive major-version update, changed default
features, an advisory or MSRV change, a specification revision affecting
transcript bytes, an independent-router vector discrepancy, or before exposing
complete handshake/runtime behavior.

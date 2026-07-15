# `i2pr-proto` — Deep Dive

The foundational protocol crate. Owns all bounded wire-level codecs for
I2P common structures and the initial I2NP message surface.

Path: `crates/i2pr-proto/`

## Purpose

`i2pr-proto` is the bottom of the dependency chain. It provides:

- **Common-structure codecs**: `Mapping`, `Hash`, `Date`/`Date32`,
  `SigningKeyType`/`CryptoKeyType`, `PublicKey`, `SigningPublicKey`,
  `SignatureValue`, `Certificate`/`KeyCertificate`, `KeyAndCert`,
  `RouterIdentity`, `Destination`, `RouterAddress`, `RouterInfo`, `Lease`,
  classic `LeaseSet`.
- **I2NP wire codecs**: headers (`Standard`/`ShortSsu`/`ShortTransport`),
  body registry (`I2npBody`), framing, `DeliveryStatusMessage`,
  `TunnelDataMessage`/`TunnelGatewayMessage`/`DeferredBuildRecords`,
  `DatabaseStore`/`DatabaseLookup`/`DatabaseSearchReply`,
  `ReplySecret<N>` (zeroizing), `DeferredPayload`, `OpaqueMessageBody`.

It owns framing and structural validation. Anything requiring later
cryptography, state machines, or interpretation is stored as a bounded
opaque payload (`DeferredPayload` / `OpaqueMessageBody` /
`DeferredBuildRecords`) with bytes redacted in `Debug`.

It does **not** own: routing, transport state machines, NetDB behavior,
tunnel build execution, crypto policy, runtime integration, I/O.

## Module layout

The crate is single-directory with two top-level submodules under
`src/`:

| File | Responsibility | Main public items |
| --- | --- | --- |
| `src/lib.rs` | Crate root: `ProtocolErrorKind`, `Namespace`, glob re-exports | `ProtocolErrorKind`, `Namespace` |
| `src/codec.rs` | Primitive bounded codec mechanics | `CodecError`, `DecodeCursor<'a>`, `EncodeBuffer<'a>`, `decode_exact`, `encode_to_vec` |
| `src/common/mod.rs` | Bounds constants, helpers, glob re-exports | `MAX_COMMON_STRUCTURE_SIZE`, `MAX_MAPPING_BODY_SIZE`, `MAX_ROUTER_ADDRESSES`, `MAX_LEASES`, `MAX_ENCRYPTION_KEYS`, `Mapping`, `Hash`, `Date`, `Date32`, key/cert/identity types, `RouterAddress`, `RouterInfo`, `Lease`, `LeaseSet` |
| `src/common/mapping.rs` | Canonical sorted Java-style `Mapping` | `Mapping`, `MappingEntry`, `MappingBuilder` |
| `src/common/hash.rs` | 32-byte `Hash` (SHA-256) | `Hash` |
| `src/common/date.rs` | `Date` (8-byte ms) and `Date32` (4-byte s) | `Date`, `Date32` |
| `src/common/keys.rs` | Typed key types with algorithm-policy enforcement | `SigningKeyType`, `CryptoKeyType`, `PublicKey`, `SigningPublicKey`, `SignatureValue` |
| `src/common/certificate.rs` | `Certificate` enum and `KeyCertificate` | `Certificate`, `KeyCertificate` |
| `src/common/identity.rs` | 384-byte `KeyAndCert` plus `RouterIdentity`/`Destination` | `KeyAndCert`, `RouterIdentity`, `Destination` |
| `src/common/router_address.rs` | Transport-address record | `RouterAddress` |
| `src/common/router_info.rs` | Signed descriptor with retained `signed_bytes` | `RouterInfo`, `ProtocolVersion`, `Capabilities` |
| `src/common/lease.rs` | Classic `Lease`/`LeaseSet` with deferred variants explicitly rejected | `Lease`, `LeaseSet`, `DeferredLeaseSetVariant`, `decode_lease_set_variant` |
| `src/i2np/mod.rs` | I2NP wire constants and glob re-exports | `MAX_I2NP_PAYLOAD_SIZE`, `STANDARD_HEADER_SIZE`, `SHORT_SSU_HEADER_SIZE`, `SHORT_TRANSPORT_HEADER_SIZE`, `MAX_DATABASE_LOOKUP_EXCLUDED_PEERS`, `MAX_DATABASE_SEARCH_REPLY_PEERS`, `MAX_BUILD_RECORDS`, `VARIABLE_BUILD_RECORD_SIZE`, `SHORT_BUILD_RECORD_SIZE`, `TUNNEL_DATA_PAYLOAD_SIZE` |
| `src/i2np/header.rs` | Three-variant header enum, `MessageType` registry | `MessageType`, `I2npHeader` |
| `src/i2np/message.rs` | Top-level dispatch + 14-variant `I2npBody` | `I2npBody`, `I2npMessage` |
| `src/i2np/delivery.rs` | `DeliveryStatusMessage` body | `DeliveryStatusMessage` |
| `src/i2np/tunnel.rs` | Tunnel data, gateway, deferred build records | `TunnelDataMessage`, `TunnelGatewayMessage`, `DeferredBuildRecords` |
| `src/i2np/netdb.rs` | `DatabaseStore`, `Lookup`, `SearchReply`, `ReplyEncryption`, zeroizing `ReplySecret<N>` | `DatabaseStoreType`, `DatabaseStoreData`, `DatabaseStoreMessage`, `DatabaseLookupMessage`, `DatabaseSearchReplyMessage`, `ReplyEncryption`, `ReplySecret<N>` |
| `src/i2np/deferred.rs` | Bounded opaque payloads | `DeferredPayload`, `OpaqueMessageBody` |

Integration tests live in `tests/i2np_fixtures.rs`.

## Public surface

### Crate-root items (`src/lib.rs`)
- `enum ProtocolErrorKind` — `lib.rs:20`
- `enum Namespace` — `lib.rs:39`
- Re-exports of `codec`, `common`, `i2np` items

### Codec primitives (`src/codec.rs`)
- `enum CodecError` — 10 variants, all carry offset and static context
  (`codec.rs:24`)
- `struct DecodeCursor<'a>` — `codec.rs:179`
- `struct EncodeBuffer<'a>` — `codec.rs:360`
- `fn decode_exact()` — `codec.rs:349`
- `fn encode_to_vec()` — `codec.rs:552`

### Notable constants
- `MAX_COMMON_STRUCTURE_SIZE`, `MAX_MAPPING_BODY_SIZE`,
  `MAX_ROUTER_ADDRESSES`, `MAX_LEASES`, `MAX_ENCRYPTION_KEYS`
- `MAX_I2NP_PAYLOAD_SIZE` (62,708 — tighter than the spec's 64 KiB due
  to tunnel fragmentation)
- `TUNNEL_DATA_PAYLOAD_SIZE` (1024)

## Key data structures

| Structure | Location | Role |
| --- | --- | --- |
| `CodecError` | `codec.rs:24` | 10-variant bounded error type. No attacker-controlled bytes carried. |
| `DecodeCursor<'a>` | `codec.rs:179` | Borrowed cursor over `&[u8]` with mandatory input cap and checked arithmetic. |
| `EncodeBuffer<'a>` | `codec.rs:360` | Bounded encoder over `&mut Vec<u8>` with caller-visible output cap. |
| `Mapping` | `common/mapping.rs:43` | Sorted, duplicate-free. Canonical order via Java UTF-16 comparison. |
| `Hash` | `common/hash.rs:7` | 32-byte SHA-256 digest; `Debug` redacts bytes. |
| `Date`/`Date32` | `common/date.rs:7,52` | Pure value types; freshness interpretation deferred. |
| `PublicKey`/`SigningPublicKey`/`SignatureValue` | `common/keys.rs` | Enforce algorithm-specific length at construction. |
| `KeyAndCert`/`RouterIdentity`/`Destination` | `common/identity.rs` | Padding bytes retained verbatim for hash input. |
| `RouterInfo` | `common/router_info.rs:60` | Retains `signed_bytes` for sign-verifiers to operate on the original wire form. |
| `I2npHeader` | `i2np/header.rs:90` | Three header variants. |
| `I2npBody` | `i2np/message.rs:7` | 14 body variants; Garlic/Data/Build hold deferred payloads. |
| `I2npMessage` | `i2np/message.rs:128` | Top-level dispatch; three decode paths, three encode paths. |
| `TunnelGatewayMessage` | `i2np/tunnel.rs` | Nesting a full `I2npMessage` (recursive decode path). |
| `ReplySecret<N>` | `i2np/netdb.rs:90` | Non-cloneable, zeroizing wrapper. `Debug` redacts bytes. Memory hygiene only — no crypto. |

## Codec architecture

Pipeline:

1. `decode_exact(input, maximum, closure)` creates a `DecodeCursor`
   with the mandatory cap, invokes the closure, then `finish()`
   rejects trailing bytes. Strict, top-level entry point.
2. Cursor reads use checked arithmetic; length-prefixed reads enforce
   a caller-supplied cap before consuming the declared length.
3. `EncodeBuffer` enforces the output cap on every write, including
   length-prefixed fields. `write_raw()` is `pub(crate)` for
   already-validated internal callers.
4. Every structure exposes `decode(input, maximum)` and
   `encode_to_vec(maximum)`. There is no hidden unlimited policy.
5. Signed-region preservation: `RouterInfo` and `LeaseSet` retain the
   exact signed bytes; encode writes them verbatim followed by the
   signature, eliminating reserialization artifacts.
6. Deferred semantics: bodies requiring later crypto or state machines
   are stored as `DeferredPayload`/`OpaqueMessageBody`/
   `DeferredBuildRecords` — bounded but opaque. Their `Debug` shows
   only length.

### `CodecError` taxonomy (10 variants)

`Truncated`, `LengthExceeded`, `ArithmeticOverflow`, `InvalidUtf8`,
`InvalidFieldValue`, `NonCanonical`, `Unsupported`, `TrailingBytes`,
`DuplicateField`, `PolicyRejected`. Each maps to a `ProtocolErrorKind`
via `CodecError::kind()`.

## Dependencies and boundary compliance

- Direct deps: `sha2` (workspace), `zeroize` (workspace).
- `#![forbid(unsafe_code)]` at the crate root (`lib.rs:8`).
- No `tokio`, no `async`, no `std::net`/`std::fs`, no transport
  imports, no runtime/routing code, no `unbounded_channel`,
  no dependency on `i2pr-testkit`.

## Tests

- `src/lib.rs:57-78` — `Namespace::as_str()` and `ProtocolErrorKind` distinctness.
- `src/codec.rs:608-917` — 18 cursor/encoder/round-trip/redaction/error-display tests.
- `src/common/mod.rs:97-319` — 8 tests on Mapping, Hash, dates, certificates, identity truncation, RouterInfo and Lease round-trips.
- `src/i2np/message.rs:966-1147` — 10 tests on body registry, headers, fixture vectors, search-reply bounding, DH mode rejection, deferred redaction.
- `tests/i2np_fixtures.rs` — Loads hex fixtures via `include_str!` from `tests/fixtures/i2np/`. Tests:
  - every positive fixture decodes and re-encodes canonically;
  - positive fixture truncations fail without panics;
  - malformed fixtures produce typed errors;
  - `ReplySecret` debug is redacted and only memory hygiene is claimed.
- Fixture corpus: `tests/fixtures/i2np/` — 31 hex fixtures + manifest
  (`manifest.tsv`) + README. Covers all header variants, major body
  types, and 16 malformed inputs.
- Manifest integrity checked by `scripts/check-fixture-manifest.sh`.

## Distinctive design choices

- **Mandatory `maximum` at every call site** — limits cannot be hidden
  behind a default.
- **Signed-region retention** — round-trip is bit-identical to the
  original wire form, critical for cryptographic verification.
- **Java UTF-16 ordering for Mappings** — subtle interop requirement;
  `java_string_cmp` helper at `common/mod.rs:56`.
- **`ReplySecret<N>` is non-cloneable and zeroizing** — explicit memory
  hygiene, no derived `Debug`. Holds only the secret, no crypto API.
- **Deferred payloads are bounded but opaque** — the crate owns
  framing, not interpretation.
- **`CryptoKeyType::allowed_in_identity()`** — only `ElGamal` and
  `X25519` currently permitted in identity encryption.
- **Key-certificate excess material** — `KeyCertificate` stores
  `excess_signing`/`excess_crypto` byte vectors to support longer
  algorithm keys without truncation.

## Cross-references

- [Overview](overview.md)
- [Conformance](../../specs/CONFORMANCE.md)
- [Protocol support matrix](../../specs/support.toml)
- [sources](../../specs/SOURCES.md)
- Plan-of-record for codec work: `plans/011-m1-codec-foundation.md`
  and its closure at `plans/011-closure.md`

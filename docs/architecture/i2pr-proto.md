# `i2pr-proto` — Deep Dive

The foundational protocol crate. Owns the bounded wire-level codecs for
I2P common structures, the initial I2NP message surface, the ECIES Garlic
payload block form, the i2pd-compatible I2CP-style Data body, and the
Streaming packet/payload envelope.

Path: `crates/i2pr-proto/`

## Purpose

`i2pr-proto` is the bottom of the workspace dependency chain — it has zero
workspace production dependencies. It provides:

- **Common-structure codecs**: `Mapping`, `Hash`, `Date`/`Date32`,
  `SigningKeyType`/`CryptoKeyType`, `PublicKey`, `SigningPublicKey`,
  `SignatureValue`, `Certificate`/`KeyCertificate`, `KeyAndCert`,
  `RouterIdentity`, `Destination`, `RouterAddress`, `RouterInfo`, classic
  `Lease`/`LeaseSet`, the Standard LeaseSet2 family
  (`Lease2`, `LeaseSet2Header`, `LeaseSet2EncryptionKey`, `LeaseSet2`,
  `MetaLease`, `MetaLeaseSet`, `OfflineSignature`; signature domain
  `0x03 || signed_bytes`), the type-5 encrypted-LeaseSet2 outer layer
  (`EncryptedLeaseSet2`, `EncryptedLeaseSet2OfflineKeys`), and RFC 4648
  base 32 with the encrypted-service (`b33`) address form.
- **I2NP wire codecs**: headers (`Standard` / `ShortSsu` / `ShortTransport`),
  the 15-variant body registry (`I2npBody`), `I2npMessage` framing and
  dispatch, `DeliveryStatusMessage`,
  `TunnelDataMessage`/`TunnelGatewayMessage`/`TunnelTestMessage`/
  `DeferredBuildRecords`, `DatabaseStore`/`DatabaseLookup`/
  `DatabaseSearchReply`, `ReplySecret<N>`, `DeferredPayload`,
  `OpaqueMessageBody`.
- **ECIES Garlic payload block codec** (Plan 121, with the Plan 193 block-type
  correction; `src/ecies_payload.rs`): the bounded structural payload block
  codec consumed by the destination session layer in `i2pr-client`. It encodes
  DateTime, Garlic Clove (Local and Destination delivery), Padding, Options,
  and Ack Request, and rejects Termination and MessageNumbers.
- **I2CP-style Data body codec** (Plan 192, `src/i2cp_data_body.rs`): the
  i2pd-compatible Data body layout used on the SAM `STYLE=RAW` /
  `STYLE=DATAGRAM VERSION=3` inbound-delivery path, plus a helper that wraps
  one in an I2NP `Data` body inside a standard-header `I2npMessage`.
- **Streaming wire codecs** (Plans 125/128, `src/streaming/`): the normative
  Streaming packet form and the protocol-6 gzip client payload envelope that
  wraps every Streaming packet carried inside an I2NP `Data` body.

It owns framing and structural validation. Anything requiring later
cryptography, state machines, or interpretation is stored as a bounded
opaque payload (`DeferredPayload` / `OpaqueMessageBody` /
`DeferredBuildRecords` / `OpaqueMessageBody`) with bytes redacted in `Debug`.

It does **not** own: routing, transport state machines, NetDB behavior,
tunnel build execution, crypto policy, runtime integration, I/O,
destination lifecycle, destination keys, ECIES session state, ECIES
ephemeral key generation, the ECIES HKDF transcript, or destination
tunnel pools. The LeaseSet2 / encrypted-LeaseSet2 codecs and their
signature-domain bytes are structural; private destination signing material
stays out of `i2pr-proto`. The ECIES Garlic block codec is structural —
encryption, session ratchet, and ephemeral key handling stay in
`i2pr-crypto` (primitives) and `i2pr-client` (manager). Likewise, the
Streaming modules are wire-only; connection policy and state live in
`i2pr-client`.

## Module layout

Line counts are from `wc -l` against the current tree. The crate is
single-directory with **three** top-level submodules under `src/`
(`common/`, `i2np/`, `streaming/`) plus three flat files
(`codec.rs`, `ecies_payload.rs`, `i2cp_data_body.rs`):

| Module | File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- | --- |
| — | `src/lib.rs` | 84 | Crate root: `ProtocolErrorKind`, `Namespace`, glob re-exports | `ProtocolErrorKind`, `Namespace` |
| — | `src/codec.rs` | 917 | Primitive bounded codec mechanics | `CodecError`, `DecodeCursor<'a>`, `EncodeBuffer<'a>`, `decode_exact`, `encode_to_vec` |
| `common` | `src/common/mod.rs` | 326 | Bounds constants, `java_string_cmp`, glob re-exports | `MAX_COMMON_STRUCTURE_SIZE`, `MAX_MAPPING_BODY_SIZE`, `MAX_ROUTER_ADDRESSES`, `MAX_LEASES`, `MAX_ENCRYPTION_KEYS` |
| `common::mapping` | `src/common/mapping.rs` | 235 | Canonical sorted Java-style `Mapping` | `Mapping`, `MappingEntry`, `MappingBuilder` |
| `common::hash` | `src/common/hash.rs` | 57 | 32-byte `Hash` (SHA-256) | `Hash` |
| `common::date` | `src/common/date.rs` | 82 | `Date` (8-byte ms) and `Date32` (4-byte s) | `Date`, `Date32` |
| `common::keys` | `src/common/keys.rs` | 274 | Typed key types with algorithm-policy enforcement | `SigningKeyType`, `CryptoKeyType`, `PublicKey`, `SigningPublicKey`, `SignatureValue` |
| `common::certificate` | `src/common/certificate.rs` | 194 | `Certificate` enum and `KeyCertificate` | `Certificate`, `KeyCertificate` |
| `common::identity` | `src/common/identity.rs` | 368 | `KeyAndCert` plus `RouterIdentity`/`Destination` | `KeyAndCert`, `RouterIdentity`, `Destination` |
| `common::router_address` | `src/common/router_address.rs` | 78 | Transport-address record | `RouterAddress` |
| `common::router_info` | `src/common/router_info.rs` | 262 | Signed descriptor with retained `signed_bytes` | `RouterInfo`, `ProtocolVersion`, `Capabilities` |
| `common::lease` | `src/common/lease.rs` | 299 | Classic `Lease`/`LeaseSet` plus explicit variant classification | `Lease`, `LeaseSet`, `DeferredLeaseSetVariant`, `decode_lease_set_variant`, `decode_lease_set2_variant` |
| `common::lease2` | `src/common/lease2.rs` | 1789 | Standard LeaseSet2 family: carrier, header, flags, keys, offline signature, MetaLeaseSet | `Lease2`, `LeaseSet2`, `LeaseSet2Header`, `LeaseSet2Flags`, `LeaseSet2EncryptionKey`, `OfflineSignature`, `MetaLease`, `MetaLeaseSet`, `LeaseSet2BuildError`, `LeaseSet2HeaderError`, `LeaseSet2EncryptionKeyError`, `LeaseSet2KeySelectionError`, `LEASE_SET2_SIGNATURE_DOMAIN_BYTE`, `LEASE_SET2_DATABASE_STORE_TYPE`, `LEASE2_WIRE_SIZE`, `MAX_META_LEASES`, `MAX_LEASE_SET2_*` |
| `common::base32` | `src/common/base32.rs` | 789 | RFC 4648 base 32, CRC-32, encrypted-service (`b33`) address form (Plan 332) | `EncryptedServiceAddress`, `Base32Error`, `crc32`, `base32_encode`, `base32_decode`, `is_encrypted_service_address`, `B32_SUFFIX`, `B32_HASH_CHARS`, `B32_SERVICE_CHARS`, `B32_SERVICE_WIDE_CHARS`, `B32_FLAG_*`, `B32_UNBLINDED_SIGTYPE_*`, `B32_BLINDED_SIGTYPE` |
| `common::els2` | `src/common/els2.rs` | 662 | DatabaseStore type 5: encrypted-LeaseSet2 outer-layer framing, offline-key block, exact signature preimage (Plan 332) | `EncryptedLeaseSet2`, `EncryptedLeaseSet2Flags`, `EncryptedLeaseSet2OfflineKeys`, `ENCRYPTED_LEASE_SET2_STORE_TYPE`, `ENCRYPTED_LEASE_SET2_BLINDED_SIGTYPE`, `ENCRYPTED_LEASE_SET2_UNBLINDED_SIGTYPES`, `ENCRYPTED_LEASE_SET2_FLAG_OFFLINE_KEYS`, `ENCRYPTED_LEASE_SET2_FLAGS_RESERVED_MASK`, `ENCRYPTED_LEASE_SET2_MAX_EXPIRES_OFFSET`, `ENCRYPTED_LEASE_SET2_SALT_LENGTH`, `ENCRYPTED_LEASE_SET2_MIN_OUTER_CIPHERTEXT_LENGTH`, `INNER_LEASE_SET2_STORE_TYPE`, `INNER_META_LEASE_SET2_STORE_TYPE` |
| `i2np` | `src/i2np/mod.rs` | 79 | I2NP wire constants and glob re-exports | `MAX_I2NP_PAYLOAD_SIZE`, `STANDARD_HEADER_SIZE`, `SHORT_SSU_HEADER_SIZE`, `SHORT_TRANSPORT_HEADER_SIZE`, `MAX_DATABASE_LOOKUP_EXCLUDED_PEERS`, `MAX_DATABASE_SEARCH_REPLY_PEERS`, `MAX_BUILD_RECORDS`, `VARIABLE_BUILD_RECORD_SIZE`, `SHORT_BUILD_RECORD_SIZE`, `SHORT_REQUEST_PLAINTEXT_SIZE`, `SHORT_REPLY_PLAINTEXT_SIZE`, `SHORT_BUILD_EPHEMERAL_KEY_LEN`, `SHORT_BUILD_NONCE_LEN`, `SHORT_BUILD_TAG_LEN`, `TUNNEL_DATA_PAYLOAD_SIZE`, `TUNNEL_TEST_BODY_SIZE` |
| `i2np::header` | `src/i2np/header.rs` | 142 | Three-variant header enum, `MessageType` registry | `MessageType`, `I2npHeader` |
| `i2np::message` | `src/i2np/message.rs` | 1383 | Top-level dispatch + 15-variant `I2npBody` | `I2npBody`, `I2npMessage` |
| `i2np::delivery` | `src/i2np/delivery.rs` | 22 | `DeliveryStatusMessage` body | `DeliveryStatusMessage` |
| `i2np::tunnel` | `src/i2np/tunnel.rs` | 124 | Tunnel data, gateway, peer test, deferred build records | `TunnelDataMessage`, `TunnelGatewayMessage`, `TunnelTestMessage`, `DeferredBuildRecords` |
| `i2np::netdb` | `src/i2np/netdb.rs` | 230 | `DatabaseStore`, `Lookup`, `SearchReply`, `ReplyEncryption`, zeroizing `ReplySecret<N>` | `DatabaseStoreType`, `DatabaseStoreData`, `DatabaseStoreMessage`, `DatabaseLookupMessage`, `DatabaseSearchReplyMessage`, `ReplyEncryption`, `ReplySecret<N>` |
| `i2np::deferred` | `src/i2np/deferred.rs` | 47 | Bounded opaque payloads | `DeferredPayload`, `OpaqueMessageBody` |
| — | `src/ecies_payload.rs` | 800 | Bounded structural ECIES Garlic payload block codec (Plan 121, Plan 193 correction) | `EciesPayloadSequence`, `EciesPayloadBlock`, `GarlicCloveBlock`, `GarlicDelivery`, `MAX_ECIES_PAYLOAD_BYTES`, `MAX_ECIES_PAYLOAD_BLOCKS`, `MAX_GARLIC_CLOVE_BODY`, `MAX_PADDING_BODY`, `BLOCK_TYPE_*` |
| — | `src/i2cp_data_body.rs` | 922 | i2pd-compatible I2CP-style Data body codec (Plan 192) | `I2cpDataBody`, `I2cpDataBodyDecodeError`, `I2cpDataBodyEncodeError`, `encode_i2cp_data_body`, `decode_i2cp_data_body`, `encode_destination_data_envelope`, `PROTOCOL_TYPE_STREAMING`/`_DATAGRAM`/`_RAW`/`_DATAGRAM2`/`_DATAGRAM3`, `I2CP_DATA_BODY_*`, `MAX_I2CP_DATA_BODY_PAYLOAD` |
| `streaming` | `src/streaming/mod.rs` | 42 | Streaming module wiring; explicit `pub use` of the `packet` and `payload` surfaces | (re-exports only) |
| `streaming::packet` | `src/streaming/packet.rs` | 1586 | Streaming packet wire codec (Plan 128 normative form) | `StreamingPacket`, `StreamingPacketBuilder`, `StreamingFlags`, `StreamingOptions`, `StreamingOptionDecodeContext`, `StreamingHeaderPeek`, `SignatureLocation`, `StreamingPacketError`, `StreamingReceiveLimit`, `StreamingSendLimit`, `peek_streaming_header`, `decode_streaming_packet`, `encode_streaming_packet`, `build_signature_preimage`, `install_packet_signature`, `install_packet_signature_at`, `encode_syn_replay_binding`, `verify_syn_replay_binding`, `validate_initial_syn`, `validate_syn_response`, `validate_signature_policy`, `MIN_STREAMING_HEADER_BYTES`, `DEFAULT_ADVERTISED_MAX_PAYLOAD`, `MAX_STREAMING_PAYLOAD_BYTES`, `MAX_STREAMING_OPTION_BYTES`, `MAX_STREAMING_NACK_COUNT`, `SYN_REPLAY_NACK_COUNT`, `MAX_STREAMING_PACKET_BYTES`, `FLAG_*`, `INITIAL_SYN_FLAGS`, `SYN_RESPONSE_FLAGS`, `CLOSE_FLAGS`, `RESET_FLAGS` |
| `streaming::payload` | `src/streaming/payload.rs` | 745 | Protocol-6 gzip client payload envelope (Plan 125) | `ClientPayload`, `encode_client_payload`, `decode_client_payload`, `ClientPayloadDecodeError`, `ClientPayloadEncodeError`, `STREAMING_PROTOCOL_NUMBER`, `DEFAULT_DESTINATION_PORT`, `DEFAULT_SOURCE_PORT`, `MAX_CLIENT_PAYLOAD_BYTES`, `MAX_APPLICATION_PAYLOAD_BYTES`, `GZIP_MAGIC`, `GZIP_CM_DEFLATE` |

Integration tests live in `tests/i2np_fixtures.rs` (I2NP hex-fixture
coverage), `tests/lease_set2_fixture.rs` (Plan 119 LeaseSet2 frozen-fixture
plus i2np-level round-trip coverage, and the Plan 332 type-5
`EncryptedLeaseSet2` layer-0 round trip), and `tests/plan128_wire.rs`
(Plan 128 §11 wire fixtures pinning the normative flag map, exact SYN
layout, TLV absence, raw final-signature placement, and the
zeroed-signature preimage).

### Plan 332: base 32 and the encrypted-LeaseSet2 outer layer

`common/base32.rs` and `common/els2.rs` are the two modules Plan 332 added.
Both are framing-only; neither performs cryptography, because `i2pr-proto`
may not depend on any `i2pr-*` crate.

**`base32.rs`** owns RFC 4648 base 32, a CRC-32, and the encrypted-service
(`b33`) address form. The ordinary 52-character hash form is left untouched:
it is still accepted, still emitted, and never reinterpreted. The
encrypted-service form is separate because it must carry the *unblinded
public key* and both signature types — a client cannot derive the daily
blinded key from a Destination hash.

Two properties are enforced on every decode:

- **Canonical encoding.** Unused trailing bits of the final base-32 group
  must be zero, and the decoded value must re-encode to exactly the input
  string. The second check matters more than it looks: the CRC-32 is folded
  into the leading three bytes, so those bytes are a function of the
  sigtypes and the key, and a bit flip there maps onto a *different valid
  address* rather than a failure. The re-encode check is what makes one
  address have exactly one text form. The CRC stays a typo detector, not a
  MAC, and the module documents that.
- **No secret material.** The flags say a blinding secret or a per-client
  key is *required*; neither is ever encoded.

**`els2.rs`** owns the type-5 layer-0 framing. The one piece of semantics it
keeps is `EncryptedLeaseSet2::signature_preimage()`: the exact region a
verifier must cover. The store-type byte is *not* part of the stored record
but *is* part of the signature, and a verifier that omitted it would accept a
record no other implementation produced. Everything else — the credential
and subcredential, the two layer key derivations, encryption, signature
verification, freshness policy — belongs to `i2pr-netdb`, which composes
this framing with the `i2pr-crypto` primitives.

### Plan 193: ECIES block-type correction

`ecies_payload.rs` uses `Garlic Clove` = block type **11** and `Options` =
block type **5**, per `geti2p.net/spec/ecies#blocks` and exact-pinned i2pd
2.61.0. The original Plan 121 codec used types `1` and `2`, which match an
early draft but not the published specification; i2pd 2.61.0 rejects block
type `1` as `Unknown block type` (it uses `1` for `Session ID`). `Ack
Request` (type 9) is parsed and skipped per the Plan 213 corrective.

## Public surface

`src/lib.rs` declares `codec`, `common`, `ecies_payload`, `i2cp_data_body`,
and `i2np` as private modules and `streaming` as a public module, then
re-exports:

```rust
pub use codec::{CodecError, DecodeCursor, EncodeBuffer, decode_exact, encode_to_vec};
pub use common::*;
pub use ecies_payload::*;
pub use i2cp_data_body::*;
pub use i2np::*;
pub use streaming::*;
```

Crate-root items are `ProtocolErrorKind` (7 variants: `Malformed`,
`Truncated`, `LimitExceeded`, `InvalidValue`, `TrailingBytes`,
`PolicyRejected`, `Unsupported`) and `Namespace` (`Common`, `I2np`, with
`as_str()`).

The four glob re-exports flatten `common` (12 submodules), `i2np`
(6 submodules), `ecies_payload`, `i2cp_data_body`, and `streaming` into a
single flat namespace at the crate root. There is no per-module path
disambiguation; `streaming::packet` and `streaming::payload` remain
reachable through the public `streaming` module.

## Key contracts

### Typed error enums

| Enum | Location | Notes |
| --- | --- | --- |
| `ProtocolErrorKind` | `lib.rs:26` | 7 coarse categories shared across the crate. |
| `CodecError` | `codec.rs:24` | 10 variants: `Truncated`, `LengthExceeded`, `ArithmeticOverflow`, `InvalidUtf8`, `InvalidFieldValue`, `NonCanonical`, `Unsupported`, `TrailingBytes`, `DuplicateField`, `PolicyRejected`. All carry an offset plus *static* context; no attacker-controlled bytes. Maps to `ProtocolErrorKind` via `kind()`. |
| `Base32Error` | `common/base32.rs:59` | Canonical-encoding and length failures. |
| `LeaseSet2BuildError` / `LeaseSet2HeaderError` / `LeaseSet2EncryptionKeyError` / `LeaseSet2KeySelectionError` | `common/lease2.rs:759` / `:422` / `:210` / `:144` | Expiration overflow, offline-signature/flag mismatch, empty key, no keys, duplicate X25519. |
| `I2cpDataBodyDecodeError` / `I2cpDataBodyEncodeError` | `i2cp_data_body.rs:146` / `:295` | Truncation, declared-length overflow, payload-too-large, gzip/deflate and CRC/ISIZE trailer failures. |
| `StreamingPacketError` | `streaming/packet.rs:314` | `TrailingBytes`, `Truncated`, `OptionOverflow`, `NackOverflow`, `PayloadOverflow`, reserved-bit and signature-policy failures. |
| `ClientPayloadEncodeError` / `ClientPayloadDecodeError` | `streaming/payload.rs:108` / `:142` | gzip framing, size, and application-payload limits. |

The ECIES payload codec has no dedicated error enum: it returns
`CodecError` directly, with `context` strings such as
`"ECIES first block must be DateTime"`.

### Bound constants

| Constant | Value | Location |
| --- | --- | --- |
| `MAX_COMMON_STRUCTURE_SIZE` | 1 MiB | `common/mod.rs:23` |
| `MAX_MAPPING_BODY_SIZE` | `u16::MAX` | `common/mod.rs:25` |
| `MAX_ROUTER_ADDRESSES` | 255 | `common/mod.rs:27` |
| `MAX_LEASES` | 16 | `common/mod.rs:29` |
| `MAX_ENCRYPTION_KEYS` | 8 | `common/mod.rs:31` — declared and public, but no current call site; the LS2 family enforces `MAX_LEASE_SET2_ENCRYPTION_KEYS` instead. |
| `LEASE2_WIRE_SIZE` | 40 | `common/lease2.rs:32` |
| `MAX_LEASE_SET2_ENCRYPTION_KEYS` | 8 | `common/lease2.rs:38` |
| `MAX_LEASE_SET2_ENCRYPTION_KEY_BYTES` | 8 KiB | `common/lease2.rs:43` |
| `MAX_LEASE_SET2_OPTIONS_BYTES` | `u16::MAX` | `common/lease2.rs:49` |
| `MAX_META_LEASES` | 16 | `common/lease2.rs:901` |
| `LEASE_SET2_SIGNATURE_DOMAIN_BYTE` | `0x03` | `common/lease2.rs:55` |
| `LEASE_SET2_DATABASE_STORE_TYPE` | `0x03` | `common/lease2.rs:898` |
| `ENCRYPTED_LEASE_SET2_SALT_LENGTH` | 32 | `common/els2.rs:73` |
| `MAX_I2NP_PAYLOAD_SIZE` | 62,708 | `i2np/mod.rs:19` — tighter than the spec's nominal 64 KiB because tunnel fragmentation constrains a message to ≈61.2 KiB. |
| `TUNNEL_DATA_PAYLOAD_SIZE` | 1024 | `i2np/mod.rs:62` |
| `TUNNEL_TEST_BODY_SIZE` | 12 | `i2np/mod.rs:65` |
| `MAX_DATABASE_LOOKUP_EXCLUDED_PEERS` | 512 | `i2np/mod.rs:27` |
| `MAX_DATABASE_SEARCH_REPLY_PEERS` | 16 | `i2np/mod.rs:29` |
| `MAX_ECIES_PAYLOAD_BYTES` | 65,507 | `ecies_payload.rs:52` |
| `MAX_ECIES_PAYLOAD_BLOCKS` | 64 | `ecies_payload.rs:57` |
| `MAX_GARLIC_CLOVE_BODY` | 60,000 | `ecies_payload.rs:60` |
| `MAX_PADDING_BODY` | 60,000 | `ecies_payload.rs:63` |
| `MAX_I2CP_DATA_BODY_PAYLOAD` | 61,440 | `i2cp_data_body.rs:94` |
| `I2CP_DATA_BODY_TOTAL_OVERHEAD` | 27 (19 header + 8 trailer) | `i2cp_data_body.rs:121` |
| `MIN_STREAMING_HEADER_BYTES` | 22 | `streaming/packet.rs:70` |
| `DEFAULT_ADVERTISED_MAX_PAYLOAD` / `MAX_STREAMING_PAYLOAD_BYTES` | 1730 | `streaming/packet.rs:74`, `:78` |
| `MAX_STREAMING_OPTION_BYTES` | 1024 | `streaming/packet.rs:86` |
| `MAX_STREAMING_NACK_COUNT` | 64 | `streaming/packet.rs:88` |
| `MAX_STREAMING_PACKET_BYTES` | 3032 (22 + 64·4 + 1024 + 1730) | `streaming/packet.rs:96` |
| `MAX_CLIENT_PAYLOAD_BYTES` | 61,440 | `streaming/payload.rs:97` |
| `MAX_APPLICATION_PAYLOAD_BYTES` | 61,376 | `streaming/payload.rs:99` |

### Ownership and decode rules

1. **Mandatory caller cap.** Every decode entry point takes an explicit
   `maximum`, and every encode entry point takes an explicit `maximum`.
   There is no hidden default limit.
2. **Exact consumption.** `decode_exact(input, maximum, closure)` builds a
   `DecodeCursor`, runs the closure, then `finish()` rejects trailing bytes.
   Length-prefixed reads enforce the caller cap *before* consuming the
   declared length.
3. **Checked arithmetic.** Cursor offsets and encoder output sizes use
   checked arithmetic; overflow surfaces as
   `CodecError::ArithmeticOverflow`.
4. **`write_raw` is `pub(crate)`.** `EncodeBuffer::write_raw` is crate-internal
   and used only for already-validated byte regions.
5. **Signed-region preservation.** `RouterInfo`, `LeaseSet`, `LeaseSet2`,
   `MetaLeaseSet`, and `EncryptedLeaseSet2` retain the exact bytes preceding
   their signature and re-emit them verbatim, so round-trips are
   bit-identical and a verifier never sees a reserialized value.
6. **Signature preimages are explicit.** `LeaseSet2::signature_preimage()`
   returns `LEASE_SET2_SIGNATURE_DOMAIN_BYTE || signed_bytes`.
   `EncryptedLeaseSet2::signature_preimage()` returns the retained region
   including the store-type byte. `OfflineSignature::signed_bytes()` and
   `build_signature_preimage` cover the offline and Streaming cases.
7. **Deferred semantics.** Bodies needing later crypto or state machines are
   held as `DeferredPayload` / `OpaqueMessageBody` / `DeferredBuildRecords` —
   bounded, opaque, and `Debug`-redacted to a length only.
8. **Zeroizing secret wrapper.** `ReplySecret<N>` wraps `Zeroizing<[u8; N]>`
   and its `Debug` prints only the length. It is `Clone` (a manual impl), so
   it is a *memory-hygiene* wrapper, not a secret-management boundary; it
   exposes no derivation or encryption API.
9. **Policy-gated struct variants.** `DatabaseStoreData` structurally
   decodes types 0/1/3/5/7 (`RouterInfoCompressed`, `LeaseSet`, `LeaseSet2`,
   `EncryptedLeaseSet`, `MetaLeaseSet`); anything else is retained in the
   `Deferred { store_type, payload }` variant. `common/lease.rs` additionally
   exposes `decode_lease_set_variant` / `decode_lease_set2_variant`, which
   decode only their own store type and return `CodecError::Unsupported` for
   the others rather than guessing.
10. **Secret-free redaction.** No `Debug`/`Display` on the crate prints
    payload bytes, key bytes, or secret material.

### Codec pipeline

1. `decode_exact(input, maximum, closure)` — strict top-level entry point.
2. Cursor reads use checked arithmetic; length-prefixed reads enforce a
   caller-supplied cap before consuming the declared length.
3. `EncodeBuffer` enforces the output cap on every write, including
   length-prefixed fields.
4. Every structure exposes `decode(input, maximum)` and
   `encode_to_vec(maximum)`.
5. `I2npMessage` has three decode paths (`decode_standard`,
   `decode_short_ssu`, `decode_short_transport`) and three matching encode
   paths; `TunnelGatewayMessage` nests a full `I2npMessage`, so the decode
   path is recursive.

## Dependencies

`scripts/check-dependency-direction.sh` lists `i2pr-proto` with an **empty**
allowlist: it must have zero workspace production dependencies.

**Production dependencies** (external only):

| Crate | Purpose |
| --- | --- |
| `flate2` (workspace) | gzip/deflate for the protocol-6 Streaming client payload envelope and the I2CP Data body. |
| `sha2` (workspace) | `Hash::digest` and identity/destination hashing. |
| `zeroize` (workspace) | `ReplySecret<N>` and other bounded secret wrappers. |

**Dev-dependencies** (not part of the boundary contract):

| Crate | Purpose |
| --- | --- |
| `i2pr-crypto` (path `../i2pr-crypto`) | **Dev-only.** Lets `tests/lease_set2_fixture.rs` verify a LeaseSet2 signature against the embedded destination signing key. It is a test-only edge and does not appear in the production dependency graph. |
| `rand_chacha` (workspace) | Deterministic test byte generation. |
| `rand_core` (workspace) | Seed plumbing for the deterministic test RNG. |

Boundary compliance:

- `#![forbid(unsafe_code)]` at `lib.rs:8`; also re-asserted inside
  `ecies_payload.rs:43`, `i2cp_data_body.rs:59`, and `streaming/mod.rs:19`.
- No `tokio`, no `async`, no `std::net` / `std::fs`, no transport imports,
  no runtime or routing code, no unbounded channels.
- No dependency on `i2pr-testkit`.

## Tests

### Inline unit tests

| File | Test block | Tests | Coverage |
| --- | --- | --- | --- |
| `src/lib.rs` | `:62-83` | 2 | `Namespace::as_str()`; `ProtocolErrorKind` distinctness. |
| `src/codec.rs` | `:607-917` | 20 | Cursor network-order reads, length-prefix caps, `finish()` strictness, encoder caps, round-trips, `Debug` redaction, `Display`/error mapping, and a `test_support` helper module (`:562`) supplying deterministic bytes, truncation prefixes, trailing-byte, and bit-flip mutators. |
| `src/common/mod.rs` | `:102-326` | 9 | `Mapping` canonical sort/duplicate rejection, primitive vectors, unknown algorithm paths, `KeyCertificate` excess material, identity/destination round-trips, identity truncation at every prefix, `RouterAddress`, `RouterInfo` signed-region retention plus trailing-byte rejection, `Lease`/`LeaseSet` round-trip and `decode_lease_set_variant` rejection. |
| `src/common/lease2.rs` | `:1219-1789` | 27 | Plan 119 phases A–E: Lease2 exact 40-byte wire length, big-endian order, truncation and trailing bytes, checked time conversion, header round-trip, reserved/unsupported flag errors, expiration overflow, typed key handling, X25519 wrong length, zero keys, duplicate X25519 determinism, bounded aggregate key bytes, canonical options ordering, `signed_bytes` preservation, noncanonical mapping cannot gain a valid signature, full round-trip, signature preimage with the prepended `0x03` domain byte, and negative paths. |
| `src/common/base32.rs` | — | 13 | Base 32 round-trip, canonical-encoding enforcement, CRC-32, `b33` text forms. |
| `src/common/els2.rs` | — | 9 | Type-5 layer-0 round-trip, flags, offline keys, signature preimage with the store-type byte. |
| `src/i2np/message.rs` | `:1070-1383` | 14 | Body registry ↔ `MessageType` mapping, all three header variants, fixture vectors, search-reply bounding, DH-mode rejection, deferred `Debug` redaction. |
| `src/ecies_payload.rs` | `:466-800` | 20 | Round-trip with DateTime + Clove + Padding; oversized clove rejection (`> MAX_GARLIC_CLOVE_BODY`); oversized payload rejection (`> MAX_ECIES_PAYLOAD_BYTES` = 65,507); truncated header; truncated clove body; DateTime-not-first; padding-then-non-padding; unknown delivery flag; the `MAX_ECIES_PAYLOAD_BLOCKS` guard; the malformed entry-count guard; block-type constants against the published ECIES spec. |
| `src/i2cp_data_body.rs` | `:640-922` | 14 | Data body round-trip, protocol-constant rejection, declared-length and payload-bound rejection, gzip trailer/CRC/ISIZE validation, and the destination data envelope. |
| `src/streaming/packet.rs` | — | 12 | Plan 128 flag map, SYN layout, replay binding, signature placement. |
| `src/streaming/payload.rs` | — | 14 | Protocol-6 gzip envelope round-trip and bounded negative paths. |

### Integration tests

- `tests/i2np_fixtures.rs` (4 tests) — loads hex fixtures via `include_str!`
  from the repository-level `tests/fixtures/i2np/` directory. Asserts every
  positive fixture decodes and re-encodes canonically, every positive
  truncation fails without panicking, malformed fixtures produce typed
  errors, and `ReplySecret` `Debug` is redacted.
- `tests/lease_set2_fixture.rs` (12 tests) — Plan 119 LeaseSet2 integration:
  frozen LS2 round-trip through the `LeaseSet2` codec; signature verification
  against the embedded destination signing key (this is the dev-only
  `i2pr-crypto` edge); usable X25519 selection; duplicate X25519 deterministic
  rejection; i2np-level `DatabaseStore` round-trip through the standard and
  short-transport envelopes with the `0x03` signature-domain byte present in
  the wire form; Plan 332 type-5 `EncryptedLeaseSet2` layer-0 round-trip;
  `Deferred` retention for later store types; reserved/unsupported flags
  yielding typed errors.
- `tests/plan128_wire.rs` (11 tests) — Plan 128 §11 wire fixtures: the
  normative flag map, the policy flag sets for current packet shapes, the
  size constants separating payload from packet bounds, the exact initial-SYN
  layout, SYN response with zero NACKs and no `NO_ACK` bit, an option region
  free of synthetic TLV tags, the raw final-signature option field and its
  zeroed-signature preimage, normative option ordering, fail-closed
  rejection of trailing option garbage, header peek routing without option
  parsing, and rejection of a nonzero placeholder signature tail.

### Fixture corpus

- `tests/fixtures/i2np/` (repository level) — 31 hex fixtures plus
  `manifest.tsv` and `README.md`, covering all header variants, major body
  types, and 16 malformed (negative) inputs.
- Manifest integrity is checked by `scripts/check-fixture-manifest.sh`.
- Streaming wire fixtures follow
  [specs/references/streaming-packet-wire.md](../../specs/references/streaming-packet-wire.md).

Determinism: no test uses a wall-clock sleep or a nondeterministic RNG.
`codec.rs`'s `test_support` module supplies a fixed LCG seed generator, and
the dev-dependency `rand_chacha` is used with fixed seeds.

## Distinctive design choices

- **Mandatory `maximum` at every call site** — decode and encode limits are
  always caller-supplied, so no policy can be hidden behind a default.
- **Signed-region retention** — round-trip is bit-identical to the original
  wire form, which is what makes cryptographic verification of a decoded
  record meaningful.
- **Explicit signature domains** — `LeaseSet2` prepends `0x03` and
  `EncryptedLeaseSet2` includes the store-type byte, so a preimage cannot be
  confused with the stored region.
- **Java UTF-16 ordering for `Mapping`** — a subtle interop requirement;
  `java_string_cmp` at `common/mod.rs:56`.
- **`ReplySecret<N>` is zeroizing with a redacted `Debug`** — memory hygiene
  without a secret-management API, and the docs claim only hygiene.
- **Deferred payloads are bounded but opaque** — the crate owns framing, not
  interpretation; `Debug` shows length only.
- **Unknown values are never defaulted** — `SigningKeyType::from_code` and
  `CryptoKeyType::from_code` map unrecognized codes to `Unknown(code)` and
  raise `CodecError::Unsupported` where a decision is required.
- **Algorithm policy at construction** — `CryptoKeyType::allowed_in_identity()`
  (`common/keys.rs:151`, crate-internal) permits only `ElGamal` and `X25519`
  as identity encryption keys, even though `CryptoKeyType` also names
  `MlKem512X25519` / `MlKem768X25519` / `MlKem1024X25519`.
- **Key-certificate excess material** — `KeyCertificate` stores
  `excess_signing`/`excess_crypto` byte vectors so longer algorithm keys
  survive round-trips without truncation.
- **Canonical base-32 re-encode check** — the decode path re-encodes and
  compares, which is what gives an address exactly one text form; the CRC-32
  stays a typo detector, not a MAC.
- **The Streaming and ECIES codecs are wire-only** — they never carry
  ephemeral keys, session state, or AEAD ciphertexts; the corresponding
  state machines live in `i2pr-crypto` and `i2pr-client`.

## Cross-references

Architecture and specifications:

- [Architecture overview](overview.md)
- [Dependency graph and allowlist](dependency-graph.md)
- [Conformance](../../specs/CONFORMANCE.md)
- [Protocol support matrix](../../specs/support.toml)
- [Sources](../../specs/SOURCES.md)
- [Streaming packet wire reference](../../specs/references/streaming-packet-wire.md)

Related deep dives:

- [i2pr-crypto.md](i2pr-crypto.md) — the primitives `i2pr-proto` deliberately
  does not implement, and the crate's dev-only test dependency.
- [i2pr-netdb.md](i2pr-netdb.md) — RouterInfo/LeaseSet2 validation and store,
  which composes the structural codecs with `i2pr-crypto`.
- [i2pr-client.md](i2pr-client.md) — destination lifecycle, ECIES session and
  ratchet, Streaming state machine.
- [i2pr-storage.md](i2pr-storage.md) — identity and key persistence.
- [i2pr-transport.md](i2pr-transport.md) and
  [i2pr-transport-ntcp2.md](i2pr-transport-ntcp2.md) — transport crates that
  consume the I2NP codecs.

Decisions:

- [ADR 0001 — modular monolith](../adr/0001-modular-monolith.md)
- [ADR 0002 — Tokio runtime boundary](../adr/0002-tokio-runtime-boundary.md)
- [ADR 0004 — router identity algorithms](../adr/0004-router-identity-algorithms.md)
- [ADR 0005 — crypto dependency selection](../adr/0005-crypto-dependency-selection.md)
- [ADR 0006 — private identity storage](../adr/0006-private-identity-storage.md)
- [ADR 0010 — transport contracts and crate boundaries](../adr/0010-transport-contracts-and-crate-boundaries.md)
- [ADR 0029 — anonymity boundaries and profile convergence](../adr/0029-anonymity-boundaries-and-profile-convergence.md)

Plans of record:

- Plan 011 (codec foundation) — `plans/implementation/workspace-foundation/011-m1-codec-foundation.md`,
  closure at `plans/closure/workspace-foundation/011-closure.md`
- Plan 119 (LeaseSet2) — `plans/implementation/destination-streaming/119-m6-leaseset2-protocol-foundation.md`,
  closure at `plans/closure/destination-streaming/119-status.md`
- Plan 121 (ECIES Garlic session layer) — `plans/implementation/destination-streaming/121-m6-ecies-garlic-session-layer.md`,
  closure at `plans/closure/destination-streaming/121-status.md`
- Plan 125 (Streaming corrective) — closure at `plans/closure/destination-streaming/125-status.md`
- Plan 128 (Streaming wire protocol) — closure at `plans/closure/destination-streaming/128-status.md`
- Plan 192 (I2CP wire format) — `plans/implementation/mixed-router-interop/192-m6-i2cp-wire-format-corrective.md`,
  closure at `plans/closure/mixed-router-interop/192-status.md`
- Plan 193 (i2pd mixed-router Streaming, block-type correction) — closure at
  `plans/closure/mixed-router-interop/193-status.md`
- Plan 213 (Ack Request handling corrective) — closure at
  `plans/closure/service-tunnels/213-status.md`
- Plan 332 (base 32 and encrypted-LeaseSet2) — `plans/implementation/i2pcontrol-proposal-170/332-encrypted-leaseset2-foundation.md`,
  closure at `plans/closure/i2pcontrol-proposal-170/332-status.md`

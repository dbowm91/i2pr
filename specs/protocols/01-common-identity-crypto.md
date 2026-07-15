# Common structures, identities, and cryptography

Status: **required**  
Primary roadmap milestone: **1**  
Feeds: every later milestone

## Scope

This dossier covers the shared binary structures and cryptographic type system used throughout I2P: integers, dates, strings, mappings, hashes, certificates, signature and encryption keys, RouterIdentity, Destination, RouterAddress, RouterInfo, Lease and LeaseSet-family records, signed-data boundaries, Base32/Base64 encodings, and protocol-specific key wrappers.

It does not define the higher-level state machines for NetDB, tunnels, garlic routing or transports. Those dossiers consume these types.

## Authoritative sources

- [Common structures specification](https://i2p.net/en/docs/specs/common-structures/), pinned in [SOURCES.md](../SOURCES.md).
- [Current ECIES specification](https://i2p.net/en/docs/specs/ecies/).
- [ECIES router specification](https://i2p.net/en/docs/specs/ecies-routers/).
- [Encrypted LeaseSet specification](https://i2p.net/en/docs/specs/encryptedleaseset/).
- [Legacy low-level cryptography overview](https://i2p.net/en/docs/specs/cryptography/) only for formats still referenced by current protocols.
- Proposals 145, 161 and 169 where incorporated by current specifications.

The official common-structures snapshot is accurate for 0.9.68 and includes X25519 plus ML-KEM/hybrid type identifiers introduced for 0.9.67-era protocols. The legacy cryptography page explicitly identifies itself as mostly obsolete. New code must follow the protocol-specific current documents rather than treating that page as a complete crypto specification.

## Required MVP subset

### Primitive encodings

Implement strict, allocation-bounded codecs for:

- unsigned network-byte-order integers of each width actually used;
- 8-byte millisecond dates, including explicit handling of zero/undefined;
- one-byte-length UTF-8 strings with the specification’s byte-length limit;
- mappings/properties with canonical serialization and duplicate-key policy;
- 32-byte hashes and router/destination hash derivation;
- I2P Base64 and Base32 forms needed by APIs, files and service adapters;
- certificates and key certificates with type/length validation.

Use width-specific Rust newtypes instead of a generic integer parser wherever a field has a fixed width. Parsing must reject impossible lengths before allocation and must not normalize signed bytes before verification.

### Identity and signed records

Implement:

- `RouterIdentity` and its certificate/key-type interpretation;
- `Destination` and destination hash/address derivation;
- `RouterAddress`, including transport style, cost, expiration and options;
- `RouterInfo`, including published timestamp, address collection, peer/options fields, capabilities and signature;
- `Lease`, `Lease2` and LeaseSet-family envelopes required by Milestones 4 and 6;
- offline-signature structures needed by current destinations/LeaseSets and streaming;
- exact signed-byte ranges and verification-before-use rules.

Decoded records from disk, reseed bundles and the network must pass the same structural, signature, timestamp and policy validation. A previously trusted cache entry is not trusted after restart merely because it was written locally.

### Cryptographic types

The initial generated router identity should use the current interoperable ECIES-X25519 router encryption and a current broadly deployed signature type selected by the Milestone 1 plan. The choice must be justified from current RouterInfo interoperability, not copied from another router’s default.

Required wrappers include:

- signature public/private keys and signatures keyed by signature type;
- X25519 static and ephemeral keys with little-endian protocol encoding;
- ChaCha20-Poly1305 keys/nonces/tags as protocol-specific types;
- SHA-256, HMAC and HKDF inputs with domain-specific constructors;
- legacy key types only where the MVP must decode deployed records or construct mixed-peer tunnel builds.

Secret types must not implement revealing `Debug`, serialization intended for logs, accidental cloning, or implicit conversion to ordinary byte vectors.

## Compatibility and deferral

- **Legacy ElGamal RouterIdentity generation:** legacy-reject/deferred. Current official material marks ElGamal router identities deprecated. Parse only where a later compatibility requirement demonstrates need.
- **ElGamal destination/LeaseSet support:** compatibility decision for Milestone 6. ECIES-only destinations simplify implementation but may narrow interoperability. Resolve with current network and service targets before claiming general destination compatibility.
- **ML-KEM hybrid/PQ key types:** compatibility watch. Type identifiers and lengths must not crash parsers; full generation and handshakes are deferred until a milestone plan establishes deployment requirements and reviewed Rust crypto dependencies.
- **MetaLeaseSet and advanced service records:** required-later or deferred according to NetDB and service requirements. Preserve type-safe rejection of unsupported records.
- **Unknown future types:** reject before type-dependent allocation or crypto. Do not guess lengths.

## Implementation references

- Java I2P: `core/java/src/net/i2p/data`, `core/java/src/net/i2p/crypto`, and router I2NP data classes.
- I2P+: corresponding `core` and `router` packages; inspect recent validation and caching differences.
- i2pd: `libi2pd/RouterInfo.cpp`, identity/data structures and crypto helpers.
- Emissary/go-i2p: common-structure, crypto and I2NP packages under `lib`.

Compare serialization output, signed ranges, mapping ordering, timestamp validation, type-length tables and unsupported-type behavior. Do not inherit Java serialization/object assumptions.

## Required tests

- Fixed byte vectors for every primitive and signed record.
- Boundary vectors at zero, maximum and maximum-plus-one lengths/counts.
- Truncation at every field boundary.
- Invalid UTF-8, duplicate mapping keys, noncanonical mappings and trailing bytes.
- Key-certificate/type combinations with wrong key or signature lengths.
- RouterInfo and LeaseSet signature mutation tests.
- Timestamp tests for zero, expiry, excessive future time and clock skew.
- Differential RouterInfo/Destination hashes and encodings against at least Java I2P and i2pd fixtures.
- Property tests asserting encode length, canonical round trip and no panic.
- Fuzz targets for RouterInfo, Destination, LeaseSet variants, certificates and mappings.
- Secret-redaction tests for errors, tracing and debug output.

## Open decisions before Milestone 1 implementation

1. Exact signature type for newly generated router and destination identities.
2. Minimum LeaseSet variants that must be fully parsed, validated, stored and emitted at the first interoperable checkpoint.
3. Whether legacy ElGamal destination encryption is required for the MVP’s intended service compatibility.
4. How preserved signed bytes and normalized semantic views are represented without duplicate unbounded storage.
5. Initial policy for hybrid/PQ records: recognize-and-reject, parse-and-store opaque, or fully validate signatures while deferring decryption.
6. Maximum accepted RouterInfo size, address count, mapping size and LeaseSet lease count, reconciled against the current specification and deployed routers.

## Current i2pr structural implementation

Plan 012 implements the bounded structural subset in
`crates/i2pr-proto/src/common.rs`: Date/Date32, Hash, typed public/signing
material and signatures, Mapping, Certificate/KeyCertificate,
RouterIdentity, Destination, RouterAddress, RouterInfo, Lease, and classic
LeaseSet. The module follows the pinned source in `specs/SOURCES.md`, uses a
1 MiB caller-visible common-structure ceiling, the specification's 65,535-byte
Mapping body ceiling, 255 RouterAddress/peer entries, and 16 classic leases.

Parsed RouterInfo and LeaseSet values retain the exact bytes before their
signatures. No signature verification, key generation, secret material,
timestamp freshness policy, transport option interpretation, or capability
advertisement is present. LeaseSet2, MetaLeaseSet, and EncryptedLeaseSet are
rejected explicitly until later plans define their crypto and NetDB semantics.
Local fixed bytes and malformed/boundary tests are not interoperability
evidence; the support ledger therefore remains `experimental` and
`advertised = false`.

## Plan 013 execution boundary

Plan 013 selects I2P signature type 7 (Ed25519) and router encryption type 4
(X25519) for newly generated identities. `crates/i2pr-crypto` wraps reviewed
Rust implementations, accepts an injected cryptographic RNG, zeroizes private
wrappers, signs the exact retained RouterInfo signed region, and verifies it
through the public identity. `crates/i2pr-storage` persists only the explicit
version-1 private identity format documented by ADR 0006.

This is local execution evidence only. No RouterInfo capability or version is
advertised, no transport or NetDB behavior is enabled, and the support ledger
remains non-advertised and experimental until authoritative vectors and mixed
router interoperability evidence exist.

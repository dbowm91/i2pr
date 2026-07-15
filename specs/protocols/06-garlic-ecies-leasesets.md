# Garlic routing, ECIES, and LeaseSets

Status: **required**  
Primary roadmap milestone: **6**  
Dependencies: common structures, I2NP, NetDB and destination tunnel pools

## Scope

This dossier covers end-to-end destination communication: garlic messages and cloves, delivery instructions, ECIES-X25519-AEAD-Ratchet sessions, router-directed ECIES where required, LeaseSet creation/publication/lookup/refresh, encrypted LeaseSets, replay controls and local destination routing.

Transport and tunnel encryption protect individual links and paths. Garlic/ECIES protects messages end-to-end between destinations or to the specified router role. These layers must use distinct key types and state.

## Authoritative sources

- [ECIES-X25519-AEAD-Ratchet](https://i2p.net/en/docs/specs/ecies/), pinned in [SOURCES.md](../SOURCES.md), updated 2025-06 and accurate for 0.9.67.
- [ECIES for routers](https://i2p.net/en/docs/specs/ecies-routers/).
- [Encrypted LeaseSet](https://i2p.net/en/docs/specs/encryptedleaseset/).
- [Common structures](https://i2p.net/en/docs/specs/common-structures/) and [I2NP](https://i2p.net/en/docs/specs/i2np/) for LeaseSet2-family and Garlic message structures.
- [Garlic routing overview](https://i2p.net/en/docs/overview/garlic-routing/).
- Proposals 123, 144, 145 and the current hybrid/PQ specification where applicable.

The pinned ECIES specification identifies deployment as complete and replaces ElGamal/AES+SessionTags for current end-to-end encryption. It also lists protocol features not implemented by Java I2P as of its stated version; `i2pr` must not implement optional sections merely because they appear in the document without checking current deployment and interoperability.

## Required destination model

A local destination owns:

- Destination identity and signing key;
- one or more LeaseSet encryption keys/types;
- current inbound/outbound tunnel pools;
- published and pending LeaseSets;
- ECIES session/ratchet state;
- replay and duplicate filters;
- application protocol dispatch such as streaming or datagrams;
- lifecycle and resource budgets independent of the router identity.

Destination shutdown must cancel LeaseSet publication/lookups, tunnel pools, ratchet sessions and application streams without affecting unrelated destinations.

## LeaseSet requirements

Implement the LeaseSet variant(s) selected by the Milestone 6 plan, including:

- exact signed structure and canonical byte preservation;
- destination/signing identity binding;
- encryption keys and key-type list validation;
- leases containing tunnel gateway, tunnel ID and expiration;
- offline signatures where supported/required;
- publication, lookup, refresh-before-expiry and withdrawal-by-expiry behavior;
- bounded lease count and lifetime;
- replacement/conflict policy for newer records;
- NetDB storage and encrypted reply integration.

LeaseSet2 should be the baseline candidate because current ECIES keys are carried there. Legacy LeaseSet support is a compatibility decision, not an automatic default. Encrypted LeaseSets and MetaLeaseSets should be enabled only after their access-control/service semantics are implemented and tested.

## ECIES session requirements

Implement current crypto type 4 destination encryption with:

- New Session and Existing Session container formats;
- X25519 static/ephemeral key agreement as specified;
- exact transcript/KDF labels and associated data;
- ChaCha20-Poly1305 authenticated encryption;
- receive-tag/ratchet generation, consumption and expiry;
- bounded session and tag state;
- out-of-order/lost-message behavior permitted by the protocol;
- replay detection and duplicate container rejection;
- payload block parsing and padding;
- session reset/re-establishment behavior without unauthenticated fallback.

A session lookup miss or authentication failure must not reveal whether a destination/key exists through distinguishable remote responses. Ratchet state updates must be atomic with successful authentication to avoid desynchronization from forged input.

## Garlic messages and cloves

Implement:

- GarlicMessage envelope handling;
- one or more cloves with bounded count and aggregate bytes;
- local, destination, router and tunnel delivery instructions required by the MVP;
- clove IDs/expiration and duplicate suppression;
- delivery-status cloves for acknowledgements where required;
- no recursive unbounded garlic nesting;
- dispatch through bounded queues to NetDB, tunnel and destination services.

Clove parsing occurs only after authenticated decryption where encryption is required. Unknown delivery types/options must follow specification-defined rejection or skipping behavior.

## Router ECIES

Router-directed garlic and encrypted NetDB replies use router ECIES rules distinct from destination ratchets. Keep separate types and APIs for:

- router static keys from RouterInfo;
- one-shot or protocol-specific sessions/replies;
- destination LeaseSet keys;
- tunnel-build X25519 keys;
- NTCP2/SSU2 transport keys.

Key-type confusion is a protocol vulnerability. The Rust type system should make cross-use difficult.

## Implementation references

- Java I2P: data/LeaseSet classes, router message/garlic processing, client-message pools and ECIES crypto/session components.
- I2P+: matching packages; compare ratchet cleanup, session bounds, LeaseSet publication and destination routing changes.
- i2pd: `Garlic*`, ECIES-X25519-AEAD-Ratchet sources, LeaseSet/LocalDestination and NetDB client code.
- Emissary/go-i2p: I2NP garlic session, crypto, LeaseSet and destination packages under `lib`.

Compare exact KDF/transcript bytes, tag/session limits, padding, LeaseSet variants emitted by default, offline signature behavior, lookup/publication refresh and failure recovery. README support claims are insufficient without vectors and current tests.

## Required tests

- Fixed New Session and Existing Session vectors from independent implementations.
- One-bit mutations to ephemeral/static keys, associated data, ciphertext and tags.
- Ratchet/tag use-once, expiry, replay, out-of-order and state-rollback tests.
- Session-table and tag-table pressure with deterministic eviction.
- Garlic clove count, nesting, aggregate-size and delivery-instruction bounds.
- LeaseSet encode/sign/verify vectors for every supported variant/key type.
- Invalid destination binding, wrong signing type, stale/future leases and conflicting records.
- Publication, lookup and refresh through Java I2P and i2pd floodfills.
- Two `i2pr` destinations plus cross-router destination-to-destination messages.
- Cancellation during encryption, publication and dispatch.
- Fuzzing authenticated plaintext block/clove/LeaseSet parsing.

## Deferred and compatibility behavior

- ElGamal/AES+SessionTags: compatibility-only if the MVP must communicate with legacy destinations; never a fallback after ECIES authentication failure.
- Unimplemented/optional ECIES blocks identified by the official spec: deferred until current deployment and peer behavior are verified.
- ML-KEM hybrid ECIES: compatibility watch; safe type recognition first, full implementation only with reviewed dependencies and deployment need.
- Encrypted LeaseSet client authorization and MetaLeaseSet service composition: deferred unless required by MVP service hosting.
- Raw and repliable datagrams: required-later only if exposed through SAM or service APIs selected for MVP.

## Open decisions

1. Exact LeaseSet variants and encryption/signature types emitted by the first destination implementation.
2. Whether legacy ElGamal destinations are within the MVP interoperability promise.
3. Maximum active ECIES sessions/tags globally and per destination/peer.
4. Persistent versus ephemeral destination keys and secure on-disk format.
5. Padding distribution that follows the spec without a stable implementation fingerprint.
6. Encrypted LeaseSet scope for the MVP, including authorization-key storage and API exposure.
7. Failure semantics when destination tunnels expire while a streaming/ECIES session remains active.
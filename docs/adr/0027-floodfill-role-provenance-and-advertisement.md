# ADR 0027: Floodfill role, NetDB provenance, and advertisement gates

- Status: Accepted
- Date: 2026-10-01
- Decision owner: repository maintainer (owner-authorized Plans 270–279)
- Related: ADR 0021, ADR 0026, `specs/CONFORMANCE.md`, `specs/protocols/04-reseed-netdb.md`, Floodfill Plans 270–279

## Context

The router already has client-side NetDB lookup and publication machinery, validated RouterInfo and ordinary LeaseSet2 storage, I2NP codecs, authenticated SSU2 I2NP ingress, and local RouterInfo publication snapshots. It does not have a floodfill server role. A server role must not turn client-learned or lookup-response data into globally answerable or floodable material, and must not infer publication authority from record validity alone.

Plan 270 compared the current official I2P specification corpus at website commit `8baa1d680db263941daf2fb4462fbd75ba01c47f` with the repository's pinned commit `88596022920bdf99f27db27688faf4f204792fcd`. The relevant English pages for common structures, I2NP, encrypted LeaseSet, ECIES router messages, and the Network Database overview have no content diff. Current metadata remains I2NP accurate for 0.9.69 and common structures accurate for 0.9.68. Exact behavioral references are Java I2P 2.13.0 commit `9134f808337b401e8e53c73734c81fab04280c9d` and i2pd 2.61.0 commit `635b013a612ff47278ef02acf8580a28e10e26c5`.

The official I2NP document explicitly records an unresolved exploratory-hit difference: Java may return a RouterInfo when an exploratory key hits one, while i2pd returns a DatabaseSearchReply. Exploration is defined to discover non-floodfill routers, so M12 resolves this to a search-reply-only policy. This avoids an accidental data response from an exploration request and agrees with i2pd. The difference is retained as a Java qualification observation, not hidden as normative certainty.

## Decision

### 1. Role and ownership

Floodfill is an optional server role layered on `i2pr-netdb`. `RouterInfoLookup` and LeaseSet lookup remain client state machines and are not made bidirectional. Runtime-neutral NetDB code validates records, applies pure policy, and emits typed effects. `i2pr-daemon` owns sockets, tunnel/direct dispatch, supervision, shared resource governance, lifecycle health, and RouterInfo publication. Transport crates do not mutate NetDB.

### 2. Namespace and provenance

Every server-authority record access is scoped by an explicit namespace and provenance. The minimum dimensions are:

- `MainRouter` versus an opaque, bounded `ClientNamespaceId`;
- direct authenticated router ingress versus router-managed tunnel ingress, client tunnel ingress, or local origin;
- `PublishedStore`, `LookupResponse`, `FloodReplica`, or `LocalPublication` purpose;
- validated record identity/type and observed time, plus an authenticated source category where available.

No client namespace falls back to main-router LeaseSets, and main-router server queries never inspect client namespaces. A lookup response is not publish provenance. A client-tunnel observation is not main-router authority. A zero-token replica is not a new publisher store. Client Destination keys, sockets, tunnel objects, and daemon handles are not provenance metadata. Where ingress cannot be classified, server disclosure and replication fail closed.

### 3. Validity, disclosure, replication, and persistence are separate

Cryptographic/structural validity does not itself permit answering, replication, or persistence. These are separate typed decisions over validated record plus namespace, provenance, flags, time, role state, and bounded policy. Each decision has a typed denial reason. Only records with explicit server-authority provenance can be disclosed or replicated. Lookup-response-only, client-namespace, unpublished, expired, hidden RouterInfo, or ambiguously restored material is not answerable or floodable.

### 4. Supported DatabaseStore record floor

The broad floodfill server floor is DatabaseStore types 0, 1, 3, 5, and 7. Plan 272 cannot begin until both provenance/storage Plan 271 and the required Red25519 crypto-provider review Plan 280 have passed:

| Type | Record | Key binding | Signature / freshness and disclosure rule |
|---:|---|---|---|
| 0 | RouterInfo (compressed in DatabaseStore) | DatabaseStore key equals SHA-256(RouterIdentity) | Validate canonical bounded decompression, RouterInfo signature, identity/hash binding, network and publication time. Do not answer hidden or stale records. Do not flood RouterInfo published more than one hour ago. |
| 1 | Classic LeaseSet | DatabaseStore key equals SHA-256(Destination) | Verify the Destination signature over the classic signed bytes. Version is the earliest lease expiration. It has no published/unpublished flag; disclosure requires explicit publisher-store provenance in the main-router namespace and unexpired leases. |
| 3 | LeaseSet2 | DatabaseStore key equals SHA-256(Destination) | Verify signature over `0x03 || signed bytes`, using the Destination key or verified transient offline-signing key. Compare `published` for replacement; validate header flags and expiry. Header bit 1 means unpublished and denies answer/flood; bit 2 marks blinded-on-publication; an unencrypted type-3 value with this bit is not answerable/floodable until a separately validated type-5 encrypted publication is received, and bit 3 is reserved and must be zero. |
| 5 | EncryptedLeaseSet | DatabaseStore key equals SHA-256(two-byte blinded sigtype || blinded public key) | Keep encrypted body opaque. Validate bounded outer fields, offline-signing block if present, signature over `0x05 || signed bytes`, expiry, flags, and exact key binding. Header bit 1 means unpublished and denies answer/flood. Do not decrypt. |
| 7 | MetaLeaseSet | DatabaseStore key equals SHA-256(Destination) | Verify signature over `0x07 || signed bytes`, using Destination or verified transient signing key. Validate bounded options, lease/revocation counts, flags, publication and expiry. Header bit 1 means unpublished and denies answer/flood; bit 2 marks blinded-on-publication and denies serving this unencrypted body as ordinary published material. |

Type 1 has no flag field; no implementation may invent one. For LS2 headers (types 3 and 7), bit 0 is offline signature, bit 1 unpublished, bit 2 blinded-on-publication, and bits 15–3 reserved. For EncryptedLeaseSet (type 5), bit 0 is offline signature, bit 1 unpublished, and bits 15–2 reserved. The offline block encodes an expiration, transient signature type and key, then a signature by the long-term signing key over the big-endian expiration, transient type, and transient key. The record signature is checked with the destination/transient key for types 1/3/7, or blinded/transient key for type 5. The record signature for types 3/5/7 covers the record bytes prefixed with its one-byte DatabaseStore type; classic type 1 uses its signed record bytes without an invented prefix. The exact offline delegation fields follow the canonical structures specification.

The initial validated signature profile is EdDSA-SHA512-Ed25519 (type 7) for supported Destination/router signing paths and RedDSA-SHA512-Ed25519 (type 11) where a blinded EncryptedLeaseSet requires it. RouterInfo support remains the already-validated RouterInfo profile. Every other key/signature type returns typed `Unsupported`; it is never treated as structurally valid server data. The workspace currently has no Red25519 verifier. Plan 280 is an explicit prerequisite for Plan 272: it must select and review a vetted provider/wrapper for type 11 or record a blocking disposition. No local cryptographic primitive or permissive fallback may be added. Standard LeaseSet2, MetaLeaseSet, and EncryptedLeaseSet must satisfy current type-specific expiry limits. Expired records are never served or flooded. Same-version byte-identical data is idempotent; same-version different bytes are conflict/reject; older data cannot replace newer validated data.

### 5. DatabaseLookup and DatabaseSearchReply behavior

The lookup type flags are interpreted as specified by I2NP:

- `00` ANY is deprecated but supported as compatibility lookup: serve an eligible RouterInfo first, then an eligible LeaseSet-family record, else a bounded search reply.
- `01` LeaseSet lookup: serve only an eligible LeaseSet-family record, else a bounded search reply.
- `10` RouterInfo lookup: serve only an eligible RouterInfo, else a bounded search reply.
- `11` exploration: always return a bounded DatabaseSearchReply containing eligible non-floodfill RouterInfo hashes only; do not return a record even on exact local hit.

Honor the explicit exclusion list, always exclude self, deduplicate candidates, and return only verified non-hidden candidate RouterInfos. The normal target candidate count is 3; the hard per-response ceiling is 16. An invalid/unsupported request receives no large fallback response. Search keys and excluded peers are real record/router hashes; daily routing keys are never placed on the wire.

### 6. Reply encryption

For replies carried to a requested tunnel, support the current supplied-key ECIES form only: one 32-byte session key, exactly one 8-byte tag, nonce zero, and that tag as associated data, for both DatabaseStore and DatabaseSearchReply payloads. Keys/tags are secret owners with redacted formatting and zeroization support. Direct replies do not gain an encryption claim. The TBD DH-derived ECIES forms and ElGamal floodfill-router reply support are deferred unless a later frozen interoperability plan proves them necessary. ElGamal destination compatibility elsewhere is unaffected.

### 7. Replication and routing-key rollover

A floodfill emits replication work only for a cryptographically valid, eligible, strictly newer publisher store. Replication uses a direct authenticated router connection, DatabaseStore reply token zero, and never a tunnel. A zero-token replica is stored if otherwise eligible but is never acknowledged, re-flooded, or treated as publisher provenance. Fanout is 3 by default and is bounded by policy. Select targets nearest to the record's daily routing key, not nearest to the floodfill itself.

The daily routing key is SHA-256(real record key || UTC ASCII `yyyyMMdd`); it changes at UTC midnight and is local selection state only. Around rollover, selection considers current and next-day routing keys under a bounded distinct-peer policy; wire messages always contain the real record key. Never flood an expired LeaseSet or a RouterInfo published more than one hour ago.

### 8. Persistence and restart

Persist only bounded validated record bytes and the metadata required to revalidate identity, type, expiry, replacement, and provenance. On load, all records are revalidated. Provenance metadata that is absent, corrupt, unsupported, or inconsistent maps to `RestoredUntrusted`, which is neither answerable nor floodable until a fresh eligible publisher store re-establishes authority. Client namespace data remains segregated and bounded. Storage formats are versioned and atomic/recoverable; Plan 276 owns concrete format and migration details.

### 9. Lifecycle and advertisement

Role states include disabled, starting, serving, degraded, and stopping/stopped. Configuration expresses intent only. A typed daemon-owned readiness/health authority must prove enabled configuration, required server paths, resource admission, usable direct transport, storage/maintenance readiness, and healthy supervision before producing an advertisement-eligible RouterInfo snapshot. Loss of a required condition withdraws `caps=f` and stops new server work; in-flight work is bounded and drained/cancelled by its owner. No code path may add `caps=f` from configuration alone.

Plan 278 may establish only one-family experimental progression and cannot authorize normal-daemon advertisement. Plan 279 must qualify the second independent router family and satisfy ADR 0026 plus `specs/CONFORMANCE.md` before an explicit operator opt-in may permit normal-daemon `caps=f`. Default remains off. RouterInfo `router.version` reports only the actual implemented API/support level; it is not copied from the newest specification as branding.

### 10. Evidence vocabulary

M12 evidence is recorded as distinct tiers: `architecture-frozen`, `local-validated`, `one-family-experimental`, `two-family-qualified`, and `normal-opt-in-activated`. The latter two require their own observed evidence and authority transition. A lower tier never implies a higher tier. No tier implies public-network operation, production readiness, anonymity, or privacy guarantees.

## Rejected alternatives

- Reusing RouterInfoLookup as a bidirectional client/server state machine: mixes ownership and lets server work inherit client retry/state assumptions.
- Treating every valid record as public answer/flood material: ignores the NetDB segmentation and publisher/lookup provenance distinction.
- Serving exploration hits as records: conflicts with exploration's peer-discovery purpose and i2pd behavior; bounded DSRM-only is the selected policy.
- Routing replicas through tunnels or giving them nonzero tokens: creates unnecessary intermediaries, acknowledgements, and re-flood loops.
- Enabling caps=f from config alone or after one-family evidence: confuses operator intent/readiness and experimental progression with full conformance.
- Persisting answer authority without provenance integrity: permits restart or disk corruption to turn lookup-only material into server disclosure.

## Review triggers

Revisit this ADR only through a new accepted ADR if the official I2P specification changes any record, lookup, reply-encryption, routing-key, or advertisement rule; a pinned Java/i2pd qualification demonstrates a material compatibility defect; the M12 record support floor changes; or a future plan proposes a new persistence, network-exposure, or capability policy.

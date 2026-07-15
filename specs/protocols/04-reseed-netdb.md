# Reseeding and network database (NetDB)

Status: **required**  
Primary roadmap milestone: **4**  
Dependencies: common structures, I2NP, NTCP2, storage and core lifecycle

## Scope

This dossier covers bootstrap through signed reseed bundles; validation, persistence and expiry of RouterInfo and LeaseSet-family records; Kademlia-style network-database lookup/publication behavior; floodfill interactions; and publication of the local RouterInfo.

Reseeding is an exceptional bootstrap path. Ordinary network operation must obtain and maintain data through authenticated router links and NetDB messages rather than repeatedly depending on clearnet reseed services.

## Authoritative sources

- [Reseed documentation](https://i2p.net/en/docs/misc/reseed/), pinned in [SOURCES.md](../SOURCES.md), updated 2025-10 and accurate for I2P 2.10.0.
- [Signed update/SU3 specification](https://i2p.net/en/docs/specs/updates/).
- [Network database overview](https://i2p.net/en/docs/overview/network-database/).
- [I2NP specification](https://i2p.net/en/docs/specs/i2np/) for DatabaseStore, DatabaseLookup, DatabaseSearchReply and delivery-status behavior.
- [Common structures](https://i2p.net/en/docs/specs/common-structures/) for RouterInfo and LeaseSet-family records.
- Current ECIES/encrypted-LeaseSet specifications for encrypted replies and records.

The current reseed documentation describes HTTPS acquisition of signed `i2pseeds.su3` bundles, production network ID 2, a ZIP payload containing RouterInfo files, and an embedded signer/certificate trust model. Treat website counts and current server lists as operational guidance, not protocol constants.

## Required MVP reseed behavior

- Configure multiple independent HTTPS reseed sources and a local/offline bundle path.
- Send the current production network ID and reject cross-network material.
- Enforce HTTPS certificate validation separately from SU3 signature validation.
- Parse SU3 in a streaming, length-bounded manner.
- Verify the signer against an explicitly packaged/configured trust store.
- Verify content type, file type, signature type/length, reserved fields and signed byte range.
- Bound compressed bytes, decompressed bytes, file count, per-entry size, total RouterInfo bytes, path depth and archive metadata.
- Reject paths, directories, symlinks, duplicate entries and unexpected filenames.
- Validate every RouterInfo structurally, cryptographically and temporally before insertion.
- Deduplicate by router hash and choose records by validated publication time/policy.
- Avoid logging source URLs containing sensitive local configuration or complete peer sets.

A failed source must not cause immediate unbounded retries or fallback to unsigned/plain HTTP material. Source rotation and backoff must be deterministic and bounded.

## Required MVP NetDB behavior

### Storage model

Maintain separate concepts for:

- validated immutable record bytes and semantic view;
- current record by key/type;
- observation/source metadata;
- peer profile and selection information;
- local RouterInfo under construction versus last published signed snapshot.

Enforce independent quotas for RouterInfo, LeaseSets, per-type records, total encoded bytes and pending lookup state. Expired, invalid or unsupported records must not remain eligible merely because they are persisted.

### Lookup and publication

Implement bounded state machines for:

- RouterInfo lookup;
- LeaseSet lookup;
- iterative floodfill queries and DatabaseSearchReply processing;
- encrypted replies required by current ECIES peers;
- duplicate query coalescing;
- deadlines, retry/peer budgets and cancellation;
- DatabaseStore acceptance and forwarding policy;
- local RouterInfo publication, confirmation and republish;
- LeaseSet publication when Milestone 6 activates destinations.

Peer selection must consume validated observations and policy; codecs and storage must not embed scoring policy.

### Floodfill role

The MVP roadmap requires floodfill operation when configured and eligible. Implement it after ordinary client behavior is stable. Floodfill mode requires:

- explicit enablement and truthful RouterInfo capability advertisement;
- stricter storage, lookup, reply and unsolicited-store quotas;
- key-distance selection consistent with current network behavior;
- anti-amplification and per-peer request controls;
- database maintenance, expiry and rebroadcast policy;
- tests showing the router cannot be induced to reflect large replies or retain unlimited records.

Do not advertise floodfill capability from a configuration flag alone.

## Local RouterInfo publication

RouterInfo generation must use a validated snapshot of:

- RouterIdentity and signing key;
- current I2NP feature/API version;
- capabilities/bandwidth class;
- reachable NTCP2/SSU2 addresses and options;
- network ID and family fields if supported;
- publication timestamp and canonical properties.

Transport state may emit address observations, but only the publication service signs and commits RouterInfo. Rate-limit resigning/publication during address churn.

## Implementation references

- Java I2P: router reseed components, SU3/core signing containers, `router/java/src/net/i2p/router/networkdb` and RouterInfo publication components.
- I2P+: corresponding packages; compare Kademlia edge cases, floodfill policy, trimming and cache behavior.
- i2pd: `libi2pd/NetDb.cpp`, `RouterInfo.cpp`, reseed and family/signature helpers.
- Emissary/go-i2p: NetDB, RouterInfo, reseed and router composition packages under `lib`.

Compare record age policy, closest-peer selection, retry fan-out, unsolicited store handling, publication confirmation, corruption recovery, floodfill eligibility and memory/disk quotas. Distinguish interoperability behavior from implementation-specific peer scoring.

## Required tests

- Valid SU3 fixture and mutations of every header length/type/reserved field.
- Wrong signer, wrong network, invalid signature and truncated content.
- ZIP bombs, excessive entries, duplicate names, traversal paths, nested directories and oversized RouterInfo files.
- Mixed valid/invalid bundle entries with atomic or explicitly documented partial acceptance.
- RouterInfo signature, hash/name mismatch, stale/future publication and unsupported-key tests.
- Lookup success, timeout, retry, duplicate coalescing, cancellation and peer exhaustion under virtual time.
- Malicious DatabaseSearchReply loops, duplicates, excessive peer lists and non-progressing closest sets.
- DatabaseStore quotas, conflicting records and replayed/stale entries.
- Persistence interruption/corruption and restart revalidation.
- Local RouterInfo publication/confirmation against Java I2P and i2pd.
- Floodfill request/reply amplification and storage-pressure tests.

## Deferred and compatibility behavior

- Unsigned reseed formats and plain HTTP: legacy-reject.
- Automated reseed-server operation: outside the MVP; only the client is required.
- Advanced family trust/reputation policy: required only if current RouterInfo validation or peer selection depends on it.
- MetaLeaseSet/service records and PQ-hybrid records: parse/store policy decided with Milestone 6; do not advertise unsupported processing.
- Public-network floodfill enablement: deferred until controlled mixed-router tests and resource review pass.

## Open decisions

1. Initial packaged reseed trust store, update process and operator override model.
2. Atomicity policy when a signed bundle contains some invalid RouterInfos.
3. Disk format for preserved signed records, indexes, expiry and corruption detection.
4. NetDB memory/disk quotas suitable for Raspberry Pi-class targets.
5. Lookup convergence algorithm and exact closest-peer semantics after comparing Java I2P and i2pd.
6. Minimum peer/I2NP versions accepted for lookup, store, tunnel construction and floodfill exchange.
7. Eligibility and operational safeguards required before `i2pr` may advertise floodfill capability.
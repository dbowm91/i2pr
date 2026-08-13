# Plan 104: persistent NetDB cache and SU3 reseed trust path

## Status and authority

- Status: **planned; blocked until Plan 103 closes**.
- Date: 2026-08-13.
- Parent authority: Plan 102.
- Direct prerequisite: Plan 103.
- Successor: Plan 105.
- Protocol authority: `specs/protocols/04-reseed-netdb.md` plus the current official I2P SU3/update specification.

## Objective

Make the Plan 103 RouterInfo trust boundary durable and bootstrap-capable without activating an I2P transport.

Plan 104 adds two things: a simple bounded persistent cache of canonical signed RouterInfo bytes, and a bounded SU3 reseed verification/ingestion path. Disk and reseed data remain untrusted until they pass the same Plan 103 RouterInfo validation path. This plan must not create a special "trusted cache" or "trusted reseed" insertion API.

Normal `i2pr run` remains transport-neutral after this plan. Automatic startup reseeding and daemon service composition belong to Plan 106.

## Current protocol requirements

The implementation must follow the current I2P reseed/SU3 contract captured in the repository dossier. The relevant production form is `i2pseeds.su3`, SU3 content type RESEED, file type ZIP, a signer selected from a reseed-specific trust set, and top-level RouterInfo entries whose filenames contain the I2P Base64 router hash. Production requests include the configured network ID; network ID 2 is the production I2P network.

HTTPS authentication and SU3 authentication are separate. Both must succeed. A valid SU3 signature authenticates the container but does not make an invalid/stale RouterInfo eligible for NetDB.

## Scope lock

Expected surfaces:

```text
crates/i2pr-storage/...                 raw RouterInfo cache effects
crates/i2pr-netdb/src/reseed.rs         SU3/reseed validation and ingestion
crates/i2pr-crypto/...                  only the minimal reviewed signature helper needed
assets/reseed/...                       packaged public reseed signer certificates, if selected
Cargo.toml / Cargo.lock                 narrowly reviewed dependencies
specs/SOURCES.md                        trust-anchor provenance if required
scripts/check-dependency-direction.sh   only if dependency graph changes
```

A small dormant HTTPS acquisition adapter may land in `i2pr-runtime` or `i2pr-daemon` if it remains cleanly separable. If the HTTP/TLS choice would dominate this plan, leave acquisition as an explicit byte-source interface and complete network activation in Plan 106.

Do not touch NTCP2 behavior, Plan 099 tooling, Plan 079, SSU2, tunnels, SAM/I2CP, or floodfill service behavior.

## Dependency policy

Do not implement RSA, X.509, ZIP/DEFLATE, or TLS primitives locally. Select maintained Rust crates compatible with the workspace Rust 1.85 MSRV, avoid OpenSSL/system-TLS requirements where a mature pure-Rust option exists, disable unnecessary default features, and run the repository dependency/advisory/license checks after additions.

Do not add a database engine for the RouterInfo cache.

## Work package 1 — persistent RouterInfo cache

Use a deliberately simple recoverable layout under the router data directory, preferably:

```text
netdb/routers/<64-lowercase-hex-router-hash>.ri
```

Each file contains only the canonical encoded RouterInfo bytes. No database index is required in this phase.

Persistence requirements:

- same-directory temporary write followed by atomic replacement using existing storage conventions;
- bounded per-file and aggregate disk work;
- failed replacement preserves the prior valid file;
- no raw RouterInfo or peer inventory in normal diagnostics;
- temporary/interrupted files never become eligible records.

On startup/load, every candidate file must go through:

```text
strict filename validation
 -> bounded read
 -> RouterInfo structural decode
 -> Plan 103 validation with expected filename hash and current time
 -> normal Plan 103 store insertion
```

Persistence is not a trust boundary. `ValidatedRouterInfo` must never be deserialized or reconstructed directly from disk.

### Corruption policy

One bad cache file must not make all valid peers unavailable. Isolate invalid, unreadable, oversized, stale, signature-invalid, or hash-mismatched entries and continue within global scan limits. Report aggregate typed counts only.

Choose one simple policy for rejected files: leave them in place with bounded repeated-scan cost, or delete them after failed validation. Do not create an unbounded quarantine/archive of rejected bytes.

### Required cache limits

At minimum bound:

```text
entries inspected
single file bytes
total bytes read
validated records accepted
validated encoded bytes
```

The cache loader must stop deterministically when scan limits are reached.

## Work package 2 — keep storage and validation acyclic

The preferred architecture keeps `i2pr-netdb` filesystem-free. If `i2pr-storage -> i2pr-netdb` would violate dependency direction, use a raw-byte storage seam:

```text
i2pr-storage: bounded read/write/remove of canonical bytes
composition/netdb owner: decode + Plan 103 validation + store insertion
```

Do not introduce a dependency cycle or a generic persistence framework to avoid a few explicit calls.

Plan 104 closure must document the chosen ownership direction.

## Work package 3 — bounded SU3 reseed parser

Implement only the SU3 subset required for reseed verification. Strictly validate:

- SU3 magic and supported format version;
- required zero/reserved fields;
- signature type and matching signature length;
- bounded version and signer identifier lengths;
- checked content-length arithmetic and exact total file consumption;
- file type ZIP;
- content type RESEED;
- exact signed byte region retained from the input.

Unsupported content types and unsupported signing algorithms fail closed. This is not a general software-update/plugin/news implementation.

The reseed version field is metadata, not a replacement for signer trust or RouterInfo freshness validation.

## Work package 4 — explicit reseed signer trust store

Package or configure an explicit trust set for reseed SU3 signers. Production trust anchors must be public certificates obtained from a pinned, documented upstream source. Record at least the signer identifier, certificate digest, provenance/revision, permitted reseed signature type, and content-type scope.

At verification time require:

```text
signer ID resolves unambiguously
certificate is within its validity interval
key type/size matches allowed signature algorithm
content type is RESEED
SU3 signature verifies over the exact signed bytes
```

These certificates are SU3 reseed trust anchors, not general TLS roots. Self-signed certificates are acceptable only when explicitly pinned as reseed trust anchors.

Do not add an `insecure` mode. Private/test networks may use an explicitly separate test/custom signer configuration.

## Work package 5 — SU3 signature verification

Use a reviewed library implementation for the production reseed signing algorithm(s). Current official guidance uses RSA-SHA512-4096/signature type 6 for reseed signers; verify the current packaged trust set during implementation before finalizing the enabled algorithm list.

Tests must prove rejection after mutation of signed header bytes, content, signer identity, or signature, and rejection for wrong/expired/not-yet-valid trust anchors, unsupported algorithms, truncation, and trailing data.

Only verified SU3 content may enter ZIP parsing.

## Work package 6 — bounded ZIP processing

Process the verified ZIP under explicit limits:

```text
maximum archive entries
maximum compressed content bytes
maximum per-entry uncompressed bytes
maximum total uncompressed bytes
maximum cumulative RouterInfo bytes
```

Require ordinary top-level file entries with the exact reseed RouterInfo filename grammar. Reject archive-level path/topology anomalies, duplicate names, unsupported entry forms, or aggregate-limit violations.

Decode the filename hash with the I2P Base64 alphabet. Do not assume the RFC 4648 alphabet. If the repository lacks a strict I2P Base64 helper, add the smallest reusable protocol codec with exact alphabet/length tests.

Each entry pipeline is:

```text
filename router hash
 -> bounded RouterInfo decode
 -> Plan 103 validate(expected_hash)
 -> staged ValidatedRouterInfo
```

## Work package 7 — staged ingestion policy

Use two failure levels.

For SU3 trust failure, malformed archive structure, or aggregate archive limit failure: accept zero records and mutate zero NetDB state.

For an individual RouterInfo failure inside an otherwise authentic, structurally valid archive: reject that record, continue the bounded scan, and keep only aggregate typed failure counts. Do not mutate live NetDB while parsing the archive.

After the complete archive has been verified and scanned, insert the staged valid set through the normal Plan 103 store API in deterministic order. A bundle yielding zero valid RouterInfos is a failed source.

This resolves the earlier open question without giving malformed inner records authority merely because their container was signed.

## Work package 8 — reseed source/acquisition seam

Define a bounded source policy suitable for Plan 106:

- multiple configured HTTPS sources;
- production request carries network ID 2, private networks their configured nonzero ID;
- bounded source attempts per bootstrap cycle;
- connect and whole-request deadlines;
- bounded response bytes;
- HTTPS-only redirects if redirects are supported;
- no fallback to unsigned/plain HTTP reseed data;
- no unbounded retries;
- no credential-bearing source URL in ordinary diagnostics.

Keep the pipeline explicit:

```text
HTTPS bytes
 -> SU3 trust verification
 -> bounded ZIP processing
 -> Plan 103 RouterInfo validation
 -> staged insertion
```

If network acquisition is deferred to Plan 106, Plan 104 must still expose a clean `bytes -> verified/staged records` API and a source-policy configuration type so Plan 106 does not redesign trust semantics.

## Work package 9 — fixtures and tests

Commit deterministic non-secret fixtures for a test-only reseed signer and representative SU3/RouterInfo cases. Production signer certificates are public trust material only; no production signing secret belongs in the repository.

Required tests include:

### Cache

1. valid write/reload round trip;
2. filename/hash mismatch rejection;
3. truncated/corrupt file isolation;
4. signature-invalid and stale file rejection;
5. atomic newer replacement;
6. interrupted temporary file ignored;
7. bounded malicious directory scan;
8. loader proves Plan 103 revalidation occurs.

### SU3/reseed

1. valid test SU3 containing multiple valid RouterInfos;
2. wrong signer/signature rejection;
3. certificate validity rejection;
4. wrong file/content type rejection;
5. truncated/extra SU3 bytes rejection;
6. duplicate/nested/invalid archive entry rejection;
7. archive count/byte-limit rejection without giant checked-in files;
8. RouterInfo filename/hash mismatch;
9. one invalid RouterInfo among valid entries follows staged partial-acceptance policy;
10. zero valid RouterInfos fails the source.

Tests run entirely locally. No root, namespaces, Java I2P, i2pd, or Internet connection is required.

## Work package 10 — documentation and support state

After closure, record the state accurately:

```text
local RouterInfo validation    = implemented
bounded local NetDB            = implemented
persistent RouterInfo cache    = implemented, untrusted-on-load
SU3 reseed verification        = implemented
reseed ingestion               = implemented from verified bytes
automatic daemon reseed        = not yet active unless explicitly scoped here
live NetDB query               = not implemented (Plan 105)
I2P peer connectivity          = still blocked/experimental NTCP2
NTCP2 advertised               = false
```

Do not claim that i2pr can join the network yet.

## Validation

Run at minimum:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo test --locked -p i2pr-netdb
cargo test --locked -p i2pr-storage
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-interoperability.sh
git diff --check
```

Run current dependency/advisory/license checks when dependencies change. Do not create a Plan 104-specific CI workflow.

## Non-goals

Plan 104 does not activate NTCP2, connect to I2P peers, implement DatabaseLookup state machines, construct exploratory tunnels, implement LeaseSet semantic persistence, update reseed trust anchors dynamically, implement general SU3 update handling, support unsigned/plain-HTTP legacy reseed, run a reseed server, implement peer reputation, or implement floodfill service behavior.

## Closure criteria

Plan 104 closes only when:

1. canonical RouterInfo bytes persist atomically and reload through full Plan 103 revalidation;
2. cache work is bounded by explicit file/count/byte limits;
3. corrupt individual cache records are isolated safely;
4. SU3 parsing validates format, lengths, content/file type, signer metadata, and exact signed-byte boundaries;
5. reseed signer trust is explicit, provenance-recorded, content-type scoped, and certificate validity is enforced;
6. signature verification uses reviewed cryptographic dependencies rather than local primitives;
7. ZIP processing is bounded and accepts only valid top-level reseed RouterInfo entry shapes;
8. filename hashes use strict I2P Base64 and are bound to the contained RouterIdentity;
9. every extracted RouterInfo uses the same Plan 103 validator;
10. archive-level failure mutates no NetDB state and individual-record failure follows the documented staged policy;
11. a bounded multi-source HTTPS acquisition seam exists or is explicitly left for Plan 106 behind a stable verified-byte API;
12. normal daemon NTCP2 remains disabled/unadvertised;
13. workspace validation and dependency checks pass;
14. Plan 105 can consume the populated RouterInfo store without knowing SU3, ZIP, X.509, HTTPS, or cache file layout.

## Handoff to Plan 105

Record the exact APIs for enumerating validated floodfill-advertising RouterInfos, obtaining canonical RouterInfo bytes, feeding newly learned RouterInfos into persistence, and loading the in-memory store at startup. Plan 105 must remain unaware of the cache file format and reseed container format.

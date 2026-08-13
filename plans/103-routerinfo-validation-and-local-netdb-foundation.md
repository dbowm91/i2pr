# Plan 103: RouterInfo validation and local NetDB foundation

## Status and authority

- Status: **planned; next executable implementation plan**.
- Date: 2026-08-13.
- Parent authority: Plan 102.
- Baseline: Plan 102 branch/commit or a clean descendant preserving Plan 101.
- Primary protocol dossier: `specs/protocols/04-reseed-netdb.md`.
- Prerequisites: Plans 099-101 closed as recorded; no NTCP2 activation required.
- Successor: Plan 104.

## Objective

Create the first stateful router-information subsystem in i2pr: a runtime-neutral `i2pr-netdb` crate that accepts only cryptographically and temporally validated RouterInfos, binds each record to the SHA-256 RouterHash of its contained RouterIdentity, stores records under explicit memory/count limits, resolves replacement/conflict/expiry deterministically, and can construct the router's own signed RouterInfo without advertising unqualified transports.

This plan is deliberately local and offline. It opens no sockets, performs no DNS, downloads nothing, and does not persist NetDB records. Its purpose is to establish a trustworthy semantic boundary that Plan 104 persistence/reseed and Plan 105 distributed lookup logic can reuse without privileged insertion paths.

## Current repository facts to preserve

The implementation should build on existing owners rather than duplicate them:

- `i2pr-proto::RouterInfo` already performs bounded structural decoding/encoding and retains the exact `signed_bytes()` plus typed `signature()`.
- `RouterInfo::router_identity()`, `published()`, `addresses()`, `options()`, `capabilities()`, and `protocol_version()` already expose the fields needed by NetDB policy.
- `i2pr-crypto::verify_signature()` verifies a typed signature against explicit message bytes.
- `RouterIdentityBundle::sign_router_info()` already creates a signed local RouterInfo.
- `i2pr-proto::Hash` is the protocol 32-byte hash representation. Do not create a second wire hash type unless a semantic wrapper has clear value.
- `i2pr-daemon` loads the persistent `RouterIdentityBundle`, but Plan 103 does not need to wire the new subsystem into the long-lived daemon yet.

The current router identity uses Ed25519 signing and X25519 encryption. Remote RouterInfo validation must not silently claim support for signing algorithms `i2pr-crypto` cannot verify. Unsupported algorithms are a typed rejection, not an invalid-signature claim.

## Hard scope lock

Expected implementation surfaces:

```text
Cargo.toml
Cargo.lock only if dependency graph changes
crates/i2pr-netdb/Cargo.toml
crates/i2pr-netdb/src/lib.rs
crates/i2pr-netdb/src/router_info.rs          preferred
crates/i2pr-netdb/src/store.rs                preferred
crates/i2pr-netdb/src/routing.rs              only for local hash/distance primitives needed now
crates/i2pr-netdb/src/local.rs                preferred local RouterInfo builder/owner
crates/i2pr-netdb/tests/...                    if integration-style tests are clearer
crates/i2pr-crypto/src/lib.rs                  only for a missing minimal reusable verification/hash helper
crates/i2pr-proto/...                          only for a missing structural accessor/helper
scripts/check-dependency-direction.sh          if the new crate must be registered
README/docs/spec support files                 only to record actual support after implementation
plans/103...                                   closure/status notes if repository convention requires
```

File names may differ if an existing repository convention gives a better owner, but keep modules behavior-oriented and small enough to review.

Do **not** touch:

- NTCP2 handshake/frame/runtime behavior;
- Plan 099 workflow or Python interop runner;
- Plan 079;
- daemon NTCP2 config guard;
- SSU2;
- tunnel code;
- reseed HTTP/SU3 acquisition;
- persistent NetDB cache;
- public-network code.

## Required crate boundary

Add `crates/i2pr-netdb` to the workspace.

The crate must be runtime-neutral:

```text
allowed: std collections, i2pr-proto, i2pr-crypto, narrowly justified pure dependencies
forbidden: tokio, sockets, DNS, HTTP, filesystem access, i2pr-runtime, i2pr-daemon,
           i2pr-transport-ntcp2, i2pr-transport-ssu2
```

Prefer no new third-party dependency in Plan 103. SHA-256 already exists in the workspace through `sha2`; if RouterIdentity hashing is not already exposed at the right layer, add the smallest helper at the existing crypto/protocol owner rather than another hashing abstraction.

## Work package 1 — establish RouterHash derivation and binding

### 1.1 Canonical RouterHash

I2P keys a RouterInfo under:

```text
SHA256(encoded RouterIdentity)
```

Implement one canonical helper that derives this value from the contained RouterIdentity using its canonical encoded bytes.

Preferred API shape, adjusted to existing type conventions:

```rust
pub fn router_hash(identity: &RouterIdentity) -> Result<Hash, ...>;
```

or a semantic newtype around `Hash` if and only if it prevents meaningful key confusion without duplicating encoding logic.

Do not hash the complete RouterInfo, signing key alone, `signed_bytes()`, or a debug/string representation.

### 1.2 Claimed-key validation

The local NetDB insertion API should accept an optional/required expected key where the caller has one, especially for future `DatabaseStore` and reseed filename ingestion. The record is eligible only if:

```text
expected_key == SHA256(encoded contained RouterIdentity)
```

A mismatch is a typed `KeyMismatch`-class rejection.

The core validator should also be usable when no external key exists, returning the derived key with the validated record.

### 1.3 Tests

Required cases:

- deterministic RouterHash for the same identity;
- identity mutation changes RouterHash;
- RouterInfo bytes outside the RouterIdentity do not change the RouterHash;
- expected matching key passes;
- expected wrong key rejects before store mutation.

Use an independently generated/static expected hash fixture where practical so a test does not merely compare the helper to itself.

## Work package 2 — define the validated RouterInfo boundary

### 2.1 Validated representation

Create a type that cannot be constructed from arbitrary `RouterInfo` without validation. Suggested shape:

```rust
pub struct ValidatedRouterInfo {
    key: Hash,
    router_info: RouterInfo,
    encoded_len: usize,
    // minimal derived metadata only
}
```

Do not make fields public if that permits callers to manufacture an invalid state. Expose read-only accessors.

If retaining canonical encoded bytes is needed later for persistence/DatabaseStore, either retain them here or provide an exact bounded re-encoding method. Prefer a single canonical byte owner rather than independently encoded copies.

### 2.2 Validation context

Time-dependent policy must be explicit and testable. Suggested structure:

```rust
pub struct RouterInfoValidationPolicy {
    max_age: Duration,
    max_future_skew: Duration,
    max_encoded_len: usize,
}

pub struct ValidationContext {
    now: Date,
    policy: RouterInfoValidationPolicy,
}
```

Exact names may differ. The critical constraint is that validation does not call `SystemTime::now()` internally. The caller supplies `now`.

Use conservative initial bounds derived from current I2P compatibility behavior and the existing protocol dossier. If the exact peer-compatible age window is not already pinned in the repository, inspect Java I2P/i2pd behavior before hard-coding it and document the selected compatibility policy in code/tests. Do not invent a protocol constant where the specification leaves freshness to implementation policy.

### 2.3 Validation order

Use a clear fail-closed order:

```text
bounded structural RouterInfo already decoded
    -> derive RouterHash
    -> expected-key binding if supplied
    -> supported signing-key/signature type check
    -> verify signature over RouterInfo::signed_bytes()
    -> publication timestamp policy
    -> semantic option/address policy needed for eligibility
    -> construct ValidatedRouterInfo
```

Do not use post-validation semantic normalization that changes signed fields.

### 2.4 Required signature behavior

Verify against:

```text
RouterInfo.router_identity().signing_key()
RouterInfo.signed_bytes()
RouterInfo.signature()
```

A modified signed byte must fail.

Distinguish at least:

- unsupported signing algorithm;
- cryptographically invalid signature;
- malformed/structurally invalid record (from `i2pr-proto`);
- key mismatch;
- stale publication;
- excessive future publication;
- policy/resource rejection.

Do not retain arbitrary peer error strings in the validated type or default diagnostics.

### 2.5 Capability metadata

The `caps` mapping may be used to derive candidate metadata such as `is_floodfill`. Treat it as signed-but-self-asserted data:

```text
valid signature + caps contains 'f' => peer advertises floodfill capability
```

It does not mean the peer is honest, responsive, healthy, or trusted. Name accessors accordingly, e.g. `advertises_floodfill()` rather than `is_trusted_floodfill()`.

Do not implement floodfill service behavior in this plan.

## Work package 3 — bounded in-memory RouterInfo store

### 3.1 Store ownership

Create one in-memory owner, for example:

```rust
pub struct RouterInfoStore { ... }
```

Use deterministic standard-library containers unless measurement later justifies another dependency.

The store receives only `ValidatedRouterInfo`; it must not provide `insert_unchecked`, `load_trusted`, `from_reseed_trusted`, or similar bypasses.

### 3.2 Explicit limits

Define a constructor/config object with at least:

```text
max_records
max_total_encoded_bytes
```

Consider a maximum per-record encoded size at validation rather than store time.

Zero limits and arithmetic-overflow cases must reject cleanly.

Store accounting must be exact on:

- insert;
- idempotent reinsert;
- replacement;
- removal;
- expiry/prune;
- failed replacement;
- capacity rejection.

No saturating arithmetic may hide an accounting bug. Use checked arithmetic and fail closed.

### 3.3 Replacement policy

For the same RouterHash:

```text
incoming published > existing published  -> replace if valid and budget permits
incoming published < existing published  -> reject/ignore as stale replacement
same published + byte-identical record    -> idempotent no-op
same published + different signed record  -> typed conflict; retain existing
```

If current I2P compatibility requires a different tie-break for equal timestamps, document and test it before changing this rule. Do not select by arrival order.

A failed replacement must leave the existing valid record intact.

### 3.4 Capacity policy

Do not silently evict arbitrary valid routers merely to accept one new record unless an explicit deterministic eviction policy is part of this plan. Preferred Plan 103 behavior:

- prune records already outside freshness eligibility first;
- otherwise reject insertion with a typed capacity outcome;
- defer peer-quality/LRU eviction to later policy work.

This keeps initial semantics simple and auditable.

### 3.5 Store API

Minimum useful API:

```text
insert(validated) -> InsertOutcome / error
get(&Hash) -> Option<&ValidatedRouterInfo>
contains(&Hash)
remove(&Hash)
len()
encoded_bytes()
prune(now, policy) -> bounded summary
iter / snapshot only if bounded consumer needs it
```

Do not expose mutable references that permit changing key/publication fields behind accounting.

### 3.6 Privacy-safe snapshots

If a store snapshot is needed, expose counts/bytes and perhaps bounded category counts. Do not dump every peer hash or RouterInfo in routine health output.

## Work package 4 — local RouterInfo construction

### 4.1 Ownership model

Add a small local RouterInfo builder/manager at the NetDB/crypto seam. It should borrow the existing `RouterIdentityBundle` long enough to sign the record; it should not clone or serialize private key material.

Inputs should be explicit:

```text
persistent RouterIdentityBundle
publication Date supplied by caller
validated/publishable RouterAddress snapshot
peers = empty for current implementation
bounded Mapping options
```

### 4.2 Current address rule

Under Plan 101 authority:

```text
normal daemon NTCP2 = disabled
NTCP2 advertised = false
```

Therefore the Plan 103 local RouterInfo must not contain an NTCP2 `RouterAddress` merely because NTCP2 codecs/runtime support exist.

For this phase, an empty address list is valid local construction evidence. Do not fake a loopback/private/public address.

### 4.3 Options

Use the smallest truthful options needed by current interoperability goals. Include a bounded router version/capability value only when the repository already has an authoritative value/format. Do not advertise floodfill, reachability, tunnel participation, SSU2, or unsupported bandwidth semantics.

If no capability is currently truthful, use a minimal valid capability mapping rather than borrowing another router implementation's defaults.

### 4.4 Self-validation

After signing the local RouterInfo, run it through the same Plan 103 validator before making it the current local record. There must be no privileged local bypass.

A local signing/validation failure must not replace the last valid local snapshot.

## Work package 5 — peer-selection primitives only

Plan 105 will own query state machines. Plan 103 may implement only the pure primitives needed to select records later:

- XOR distance between two 32-byte hashes;
- stable ordering by distance with RouterHash as deterministic tie-break;
- filtering records that advertise floodfill capability;
- bounded nearest-N selection from the local store.

Do **not** implement the daily routing-key transform in a transport/query owner if it naturally belongs in this pure module; it may land in Plan 103 if doing so keeps all distance primitives together. If implemented here, make UTC date an explicit input and test the exact `SHA256(key || yyyyMMdd)` byte convention. Otherwise Plan 105 will add it.

Do not implement peer scoring, latency reputation, tunnel eligibility, family avoidance, or random exploration selection.

## Work package 6 — test matrix

### 6.1 RouterInfo validation

Required deterministic tests:

1. valid locally signed RouterInfo passes;
2. one-bit mutation of signed region fails signature verification;
3. signature mutation fails;
4. expected RouterHash mismatch rejects;
5. unsupported signing type returns unsupported, not invalid-signature;
6. stale publication rejects at the exact boundary;
7. excessive-future publication rejects at the exact boundary;
8. exact policy boundary values behave deterministically;
9. structural decode failure cannot enter the validator/store;
10. floodfill capability extraction is signed data but no trust flag is created.

### 6.2 Store behavior

Required tests:

1. first insert;
2. byte-identical idempotent insert;
3. newer replacement;
4. older replacement rejection;
5. equal-timestamp conflict rejection;
6. count quota;
7. byte quota;
8. replacement accounting where encoded size grows/shrinks;
9. failed oversized replacement preserves old record/accounting;
10. prune expired/stale records;
11. arithmetic/limit edge cases;
12. deterministic nearest-N ordering.

### 6.3 Local RouterInfo

Required tests:

- generated record verifies through the normal validator;
- record key matches persistent RouterIdentity;
- current Plan 101 state produces zero transport addresses;
- no `NTCP2` transport style or NTCP2 address option appears;
- signing failure does not alter prior snapshot (use a deterministic test seam if needed);
- private key material is not `Debug`/serialized/logged by the new owner.

### 6.4 Property/fuzz testing

Do not create a large new fuzz campaign. If `i2pr-netdb` introduces a new parser, add a fuzz target. Plan 103 should primarily consume already-decoded structures, so unit/property tests are preferred.

## Work package 7 — documentation and support claims

After implementation, update only truthful current-state documentation. Expected support claim:

```text
RouterInfo structural codec       = implemented
RouterInfo signature validation   = implemented for supported algorithms
local RouterInfo construction     = implemented
local RouterInfo NetDB            = implemented, bounded, memory-only
persistent NetDB                  = not yet implemented
reseed                             = not yet implemented
live NetDB lookup                 = not yet implemented
RouterInfo publication            = not yet live
NTCP2                              = experimental-non-advertised
```

Do not label NetDB client support "implemented" merely because local storage exists.

## Validation commands

Run at minimum:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo test --locked -p i2pr-netdb
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-ntcp2-interoperability.sh
git diff --check
```

If the repository's MSRV lane has a local command, run the new crate through it or ensure CI covers the added workspace member. Do not add a new workflow solely for Plan 103.

## Explicit non-goals

Plan 103 does not:

- read/write the NetDB cache on disk;
- parse or verify SU3;
- fetch reseed data;
- decode gzip-compressed RouterInfo from `DatabaseStore` beyond existing structural behavior;
- send any I2NP message;
- perform a DatabaseLookup;
- connect to any peer;
- construct exploratory tunnels;
- publish local RouterInfo to floodfills;
- accept unsolicited DatabaseStore traffic;
- implement LeaseSet-family semantic storage;
- implement floodfill server behavior;
- change NTCP2 support/advertisement/activation state.

## Closure criteria

Plan 103 is complete only when all of the following are true:

1. `i2pr-netdb` exists as a workspace crate with enforced runtime-neutral dependency direction.
2. RouterHash is derived canonically from encoded RouterIdentity and expected-key mismatch is rejected.
3. A caller cannot construct `ValidatedRouterInfo` without cryptographic/time/policy validation through the intended API.
4. Signature verification uses the contained signing public key and exact retained signed bytes.
5. Unsupported algorithms, invalid signatures, stale/future records, key mismatch, and resource rejection have distinct typed outcomes.
6. The in-memory store enforces record and byte quotas with exact accounting.
7. Replacement/conflict/idempotence semantics are deterministic and tested.
8. Expired/stale records can be pruned without corrupting accounting.
9. Floodfill capability is exposed only as self-advertised metadata.
10. The local RouterInfo is signed by the persistent identity, self-validates through the same path, and advertises no NTCP2 address under current authority.
11. No socket, DNS, filesystem, HTTP, Tokio, or transport-implementation dependency entered `i2pr-netdb`.
12. Workspace tests/lints/docs and boundary checks pass.
13. Documentation states that NetDB is local/in-memory only and that live lookup/reseed/publication remain unimplemented.
14. Plan 104 can consume one narrow public validation/store API without an unchecked insertion path.

## Handoff to Plan 104

The Plan 103 implementation handoff must identify the exact stable APIs Plan 104 should use for:

```text
encoded RouterInfo -> structural decode -> validate -> ValidatedRouterInfo
ValidatedRouterInfo -> insert/replace outcome
store -> canonical record bytes for persistence
current local RouterInfo -> validated signed snapshot
```

Do not begin persistent cache or reseed work until these APIs are present and Plan 103 tests are green.

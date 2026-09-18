# Milestone 1 plan B: common structures and canonical protocol model

## Purpose

Implement the common I2P structures that define identity, addressing, signed metadata, and leases, using the bounded codec foundation from Plan 011.

## Required sources

- `specs/protocols/01-common-identity-crypto.md`
- relevant sections of `specs/protocols/04-reseed-netdb.md`
- relevant sections of `specs/protocols/06-garlic-ecies-leasesets.md`
- pinned official specifications in `specs/SOURCES.md`
- `specs/CONFORMANCE.md`

Implementation evidence from Java I2P, i2pd, I2P+, and Emissary may clarify ambiguity but must not silently override clear specifications.

## Scope

Implement protocol representations and strict codecs for the Milestone 1 subset of:

- dates/timestamps and network IDs where encoded in common structures;
- `Mapping` and mapping entries;
- certificates and key certificates;
- public key and signing public key encoded material;
- `RouterIdentity`;
- `Destination`;
- router addresses and transport options as opaque/canonical mappings;
- router capabilities and version fields as bounded protocol values;
- `RouterInfo` unsigned and signed byte representation boundaries;
- `Lease`;
- LeaseSet types required as data models by later milestones, with unsupported variants explicitly rejected or represented as deferred according to the dossier.

Cryptographic execution and secret keys are handled in Plan 013. This plan may model algorithm identifiers and validate encoded public-material lengths, but it must not implement cryptographic primitives.

## Representation rules

### Signed bytes versus semantic values

For signed containers, preserve the exact signed byte region used for verification. Do not decode into a semantic structure and later assume reserialization reproduces the original signed bytes unless canonical encoding is proven and tested.

Recommended shape:

```text
parsed semantic fields
+ bounded original signed bytes or verified canonical bytes
+ signature bytes/type
```

The final API should make it difficult to verify the wrong byte range.

### Unknown and legacy algorithms

Represent known algorithm identifiers with typed enums and an explicit unsupported/unknown path. Do not map unknown values to a default algorithm. Legacy algorithms required to parse deployed records should be isolated from generation policy.

### Mappings

Mappings must enforce:

- maximum encoded size;
- key/value length limits;
- key uniqueness;
- canonical ordering where required;
- deterministic encoding;
- exact declared-length consumption;
- no silent overwrite of duplicate keys.

Prefer an ordered representation that preserves canonical behavior. Do not expose unrestricted mutation that can create a noncanonical value without validation.

### Router addresses

Transport styles and options should remain protocol data, not transport implementation objects. Plan 012 should validate common structure and bounds but must not interpret NTCP2/SSU2 handshakes or establish sockets.

### Time semantics

Separate structural timestamp decoding from runtime policy such as expiry tolerance and clock-skew acceptance. Structural codecs may reject impossible representation; later NetDB policy decides freshness.

## Implementation phases

### Phase A: primitive domain types

1. Add bounded protocol string/byte newtypes only where they enforce a real invariant.
2. Implement algorithm/type identifiers and encoded-length lookup tables from pinned specifications.
3. Add timestamp and hash wrappers with nonrevealing or concise debug behavior.
4. Add exhaustive tests for identifier-to-length relationships.

### Phase B: Mapping and certificates

1. Implement strict Mapping decoding and canonical encoding.
2. Reject duplicates, malformed ordering, invalid lengths, and trailing content.
3. Implement certificate base form.
4. Implement key-certificate parsing and validation.
5. Record unsupported certificate types and safe rejection behavior.

### Phase C: RouterIdentity and Destination

1. Implement structural decode for legacy fixed fields plus certificate-selected key material.
2. Validate public/signing key material length against the declared algorithms.
3. Implement canonical encoding for values created by `i2pr`.
4. Implement identity/destination hash derivation only through reviewed hash-library use and fixed vectors.
5. Ensure no private key type enters `i2pr-proto`.

### Phase D: RouterAddress and RouterInfo

1. Implement bounded RouterAddress fields and options.
2. Enforce maximum address count and encoded size from specifications/policy.
3. Implement RouterInfo signed-region parsing.
4. Preserve signature bytes and signed bytes.
5. Provide a builder for locally generated unsigned RouterInfo values that validates canonical ordering and required fields.
6. Do not advertise NTCP2, SSU2, floodfill, or other capabilities by default.

### Phase E: Lease and LeaseSet model

1. Implement Lease structural codec.
2. Implement only the LeaseSet variants required by the current dossier and later milestones.
3. Separate structural validity from NetDB freshness/publication policy.
4. Enforce lease counts, key counts, signature lengths, and total encoded size.
5. Mark unsupported variants clearly in the support ledger.

## Test vectors

For each top-level structure include:

- at least one authoritative or independently generated positive vector;
- exact encoded bytes for locally generated canonical values;
- truncation at every structural boundary;
- maximum and maximum-plus-one counts and sizes;
- duplicate Mapping keys;
- invalid certificate and key-certificate lengths;
- unknown algorithm identifiers;
- key-length/type mismatches;
- malformed and noncanonical ordering;
- trailing bytes;
- one-bit mutations in signed regions and signatures for later verification tests.

All fixtures must include provenance metadata.

## Public API constraints

- Prefer immutable validated values.
- Builders may accumulate fields but must validate on `build()`.
- Avoid exposing raw mutable maps for signed/canonical structures.
- Do not implement `Default` for values where a default would be invalid or misleading.
- Do not implement broad `serde` support for wire structures unless a concrete non-wire use requires it; serde is not the protocol codec.
- Secret material must not appear in these types.

## Documentation updates

- Update `docs/protocol-support.md` and the machine-readable support ledger with exact structural codec status.
- Add module-level source citations and pinned revision references.
- Add a known-limitations section listing unsupported algorithms, certificates, LeaseSet variants, and policy deferred to later milestones.

## Acceptance criteria

- Mapping, certificate, RouterIdentity, Destination, RouterAddress, RouterInfo, Lease, and selected LeaseSet structures have strict bounded decoders and canonical encoders.
- Fixed vectors and independent vectors decode correctly.
- Signed byte ranges are unambiguous and retained safely.
- Unknown/unsupported types fail explicitly.
- Duplicate and noncanonical mappings are rejected.
- No network behavior or transport-specific state machine exists.
- Fuzz entry points for each top-level decoder are identified for Plan 014.
- Workspace quality and MSRV checks pass.

## Stop conditions

Stop the affected structure if:

- specification sources disagree on signed-byte boundaries or canonicalization;
- algorithm identifier/length behavior is unresolved;
- fixture provenance is unclear;
- a LeaseSet variant cannot be safely modeled without prematurely implementing garlic/session crypto;
- the implementation would need to copy code from another router.

Record the issue in the dossier and support ledger rather than guessing.

## Handoff

Report each implemented structure, limits, supported algorithms/types, fixed-vector sources, public APIs, test categories, unsupported cases, and all specification ambiguities encountered.
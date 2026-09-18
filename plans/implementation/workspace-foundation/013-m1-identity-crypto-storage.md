# Milestone 1 plan C: cryptographic wrappers, router identity, and storage

## Purpose

Add reviewed cryptographic execution behind protocol-specific wrappers and implement persistent router identity generation, atomic storage, loading, and RouterInfo signing.

## New crates

This plan may create:

```text
crates/i2pr-crypto/
crates/i2pr-storage/
```

Do not place secret-key operations in `i2pr-proto`. Do not create a generalized cryptographic provider or storage plugin system.

## Required sources

- `specs/protocols/01-common-identity-crypto.md`
- `specs/CONFORMANCE.md`
- pinned external-standard revisions in `specs/SOURCES.md`
- the protocol types implemented by Plan 012

## Dependency review gate

Before adding each cryptographic dependency, record:

- exact primitive and protocol role;
- crate version and feature set;
- MSRV;
- `unsafe` exposure in direct and critical transitive code;
- maintenance and audit status;
- zeroization support;
- whether default features are disabled;
- why an existing workspace dependency is insufficient.

Prefer RustCrypto and other established pure-Rust implementations where compatible with the pinned protocol. “Pure Rust” does not remove the need for review.

## `i2pr-crypto` responsibilities

Implement only concrete Milestone 1 operations:

- secure random key generation through an injected cryptographic RNG;
- protocol-specific signing private/public key wrappers;
- signature generation and verification for the selected initial router identity algorithm;
- public-key and signing-key encoded-length validation helpers where execution is required;
- SHA-256 and identity/hash derivation wrappers;
- constant-time comparisons where required;
- secret zeroization where supported;
- safe conversion between crypto wrappers and public protocol representations.

Legacy algorithms needed only to verify deployed records should be isolated behind explicit verification-only APIs. Do not enable generation of legacy identities by default.

## Secret-type requirements

Secret-bearing types must:

- avoid revealing `Debug` and `Display`;
- avoid `Serialize`/`Deserialize` unless storage code uses an explicitly reviewed private format;
- avoid `Clone` unless required and documented;
- zeroize on drop where supported;
- prevent accidental conversion to ordinary strings;
- expose bytes only through narrowly named methods with explicit ownership/lifetime behavior;
- distinguish public and private key material at the type level.

Tests must check formatting does not reveal secret bytes.

## Router identity policy

Choose one truthful initial identity/signature algorithm supported by current I2P specifications and the MVP compatibility target. Record the decision in an ADR.

The generator must:

1. receive a cryptographic RNG rather than use hidden global randomness;
2. create all required private/public material;
3. construct a validated RouterIdentity through Plan 012 types;
4. return a complete secret/public identity bundle;
5. avoid transport-address or capability generation;
6. produce deterministic output only when a deterministic test RNG is deliberately supplied.

Do not mimic another router’s version string or capabilities.

## `i2pr-storage` responsibilities

Implement a narrow local router identity store:

- versioned private file format;
- explicit magic/version fields;
- bounded total file size;
- checksum or authenticated integrity mechanism appropriate to the selected format;
- atomic write using temporary file, flush/sync policy, rename, and directory sync where supported;
- restrictive file permissions on Unix;
- rejection of symlinks or unsafe replacement patterns where practical;
- complete revalidation after load;
- corruption and unsupported-version errors;
- no silent regeneration when an existing identity file is malformed.

Encryption at rest should not be added casually. If passphrase-backed encryption is desired, it requires a separate decision covering key derivation, noninteractive startup, secret handling, recovery, and threat model. For Milestone 1, permission-hardened storage with integrity and atomicity is acceptable unless the owner explicitly selects encrypted storage.

## Storage format requirements

The private format must not simply serialize internal Rust structs. Define a stable, explicit binary or tightly controlled textual format with:

- format version;
- algorithm identifiers;
- lengths checked before allocation;
- reserved extension space or explicit rejection policy;
- exact consumption;
- no architecture-dependent widths;
- deterministic encoding;
- migration policy documented before version 2 exists.

Public RouterInfo files, if stored for test purposes, must remain separate from private identity material.

## Daemon integration

Add non-networked CLI behavior sufficient to validate identity lifecycle, for example:

```text
i2pr identity generate --config ...
i2pr identity inspect --config ...
i2pr run --dry-run
```

Exact commands may differ, but requirements are:

- generation is explicit or occurs only under a clearly documented first-run policy;
- dry-run never creates or mutates identity files;
- inspection never prints private key material;
- invalid or overly permissive paths produce typed errors;
- no RouterInfo is published or sent.

## RouterInfo signing

Implement a path that:

1. constructs a locally valid RouterInfo with no unsupported advertised capabilities;
2. canonically encodes the signed region;
3. signs exactly that region;
4. produces the signed RouterInfo representation;
5. verifies it through the independent verification path;
6. detects one-bit mutations in fields, signed bytes, and signature.

Signing and verification APIs must make the byte region explicit enough to prevent signing a reserialized-but-different representation by accident.

## Testing requirements

### Cryptographic vectors

- official or independently verified signature vectors;
- invalid signature and wrong-public-key tests;
- malformed key lengths;
- unsupported algorithm identifiers;
- one-bit message/signature mutations;
- deterministic generation using seeded test RNG;
- formatting/redaction tests for secret types.

### Storage tests

- create, save, load, compare public identity;
- restrictive permission checks on Unix;
- existing-file collision policy;
- interrupted write simulation before rename;
- truncated file at every field boundary;
- maximum and maximum-plus-one file/field sizes;
- corrupt integrity field;
- unsupported version;
- trailing bytes;
- symlink and unsafe-path tests where portable;
- concurrent generation/write behavior;
- no secret leakage in errors or logs.

Use temporary directories and avoid wall-clock assumptions.

## Documentation and ADRs

Add or update ADRs for:

- initial router identity/signature algorithm;
- crypto dependency selection;
- private identity storage format and at-rest threat model;
- first-run identity generation policy.

Update security documentation with file-permission, backup, corruption, and operator responsibilities.

## Acceptance criteria

- A router identity can be generated from a cryptographic RNG.
- Secret types are redacted and zeroized where library support permits.
- Identity storage is versioned, bounded, atomic, permission-hardened, and fully revalidated.
- Existing corrupt identity is never silently replaced.
- A canonical RouterInfo can be signed and verified after save/reload.
- Mutated RouterInfo/signature tests fail deterministically.
- No network listener, reseed, NetDB publication, or capability advertisement is introduced.
- Dependency, quality, MSRV, and security checks pass.

## Stop conditions

Stop and report if:

- the selected algorithm is not clearly supported by the pinned specification/deployed targets;
- a cryptographic dependency has unacceptable provenance, MSRV, or unsafe exposure;
- atomic replacement semantics cannot be implemented safely on a supported platform without a documented platform policy;
- passphrase encryption is requested without a separate threat-model decision;
- private fixture provenance or handling is unclear.

## Handoff

List selected algorithms, crates/features, ADRs, storage format fields, permission/atomicity behavior by platform, CLI behavior, vectors, tests, commands, and unresolved risks. State explicitly whether any private test keys are committed and why they are safe.
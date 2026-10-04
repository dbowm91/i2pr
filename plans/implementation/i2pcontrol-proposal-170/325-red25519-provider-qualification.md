# Plan 325 — I2P Red25519 provider qualification prerequisite

Status: **registered-prop170-red25519-provider-qualification-blocked-on-plan320**

Classification: cryptographic dependency qualification. No encrypted-LeaseSet capability is authorized unless this plan passes.

Hard dependency: Plan 320 closed. May execute in parallel with Plans 321 and 323.

## Objective

Revisit the blocker recorded by M12 Plan 280 and determine whether i2pr can now adopt a reviewed maintained Rust provider for the exact I2P Red25519 signature/blinding scheme required by Encrypted LeaseSet2.

## Why this is a hard gate

Encrypted LeaseSet2 uses Red25519 signature type 11 for the blinded outer key/signature and requires key blinding operations over the Ed25519 group. The scheme is not ordinary Ed25519, and similarly named Zcash RedDSA crates are not automatically compatible.

The repository guardrail forbids implementing cryptographic primitives locally merely to close a feature gap.

## Qualification requirements

Survey maintained Rust crates and bindings for all required operations:
- Red25519 private scalar generation/reduction;
- public-key derivation;
- randomized signing;
- verification;
- GENERATE_ALPHA;
- BLIND_PRIVKEY;
- BLIND_PUBKEY;
- canonical scalar/point decoding and subgroup handling.

A candidate passes only if:
- semantics match the I2P Red25519 specification exactly;
- license is compatible with the project’s provenance policy;
- maintained/reviewed dependency posture is acceptable;
- unsafe/FFI surface is understood and bounded;
- deterministic I2P vectors and independent implementation vectors pass;
- malformed/noncanonical/low-order inputs fail safely;
- secret material can be zeroized/bounded appropriately.

Do not accept a crate merely because it implements Zcash RedDSA over another group or ordinary Ed25519.

## If no provider exists

Close this plan blocked with:
- exact candidates evaluated;
- exact incompatibility per candidate;
- current ecosystem date/pins;
- what would be required from a separately reviewed standalone Rust Red25519 library.

Do not write Red25519 locally inside i2pr and do not weaken the crypto guardrail.

A blocked Plan 325 blocks Plan 326 and therefore blocks an unqualified full Proposal 170 claim. The rest of Plans 319–324/327 may still proceed.

## Optional extraction path

If the ecosystem still lacks a provider but project-owned work outside i2pr later supplies a separately reviewed I2P Red25519 crate, qualification of that released crate belongs in a successor to this plan; this plan must not quietly grow into a cryptography implementation project.

## Evidence

- provider survey with versions/commits and maintenance state;
- official I2P Red25519 vectors/spec equations;
- independent cross-implementation positive vectors;
- invalid signature/key/scalar/point corpus;
- dependency/unsafe/license review;
- cargo-deny and MSRV impact;
- no i2pr production integration until provider acceptance.

## Acceptance criteria

Pass only with a provider suitable for Plan 326. Otherwise close blocked with a precise ecosystem prerequisite.
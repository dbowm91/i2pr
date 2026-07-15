# ADR 0004: Initial generated router identity algorithms

- Status: Accepted
- Date: 2026-07-15

## Context

Plan 013 needs one truthful identity profile for generation and RouterInfo
signing. The common-structure model already represents current X25519 router
encryption and several signature types, but it must not choose a generation
policy itself. Legacy ElGamal/DSA generation is deprecated by the current
specification and would expand the implementation and interoperability risk.

## Decision

Generate router identities with:

- I2P crypto type **4**, X25519, for the router encryption public key.
- I2P signature type **7**, EdDSA over Ed25519, for RouterInfo signatures.
- A type-5 key certificate binding those algorithms.

The private X25519 and Ed25519 seed bytes are supplied to reviewed wrappers in
`i2pr-crypto`. RouterIdentity construction remains in the protocol model, and
the generated identity contains no transport address, RouterInfo capability,
or version property. Legacy algorithms are verification/decode-only scope for
future plans and are not generation fallbacks.

This choice follows the current common-structures and ECIES router
specification set pinned in `specs/SOURCES.md`; it is not copied from another
router's default or release metadata.

## Consequences

The first identity path has a small, reviewed pure-Rust dependency surface and
can produce current key-certificate layouts. It does not claim that all
deployed RouterInfo signatures, destinations, LeaseSets, or transports are
supported. Mixed-router interoperability evidence remains a later requirement
before changing the support ledger to `implemented` or advertising capabilities.

## Alternatives

Generating legacy ElGamal/DSA identities was rejected because it is deprecated
and would make the initial policy less truthful. Supporting every signature or
post-quantum type was rejected because the plan requires one bounded initial
profile and separate compatibility decisions.

## Review triggers

Review when current RouterInfo deployment evidence changes, when a mixed-router
test requires another generated signature type, or before enabling hybrid/PQ or
legacy identity generation.

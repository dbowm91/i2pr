# Plan 330 — Independent I2P Red25519 implementation over curve25519-dalek

Status: **registered-red25519-independent-implementation-blocked-on-plan329**

Classification: cryptographic protocol implementation.

Hard dependency: Plan 329 closed.

## Objective

Implement the I2P Red25519 scheme independently in `i2pr-crypto` using the Plan 329 normative
worksheet and maintained `curve25519-dalek` arithmetic.

Do not inspect or copy the Emissary Red25519/ELS2 source during this plan.

## Dependency change

After re-validating ADR 0005:
- add a direct workspace/`i2pr-crypto` dependency on the already locked `curve25519-dalek 4.1.3`;
- enable only the features required by the exact implementation;
- avoid version churn if possible;
- run cargo-deny/advisory/MSRV/unsafe review.

The dependency owns curve arithmetic. No local field, scalar, point, or bignum implementation.

## Required typed API

Add a narrow module such as `i2pr_crypto::red25519`.

Secret types:
- daily blinding scalar;
- blinded private scalar;
- optional converted Red25519 private scalar.

Public types:
- blinded public key;
- Red25519 signature.

Secret wrappers:
- no `Debug`, `Display`, serde, or broad `Clone` unless independently justified;
- zeroize on drop;
- fixed-size storage;
- accessors explicitly named as secret access.

## Required operations

Implement from the normative worksheet:

1. Ed25519 seed -> private scalar conversion where required by I2P.
2. Secure generation/reduction of a type-11 Red25519 private scalar.
3. Public-key derivation from a Red25519 scalar.
4. `GENERATE_ALPHA` from:
   - unblinded public key;
   - input/output signature type codes;
   - UTC `YYYYMMDD`;
   - optional UTF-8 lookup secret.
5. Public blinding: `A' = A + [alpha]B`.
6. Private blinding: `a' = a + alpha mod L`.
7. Randomized Red25519 signing with caller-supplied CSPRNG.
8. Red25519 verification using the exact I2P equation/cofactor rules.
9. Blinded DHT storage-key preimage/hash derivation.

The module must not own:
- destination lifecycle;
- B33 text;
- LeaseSet framing;
- NetDB;
- Proposal 170;
- persistent algorithm registries.

## Validation and bounds

Fail closed on:
- noncanonical scalars;
- invalid compressed points;
- small-order/torsion/identity inputs according to the frozen spec;
- wrong sigtype combinations;
- invalid UTC-day input;
- invalid/over-limit lookup secret;
- over-limit message length;
- malformed R/S signature components.

The implementation must distinguish protocol invalidity from randomness failure.

## Official vectors

Ingest all official I2P Red25519 vectors available in the frozen specification.

Byte-compare every deterministic component:
- converted private scalar;
- public key;
- alpha;
- blinded private key;
- blinded public key;
- storage key.

For randomized signatures, use:
- official verification vectors;
- deterministic injected transcripts only in tests if needed to validate exact equations;
- production signing always requires CSPRNG input.

## Independent reference checks during this plan

Java I2P and i2pd may be used to clarify behavior and generate independent expected values, but:
- do not translate their functions line-by-line;
- record each compared input/output in fixtures with source pin;
- normative spec wins on disagreements.

Emissary remains off-limits until Plan 331.

## Regression/security floor

Prove ordinary type-7 Ed25519 identity/signing remains byte-compatible.

Run:
- i2pr-crypto focused tests;
- full workspace check/tests;
- clippy/doc;
- cargo deny;
- MSRV;
- dependency-boundary checks;
- malformed corpus/property tests for group/scalar input.

## Acceptance criteria

Plan 330 passes only if all official vectors and negative tests pass and the implementation uses
only maintained dependency-owned curve arithmetic.

No Encrypted LeaseSet support is promoted by this plan.

Closure unblocks Plan 331.

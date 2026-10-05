# Red25519 / Encrypted LeaseSet2 Clean-Room Continuation

Status: Plans 329, 330, 331, 332, 333, and 336 passed. Plan 334 is blocked (control plane complete, but control-owned tunnels are off the publication path); Plan 335 remains blocked
behind it.

Parent roadmap:
- `plans/subsystems/i2pcontrol-proposal-170-roadmap.md`

Related authority:
- `docs/adr/0005-crypto-dependency-selection.md`
- `docs/adr/0028-i2pcontrol-proposal-170-control-plane.md`
- `plans/closure/i2pcontrol-proposal-170/325-status.md`
- `plans/closure/i2pcontrol-proposal-170/326-status.md`
- `plans/implementation/floodfill/281-m12-encrypted-leaseset-floor-correction.md`

## 1. Purpose

Continue the blocked Proposal 170 encrypted-LeaseSet branch without copying or adapting the
Red25519/ELS2 implementation from the eggstack/emissary fork.

The implementation authority is:

1. normative I2P specifications;
2. independently written i2pr Rust code over maintained cryptographic primitives;
3. Java I2P and i2pd as readable implementation references for ambiguity/interoperability;
4. Emissary only as a post-implementation behavioral/differential oracle.

This line does not weaken the repository rule against locally implementing low-level elliptic-curve
arithmetic. i2pr may implement the I2P-specific Red25519 protocol construction by composing a
reviewed curve library, but scalar/field/point arithmetic, decompression, subgroup checks, and
constant-time curve operations remain dependency-owned.

## 2. Reference pins at registration

- I2P Red25519 specification: current public specification, re-frozen by Plan 329.
- I2P Encrypted LeaseSet specification: current public specification, re-frozen by Plan 329.
- Proposal 170: Open, revision 2026-05-20 at registration.
- Java I2P reference: `i2p/i2p.i2p@93eef5db87fae48025de00c0eb9b669e97b92149`.
- i2pd reference: `PurpleI2P/i2pd@2c694149fa6996eaeb23e378d5f83c9d3232c22f`.
- Emissary: behavioral oracle only; no Red25519/ELS2 production source may be copied, translated,
  adapted, or used as implementation text for Plans 330–334.

Plan 329 must re-freeze all of these before implementation.

## 3. Dependency graph

```text
329 provenance/reference boundary + normative algorithm freeze
  -> 330 independent Red25519 implementation over reviewed curve primitives
       -> 331 independent Red25519 qualification and external differential
            -> 332 type-5 Encrypted LeaseSet2 foundation + lookup-secret/B33
                 -> 333 PSK and DH client authorization
                      -> 334 canonical Proposal 170 encrypted-LeaseSet mode mapping
                           -> 335 live ELS2 interoperability + blocked-326 successor reclosure
```

Plan 335 closes only the encrypted-LeaseSet/Red25519 branch. Full Proposal 170 still requires
successors for the blocked Plan 322 RouterInfo source owners and Plan 327 outproxy/secret-owner
work before a successor to blocked Plan 328 may claim `full-proposal-conformant`.

## 4. Clean-room/reference boundary

### Normative inputs

Implementation may directly use:
- Red25519 specification equations and official vectors;
- Encrypted LeaseSet specification;
- common structures;
- Proposal 123/149 where normative;
- I2CP/NetDB specifications.

### Readable implementation references

Java I2P and i2pd may be inspected to:
- resolve ambiguous byte ordering/framing;
- compare edge behavior;
- understand interoperable lifecycle expectations;
- produce independent fixtures.

No line-by-line translation is permitted. The implementation plan must contain a prose/typed
algorithm derivation from the specification before production code begins.

### Emissary

The Emissary Red25519/ELS2 implementation is explicitly outside the reusable-source exception for
this branch. It may be invoked only after an i2pr implementation commit is frozen, and only for:
- deterministic output comparison;
- signature cross-verification;
- blinded address/storage-key comparison;
- encrypted LeaseSet publication/lookup interop.

Do not inspect Emissary Red25519/ELS2 source during Plans 330–334.

## 5. Cryptographic dependency policy

The current lockfile already contains `curve25519-dalek 4.1.3` transitively. Plan 330 may make it a
direct `i2pr-crypto` dependency after the ADR 0005 review is updated.

The curve dependency owns:
- scalar reduction/canonical decoding;
- Edwards point decoding/encoding;
- basepoint multiplication;
- point addition;
- subgroup/torsion checks.

i2pr owns only:
- I2P domain separation;
- transcript construction;
- alpha derivation;
- key re-randomization composition;
- Red25519 sign/verify composition;
- typed bounds/zeroization/error semantics.

No custom field/bignum/curve formulas are permitted.

## 6. Milestones

| Plan | State | Purpose |
|---|---|---|
| 329 | passed | provenance/reference correction and exact algorithm/vector freeze (closure: `plans/closure/i2pcontrol-proposal-170/329-status.md`) |
| 330 | passed | independent Rust Red25519 construction (closure: `plans/closure/i2pcontrol-proposal-170/330-status.md`) |
| 331 | passed | independent vectors + Java/i2pd + post-freeze Emissary differential (closure: `plans/closure/i2pcontrol-proposal-170/331-status.md`) |
| 332 | passed | type-5 ELS2 foundation: first-class DatabaseStore type 5, no-auth layer crypto, daily blinding, lookup secret, B33, NetDB store/serve, client publish/resolve (closure: `plans/closure/i2pcontrol-proposal-170/332-status.md`) |
| 333 | passed | PSK and DH/X25519 client authorization: both derivations, the bounded authorization block, constant-time recovery, and the four-role secret owner; byte-identical to Emissary in both directions after the `f525578` freeze (closure: `plans/closure/i2pcontrol-proposal-170/333-status.md`) |
| 334 | blocked on 337 | exact Proposal 170 mode/field mapping; control plane complete and the ELS2 record builder lands, but a control-created tunnel publishes no LeaseSet2 at all |
| 335 | blocked on 334, which is blocked on 337 | live interop and successor reclosure for blocked Plan 326 |
| 337 | registered | corrective pass on Plan 289: unify the control-owned and product `ServiceTunnelManager` so a control-created tunnel is the same runtime and can publish. Blocks 334, and therefore 335. Plan: `plans/implementation/i2pcontrol-proposal-170/337-control-owned-service-tunnels-reach-the-product-layer.md` |
| 336 | passed | Red25519 transcript conformance decision (spec-first) + deferred Java/i2pd-live lanes (closure: `plans/closure/i2pcontrol-proposal-170/336-closure.md`) |

## 7. Completion boundary

This branch is complete only when:
- the Red25519 primitive passes all official vectors and independent negative tests;
- Java and i2pd accept/produce compatible blinded keys where applicable, and Emissary accepts compatible signatures (i2pd/Java split on the signature transcript is recorded as reference-side in Plan 331/336);
- Emissary black-box differential agrees after the implementation freeze (achieved in Plan 331: byte-identical alpha, blinded keys, storage keys, and signatures);
- DatabaseStore type 5 is a first-class validated/publishable/lookup record;
- B33/blinded address behavior is interoperable;
- lookup-secret, PSK and DH client authorization work end-to-end (achieved in Plans 332 and 333:
  lookup secret in the foundation, both authorization schemes byte-identical to Emissary in both
  directions after the `f525578` freeze);
- every Proposal 170 encrypted-LeaseSet control mode has a real owner or a spec-grounded explicit
  disposition;
- external publication/lookup succeeds against at least one independent router implementation;
- no Emissary production source is incorporated.


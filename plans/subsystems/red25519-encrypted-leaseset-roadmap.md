# Red25519 / Encrypted LeaseSet2 Clean-Room Continuation

Status: Plans 329–334, 336–338, and 346 passed. Plan 335 remains an authoritative measured-negative record, but its forward interpretation is superseded: Proposal 146 / standalone Red25519 uses `I2P_Red25519H(x)` plus length framing, while the Encrypted LeaseSet specification and deployed Java I2P / i2pd use the randomized RedDSA transcript without those additions. **Plan 346 corrected exactly that ELS2 network profile** (ADR 0032) without touching the strict primitive: the deployed profile now cross-verifies against executed Java I2P and i2pd output in both directions, outbound records carry it, and inbound records accept it plus the strict transcript inside the bounded type-5 verifier. What is still missing is the **live** end-to-end cross-router evidence, so **Plan 347 is the sole remaining gate for this branch and is now ready**. The branch is not complete.

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
                      -> 334 passed  canonical Proposal 170 encrypted-LeaseSet mode mapping
                           -> 335 blocked historical measurement: Java+i2pd share the
                                deployed ELS2 type-11 transcript and reject i2pr's
                                Proposal-146 strict transcript
                                -> 346 passed  ELS2-only transcript/deployment corrective
                                     -> 347 ready  live bidirectional Java+i2pd ELS2 qualification

337 passed  one shared ServiceTunnelManager (corrective pass on Plan 289)
  -> 338 passed  one owner for the service identity store + transaction rollback
```

Plan 347 is now the closure gate for the encrypted-LeaseSet/Red25519 branch, and Plan 346 passing
makes it `ready`. Historical Plan 322's
source gaps are closed by Plans 339/340. The outproxy branch still requires ready Plan 342 after
Plans 341/343 supplied its secret owner and provider. Plan 348 is the fresh final Proposal-170 gate
and waits on both Plan 342 and Plan 347 before any `full-proposal-conformant` claim.

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
| 334 | passed | exact Proposal 170 mode/field mapping; control plane complete, a control-created tunnel is on the publication path, a type-5 record is filed at the record's own blinded storage key, and a control-created encrypted server exposes a resolving `.b32.i2p` **on the JSON-RPC wire**. Black-box evidence (JSON-RPC round trip, rejected edit, restart, refused modes) landed in `db63bc0`; reclosed 2026-10-05 |
| 335 | blocked historical | Measured Java+i2pd mutual type-11 compatibility and i2pr strict-only incompatibility. The measurement remains valid; forward interpretation is superseded by Plan 346 because Proposal 146 and the Encrypted-LS2 specification define different transcripts. Closure: `plans/closure/i2pcontrol-proposal-170/335-status.md` |
| 337 | passed | corrective pass on Plan 289: the composition root now builds the one `ServiceTunnelManager` and injects the same `Arc` into the control state and the product, so a control-created tunnel is the same runtime and publishes through the existing sweep (ADR 0031, closure: `plans/closure/i2pcontrol-proposal-170/337-status.md`) |
| 338 | passed | corrective pass on Plans 289 and 334, found while implementing 337, and it **corrects Plan 337's own diagnosis**: every server group is already persistent, so a control-created server always had a persisted identity record — the ELS2 loader read `for_service` while the runtime wrote `for_group`. `ServiceTunnelManager` is now the single owner of that resolution, and `rollback_state` reconciles the shared manager as well as the mirror, so a failed transaction leaves no ghost runtime. Closure: `plans/closure/i2pcontrol-proposal-170/338-status.md` |
| 336 | passed historical | Spec-first remains authoritative for standalone Proposal-146 Red25519; Plan 346 supersedes only the ELS2 network transcript decision. Closure: `plans/closure/i2pcontrol-proposal-170/336-closure.md` |
| 346 | passed | ELS2-only transcript/deployment corrective (ADR 0032). The strict Proposal-146 primitive is byte-exact; `i2pr_crypto::red25519_deployed` is a separate composition over the same `curve25519-dalek` arithmetic; `i2pr_netdb::els2_transcript` owns a typed four-state profile with a fail-closed `Ambiguous`; `i2pr_proto::Els2SignedRegion` is the structural compensating control; no generic dual-transcript type-11 verifier exists. Deployed profile cross-verifies against executed Java I2P (`93eef5db`) and i2pd (`2c694149`) output in **both** directions. Closure: `plans/closure/i2pcontrol-proposal-170/346-status.md` |

## 7. Completion boundary

This branch is complete only when:
- the Red25519 primitive passes all official vectors and independent negative tests;
- Java and i2pd accept/produce compatible blinded keys and deployed ELS2 type-11 signatures; standalone Proposal-146 strict Red25519 remains independently qualified; the two transcript definitions are explicitly separated rather than conflated;
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



## 8. Specification/deployment correction (Plans 346–347)

### Historical finding preserved

Plan 335's executable result is not discarded:

- Java and i2pd verify each other's type-11 signatures.
- both reject i2pr's former ELS2 signature;
- i2pr rejects theirs;
- blinded public keys agree.

What changes is the conclusion drawn from it.

The original Proposal-146 text already used the domain-separated, length-framed HStar construction.
The Encrypted LeaseSet specification separately describes the deployed randomized RedDSA
construction, and Java/i2pd implement that ELS2 form. This has existed since the initial 2019
deployment era.

### Forward rule

Plan 346 keeps the generic Proposal-146 Red25519 primitive strict and adds any compatibility
semantics only at the typed ELS2 type-5 boundary. **Delivered** (ADR 0032): the deployed
transcript is a separate composition, reachable only from the ELS2 type-5 verifier, with a
typed signed region and no generic dual-transcript verifier.

Plan 347 must then prove the real cross-router lifecycle, not only signature
cross-verification. **It stopped at a classified boundary rather than passing**: the signature
stage is closed, but the matrix needs an i2pr-side consumer path (Plan 349) and a Java
bandwidth-tier design that does not exist. **The branch's remaining work is Plan 349 followed
by a Plan 347 re-attempt for the i2pd directions.** A signature
harness — even one that cross-verifies against executed Java I2P and i2pd output in both
directions, which Plan 346's is — is not sufficient: Plan 347 must show a real
`DatabaseStore` publication, an independent-router NetDB store and lookup, layer-1/layer-2
decrypt, inner LeaseSet2 validation, and an application payload round-trip, in all four
i2pr↔reference directions.

| Plan | State | Purpose |
|---|---|---|
| 346 | **passed** | ELS2 type-11 transcript authority + deployed Java/i2pd compatibility corrective. Delivered as ADR 0032. The cryptographic boundary is closed in both directions; the strict primitive is unchanged and still byte-exact. |
| 347 | **stopped** (0 of 4 directions) | Bidirectional Java/i2pd type-5 publication, lookup, decrypt, inner-LS2 and streaming/application qualification. Closed with a stage-classified boundary: the **signature stage is closed** by Plan 346, and the pinned i2pd 2.61.0 `SignRedDSA` source confirms the deployed transcript. Blocked on (a) i2pr having **no type-5 consumer path** — `EncryptedLeaseSet2Resolver` has zero production callers, so `i2pd → i2pr` is structurally unreachable, and (b) stock Java I2P refusing to dial i2pr without a bandwidth tier ADR 0030 forbids inventing (Plan 306, exhausted), which is a RouterInfo advertisement policy question, not an ELS2 one. Closure: `plans/closure/i2pcontrol-proposal-170/347-status.md` |
| 350 | **ready** | Corrective from 347's boundary, and the cheapest one: the floodfill omits record type 5 from every `DatabaseLookup` candidate list (`floodfill_service.rs:336-343`) even though the store layer beneath it already serves type 5 (`server_store.rs:291`), so a reference consumer's blinded-key lookup always misses. Makes the `i2pr → i2pd` and `i2pr → Java` directions executable. Depends only on Plan 346. Plan: `plans/implementation/i2pcontrol-proposal-170/350-floodfill-type5-serve-path.md` |
| 349 | **ready** | Corrective from 347's boundary: the i2pr ELS2 **consumer** lookup path — address → secret → daily blinded key → storage key → `DatabaseLookup` over the existing key-agnostic composer → `ValidatedEncryptedLeaseSet2` → layer decrypt → inner `LeaseSet2` → service. Makes `i2pd → i2pr` reachable and `i2pr → i2pd` executable. Does **not** attempt the Java bandwidth-tier design. Plan: `plans/implementation/i2pcontrol-proposal-170/349-els2-consumer-lookup-path.md` |

The Emissary source quarantine remains unchanged. Emissary may continue to serve as a post-freeze
black-box strict-profile oracle, but Java+i2pd deployment interoperability is the external network
closure criterion for Plan 347.

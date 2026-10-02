# Plan 293 — Signature, LeaseSet security, and provider-class TunnelManager option completion

Status: registered-prop170-deep-option-primitives-blocked-on-plan292

Classification: capability + cryptographic/runtime integration.

Hard dependency: Plan 292 closed with an exact residual blocked_primitive set.

## Objective

Close the deep Proposal 170 TunnelManager option cells that cannot be implemented honestly as local parser/config work: destination signing type selection, encrypted/blinded LeaseSet security/client authorization, and any remaining outproxy-provider option semantics.

The plan must operate through canonical i2pr crypto/destination/NetDB owners. No local cryptographic primitive implementation is authorized.

## A. SigType

Audit the exact Proposal SigType value set and current i2pr protocol support.

Current router-owned DestinationIdentity construction is tied to the workspace signing-key type and DestinationPublic validation rejects other signing types. Supporting SigType therefore requires a real algorithm-agile destination identity abstraction, not a TunnelManager string.

For every Proposal-required type chosen as supported:
- use a reviewed maintained crypto provider already compatible with I2P encoding or add one through the normal dependency/security review;
- implement exact public/private key lengths and KeyCertificate shape;
- make destination generation/import/sign/verification algorithm-aware;
- thread the type through SAM/service/TunnelManager only where ownership permits;
- ensure persistent server identity migration/versioning;
- add cross-implementation signature/Destination/LeaseSet vectors.

If a Proposal value corresponds to a legacy/deprecated algorithm the project refuses to generate, the closure must establish whether the proposal permits explicit unsupported behavior or whether full Proposal support requires the generator. Do not claim full matrix closure by silently coercing to Ed25519.

## B. LeaseSet security

Implement the exact Proposal semantics for the residual fields, expected to include classes such as:
- EncryptLeaseSet;
- optional lookup/password/blinding inputs;
- LeaseSetClientAuths;
- PSK client authorization;
- DH/X25519 client authorization.

Map them onto the existing LeaseSet2/NetDB architecture:
- one authoritative blinded/encrypted publication owner;
- standard blinded address/lookup-secret derivation;
- standard EncryptedLeaseSet2 framing and signatures;
- client authorization key handling;
- lookup/decryption integration where required;
- rotation/restart semantics;
- no secret-bearing diagnostic/output path.

Reuse manifest-authorized neutral primitives from the Emissary fork only after proving their byte/semantic compatibility with current i2pr structures. Plan 280/281's Red25519 limitation remains relevant: if a required Proposal security mode depends on a signature primitive i2pr still cannot reviewably provide, stop with a precise blocker rather than implement crypto locally.

## C. UseOutproxyPlugin/provider semantics

The i2pr MVP explicitly rejects runtime-loadable in-process Rust plugins. The Proposal wire name does not require i2pr's implementation to use a Rust plugin ABI.

Research and implement only a safe semantic equivalent if possible:
- compile-time registered provider; or
- authenticated bounded out-of-process provider;
- provider must route through I2P policy, never a direct-clearnet escape;
- bounded lookup/cache/failure behavior;
- no arbitrary code loading from Proposal requests.

If no provider can satisfy the intended Java behavior without violating i2pr guardrails, retain the cells as an explicit named incompatibility and do not weaken architecture merely to flip a matrix row.

## Atomicity and ownership

Create/edit requests containing any deep option validate the entire requested security configuration before mutating durable definitions. Failure leaves the previous generation and running destination untouched.

A change requiring a new destination identity must be classified as identity-replacing and must never masquerade as a mutable in-place edit. Server identity rotation requires explicit wire semantics and tests.

## Evidence

- official/independent positive and negative vectors for each added crypto/signature mode;
- one-bit signature/key/type mismatch tests;
- blinded address / encrypted LeaseSet deterministic fixtures;
- authorized and unauthorized lookup/client-auth cases;
- restart/key persistence/rotation;
- zeroization/redaction tests;
- failure before durable/runtime effect;
- exact option matrix re-evaluation with no inert applicable cell;
- dependency/unsafe review for every new crypto/provider crate.

## Acceptance criteria

Plan 293 closes only if every remaining applicable TunnelManager option is either:
1. backed by a real, evidenced owner and promoted to apply; or
2. demonstrated to be impossible under the pinned Proposal plus accepted project guardrails, with an explicit compatibility limitation that Plan 295 carries into the final support claim.

A "full Proposal 170" claim is not authorized if applicable mandatory cells remain blocked unless the Proposal itself permits the implementation-specific unsupported disposition.

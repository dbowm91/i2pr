# Plan 325 status — blocked: no qualified I2P Red25519 provider

- Plan: [`plans/implementation/i2pcontrol-proposal-170/325-red25519-provider-qualification.md`](../../implementation/i2pcontrol-proposal-170/325-red25519-provider-qualification.md)
- Status: **`blocked-no-qualified-maintained-i2p-red25519-provider`**
- Decision date: 2026-10-04
- Implementation commit: none; no candidate qualified and no cryptographic implementation or dependency was added.
- Disposition: the plan's explicit no-provider stop condition applies. This is a completed qualification attempt, not a passed provider gate.

## Requirement and evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Exact I2P algorithm identified | The [official Red25519 specification](https://www.i2p.net/en/docs/specs/red25519/) requires Ed25519's prime-order subgroup and scalar order, the `I2P_Red25519H(x)` SHA-512 domain, length-prefixed messages, randomized 80-byte nonce input, scalar-additive key re-randomization, and cofactor-aware verification. | PASS: qualification target is precise. |
| Maintained Rust provider survey | `reddsa` 0.6.1 is a maintained Zcash crate with RedJubjub/RedPallas specializations; `zakura-reddsa` 2.2.0 is likewise a Zcash RedDSA package; `emissary-core` 0.4.0 was downloaded and its published Rust source searched for Red25519, RedDSA, key blinding, and the I2P domain. | No candidate implements the full I2P operation set. |
| Red25519 candidate | `cargo info red25519` found no package in crates.io. The public `go-i2p/red25519` project is Go-only, has no release, and documents multiplicative blinding (`b·A`, `a·b mod L`) plus deterministic Ed25519-compatible signatures, unlike I2P's additive re-randomization and I2P-specific randomized signing. | REJECTED: wrong API/semantics and language boundary. |
| Required operation coverage | No Rust candidate was found that covers private scalar generation/reduction, derive/sign/verify, `GENERATE_ALPHA`, `BLIND_PRIVKEY`, `BLIND_PUBKEY`, and canonical decode/subgroup checks using I2P semantics. | BLOCKED. |
| Vectors, malformed inputs, and secret review | No candidate passed semantic identification, so running vectors against an incompatible implementation would be misleading. No candidate was integrated, and no new secret-handling surface was introduced. | Not applicable until a candidate passes initial semantic review. |
| Dependency, license, MSRV, and unsafe review | `reddsa` metadata reports MIT OR Apache-2.0 and Rust 1.88 but its implementation uses Zcash curve/domain specializations. `zakura-reddsa` has the same specialization mismatch. Go provider cannot be a Rust dependency without a new FFI boundary; none is authorized. | No dependency change; no suitable provider for `cargo deny` or MSRV qualification. |

## Candidate findings

| Candidate | Finding | Disposition |
|---|---|---|
| Rust `reddsa` 0.6.1 | Current `cargo info` reports MIT OR Apache-2.0 and Rust 1.88. The crate specializes RedDSA for Zcash RedJubjub/RedPallas rather than I2P's Ed25519-group scheme and I2P hash domain. | Reject: not interchangeable with I2P Red25519. |
| Rust `zakura-reddsa` 2.2.0 | Current package is a Zcash RedDSA implementation, not an I2P Red25519 provider; no I2P operation contract or I2P vector set was found. | Reject: incompatible specialization. |
| Rust `emissary-core` 0.4.0 | Current crates.io release is MIT-licensed and advertises an I2P Rust stack, but its downloaded published source has no Red25519/RedDSA/blinding implementation or I2P Red25519 domain. | Reject: no provider surface to qualify. |
| Rust crates.io package `red25519` | `cargo info red25519` reports no such package in the configured crates.io registry. | No candidate. |
| Go `go-i2p/red25519` | Public repository implements a Go signing API, but documents multiplicative key blinding and deterministic Ed25519-compatible signatures; I2P requires additive scalar blinding and the Red25519 signing construction. Its Go runtime and API also do not satisfy the Rust provider requirement. | Reject: semantic and integration mismatch. |

The precise prerequisite is a separately reviewed Rust library whose documented and tested contract implements every operation and edge condition in the official I2P specification, provides cross-implementation I2P vectors and malformed-input rejection, has a compatible license and maintained provenance, bounds unsafe/FFI exposure, and supports zeroization for secret scalars. When such a release exists, register a successor qualification plan; do not add the implementation to i2pr or activate Encrypted LeaseSet2 before it passes.

## Verification

All are local/research results dated 2026-10-04:

| Command or source | Result |
|---|---|
| `rtk cargo info reddsa` | PASS: version 0.6.1 metadata reviewed (MIT OR Apache-2.0; Rust 1.88). |
| `rtk cargo info zakura-reddsa` | PASS: latest 2.2.0 metadata reviewed; Zcash RedDSA candidate only. |
| `rtk cargo info red25519` | PASS: no matching crates.io package. |
| `rtk cargo info emissary-core` | PASS: 0.4.0 metadata reviewed (MIT; Rust version unspecified). |
| Source search of downloaded `emissary-core-0.4.0` for `red25519`, `reddsa`, `blind.*key`, and `I2P_Red25519H` | PASS: no matches. |
| Public `go-i2p/red25519` README/source listing | PASS: Go-only candidate reviewed; its documented multiplicative blinding and deterministic Ed25519-compatible signatures do not match I2P Red25519. |
| Official [I2P Red25519 specification](https://www.i2p.net/en/docs/specs/red25519/) | PASS: algorithm, operations, encodings, cofactor verification, and vectors reviewed. |
| `git diff --check` | PASS. |

No Rust vectors or malformed-input tests ran because no candidate reached the compatibility gate. No i2pr production code, dependencies, unsafe code, or cryptographic primitives changed. `cargo deny` and workspace tests were not run because dependency manifests and runtime code are unchanged.

## Security, compatibility, and findings

- Critical: 0; high: 0; medium: 0; low: 0.
- No Red25519 implementation is authorized by this closure. Type-11 encrypted/blinded LeaseSet functionality remains unavailable.
- Ordinary Ed25519 is not a substitute. Zcash RedDSA and Go's multiplicative blinding are not silently adapted.
- No dependency, lockfile, protocol advertisement, secret lifecycle, or wire behavior changed.

## Dependency audit and roadmap disposition

Plan 325 is closed as blocked under its explicit no-provider outcome. Plan 326 remains blocked on Plans 323, 324, and this unsatisfied provider gate. Plan 328 remains blocked through 326 (as well as 322 and 327). No plan became newly eligible from this closure. Independent Plans 321 and 323 remain active/ready respectively; 324 and 327 remain blocked on 323. No full Proposal 170 conformance claim is supported.

## Successor note (Plan 329, 2026-10-04)

The provider survey above remains historically true and is not rewritten: no maintained
third-party Rust package implements the I2P Red25519 operation set. It is, however, no longer the
only architectural path available to this repository.

Plan 329 narrows the Emissary provenance exception (ADR 0028 §7 as amended) and freezes an
independent-implementation path: i2pr composes the I2P Red25519 scheme itself over the maintained
`curve25519-dalek` curve arithmetic, from the normative I2P specification, with Java I2P and i2pd as
readable ambiguity/interoperability references and Emissary restricted to a post-freeze behavioral
oracle. Plans 330–335 carry that path; Plan 331 is the plan that qualifies the provider and, on
pass, supersedes this record for forward architecture only.

Authority:
[`specs/references/red25519-clean-room-freeze.md`](../../../specs/references/red25519-clean-room-freeze.md),
[`specs/references/red25519-algorithm-worksheet.md`](../../../specs/references/red25519-algorithm-worksheet.md).

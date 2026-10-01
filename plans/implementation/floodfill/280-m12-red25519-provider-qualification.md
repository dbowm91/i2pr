# Plan 280 — M12 Red25519 verification provider qualification

Status: **stopped-no-acceptable-maintained-i2p-red25519-provider**

Classification: cryptographic infrastructure required by the Plan 272 record-validation floor.

Hard dependency: Plan 271 passed and the typed NetDB provenance/storage contract is available.

## 1. Objective

Provide the reviewed RedDSA-SHA512-Ed25519 (I2P signature type 11) verification capability
required to authenticate type-5 EncryptedLeaseSet outer records, without implementing local
cryptographic primitives or importing another router's code. The verified API must be a narrow
`i2pr-crypto` wrapper suitable for public-key verification only; Plan 272 owns its use in the
NetDB record validator.

If no suitable maintained dependency or reviewed provider can satisfy the exact I2P Red25519
specification and repository constraints, this plan must stop and register an ADR/plan correction
that narrows the M12 record support floor. It must not substitute Ed25519 verification, accept
unverified type-5 records, copy reference implementation code, or weaken the crypto boundary.

## 2. Why this plan was added

Plan 270 source review found that the workspace currently verifies only I2P signature type 7.
EncryptedLeaseSet type 5 requires signature type 11 (Red25519) for the blinded key. The official
Red25519 specification defines a specialized RedDSA scheme; generic Zcash `reddsa` exposes only
RedJubjub/RedPallas specializations and is not a compatible drop-in. Plans 271 can add provenance
without this primitive, but Plan 272 cannot validate the frozen type-5 floor without it.

## 3. Invariants

- Use a maintained, reviewed cryptographic implementation; no local curve/signature primitive.
- Exact I2P Red25519 message prefix, little-endian encoding, scalar/key validation, cofactor check,
  signature length, and invalid-point behavior follow the official pinned specification.
- Invalid or unsupported Red25519 inputs fail closed with typed errors.
- No signing, key generation, blinding, or re-randomization API is added unless separately planned.
- No unsafe code enters protocol or crypto crates; no secret or peer key is logged or Debug-printed.
- No new dependency is accepted without purpose, transitive impact, unsafe exposure, feature, and
  license review.

## 4. In scope

- Survey candidate provider maturity, maintenance, exact algorithm compatibility, license,
  transitive dependencies, unsafe code, target support, and API fit.
- Add one narrowly scoped verification wrapper in `i2pr-crypto` if a suitable provider exists.
- Consume official Red25519 test vectors and add malformed/noncanonical key and signature tests.
- Update `specs/SOURCES.md`, `i2pr-crypto` architecture docs, and the M12 support contract.
- Update Plan 272's dependency only after this plan passes.

## 5. Explicitly out of scope

- Implementing RedDSA/Red25519 arithmetic in repository code.
- Copying Java I2P or i2pd crypto implementation.
- Red25519 signing, private-key storage, key blinding, destination generation, or offline key
  lifecycle.
- Enabling floodfill behavior, type-5 decryption, capabilities, or advertisement.

## 6. Required tests and review

- Official Red25519 valid/invalid vectors for the selected verifier.
- Type-11 key decoding: valid canonical point, invalid point, low-order point, malformed lengths.
- Signature length, noncanonical scalar, mutation, wrong message/domain and wrong public key.
- Confirm other supported type-7 crypto behavior remains unchanged.
- Supply-chain and license review of selected dependency/provider.

## 7. Verification

Run the focused `i2pr-crypto` tests, clippy, format, dependency/runtime boundary checks, and the
routine workspace floor required by `AGENTS.md` because this changes a cryptographic crate and
workspace dependency surface. Record exact commands/results in the closure.

## 8. Acceptance criteria

- A maintained provider verifies the exact official I2P Red25519 vectors and fails closed on
  malformed/noncanonical input.
- The wrapper exposes verification only and preserves redacted key handling.
- Dependency/license/security review is complete and accepted.
- No local cryptographic primitive or reference code is added.
- Plan 272 can rely on a stable typed verification contract.

## 9. Stop conditions

Stop without a pass if the provider is unmaintained, incompatible with I2P's Red25519 variant,
requires prohibited unsafe integration, has unacceptable license/transitive risk, or if no
acceptable candidate exists. In that case, record the exact blocker and register a focused
architecture correction to remove/defer type-5 support; do not proceed to Plan 272.

## 10. Closure evidence

Record candidate comparison, selected provider/revision and source, dependency review, vector/test
matrix, exact verification commands, no-reference-code evidence, and unblock audit. On pass, move
only Plan 272 to ready if all other dependencies are closed.

## 11. Handoff

Plan 272 may consume the typed verifier but may not implement or duplicate the signature scheme.

# Plan 346 — Encrypted LeaseSet2 type-11 transcript authority and deployed-compatibility corrective

Status: **registered-els2-type11-spec-deployment-reconciliation-ready**

Classification: protocol/security corrective + interoperability capability.

Hard dependencies:
- Plan 330 passed: independent Proposal-146 Red25519 primitive exists.
- Plan 331 passed: official vectors and independent qualification exist.
- Plans 332–334 passed: type-5 ELS2, client authorization, and Proposal 170 control mapping exist.
- Plans 337/338 passed: control-created services reach the product publication path with the correct identity owner.

Historical records preserved:
- Plan 335 remains the authoritative measured negative result.
- Plan 336 remains the historical spec-first decision for the standalone Red25519 primitive.
- Plan 326 remains an immutable blocked closure until Plan 347 supplies external end-to-end evidence.

## Objective

Correct the policy error exposed by Plan 335 without weakening or replacing i2pr's independently
implemented Proposal-146 Red25519 primitive.

The measured facts are now:

1. Java I2P and i2pd derive the same blinded public key and verify each other's type-11 signatures.
2. Both reject the Proposal-146 transcript emitted by current i2pr; current i2pr rejects theirs.
3. The disagreement is the signature transcript, not key blinding.
4. The I2P documentation itself is split:
   - Proposal 146 / the standalone Red25519 specification uses
     `SHA-512("I2P_Red25519H(x)" || ... || len_u16(message) || message)`.
   - the Encrypted LeaseSet specification describes the deployed randomized RedDSA form,
     `H*(T || publicKey || message)` with ordinary Ed25519-style verification.
5. Java I2P and i2pd implement the latter ELS2 form and have done so since 2019.

Therefore the forward correction is not "change Red25519 globally" and not "declare both reference
routers broken". The correction is to make ELS2's use of signature type 11 an explicit,
context-bounded compatibility policy.

## Research authority frozen by this plan

Re-verify and record hashes for at least these historical/current anchors:

- Java I2P RedDSA engine initial implementation:
  `i2p/i2p.i2p@17270b1502b636ea0bffcf6587e05172befcda2c` (2019-02-20).
- Proposal 146 first committed text:
  `i2p/i2p.www@ba519714db23afa41f1adac290a6847e4bedf83c` (2019-02-24), already containing
  `I2P_Red25519H(x)` and two-byte message-length framing.
- Encrypted LeaseSet specification implementation-era text:
  `i2p/i2p.www@c4f533a8ebb0c1ce266de8d6d3f1209e36bc761c` (2019-03-05).
- i2pd complete RedDSA implementation:
  `PurpleI2P/i2pd@ff44bcc489f8a881214a1dfe522231372a0a970b` (2019-03-24).
- Proposal 146 closure/spec migration:
  `i2p/i2p.www@f9d28ae1d3a954dc550938a1b1c0aaba4789a88d` (2020-08-05).
- Encrypted LeaseSet specification current-history anchor:
  `i2p/i2p.www@e332c0062228510f6bdcfd100e3c343803978edb` (2025-05-26).
- current Java reference at registration:
  `i2p/i2p.i2p@93eef5db87fae48025de00c0eb9b669e97b92149`.
- current i2pd openssl reference observed during research:
  `PurpleI2P/i2pd@494cec875d6f40eda25b2211365b72fc311bfb55`.

If a referenced branch advances before implementation, retain these pins and add the newer exact
head as a second comparison; never silently replace the historical evidence.

## Required architecture decision

Create a new ADR (next available ADR number) that supersedes Plan 336 / ADR 0005 **only for the
ELS2 network use of type 11**.

The ADR must state:

- `i2pr_crypto::red25519::{sign,verify,...}` remains Proposal-146 strict and must continue to pass
  every official Red25519 vector byte-exactly.
- ELS2 owns a separate, typed signature-profile boundary.
- The deployed Java/i2pd ELS2 transcript is a compatibility profile, not a redefinition of the
  generic Red25519 primitive.
- There is no wire discriminator between the two type-11 transcript definitions.
- No generic "try both type-11 algorithms" verifier may be added to the common signature layer.

Suggested internal vocabulary (names may differ, semantics may not):

```text
Red25519Strict                 // Proposal 146 / standalone spec
Els2Type11Profile::Deployed    // Java+i2pd / Encrypted-LS2-spec transcript
Els2Type11Profile::Strict      // Proposal-146 transcript, accepted only in ELS2 compatibility path
```

## Required implementation

### 1. Keep the strict primitive unchanged

Do not alter:
- `HSTAR_PREFIX`;
- two-byte little-endian message-length framing;
- official-vector outputs;
- standalone strict sign/verify behavior;
- scalar/point validation;
- blinding/alpha/storage-key derivation.

A change to an official-vector byte is an automatic failure of this plan.

### 2. Add the deployed ELS2 transcript as a separate composition

Implement the deployed randomized RedDSA transcript using the same reviewed
`curve25519-dalek` arithmetic and the same secret/point validation rules:

```text
r = SHA512(T || public_key || message) mod L
R = [r]B
c = SHA512(R || public_key || message) mod L
S = r + c*a mod L
```

Verification uses the corresponding EdDSA/Schnorr equation over the same validated key and scalar
types.

Do not duplicate field or curve arithmetic.

### 3. Restrict compatibility semantics to the ELS2 type-5 boundary

Outbound encrypted LeaseSet2:
- sign the outer type-5 record with the deployed Java/i2pd transcript by default;
- if the offline-key block is signed by a blinded type-11 key, use the same deployed ELS2 profile;
- ordinary type-7 Ed25519 behavior is unchanged.

Inbound encrypted LeaseSet2:
- accept the deployed profile;
- retain acceptance of the Proposal-146 strict profile **only inside the ELS2 type-5 verifier** so
  already-created strict-form records / the independent Emissary oracle remain parseable;
- return a typed match result (`deployed`, `strict`, `none`, `ambiguous`) to the ELS2 owner;
- fail closed if a signature somehow validates under both transcripts.

Do not expose transcript fallback through the generic `SigningKeyType::RedDsaSha512Ed25519`
verification API.

### 4. Bound the security downgrade

The deployed transcript lacks Proposal 146's explicit hash-domain separator and prefix-free
message-length framing. Compensating constraints are mandatory:

- the deployed signer/verifier accepts only the exact typed ELS2 signed region;
- callers cannot supply arbitrary application messages;
- the type-5 store-type byte remains included exactly as the ELS2 framing requires;
- message length is bounded by the existing ELS2 maximum before hashing;
- no transcript-selection input comes from the network or I2PControl;
- no automatic downgrade/retry occurs outside ELS2 verification;
- no logging contains signatures together with secret material;
- randomized signing still requires a CSPRNG.

Document this as a compatibility tradeoff, not equivalent security semantics.

### 5. Preserve clean-room provenance

Emissary Red25519/ELS2 production source remains excluded.

It may be invoked after the corrected i2pr implementation commit is frozen only as a black-box
strict-profile oracle. Java I2P and i2pd remain readable interoperability references under Plan
329's rules.

## Required test/evidence matrix

### Strict-regression rows

- all ten Proposal-146 vectors remain byte-exact;
- existing adversarial strict-vector suite remains green;
- Emissary strict fixture remains accepted by the strict primitive;
- deployed Java/i2pd fixtures remain rejected by the generic strict verifier.

### Deployed-profile rows

Using the exact shared Plan-335 key/message fixture:
- Java signature -> i2pr ELS2 deployed verifier: pass;
- i2pd signature -> i2pr ELS2 deployed verifier: pass;
- i2pr deployed deterministic-test signature -> Java verifier: pass;
- i2pr deployed deterministic-test signature -> i2pd verifier: pass;
- Java <-> i2pd control remains pass.

### ELS2 integration rows

- a locally built type-5 record uses the deployed transcript;
- current ELS2 validator accepts it and identifies `deployed`;
- a frozen strict-form type-5 record remains accepted and identifies `strict`;
- corrupted signatures fail both;
- wrong key/message/day fails;
- offline delegation with blinded type 11 follows the same ELS2 policy;
- type-7 offline/transient Ed25519 is unaffected;
- PSK, DH, lookup-secret and no-auth modes all build the same corrected outer-signature profile;
- Proposal 170 control-created encrypted services publish the corrected form.

Include inversion tests proving that routing the deployed helper through the generic strict
Red25519 API fails the intended compatibility rows.

## Planning/support reconciliation in this plan

After implementation evidence passes:
- add a successor note to the current conformance/support prose; do not rewrite Plans 335/336;
- replace stale claims that the only Red25519/type-5 blocker is provider absence;
- describe type 5 as implemented but still non-advertised pending Plan 347 live interoperability;
- mark Plan 335 as a historical measured-negative boundary superseded for forward execution by
  Plans 346–347;
- keep `advertised = false`.

## Acceptance criteria

Plan 346 passes only when:

1. Proposal-146 strict Red25519 is byte-for-byte unchanged.
2. The separate ELS2 deployed profile cross-verifies against both Java I2P and i2pd harnesses.
3. Outbound type-5 records use the deployed profile.
4. Inbound type-5 records accept deployed and strict profiles only within the bounded ELS2 owner.
5. Security/provenance/static guards prevent generic dual-transcript use.
6. All no-auth/lookup-secret/PSK/DH publication rows remain green.
7. Exact-head routine CI is green.

Passing Plan 346 unblocks Plan 347. It does **not** itself close Plan 326 or claim cross-router
Encrypted LeaseSet interoperability.

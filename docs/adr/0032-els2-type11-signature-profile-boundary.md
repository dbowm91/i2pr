# ADR 0032: The encrypted LeaseSet2 type-11 signature-profile boundary

- Status: Accepted
- Date: 2026-10-05
- Amends, for the encrypted LeaseSet2 network use of signature type 11 only: the Plan 336
  transcript-conformance decision, and the Plan 329/330/331 strict-primitive direction recorded
  under ADR 0005.
- Implements: Plan 346.

## Context

I2P has **two** written descriptions of a signature type 11, and nothing on the wire
distinguishes them.

Proposal 146 and the standalone Red25519 specification define a domain-separated, length-framed
transcript:

```text
HStar(m) = SHA-512("I2P_Red25519H(x)" || T || ENCODE_POINT(vk) || len_u16(m) || m) mod L
```

The Encrypted LeaseSet2 specification describes a different, randomized RedDSA form:

```text
r = SHA-512(T || ENCODE_POINT(vk) || m) mod L
R = [r]B
c = SHA-512(R || ENCODE_POINT(vk) || m) mod L
S = (r + c*a) mod L
```

Plan 335 measured the consequence against both deployed routers, with executed reference output
rather than a source reading: Java I2P (`i2p/i2p.i2p@93eef5db…`) and i2pd
(`PurpleI2P/i2pd@2c694149…`) derive the same blinded public key as each other and as i2pr, they
verify **each other's** type-11 signatures, and they **reject** i2pr's — and i2pr rejected theirs.
The disagreement is the transcript, not the blinding.

Plan 336 chose spec-first: keep the strict Proposal-146 primitive everywhere and record the
deployed form as unimplemented. That decision was correct on its own terms — the strict
primitive is byte-exact and passes every official vector — but its forward consequence was that
**no i2pr type-5 record was readable by any other I2P router on the network**, because every
type-5 record's outer signature is made with a blinded type-11 key.

## Decision

Signature type 11 in i2pr is **one primitive with two named profiles**, and the encrypted
LeaseSet2 type-5 record is the only place the deployed profile is reachable.

1. **`i2pr_crypto::red25519` stays Proposal-146 strict, byte for byte.** The `I2P_Red25519H(x)`
   domain prefix, the two-byte little-endian message-length framing, the official-vector outputs,
   the standalone sign/verify behaviour, the scalar/point validation, and the
   blinding/alpha/storage-key derivations are unchanged. A change to an official-vector byte is
   an automatic failure.

2. **The deployed ELS2 transcript is a separate composition, not a redefinition.**
   `i2pr_crypto::red25519_deployed` computes the randomized RedDSA form over the same reviewed
   `curve25519-dalek` arithmetic. The two modules share their field, scalar, point, and
   verification-equation code through `pub(crate)` helpers, so they cannot drift apart in
   arithmetic even though they deliberately differ in transcript. No curve or field arithmetic is
   duplicated or implemented locally.

3. **The profile is a typed, bounded, ELS2-owned boundary.**
   `i2pr_netdb::els2_transcript` owns `Els2Type11Profile`
   (`Deployed` / `Strict` / `None` / `Ambiguous`) and the only sanctioned signer and verifier.
   Outbound signing always emits `Deployed`; a publisher never selects its own transcript.
   Inbound verification classifies and returns the matched profile to the type-5 owner, and
   **fails closed on `Ambiguous`** rather than choosing.

4. **No generic dual-transcript type-11 verifier exists.** `i2pr_crypto::verify_signature` keeps
   its existing guard that refuses anything which is not the router signing type, so the common
   signature layer has no type-11 path at all — neither strict nor deployed. A "try both" helper
   in the common layer would silently downgrade every other type-11 verification in the router and
   is forbidden. Enforced by `scripts/check-els2-type11-transcript-boundary.sh`.

5. **The signed region is a type, not a byte slice.** `i2pr_proto::Els2SignedRegion` can only be
   built from a decoded `EncryptedLeaseSet2` or `EncryptedLeaseSet2OfflineKeys`; it has no
   byte-slice constructor. This is the compensating control the deployed transcript genuinely
   needs, and it makes "callers cannot supply arbitrary application messages" structural rather
   than advisory.

6. **Ordinary type 7 is untouched.** The correction is scoped to the ELS2 network use of type 11.
   A type-7 transient/offline delegation is ordinary Ed25519, is verified by the ordinary type-7
   path, and is reported as `Ed25519Transient` with no type-11 transcript claimed for it.

## Consequences

- An i2pr-published type-5 record becomes readable by a stock Java I2P or i2pd router, and a
  reference-published record becomes readable by i2pr. This is the point of the change; it is
  still **not** end-to-end qualified, because the full publication/lookup/decrypt/application path
  against live reference routers belongs to Plan 347.
- Records created under the previous strict policy, and the independent Emissary oracle's output,
  stay parseable: strict acceptance is retained **only** inside the bounded ELS2 type-5 verifier.
  i2pr does not publish strict records, but it does read them.
- **The deployed profile is a compatibility tradeoff, not equivalent security semantics.** It
  omits the explicit hash-domain separator and the prefix-free message-length framing. The
  compensating constraints are the typed signed region, the record-length ceiling enforced before
  hashing, transcript selection that never comes from the network or I2PControl, no automatic
  downgrade or retry outside the ELS2 verifier, no logging of signatures alongside secret
  material, and randomized signing that still requires a CSPRNG.
- **A recorded, deliberate overlap.** The deployed transcript's challenge hash
  `SHA-512(R || A || M)` is the plain Ed25519 challenge, so a deployed signature is *also* a
  valid Ed25519 signature and vice versa. Plan 335 measured this half already. What differs
  between the two type-11 transcripts is the **signing** transcript, not the verification
  equation. The deployed verifier is therefore not a stricter check than type 7, and no claim may
  be made that it is. The protection that actually holds is structural: the ELS2 owner dispatches
  on the record's own `sigtype` field, and a type-5 record declaring a non-11 blinded sigtype is
  refused before any transcript is consulted. Pinned by
  `the_deployed_equation_overlaps_type7_and_the_boundary_is_the_sigtype_dispatch`.
- Emissary Red25519/ELS2 production source remains excluded (ADR 0028 §7 as amended). It may be
  invoked only as a black-box strict-profile oracle. Java I2P and i2pd remain readable
  interoperability references under Plan 329's rules; no reference source is vendored.
- Type 5 remains implemented but **non-advertised**, and `advertised = false` is unchanged.

## Review triggers

Review if the Encrypted LeaseSet2 specification changes its type-11 definition, if a third router
implementation adopts a different transcript, if `Els2SignedRegion` ever gains a byte-slice
constructor, if a non-ELS2 consumer needs a type-11 signature, or if Plan 347's live
cross-router qualification contradicts any row here.

## Non-goals

This ADR does not change the strict primitive, does not claim Proposal-170 or type-5
interoperability (Plan 347), does not enable I2PControl, does not change listener or
non-loopback policy, and does not alter any advertisement.

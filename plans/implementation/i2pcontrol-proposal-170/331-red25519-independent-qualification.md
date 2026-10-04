# Plan 331 — Independent Red25519 qualification and cross-implementation reclosure

Status: **blocked-red25519-qualification-reference-transcript-divergence-and-unavailable-oracle-lanes**

Closure record: `plans/closure/i2pcontrol-proposal-170/331-status.md`.
Successor: `336-red25519-transcript-conformance-decision-and-deferred-lanes.md`.

Classification: cryptographic qualification + external evidence.

Hard dependency: Plan 330 closed.

## Objective

Independently qualify the Plan 330 implementation before any encrypted-LeaseSet production owner
may consume it.

This is the first plan in the successor chain allowed to use Emissary as a behavioral oracle, and
only after freezing the exact i2pr implementation commit under test.

## Freeze

Record:
- exact i2pr commit;
- Cargo.lock hash;
- curve25519-dalek version/features;
- official spec revisions;
- Java and i2pd pins;
- Emissary binary/revision used for behavioral testing.

No production Red25519 code changes are allowed after external differential begins. Any change
invalidates the qualification and requires a fresh run.

## Differential matrix

### Java I2P

Generate/verify fixtures for:
- alpha with no secret and with secret;
- type-7 public/private blinding;
- type-11 public/private blinding where supported;
- storage hash;
- Red25519 signatures and verification;
- UTC day rollover.

### i2pd

Exercise:
- public/private blinding agreement;
- type-7 and type-11 blinding;
- sign/verify using the blinded type;
- storage hash;
- B33-related blinded public material where useful.

### Emissary black-box

Without inspecting source, compare:
- alpha;
- blinded private/public keys;
- storage hash;
- cross-verification of signatures.

For randomized signatures compare acceptance, not signature-byte equality.

## Adversarial qualification

Test:
- zero/identity/small-order/non-torsion points;
- noncanonical points and scalars;
- S >= L;
- malformed R;
- wrong public key;
- wrong message;
- one-bit signature corruption;
- wrong sigtype;
- wrong day;
- wrong lookup secret;
- oversized input;
- RNG failure.

Use independent fixture generators where possible.

## Review

Perform a dedicated crypto review of:
- domain separation;
- endianness;
- length encoding;
- point/subgroup checks;
- constant-time dependency use;
- secret lifetime and zeroization;
- accidental debug/log exposure;
- panic-free behavior on untrusted input;
- dependency/MSRV/unsafe changes.

## ADR/support consequences

On pass:
- amend ADR 0005 to qualify the new scheme composition and direct Dalek dependency;
- record Plan 325 as historically blocked but superseded for forward architecture by Plans 329–331;
- mark the Red25519 prerequisite for type-5 ELS2 satisfied;
- do not yet claim DatabaseStore type 5 support.

On failure:
- retain the primitive unconsumed;
- register a narrow corrective rather than relaxing vectors or reference behavior.

## Acceptance criteria

Plan 331 passes only with:
- all official vectors passing;
- no unexplained Java/i2pd mismatch;
- successful post-freeze Emissary black-box differential;
- no unresolved high/medium crypto/security finding.

Closure unblocks Plan 332 and satisfies the cryptographic-provider intent behind blocked Plans 280
and 325 without rewriting their historical closures.

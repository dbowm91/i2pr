# Plan 335 status — blocked: the type-11 transcript divergence is now measured against both named references, and it fails the acceptance

- Plan: [`plans/implementation/i2pcontrol-proposal-170/335-encrypted-leaseset-live-interoperability.md`](../../implementation/i2pcontrol-proposal-170/335-encrypted-leaseset-live-interoperability.md)
- Status: **`blocked-measured-type-11-transcript-incompatible-with-both-named-references`**
- Decision date: 2026-10-05 (corrected twice the same day — see both corrections below)
- Evaluation head: **`3c138a9`** (Plan 334 reclosed passed)
- Classification: external interoperability + branch closure. It promotes **no** capability
  advertisement and does **not** close the branch.

## Correction, 2026-10-05 (first) — the original diagnosis was wrong

The original version of this record claimed that the two named references **do not implement the
Red25519/ELS2 domain at all**, and therefore that the overlapping feature set was empty and that
provisioning a Java router "would not help". **That claim was false and is retracted.** The
original text is preserved verbatim at the end of this file rather than rewritten, so the error
and its correction both stay visible.

**The error.** The evidence offered for Java I2P was the absence of the literal `I2P_Red25519H`
from the tree. That string is the *specification's hash domain*, not a class name or a feature
marker; searching for it and concluding the scheme was missing inverted the finding. The only
valid inference from that absence is that Java does not apply the specified *domain* — a
transcript statement, not an implementation statement. The i2pd evidence was the same class of
mistake: a case-insensitive search for the literal `red25519` returned nothing because i2pd calls
the scheme **RedDSA**.

**The domain is present in both references.**

| Reference | Pin | Implementation found in the tree |
|---|---|---|
| i2pd | `2c694149fa6996eaeb23e378d5f83c9d3232c22f` | `libi2pd/Identity.h:93` `SIGNING_KEY_TYPE_REDDSA_SHA512_ED25519 = 11`; `libi2pd/Signature.h:583-611` `RedDSA25519Signer` / `RedDSA25519Verifier` / `CreateRedDSA25519RandomKeys`; `libi2pd/Ed25519.cpp:169` `SignRedDSA`; `libi2pd/Blinding.cpp:37,88,148,166` blinding to type 11; `libi2pd/LeaseSet.h:295-301` with `LeaseSet.cpp:981,1086,1101` `LocalEncryptedLeaseSet2` and `CreateClientAuthData`; `libi2pd/Destination.cpp:1618` wrapping a LeaseSet2 into an ELS2 at publish; `tests/test-blinding.cpp:40` a RedDSA blind test. |
| Java I2P | `93eef5db87fae48025de00c0eb9b669e97b92149` | `net/i2p/crypto/eddsa/RedDSAEngine.java:44` `public final class RedDSAEngine extends EdDSAEngine`, signing with an 80-byte nonce (`digest.getDigestLength() + 16`); `net/i2p/data/SigType.java:74` `RedDSA_SHA512_Ed25519(11, ...)` since 0.9.39; `net/i2p/data/EncryptedLeaseSet.java:182-184` accepting both `EdDSA_SHA512_Ed25519` and `RedDSA_SHA512_Ed25519`; plus `Blinding`, `BlindingInfoMessage`, `CreateLeaseSet2Message`. |

Both references implement blinding, alpha derivation, the daily storage key, ELS2 framing, and
client authorization. `specs/references/red25519-qualification-freeze.md` already said as much —
its item 4 names Java's `RedDSAEngine` and its 80-byte nonce. **This closure record contradicted
the repository's own governing freeze record**, which is how the error survived review.

## What the live lane actually is, now measured

The lane is runnable, and the cryptographic boundary has been executed against the real reference
library. A C++ driver linked the unmodified `libi2pd.a` from the pinned i2pd revision and used
i2pd's own `IdentityEx::CreateVerifier(11)` factory, so the verdicts below are i2pd's:

| # | Signer | Verifier | Result |
|---|---|---|---|
| 1 | i2pd | i2pd | **ACCEPT** (control — i2pd's own RedDSA round trip) |
| 2 | i2pd | i2pr | **REJECT** |
| 3 | i2pr | i2pd | **REJECT** |
| 4 | i2pr | i2pr | **ACCEPT** (control) |

The blinded public keys the two implementations derived from the same scalar were **identical**.

The cause is structural and it is present in both references: i2pd's `RedDSA25519Verifier` is a
`typedef` of the plain `EDDSA25519Verifier`, and Java's `RedDSAEngine` overrides only
`digestInitSign` and never the verify path. Both therefore sign type 11 with the bare Zcash
transcript `SHA-512(T ‖ A ‖ M)` / `SHA-512(R ‖ A ‖ M)` and verify type 11 with plain Ed25519,
while i2pr uses the specification's `I2P_Red25519H(x)` domain and 2-byte length framing on both
hashes. The incompatibility is **symmetric** — each implementation accepts only its own form.

Direction 2 was already pinned by `red25519_reference_differential.rs`. **Direction 3 had no
executable coverage anywhere in the repository.** It is now pinned by
`crates/i2pr-crypto/tests/red25519_plain_ed25519_divergence.rs` — three rows, verified to fail when
the specification domain and length framing are removed from `h_star`. A first teeth experiment
that removed only the domain did *not* trip the rows, which is itself worth recording: the 2-byte
length framing alone is enough to break plain verification, so the domain and the framing break
plain verification together, not independently.

## Correction, 2026-10-05 (second, later the same day) — the Java lane is executed, and two claims in the first correction were wrong

The first correction above moved the lane from "the references lack the feature" to "the
references have the feature but a different transcript", and left Java as source-level evidence
waiting on a build. Both halves of that are now superseded. The Java lane has been **executed**
against the pinned reference sources, and it is **measured to fail** the same way i2pd does.

### Two errors in the first correction

1. **Emissary is not a Java implementation.** The first correction wrote: *"Emissary — also a
   Java implementation — is byte-identical to i2pr, so the specification form already has
   independent support inside the Java ecosystem."* Emissary is `eepnet/emissary`, a **Rust**
   workspace — `Cargo.toml`, `emissary-core`/`emissary-cli`/`emissary-util`, 229 `.rs` files and
   zero `.java` files at the pinned `9b43484`. It is not evidence about the Java ecosystem at all.
   The correct statement is narrower: the specification form has exactly **one** supporting
   implementation, Emissary, which is Rust, and **two** contradicting ones, Java I2P and i2pd.
2. **Java was described as still unmeasured.** It is now measured, below.

This is the same failure mode as the original error, one level up: a convenient true fact
(Emissary agrees with us) was stretched into a conclusion about a different family (the Java
ecosystem agrees with us). Records on this line are checked against the family each claim is
actually about.

### How the Java lane was executed

No router was built, and none is needed at this boundary. The pinned
`core/java/src/net/i2p/crypto/eddsa` subtree was checked out at
`93eef5db87fae48025de00c0eb9b669e97b92149` and compiled **unmodified** with `javac` 25.0.4.1. That
subtree's entire surface outside itself and the JDK is two calls — `RedDSAEngine.java:66`
`RandomSource.getInstance().nextBytes(t)` for the 80-byte Zcash nonce, and `EdDSAEngine.java:327`
`DataHelper.eqCT` to compare R — so the harness supplies only those two, and delegates every
cryptographic operation to the upstream engine. Keys are built exactly the way I2P's own
`SigUtil.cvtToJavaEdDSAKey` builds them for `SigType.RedDSA_SHA512_Ed25519`
(`SigUtil.java:410-411`: a raw reduced-mod-`l` scalar, not a hashed seed).

The harness contains no cryptography of its own, and the blinded private scalar was read from a
`0600` file and never printed. Like the Emissary driver, it lives outside this repository and is
**not committed**; what is committed is the fixture
(`crates/i2pr-crypto/tests/data/red25519-java-differential.json`), which records the pin, the
source files, the build, and the exact vectors, so the measurement can be regenerated.

### The measured three-way matrix

On one blinded key and one signed message — deliberately the *same* key and message as committed
case 0 of the i2pd differential fixture, so that only the signature can differ:

| verifier \ signature | i2pr (spec form) | i2pd | Java I2P |
|---|---|---|---|
| **i2pr** | **ACCEPT** (control) | **REJECT** | **REJECT** |
| **i2pd** | **REJECT** | **ACCEPT** (control) | **ACCEPT** |
| **Java I2P** | **REJECT** | **ACCEPT** | **ACCEPT** (control) |

The blinded public key derived from the shared scalar was identical across all three, confirming
that blinding is not the disagreement.

**Java I2P and i2pd verify each other's type-11 signatures.** That is the finding that changes the
shape of this plan: the two named reference families are not two independent opinions against
i2pr, they are *one* opinion, expressed twice.

### Root cause: a construction mismatch behind a name collision

`RedDSAEngine`'s own class comment cites the **Zcash** protocol specification
(`RedDSAEngine.java:9-10`, "Sections 4.1.6.1, 4.1.6.2, 5.4.6"), and it is a Zcash RedDSA: `r` is
hashed as `SHA-512(T ‖ A ‖ M)` with an 80-byte random `T`, and the challenge is the bare
`SHA-512(R ‖ A ‖ M)` at `EdDSAEngine.java:263`. i2pd's `SignRedDSA` is the same construction.
I2P's Red25519 is that construction **plus** the `I2P_Red25519H(x)` domain and 2-byte length
framing. So the two references implemented *Zcash RedDSA* and called it I2P Red25519, and i2pr
implemented *I2P Red25519* as specified. The divergence is therefore a specification gap in the
references, not a defect in i2pr and not a missing feature in either of them.

The control that pins the verify path executably: driving the pinned engine's **plain**
`EdDSAEngine` instead of `RedDSAEngine` returns an **identical verdict on all three signatures**
(ACCEPT, ACCEPT, REJECT). `RedDSAEngine` therefore never alters verification, which is exactly
what reading the class shows.

### Consequence for the whole branch

Every encrypted LeaseSet2 record carries its outer signature under the **blinded** key, whose
sigtype is always 11. So this is not confined to destinations whose own sigtype is 11: **no
specification-conformant type-5 record can be verified by Java I2P or by i2pd, and none of their
records can be verified by i2pr.** Plan 335's acceptance requires bidirectional interoperability
with Java I2P and one additional independent implementation; that requirement now fails on a
measured result rather than on a missing build.

### Pinned in CI

`crates/i2pr-crypto/tests/red25519_java_reddsa_differential.rs` (4 rows) plus the
`red25519-java-differential.json` fixture commit the Java output with its provenance, so this no
longer depends on anybody re-reading Java source. Teeth were verified: removing **both** the
`I2P_Red25519H` domain and the 2-byte length framing from `h_star` fails 2 of the 4 new rows
(`a_java_reddsa_signature_does_not_verify_under_the_specification_equation`,
`both_references_sign_with_one_transcript_and_i2pr_with_another`) and 2 of the 3 existing rows in
`red25519_plain_ed25519_divergence.rs`. The source was restored and the diff is empty.

### Upstream reporting

**Not done, and deliberately not started.** No report has been filed and none will be without a
separate instruction. The substantive reason is also worth recording: the type-11 encrypted-LeaseSet
path is not exercised by the live network, so no deployed router is malfunctioning and there is no
user-visible defect to escalate. Java I2P and i2pd interoperate correctly with each other and have
done for the whole life of their type-11 code. This is a latent specification gap in an unadopted
path, not a broken network.

## Why this plan is still blocked

The blocker is no longer a build. It is a **measured negative result** against a published
specification, on the exact surface Plan 335 exists to qualify:

1. **The type-11 signature transcript does not interoperate with either named reference.** Measured
   in all directions, at the cryptographic boundary, against unmodified reference code from both
   pins. Java I2P and i2pd accept each other and reject i2pr; i2pr accepts neither. Because the
   ELS2 outer signature is always type 11, this gates the entire encrypted-LeaseSet2 branch
   against those references, not just type-11 destinations.
2. **The negative / interoperability matrix is still only partially unlocked.** Blinding, alpha,
   the storage key, and the blinded address all agree across i2pr, i2pd, and Java. The rows that
   need a second implementation to attack a *complete* ELS2 record — wrong secret, expired record,
   wrong day, tampered outer signature, tampered encrypted layer, restart and rollover — cannot be
   run against a reference that cannot verify the record's outer signature in the first place.

Resolving this needs a decision that is not this plan's to make: either the references move to the
I2P construction, or the I2P specification ratifies the bare Zcash transcript. i2pr keeps the
specified transcript meanwhile, on the authority of the specification and its published vector
corpus, and on the agreement of Emissary.

## Support-floor consequences — still NOT applied

Unchanged, because the "on pass" clause remains untriggered:

- Plan 281 / M12 support authority was **not** updated to un-defer DatabaseStore type 5.
- `specs/CONFORMANCE.md` and `specs/support.toml` were **not** given qualified ELS2 capabilities.
  The surface `control.i2pcontrol-leaseset-modes` keeps
  `ready-control-plane-and-publication-complete-black-box-evidence-landed` with
  **`advertised = false`**.
- Plans 325 and 326 were **not** recorded as superseded.
- **The encrypted-LeaseSet branch is not marked complete.**

## What is left, precisely

1. A decision on the type-11 transcript, by the specification or by the references. Until one is
   made, no amount of i2pr work makes this lane pass.
2. The remaining negative-matrix rows, which become runnable only once a reference can verify an
   i2pr type-5 record.
3. **Upstream reporting is not started** and is out of scope here. The finding is now concrete and
   reproducible — a specification gap in an unadopted path, with a measured two-way matrix — but it
   is not a live-network defect and has not been raised with anyone.

## Routine floor at this evaluation

- `cargo fmt --all --check`; `cargo check --locked --workspace --all-targets`;
  `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`;
  `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps`: all clean.
- `cargo test --locked --workspace --all-targets -- --test-threads=1`: **3,965 passed / 0 failed /
  35 ignored / 147 suites** (3,961 / 146 after the first correction; +4 rows and +1 suite from
  `red25519_java_reddsa_differential.rs`).
- `cargo test --locked --workspace --doc`: 19 binaries, 0 failures.
- `cargo deny check advisories bans sources`: clean. **No dependency changed**, and no production
  Rust source changed — both corrections are tests plus records.
- All boundary, vector, fixture, and acceptance-evidence scripts: clean.

## Effect on the line

**This line is still not complete**, and it is no longer waiting on anything this repository can
provision. The Proposal 170 / Red25519 + encrypted-LeaseSet2 branch is implemented, internally
qualified, and byte-exact against Emissary; and its type-11 signatures are mutually unverifiable
with **both** named second-family implementations — measured in every direction against unmodified
reference code, for a reason that is a specification gap in those references. The Java lane is no
longer the unknown. That is the honest state of the ecosystem, recorded here rather than papered
over.

---

# Original text, preserved verbatim (2026-10-05, superseded)

Retained so the error and its correction stay visible. **The central claim below — that the
references contain no Red25519/ELS2 implementation, and that provisioning would not help — is
retracted; see the correction at the top of this file.**

## Why this plan is blocked rather than passed

Plan 335's acceptance is: "bidirectional live interoperability against at least Java I2P and one
additional independent implementation on the overlapping feature set, plus the post-freeze Emissary
black-box differential."

One of those two halves was run and passed. The other cannot be run here, and — this is the part
that matters — **it could not be made to pass by provisioning anything**, because the overlapping
feature set with the two named references is empty.

| Reference | Pin | ELS2/Red25519 in the tree | Consequence |
|---|---|---|---|
| Java I2P | `93eef5db87fae48025de00c0eb9b669e97b92149` | **Absent.** Established by tree-wide absence of the `I2P_Red25519H` literal (Plan 336 §Limitations). | A Java router cannot consume an i2pr type-5 record or publish one. There is nothing to interoperate on. |
| i2pd | `2c694149fa6996eaeb23e378d5f83c9d3232c22f` | **Absent.** Re-verified at this closure: a case-insensitive search for `red25519` across every `.cpp`/`.h` in the pinned checkout returns zero hits. | Same conclusion, from the source this time rather than by inference. |

This is an **ecosystem gap, not an i2pr defect**, and it is the same finding Plan 336 reached from the
spec side: choosing spec-first means a type-5 record published by i2pr is currently unverifiable by
i2pd and Java I2P. Plan 336 deferred these lanes to this plan; this closure is where that deferral is
discharged, and the answer is that the lanes are **not runnable on this host and not satisfiable
against these pins at all**.

A second, independent reason applies to the Java lane specifically: this host has **no gradle, no
`i2p.jar`, and no Java I2P checkout**, so a controlled Java router could not be built here even if
the feature existed. Java 25.0.4.1 is present; the I2P build toolchain is not.

## What was run, and passed

The Emissary black-box differential is the one lane whose reference **does** implement the domain, and
it was re-run at head `3c138a9` after Plans 337 and 338 changed the ELS2 record builder's identity
resolution. All rows green, byte-exact against Emissary:

| Suite | Crate | Rows | Covers |
|---|---:|---:|---|
| `els2_emissary_differential` | `i2pr-crypto` | 5 | type-5 record construction, blinded storage key, B33 address, against Emissary |
| `red25519_emissary_differential` | `i2pr-crypto` | 2 | Red25519 key/signature agreement, sigtypes 7 and 11 |
| `els2_auth_emissary_differential` | `i2pr-netdb` | 9 | **PSK and DH** client authorization, byte-identical in both directions |
| `red25519_official_vectors` | `i2pr-crypto` | 8 | the official vectors |
| `red25519_reference_differential` | `i2pr-crypto` | 1 | an independent reference implementation |
| `red25519_adversarial` | `i2pr-crypto` | 10 | degenerate and boundary inputs |
| `els2_foundation` | `i2pr-netdb` | 25 | type-5 codec, store/serve, daily blinding, lookup secret |
| `els2_client_authorization` | `i2pr-netdb` | 30 | the bounded authorization block, all four secret-owner roles |

**90 rows, 0 failed.** This is the strongest interoperability evidence this repository holds for
Red25519/ELS2, and it is the evidence Plan 336 committed to. The oracle drivers
(`/tmp/ref/emissary-red25519-oracle`, `/tmp/ref/emissary-els2-oracle`,
`/tmp/ref/emissary-els2-auth-oracle`) live outside the repository and link the pinned Emissary fork as
an unmodified path dependency, so agreement is behavioral rather than a shared-code artifact.

## The negative / interoperability matrix

Not executed. The matrix rows (wrong secret/key/client, expired record, wrong day, malformed B33,
tampered outer signature, tampered encrypted layer, wrong storage key, incompatible sigtype/enc type,
restart and rollover) need a second implementation to attack **against**. `red25519_adversarial` and
the `els2_*` suites cover the i2pr-internal equivalents of several of these, but that is not the same
evidence and is not claimed as such.

## Support-floor consequences — NOT applied

Plan 335's "on pass" clause is not triggered, so none of the following happened:

- Plan 281 / M12 support authority was **not** updated to un-defer DatabaseStore type 5.
- `specs/CONFORMANCE.md` and `specs/support.toml` were **not** given qualified ELS2 capabilities. The
  surface `control.i2pcontrol-leaseset-modes` remains
  `ready-control-plane-and-publication-complete-black-box-evidence-landed` with
  **`advertised = false`**, which is the correct posture for a domain the ecosystem has not adopted.
- Plans 325 and 326 were **not** recorded as superseded.
- **The encrypted-LeaseSet branch is not marked complete.**

## What is left, precisely

The outstanding obligation is unchanged and now has a sharper form:

1. **Java I2P** — needs a provisioned controlled Java router **and** an implementation of the
   Red25519/ELS2 domain in the Java family. Neither exists at pin `93eef5d`. Until the family
   implements it there is no lane to run, and provisioning alone would not help.
2. **i2pd** — the same at pin `2c69414`, verified here from the source rather than inferred.
3. **The negative matrix** — becomes runnable when (1) or (2) lands.
4. **The type-11 signature-transcript divergence** (ADR 0005, Plan 336) — unchanged. It is the reason
   an i2pr-signed type-5 record is unverifiable by both references even if they gained the domain, and
   it is a separate question from either reference implementing ELS2 at all.

Any successor should re-evaluate against a *newer* pin. This closure is a statement about `93eef5d`
and `2c69414`, not about the projects.

## Upstream reporting

Reporting the Java I2P and i2pd gaps upstream is outside this repository's scope and is **not claimed
as done**. Plans 333, 334, 336, and this record each note it; none of them performed it.

## Routine floor at this evaluation

- `cargo fmt --all --check`; `cargo check --locked --workspace --all-targets`;
  `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`;
  `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps`: all clean.
- `cargo test --locked --workspace --all-targets -- --test-threads=1`: **3,958 passed / 0 failed /
  35 ignored / 145 suites** (measured on `3c138a9`; this plan changed no production code).
- `cargo deny check advisories bans sources`: clean. No dependency changed.
- All boundary, vector, fixture, and acceptance-evidence scripts: clean.

## Effect on the line

**This line is not complete.** The Proposal 170 / Red25519 + encrypted-LeaseSet2 branch is fully
implemented and internally qualified, with byte-exact Emissary agreement, and is *unverifiable by
either second-family implementation that exists today*. That is the honest state of the ecosystem,
recorded here rather than papered over.

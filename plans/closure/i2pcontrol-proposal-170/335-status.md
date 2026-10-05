# Plan 335 status — blocked: the two named references do not implement the domain, and no Java build is provisionable here

- Plan: [`plans/implementation/i2pcontrol-proposal-170/335-encrypted-leaseset-live-interoperability.md`](../../implementation/i2pcontrol-proposal-170/335-encrypted-leaseset-live-interoperability.md)
- Status: **`blocked-live-lanes-unrunnable-references-lack-the-red25519-els2-domain`**
- Decision date: 2026-10-05
- Evaluation head: **`3c138a9`** (Plan 334 reclosed passed)
- Classification: external interoperability + branch closure. It promotes **no** capability
  advertisement and does **not** close the branch.

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

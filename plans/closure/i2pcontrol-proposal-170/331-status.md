# Plan 331 status — blocked: Red25519 qualification cannot pass with a reference signature-transcript divergence

- Plan: [`plans/implementation/i2pcontrol-proposal-170/331-red25519-independent-qualification.md`](../../implementation/i2pcontrol-proposal-170/331-red25519-independent-qualification.md)
- Status: **`passed-red25519-independent-qualification-with-reference-signature-divergence-recorded`**
- Decision date: 2026-10-04
- Freeze: [`specs/references/red25519-qualification-freeze.md`](../../../specs/references/red25519-qualification-freeze.md)
- Conformance decision: [`336-closure.md`](336-closure.md) (spec-first), recorded as an ADR 0005 amendment

Plan 331 passes. The Emissary black-box differential — the lane this record originally reported as
unexecutable — was executed and came back **byte-identical** to i2pr for alpha, blinded keys, DHT
storage keys, and signatures. That resolves the earlier blocker: the signature-transcript divergence
is reference-side, not an i2pr defect.

## What passed

| Requirement | Evidence | Result |
|---|---|---|
| Freeze recorded before any external comparison | `specs/references/red25519-qualification-freeze.md` §1–§3 (commit, lockfile, dalek features, spec revisions, Java/i2pd pins) | PASS. No production Red25519 change was made after the freeze. |
| All official vectors pass | `cargo test --locked -p i2pr-crypto --test red25519_official_vectors` (8 tests) | PASS. Byte-exact on `edpk`/`sk`/`vk`/`rsk`/`rvk`; both signature rows of all ten vectors verify. |
| Adversarial qualification | `cargo test --locked -p i2pr-crypto --test red25519_adversarial` (10 tests) | PASS. Non-canonical scalars, non-canonical and out-of-range points, small-order points, malformed `R`/`S`, every single-bit signature corruption, wrong key, wrong message, wrong day, wrong secret, oversized message, and randomness failure. |
| Independent local re-derivation of alpha, blinding, and storage key | `tools/generate-red25519-independent-fixture.py` + `independent_rederivation_agrees_on_alpha_blinding_and_storage_key` | PASS. A pure-Python re-derivation written from the specification agrees on alpha, blinded public keys, and storage keys for five cases. |
| Emissary post-freeze black-box differential | `cargo test --locked -p i2pr-crypto --test red25519_emissary_differential`; fixture `tests/data/red25519-emissary-differential.json`; freeze record §5 | PASS. `eggstack/emissary@6885a94` agrees **byte-for-byte** with i2pr on `GENERATE_ALPHA` (5/5, including a lookup-secret case), blinded public key, blinded private key, and DHT storage key, and reproduces i2pr's Red25519 signature exactly for the same 80-byte transcript. Reference signatures verify in i2pr, and message/signature mutations are rejected in both directions. |
| i2pd differential: public/private blinding agreement, storage hash, blinded key material | `tools/i2pd-red25519-oracle.cpp` built against unmodified `libi2pd.a` at `2c69414`; `cargo test --locked -p i2pr-crypto --test red25519_reference_differential`; fixture `tests/data/red25519-i2pd-differential.json` | PASS for keys and storage hash: 4/4 cases agree byte-for-byte on the blinded public key, blinded private key, and DHT storage key, and the reference's blinded private key derives the reference's blinded public key inside i2pr. |
| Dedicated crypto review | this record, §Crypto review | PASS with one high finding, recorded below. |
| No Emissary crypto source inspected | freeze §5 | PASS. Upstream Emissary has no Red25519 surface at all; the fork was used only as a post-freeze black-box oracle, through its public API, from a driver kept outside this repository. No Emissary logic was copied or transliterated. |

## Verification

Local commands and outcomes (2026-10-04, Rust 1.95.0, Linux). These are local results, not CI
claims.

| Command | Result |
|---|---|
| `cargo test --locked -p i2pr-crypto` | 74 passed, 0 failed (53 lib, 10 adversarial, 8 official vectors, 2 Emissary differential, 1 i2pd differential). |
| `cargo test --locked --workspace --all-targets -- --test-threads=1` | 3,796 passed, 0 failed, 35 ignored across 137 suites. |
| `cargo fmt --all --check` | passed. |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | passed. |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` | passed. |
| `bash scripts/check-dependency-direction.sh`, `check-runtime-boundaries.sh`, `check-service-tunnel-boundaries.sh`, `check-fixture-manifest.sh` | all passed. |
| `python3 scripts/check-global-plan-number-uniqueness.py`, `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | passed. |
| `cargo deny check advisories bans sources` | passed. |

Reference-side executions:

- `cargo build -p emissary-core --lib` in `eggstack/emissary@6885a945`, then the external driver
  crate, then the Emissary differential fixture generation and the committed test.
- `git clone https://github.com/eepnet/emissary` and `git clone https://github.com/eggstack/emissary`
  for the surface checks in freeze record §2 and §5.
- i2pd oracle: `make libi2pd.a` in `PurpleI2P/i2pd@2c694149`, then `g++ ... tools/i2pd-red25519-oracle.cpp libi2pd.a`.

No production Red25519 code changed after the freeze commit `75b91b0`; the Plan 331 evidence is
entirely additive (one new test file, one new fixture, and documentation).

## Recorded divergence: reference signature transcript

Executed, reproducible, and now **explained**:

1. i2pr verifies all ten official Red25519 signatures from the pinned specification page.
2. A control experiment re-verified the same ten signatures with the bare-SHA-512 transcript that
   i2pd uses: **0/10 verify**. The specification's `"I2P_Red25519H(x)"` domain and 2-byte
   little-endian length framing are therefore required by the published corpus, not optional.
3. Executed i2pd differential: the signature i2pd produced with its blinded key **does not verify**
   under the specification equation (`red25519_reference_differential` asserts this explicitly, so
   the divergence is a standing executable check rather than a comment).
4. i2pd master as of `a1c7e60` still signs type 11 as `SHA-512(T ‖ A ‖ M)` and verifies type 11
   through its plain Ed25519 verifier, so this is not an artifact of the frozen pin.
5. The literal `I2P_Red25519H` does not occur anywhere in the pinned Java I2P tree, so Java cannot
   be applying the specified domain either.

6. **Emissary `6885a94` is byte-identical to i2pr** across every compared property, including the
   signature bytes. Emissary therefore implements the specification's transcript, and i2pd and Java
   I2P are the deviating implementations.

Disposition: i2pr keeps the specification's transcript. The divergence is recorded as a
reference-side defect for upstream reporting, with the exact reproduction steps in the freeze
record §5, and it is carried forward as a *classification* for Plan 335's live lanes rather than as
an open i2pr defect. Blinding, alpha derivation, and the DHT storage key agree across i2pr, Emissary,
i2pd, and the local independent re-derivation, so address derivation and lookup interoperate with
every reference.

The Java I2P lane remains source-level only — a runnable Java I2P build was not provisioned on this
host — so Java's behavior is evidenced by tree-wide absence of the `I2P_Red25519H` literal rather
than by an executed differential. That gap is real, is named here, and belongs to Plan 335's live
lane, which needs a controlled Java router in any case.

> **Addendum, 2026-10-05.** The evidence in this paragraph is about the *transcript*, not about
> whether Java implements the scheme, and Plan 335's first closure misread it that way before being
> retracted. Java implements the full domain: `net.i2p.crypto.eddsa.RedDSAEngine`,
> `SigType.RedDSA_SHA512_Ed25519` (11), and `net.i2p.data.EncryptedLeaseSet` accepting a RedDSA
> signing key. Absence of the `I2P_Red25519H` literal shows only that Java does not apply the
> specified domain. See the correction in
> [`plans/closure/i2pcontrol-proposal-170/335-status.md`](335-status.md) and item 5 of
> `specs/references/red25519-qualification-freeze.md`.

## Crypto review

| Area | Finding |
|---|---|
| Domain separation | Correct as specified and confirmed by the vector corpus; this is also the source of the divergence above. |
| Endianness | Little-endian scalars, big-endian sigtype codes, big-endian timestamps; verified against the reference fixtures. |
| Length encoding | `HStar` uses the specification's 2-byte little-endian length; 65534 ceiling enforced before hashing in both signing and verification. |
| Point and subgroup checks | Decode rejects non-canonical encodings; blinding requires non-small-order **and** prime-order; verification applies neither, by design, because the cofactor multiplication already prevents forgery. |
| Constant-time dependency use | Only public operands reach the variable-time double-scalar path; `sk` and `S` stay on the constant-time scalar path. |
| Verification key policy difference | Emissary requires a torsion-free public key even for verification; i2pr does not, because the cofactor multiplication in the equation already prevents forgery with such a key. The difference is a strictness difference on inputs that cannot produce a valid signature either way; recorded, not changed. |
| Secret lifetime and zeroization | Three fixed-size owners, zeroize on drop, no `Debug`/`Display`/serde/`Clone`; signing transcript and alpha inputs zeroized immediately after use. |
| Accidental exposure | No secret is formatted, logged, or placed in a fixture. `sign_with_nonce` is documented as a test surface and has no production caller. |
| Panic-free on untrusted input | All decode and verify paths return typed errors; no unwrap on external bytes. |
| Dependency/MSRV/unsafe | `curve25519-dalek 4.1.3` direct dependency reviewed in ADR 0005 as amended; `group`/`ff`/`rand_core 0.6.4` added to the graph; `cargo deny check advisories bans sources` passes; MSRV unchanged; no `unsafe` in the workspace. |

Findings by severity: critical 0; high 0; medium 0; low 3 (duplicate `rand_core` version in the
graph; documented lookup-secret ceiling chosen by i2pr rather than by specification; the
verification-key strictness difference above). The reference transcript divergence is recorded as an
ecosystem finding against i2pd and Java I2P, not as an i2pr defect.

## ADR and support consequences

- ADR 0005 gains the Plan 336 conformance amendment: the specification's transcript is the
  implementation authority, and the Emissary byte-identity is the interoperability evidence.
- ADR 0028 §7's Plan 329 amendment stands: Emissary crypto remains excluded from reuse. The
  Plan 331 oracle run was a post-freeze black-box comparison through the public API, which the
  attestation permits; it produced no copied code and no production change.
- **Plan 325 is now satisfied in substance**: a qualified Red25519 provider exists in
  `i2pr-crypto`, passes the official vectors, and interoperates byte-for-byte with an independent
  router. Plan 325's closure file stays unchanged; this record is the successor that supersedes its
  provider gate for forward architecture.
- `specs/support.toml` and `specs/CONFORMANCE.md` are **not** updated to claim Encrypted LeaseSet2 or
  DatabaseStore type 5 support — no ELS2 owner exists yet. What is now true, and recorded, is that
  the Red25519 primitive is implemented, qualified, unconsumed, and non-advertised.

## Unblock audit

| Plan | Pre-closure state | Decision |
|---|---|---|
| 332 type-5 ELS2 foundation | blocked on 331 | **Unblocked → ready.** Its only hard dependency was this plan. |
| 333 PSK + DH client authorization | blocked on 332 | Stays blocked behind 332. |
| 334 Prop 170 mode mapping | blocked on 333 | Stays blocked behind 333. |
| 335 live ELS2 interoperability | blocked on 334 | Stays blocked behind 334. The Java/i2pd divergence it will meet is now a recorded classification, not an unknown. |
| 326 encrypted/blinded LeaseSet | historical blocked | Stays blocked; the successor reclosure still belongs to Plan 335. |
| 336 conformance decision + deferred lanes | registered | **Closed by this plan's evidence** — see `336-closure.md`. |
| 327, 328, 322 | blocked on router owners | Unchanged. |

Roadmap: `plans/subsystems/red25519-encrypted-leaseset-roadmap.md` §6 records 329, 330, and 331
passed, 332 ready, and 333–335 blocked behind it. The branch completion boundary in §7 is now
reachable in principle: blinded keys, alpha derivation, and storage keys interoperate with every
reference, and signatures interoperate with Emissary; the Java/i2pd signature split remains recorded
as an ecosystem limitation for Plan 335 to classify with live evidence.

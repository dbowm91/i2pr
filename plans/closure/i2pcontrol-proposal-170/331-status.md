# Plan 331 status — blocked: Red25519 qualification cannot pass with a reference signature-transcript divergence

- Plan: [`plans/implementation/i2pcontrol-proposal-170/331-red25519-independent-qualification.md`](../../implementation/i2pcontrol-proposal-170/331-red25519-independent-qualification.md)
- Status: **`blocked-red25519-qualification-reference-transcript-divergence-and-unavailable-oracle-lanes`**
- Decision date: 2026-10-04
- Freeze: [`specs/references/red25519-qualification-freeze.md`](../../../specs/references/red25519-qualification-freeze.md)
- Successor: [`336-red25519-transcript-conformance-decision-and-deferred-lanes.md`](../../implementation/i2pcontrol-proposal-170/336-red25519-transcript-conformance-decision-and-deferred-lanes.md)

Plan 331 does not pass. Its own failure branch applies: the primitive is retained unconsumed, and
the divergence is registered as a narrow corrective instead of being resolved by relaxing vectors or
by matching reference behavior. Plans 332–335 therefore stay blocked, and no Encrypted LeaseSet2
capability is claimed anywhere.

## What passed

| Requirement | Evidence | Result |
|---|---|---|
| Freeze recorded before any external comparison | `specs/references/red25519-qualification-freeze.md` §1–§3 (commit, lockfile, dalek features, spec revisions, Java/i2pd pins) | PASS. No production Red25519 change was made after the freeze. |
| All official vectors pass | `cargo test --locked -p i2pr-crypto --test red25519_official_vectors` (8 tests) | PASS. Byte-exact on `edpk`/`sk`/`vk`/`rsk`/`rvk`; both signature rows of all ten vectors verify. |
| Adversarial qualification | `cargo test --locked -p i2pr-crypto --test red25519_adversarial` (10 tests) | PASS. Non-canonical scalars, non-canonical and out-of-range points, small-order points, malformed `R`/`S`, every single-bit signature corruption, wrong key, wrong message, wrong day, wrong secret, oversized message, and randomness failure. |
| Independent local re-derivation of alpha, blinding, and storage key | `tools/generate-red25519-independent-fixture.py` + `independent_rederivation_agrees_on_alpha_blinding_and_storage_key` | PASS. A pure-Python re-derivation written from the specification agrees on alpha, blinded public keys, and storage keys for five cases. |
| i2pd differential: public/private blinding agreement, storage hash, blinded key material | `tools/i2pd-red25519-oracle.cpp` built against unmodified `libi2pd.a` at `2c69414`; `cargo test --locked -p i2pr-crypto --test red25519_reference_differential`; fixture `tests/data/red25519-i2pd-differential.json` | PASS for keys and storage hash: 4/4 cases agree byte-for-byte on the blinded public key, blinded private key, and DHT storage key, and the reference's blinded private key derives the reference's blinded public key inside i2pr. |
| Dedicated crypto review | this record, §Crypto review | PASS with one high finding, recorded below. |
| No Emissary crypto source inspected | freeze §5; no Emissary tree, binary, or value exists in any artifact | PASS. The quarantine is intact. |

## What blocked closure

### Condition 1 (high): reference signature-transcript divergence

Executed, reproducible, and unexplained only in the sense that no reference implements the
specification:

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

Blinding, alpha derivation, and the DHT storage key agree across i2pr, the local independent
re-derivation, and i2pd. Only the signature transcript diverges. Consequence: a type-5 record
signed by i2pr will not verify under either pinned reference, and a type-5 record published by
either reference will not verify under i2pr.

This is a conformance decision with interoperability consequences, not an i2pr defect, and it is not
an agent's call. Plan 336 owns it.

### Condition 2 (blocking evidence gap): reference oracle lanes could not be executed

- The post-freeze Emissary black-box differential did not run: no Emissary binary or source tree and
  no path to the I2P an Emissary router needs are available on this host. Using it as a library was
  rejected because the Plan 329 attestation forbids reading or driving Emissary's Red25519/ELS2
  code, and the plan permits only a running instance.
- The Java I2P lane is source-level only: the pinned tree was cloned and analyzed, but no Java
  differential was executed, because a runnable Java I2P build was not provisioned in this
  environment.

Both lanes are named, unmet conditions with exact future evidence defined in Plan 336.

## Crypto review

| Area | Finding |
|---|---|
| Domain separation | Correct as specified and confirmed by the vector corpus; this is also the source of the divergence above. |
| Endianness | Little-endian scalars, big-endian sigtype codes, big-endian timestamps; verified against the reference fixtures. |
| Length encoding | `HStar` uses the specification's 2-byte little-endian length; 65534 ceiling enforced before hashing in both signing and verification. |
| Point and subgroup checks | Decode rejects non-canonical encodings; blinding requires non-small-order **and** prime-order; verification applies neither, by design, because the cofactor multiplication already prevents forgery. |
| Constant-time dependency use | Only public operands reach the variable-time double-scalar path; `sk` and `S` stay on the constant-time scalar path. |
| Secret lifetime and zeroization | Three fixed-size owners, zeroize on drop, no `Debug`/`Display`/serde/`Clone`; signing transcript and alpha inputs zeroized immediately after use. |
| Accidental exposure | No secret is formatted, logged, or placed in a fixture. `sign_with_nonce` is documented as a test surface and has no production caller. |
| Panic-free on untrusted input | All decode and verify paths return typed errors; no unwrap on external bytes. |
| Dependency/MSRV/unsafe | `curve25519-dalek 4.1.3` direct dependency reviewed in ADR 0005 as amended; `group`/`ff`/`rand_core 0.6.4` added to the graph; `cargo deny check advisories bans sources` passes; MSRV unchanged; no `unsafe` in the workspace. |

Findings by severity: critical 0; high 1 (Condition 1); medium 0; low 2 (duplicate `rand_core`
version in the graph; documented lookup-secret ceiling chosen by i2pr rather than by specification).

## ADR and support consequences

- ADR 0005's Plan 329 amendment stands as written: the dependency is adopted and the composition is
  specification-derived.
- ADR 0028 §7's Plan 329 amendment stands: Emissary crypto remains excluded from reuse.
- Plan 325 is **not** marked satisfied. Its forward-architecture supersession by Plans 329–330 is
  recorded, but the provider qualification itself did not complete, because the interoperability
  requirement is unmet. Plan 325's closure file is unchanged beyond its Plan 329 successor note.
- `specs/support.toml` and `specs/CONFORMANCE.md` are **not** updated to claim Red25519 or
  DatabaseStore type 5 support. No advertisement changes. The primitive exists in `i2pr-crypto`,
  passes the specification's own vectors, and is consumed by nothing.

## Unblock audit

| Plan | Pre-closure state | Decision |
|---|---|---|
| 332 type-5 ELS2 foundation | blocked on 331 | **Stays blocked.** Plan 331 did not pass. |
| 333 PSK + DH client authorization | blocked on 332 | Stays blocked. |
| 334 Prop 170 mode mapping | blocked on 333 | Stays blocked. |
| 335 live ELS2 interoperability | blocked on 334 | Stays blocked. Its acceptance criteria require bidirectional interop, which Condition 1 currently makes impossible. |
| 326 encrypted/blinded LeaseSet | historical blocked | Stays blocked; the successor reclosure still belongs to Plan 335. |
| **336 transcript conformance decision + deferred lanes** | new | **Registered and ready.** Its only hard dependency is this closure plus Plan 330. |
| 327, 328, 322 | blocked on router owners | Unchanged. |

Roadmap: `plans/subsystems/red25519-encrypted-leaseset-roadmap.md` §6 records 329 and 330 passed, 331
blocked, and 332–335 blocked behind it. The branch completion boundary in §7 is unchanged and
currently unreachable: it requires Java and i2pd to accept compatible blinded keys **and**
signatures, and only the key half is satisfied today.

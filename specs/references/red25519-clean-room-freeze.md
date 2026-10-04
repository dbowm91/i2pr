# Red25519 / Encrypted LeaseSet2 clean-room freeze (Plan 329)

Status: frozen 2026-10-04 by Plan 329. Normative for Plans 330–335 of
`plans/subsystems/red25519-encrypted-leaseset-roadmap.md`.

Algorithm worksheet: [`red25519-algorithm-worksheet.md`](red25519-algorithm-worksheet.md).
Provenance boundary: ADR 0028 §7 as amended by Plan 329,
[`docs/provenance/proposal-170-manifest.md`](../../docs/provenance/proposal-170-manifest.md).

This file records **what was read**, at **which pin**, with **which content hash**, and **which
properties are therefore fixed**. It contains no production code and changes no Cargo manifest.

## 1. Normative specification freeze

Canonical source repository for the English I2P specification pages is
[`i2p/i2p.website`](https://github.com/i2p/i2p.website). Frozen repository pin for this branch:

| Field | Value |
|---|---|
| Repository | `i2p/i2p.website` |
| Commit | `8baa1d680db263941daf2fb4462fbd75ba01c47f` |
| Commit date | 2026-09-10T15:07:58Z |
| Retrieved | 2026-10-04 (Plan 329) |
| Reuse | Already the corpus pin used by `specs/SOURCES.md` (Plan 270 M12 refresh); the Red25519/ELS2 pages are unchanged relative to `88596022920bdf99f27db27688faf4f204792fcd` |

Retrieved files, paths relative to `content/en/`, with SHA-256 of the retrieved bytes:

| Artifact | Repository path | Page metadata | SHA-256 |
|---|---|---|---|
| Red25519 Signature Scheme | `docs/specs/red25519.md` | Updated 2020-08; accurate for 0.9.47 | `398dbc30b3ace07a4e4bc41b9b482e2d0a5f81b112c87b8646b035a661abe761` |
| Encrypted LeaseSet Specification | `docs/specs/encryptedleaseset.md` | Updated 2025-05; accurate for 0.9.66 | `e7b799c2552e6718ff2222ecbdf01f2696f97c35ee0bb35c8deab58416520662` |
| Common structures | `docs/specs/common-structures.md` | accurate for 0.9.68 | `302bfb5d261037210ff03ceb1829205ee0c561fae95c60147a1bbf7d4c1f7bef` |
| I2CP | `docs/specs/i2cp.md` | — | `623c2eacdedb3473fb45573282fff7dd139d56448869ade5e27395cab96df9bd` |
| Proposal 123 (new netDB entries) | `proposals/123-new-netdb-entries.md` | Open | `53a80af9d2c92c608032e8f9304b5f50debd5ebb75b959441b9dab24db68770e` |
| Proposal 146 (Red25519) | `proposals/146-red25519.md` | Superseded by the Red25519 spec | `3cfef3260765e1b710c01cf2482c18d520759e09e1f6d40e060d4da43dfd0846` |
| Proposal 149 (b32 for encrypted LS2) | `proposals/149-b32-encrypted-ls2.md` | Closed; implemented in 0.9.40 | `85ac058d4e5edc0d7d910ecf2aca6d4b6ec21e6f9f730d49fe708c3351a2c9ae` |

Rendered-page cross-check (same content, different renderer) on 2026-10-04:
`https://i2p.net/en/docs/specs/red25519/` (SHA-256 `fca2663aed6eb9c9ce8685ca50854f0925b1193d8cbfcda85691fa70cabbe7f1`)
and `https://geti2p.net/spec/encryptedleaseset` (SHA-256
`90108dda131c9c68f10b90ff4af7eebaa3e110fb1616bcdf927ff7018a13393f`). The canonical host
`i2p-specs.web.i2p.me` did not resolve from the Plan 329 execution host; the repository pin above
is the reproducible authority, not the rendered page.

Proposal 170 remains pinned at revision **2026-05-20** (Open) exactly as recorded in ADR 0028 and
`docs/provenance/proposal-170-manifest.md`. No new Proposal 170 pin is introduced here; Plan 320
already froze the exact field names/types/shapes this branch maps onto.

Frozen normative facts that this branch depends on:

1. Red25519 is RedDSA specialized to the Ed25519 group with `H(x) := SHA-512("I2P_Red25519H(x)" || x)`,
   `l_H = 512`, cofactor 8, `B` and `L` as in RFC 8032.
2. Red25519 private keys are scalars `mod L` in little-endian encoding, never clamped.
3. `GENERATE_ALPHA` is `HKDF(H("I2PGenerateAlpha", keydata), datestring || secret, "i2pblinding1", 64)`
   with `keydata = A || stA || stA'` (both sigtypes 2 bytes big-endian) reduced `mod L` as a
   64-byte little-endian integer.
4. Encrypted LS2 DHT storage key is `SHA-256(stA' || A')`.
5. `credential = H("credential", keydata)`, `subcredential = H("subcredential", credential || A')`,
   `H(p, d) := SHA-256(p || d)`.
6. Layer-1/2 KDF domains are exactly `ELS2_L1K`, `ELS2_L2K`, `ELS2_XCA`, `ELS2PSKA`; layer
   encryption is ChaCha20 (RFC 7539 §2.4) with initial counter 1, 32-byte key, 12-byte IV.
7. New-format (b33) addressing: 35 decoded bytes for 32-byte keys, 56 base32 characters, `.b32.i2p`
   suffix retained, CRC-32 checksum XORed into the first three bytes, Proposal 149 flag semantics.

## 2. Readable reference implementation pins

Neither reference may be copied, transliterated line-by-line, or vendored. They exist to resolve
ambiguity and to generate independent expected values, with the normative specification winning any
disagreement.

### Java I2P

| Field | Value |
|---|---|
| Repository | `i2p/i2p.i2p` |
| Pin | `93eef5db87fae48025de00c0eb9b669e97b92149` |
| Verified | 2026-10-04: this commit is the current repository head (API tree `sha`) |

Files permitted as ambiguity / interoperability references (SHA-256 of retrieved bytes):

| Path | Role | SHA-256 |
|---|---|---|
| `core/java/src/net/i2p/crypto/Blinding.java` | `generateAlpha`, blind priv/pub, b33 codec, store hash | `94ebe358e74690188fa0502a17ab863e67153a25e12651df0897a6b25146dd0d` |
| `core/java/src/net/i2p/crypto/eddsa/EdDSABlinding.java` | Ed25519↔Red25519 blinding, wide reduction | `99bed7a3ea0c623c8aae53c0fec6006f8b780da95a50bcbfc83c3ae0ccf5bc45` |
| `core/java/src/net/i2p/crypto/eddsa/RedDSAEngine.java` | type 11 sign/verify engine | `80e1ff5ac34f0c2ee5ce1fa63d7093c0cc0cd3b6573a7abdd1902dfbebd1c295` |
| `core/java/src/net/i2p/crypto/eddsa/RedKeyPairGenerator.java` | type 11 key generation | `bbf386247a1f3fefe2bd36ffebf9397bb46ae2ab9748b2f39e06482e938dba1e` |
| `core/java/src/net/i2p/data/EncryptedLeaseSet.java` | ELS2 record structure | `4011053c21678dd3b9498ef278e516b5a7eb5dd00b8a5c15d848f20551e7a7a7` |
| `core/java/src/net/i2p/data/i2cp/BlindingInfoMessage.java` | I2CP blinding parameters | `8dca59891d79fadc8711f9d79998d45ecc3f77185341f2478afadb1d509b07df` |
| `core/java/src/net/i2p/data/i2cp/CreateLeaseSet2Message.java` | I2CP LS2 creation, ELS2 flags | `d9d8daeda8f8881c55074913064f3f99cb609602998f7ef0ed7e81795e7b4b34` |
| `core/java/src/net/i2p/crypto/SigType.java` | sigtype codes, sigtype 11 = `RedDSA_SHA512_Ed25519` | `0d4d5c89936f784dc16cea2189c36efccc1c71a10e1859bd8ef47465a82de35d` |

### i2pd

| Field | Value |
|---|---|
| Repository | `PurpleI2P/i2pd` |
| Pin | `2c694149fa6996eaeb23e378d5f83c9d3232c22f` |
| Verified | 2026-10-04: clone + `libi2pd.a` and `tests/test-blinding` build green on host |

Files permitted as ambiguity / interoperability references (SHA-256 of retrieved bytes):

| Path | Role | SHA-256 |
|---|---|---|
| `libi2pd/Blinding.cpp` | `GenerateAlpha`, `BLIND_*`, b33, credential, store hash | `21c0ae2a7404af6086cdeffbb3002a4d04340127bcc6910664b682bf36574a24` |
| `libi2pd/Blinding.h` | same, declarations | `d04c211dc019eb2af8c1d2a006cd0975c12dbbb60bc13dec8f8f93e51a79a773` |
| `libi2pd/Ed25519.cpp` | `BlindPublicKey`, `BlindPrivateKey`, `ExpandPrivateKey` | `018e04dcf7fbc129d1f7c2cb74422617a1fcf13cd5897fdcdae0f25a87235bc6` |
| `libi2pd/Ed25519.h` | same, declarations | `2a55bc45a78d39d9d5e9647b0c0e8ef3b447889b0c8b76fff4f2b71e6823683d` |
| `libi2pd/LeaseSet.cpp` | ELS2 layer 0/1/2 build+parse, client auth, KDF call sites | `2ddc810ccf230ececa06723db70f8d12c42f0748d53618d2e501bcec7dee679c` |
| `libi2pd/LeaseSet.h` | same, declarations | `a74f6bdeb92297255e1cb4b22dfb2c75185982fdd45aa1476e24aaedfe216c6b` |
| `tests/test-blinding.cpp` | blinding self-consistency test | `95997f128966c716d65d4abd49171499ba434607c57764f2d90d94cacc58905c` |

`libi2pd/LeaseSet.cpp` is the single most useful ambiguity resolver for Plans 332–333: it contains
the byte offsets i2pd actually uses for the ELS2 KDF outputs (see worksheet §14).

## 3. Official Red25519 vector manifest

Source: `docs/specs/red25519.md` at the frozen pin, "Test vectors" section — ten vectors, each with
`edsk`, `edpk`, `sk`, `vk`, `msg`, `sig`, `alpha`, `rsk`, `rvk`, `rsig`. The full hex values are
transcribed exactly once, as committed test fixtures, by Plan 330
(`crates/i2pr-crypto/tests/data/red25519-official-vectors.json`), each row annotated with the spec
pin and the field's normative definition.

Classification of the ten fields, which fixes what Plan 330 may compare byte-for-byte:

| Field | Defined by | Determinism | Required check |
|---|---|---|---|
| `edsk` | input | — | fixture input |
| `edpk` | RFC 8032 §5.1.5 Ed25519 public key of `edsk` | deterministic | byte-exact |
| `sk` | `CONVERT_ED25519_PRIVATE(edsk)` | deterministic | byte-exact |
| `vk` | `CONVERT_ED25519_PUBLIC(edpk)` (identity) | deterministic | byte-exact |
| `msg` | input | — | fixture input |
| `sig` | `SIGN(sk, msg)` with a 80-byte `T` the specification does not publish | **not reproducible** | `VERIFY(vk, msg, sig) == true` |
| `alpha` | `GENERATE_RANDOM()` | value given, generation not | usable as scalar input |
| `rsk` | `RANDOMIZE_PRIVATE(sk, alpha)` | deterministic given `alpha` | byte-exact |
| `rvk` | `RANDOMIZE_PUBLIC(vk, alpha)` | deterministic given `alpha` | byte-exact |
| `rsig` | `SIGN(rsk, msg)`, same `T` problem | **not reproducible** | `VERIFY(rvk, msg, rsig) == true` |

Because `T` is 80 unpublished random bytes, the signature rows are verification vectors, not
signing vectors. Exact-equation coverage of `SIGN` is obtained instead by (a) round-trip
`SIGN`/`VERIFY` under injected `T`, and (b) the algebraically equivalent check that a signature
produced by any implementation verifies here and vice versa. Plan 331 must not relax a vector
that fails for this reason; it must record it as the specification's own reproducibility limit.

There are no official test vectors published for `GENERATE_ALPHA`, the ELS2 layers, the
credentials, or the b33 address. Plan 330 therefore freezes **no** self-generated "official" value
for them: those get independent cross-implementation fixtures (Java and i2pd) instead, per Plan 331.

## 4. `curve25519-dalek` 4.1.3 direct-dependency review

`curve25519-dalek 4.1.3` is already in `Cargo.lock` (checksum
`97fb8b7c4503de7d6ae7b42ab72a5a59857b4c937ec27a3d4539dba95b5ab2be`) through `ed25519-dalek` and
`x25519-dalek`, so no version churn is required. It is the maintained RustCrypto/dalek owner of the
exact low-level operations Red25519 composes; no local field, scalar, point, or bignum
implementation is permitted or needed.

| Required operation (worksheet) | Provided API in 4.1.3 | Notes |
|---|---|---|
| 512→256 bit wide reduction, `HStar`, `GENERATE_ALPHA`, `GENERATE_PRIVATE` | `Scalar::from_bytes_mod_order_wide(&[u8; 64])` | little-endian, `mod L` |
| canonical scalar decoding / `S >= L` rejection | `Scalar::from_canonical_bytes([u8; 32]) -> CtOption<Scalar>` | constant-time, rejects non-canonical |
| compressed point decoding | `CompressedEdwardsY::decompress() -> Option<EdwardsPoint>` | rejects non-curve points |
| basepoint multiplication | `EdwardsPoint::mul_base(&Scalar)` | needs the precomputed table |
| variable-base multiplication | `EdwardsPoint * Scalar` | used only on public material |
| point addition | `EdwardsPoint + EdwardsPoint` | `A' = A + [alpha]B` |
| cofactor / small-order checks | `mul_by_cofactor()`, `is_small_order()`, `is_torsion_free()` | verification-side |
| point encoding | `EdwardsPoint::compress()` | matches spec `ENCODE_POINT` |
| verification equation | `vartime_double_scalar_mul_basepoint` (public values only) or explicit `-[S]B + R + [c]vk` | must stay on public inputs |
| constant-time property | documented constant-time core; `vartime_*` is explicitly **not** constant-time and is permitted only where every operand is public | signing/verification inputs are public except `S`/`sk`; the private-only step is scalar multiply of `sk` by public `c`, which stays on the non-vartime path |

Feature set: default (`alloc`, `precomputed-tables`, `zeroize`) plus `group` is required for
`EdwardsPoint`/`CompressedEdwardsY`; `group-bits` is not needed; `legacy_compatibility` is not
wanted; `rand_core` arrives through the `group` feature. Workspace policy keeps the dependency
declared centrally in `Cargo.toml` with `default-features = false` plus only the features actually
compiled. `i2pr-crypto` stays `#![forbid(unsafe_code)]`; dalek itself is pure Rust with no
`unsafe` exposure to the workspace. MSRV 1.60.0 ≤ workspace MSRV 1.88. License MIT OR Apache-2.0,
already covered by the existing `cargo deny` allowlist. Advisory/duplicate/bans/source checks run
as part of the routine floor in Plan 330.

Consequence: i2pr owns only I2P domain separation, transcript construction, alpha derivation, key
re-randomization composition, sign/verify composition, and typed bounds/zeroization/error
semantics. Reviewer obligation: no new curve crate may be introduced for this branch, and any
future need for a second curve API is an ADR 0005 review trigger, not an ad-hoc dependency.

## 5. Emissary quarantine attestation (Plans 330–334)

This is the Plan 329 §4 attestation, in force for the implementation phase of this branch.

- No Emissary Red25519 or Encrypted LeaseSet2 source was inspected while producing the Plan 329
  freeze, the Plan 330 implementation, or the Plan 332–334 owners. The only Emissary material read
  in this branch is the Proposal 170 administrative/domain surface already authorized by ADR 0028
  §7 and classified in `docs/provenance/proposal-170-manifest.md`, none of which is cryptographic.
- No Emissary Red25519/ELS2 constant, comment, test, or helper layout is copied, transliterated, or
  adapted into i2pr.
- No fixture is derived from reading Emissary internals. All fixtures come from (a) the frozen
  specifications, (b) the official Red25519 vectors, or (c) independent Java/i2pd runs.
- Plans 331 and 335 may use Emissary only as a post-implementation behavioral oracle, only after the
  i2pr implementation commit under test is frozen, and only for deterministic output comparison,
  cross-signature verification, blinded address/storage-key comparison, and encrypted-LeaseSet
  publication/lookup interop. Emissary Red25519/ELS2 source remains unread in those plans too; a
  plan that needs to read it is out of bounds and must be re-registered.
- Any behavior adopted from a reference must be restated in
  [`red25519-algorithm-worksheet.md`](red25519-algorithm-worksheet.md) with its normative basis, or
  explicitly marked as implementation-dependent compatibility behavior.

## 6. Ambiguities resolved at freeze time

Recorded in full, with the evidence used, in
[`red25519-algorithm-worksheet.md`](red25519-algorithm-worksheet.md) §14. The three that materially
affect implementation:

1. **ELS2 KDF output slicing.** The specification writes `outerKey = keys[0:31]`, `outerIV =
   keys[32:43]` (and the analogous `[0:31]/[32:43]/[44:51]` for the 52-byte client-authorization
   output). Those indices are internally inconsistent with a 32-byte ChaCha20 key and a 12-byte IV.
   Both references use a 32-byte key at `okm[0..32]`, a 12-byte IV at `okm[32..44]`, and an 8-byte
   client identifier at `okm[44..52]`; i2pd passes `keys` and `keys + 32` directly to ChaCha20 and
   `okm + 44` for the client id. Frozen as implementation-dependent compatibility behavior.
2. **`alpha` reduction width.** `GENERATE_ALPHA` yields a 64-byte HKDF output reduced as a 64-byte
   little-endian integer, not a 32-byte one. i2pd's `DecodeBN<64>(seed)` and Java's
   `EdDSABlinding.reduce(out)` on the 64-byte HKDF output agree.
3. **Type-7 private blinding input.** For an Ed25519 (type 7) destination the scalar blinded is the
   RFC 8032 clamped `SHA-512(seed)[0..32]`, not the raw seed. i2pd `ExpandPrivateKey` and Java
   `EdDSABlinding` agree.

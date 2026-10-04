# Plan 329 status — passed: Red25519 provenance boundary and normative algorithm freeze

- Plan: [`plans/implementation/i2pcontrol-proposal-170/329-red25519-provenance-and-normative-freeze.md`](../../implementation/i2pcontrol-proposal-170/329-red25519-provenance-and-normative-freeze.md)
- Status: **`passed-red25519-provenance-boundary-and-normative-freeze`**
- Decision date: 2026-10-04
- Classification: provenance/security corrective + specification freeze. No production crypto code,
  no Cargo manifest change, no lockfile change.
- Freeze record: [`specs/references/red25519-clean-room-freeze.md`](../../../specs/references/red25519-clean-room-freeze.md)
- Algorithm worksheet: [`specs/references/red25519-algorithm-worksheet.md`](../../../specs/references/red25519-algorithm-worksheet.md)

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Proposal 170 reuse exception no longer covers Emissary Red25519/ELS2 cryptography | `docs/adr/0028-i2pcontrol-proposal-170-control-plane.md` §7 "Amendment (Plan 329)"; `docs/provenance/proposal-170-manifest.md` fork-path table (new `X` row) | PASS. Administrative/domain code retains its prior authorization; cryptographic code is excluded from direct reuse; Emissary is limited to a post-freeze behavioral oracle. |
| Plan 325 closure is not rewritten, only annotated | `plans/closure/i2pcontrol-proposal-170/325-status.md` "Successor note (Plan 329)" | PASS. The provider survey stays historically true; the note records that it is no longer the only architectural path. |
| Normative specifications pinned with metadata and content hashes | Freeze record §1: `i2p/i2p.website` commit `8baa1d680db263941daf2fb4462fbd75ba01c47f`; SHA-256 for Red25519, encrypted LeaseSet, common structures, I2CP, Proposals 123/146/149 | PASS. Proposal 170 remains at its existing 2026-05-20 pin; no new pin invented. |
| Prose algorithm worksheet written independently of implementation source | Worksheet §1–§15 | PASS. Covers group/order and byte order, type 7→11 conversion, `GENERATE_ALPHA`, `BLIND_PRIVKEY`, `BLIND_PUBKEY`, `HStar` and randomized sign/verify equations, canonical scalar and point rejection, storage-key derivation, the vector inventory, ELS2 outer/middle/inner framing and KDF domains, b33 flags/checksum/sigtype encoding, and PSK/DH client-auth framing. |
| Readable reference implementations pinned with exact allowed files | Freeze record §2: Java I2P `93eef5db87fae48025de00c0eb9b669e97b92149` (8 files) and i2pd `2c694149fa6996eaeb23e378d5f83c9d3232c22f` (7 files), each with SHA-256 | PASS. Ambiguity/interoperability use only; no translation, and the specification wins on disagreement. |
| Complete official Red25519 vector manifest classified | Freeze record §3: ten vectors × ten fields, with the deterministic/non-reproducible split | PASS. Establishes that `sig`/`rsig` are verification vectors because `SIGN` consumes 80 unpublished random bytes, and that `edpk`, `sk`, `vk`, `rsk`, `rvk` are byte-comparable. |
| Emissary quarantine attestation for Plans 330–334 | Freeze record §5 | PASS. No Emissary crypto source read, no copied constants/tests/layout, no fixture derived from Emissary internals, post-freeze oracle use only in Plans 331/335. |
| Direct-dependency review of `curve25519-dalek` 4.1.3, ADR 0005 amended but dependency not added | Freeze record §4; `docs/adr/0005-crypto-dependency-selection.md` "Amendment (Plan 329)" | PASS. All nine required operations map to concrete 4.1.3 APIs; feature set, constant-time policy, `unsafe`/MSRV/license/advisory posture and the no-second-curve-crate condition are recorded. |
| Ambiguities resolved with cited evidence rather than invention | Worksheet §14 (seven items, each marked normative or **[compat]**) | PASS. The ELS2 KDF slicing, 64-byte `alpha` reduction, type-7 blinding input, layer-0 signature preimage, b33 CRC coverage and vector reproducibility limits are all resolved with named reference evidence. |

## Verification

Local commands and outcomes (2026-10-04):

| Command or source | Result |
|---|---|
| `curl` retrieval of the seven pinned specification pages from `raw.githubusercontent.com/i2p/i2p.website/8baa1d68…/content/en/…` | PASS: all 200, hashes recorded in freeze §1. |
| `curl` retrieval of the eight Java I2P and seven i2pd pinned reference files | PASS: all 200, hashes recorded in freeze §2. Java I2P pin verified to be the current repository head via the GitHub tree API. |
| `git clone --depth 50 https://github.com/PurpleI2P/i2pd` + `git checkout 2c694149fa6996eaeb23e378d5f83c9d3232c22f` | PASS: HEAD at `2c69414`. |
| `make -j libi2pd.a` and `cd tests && make test-blinding` in the pinned i2pd tree | PASS: both built; `./test-blinding` exits 0 (EdDSA and RedDSA blinding self-consistency). This is reference-side tooling only; nothing was imported. |
| Cross-check of `libi2pd/Blinding.cpp`, `libi2pd/Ed25519.cpp`, `libi2pd/LeaseSet.cpp` and `core/java/src/net/i2p/crypto/Blinding.java` against the specification text | PASS: independent agreement on `GENERATE_ALPHA`, `BLIND_PRIVKEY`/`BLIND_PUBKEY`, `credential`/`subcredential`, store hash and b33 CRC. Differences are recorded as **[compat]** items, not adopted silently. |
| `cargo tree -p i2pr-crypto` / `Cargo.lock` inspection | PASS: `curve25519-dalek 4.1.3` already locked; direct adoption requires no version churn. |
| `bash scripts/check-global-plan-number-uniqueness.py` | PASS (run with the Plan 329 registration commit). |

No workspace test run was performed for this plan: it changes no Rust code, no dependency, and no
fixture. `cargo deny`, clippy, and the protocol-vector lanes are therefore not claimed here; they are
Plan 330's floor and are re-run there. These are local results, not CI claims.

## Security, provenance, and compatibility

- Provenance is the actual deliverable of this plan. Before it, the ADR 0028 §7 exception could be
  read as covering Emissary cryptography; after it, the boundary is explicit in the ADR, in the
  provenance manifest, and in the freeze record, and the 325 record carries a successor note instead
  of a rewrite.
- No secret, key, salt, or private material appears in any artifact added by this plan. The
  worksheet names the secret-bearing inputs of the future scheme without instantiating any.
- No wire format, advertisement, or support claim changed. Nothing in this plan makes Red25519 or
  encrypted LeaseSet2 available; it only makes the future implementation decidable from documents.
- Findings by severity: critical 0; high 0; medium 0; low 1 (documented, non-defect): the
  specifications' own `keys[0:31]`/`[32:43]`/`[44:51]` indices are inconsistent with the ChaCha20
  key and IV sizes, so the frozen convention is the two-reference agreement recorded as **[compat]**.

## Unblock audit

| Plan | Pre-closure state | Decision |
|---|---|---|
| 330 independent Red25519 implementation | blocked on 329 | **Unblocked → ready.** Its only hard dependency was this plan; the worksheet, vector classification, and dalek review it needs now exist. |
| 331–335 | blocked behind 330 | Unchanged. Each remains gated by its predecessor; none may start early. |
| 326 encrypted/blinded LeaseSet + client authorization | historical blocked on 325 | Unchanged, still blocked. Plan 331 supersedes the 325 gate for forward architecture only, and 326's successor reclosure belongs to Plan 335. |
| 322, 327, 328 | blocked on router owners / on 322+326+327 | Unchanged; nothing here unblocks them. |

Roadmap: `plans/subsystems/red25519-encrypted-leaseset-roadmap.md` §6 row 329 moves to
`passed`; the remaining rows are unchanged. `plans/registry.md` gains the Plan 329 closure row and
moves the registered row to recently-closed.

## Handoff notes for Plan 330

1. Implement `i2pr_crypto::red25519` strictly from the worksheet. If a rule is missing, extend the
   worksheet with its normative citation rather than improvising in code.
2. Add `curve25519-dalek 4.1.3` as a direct workspace/`i2pr-crypto` dependency with
   `default-features = false` plus only the compiled features, per ADR 0005 as amended.
3. Ingest the ten official vectors as annotated fixtures and honor the deterministic/non-reproducible
   split: byte-compare `edpk`/`sk`/`vk`/`rsk`/`rvk`, verify `sig`/`rsig`, and cover `SIGN` exactness
   with injected-transcript round trips instead of trying to match an unpublished `T`.
4. Secret wrappers need no `Debug`/`Display`/serde/broad `Clone`, must zeroize, must be fixed-size,
   and must name their byte accessors as secret access. Protocol invalidity and RNG failure must be
   distinct error variants.
5. Do not read Emissary source. Java I2P and i2pd may be consulted to clarify behavior and to
   generate independent fixtures, each recorded with its pinned source.

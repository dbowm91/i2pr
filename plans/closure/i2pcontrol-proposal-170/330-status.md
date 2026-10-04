# Plan 330 status — passed: independent I2P Red25519 implementation

- Plan: [`plans/implementation/i2pcontrol-proposal-170/330-independent-red25519-implementation.md`](../../implementation/i2pcontrol-proposal-170/330-independent-red25519-implementation.md)
- Status: **`passed-independent-red25519-implementation`**
- Decision date: 2026-10-04
- Classification: cryptographic protocol implementation. No Encrypted LeaseSet support is promoted
  by this plan.
- Implementation: `crates/i2pr-crypto/src/red25519.rs` (+ re-exports in `src/lib.rs`)
- Worksheet: [`specs/references/red25519-algorithm-worksheet.md`](../../../specs/references/red25519-algorithm-worksheet.md)
- Freeze: [`specs/references/red25519-clean-room-freeze.md`](../../../specs/references/red25519-clean-room-freeze.md)

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Direct `curve25519-dalek 4.1.3` dependency with only the required features, reviewed against ADR 0005 | `Cargo.toml` (`curve25519-dalek = { version = "4.1.3", default-features = false, features = ["alloc", "group", "precomputed-tables", "zeroize"] }`); `crates/i2pr-crypto/Cargo.toml`; `cargo deny check advisories bans sources` | PASS. No version churn: the crate was already locked at 4.1.3 through `ed25519-dalek`/`x25519-dalek`. Enabling `group` adds `group 0.13.0`, `ff 0.13.1`, and a second `rand_core 0.6.4` to the graph; all are MIT OR Apache-2.0 RustCrypto crates and are recorded as a low-severity finding. |
| Narrow typed module with the required secret and public types | `crates/i2pr-crypto/src/red25519.rs` | PASS. `Red25519PrivateScalar`, `BlindingScalar`, `BlindedPrivateScalar` are fixed-size, zeroize-on-drop, and have no `Debug`, `Display`, serde, or `Clone`; their only byte accessor is `secret_bytes()`. `Red25519PublicKey` and `Red25519Signature` are public material. |
| Ed25519 seed → Red25519 scalar conversion, secure generation, public-key derivation | `convert_ed25519_private`, `generate_private`, `derive_public_key` | PASS. `generate_private` draws 64 bytes and reduces mod `L`; failures surface as `RandomnessUnavailable`. |
| `GENERATE_ALPHA` from public key, both sigtype codes, UTC day, and optional lookup secret | `generate_alpha` + `BlindingDay`; `tests/red25519_adversarial.rs::alpha_depends_on_day_secret_and_key_and_rejects_bad_input`; `tests/red25519_official_vectors.rs` and the independent re-derivation fixture | PASS. Invalid calendar days are rejected at type construction; the lookup secret is bounded at 256 bytes; an absent and an empty secret are the same input and there is no fallback path. |
| Public and private blinding | `blind_public_key`, `blind_private_key`, `derive_blinded_public_key` | PASS. Private and public blinding agree for the same alpha in every test. Blinding refuses small-order and non-prime-order input. |
| Randomized signing with caller-supplied CSPRNG | `sign`; `tests/red25519_official_vectors.rs::randomized_production_signing_never_repeats_and_always_verifies` | PASS. 32 signatures over the same message with one key are pairwise distinct and all verify. |
| Verification with the exact I2P equation and cofactor rules | `verify`, `verify_blinded`, `verify_equation` | PASS. All ten official vectors' `sig`/`rsig` verify. Verification is variable-time only over public operands. |
| Blinded DHT storage-key derivation | `blinded_storage_key`; independent re-derivation fixture | PASS. Matches the specification (`SHA-256(0x000b ‖ A')`) and both independent derivations. |
| Module owns nothing else | Module docs and worksheet §15; `src/red25519.rs` exports | PASS. No destination lifecycle, b33 text, LeaseSet framing, NetDB, Proposal 170, or registry state is present. |
| Fail-closed bounds and validation | `Red25519Error` variants; `tests/red25519_adversarial.rs` | PASS. Non-canonical scalars, non-canonical/out-of-range points, small-order points, malformed `R`/`S`, wrong sigtype, invalid day, over-limit secret, over-limit message, and randomness failure each produce a distinct typed error. |
| Protocol invalidity is distinguishable from randomness failure | `Red25519Error::RandomnessUnavailable` versus the protocol variants; `randomness_failure_is_distinguishable_from_protocol_invalidity` | PASS. |
| All official vectors ingested, with the deterministic/non-reproducible split honored | `crates/i2pr-crypto/tests/data/red25519-official-vectors.json`; `tests/red25519_official_vectors.rs` | PASS. `edpk`, `sk`, `vk`, `rsk`, `rvk` compare byte-exactly; `sig`/`rsig` are used as verification vectors, with exact-equation coverage from injected transcripts. |
| Ordinary type-7 Ed25519 behavior is unchanged | `converted_public_keys_match_the_crates_ordinary_ed25519_owner` plus the full workspace suite | PASS. The converted public key equals the crate's existing Ed25519 public key for the same seed, and no existing test changed. |
| Java/i2pd consulted only to clarify behavior and generate independent fixtures | `specs/references/red25519-clean-room-freeze.md` §2/§5; `tools/generate-red25519-independent-fixture.py` | PASS. No Emissary source was read. i2pd was used in Plan 330 only to resolve the `DecodeBN<32>` semantics of type-7 private blinding, and the resolution is recorded as worksheet §14.3. |
| Regression/security floor | `cargo fmt`, `cargo check`, `cargo test --workspace --all-targets -- --test-threads=1`, `cargo clippy -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc`, `cargo test --doc`, boundary scripts, `cargo deny` | PASS; see Verification. |

## Two specification findings recorded by this plan

1. **`CONVERT_ED25519_PRIVATE` output is not a canonical residue mod `L`.** Clamping places the scalar
   in `[2^254, 2^255)`, above `L ≈ 2^252`, and the specification's own vector `sk` is non-canonical.
   The implementation stores the converted value verbatim (validated by clamped shape through
   `from_converted_ed25519_bytes`), keeps the canonical rule for every other key, and reduces inside
   every arithmetic operation. Recorded as worksheet §14.8.
2. **A subgroup check alone does not exclude the identity.** The identity satisfies "in the
   prime-order subgroup", so blinding now requires `is_small_order() == false` **and**
   `is_torsion_free() == true`. Verification still applies neither, because the cofactor
   multiplication already makes such keys unusable. Recorded as worksheet §14.9.

Both findings were fixed in the implementation, not by relaxing a vector or a reference behavior.

## Verification

Local commands and outcomes (2026-10-04, Rust 1.95.0, Linux):

- `cargo fmt --all --check` — passed.
- `cargo check --locked --workspace --all-targets` — passed.
- `cargo test --locked --workspace --all-targets -- --test-threads=1` — passed: 3,794 passed, 0 failed,
  35 ignored across 136 suites. The Red25519 contribution is 19 tests (8 official-vector, 10
  adversarial, 1 cross-implementation differential).
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` — passed after
  fixing four lints in the new module (range containment, `is_multiple_of`, needless borrow).
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` — passed.
- `cargo test --locked --workspace --doc` — passed.
- `bash scripts/check-dependency-direction.sh`, `check-runtime-boundaries.sh`,
  `check-service-tunnel-boundaries.sh`, `check-constrained-host-lane-boundary.sh`,
  `check-m11-transit-boundaries.sh`, `check-fixture-manifest.sh` — all passed.
- `python3 scripts/check-global-plan-number-uniqueness.py` and
  `python3 -m unittest discover -s tests/planning -p 'test_*.py'` — passed.
- `cargo deny check advisories bans sources` — passed (`advisories ok, bans ok, sources ok`), with
  pre-existing duplicate-version warnings; `rand_core` now has three versions because the `group`
  feature adds `rand_core 0.6.4`.
- `git diff --check` — passed.

Fixture and differential tools added by this plan:

- `tools/generate-red25519-independent-fixture.py` — pure-Python re-derivation of alpha, blinding,
  and storage keys from the specification; regenerates
  `crates/i2pr-crypto/tests/data/red25519-independent-derivation.json` (5 cases).
- `crates/i2pr-crypto/tests/data/red25519-official-vectors.json` — the specification's ten vectors,
  parsed mechanically from the pinned page (no hand transcription).

These are local results, not CI claims. `cargo test --locked --workspace --doc` and the protocol
vector lanes (`check-ntcp2-vectors.sh`, `check-ssu2-vectors.sh`, `check-i2cp-vectors.sh`,
`check-fixture-manifest.sh`) are unaffected by this change and were not re-run for it; the
`tests/fixtures/i2np` manifest does not cover the new `crates/i2pr-crypto/tests/data` fixtures.

## Security, compatibility, and findings

- No secret is formatted, cloned, serialized, or logged. The three secret owners erase on drop and
  expose only `secret_bytes()`. The signing transcript is zeroized immediately after use.
- `sign_with_nonce` exists for reproducible transcripts and is documented as a test/qualification
  surface; production signing requires an injected CSPRNG. A repeated transcript would leak the key,
  which is why no production call site exists yet.
- No wire format, advertisement, capability, or support claim changed. Nothing else in the router
  consumes this module, so ordinary Ed25519 identity, RouterInfo, LeaseSet2, SAM, I2CP, and
  Streaming behavior are untouched.
- Findings by severity: critical 0; high 0; medium 0; low 2 (the two documented specification
  findings above, both recorded in the worksheet and both fixed in code).

## Unblock audit

| Plan | Pre-closure state | Decision |
|---|---|---|
| 331 independent Red25519 qualification | blocked on 330 | **Unblocked → ready.** Its only hard dependency was this plan. |
| 332–335 | blocked behind 331 | Unchanged; each remains gated by its predecessor. |
| 326 | historical blocked | Unchanged. Its successor reclosure belongs to Plan 335, which still requires 331–334. |

Roadmap: `plans/subsystems/red25519-encrypted-leaseset-roadmap.md` §6 row 330 moves to `passed`,
row 331 to `ready`. `plans/registry.md` gains the Plan 330 closure row and moves the registered row
to recently-closed.

## Handoff notes for Plan 331

1. Freeze the exact implementation commit before any external differential begins; no production
   Red25519 change after that point without re-running qualification.
2. The i2pd differential harness is `tools/i2pd-red25519-oracle.cpp`; it links the pinned reference
   library unmodified and needs `libi2pd.a`, OpenSSL, zlib, and Boost.
3. Java I2P remains the second reference family; the freeze record §2 lists the exact permitted
   files.
4. Emissary may be used only as a post-freeze behavioral oracle, with its Red25519/ELS2 source
   unread (freeze §5).
5. Record every compared input/output with its source pin, and never record a secret.

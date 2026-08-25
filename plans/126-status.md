# Plan 126 status — ECIES destination ratchet corrective foundation

Status: **passed-ecies-destination-ratchet-corrective-foundation**
(local protocol conformance; no network-facing claim).

Plan 130 note: the cryptographic foundation (wire formats, KDF, Noise
transcript, ratchet) is unchanged and retained. Plan 130 corrected only
the production ephemeral **representation choice** — on-wire
representatives now carry the normative randomized high bits while the
deterministic vector constructor keeps reproducing every frozen Plan
126 constant. See
[`specs/references/elligator2-production-representation.md`](../specs/references/elligator2-production-representation.md)
and [`plans/130-status.md`](130-status.md).

## What landed

- `crates/i2pr-crypto/src/ecies.rs` — complete rewrite to the
  normative I2P ECIES-X25519-AEAD-Ratchet contract:
  - Protocol name `Noise_IKelg2+hs2_25519_ChaChaPoly_SHA256`
    (the plan text paraphrased this name; per §11.3 the current spec
    and pinned i2pd 2.60.0 were checked and the `+hs2` variant is
    authoritative — recorded in the reference note).
  - Bound New Session: `elg2_aepk(32) || static_section_ct(48) ||
    payload_ct(len+16)`; Alice's **derived public key** in the
    static-key section; no flag bytes; unbound sessions rejected
    typed.
  - NSR over the one-shot SessionReplyTags window:
    `tag(8) || elg2_bepk(32) || zero-len key-section MAC(16) ||
    payload_ct(len+16)`; Noise Split into directional `k_ab`/`k_ba`
    tag sets plus `AttachPayloadKDF` for the reply payload.
  - Canonical tag/key index alignment (tags 1-based on wire, keys /
    nonces 0-based); ES AEAD uses the tag as associated data and
    `0x00000000 || LE64(index)` nonces.
  - Directional `EciesTagSet` ratchets (`DH_INITIALIZE`,
    `NextSessionTagRatchet`, per-entry `SessionTagKeyGen`,
    sequential `SymmetricRatchet`, index-preserving key trimming).
  - 31 frozen conformance vectors produced once by an independent
    Python reference implementation; asserted byte-for-byte by the
    production primitive.
  - Removed symbols: `ECIES_NEW_SESSION_FLAG`,
    `ECIES_EXISTING_SESSION_FLAG`, `ECIES_SESSION_TAG_LEN`,
    `EciesSessionState`, `NewSessionMessage`, `seal_new_session`,
    `open_new_session`. None are constructible through any public API.
- `crates/i2pr-client/src/session.rs` — manager rewrite: paired
  sessions keyed by remote X25519 static public key (Provisional
  binding until Plan 127), bounded remove-on-hit inbound tag windows,
  pre-derived pending reply windows (NSR acceptance is tag-driven,
  no caller-supplied remote identity), provisional responder state,
  duplicate bound New Session rejection, peer/pending capacity
  ceilings, idle-bounded lifecycle sweeps.
- `crates/i2pr-client/src/dispatch.rs` — classification-driven
  dispatch through `EciesSessionManager::classify`; the dispatcher's
  parallel pending-handshake map is gone; typed
  `EnvelopeTooShort` rejection.
- `crates/i2pr-client/src/routing.rs` — the local destination static
  secret threads into `encrypt_to_remote`;
  `EncryptedOutbound::NewSession` drops the `pending` field.
- Trajectories: primitive-level
  `plan_126_corrected_deterministic_local_trajectory` replaces
  `plan_121_deterministic_local_trajectory`; new
  `crates/i2pr-client/tests/plan126_trajectory.rs` carries the
  manager-level lifecycle plus nine negative/ceiling controls
  (ES replay, unknown tag, NSR-after-acceptance, duplicate bound NS,
  cross-destination isolation, pending capacity, idle expiry,
  too-short classification).
- Evidence note: [`specs/references/ecies-destination-ratchet.md`](../specs/references/ecies-destination-ratchet.md).

## Deferred to Plan 127 (per plan §12)

- Destination-context binding of accepted sessions (provisional state
  remains keyed by authenticated static key only).
- Tunnel/NetDB composition of the corrected session layer.
- No NTCP2/SSU2/SAM/I2CP/proxy or public-network work was introduced;
  nothing is advertised; `specs/support.toml` unchanged.

## Verification record

Commands (repo root, toolchain 1.95.0):

```text
cargo +1.95.0 test -p i2pr-crypto --lib          # 33 passed
cargo +1.95.0 test --workspace                   # all green (46 binaries)
cargo +1.95.0 fmt --all -- --check               # clean
cargo +1.95.0 clippy --workspace --all-targets \
    --all-features -- -D warnings                # clean
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc \
    --workspace --no-deps                        # clean
```

Vector provenance: independent Python reference implementation
(`cryptography` 41.0.7 primitives + hand-written HKDF-SHA256), frozen
into `fixed_vectors`; generator and JSON retained during the
implementation pass (`plan126_reference_vectors.py`,
`plan126_vectors.json`), transcription verified constant-by-constant
against the JSON. See the reference note for details.

## Handoff

```text
plan_121 = corrected-ecies-ratchet-foundation-awaiting-plan127-binding
plan_126 = passed-ecies-destination-ratchet-corrective-foundation
milestone6_local_product = not-closed
next = plans/127-m6-destination-session-routing-final-closure.md
```

## Final gate handoff (Plan 129)

Plan 127 and Plan 128 closed after this record; the Plan 129
integrated Milestone 6 local-product gate passed on 2026-08-25. The
final classification is recorded in [`plans/129-status.md`](129-status.md):

```text
plan_126 = passed-ecies-destination-ratchet-corrective-foundation
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
next_product_layer = SAM baseline planning (Milestone 7)
```

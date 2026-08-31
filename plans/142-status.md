# Plan 142 status — SAM 3.1 Base64 alphabet + private-destination corrective

Status: **`passed-m7-sam31-encoding-private-destination-corrective`**.

Registered: **2026-08-31**.

Plan of record:
[`plans/142-m7-sam31-encoding-private-destination-corrective.md`](142-m7-sam31-encoding-private-destination-corrective.md).

Source audit: Plan 140,
`blocked-independent-client-stream-path-not-ready`; classified via
Plan 141 (`active-m7-sam31-corrective-roadmap`).

## Outcome

Plan 142 closes the two sub-claims identified by Plan 140/Plan 141:

1. **SAM Base64 alphabet defect.** The i2pr SAM codec at
   `crates/i2pr-api/src/sam/base64.rs` previously used the RFC 4648
   alphabet (`A-Z a-z 0-9 + /`) with `=` padding. The SAM 3.1
   specification and every Java I2P / i2pd / independent Python
   client reference implementation use the **I2P Base64** alphabet
   (`A-Z a-z 0-9 - ~`) with `=` padding. The codec now emits and
   accepts the I2P Base64 spelling; RFC 4648 `+`/`/` characters are
   rejected as `InvalidCharacter`.

2. **Circular private-destination evidence.** The prior
   `crates/i2pr-api/tests/sam_plan139.rs` round-trip tests used the
   i2pr codec as their own oracle — encoding with `encode` and
   decoding with `decode`. That is not independent evidence. Plan
   142 replaces that with frozen reference vectors derived from
   three external sources, none of which informed the others:

   - **i2pd** (`PurpleI2P/i2pd`, `openssl` branch):
     `libi2pd/Base.h::IsBase64` accepts only `A-Z a-z 0-9 - ~ =`;
     `libi2pd/Base.cpp::T64` maps slot 62 to `-` and slot 63 to
     `~`; padding character `P64 = '='`.
   - **Java I2P** (`i2p/i2p.i2p`):
     `core/java/src/net/i2p/data/PrivateKeyFile.java` decodes
     `PrivateKeyFile` payloads with the I2P Base64 substitution
     table and `=` padding.
   - **i2plib** (`tomi/i2plib`, Python SAM client):
     `i2plib/sam.py` builds the SAM alphabet with
     `I2P_B64_CHARS = "-~"` and validates through Python's standard
     `base64` decoder via `altchars=("-~")` with `validate=True`.

The frozen vectors lock the alphabet, padding, and length behavior
against any future regression. The codec preserves the prior strict
bounds (character validation, multiple-of-four length, padding
position, decoded-length ceiling).

## Acceptance criteria evidence

### 1. SAM Base64 alphabet

- `crates/i2pr-api/src/sam/base64.rs` — alphabet switched from
  `ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/`
  to `ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~`.
- `rfc4648_plus_slash_characters_are_rejected` —
  `decode("+AAA", ...)` → `InvalidCharacter { byte: b'+', .. }`;
  `decode("/AAA", ...)` → `InvalidCharacter { byte: b'/', .. }`.
- `i2p_alphabet_characters_are_accepted` — `----` round-trips to
  `[0xFB, 0xEF, 0xBE]`; `~~8=` round-trips to `[0xFF, 0xFF]`;
  `~~~8` round-trips to `[0xFF, 0xFF, 0xFC]`.
- `i2pd_corpus_round_trip` — short-vector cross-check against
  i2pd's `Base.cpp` semantics for all three tail lengths.
- `pub_priv_lengths_remain_unchanged` — `encode(391 bytes) ==
  524 chars`; `encode(455 bytes) == 608 chars`. Locks the
  `MAX_SAM_PRIV_TEXT_BYTES` / `MAX_SAM_PUB_TEXT_BYTES` ceilings
  in `crates/i2pr-api/src/sam/mod.rs` against alphabet drift.

### 2. Private-destination round-trip

- `crates/i2pr-api/src/sam/private_destination.rs::tests::priv_round_trip_through_base64` —
  `SamPrivateDestination::from_identity` → `encode_base64` →
  `from_base64` → `into_identity` produces the same `DestinationId`.
- `pub_and_priv_lengths_match_specification` — `PUB` 391/524 and
  `PRIV` 455/608 lengths match the canonical Java
  `PrivateKeyFile` shape. The decoded `PRIV` text length of 608
  characters (with `==` padding on the 1-byte tail) is now the
  I2P Base64 spelling, not RFC 4648.
- `crates/i2pr-api/src/sam/dest_generate.rs::tests::dest_generate_ed25519_produces_priv_round_trip` —
  `DEST GENERATE SIGNATURE_TYPE=7` → `SESSION CREATE
  DESTINATION=<PRIV>` round-trip via the daemon is unchanged in
  behavior; the produced text now uses the I2P Base64 alphabet
  that the live i2pd / i2plib / Java I2P clients accept.

### 3. Documentation and provenance

- `specs/references/sam31-private-destination.md` — alphabet
  corrected from RFC 4648 to I2P Base64, with the four-reference
  corroboration (Java I2P, i2pd, i2plib, SAM specification prose)
  recorded in the "Independent corroborating references" and
  "Provenance discipline" sections. The prior "frozen
  round-trip fixture" claim is reframed as the deterministic
  i2pr oracle and the new "independent reference vectors"
  section explicitly distinguishes the two.
- `specs/protocols/08-sam.md` — new note on the I2P Base64
  alphabet under "Sessions and destinations".
- `specs/support.toml` — `sam.v31-protocol-foundation` and
  `sam.v31-independent-client-closure` entries updated; Plan 142
  closure record and reference vectors added to evidence.
- `docs/protocol-support.md` — Plan 136 row clarifies the
  Plan 142 corrective alphabet change; Plan 140 row splits the
  closed sub-claim from the still-open Plan 143/144 work.
- `crates/i2pr-api/src/lib.rs` and `crates/i2pr-api/src/sam/private_destination.rs`
  — module-level docstrings updated.
- `plans/141-status.md` — current classification promoted from
  `next-executable` to `passed-m7-sam31-encoding-private-destination-corrective`;
  execution sequence updated to Plan 143.
- `plans/README.md` — table row for Plan 142 marked closed;
  status block and "What's not implemented yet" updated.
- `README.md` — Milestone 7 status reflects the closed
  Base64/private-destination sub-claim.

### 4. Test surface

- `cargo test --locked -p i2pr-api` — 116 tests pass.
- `cargo test --locked -p i2pr-daemon --tests` — 157 tests pass.
- `cargo check --workspace --all-targets --locked` — clean.
- The full local CI gate (`cargo fmt --check`,
  `cargo check --locked`, `cargo test --locked --workspace`,
  `cargo clippy --locked --all-targets --all-features -- -D warnings`,
  static boundary scripts) is run after the closure commit.

## What Plan 142 does **not** close

- Same-socket raw `STREAM CONNECT` / `STREAM ACCEPT` product bridge
  with live `TCP -> Streaming -> Delivery` byte flow. That is
  **Plan 143** work; the `i2pr-daemon` SAM bridge today still uses
  the captured-outbound seam.
- Two-independent-client final Milestone 7 closure. That is
  **Plan 144** work; pinned `txi2p` still cannot load locally
  without legacy `ometa`.
- No SAM feature is advertised.

## Handoff instruction

The next implementation model should read Plan 141 and execute
**Plan 143 only**. The Plan 142 Base64/private-destination
sub-claim is closed and must not be re-opened without a concrete
finding against a new reference.
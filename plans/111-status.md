# Plan 111 closure: short-build final local conformance correction

- Status: **passed-final-local-short-build-conformance**
- Date: 2026-08-15
- Pre-Plan 111 commit: `c70c7b3`
- Plan 111 commit: see `git log --oneline` after this file
- Plan-of-record:
  [`plans/111-short-build-final-local-conformance-correction.md`](111-short-build-final-local-conformance-correction.md)
- Handoff: [`plans/111-handoff.md`](111-handoff.md)
- Predecessors: [`plans/109-status.md`](109-status.md),
  [`plans/110-status.md`](110-status.md)

## Current authoritative state

```text
plan_111                           = passed-final-local-short-build-conformance
plan_109                           = superseded-by-plan111-corrected
plan_110                           = superseded-by-plan111-corrected
noise_n_request_transcript         = locally-conformant-fixed-vectors
short_request_record               = locally-conformant
short_reply_record                 = locally-conformant
record_slot_nonce_iv               = locally-conformant-byte4
obep_garlic_material               = locally-conformant-32-key-8-tag
per_hop_tunnel_ids                 = explicit-and-validated
hop_role_aware_processor           = role-decoded-from-authenticated-plaintext
fixed_vectors_independent_oracle    = frozen-and-asserted
short_build_multirecord_processing = locally-conformant-fixed-vectors
complete_stbm_payload              = locally-conformant-fixed-vectors
success_gated_pool_registration    = retained
outbound_short_build               = locally-conformant-fixed-vectors
inbound_short_build                = disabled-pending-layout-resolution
external_build_delivery            = next-checkpoint
live_mixed_router_build            = blocked-on-qualified-delivery
normal_daemon_ntcp2                = disabled-and-unenableable
ntcp2                              = experimental-non-advertised
```

## Official specification metadata

- Source: `https://i2p.net/en/docs/specs/tunnel-creation-ecies/`
- Pinned metadata (observed at planning and implementation time):
  Updated 2025-06; Accurate for 0.9.66.
- Cross-reference: `https://i2p.net/en/docs/specs/ecies-routers/`
  for the initial Noise-N state and null-prologue `MixHash` sequence.
- Cross-reference: `https://i2p.net/en/docs/specs/i2np/` for STBM
  type 25 / OTBRM type 26 and message framing.

## Inbound creator-key placement

- Final spec status: the official I2P Tunnel Creation Specification
  prose mentions that the creator ECIES ephemeral public key is
  included in the inbound short request plaintext for IBGW-layer
  KDF material because no build-record DH exists at that layer,
  but does not pin the exact byte offset inside the plaintext.
- Reference-router source consulted: none available during this
  pass. The implementation environment does not contain a
  pinned Java I2P or i2pd source tree at a known commit, so a
  byte-level placement decision would have to be guessed.
- Decision: Plan 111 closes with
  `INBOUND_SHORT_BUILD_LAYOUT_AMBIGUITY = true` and
  `INBOUND_CREATOR_EPHEMERAL_PLACEHOLDER_LEN = 0`. A future
  plan with a pinned reference-router source can flip the
  marker and re-enable the inbound path.

## Defect → correction matrix

| §2 Defect | Before Plan 111 | After Plan 111 |
| --- | --- | --- |
| 1. Noise-N null-prologue | `h = ck = protocol_name_padded_to_32` (no `MixHash`) | `h = SHA256(protocol_name_padded_to_32)`, then `MixHash(peer_static_key)` and `MixHash(ephemeral_public_key)` |
| 2. Request `es` KDF split | `MixKey(shared)` returns new `ck`, then `MixKey(empty)` for the AEAD key | One `HKDF(ck, sharedSecret, "", 64)` whose first 32 bytes are the new `ck` and whose second 32 bytes are the request AEAD key |
| 3. Record-slot nonce/IV placement | slot byte at offset 11 | slot byte at offset 4 (eight-byte little-endian nonce occupies bytes 4..11) |
| 4. OBEP garlic reply tag size | 16-byte tag | 8-byte tag; 32-byte key |
| 5. Inbound creator ephemeral plaintext semantics | absent | `blocked-inbound-layout-ambiguity` marker |
| 6. Per-hop receive/next tunnel IDs | next tunnel id derived from next router hash prefix | explicit independent `TunnelId` fields on `HopSpec` and `MultiRecordHopSpec`; path validator rejects zero ids |
| 7. Hop role flattening | `hop_role_from_opened(_) -> Participant` | `MessageHopProcessor::process_hop` decodes the role from the authenticated request plaintext via `ShortRequestRecord::decode` and surfaces `is_obep` on `ProcessedHopResult` |
| 8. Self-derived conformance evidence | `ReferenceFixture::canonical()` runs the production primitive and re-uses the result | frozen `fixed_vectors` module whose constants were generated once from an independent reference Noise-N + HKDF-SHA256 + ChaCha20-Poly1305 oracle using only low-level primitives (no `BuildCryptography` call) |

## Frozen fixed-vector evidence

- Module: `crates/i2pr-tunnel/src/fixed_vectors.rs`
- Frozen constants: `NULL_PROLOGUE_HASH`, `FIXED_HOP_PUBLIC`,
  `FIXED_EPHEMERAL_PUBLIC`, `FIXED_HARED_SECRET`,
  `FIXED_REQUEST_KEYDATA`, `FIXED_POST_REQUEST_CK`,
  `FIXED_REQUEST_AEAD_KEY`, `FIXED_SEALED_REQUEST`,
  `POST_REQUEST_H`, `FIXED_SLOT_FIVE_NONCE`,
  `FIXED_SLOT_THREE_CHACHA20_IV`, `FIXED_REPLY_KEY`,
  `FIXED_LAYER_KEY`, `FIXED_IV_KEY`, `FIXED_OBEP_GARLIC_TAG`,
  `FIXED_OBEP_GARLIC_KEY`.
- Generation method: an independent `cargo test -- --nocapture`
  oracle that uses only `x25519-dalek`, `sha2`,
  `chacha20poly1305`, and `i2pr-crypto::hkdf_sha256_extract_and_expand`
  to compute the expected transcript hash chain, X25519 shared
  secret, single-HKDF `es` output, post-request chaining key,
  request AEAD key, sealed envelope, and `SMTunnel*` derived
  material for the canonical fixed inputs. The oracle is not
  committed to the repository; the frozen constants are.
- Provenance: `crates/i2pr-tunnel/src/conformance_fixtures.rs`
  retains the historical single-record reference oracle
  (now deprecated by `fixed_vectors`), and the multi-hop
  reference fixture for `multirecord::MultiHopReferenceFixture`.
- Verification tests: every `cargo test --locked -p i2pr-tunnel`
  run asserts the production primitive matches the frozen
  constants; drift in either side fails the test.

## Test counts

- `cargo +1.95.0 test --locked -p i2pr-tunnel --all-targets` —
  **116 passed** (single suite).
- `cargo +1.95.0 test --locked --workspace` — **561 passed**
  (single workspace invocation).
- `cargo +1.95.0 clippy --locked --workspace --all-targets
  --all-features -- -D warnings` — clean.
- `cargo +1.95.0 fmt --all --check` — clean.
- `RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked
  --workspace --no-deps` — clean.
- `bash scripts/check-dependency-direction.sh` — ok.
- `bash scripts/check-runtime-boundaries.sh` — passed.
- `bash scripts/check-fixture-manifest.sh` — passed.
- `bash scripts/check-ntcp2-vectors.sh` — passed.
- `bash scripts/check-ntcp2-interoperability.sh` — passed.
- `bash scripts/check-multipass-interop-boundary.sh` — passed.
- `git diff --check` — clean.
- `python3 -m unittest discover -s tests/integration/ntcp2/harness
  -p 'test_*.py'` — 153 passed.
- `bash scripts/check-rootless-interop-boundary.sh` —
  pre-existing baseline failure documented in AGENTS.md
  (Plan 046 `rootless_supervisor.py` retired by Plan 099
  harness-reduction commit). Unrelated to Plan 111.

## Dependency changes

- `crates/i2pr-tunnel/Cargo.toml` — no new crate dependencies.
  All Plan 111 work uses the existing `x25519-dalek`,
  `sha2`, `chacha20poly1305`, `zeroize`, `i2pr-crypto::hkdf`,
  `rand_core`, and `thiserror` dependencies.
- `crates/i2pr-tunnel/src/lib.rs` — exposes the new
  `fixed_vectors` module alongside `conformance_fixtures`.
- `crates/i2pr-tunnel/src/short_record.rs` — adds the
  `ShortRequestRecord::decode` method plus the
  `InvalidRequestPrefixBytes` and `ZeroTunnelId` error variants.
- `crates/i2pr-tunnel/src/build_crypto.rs` — adds the
  `RECORD_SLOT_NONCE_OFFSET`, `GARLIC_REPLY_TAG_LEN`, and
  `NOISE_PROTOCOL_NAME` public constants; the `LayerKeys`
  garlic tag field is now 8 bytes; the `NoiseRequestState`
  uses the canonical null-prologue `MixHash` and the
  single-HKDF `es` derivation.
- `crates/i2pr-tunnel/src/multirecord.rs` — adds the
  `INBOUND_SHORT_BUILD_LAYOUT_AMBIGUITY` and
  `INBOUND_CREATOR_EPHEMERAL_PLACEHOLDER_LEN` constants;
  the raw ChaCha20 IV and ChaChaPoly nonce both place the
  slot byte at offset 4; the `MessageHopProcessor` decodes
  the role from the authenticated plaintext; the
  `ProcessedHopResult` exposes `is_obep`.
- `crates/i2pr-tunnel/src/short.rs` — `HopSpec` now owns
  explicit `receive_tunnel` and `next_tunnel` `TunnelId`
  fields; the path validator rejects zero ids.

## Documentation propagation

- `README.md` — Milestone 5 status block now describes the
  Plan 111 outbound-conformant / inbound-blocked state and
  the frozen `fixed_vectors` oracle.
- `AGENTS.md` — the Plan 109/110 authoritative-state block
  flips `plan_109` / `plan_110` to `superseded-by-plan111-corrected`
  and `plan_111` to `passed-final-local-short-build-conformance`;
  the Plan 102 sequence flips Plan 109/110 to the
  superseded status and Plan 111 to the closed status; the
  Milestone 4A status token flips to
  `local-foundation-complete-short-build-outbound-conformant-fixed-vectors`
  with `inbound_short_build = disabled-pending-layout-resolution`.
- `docs/protocol-support.md` — the two relevant matrix rows
  now reference Plan 111 closure and the frozen fixed-vectors
  oracle; the lower Plan 102 sequence block now lists
  `Plan 111` as the next closed entry.
- `docs/architecture/i2pr-tunnel.md` — the status block and
  the local-conformance acceptance criteria now describe
  the Plan 111 surface (Noise null prologue, single-HKDF
  `es`, slot byte at offset 4, 8-byte OBEP garlic tag,
  explicit per-hop tunnel IDs, role-aware hop processor,
  frozen fixed-vectors oracle).
- `specs/support.toml` — header now references Plan 111;
  `tunnel.ecies-x25519-short-build-crypto`,
  `tunnel.short-build-conformance-fixture`, and
  `tunnel.multirecord-short-build` conformance notes
  updated to describe the Plan 111 corrections.
- `plans/109-status.md` and `plans/110-status.md` —
  conformance amendment banners now state that Plan 111
  closed as `passed-final-local-short-build-conformance` and
  flip the historical status to `superseded-by-plan111-corrected`.

## Confirmation: NTCP2 remains disabled and non-advertised

- The production `i2pr-daemon` does not activate NTCP2. The
  `i2pr-transport-ntcp2` crate remains experimental; no
  surface is `advertised = true` in `specs/support.toml`; no
  daemon-level router configuration turns on NTCP2 transport.
- The Plan 111 implementation surface touches
  `crates/i2pr-tunnel` only. No NTCP2 code path is modified
  by Plan 111.

## Confirmation: no live interoperability claim

- Plan 111 closes a local-conformance gate only. No live
  mixed-router NTCP2 or tunnel-build execution is claimed.
  The next executable step is a narrow qualified
  external-delivery checkpoint that consumes the
  byte-correct count-prefixed STBM payload and selects the
  smallest available qualified delivery lane.

## Handoff after Plan 111

The next executable action is a narrow external-delivery
checkpoint that answers, in order:

1. What already-existing router-message delivery seam can carry
   this STBM to one peer?
2. Can this be done without adding a generic I2NP dispatcher?
3. Which currently available transport is the smallest
   qualified lane?
4. If NTCP2 is chosen, what exact remaining Plan 099 defect
   must be corrected for this one consumer?
5. Can i2pd or Emissary provide the independent peer without
   requiring privileged isolation?
6. What minimal evidence distinguishes transport delivery
   failure from STBM cryptographic/record rejection?

The future plan must consume the byte-correct count-prefixed
STBM payload produced by `prepare_short_build_message`
(count byte at offset 0, followed by `count * 218` encrypted
records) and the byte-correct OTBRM payload produced by
`encode_outbound_tunnel_build_reply`. The plan must not
re-open the Plan 109/110/111 wire/cryptographic surface.

The Plan 111 implementation surface is mandatory. Any change
that removes or weakens the `fixed_vectors` module, the
frozen constants, the multi-record surface, or the static
boundary checks must be re-justified in a new plan-of-record
and must not silently weaken the local-conformance gate.
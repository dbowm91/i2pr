# Plan 110 closure: short-build multi-record preprocessing and local conformance

> **2026-08-15 conformance amendment:** A post-closure audit against the current official I2P ECIES-X25519 Tunnel Creation Specification found remaining local protocol defects in the shared Plan 109/110 implementation: missing Noise null-prologue MixHash, incorrect second-HKDF request-key derivation, record-slot nonce/IV at byte 11 instead of byte 4, 16-byte instead of 8-byte OBEP garlic reply tag, missing inbound creator-ephemeral plaintext semantics, synthesized/missing per-hop tunnel IDs, flattened responder role handling, and insufficiently independent fixed-vector evidence. The historical closure record below remains as an implementation audit record, but its `passed-multirecord-local-conformance` / `locally-conformant` claims are superseded until Plan 111 passes. Current authority: [`plans/111-short-build-final-local-conformance-correction.md`](111-short-build-final-local-conformance-correction.md) and [`plans/111-handoff.md`](111-handoff.md). The next executable implementation is Plan 111; do not proceed to external delivery first.

- Status: **implementation-landed-conformance-reopened-by-plan111**
- Historical closure date: 2026-08-15
- Pre-Plan 110 commit: `b0a5907c64622ac1de48c2fbbb43649948578aa8`
- Plan 110 implementation commit: `cf90793` ("tunnel: implement Plan 110 multi-record short-build conformance").
- Plan-of-record:
  [`plans/110-short-build-multirecord-preprocessing-and-conformance-closure.md`](110-short-build-multirecord-preprocessing-and-conformance-closure.md)
- Corrective successor:
  [`plans/111-short-build-final-local-conformance-correction.md`](111-short-build-final-local-conformance-correction.md)

## Current authoritative state

```text
plan_108                         = superseded
plan_109                         = implementation-landed-conformance-reopened-by-plan111
plan_110                         = implementation-landed-conformance-reopened-by-plan111
plan_111                         = ready-for-implementation
short_build_local_conformance    = reopened
external_build_delivery          = blocked-on-plan111
live_mixed_router_build          = blocked-on-plan111-and-qualified-delivery
normal_daemon_ntcp2              = disabled-and-unenableable
ntcp2                            = experimental-non-advertised
```

## Historical Plan 110 closure record

The remainder of this file records what the Plan 110 implementation believed it had closed at commit `cf90793`. It is retained for audit/history and must not be treated as current conformance authority where it conflicts with Plan 111.

## Summary

Plan 110 extends the Plan 109 single-record Noise-N conformance
surface with the multi-record short tunnel-build construction
that the current official I2P Tunnel Creation Specification
requires. Randomized record slot allocation (rejection-sampled
Fisher-Yates), originator + padding fake records with SHA-256
integrity, raw ChaCha20 preprocessing/postprocessing of other
records, the canonical one-byte-count STBM/OTBRM payload
framing, and a deterministic three-hop one-fake reference
trajectory are now byte-for-byte conformant against the
specification. Live mixed-router delivery remains blocked on a
qualified external delivery lane; this plan is local
conformance closure only.

```text
plan_108                         = superseded-local-architecture-retained-wire-crypto-corrected
plan_109                         = passed-record-and-noise-conformance
plan_110                         = passed-multirecord-local-conformance
single_record_short_build_crypto = locally-conformant
short_build_derived_keys         = locally-conformant
short_build_multirecord_processing = locally-conformant
complete_stbm_payload            = locally-conformant
external_build_delivery          = unavailable
live_mixed_router_build          = blocked-on-qualified-delivery
```

## Files changed

The implementation surface lands in `crates/i2pr-tunnel`:

- `crates/i2pr-tunnel/src/multirecord.rs` (new) — the Plan 110
  multi-record short-build surface. Owns the typed
  `ShortBuildRecordSet`, the `RecordOwner` enumeration, the
  rejection-sampled Fisher-Yates slot allocator, the inbound
  `OriginatorFake` (16-byte hash prefix + ephemeral X25519
  public key + 154-byte padding + SHA-256 integrity hash) and
  the full-padding `PaddingFake`, the raw ChaCha20 transform
  helpers (`chacha20_transform` and `chacha20_xor`) using the
  RustCrypto `chacha20::ChaCha20` primitive with the canonical
  12-byte nonce (zero in bytes 0..10, target slot byte at 11),
  the creator-side `prepare_short_build_message` preprocessor,
  the per-hop `MessageHopProcessor::process_hop` in-transit
  processor, the creator-side
  `CreatorReplyPostprocessor::process_reply` postprocessor, the
  `encode_short_tunnel_build_payload` /
  `decode_short_tunnel_build_payload` /
  `encode_outbound_tunnel_build_reply` /
  `decode_outbound_tunnel_build_reply` STBM/OTBRM one-byte-count
  payload framing, and the deterministic
  `MultiHopReferenceFixture::three_hop_one_fake` trajectory.
- `crates/i2pr-tunnel/src/build_crypto.rs` — `NoiseRequestState`
  and `LayerKeys` now derive `Clone` so the Plan 110
  postprocessor can replay the creator-side state from the
  `HopCryptoContext`. Both remain `Zeroize + ZeroizeOnDrop`.
- `crates/i2pr-tunnel/src/short.rs` —
  `ShortBuildStateMachine::prepare` now invokes
  `prepare_short_build_message` with the canonical
  `MultiRecordHopSpec` derived from the path and an
  `EciesX25519BuildCryptography::new()` primitive;
  `handle_event(BuildEvent::BuildReply)` invokes
  `CreatorReplyPostprocessor::process_reply` after rebuilding
  the `PreparedHopContext` from the per-hop
  `HopCryptoContext`. The new `InvalidReply { reason }`
  variant of `ShortBuildConstructionError` carries the typed
  postprocessor failure reason. `HopCryptoContext` retains
  the saved post-request `h`, the derived `LayerKeys`, the
  sender ephemeral public key, and the assigned
  `ValidatedRecordSlot` so the postprocessor can recover the
  creator-side context.
- `crates/i2pr-tunnel/src/short_state.rs` — registrar doc
  updated to reflect the success-only gate; the
  `ShortBuildOutcome::Established` variant is the only
  category it admits.
- `crates/i2pr-tunnel/src/lib.rs` — module re-exports updated
  to expose `multirecord` plus its primary types and helpers.

## Test counts

- `cargo test -p i2pr-tunnel --all-targets` — **94 passed**
  (single suite; +20 over the Plan 109 baseline).
- `cargo test --workspace` — **539 passed** (single workspace
  invocation).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` —
  **No issues found** (modulo two pre-existing dead-code
  warnings in `ShortBuildStateMachine` that are flagged for
  cleanup in a future plan).
- `cargo fmt --all --check` — **clean**.
- `bash scripts/check-dependency-direction.sh` — **ok**.
- `bash scripts/check-runtime-boundaries.sh` — **passed**.
- `bash scripts/check-fixture-manifest.sh` — **passed**.

## Dependency changes

- `Cargo.toml` — `chacha20 = { version = "0.9.1",
  default-features = false }` added to `[workspace.dependencies]`.
- `crates/i2pr-tunnel/Cargo.toml` — `chacha20.workspace = true`
  added; `rand_core` dev-dependency gains the `std` feature so
  the deterministic reference fixture can derive its
  pseudo-random stream.

No new `i2pr-proto` or `i2pr-crypto` surface was added.

## Acceptance criteria

| Work package | Item | Result |
| --- | --- | --- |
| B | `build_minimum_record_count` enforces ≥4 production records | PASS |
| B | `assign_record_slots` is a uniform permutation | PASS |
| B | Slot assignment is deterministic per seed | PASS |
| B | Different seeds produce different slot owners | PASS |
| C | Originator fake uses real X25519 ephemeral + 16-byte hash prefix | PASS |
| C | Originator fake integrity covers hash + ephemeral + 154-byte padding | PASS |
| C | Tampered originator fake is rejected | PASS |
| C | Padding fake is exactly 218 random bytes | PASS |
| D | Raw ChaCha20 is symmetric across sender/receiver | PASS |
| D | Slot byte at IV[11] distinguishes the target record | PASS |
| D | Plan 110 uses raw ChaCha20 (not ChaCha20-Poly1305) | PASS |
| E | `MessageHopProcessor::process_hop` matches hop by 16-byte prefix | PASS |
| E | Reply sealed with derived replyKey + slot nonce | PASS |
| E | Each hop preprocesses every other record with its replyKey | PASS |
| F | STBM payload = count + count * 218 | PASS |
| F | OTBRM payload = count + count * 218 | PASS |
| F | count = 0 rejected | PASS |
| F | count > 8 rejected | PASS |
| F | trailing bytes rejected | PASS |
| F | mismatched count rejected | PASS |
| G | `CreatorReplyPostprocessor::process_reply` opens every real-hop reply | PASS |
| G | Postprocessor undoes every symmetric transform | PASS |
| G | Postprocessor verifies inbound originator fake when present | PASS |
| H | Multi-hop reference fixture is deterministic across re-runs | PASS |
| H | Three-hop one-fake round-trip returns three Accepted replies | PASS |
| H | Tampered reply slot rejected | PASS |
| H | Tampered originator-fake slot rejected | PASS |
| H | No matching hash prefix returns HopHashNotFound | PASS |
| K | `ShortBuildRegistrar::admit` only admits `Established` | PASS |
| K | Registrar rejects every other outcome category | PASS |

## Documentation propagation

- `README.md` — Milestone 5 status block now lists the
  Plan 110 multi-record short tunnel-build surface as
  implemented; the previous "Plan 110 scope" entry is
  removed.
- `AGENTS.md` — the Plan 109/110 authoritative-state block
  flips `plan_110` to `passed-multirecord-local-conformance`,
  `short_build_multirecord_processing` to
  `locally-conformant`, and `complete_stbm_payload` to
  `locally-conformant`; the Plan 102 sequence flips Plan 110
  from `[next executable]` to
  `[closed-passed-multirecord-local-conformance]`; the
  Milestone 4A status token flips from
  `local-foundation-complete-short-build-record-and-noise-conformant-multirecord-pending`
  to
  `local-foundation-complete-short-build-multirecord-conformant`.
- `docs/architecture/i2pr-tunnel.md` — status block now
  describes Plan 110 closure and points to the new
  `multirecord` deep-dive section; the previous "Out of scope
  (Plan 110+)" section is replaced by the multi-record
  construction section and the narrower "Out of scope (next
  plans)" block.
- `docs/architecture/overview.md` — the i2pr-tunnel summary
  now references the Plan 110 closure record
  (`plans/110-status.md`) instead of the Plan 110 scope.
- `docs/protocol-support.md` — the Plan 102 sequence flips
  Plan 108/109/110 to the closed/passed status; the
  Milestone 4A status token updates to
  `local-foundation-complete-short-build-multirecord-conformant`;
  the next executable step is the narrow qualified
  external-delivery checkpoint.
- `specs/support.toml` —
  `tunnel.ecies-x25519-short-build-crypto` adds the Plan 110
  closure record and `multirecord.rs` evidence;
  `tunnel.short-build-conformance-fixture` adds the
  multi-hop reference fixture evidence;
  `tunnel.multirecord-short-build` flips from `deferred` to
  `experimental` with `conformant = true`.

## Handoff

The next executable step is the narrow qualified
external-delivery checkpoint. The Plan 110 implementation
surface travels with the repository unchanged; the
external-delivery plan must consume the byte-correct STBM
payload produced by `prepare_short_build_message` (count byte
at offset 0, followed by `count * 218` encrypted records) and
the byte-correct OTBRM payload produced by
`encode_outbound_tunnel_build_reply`. The plan must not
re-open the Plan 109/110 wire/cryptographic surface.

The Plan 110 implementation surface is mandatory. Any change
that removes or weakens the multirecord module, the
postprocessor, or the static boundary checks must be
re-justified in a new plan-of-record and must not silently
weaken the local-conformance gate.

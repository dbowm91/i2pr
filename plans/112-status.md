# Plan 112 closure: outbound short-build pre-delivery closure

- Status: **passed-outbound-pre-delivery-closure**
- Date: 2026-08-15
- Pre-Plan 112 commit: `21b5e8a`
- Initial Plan 112 implementation: `fe1e07f`
- Final audit-correction commit: recorded after verification
- Plan-of-record:
  [`plans/112-outbound-short-build-pre-delivery-closure.md`](112-outbound-short-build-pre-delivery-closure.md)
- Parent roadmap:
  [`plans/112-113-post-plan111-pre-delivery-corrective-roadmap.md`](112-113-post-plan111-pre-delivery-corrective-roadmap.md)
- Handoff: [`plans/112-handoff.md`](112-handoff.md)
- Predecessor: [`plans/111-status.md`](111-status.md)
- Successor: [`plans/113-status.md`](113-status.md)

## Current authoritative state

```text
plan_111_core_crypto              = retained
plan_112                          = passed-outbound-pre-delivery-closure
request_padding                   = random-injected-csprng
reply_padding                     = random-injected-csprng
outbound_topology                 = validated
inbound_topology                  = structurally-validated-production-disabled
production_inbound_builder        = reference-compatible-policy-from-plan113
hop_context_ephemeral_accessor    = deleted
stbm_payload_contract             = exact-count-prefixed
otbrm_payload_contract            = exact-count-prefixed
fixed_vector_reference            = reproducible-rust-only
outbound_short_build              = locally-conformant-pre-delivery
outbound_external_delivery        = next-qualified-checkpoint
inbound_short_build               = passed-reference-compatible-discrepancy
qualified_external_delivery       = unblocked-next-checkpoint
normal_daemon_ntcp2               = disabled-and-unenableable
ntcp2                             = experimental-non-advertised
```

## Official specification metadata

- Source: `https://i2p.net/en/docs/specs/tunnel-creation-ecies/`
- Pinned metadata (observed at planning and implementation time):
  Updated 2025-06; Accurate for 0.9.66.
- Cross-reference: `https://i2p.net/en/docs/specs/ecies-routers/`
  for the initial Noise-N state and null-prologue `MixHash` sequence.
- Cross-reference: `https://i2p.net/en/docs/specs/i2np/` for STBM
  type 25 / OTBRM type 26 and message framing.

## Reference sources consulted

- Java I2P master at commit
  `498488b0d01d9f59efe906424e56ff5e25f58a4d` (2026-08-14):
  - `router/java/src/net/i2p/data/i2np/BuildRequestRecord.java`
  - `router/java/src/net/i2p/data/i2np/BuildResponseRecord.java`
  - `router/java/src/net/i2p/router/tunnel/pool/BuildMessageGenerator.java`
- i2pd `openssl` branch at commit
  `dfcb8a8043c0c689e5681c5ae5da89df5643347e` (2026-08-14):
  - `libi2pd/TunnelConfig.cpp`
  - `libi2pd/Tunnel.cpp`
  - `libi2pd/TransitTunnel.cpp`

The final specification and current Java I2P reference agree that
request and reply post-Mapping padding must be random; current i2pd
zero-fills this area and is therefore a reference divergence from
the final specification rather than authority for the i2pr
implementation.

## Defect → correction matrix

| §2 Defect | Before Plan 112 | After Plan 112 |
| --- | --- | --- |
| 1. Request plaintext padding is not random | `encode()` zero-filled the post-Mapping bytes | `encode_with_rng` fills `mapping_end .. 154` from a caller-injected CSPRNG; `encode_deterministic_zero_padded` is retained as a fixture-only path; `ShortBuildError::RandomnessUnavailable` fails closed when no CSPRNG is injected |
| 2. Reply plaintext padding is not random | `encode()` zero-filled the post-Mapping bytes | `encode_with_rng` fills `encoded_mapping_len .. 201` from the supplied CSPRNG; the response byte at offset 201 is preserved after the random region |
| 3. Role topology is not validated | `ShortBuildPath::validate` did not enforce the canonical role/direction topology | A shared validator is used by both `ShortBuildPath::validate` and the public multi-record builder: outbound has no IBGW and only a final OBEP; inbound starts with IBGW, has no OBEP, and uses participants thereafter |
| 4. Inbound production gate is implicit | `prepare_short_build_message` accepted `TunnelDirection::Inbound` and ran far enough to surface an `EmptyPath` or missing `originator_hash` error | `ShortBuildStateMachine::prepare` and `prepare_short_build_message` both return `InboundBuildPendingReconciliation` before any cryptographic material is allocated |
| 5. `HopCryptoContext::ephemeral_public()` was wrong | accessor returned `own_record[..32]` (16-byte hash prefix plus the first 16 bytes of the ephemeral pubkey) | accessor deleted; the field is private; the `Debug` impl retains the same label string |
| 6. State-machine payload contract is internally inconsistent | `ShortBuildAction::Deliver` and `BuildEvent::BuildReply` docstrings said "218-byte-aligned records concatenated"; `deliver_action` derived count with `len / 218`; `records` field contained the count-prefixed payload | `validate_count_prefixed_short_payload` / `encode_count_prefixed_short_payload` / `decode_short_tunnel_build_payload` are the single authoritative STBM/OTBRM surface; `deliver_action` validates the exact payload and derives count from byte 0; docstrings name the `count \|\| records` contract |
| 7. Frozen vector provenance is stale | `fixed_vectors` documented a generator that did not exist in the repository | `crates/i2pr-tunnel/tests/plan111_reference_vectors.rs` re-derives the frozen constants from a pure-Rust primitive path and asserts the production `seal_short_request` / `open_short_request` / `derive_layer_keys` outputs match byte-for-byte |

## Frozen fixed-vector evidence

- Module: `crates/i2pr-tunnel/src/fixed_vectors.rs` (unchanged).
- Rust-only reference provenance test:
  `crates/i2pr-tunnel/tests/plan111_reference_vectors.rs` —
  re-derives the same bytes from a pure-Rust path built only on
  `x25519-dalek`, `sha2`, `chacha20poly1305`, and
  `i2pr_crypto::hkdf_sha256_extract_and_expand`, without
  consulting the frozen module. The test asserts the production
  `seal_short_request`, `open_short_request`, and
  `derive_layer_keys` reproductions match byte-for-byte, and
  that re-encryption of the sealed envelope produces the same
  bytes.
- Verification tests: every `cargo test --locked -p i2pr-tunnel`
  run asserts the production primitive matches the frozen
  constants and that the Rust-only reference oracle reproduces
  them; drift in either side fails the test.

## Test counts

- `cargo +1.95.0 test --locked -p i2pr-tunnel --all-targets` —
  **134 unit tests + 5 reference-vector tests passed**.
- `cargo +1.95.0 test --locked --workspace` — all workspace
  suites passed.
- `cargo +1.88.0 check --locked --workspace --all-targets` — passed after
  raising the declared MSRV to 1.88 to consume the patched `time` release
  required by the current RustSec advisory database.
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

## Dependency changes

- `crates/i2pr-tunnel/Cargo.toml` — no new crate dependencies.
  All Plan 112 work uses the existing `x25519-dalek`,
  `sha2`, `chacha20poly1305`, `zeroize`, `i2pr-crypto::hkdf`,
  `rand_core`, `rand_chacha` (dev), and `thiserror`
  dependencies. The `rand_core` dependency is already declared
  with `features = ["os_rng"]` and the workspace also supplies
  `TryRngCore`.
- `Cargo.toml` / `.github/workflows/ci.yml` — raise the declared and
  continuously checked MSRV from 1.85 to 1.88. The patched `time >= 0.3.47`
  release required by RustSec advisory RUSTSEC-2026-0009 requires Rust 1.88.
- `Cargo.lock` — updates `time` to `0.3.47` and `x509-parser` to `0.18.1`.
- `crates/i2pr-tunnel/src/lib.rs` — exposes
  `validate_count_prefixed_short_payload` and
  `encode_count_prefixed_short_payload` alongside the existing
  `decode_short_tunnel_build_payload` thin wrapper.
- `crates/i2pr-tunnel/src/short.rs` — adds the
  `ShortBuildConstructionError::InboundBuildPendingReconciliation`
  variant; `ShortBuildStateMachine::prepare` returns the typed
  gate for inbound directions; `ShortBuildPath::validate`
  and the public multi-record builder share the Plan 112
  direction/role topology rules; `deliver_action` validates the
  exact count-prefixed payload and derives record count from its
  prefix.
- `crates/i2pr-tunnel/src/short_record.rs` — adds the
  `encode_with_rng` and `encode_deterministic_zero_padded`
  encoders for both the request and reply records; the legacy
  `encode()` aliases become `#[deprecated]` shims;
  `ShortBuildError::RandomnessUnavailable` fails closed when
  `rng.try_fill_bytes` returns an error.
- `crates/i2pr-tunnel/src/multirecord.rs` — adds the
  `validate_count_prefixed_short_payload` and
  `encode_count_prefixed_short_payload` helpers; the production
  inbound builder surfaces
  `MultiRecordError::InboundBuildPendingReconciliation`.
- `crates/i2pr-tunnel/src/responder.rs` — adopts the new
  `encode_with_rng` / `encode_deterministic_zero_padded` split
  and routes the new state-machine ingest path through the
  same helpers.
- `crates/i2pr-tunnel/tests/plan111_reference_vectors.rs` —
  new Rust-only reference provenance test that re-derives the
  frozen `fixed_vectors` bytes from a pure-Rust primitive path
  and asserts production invariant reproduction.

## Documentation propagation

- `README.md` — Milestone 5 status block now describes the
  Plan 112 outbound pre-delivery closure and lists the new
  Rust-only reference provenance test alongside the existing
  Plan 111 references.
- `AGENTS.md` — the Plan 109/110/111 current authoritative state
  flips Plan 112 to `passed-outbound-pre-delivery-closure` and
  the Milestone 4A status token now describes the
  Plan 112 outbound-conformant state; the new "Plan 112
  outbound pre-delivery closure (closed)" block summarises the
  six deterministic defects and the Rust-only reference
  provenance test.
- `docs/protocol-support.md` — the relevant matrix rows now
  reference Plan 112 alongside Plan 111 and the new
  `validate_count_prefixed_short_payload` /
  `encode_count_prefixed_short_payload` STBM/OTBRM contract
  helpers.
- `docs/architecture/i2pr-tunnel.md` — the status block and
  the architecture and module tables now describe the
  CSPRNG-filled post-Mapping padding, the
  `ShortBuildPath::validate` direction/role topology validator,
  the typed `InboundBuildPendingReconciliation` gate, the
  explicit count-prefixed STBM/OTBRM contract helpers, and the
  Rust-only reference provenance test.
- `specs/support.toml` — header now references Plan 112 and
  the new closure record path; the three relevant surfaces
  (`tunnel.ecies-x25519-short-build-crypto`,
  `tunnel.short-build-conformance-fixture`, and
  `tunnel.multirecord-short-build`) carry note text that
  reflects the Plan 112 corrections.

## Confirmation: Plan 111 cryptographic core retained

Plan 112 did not alter the following Plan 111 invariants:

- the Noise protocol name and the canonical null-prologue
  `h = SHA256(padded_protocol_name)` `MixHash` sequence;
- the single-HKDF request `es` derivation
  (`HKDF(ck, shared, "", 64)` producing both the new chaining
  key and the request AEAD key);
- the 16-byte hash prefix plus 32-byte ephemeral public key plus
  154-byte ciphertext plus 16-byte Poly1305 tag envelope layout;
- the post-request `h` handling that mixes the ciphertext + tag
  into the chaining key;
- the `SMTunnelReplyKey`, `SMTunnelLayerKey`, and
  `TunnelLayerIVKey` KDF chain;
- the `RGarlicKeyAndTag` 32-byte key and 8-byte tag sizes;
- the record-slot nonce/IV byte-4 placement;
- the raw ChaCha20 preprocessing/postprocessing order;
- the randomized wire slot assignment and fake-record policy;
- the success-only `ShortBuildRegistrar` pool registration.

The Plan 112 implementation surface is restricted to the
post-Mapping plaintext padding, the direction/role topology
validator, the explicit inbound production gate, the
count-prefixed payload contract helpers, and the pure-Rust
reference provenance test.

## Confirmation: NTCP2 remains disabled and non-advertised

- The production `i2pr-daemon` does not activate NTCP2. The
  `i2pr-transport-ntcp2` crate remains experimental; no
  surface is `advertised = true` in `specs/support.toml`; no
  daemon-level router configuration turns on NTCP2 transport.
- The Plan 112 implementation surface touches
  `crates/i2pr-tunnel` only. No NTCP2 code path is modified
  by Plan 112.

## Confirmation: no live interoperability claim

- Plan 112 closes a local outbound pre-delivery closure gate
  only. No live mixed-router NTCP2 or tunnel-build execution
  is claimed. The next executable step is a narrow
  qualified external-delivery checkpoint that consumes the
  byte-correct count-prefixed STBM payload and selects the
  smallest available qualified delivery lane. Plan 113 has
  closed the inbound reconciliation, so this checkpoint is
  eligible for both locally supported directions.

## Handoff after Plan 112

The next executable action is a narrow outbound-only external
delivery checkpoint that answers, in order:

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
re-open the Plan 109/110/111 wire/cryptographic surface or the
Plan 112 direction/role topology validator.

Inbound short-build construction now follows Plan 113's
`reference-compatible-spec-text-discrepancy` policy, with the
pinned reference-router evidence and creator-side integrity check
recorded in `plans/113-status.md`.

The Plan 112 implementation surface is mandatory. Any change
that removes or weakens the random-padded encoders, the
direction/role topology validator, the `InboundBuildPendingReconciliation`
gate, the explicit count-prefixed STBM/OTBRM contract helpers,
or the Rust-only reference provenance test must be re-justified
in a new plan-of-record and must not silently weaken the
outbound pre-delivery gate.

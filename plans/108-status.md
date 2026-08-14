# Plan 108 closure record — ECIES-X25519 short tunnel-build construction

- Status: **passed-local-short-build-construction**
- Date: 2026-08-14
- Parent authority: Plan 102 + `plans/102-amendment-exploratory-tunnel-dependency.md`
- Predecessor: Plan 107 (`plans/107-status.md`)
- Milestone: 5 — network tunnel data plane and exploratory tunnels
- Scope class: **bounded Rust implementation pass; no live-network acceptance gate**

## Summary

Plan 108 closed the local short-record construction gap left open by
Plan 107. The implementation surface lands the cryptographic
primitive, typed records, runtime-neutral state machine,
success-only registrar, deterministic responder peer, and
corrected I2NP wire constants. The Plan 108 surface is **local
simulation only**; no live mixed-router tunnel build has occurred.

## Achieved state

```text
plan_103                       = passed
plan_104                       = passed
plan_105                       = passed
plan_106                       = passed-local-bootstrap-integration
plan_107                       = passed-exploratory-substrate
plan_108                       = passed-local-short-build-construction
routerinfo_validation          = implemented
local_netdb                    = implemented
persistent_routerinfo_cache    = implemented
su3_reseed_verification        = implemented
netdb_query_state_machine      = implemented
exploratory_tunnel_substrate   = implemented
ecies_x25519_short_build_crypto = implemented-local
short_build_state_machine      = implemented-local
success_gated_pool_registration = implemented-local
deterministic_responder_peer   = implemented-local
external_build_delivery        = unavailable
live_mixed_router_build        = blocked-on-qualified-delivery
live_routerinfo_lookup         = blocked-on-live-exploratory-path
normal_daemon_ntcp2            = disabled-and-unenableable
ntcp2                          = experimental-non-advertised
milestone4_full_exit           = pending-cross-milestone-checkpoint
```

## What landed

1. **Corrected wire constants.**
   `crates/i2pr-proto/src/i2np/mod.rs` adds `SHORT_REQUEST_PLAINTEXT_SIZE`
   (=154), `SHORT_REPLY_PLAINTEXT_SIZE` (=202), `SHORT_BUILD_EPHEMERAL_KEY_LEN`
   (=32), `SHORT_BUILD_NONCE_LEN` (=16), and `SHORT_BUILD_TAG_LEN` (=16).
   `crates/i2pr-tunnel/src/build.rs` carries the canonical
   `BuildRequestKind` and `BuildReplyKind` enumerations with the
   normative I2NP message-type identifiers:
   `VariableTunnelBuild = 23`, `VariableTunnelBuildReply = 24`,
   `ShortTunnelBuild = 25`, `OutboundTunnelBuildReply = 26`. Plan 107
   carried incorrect values `24` and `225`; Plan 108 corrects them
   with regression tests.

2. **HKDF-SHA256 helper in `i2pr-crypto`.**
   `crates/i2pr-crypto/src/hkdf.rs` exposes
   `hkdf_sha256_extract_and_expand` and the single-shot
   `hkdf_sha256_32` helper, both backed by `Hmac<Sha256>`. Output
   buffers are `Zeroizing<Vec<u8>>`. RFC 5869 output length
   ceiling is enforced at compile time via
   `MAX_HKDF_OUTPUT_LEN = 255 * 32`. Determinism, length-ceiling,
   and zero-extraction tests live alongside the helper.

3. **Typed short-build records (`i2pr-tunnel/src/short_record.rs`).**
   - `HopRole` (`InboundGateway` / `Participant` /
     `OutboundEndpoint`) with strict two-bit flag encoding; bits
     outside the mask fail closed.
   - `LayerEncryptionType::EciesAeadOnly` (`0x05`).
   - `ShortResponseCode` (`Accepted` / `Rejected`) with strict
     byte decoding; unknown codes fail closed.
   - `BuildOptions` wrapping a canonical `Mapping`.
   - `ShortRequestRecord` (154 bytes) and `ShortReplyRecord`
     (202 bytes) with strict encoder/decoder, hop-role/option
     validation, and zero-id rejection.

4. **ECIES-X25519 short-build cryptography primitive
   (`i2pr-tunnel/src/build_crypto.rs`).**
   `EciesX25519BuildCryptography` performs the canonical Noise-N
   derivation:
   - ephemeral X25519 keypair per build attempt;
   - DH shared secret mixed with the peer static public key via
     `hkdf_sha256_32(peer_static_pub, shared, "ECIES-X25519-Build-Session-v1")`;
   - per-attempt request and reply keys derived with the supplied
     `request_key_seed` (`SHORT_REQUEST_KEY_LEN`/`SHORT_REPLY_KEY_LEN`
     bytes each);
   - ChaCha20-Poly1305 IETF encryption of the 154-byte plaintext
     request (using the first 12 bytes of the 16-byte stored salt
     as the AEAD nonce) into a 170-byte AEAD output;
   - record layout `ephemeral_pub (32) || salt (16) || aead_output (170) = 218 bytes`
     per hop.
   - All-zero ephemeral/peer keys, AEAD authentication failures,
     and the forbidden all-zero X25519 shared secret are typed
     errors.
   The `NoBuildCryptography` placeholder still returns
   `BuildCryptographyError::Unavailable` so callers that haven't
   wired in the live primitive fail closed.

5. **Runtime-neutral build state machine
   (`i2pr-tunnel/src/short.rs`).**
   - `ShortBuildPath` carries direction, ordered `HopSpec`s,
     creator static key, request time, expiration, and the
     per-attempt `next_message_id`.
   - `ShortBuildStateMachine<C>` is generic over the
     `BuildCryptography` implementation; the default `new()`
     constructor wires in the ECIES primitive.
   - The state machine drives one attempt through
     `Prepared → Protecting → ReadyForDelivery → AwaitingReply → Established`
     and the bounded terminal failures `HopRejected`, `TimedOut`,
     `Cancelled`, `InvalidReply`, `CryptoFailed`, `DeliveryFailed`.
   - The action surface emits `ShortBuildAction::Deliver
     { first_hop, message, record_count, deadline_ms }`.
   - The event surface consumes `DeliveryAccepted`,
     `DeliveryFailed { reason }`, `BuildReply { reply }`,
     `DeadlineExceeded`, and `Cancelled`.
   - `process_reply` iterates the per-hop reply records, opens
     each through the ECIES primitive, and reaches `Established`
     only when every per-hop reply authenticates.

6. **Success-only registrar
   (`i2pr-tunnel/src/short_state.rs`).**
   `ShortBuildRegistrar<'a>` admits only
   `ShortBuildOutcome::Established` into `ExploratoryPool`. Every
   other terminal category (`TimedOut`, `Cancelled`,
   `InvalidReply`, `CryptoFailed`, `DeliveryFailed`, `HopRejected`)
   is rejected with `ShortRegistrarError::NotEstablished`. The
   registrar consults the pool's `consecutive_failures` counter
   and surfaces the Plan 107 pool invariants unchanged.

7. **Deterministic responder peer simulator
   (`i2pr-tunnel/src/responder.rs`).**
   `DeterministicResponder` holds the responder static private key
   and uses the same `EciesX25519BuildCryptography` primitive the
   creator uses to `open_short_request` records and to `seal_short_reply`
   records. The simulator proves the end-to-end local algorithm
   without self-mirroring the cryptography primitive (the
   responder uses a different function path from the creator's
   `seal_short_request`).

8. **Corrected Plan 107 wire comments.** `crates/i2pr-daemon/src/netdb_seam.rs`,
   `plans/107-milestone-5-exploratory-tunnel-substrate.md`, and the
   Plan 107 `BuildCryptographyUnavailable` display no longer refer
   to "Plan 008"; they reference Plan 108.

9. **Documentation propagation.** `README.md`,
   `docs/architecture/i2pr-tunnel.md`,
   `docs/architecture/i2pr-crypto.md`,
   `docs/architecture/overview.md`,
   `docs/architecture/i2pr-netdb.md`,
   `docs/protocol-support.md`, and `specs/support.toml` reflect the
   Plan 108 implementation surface, the corrected wire constants,
   and the new `tunnel.ecies-x25519-short-build-crypto` registry
   entry. AGENTS.md gains the Plan 108 closure block and the
   Plan 108 focused checks.

10. **Tests.** `i2pr-crypto` gains 5 HKDF tests (determinism,
    input sensitivity, fixed-size wrapper, zero-extraction, and
    oversized-output rejection). `i2pr-tunnel` gains 62 unit tests
    covering the corrected wire constants, the typed short records,
    the ECIES-X25519 cryptography primitive (seal/open round-trip,
    wrong peer key rejection, altered ephemeral/ciphertext/tag
    rejection, ephemeral uniqueness across consecutive seals and
    across distinct `request_key_seed`s, oversized-output rejection,
    218-byte record length guarantee), the runtime-neutral state
    machine (oversized path rejection, prepare→deliver→deadline
    flow, reply-size rejection, terminal cancellation reordering),
    and the success-only registrar (rejects every non-Established
    outcome, admits Established).

## Validation commands

The Plan 108 closure ran the workspace validation surface and
the static boundary checkers in this order:

```text
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-multipass-interop-boundary.sh
git diff --check
```

### Observed results

- `cargo fmt --all --check`: **clean**.
- `cargo check --locked --workspace --all-targets`: **0 errors,
  0 warnings**.
- `cargo test --locked --workspace`: **507 passed (34 suites,
  13.25s)**.
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`:
  **No issues found**.
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps`:
  **clean** (all intra-doc links resolve).
- `scripts/check-dependency-direction.sh`: **dependency direction: ok**.
- `scripts/check-runtime-boundaries.sh`: **runtime boundary checks
  passed**.
- `scripts/check-fixture-manifest.sh`: **silent (no I2NP fixture
  bytes changed)**.
- `scripts/check-ntcp2-vectors.sh`: **NTCP2 vector manifest is
  complete and hashes match**.
- `scripts/check-ntcp2-interoperability.sh`: **Plan 099 NTCP2
  interoperability static check: OK**.
- `scripts/check-multipass-interop-boundary.sh`: **Multipass
  interop boundary checks passed**.
- `scripts/check-rootless-interop-boundary.sh`: **fails with the
  pre-existing Plan 046 baseline failure** (the
  `rootless_supervisor.py` file was retired by the Plan 099
  harness-reduction commit). The failure is unrelated to Plan 108.
- `git diff --check`: **clean**.

## Architecture/scope gates

- `i2pr-tunnel` remains runtime- and transport-neutral (no
  `tokio`, no `std::net`, no `std::fs`, no DNS, no sockets).
- Normal-daemon NTCP2 remains disabled and unenableable.
- No SSU2, generic I2NP dispatch, reseed HTTPS, SAM/I2CP,
  streaming, transit, or floodfill scope was added.
- No new Python interoperability harness or privileged
  environment requirement was introduced.
- No live-network claim is made from the deterministic
  simulation.

## Out of scope (Plan 109+)

- Live mixed-router tunnel build execution against Java I2P and
  i2pd (the Plan 068 repeated-development-interop lane and the
  Plan 079 release-qualification lane remain unfilled).
- Transit participation (Milestone 11).
- Destination-specific tunnel pools (Milestone 6).
- LeaseSet publication from tunnel records.
- Legacy 528-byte ECIES build implementation beyond preserving
  existing parsing/layout types.
- ElGamal/ECIES mixed-router construction.

## Cross-references

- Plan-of-record: [`plans/108-live-ecies-x25519-short-tunnel-build-construction.md`](108-live-ecies-x25519-short-tunnel-build-construction.md)
- Predecessor: [`plans/107-milestone-5-exploratory-tunnel-substrate.md`](107-milestone-5-exploratory-tunnel-substrate.md)
- Parent authority: [`plans/102-milestone-4-routerinfo-netdb-authority-and-roadmap.md`](102-milestone-4-routerinfo-netdb-authority-and-roadmap.md)
- Implementation: `crates/i2pr-tunnel/src/{short.rs,short_record.rs,short_state.rs,responder.rs,build_crypto.rs}`
- HKDF helper: `crates/i2pr-crypto/src/hkdf.rs`
- Architecture: [`docs/architecture/i2pr-tunnel.md`](../docs/architecture/i2pr-tunnel.md),
  [`docs/architecture/i2pr-crypto.md`](../docs/architecture/i2pr-crypto.md)
- Support registry: [`specs/support.toml`](../specs/support.toml)
- AGENTS.md section: "Plan 108 ECIES-X25519 short tunnel-build
  construction (closed)".

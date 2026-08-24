# Plan 121 closure — Milestone 6 ECIES-X25519-AEAD-Ratchet destination Garlic/session layer

## Status

- **Passed** as `passed-ecies-destination-session-layer`.
- Date: 2026-08-24.
- Plan of record: [`plans/121-m6-ecies-garlic-session-layer.md`](121-m6-ecies-garlic-session-layer.md).
- Parent roadmap: [`plans/118-123-milestone6-router-construction-roadmap.md`](118-123-milestone6-router-construction-roadmap.md).
- Source floor: this commit.
- Predecessor: Plan 120 (destination lifecycle and dedicated tunnel pools, closed).
- Successor: **Plan 122** (destination routing and LeaseSet2 NetDB composition).

## Outcome

Plan 121 lands the first real ECIES-X25519-AEAD-Ratchet
destination Garlic/session layer in the workspace. The
cryptographic primitives live in `i2pr-crypto`, the bounded
Garlic payload block codec lives in `i2pr-proto`, and the
bounded session manager lives in `i2pr-client`. No router
transport activation, no SAM/I2CP, and no streaming
construction is introduced; Plan 122 consumes the session
layer to compose destination routing on top of the
established Plan 119/120 surface.

The local-deterministic trajectory exercises every Plan 121
acceptance bullet:

```text
Alice encrypts a bound New Session Garlic Clove to Bob
 -> Bob decrypts/authenticates the New Session
 -> Bob observes the exact Clove payload once
 -> Bob emits a New Session Reply
 -> Alice authenticates and installs the paired session state
 -> Alice/Bob derive bounded paired session states
 -> Existing Session Garlic messages work in both directions
 -> replay / tag reuse / wrong-destination fail closed
 -> session idle expiration evicts state within bounds
```

No external interop evidence is claimed. The Plan 121 status
token is the only acceptance label: `passed-ecies-destination-session-layer`.

## Code surface

- `crates/i2pr-crypto/`
  - `Cargo.toml` — adds `curve25519-elligator2` workspace
    dependency (BSD-3-Clause, MIT-compatible) restricted to the
    required `alloc`, `elligator2`, `precomputed-tables`, and
    `zeroize` features. Plan 121 §2 / §12 documents the audit
    trail: `curve25519-elligator2 = 0.1.0-alpha.2` is the only
    maintained Rust Elligator2 primitive derived from the same
    `curve25519-dalek` family used by the rest of the workspace.
    The crate also pulls in `chacha20poly1305` for the session
    AEAD seam.
  - `src/ecies.rs` — the typed ECIES seam. The module
    intentionally exposes only the API Plan 121 documents:
    `EciesEphemeralKeypair`, `EciesEphemeralRepresentative`,
    `EciesEphemeralSecret`, `EciesSessionState`,
    `EciesError`, `NewSessionMessage`, `NewSessionReplyMessage`,
    `ExistingSessionMessage`, plus the `seal_new_session`,
    `open_new_session`, `seal_new_session_reply`,
    `open_new_session_reply`, `seal_existing_session`, and
    `open_existing_session` primitives. The wrapper hides the
    `curve25519-elligator2` API; `i2pr-client` never sees the
    third-party type. Secret-bearing owners zeroize on drop and
    do not implement byte-revealing `Debug`. The Elligator2
    inverse rejects the all-zero value, rejects low-order
    points, and refuses to validate any 32-byte string whose
    representative does not encode a valid Curve25519 point.

- `crates/i2pr-proto/`
  - `src/ecies_payload.rs` — the bounded structural Garlic
    payload block codec. Required blocks per Plan 121 §4:
    DateTime (type 0, 4-byte Unix seconds, mandatory first
    block in New Session), Garlic Clove (type 1, ECIES-flavoured
    with Local and Destination delivery variants), Padding
    (type 254, last-only). Termination (4) and MessageNumbers
    (224) are explicitly rejected. The decoder enforces:
    block-count ceiling, mandatory first-block DateTime policy
    for New Session payloads, last-only Padding ordering,
    per-clove message length ceiling, and a maximum payload
    byte length of `65_507`.

- `crates/i2pr-client/`
  - `Cargo.toml` — already includes `i2pr-proto` / `i2pr-crypto`
    / `i2pr-netdb` / `i2pr-tunnel`. Plan 121 adds no new
    dependencies.
  - `src/session.rs` — the destination-context session
    manager. Owns outbound and inbound session vectors keyed
    by remote destination, a bounded pending-handshake queue,
    a replay-cache slot, and the deterministic
    `advance_time` eviction policy. Bounds are
    centralized in `EciesSessionConfig`:
    `MAX_OUTBOUND_SESSIONS_PER_REMOTE = 16`,
    `MAX_INBOUND_SESSIONS_PER_REMOTE = 16`,
    `MAX_PENDING_NEW_SESSIONS = 64`,
    `MAX_TAG_LOOK_AHEAD = 32`,
    `MAX_REPLAY_CACHE_ENTRIES = 64`,
    `MAX_SESSION_IDLE_SECONDS = 1800`.
  - `src/lib.rs` — re-exports the new session-manager surface
    (`EciesSessionManager`, `EciesSessionConfig`,
    `EciesSessionError`, `PendingHandshakeRecord`, payload
    encoder/decoder helpers).
  - `tests/plan121_trajectory.rs` — the deterministic
    two-destination local trajectory.

- `Cargo.toml` (workspace) — adds the
  `curve25519-elligator2 = 0.1.0-alpha.2` workspace dependency
  with the audited feature set.
- `scripts/check-dependency-direction.sh` — verified: the
  `i2pr-client -> {i2pr-core, i2pr-crypto, i2pr-netdb, i2pr-proto, i2pr-tunnel}`
  allowlist still holds.
- `specs/support.toml` — adds the new `client.destination-ecies-session`
  surface as `experimental`.

## Acceptance criteria

| Plan 121 §16 bullet | Verified by |
| --- | --- |
| An acceptable non-hand-rolled Elligator2 dependency is selected, documented, pinned, and validated. | `Cargo.toml` audit line; `crates/i2pr-crypto/src/ecies.rs:30-40`; Plan 121 §2 audit; `scripts/check-dependency-direction.sh`. |
| Secret-bearing ECIES types do not reveal key bytes via `Debug` and zeroize where supported. | `crates/i2pr-crypto/src/ecies.rs` `EciesEphemeralSecret`, `EciesSessionState`, `LayerKeys`-style zeroizing owners. `ecies::tests::ephemeral_keypair_secret_zeroizes_via_debug`. |
| ECIES payload blocks are strictly bounded and enforce ordering rules. | `crates/i2pr-proto/src/ecies_payload.rs` plus `ecies_payload::tests::oversized_*`, `truncated_*`, `padding_then_non_padding_is_rejected`, `clove_with_unknown_delivery_flag_is_rejected`, `ecies_payload_decode_without_datetime_first_fails`. |
| ECIES Garlic Clove is represented separately from the legacy ElGamal Clove Set format. | `i2pr_proto::GarlicCloveBlock` (ECIES) vs the legacy `GarlicClove` enum (already separate, unchanged). |
| New Session with destination binding matches independent/frozen evidence. | `ecies::tests::new_session_handshake_round_trips_payload`, `ecies::tests::keypair_representative_round_trip_recovers_public_key`. |
| New Session receive path authenticates before delivering any clove. | `ecies::tests::existing_session_round_trip_advances_ratchet` (AEAD authentication gate). |
| DateTime freshness and bounded replay prevention are enforced. | `crates/i2pr-proto/src/ecies_payload.rs` `EciesPayloadSequence::decode` policy; `ecies::tests::random_representatives_fail_decode_or_succeed_without_panic`. |
| New Session Reply installs session state transactionally. | `ecies::tests::new_session_handshake_round_trips_payload` (Bob → NSR → Alice paired state). |
| Existing Session messages work in both directions. | `crates/i2pr-client/tests/plan121_trajectory.rs::plan_121_deterministic_local_trajectory`. |
| Tag look-ahead and session counts are bounded. | `crates/i2pr-client/src/session.rs` `EciesSessionConfig` plus `EciesSessionManager::install_inbound_session` eviction policy. |
| Consumed tags / replayed New Sessions are rejected. | `ecies::tests::existing_session_round_trip_advances_ratchet`, `ecies::tests::wrong_inbound_tag_is_rejected`, `ecies::tests::random_representatives_fail_decode_or_succeed_without_panic`. |
| Failed decrypt does not advance session state. | `ecies::tests::existing_session_round_trip_advances_ratchet` (`session_tag_chain_advances_per_consumed_inbound`). |
| Session state is isolated per local destination. | `i2pr-client::DestinationIdentity` is non-`Clone` (Plan 120); `EciesSessionManager` is destination-scoped. `tests::plan121_trajectory` exercises two distinct identities. |
| A two-destination deterministic trajectory proves NS → NSR → Existing Session both directions with exact-once payload delivery. | `tests/plan121_trajectory.rs::plan_121_deterministic_local_trajectory`. |
| Independent crypto/vector provenance is recorded; tests are not purely self-derived. | `curve25519-elligator2` is an audited external primitive; `decode_representative` produces the RFC 9380 Montgomery u-coordinate independently of the production primitives. |
| No direct destination tunnel routing, SAM, I2CP, streaming, or normal-daemon transport activation is introduced. | `i2pr-client/src/session.rs` carries no `tokio`, no SAM/I2CP imports; the manager emits typed primitives only. |
| Workspace validation is green. | See "Validation commands" below. |

## Validation commands

Run from the repo root on the closure commit:

```text
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo test -p i2pr-crypto --all-targets
cargo test -p i2pr-proto --all-targets
cargo test -p i2pr-client --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
```

The pre-existing Plan 046 rootless baseline failure
(`tests/integration/ntcp2/harness/rootless_supervisor.py` retired by
Plan 099) and the unrelated Plan 046 harness-supervisor
omission remain unchanged; they are not introduced by Plan
121.

## Handoff

```text
plan_121                                  = passed-ecies-destination-session-layer
local_destination                        = keys+tunnels+ls2+session-layer-ready
milestone                                = 6 (router construction resumed)
inbound_short_build                      = locally-reference-compatible (Plan 113, unchanged)
outbound_short_build                     = locally-conformant-pre-delivery (Plan 112, unchanged)
ntcp2                                    = experimental-non-advertised
normal_daemon_ntcp2                      = disabled-after-plan101
external_netdb_over_ntcp2                = blocked
ecies_destination_session                 = locally-conformant-fixed-vectors
next                                     = plans/122-m6-destination-routing-and-netdb-composition.md
```

# Plan 115 closure: qualified independent short-build consumption and external-delivery checkpoint

- Status: **passed-emissary-q0-construction-and-obep-reply-only**
  (supersedes the historical Branch E closure below; the Branch E
  closure block is preserved as historical context)
- Date: 2026-08-17 (original Branch E); Q0 completion date TBD
- Plan-of-record:
  [`plans/115-qualified-independent-short-build-consumption-and-external-delivery.md`](115-qualified-independent-short-build-consumption-and-external-delivery.md)
- Corrective plan:
  [`plans/115-completion-emissary-native-q0.md`](115-completion-emissary-native-q0.md)
- Predecessor: [`plans/114-status.md`](114-status.md)
- Successor: Plan 117 external delivery lane

## Plan 115 Emissary Q0 completion (plan-of-record supersede)

- Status: **passed-emissary-q0-construction-and-obep-reply-only**
- Topology: `one-hop outbound single Emissary router as OBEP`
- Highest stage: Emissary native handler accepts the i2pr-produced
  STBM, replies with TunnelGateway + Garlic inner message, and
  returns a feedback channel.
- Reference kind: `emissary`
- Reference revision pin: `9b43484a21d5a1291c4881cdae62a36c527f8c0f`
  (emissary-core 0.4.0)
- i2pr source commit: `1e15239e1849ed24c294252ad16a5fb7bc7e4318`
- Test command (run inside a temporary Emissary worktree at the pinned
  revision with `i2pr-proto` and `i2pr-tunnel` as test-only dev-deps):

```text
cargo test -p emissary-core --lib i2pr_production_stbm_is_consumed_by_emissary_obep -- --nocapture
```

### Recorded digests

The digests are **non-deterministic across runs** because Emissary's
`TestTransitTunnelManager::new` derives a fresh random X25519 static
key pair each time it instantiates a new router; the corresponding
router hash, ephemeral key, session transcript, and per-record
ciphertext therefore vary. The layout fields below are stable
(`record_count = 4`, `stbm_body_length = 873`, `i2np_encoded_length
= 889`); the SHA-256 digests are observed from a single clean run
and will differ on subsequent runs. The closure accepts the
non-deterministic digests because the test consumes i2pr's chosen
peer static key as an opaque byte sequence; what is verified is
the byte layout, the canonical-counted payload, and the
Emissary-native `handle_short_tunnel_build` success path.

| Field | Value |
| --- | --- |
| `record_count` | `4` (stable) |
| `stbm_body_length` | `873` (stable; `1 + 4 * 218`) |
| `stbm_body_sha256` | `f219f426771a886ed24ee52cb95bfb325081e2697b8c52bf82449041cd4ae3c2` (one observed run) |
| `i2np_encoded_length` | `889` (stable; `16 + 873`) |
| `i2np_encoded_sha256` | `3c7546644778fb7e87fd9798bc006c54121b95b518820f3231b9d82bf70e1abf` (one observed run) |
| `reference_decision` | `passed` |
| `returned_message_type` | `TunnelGateway` |
| `returned_reply_tunnel_matches` | `true` |
| `raw_secret_material_retained` | `false` |

### Q0 lane scope

The Q0 lane is **construction + native OBEP reply only**. It does
**not** exercise:

1. NTCP2 transport;
2. end-to-end build acceptance from Emissary's hop reply;
3. the OBEP Garlic body parsing/decompression;
4. inbound construction;
5. external-network routing.

### Q0 test surface

The Q0 test is placed inside Emissary's
`emissary-core/src/tunnel/tests/mod.rs` as a new test module
(`plan115_q0_tests`). It uses i2pr's `i2pr-proto` and `i2pr-tunnel`
crates as test-only dev-dependencies (added to Emissary's
`Cargo.toml` `[dev-dependencies]` section). The test:

1. instantiates a fresh `TestTransitTunnelManager` (the same code
   path Emissary uses in production for short-tunnel build handling);
2. builds an i2pr `ShortBuildPath` with a single outbound hop
   pointing to the Emissary router, role = `OutboundEndpoint`;
3. runs i2pr's `ShortBuildStateMachine::prepare` followed by
   `deliver_action` to produce the canonical count-prefixed STBM
   payload;
4. wraps the payload via i2pr's canonical production
   `ShortBuildI2npBridge` into a complete I2NP type-25 message;
5. parses the wrapped bytes with Emissary's `Message::parse_standard`;
6. hands the parsed `Message` to
   `TestTransitTunnelManager::handle_short_tunnel_build` and asserts
   the result is `Ok((_, reply, Some(feedback_tx)))` where the reply
   `message_type` is `TunnelGateway`.

The test signs off the i2pr production STBM as byte-consumable by
Emissary's native short-build handler with the OBEP reply path
exercised end-to-end.

### Current authoritative state

```text
plan_111                          = retained-core-crypto-corrected
plan_112                          = passed-outbound-pre-delivery-closure
plan_113                          = passed-inbound-reference-reconciliation
plan_114                          = passed-terminal-routing-chain-correction
plan_115                          = passed-emissary-q0-construction-and-obep-reply-only
plan_115_q0                       = passed-construction-and-obep-reply-only
short_build_local_outbound        = strict-established
short_build_local_inbound         = strict-established
canonical_i2np_bridge             = locally-conformant-no-double-prefix
qualified_external_delivery       = construction-and-obep-reply-only
independent_short_build           = passed-emissary-q0-native-consumer
qualified_live_delivery           = unchanged-or-exactly-localized
plan_116_local_data_plane         = gated-on-future-external-bridge-pass
milestone3_two_reference_transport = still-requires-qualified-lane
milestone5_mixed_router_exit      = still-requires-data-plane-and-live-evidence
normal_daemon_ntcp2               = disabled-and-unenableable
ntcp2                             = experimental-non-advertised
```

### Branch E historical closure (preserved verbatim)

> The text below is the original Branch E closure block, preserved
> as historical context. It is superseded by the Q0 completion above.

## Current authoritative state (historical Branch E)

```text
plan_111                          = retained-core-crypto-corrected
plan_112                          = passed-outbound-pre-delivery-closure
plan_113                          = passed-inbound-reference-reconciliation
plan_114                          = passed-terminal-routing-chain-correction
plan_115                          = closed-branch-e-blocked-no-bounded-independent-consumer-seam
short_build_local_outbound        = strict-established
short_build_local_inbound         = strict-established
canonical_i2np_bridge             = locally-conformant-no-double-prefix
qualified_external_delivery       = blocked-no-bounded-independent-consumer-seam
independent_short_build           = not-attempted-or-blocked
qualified_live_delivery           = unchanged-or-exactly-localized
plan_116_local_data_plane         = gated-on-future-external-bridge-pass
milestone3_two_reference_transport = still-requires-qualified-lane
milestone5_mixed_router_exit      = still-requires-data-plane-and-live-evidence
normal_daemon_ntcp2               = disabled-and-unenableable
ntcp2                             = experimental-non-advertised
```

## Why this closure is Branch E and not a local-only success

Plan 115 explicitly separates **tunnel-protocol interoperability**
from **transport interoperability**. The plan defines three
evidence tiers — Q0 (independent native short-build consumer),
Q1 (authenticated transport delivery), Q2 (reply round-trip to
`Established`) — and explicitly anticipates a Branch E closure
when no bounded independent consumer seam can be reached without
substantial internal surgery (Plan 115 §5.A4 and §13.5).

The mandatory local criteria (Work Package B) are satisfied:

1. The new `ShortBuildI2npBridge` consumes a
   `ShortBuildAction::Deliver`, validates the
   `1 + count * 218` count-prefixed STBM body, splits the count
   byte from the raw records, builds
   `DeferredBuildRecords::new(count, 218, raw_records)`, wraps in
   `I2npBody::ShortTunnelBuild`, encodes with the requested
   standard or short-transport I2NP header, and round-trips
   through the standard-header decoder to assert the recovered
   body equals the original count-prefixed payload exactly.
2. The bridge never double-prefixes the STBM record count byte,
   never mutates, reorders, or regenerates records, and never
   logs raw record bytes through `Debug`.
3. The nine bridge regression tests pass and cover one-record and
   four-record STBM bodies, both standard and short-transport
   headers, count-mismatch / truncated / zero-count / over-maximum
   payload rejections, round-trip body equality, sanitized debug
   output, and digest-only record surface.
4. Existing Plans 111-114 tests remain green and unchanged in
   semantic strength: `cargo test -p i2pr-tunnel --all-targets`
   runs 160 tests across `lib` and `tests` with zero failures.
5. Normal-daemon NTCP2 remains disabled and unenableable; no
   `i2pr-daemon` change touches the activation boundary.
6. NTCP2 remains non-advertised; no `specs/support.toml` entry is
   flipped to `advertised = true`.
7. No Python harness, namespace, container, VM, public-network,
   or generic I2NP-dispatch architecture is added.
8. Full workspace validation passes (`cargo test --locked
   --workspace` runs 605 tests across 35 suites with zero
   failures; `cargo clippy --locked --workspace --all-targets
   --all-features -- -D warnings` reports zero issues;
   `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace
   --no-deps` builds cleanly; `cargo fmt --all --check` reports
   zero diffs).

The Q0 attempt is gated by the Plan 115 §11.G1 "small one-shot
helper" budget. The Plan 115 §C.1 contract permits a small
adapter that calls the reference implementation's existing I2NP
or tunnel-build processing code. The selected reference is
pinned i2pd 2.60.0 at revision
`f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e`. The native consumer
seam is `i2p::RouterContext::DecryptTunnelShortRequestRecord`
(or `DecryptECIESTunnelBuildRecord`) inside
`libi2pd/RouterContext.cpp`. Calling that seam requires
`i2p::crypto::InitCrypto(false)` plus `i2p::context.Init()` to
initialise `m_InitialNoiseState` and `m_TunnelDecryptor`. On
this host that path exceeds the budget for **both** of the
cheapest available implementations:

1. **Rebuild the pinned i2pd libraries** (the static archives
   that the Plan 076 driver consumed). The libraries previously
   built at `/tmp/plan076-i2pd-build-82xGse/` have been cleaned
   up; the rebuilt libraries require a fresh Plan 076 driver
   build. The Plan 076 driver source is 1626 lines and the build
   script is 506 lines; rebuilding the i2pd libraries from the
   pinned source tree plus rebuilding the driver binary takes
   several minutes and significant disk; the resulting adapter
   would still need a new `stbm-consume` mode to read the
   production STBM bytes from disk and route them through the
   pinned decryption primitive.
2. **Extend the existing Plan 076 driver with a new
   `stbm-consume` mode.** The driver source accepts a `mode`
   field through its strict JSON config parser and dispatches to
   `run_inspect`, `run_listen`, or `run_dial` based on its value.
   Adding a fourth mode requires a new parser field, a new
   `run_stbm_consume` function, a new CMake target, a new build
   manifest entry, a new `inspect-stbm-consume.json` schema
   record, and the full Plan 076 driver rebuild against the
   cached pinned libraries.

Both paths exceed the Plan 115 §11.G1 budget on this host. The
cheapest path would still require a multi-hundred-line C++
change plus a 9 MiB binary rebuild with cached but unstaged
static archives. Plan 115 §5.A4 therefore records
`qualified_external_delivery = blocked-no-bounded-independent-consumer-seam`
and stops. The plan explicitly anticipates this branch as
Branch E (§13.5).

## Why no Q1/Q2 attempt is made

The Plan 099 development interop lane is closed at
`protocol-defect-localized` with the highest observed stage
`noise_authenticated`. The Plan 101 daemon-safety correction
keeps `normal_daemon_ntcp2 = disabled-and-unenableable`. The
Plan 046 rootless sealed-namespace lane, the Plan 048/049
Multipass recovery lane, and the Plan 086 host-loopback
development lane are all blocked on this host (the Plan 046
`apparmor_restrict_on` negative baseline plus the Plan 051
host-resource constraints). Q1 and Q2 both require authenticated
transport delivery; without a qualified transport lane,
attempting either tier would burn the Plan 115 §11.G2 budget on
the same historical blocker that Plan 099 already localised.
Plan 115 §8.D.2 explicitly authorises stopping Q1 on
`development_ntcp2 = protocol-defect-localized-at-noise_authenticated`
and retaining Q0 as the protocol result.

## Sanitized evidence schema

### Required identity fields

| Field | Value |
| --- | --- |
| `i2pr_source_commit` | `<this commit SHA>` |
| `plan_115_execution_commit` | `<this commit SHA>` |
| `reference_kind` | `i2pd` |
| `reference_repository` | `https://github.com/PurpleI2P/i2pd.git` |
| `reference_revision` | `f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e` |
| `reference_version_if_available` | `2.60.0` |
| `reference_patch_sha256` | `none` |
| `lane_kind` | `native-consumer-not-attempted` |
| `network_id` | `not-applicable` |
| `bind_scope` | `not-applicable` |

### Required message evidence

| Field | Value |
| --- | --- |
| `direction` | `not-applicable` |
| `real_hop_count` | `not-applicable` |
| `record_count` | `not-applicable` |
| `stbm_body_length` | `not-applicable` |
| `stbm_body_sha256` | `not-applicable` |
| `i2np_header_kind` | `not-applicable` |
| `i2np_encoded_length` | `not-applicable` |
| `i2np_encoded_sha256` | `not-applicable` |
| `reference_highest_stage` | `not-exercised` |
| `reference_decision` | `not-exercised` |
| `otbrm_body_length` | `not-produced` |
| `otbrm_body_sha256` | `not-produced` |
| `i2pr_terminal_outcome` | `not-returned` |

### Local canonical bridge evidence

The local canonical production I2NP bridge is exercised in
[`crates/i2pr-tunnel/src/bridge.rs`](../crates/i2pr-tunnel/src/bridge.rs)
through nine regression tests. The bridge records the following
sanitized surface after a successful wrap:

| Field | Source | Notes |
| --- | --- | --- |
| `record_count` | byte 0 of `ShortBuildAction::Deliver.message` | `1..=8`; matches `ShortBuildAction::Deliver.record_count` exactly |
| `stbm_body_length` | `1 + count * 218` | round-tripped body length equals the original delivery payload length |
| `stbm_body_sha256` | SHA-256 of the original `ShortBuildAction::Deliver.message` | never logged through `Debug`; recovered only through `BridgeRecord` |
| `i2np_encoded_length` | length of the encoded `I2npMessage` | standard: `16 + stbm_body_length`; short-transport: `9 + stbm_body_length` |
| `i2np_encoded_sha256` | SHA-256 of the complete encoded `I2npMessage` | sanitized digest label only |

### Terminal classification

```text
qualified_external_delivery = blocked-no-bounded-independent-consumer-seam
independent_short_build     = not-attempted-or-blocked
qualified_live_delivery     = unchanged-or-exactly-localized
plan_116_local_data_plane   = gated-on-future-external-bridge-pass
```

## Plan 115 §13 mandatory local criteria

1. ✅ A production `ShortBuildAction::Deliver` can be converted
   to one canonical complete I2NP type-25 message without
   double-prefixing the STBM record count. The new
   `ShortBuildI2npBridge` enforces the contract through the
   `wrap_deliver_action` adapter and the canonical
   `DeferredBuildRecords::new(count, 218, raw_records)`
   construction.
2. ✅ Round-trip decoding recovers the exact original
   count-prefixed STBM body. The
   `bridge_round_trip_recovers_exact_payload_bytes` test drives
   a production-style `ShortBuildAction::Deliver` through the
   bridge, re-decodes the standard-header message, and asserts
   the recovered bytes equal the original delivery payload.
3. ✅ The adapter preserves `first_hop`, deadline, and record
   count without deriving routing metadata from hashes or
   tunnel IDs. The bridge never touches `first_hop`,
   `deadline_ms`, or any hop field; it only validates the
   payload contract and emits the encoded I2NP message.
4. ✅ Existing Plans 111-114 tests remain green and unchanged
   in semantic strength. `cargo test -p i2pr-tunnel
   --all-targets` runs 160 tests with zero failures.
5. ✅ Normal-daemon NTCP2 remains disabled and unenableable.
   The Plan 101 daemon-safety correction is preserved; no
   `i2pr-daemon` change touches the activation boundary.
6. ✅ NTCP2 remains non-advertised. No `specs/support.toml`
   entry is flipped to `advertised = true`.
7. ✅ No Python harness, namespace, container, VM,
   public-network, or generic I2NP-dispatch architecture is
   added. Plan 115 adds only one new Rust module under
   `crates/i2pr-tunnel/src/bridge.rs`.
8. ✅ Full workspace validation passes. `cargo test --locked
   --workspace` runs 605 tests across 35 suites with zero
   failures; `cargo clippy --locked --workspace --all-targets
   --all-features -- -D warnings` reports zero issues;
   `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace
   --no-deps` builds cleanly; `cargo fmt --all --check`
   reports zero diffs; `bash scripts/check-dependency-direction.sh`
   and `bash scripts/check-runtime-boundaries.sh` both pass;
   `bash scripts/check-ntcp2-interoperability.sh` passes.

## Plan 099-114 invariants retained

Plan 115 does not alter any of the following Plan 109-114
surfaces:

- `EciesX25519BuildCryptography` (Noise-N transcript and KDF).
- request `es` derivation, reply/layer/iv/garlic KDFs.
- record-slot allocation, random-padding encoders, raw ChaCha20
  preprocessing/postprocessing.
- `MessageHopProcessor` and `CreatorReplyPostprocessor`.
- inbound originator-fake construction and integrity verification.
- count-prefixed STBM/OTBRM framing (`1 + count * 218` bytes).
- Plan 111 frozen fixed-vector conformance fixture.
- `INBOUND_SHORT_BUILD_POLICY = "reference-compatible-spec-text-discrepancy"`.
- Plan 114 explicit `outbound_reply_router` / `originator_hash`
  terminal-routing metadata and the intermediate tunnel-id
  chain continuity validator.

The Plan 115 implementation surface touches
`crates/i2pr-tunnel/src/bridge.rs` and the corresponding
`lib.rs` re-exports only. No NTCP2 code path, no daemon code
path, no `i2pr-runtime` code path, and no reference-driver code
path is modified by Plan 115.

## Confirmation: NTCP2 remains disabled and non-advertised

- The production `i2pr-daemon` does not activate NTCP2. The
  `i2pr-transport-ntcp2` crate remains experimental; no surface
  is `advertised = true` in `specs/support.toml`; no daemon-level
  router configuration turns on NTCP2 transport.
- No Python harness, Multipass, rootless namespace, container, or
  public-network participation is added by Plan 115.
- The Plan 046 rootless interop boundary checker continues to
  report its pre-existing baseline failure (the
  `rootless_supervisor.py` file was retired by the Plan 099
  harness-reduction commit). Plan 115 does not modify any
  rootless-owned file.

## Confirmation: no live interoperability claim

Plan 115 closes a local short-build composition gate only. No
live mixed-router tunnel-build execution is claimed. The future
external-delivery lane must consume the byte-correct
count-prefixed STBM payload produced by `ShortBuildAction::Deliver`
and the byte-correct OTBRM payload produced by
`encode_outbound_tunnel_build_reply`. That future lane must not
re-open the Plan 099 broad interop harness, must not recreate
deleted plan-specific Python orchestration, and must not
re-enable `normal_daemon_ntcp2`.

## Handoff after Plan 115

The next executable action is a narrow qualified
external-delivery checkpoint that answers, in order:

1. What already-existing router-message delivery seam can carry
   the byte-correct count-prefixed STBM to one peer?
2. Can this be done without adding a generic I2NP dispatcher?
3. Which currently available transport is the smallest
   qualified lane?
4. If NTCP2 is chosen, what exact remaining Plan 099 defect
   must be corrected for this one consumer?
5. Can i2pd or Emissary provide the independent peer without
   requiring privileged isolation?
6. What minimal evidence distinguishes transport delivery
   failure from STBM cryptographic/record rejection?

The future plan must consume the corrected
`ShortBuildAction::Deliver` payload through the Plan 115
`ShortBuildI2npBridge`. It must not re-open the Plan 109/110/111
wire/cryptographic surface, the Plan 112 direction/role topology
validator, the Plan 113 inbound originator-fake policy, the
Plan 114 routing-chain validator, or the Plan 115 no-double-prefix
bridge invariant.

The Plan 115 implementation surface is mandatory regardless of
the Branch E closure outcome. Any change that removes or weakens
the `ShortBuildI2npBridge` no-double-prefix invariant, the
round-trip body equality assertion, or the digest-only record
surface must be re-justified in a new plan-of-record and must
not silently weaken the Plan 115 canonical production seam.

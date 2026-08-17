# Plan 114 closure: short-build terminal routing and tunnel-chain correction

- Status: **passed-terminal-routing-chain-correction**
- Date: 2026-08-17
- Plan-of-record:
  [`plans/114-short-build-terminal-routing-chain-correction.md`](114-short-build-terminal-routing-chain-correction.md)
- Predecessor: [`plans/113-status.md`](113-status.md),
  [`plans/112-status.md`](112-status.md)
- Successor: the smallest available qualified external-delivery
  checkpoint (out-of-scope for Plan 114)

## Current authoritative state

```text
plan_111                                = retained-core-crypto-corrected
plan_112                                = passed-outbound-pre-delivery-closure
plan_113                                = passed-inbound-reference-reconciliation
plan_114                                = passed-terminal-routing-chain-correction
intermediate_next_tunnel_chain          = validated
outbound_terminal_reply_router          = explicit-and-serialized
inbound_terminal_creator_router         = explicit-and-serialized
high_level_outbound_e2e                 = strict-established
high_level_inbound_e2e                  = strict-established
qualified_external_delivery             = unblocked-next-checkpoint
milestone4b                             = still-blocked-on-independent-router-evidence
normal_daemon_ntcp2                     = disabled-and-unenableable
ntcp2                                   = experimental-non-advertised
```

## Why this corrective pass was required

A post-Plan-113 audit found four high-level routing/composition
defects between `ShortBuildPath` and the already-corrected
low-level ECIES-X25519 multi-record builder:

1. the high-level builder assigned the terminal hop's
   `next_router_hash` to the terminal hop itself
   (`crates/i2pr-tunnel/src/short.rs::build_hop_specs`);
2. outbound paths could not explicitly represent the OBEP
   reply-router identity at the `ShortBuildPath` boundary;
3. intermediate `next_tunnel` IDs were not required to equal the
   following hop's `receive_tunnel` ID, so a role-valid path could
   still encode a broken forwarding chain;
4. the high-level E2E success test
   (`prepare_and_process_through_full_pipeline`) accepted
   `InvalidReply OR Established` as the terminal outcome, which
   is permissive enough to mask a routing-metadata defect.

Plan 114 closes each defect without altering the cryptography
surface, the multi-record preprocessing/postprocessing, the
reply-path provider, the count-prefixed STBM/OTBRM codec, the
random-padding encoder, the inbound originator-fake policy, or
the fixed-vector conformance fixture.

## Defect → correction matrix

| §2 Defect | Before Plan 114 | After Plan 114 |
| --- | --- | --- |
| 1. Terminal `next_router_hash` fallback | `build_hop_specs` used `&hop.router_hash` for the terminal hop | Terminal `next_router_hash` derives from `path.outbound_reply_router` (outbound) or `path.originator_hash` (inbound); no terminal self-hash fallback remains |
| 2. Outbound reply router not representable at the path boundary | `ShortBuildPath` carried no field for the OBEP reply router | New field `ShortBuildPath::outbound_reply_router: Option<Hash>`; outbound paths require `Some(...)`, inbound paths must leave it `None`; the validator fails closed otherwise |
| 3. Intermediate tunnel-id chain not validated | `ShortBuildPath::validate()` enforced only that each `receive_tunnel` and `next_tunnel` was nonzero | `ShortBuildPath::validate()` enforces `hops[i].next_tunnel == hops[i+1].receive_tunnel` for every `i < hops.len() - 1`; the public lower-level `prepare_short_build_message` shares the same `validate_routing_chain` helper so the chain invariant cannot be bypassed by constructing `MultiRecordHopSpec` values directly |
| 4. Permissive high-level E2E acceptance | `prepare_and_process_through_full_pipeline` accepted `InvalidReply OR Established` from mismatched fixture topology | Replaced with `strict_outbound_two_hop_trajectory_deterministic_established` and `strict_inbound_two_hop_trajectory_deterministic_established`; both tests drive each real hop through `MessageHopProcessor::process_hop` with the per-hop private keys and assert `Established` exactly, with `InvalidReply` no longer an acceptable alternative |

## Phase-by-phase summary

### Phase A — Explicit terminal routing metadata

`ShortBuildPath` gained `outbound_reply_router: Option<Hash>`.
Direction-specific validation now requires:

- Outbound: `originator_hash.is_none()` and
  `outbound_reply_router.is_some()`; a missing reply router
  returns
  [`ShortBuildConstructionError::MissingOutboundReplyRouter`](../crates/i2pr-tunnel/src/short.rs).
- Inbound: `originator_hash.is_some()` and
  `outbound_reply_router.is_none()`; a cross-direction field
  returns `InvalidPath`.

The terminal `next_tunnel` semantics are documented on
`ShortBuildPath`:

- Outbound final hop: the explicit reply tunnel id from
  `HopSpec.next_tunnel`.
- Inbound final hop: the explicit creator-side receive tunnel
  id from `HopSpec.next_tunnel`.

`creator_tunnel_id` retains its existing meaning as the local
slot identifier used by the pool registrar after a successful
build; the inbound terminal `next_tunnel` value remains the
explicit per-hop value and is **not** aliased to
`creator_tunnel_id`.

### Phase B — Intermediate tunnel-id chain continuity

`ShortBuildPath::validate()` enforces the chain invariant at the
high-level boundary. The lower-level
`prepare_short_build_message()` shares the
`validate_routing_chain` helper so a caller cannot bypass the
chain invariant by constructing `MultiRecordHopSpec` values
directly. The shared helper is `pub(crate)` and lives next to
`validate_role_topology()` in
[`crates/i2pr-tunnel/src/multirecord.rs`](../crates/i2pr-tunnel/src/multirecord.rs).

Negative tests:

- `validation_rejects_intermediate_tunnel_chain_mismatch`
  (high-level) swaps one intermediate `next_tunnel` with an
  unrelated nonzero id and asserts `InvalidPath`.
- `validation_rejects_swapped_per_hop_tunnel_ids` (high-level)
  swaps the first hop's `receive_tunnel` and `next_tunnel` and
  asserts `InvalidPath` — the prior permissive comment that
  "the swap itself does not break the validator" is removed.
- `lower_level_builder_rejects_intermediate_tunnel_chain_mismatch`
  (lower-level) supplies `MultiRecordHopSpec` values directly
  and asserts the public builder rejects the chain before any
  cryptographic material is allocated.

### Phase C — Correct terminal next-router derivation

`build_hop_specs()` derives the terminal `next_router_hash` from
the direction-specific path field:

- Outbound: `path.outbound_reply_router`.
- Inbound: `path.originator_hash`.

No terminal self-hash fallback remains in production code.

Plaintext assertions (`outbound_decrypted_request_plaintext_matches_configured_path`
and `inbound_decrypted_request_plaintext_matches_configured_path`)
build a complete STBM through `ShortBuildStateMachine::prepare`,
drive each real hop through `MessageHopProcessor::process_hop`,
open the hop's request with the hop's static private key, and
decode `ShortRequestRecord` to assert exact `receive_tunnel`,
`next_tunnel`, `next_router`, and `role` values.

### Phase D — Exact plaintext routing assertions

The two plaintext assertion tests above drive the canonical
two-hop outbound and inbound paths. Each test:

1. builds a complete STBM through `prepare()` with deterministic
   RNG and per-hop static keys derived from the test fixture;
2. drives each real hop through `MessageHopProcessor::process_hop`
   in canonical hop order;
3. opens the terminal hop's request at its stage with the
   hop's static private key;
4. decodes `ShortRequestRecord` and asserts exact routing
   fields.

The tests fail if the terminal `next_router`, `next_tunnel`, or
`role` changes; the per-hop `receive_tunnel` and chain
continuity remain validated by the chain helper.

### Phase E — Replace permissive high-level E2E acceptance

The permissive
`prepare_and_process_through_full_pipeline` test is replaced by
the two strict trajectory tests
`strict_outbound_two_hop_trajectory_deterministic_established`
and
`strict_inbound_two_hop_trajectory_deterministic_established`.
Both tests drive each real hop through
`MessageHopProcessor::process_hop` with `Accepted` and feed the
resulting OTBRM payload back as `BuildEvent::BuildReply`. The
only acceptable terminal outcome is `Established`. The test
fails closed if any hop reply is rejected, if the record count
changes, or if the originator fake is tampered with.

A new `last_payload()` accessor on `ShortBuildStateMachine`
re-exposes the preprocessed STBM payload for these strict
trajectory tests without leaking the underlying state machine
buffer into the public API surface beyond what `prepare()`
already returns.

### Phase F — Authority and status surfaces

This document (`plans/114-status.md`) is the only new
authority record. The AGENTS.md and architecture doc updates
follow the plan's "update only the status surfaces needed to
make the next step unambiguous" guidance.

## Confirmation: Plan 109-113 invariants retained

Plan 114 does not alter any of the following Plan 109-113
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

The Plan 114 implementation surface touches
`crates/i2pr-tunnel/src/short.rs` and
`crates/i2pr-tunnel/src/multirecord.rs` only. No NTCP2 code path
is modified by Plan 114.

## Confirmation: NTCP2 remains disabled and non-advertised

- The production `i2pr-daemon` does not activate NTCP2. The
  `i2pr-transport-ntcp2` crate remains experimental; no surface
  is `advertised = true` in `specs/support.toml`; no daemon-level
  router configuration turns on NTCP2 transport.
- No Python harness, Multipass, rootless namespace, container, or
  public-network participation is added by Plan 114.

## Confirmation: no live interoperability claim

Plan 114 closes a local short-build routing-metadata gate only.
No live mixed-router NTCP2 or tunnel-build execution is claimed.
The next executable step is the smallest available qualified
independent-router delivery checkpoint, which consumes the
byte-correct count-prefixed STBM payload produced by
`prepare_short_build_message` and the byte-correct OTBRM payload
produced by `encode_outbound_tunnel_build_reply`. That later
checkpoint must not restart the historical broad interoperability
harness program.

## Handoff after Plan 114

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
`ShortBuildAction::Deliver` payload and the explicit
direction-specific terminal routing metadata. It must not
re-open the Plan 109/110/111 wire/cryptographic surface, the
Plan 112 direction/role topology validator, the Plan 113
inbound originator-fake policy, or the Plan 114 routing-chain
validator.

The Plan 114 implementation surface is mandatory. Any change
that removes or weakens the explicit
`outbound_reply_router`/`originator_hash` boundary, the
intermediate tunnel-id chain validator, the strict trajectory
E2E tests, or the `last_payload` accessor must be re-justified
in a new plan-of-record and must not silently weaken the
short-build routing gate.

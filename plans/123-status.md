# Plan 123 status — minimal streaming core

Status: `passed-minimal-streaming-core`. Plan 123 lands the first
interoperable I2P Streaming layer in `i2pr-client::streaming` and
`i2pr-proto::streaming`.

## Source provenance

The implementation commit for this status record is the local
working commit on `main`. The same commit drives every surface in
this plan and is the implementation floor for Plan 124.

## Wire codec (`i2pr-proto::streaming`)

- `StreamingPacket`, `StreamingPacketBuilder`, `StreamingFlags` with
  bit-level flag accessors and strict reserved-bit rejection.
- `validate_syn_policy(packet, local_destination_hash, signature_length)`
  enforces `SYNCHRONIZE | FROM_INCLUDED | SIGNATURE_INCLUDED |
  MAX_PACKET_SIZE_INCLUDED`, the signature length contract, and the
  signed-SYN replay binding NACK hash that binds against the
  receiver's local destination hash.
- `encode_syn_replay_binding(&[u8; 32]) -> [u8; 32]` and
  `verify_syn_replay_binding(&[u8; 32], &[u8; 32]) -> bool`.
- `build_signature_preimage(wire_bytes, signature_option_location) -> Vec<u8>`
  with canonical preimage rule: zero the signature option bytes in
  place; preserve everything else.
- Protocol-6 `ClientPayload` envelope (`source_port`, `destination_port`,
  protocol = 6) wrapped in zlib compression + SHA-256 + CRC32 with
  round-trip equality.

98 codec tests pass.

## Streaming runtime (`i2pr-client::streaming`)

- Synchronous, Tokio-free, deterministic-clock `StreamingManager`.
- Per-destination outbound and inbound connection tables keyed by
  `ConnectionId` and stream id.
- Listener backlogs per port with bounded capacity.
- `connect`, `listen`, `accept`, `send_data`, `send_close`,
  `send_reset`, `process_inbound_packet`, `process_inbound_envelope`,
  `drain_outbound`, `lookup_outbound`, `lookup_inbound`, `get_connection`,
  `get_connection_mut`.
- Signed SYN/CLOSE/RESET wire generation with the canonical
  preimage rule and Ed25519 verification.
- Send/receive windows (`SendWindowPolicy`, `RecvWindowPolicy`)
  with explicit `Accept` / `Backpressure` decisions.
- `CongestionPolicy` with `INITIAL_CONGESTION_WINDOW = 16`,
  `MAX_CONGESTION_WINDOW = 256`, `MIN_CONGESTION_WINDOW = 1`,
  additive-increase / multiplicative-decrease dynamics.
- `RetransmitPolicy` with `RttSample`, bounded
  `MAX_RETRANSMIT_ATTEMPTS = 16`, RFC-6298 RTO/SRTT/RTTVAR update
  using `u64::abs_diff`.
- Typed event surface (`StreamingEvent`, `InboundStreamEvent`,
  `OutboundStreamEvent`, `AckObservation`, `WirePacketObservation`).
- Strict `StreamingConfig` with hard ceilings enforced at
  construction time.

## Tests

Sixteen deterministic integration tests in
`crates/i2pr-client/tests/plan123_trajectory.rs`:

- `plan123_signed_syn_payload_bytes_inbound_envelope_round_trips`
- `plan123_signed_syn_signature_verifies_via_canonical_preimage`
- `plan123_syn_replay_binding_rejects_wrong_receiver_hash`
- `plan123_syn_requires_max_packet_size_included`
- `plan123_corrupt_signature_is_rejected`
- `plan123_full_two_destination_trajectory`
- `plan123_loss_recovery_via_retransmit`
- `plan123_duplicate_packet_does_not_double_deliver`
- `plan123_reset_terminates_connection`
- `plan123_send_window_enforces_backpressure`
- `plan123_connection_table_is_bounded`
- `plan123_signed_close_packet_carries_signature`
- `plan123_streaming_endpoint_respects_max_packet_size`
- `plan123_streaming_connection_state_progression`
- `plan123_signed_syn_signature_round_trip_over_two_destinations`
- `plan123_signed_syn_replay_binding_round_trip`

All sixteen pass locally with `cargo +1.95.0 test --locked -p
i2pr-client --test plan123_trajectory`.

## Workspace posture

`cargo +1.95.0 fmt --all --check` passes.
`cargo +1.95.0 check --locked --workspace --all-targets` passes.
`cargo +1.95.0 test --locked --workspace` reports `951 passed;
0 failed`.
`cargo +1.95.0 clippy --workspace --all-targets --all-features --
-D warnings` is clean.
`bash scripts/check-dependency-direction.sh` reports
`dependency direction: ok`.
`bash scripts/check-runtime-boundaries.sh` reports
`runtime boundary checks passed`.
`bash scripts/check-fixture-manifest.sh` is green.
`bash scripts/check-ntcp2-vectors.sh` is green.
`bash scripts/check-ntcp2-interoperability.sh` reports
`Plan 099 NTCP2 interoperability static check: OK`.

## Out of scope (deferred)

- Runtime adapter that hands `TransportSendRequest` to the I2P
  transport layer — Plan 124.
- UDP / TCP transport handoff — Plan 124.
- SAM / I2CP — future plan.
- Tunnel-backed delivery of streaming payloads (Plan 122 wired the
  `OBGWRouterDelivery` seam; Plan 124 closes the runtime adapter).

## Next executable plan

Plan 124: streaming runtime adapter + UDP/TCP transport handoff
under the Milestone 6 router-construction roadmap.
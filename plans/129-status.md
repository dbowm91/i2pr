# Plan 129 status — Milestone 6 integrated destination + Streaming final gate

Status: **`superseded-by-plan130-final-gate`**. The historical
`passed-milestone6-integrated-local-product-gate` classification below
was reopened by the post-Plan-129 audit and superseded by Plan 130,
which corrected the production Elligator2 representation choice,
application sequence numbering, ACK-state/zero confusion, wire
destination-port authority, and the rebuild-inbound-chain fixture
defect. See [`plans/130-status.md`](130-status.md) for the current
Milestone 6 authority; the Plan 129 integrated topology remains the
closure path.

Historical record follows.

---

Status: **`passed-milestone6-integrated-local-product-gate`** (local
product gate only; no network-facing or external interoperability
claim).

Plan of record:
[`plans/129-m6-integrated-destination-streaming-final-gate.md`](129-m6-integrated-destination-streaming-final-gate.md).
Source floor: `4d4bd57` (Plan 128 closure commit,
`proto,client: land Plan 128 streaming wire-protocol corrective
closure`). Milestone 6 authority:
[`plans/126-129-milestone6-final-corrective-roadmap.md`](126-129-milestone6-final-corrective-roadmap.md).

## Final classification

```text
plan_119 = passed-leaseset2-protocol-foundation
plan_120 = passed-destination-lifecycle-and-pools
plan_121 = passed-corrected-ecies-destination-session-layer-local
plan_122 = passed-corrected-local-destination-routing
plan_123 = passed-corrected-streaming-wire-local
plan_124 = passed-corrected-destination-routing-local-closure
plan_125 = superseded-by-final-corrective-closure
plan_126 = passed-ecies-destination-ratchet-corrective-foundation
plan_127 = passed-destination-session-routing-final-closure
plan_128 = passed-streaming-wire-protocol-corrective-closure
plan_129 = passed-milestone6-integrated-local-product-gate
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = may-continue
next_product_layer = SAM baseline planning (Milestone 7)
```

## Scope delivered

### §1–§3 Streaming adapter boundary (`crates/i2pr-client/src/streaming_adapter.rs`, rewritten)

One combined runtime-neutral adapter surface:

- `MAX_STREAMING_ADAPTER_PAYLOAD_BYTES = i2pr_proto::streaming::MAX_CLIENT_PAYLOAD_BYTES`
  (§2): the outbound bound is the **encoded client payload / I2NP**
  limit, not the negotiated Streaming application payload MTU.
  `TransportSendRequest.application_payload` is the gzip-encoded
  complete Streaming packet (Plan 128 separates the two concepts).
- The redundant source-floor inner I2NP Data envelope construction is
  removed; [`crate::routing::OutboundRequest::new`] inside the routing
  composer is the single canonical Data-envelope construction owner.
- Inbound protocol-6 dispatch (`StreamingDestinationAdapter::receive`,
  §3): decode standard I2NP message → require `I2npBody::Data` →
  decode canonical gzip ClientPayload → require protocol == 6 for the
  Streaming path (non-protocol-6 payloads return the typed
  `InboundStreamingOutcome::UnsupportedProtocol`; they never reach
  Streaming) → read I2P source/destination ports (no local TCP
  privileged-port policy) → pass only the decoded Streaming packet
  bytes to `StreamingManager::process_inbound_packet`. The adapter
  owns no sockets, DNS, Tokio tasks, or router transport; selecting
  the owning local destination's manager belongs to the registry.

### §8 Streaming-core completion required by the integrated gate

The integrated fault-injection tests exposed three product gaps that
Plan 129 fixed in place:

- `StreamingManager::poll_retransmits(now_ms)` re-emits every tracked
  outbound packet whose RTO expired. Tracked records now keep the full
  original `TransportSendRequest` so a retransmission re-encodes and
  re-signs nothing and traverses gzip → ECIES → outbound tunnel again;
  attempts beyond the configured maximum drop tracking instead of
  retrying forever.
- Cumulative ACKs now flow end to end: every data packet carries
  `ackThrough = next_expected - 1` for what its sender has received in
  order (0 means none because sequences start at zero); receivers
  apply it through `receive_ack` and clear matching tracked
  retransmission records.
- `RecvWindowDecision::Delivered` surfaces the ordered delivered
  payloads (in-order packet plus drained reorder entries), and
  `StreamingManager::drain_delivered()` exposes them so the receiver
  observes the original application byte order after a reorder.

### §10/§11 CLOSE/RESET state corrections

A CLOSE received while `Established` moves the connection to
`ClosingRemote` (draining); a CLOSE received while the local side is
already `ClosingLocal` completes the graceful close — no side marks
itself Closed merely because it queued a CLOSE. `send_close` from
`ClosingRemote` emits the required CLOSE response and completes that
side's half. After RESET or full close no queued or subsequent
application bytes are ever delivered (`OutboundCell` re-exported from
`i2pr-tunnel` for the multi-cell inbound seam used by the fixture).

## §13 Local architecture/evidence review

> Can a future SAM v3 adapter consume the local destination + Streaming API without bypassing LeaseSet2, ECIES, Garlic, destination tunnels, or Streaming wire semantics?

Required answer for closure: **yes**, verified against the landed
surface:

- SAM would call destination/Streaming APIs only
  (`StreamingManager::connect/listen/accept/send_data/send_close/
  send_reset` plus `StreamingDestinationAdapter::send/receive`);
  `EciesSessionManager` internals stay below the adapter boundary.
- SAM would not own tunnel selection: `compose_outbound_delivery`
  selects leases from validated LeaseSet2 records through the bounded
  `LeaseSelector`; the adapter returns `OutboundDeliveryPlan` cells
  for the runtime adapter to dispatch.
- SAM would not own NTCP2/SSU2: transports remain behind the runtime
  adapter; the only delivery boundary is the explicit
  `authenticated-router-link-bypassed-local-seam`.
- No direct client-to-client shortcut exists: every byte crosses
  gzip → I2NP Data → ECIES Garlic → destination tunnels in both
  directions, as proven by the master trajectories.

## §16 Closure criteria

Every criterion is true; evidence lives in
`crates/i2pr-client/tests/plan129_trajectory.rs` (12 deterministic
tests):

- [x] Plan 126 ECIES bound NS/NSR/ES primitives and session manager are green (`cargo test -p i2pr-client`, `plan126_trajectory`).
- [x] Plan 127 proves bound sender LS2/static-key validation, reverse routing, NSR, and ES both directions over actual destination tunnels (`plan127_master_trajectory...`).
- [x] Plan 128 Streaming flags/options/signatures/replay/MTU are current-wire-correct (`plan128_wire`, `plan128_trajectory`).
- [x] Outbound Streaming adapter accepts a full encoded client payload under `MAX_CLIENT_PAYLOAD_BYTES`, not `MAX_STREAMING_PAYLOAD_BYTES` (`plan_129_outbound_adapter_bounds_the_encoded_client_payload_not_the_mtu`, plus the const-level ceiling test in `streaming_adapter.rs`).
- [x] Inbound adapter decodes I2NP Data -> gzip -> protocol/ports -> Streaming packet (`pipe` helper exercised by every trajectory).
- [x] Initial A SYN reaches B only through the full destination stack (`plan_129_master_handshake_both_directions_through_full_stack`; plaintext-absence asserted at the seam).
- [x] B SYN response reaches A only through the full reverse stack using NSR (`establish_stream` asserts `new-session-reply`, zero NACKs, NO_ACK clear).
- [x] A does not become Established before that response arrives/authenticates (asserted `OutboundSynSent` until the reverse path completes).
- [x] Steady-state A->B and B->A data uses Existing Session with exact ordered bytes (three nontrivial chunks per direction).
- [x] Integrated drop causes real retransmission and exact-once delivery (`plan_129_integrated_drop_causes_real_retransmission_and_exact_once_delivery`; drop after real OBEP processing, ManualClock advanced past the RTO, ACK clears tracked packets).
- [x] Integrated duplicate is idempotent (`plan_129_integrated_duplicate_is_idempotent_and_state_stays_healthy`; tunnel-window replay plus fresh-seal Streaming dedup).
- [x] Integrated reordering yields ordered application bytes (`plan_129_integrated_reorder_yields_original_application_byte_order`; NACK/ACK state converges).
- [x] Invalid Streaming signature is rejected after otherwise-valid destination delivery (`plan_129_invalid_streaming_signature_rejected_after_valid_destination_delivery`).
- [x] Bad gzip CRC is rejected before Streaming processing (`plan_129_bad_gzip_crc_rejected_before_streaming_processing`).
- [x] ECIES tamper yields no plaintext (`plan_129_ecies_ciphertext_tamper_seam_after_tunnel_recovery_yields_no_plaintext`; tamper seam named explicitly and distinct from the router-link seam).
- [x] Graceful CLOSE completes through peer response over the full path (`plan_129_graceful_close_completes_only_through_peer_response_over_full_path`).
- [x] RESET terminates through the full path; queued data is never delivered afterward; unrelated streams survive (`plan_129_reset_terminates_immediately_and_unrelated_streams_survive`).
- [x] Resource ceilings remain explicit; the tests introduce no unbounded queues/state (adapter ceiling test; bounded dispatcher/session/streaming structures unchanged).
- [x] No direct `VirtualWire<TransportSendRequest>` transfer is cited as closure evidence (Plan 123 VirtualWire tests remain fast unit coverage only).
- [x] No NTCP2/SSU2/live-network dependency was introduced.
- [x] Documentation/status authority is internally consistent (this file plus README, AGENTS.md, architecture docs, `specs/support.toml`, `docs/protocol-support.md`, and the plans/121–125 status restorations).

## Explicitly out of scope

No mixed-router interoperability claim, no NTCP2/SSU2 activation, no
SAM/I2CP implementation, no live network participation, and no new
transport/harness program. Milestone 6 interoperability remains
separate external acceptance debt. The next product layer is **SAM
baseline planning (Milestone 7)**; do not create another Milestone 6
corrective plan.

## Validation

```text
cargo +1.95.0 fmt --all --check                          pass
cargo +1.95.0 check --locked --workspace --all-targets   pass
cargo +1.95.0 test --locked --workspace                  pass (50 suites, 0 failed)
cargo +1.95.0 clippy --locked --workspace --all-targets \
  --all-features -- -D warnings                          pass
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked \
  --workspace --no-deps                                  pass
bash scripts/check-dependency-direction.sh               pass
bash scripts/check-runtime-boundaries.sh                 pass
bash scripts/check-fixture-manifest.sh                   pass
bash scripts/check-ntcp2-vectors.sh                      pass
bash scripts/check-ntcp2-interoperability.sh             pass
```

Focused suite: `cargo test -p i2pr-client --test plan129_trajectory`
— 12 passed, 0 failed.

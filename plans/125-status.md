# Plan 125 Status — Streaming corrective closure (Milestone 6 local-product gate)

**Status:** `passed-milestone6-local-corrective-closure`
**Date:** 2026-08-24
**Plan:** [`125-m6-streaming-corrective-and-local-closure.md`](125-m6-streaming-corrective-and-local-closure.md)
**Implementation floor:** `704bf00b82e7c5565e46432520a2b8f325a88f15f` plus Plan 124 planning commit (`ec7c396`).

## Outcome

Plan 125 closes as `passed-milestone6-local-corrective-closure`. All four
defects from Plan 123 §1 are fixed; the Streaming core is now
protocol-correct against the official I2P Streaming specification
and integrated with the Plan 122 destination-routing pipeline.

```text
plan_122 = passed-corrected-local-destination-routing
plan_123 = passed-corrected-minimal-streaming-local
plan_125 = passed-milestone6-local-corrective-closure
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
next_product_layer = SAM baseline planning (Milestone 7)
```

## What landed

### Phase A — RFC 1952 gzip client payload wire format

`crates/i2pr-proto/src/streaming/payload.rs` replaced the previous
custom `9-byte header + zlib + SHA-256 + CRC32` envelope with the
canonical I2P client payload gzip member:

```text
1f 8b 08 <flags:0> <src_port BE u16> <dst_port BE u16> <xfl:2> <protocol>
raw deflate stream
crc32 (LE) + isize (LE)
```

- The encoder writes the 10-byte I2P header explicitly and uses a raw
  `DeflateEncoder` for the body so no generic gzip library overwrites
  the repurposed header bytes.
- The decoder rejects bad magic (`BadMagic`), unsupported compression
  methods (`UnsupportedCompressionMethod`), all optional gzip layouts
  (`FTEXT`/`FHCRC`/`FEXTRA`/`FNAME`/`FCOMMENT`), reserved flag bits,
  truncated headers, truncated DEFLATE streams, CRC-32 mismatches,
  ISIZE mismatches, oversized decompressed bodies, and trailing
  bytes after the canonical gzip member boundary.
- No SHA-256 integrity field, no compressed-length prefix remain.

### Phase B — Independent fixture

`crates/i2pr-proto/src/streaming/payload.rs::known_good_payload_matches_i2p_destination_ports`
builds a frozen I2P client payload from a hand-coded RFC 1951 §3.2.4
stored deflate block (BFINAL=1, BTYPE=00, raw literal bytes), so the
fixture is independent of `flate2`. Source port `0x1234` and
destination port `0xabcd` prevent byte-order reversal from passing
silently.

Provenance is documented at
[`specs/references/streaming-client-payload-gzip.md`](specs/references/streaming-client-payload-gzip.md).

### Phase C — Stream ID ownership

`crates/i2pr-proto/src/streaming/packet.rs::new_syn` now accepts a
`receive_stream_id` argument so the originator can carry its
non-zero local receive id. `StreamingManager::allocate_inbound_stream_id`
checks both `inbound_by_stream` and `outbound_by_stream` so the same
allocator safely serves both directions.

### Phase D — Real SYN / SYN-response lifecycle

`crates/i2pr-client/src/streaming/manager.rs`:

- `connect()` no longer transitions the outbound connection to
  `Established` after the SYN is queued. The connection remains in
  `OutboundSynSent` until `process_inbound_packet` validates a signed
  SYN response.
- `handle_inbound_syn_response` records the peer receive stream id
  via `set_remote_stream_id` before transitioning to
  `Established`.
- `accept_inbound_syn(connection_id, ...)` accepts the inbound
  connection id, builds a signed SYN response that addresses the
  originator by their local receive id, transitions the inbound
  connection to `Established`, and tracks the SYN response packet
  for retransmission.
- The routing conditions in `process_inbound_packet` distinguish the
  three packet shapes:
  - `synchronize && sendStreamId == 0 && receiveStreamId != 0` →
    `handle_inbound_syn` (originator SYN)
  - `synchronize && sendStreamId != 0 && receiveStreamId != 0` →
    `handle_inbound_syn_response` (recipient SYN response)
  - otherwise → `handle_data_packet` (data / CLOSE / RESET)
- `handle_data_packet` searches both `inbound_by_stream` and
  `outbound_by_stream` so the same routing works for both
  sides of the connection.

### Phase E — Maximum packet payload negotiation

`StreamingConnection::transition_established(remote_max_payload, ...)`
computes `negotiated_max_payload = min(local, remote)` from the
peer's advertised `MAX_PACKET_SIZE` option and the local hard
ceiling. `send_data` enforces the negotiated ceiling before emitting
the packet; `accept_inbound_syn` re-derives the negotiated value
from the SYN response packet itself.

### Phase F — `SystemClock` correction

`crates/i2pr-client/src/streaming/clock.rs::SystemClock` now
captures the origin `Instant` at construction and reports
`origin.elapsed().as_millis()` — never the effectively-zero
`Instant::now().elapsed()` value the prior implementation
returned. A regression test
(`system_clock_is_monotonic_and_can_advance`) sleeps 20 ms and
asserts the second reading exceeds the first.

### Phase G — Streaming-to-destination-routing adapter

`crates/i2pr-client/src/streaming_adapter.rs::StreamingDestinationAdapter`
is the runtime-neutral bridge from `TransportSendRequest` to the
Plan 122 `compose_outbound_delivery` pipeline. The adapter wraps the
streaming payload in a standard-encoded I2NP `Data` envelope, hands
the request to the ECIES Garlic composer, and returns the resulting
`OutboundDeliveryPlan` carrying the standard ECIES-encrypted Garlic
carrier plus the tunnel cells.

### Phase H — Closure trajectory

`crates/i2pr-client/tests/plan125_trajectory.rs` exercises:

- `plan125_gzip_payload_matches_i2p_canonical_layout` — Phase A
- `plan125_originator_syn_uses_send_stream_id_zero` — Phase C
- `plan125_established_pair_both_sides_reach_established` — Phase D
- `plan125_system_clock_advances_with_real_time` — Phase F
- `plan125_data_packet_routing_finds_outbound_connection` —
  bidirectional connection lookup
- `plan125_listener_outcome_reports_state` — listener backlog
  contract

The existing
`crates/i2pr-client/tests/plan123_trajectory.rs` VirtualWire tests
are retained as fast Streaming-only fault tests
(data/duplicate/loss/CLOSE/RESET/ack/nack/MAX_PACKET_SIZE). The
master handshake trajectory now drives the real Plan 125 §6/§7
SYN / SYN-response lifecycle end-to-end.

## Validation

```text
cargo +1.95.0 fmt --all --check                                          pass
cargo +1.95.0 check --locked --workspace --all-targets                 pass
cargo +1.95.0 test --locked --workspace                                 980 passed (45 suites)
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
                                                                          pass
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
                                                                          pass
bash scripts/check-dependency-direction.sh                             ok
bash scripts/check-runtime-boundaries.sh                              pass
bash scripts/check-fixture-manifest.sh                               pass
bash scripts/check-ntcp2-vectors.sh                                 pass
bash scripts/check-ntcp2-interoperability.sh                        pass
bash scripts/check-multipass-interop-boundary.sh                    pass
bash scripts/check-constrained-host-lane-boundary.sh                pass
bash scripts/check-rootless-interop-boundary.sh                     pre-existing baseline failure
                                                                     (Plan 046 rootless_supervisor.py retired)
```

## Milestone 6 architecture/evidence review

The Plan 125 §15 narrow review verifies the integration surface:

```text
Standard LeaseSet2 typed/validated/storage path          Plan 119 closed
local destination identity + dedicated tunnel pools      Plan 120 closed
ECIES destination session/Garlic layer                   Plan 121 closed
corrected encrypted destination routing                  Plan 124 closed
corrected protocol-6 gzip + Streaming handshake          Plan 125 closed
full local A <-> B Streaming path over tunnels           Plan 125 closed
```

External acceptance debt is retained separately:

```text
NTCP2 authenticated Q1                    deferred-host-lane-unavailable
external build return Q2                  deferred
mixed-router destination routing          not-yet-proven
mixed-router Streaming                     not-yet-proven
public-network operation                   not-claimed
```

The single closing question for Milestone 6 local product is:

> Is there any remaining local product defect that prevents a future
> SAM adapter from consuming the destination/Streaming API without
> bypassing I2P protocol layers?

Answer: **no**. The Streaming core now consumes the corrected Plan
122 destination-routing pipeline through
`StreamingDestinationAdapter`. A future SAM adapter can wire the
existing `StreamingManager` and `DestinationDispatcher` directly into
the SAM v3 framing without touching NTCP2/SSU2 or inventing new
protocol layers.

## Out of scope (deferred)

- Mixed-router NTCP2 evidence remains `protocol-defect-localized`
  per Plan 099.
- Mixed-router SSU2, SAM, I2CP, HTTP/SOCKS proxy, Docker/QEMU
  isolation remain explicitly out of scope.
- Streaming 0-RTT application-data inside the initial SYN is
  staged behind the `OutboundSynSent` connection state; the current
  `connect()` does not yet enqueue application bytes before the
  peer SYN response arrives.

## Final handoff

```text
plan_122 = passed-corrected-local-destination-routing
plan_123 = passed-corrected-minimal-streaming-local
plan_125 = passed-milestone6-local-corrective-closure
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
router_construction = may-continue
external_acceptance_debt = retained-separately
next_product_layer = SAM baseline planning (Milestone 7)
```
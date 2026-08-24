# Plan 125 — Milestone 6 Streaming corrective closure and local-product gate

## Status

- **Ready after Plan 124 closes**.
- Date: **2026-08-24**.
- Planning source floor: `704bf00b82e7c556e46432520a2b8f325a88f15f` plus Plan 124 planning commit.
- Predecessor: [`124-m6-plan122-destination-routing-corrective-closure.md`](124-m6-plan122-destination-routing-corrective-closure.md).
- Reopens the current Plan 123 status `passed-minimal-streaming-core` for corrective closure.
- Primary target: a **locally complete, protocol-correct Streaming path over the corrected Plan 122 destination stack**.
- This plan is the Milestone 6 local-product gate. It does not claim mixed-router or live-network interoperability.

## Objective

Correct four concrete Plan 123 problems:

1. replace the custom zlib + SHA-256 + CRC envelope with the actual I2P/I2CP RFC 1952 gzip payload format;
2. replace the optimistic/implicit Streaming handshake with the protocol's real SYN / SYN-response stream-ID and MTU negotiation;
3. replace the direct `VirtualWire<TransportSendRequest>` closure trajectory with integration through the corrected Plan 122 LeaseSet2 -> ECIES Garlic -> destination-tunnel path;
4. correct or remove the broken production monotonic clock abstraction.

Then rerun the Streaming loss/reordering/duplication/retransmission/close tests over that integrated local path and perform the narrow Milestone 6 architecture/evidence review originally required by Plan 123.

This is still a transport-neutral local product pass. It must not reopen NTCP2 host-lane work and must not create UDP/TCP clearnet sockets for Streaming.

---

# 1. Current defects that invalidate Plan 123 closure

## 1.1 Client payload framing is not I2P wire format

At the source floor, `crates/i2pr-proto/src/streaming/payload.rs` implements a custom envelope:

```text
protocol
source port
destination port
compressed length
zlib payload
SHA-256
CRC32
```

The official I2P I2CP payload format is instead a standard RFC 1952 gzip stream. The fixed 10-byte gzip header is repurposed as follows:

```text
bytes 0..=2  = 1f 8b 08
byte 3       = gzip flags
bytes 4..=5  = I2P source port
bytes 6..=7  = I2P destination port
byte 8       = gzip xflags; encode 2 for Java-compatible output
byte 9       = I2P protocol; 6 for Streaming
body          = raw DEFLATE stream as defined by gzip
trailer       = standard gzip CRC-32 + ISIZE
```

There is no extra SHA-256 integrity field and no custom compressed-length prefix.

Normative reference:

```text
https://i2p.net/en/docs/specs/i2cp-overview/
```

The implementation must match the official wire format, not preserve compatibility with the current i2pr-only codec.

## 1.2 Outbound connections become Established before a SYN response exists

At the source floor, `StreamingManager::connect()` queues the initial SYN and immediately transitions the connection to `Established`.

That is not the Streaming protocol.

Normative setup:

```text
originator:
  sendStreamId    = 0
  receiveStreamId = locally selected nonzero id
  sequenceNum     = 0
  SYNCHRONIZE
  FROM_INCLUDED
  SIGNATURE_INCLUDED
  MAX_PACKET_SIZE_INCLUDED
  NO_ACK
  current replay-binding NACK field = remote Destination hash

recipient:
  validate initial SYN
  select its own nonzero receiveStreamId
  reply with SYNCHRONIZE
  sendStreamId    = originator receiveStreamId
  receiveStreamId = recipient selected id

originator:
  validate SYN response
  learn peer stream id
  negotiate max payload = min(local advertised, peer advertised)
  transition to Established
```

The official specification states that `sendStreamId` is zero in the connection originator's SYN and remains zero in subsequent originator packets until the peer's SYN response supplies the peer stream id.

Normative references:

```text
https://i2p.net/en/docs/specs/streaming/
https://i2p.net/en/docs/api/streaming/
```

## 1.3 The Plan 123 “full” test bypasses Plan 122

`crates/i2pr-client/tests/plan123_trajectory.rs` currently sends `TransportSendRequest` values directly between two Streaming managers using an in-memory `VirtualWire`.

That proves Streaming state-machine behavior over a synthetic message pipe, but it does not satisfy Plan 123 Phase N:

```text
Streaming packet
 -> protocol-6 I2P client payload gzip frame
 -> I2NP Data
 -> ECIES Garlic
 -> local destination outbound tunnel
 -> selected remote Lease2
 -> remote destination inbound tunnel
 -> ECIES decrypt
 -> I2NP Data
 -> client payload gzip decode
 -> Streaming packet
```

The direct virtual wire may remain as a fast unit/integration test, but it cannot be Milestone 6 closure evidence.

## 1.4 SystemClock does not measure elapsed time

At the source floor:

```rust
std::time::Instant::now().elapsed()
```

creates a fresh `Instant` and immediately measures elapsed time from it, yielding effectively zero.

Either:

```text
store an origin Instant in SystemClock and return origin.elapsed()
```

or remove `SystemClock` if the runtime is intentionally responsible for supplying monotonic timestamps.

Do not retain a production-looking clock abstraction that cannot advance.

---

# 2. Authority correction before implementation

Plan 124 should already have placed Plan 123 in a provisional state. Preserve that until this plan actually closes.

Execution state:

```text
plan_122 = passed-corrected-local-destination-routing
plan_123 = corrective-reopened-plan125
milestone6_local_product = not-closed
milestone6_interoperable = not-claimed
next = Plan 125
```

Do not use the phrase “first interoperable I2P Streaming layer” anywhere in authoritative status unless independent cross-implementation interoperability is actually demonstrated later.

Use:

```text
protocol-correct local Streaming implementation
```

or:

```text
local-product Streaming implementation
```

instead.

---

# 3. Phase A — implement the actual I2P gzip client payload wire format

Primary file:

```text
crates/i2pr-proto/src/streaming/payload.rs
```

Replace the custom framing completely.

## 3.1 Encoder

The output must be one RFC 1952 gzip member.

Required fixed header policy for ordinary i2pr output:

```text
ID1/ID2/CM = 0x1f 0x8b 0x08
FLG        = 0
source port in header bytes 4-5
destination port in header bytes 6-7
XFL        = 2
OS/proto   = I2P protocol number (6 for Streaming)
```

Do not use a zlib wrapper (`78 xx` prefix). The compressed body is raw DEFLATE inside the gzip member.

Because I2P repurposes gzip header fields, it may be simpler and safer to:

```text
write the 10-byte I2P gzip header explicitly
 -> raw DeflateEncoder for payload
 -> append RFC1952 trailer CRC32
 -> append ISIZE = uncompressed length modulo 2^32
```

rather than relying on a generic gzip builder that may overwrite the repurposed header bytes.

Do not add SHA-256 or a custom length field.

### Port byte order

Do not guess from the current broken codec. Verify the exact source/destination-port byte order against the current official Java I2P implementation or a frozen known-good payload fixture, and record the provenance in the test.

The frozen fixture must make non-symmetric port values obvious, e.g.:

```text
source_port      = 0x1234
destination_port = 0xabcd
protocol         = 6
```

so a byte-order reversal cannot pass accidentally.

## 3.2 Decoder

Decode one bounded gzip member and recover:

```text
source_port
destination_port
protocol
uncompressed payload
```

Required validation:

```text
bad magic rejected
unsupported compression method rejected
malformed/truncated header rejected
unsupported/unsafe optional-header layout handled deliberately
truncated DEFLATE rejected
CRC mismatch rejected
ISIZE mismatch rejected
uncompressed-size ceiling enforced during decompression, not after unlimited growth
protocol exposed as typed metadata
trailing bytes policy explicit
```

For Streaming adapters, protocol must equal `6`. The generic client payload type may retain other valid I2P protocol values for future datagram/I2CP use.

Do not allocate according to attacker-controlled compressed/uncompressed size without hard bounds.

---

# 4. Phase B — independent/frozen payload evidence

Add at least one frozen known-good I2P gzip payload fixture whose bytes were not produced by the new encoder under test.

Preferred provenance, in order:

```text
1. current Java I2P implementation output
2. current i2pd output
3. manually derived RFC1952/I2P fixture checked by an independent gzip implementation
```

No live router is required.

Required tests:

```text
known_good_i2p_gzip_fixture_decodes
encoder_matches_known_good_header_and_metadata
encoded_magic_is_1f_8b_08
source_port_bytes_match_fixture
destination_port_bytes_match_fixture
protocol_byte_9_is_6
gzip_crc_corruption_rejected
gzip_isize_corruption_rejected
zlib_wrapped_old_i2pr_format_rejected
old_extra_sha256_format_rejected
bounded_decompression_bomb_rejected
```

The test fixture provenance should be documented in comments or `specs/references/` if that is the repo convention. Do not create a large new fixture harness.

---

# 5. Phase C — correct Stream ID ownership and handshake states

Primary files:

```text
crates/i2pr-client/src/streaming/connection.rs
crates/i2pr-client/src/streaming/manager.rs
crates/i2pr-proto/src/streaming/packet.rs
```

Represent local and peer stream identifiers so their ownership cannot be confused.

For the originator A:

```text
A chooses A_receive_id > 0 before SYN
A SYN:
    sendStreamId = 0
    receiveStreamId = A_receive_id
```

For recipient B:

```text
B validates A SYN
B chooses B_receive_id > 0
B SYN response:
    sendStreamId = A_receive_id
    receiveStreamId = B_receive_id
```

After A validates B's response:

```text
A knows B_receive_id
future A packets:
    sendStreamId = B_receive_id
    receiveStreamId = A_receive_id
```

and symmetrically for B.

The current source-floor behavior where a locally chosen `send_stream_id` is placed on the originator side before the response must be reviewed against these definitions and corrected wherever necessary.

Use field/type names such as:

```text
local_receive_stream_id
peer_receive_stream_id
```

internally if that prevents ambiguity. Wire field names remain protocol names.

---

# 6. Phase D — real SYN / SYN-response lifecycle

Required state progression for the originator:

```text
Created
 -> OutboundSynSent
 -> [receive authenticated valid SYN response]
 -> Established
```

The originator must not become `Established` merely because a SYN was queued.

Required recipient progression:

```text
[receive authenticated valid SYN]
 -> InboundSynReceived / pending accept
 -> application/policy accepts
 -> emit SYN response
 -> Established (or equivalent accepted state)
```

The exact split between protocol acceptance and application `accept()` may follow the architecture, but the peer SYN response must exist on wire and the originator must process it.

### SYN response requirements

At minimum:

```text
SYNCHRONIZE set
FROM_INCLUDED set as required by current spec
SIGNATURE_INCLUDED set
MAX_PACKET_SIZE_INCLUDED set
correct stream ids
valid signature over exact received bytes with signature bytes zeroed
```

Verify current reference behavior for `NO_ACK`, replay-binding NACK field, and any reply-specific options before freezing policy. Do not copy initial-SYN-only fields into the response merely because the current builder makes it convenient.

---

# 7. Phase E — negotiate maximum packet payload

Both SYN directions advertise their supported maximum packet size.

After successful handshake:

```text
negotiated_max_payload = min(local_max, peer_max)
```

Required behavior:

```text
send_data splits/rejects according to negotiated value
peer cannot advertise zero/invalid value and force undefined behavior
value cannot exceed configured hard ceiling
negotiated value persists for connection lifetime
```

Required tests use intentionally different values so an implementation that simply keeps its local maximum fails.

---

# 8. Phase F — correct the production monotonic clock

If retaining `Clock` / `SystemClock`:

```rust
pub struct SystemClock {
    origin: Instant,
}

now_ms() = origin.elapsed().as_millis()
```

with saturating/checked conversion to `u64` as appropriate.

Required test:

```text
system_clock_is_monotonic_and_can_advance
```

Do not make a flaky sleep-heavy timing test. A minimal production sanity test may only assert nondecrease; deterministic timeout behavior remains tested with `ManualClock`.

If all production callers already inject timestamps explicitly, deleting the unused/broken `SystemClock` is preferable to maintaining redundant time ownership. Document whichever choice is made.

---

# 9. Phase G — add the Streaming-to-destination-routing adapter

Plan 123 already emits `TransportSendRequest`, but the closure test currently treats that value as a network packet and transfers it directly to the peer.

Add one small, runtime-neutral composition adapter in `i2pr-client` that consumes a `TransportSendRequest` and invokes the corrected Plan 122 path.

Conceptual API:

```text
StreamingDestinationAdapter::send(
    local destination context,
    TransportSendRequest,
    DestinationRouting,
    EciesSessionManager,
    DestinationOutboundRole,
    ...
) -> OutboundDeliveryPlan
```

Exact ownership may differ. Preserve these boundaries:

```text
Streaming owns stream protocol
DestinationRouting owns LS2 / lease selection / ECIES composition
Tunnel owns tunnel transforms
Daemon/runtime will eventually own asynchronous scheduling and real router transport
```

Do not add UDP or TCP network handoff here. The destination message is routed through I2P router/tunnel abstractions; NTCP2/SSU2 remain the router-to-router transport layer below the allowed local seam.

### Inbound adapter

After Plan 124's destination dispatcher has authenticated/decrypted a destination payload and recovered I2NP `Data` bytes:

```text
Data payload
 -> decode I2P gzip client payload
 -> require protocol 6 for Streaming listener
 -> select local destination/port streaming manager
 -> process streaming packet
```

Ports are I2P destination ports, not local TCP privileged-port semantics.

---

# 10. Phase H — replace closure VirtualWire with full Plan 122 composition

Keep `VirtualWire` tests if they are useful for fast Streaming-only fault tests, but add a separate Milestone 6 closure trajectory that does not directly transfer `TransportSendRequest` between managers.

Required A -> B SYN path:

```text
A StreamingManager.connect(B)
 -> produces protocol-6 gzip client payload containing Streaming SYN
 -> StreamingDestinationAdapter
 -> I2NP Data
 -> ECIES New Session Garlic
 -> A destination-owned outbound tunnel
 -> A OBEP TUNNEL(B_gateway, B_tunnel_id, Garlic)
 -> Plan 124 explicit authenticated-router-link-bypassed-local-seam
 -> B destination-owned inbound tunnel
 -> B destination owner dispatch
 -> B ECIES authenticate/decrypt
 -> I2NP Data
 -> I2P gzip client payload decode
 -> B StreamingManager receives signed SYN
 -> B listener accepts
 -> B emits real SYN response
```

Required B -> A response path:

```text
B Streaming SYN response
 -> protocol-6 gzip
 -> B destination routing
 -> B outbound tunnel
 -> local router-link seam
 -> A inbound tunnel
 -> A ECIES/session processing
 -> gzip decode
 -> A validates signed SYN response
 -> A learns B stream id
 -> A transitions to Established
```

Only after this point may the closure trajectory describe the connection as established.

---

# 11. Phase I — data, ACK/NACK, loss, reorder, duplicate over integrated path

Once the handshake is real, exercise ordinary traffic through the same destination-routing adapter.

Minimum trajectory:

```text
A write request bytes
 -> one or more Streaming packets
 -> Plan 122/124 destination path
 -> B receives exact ordered bytes
B writes response bytes
 -> same path reverse
 -> A receives exact ordered bytes
```

Inject faults at a layer that does not bypass encryption/tunnel processing.

For example, drop/reorder/duplicate the typed router-delivery objects at the explicit local authenticated-router-link seam.

Required integrated fault cases:

```text
drop one data packet -> NACK/timeout -> retransmission -> exact delivery
reorder N+1 before N -> bounded receive buffer -> ordered delivery
duplicate data packet -> no duplicate application bytes
duplicate SYN response -> idempotent state
replay initial A SYN to destination C -> Bob-hash binding rejects
corrupt gzip CRC -> rejected before Streaming processing
corrupt Streaming signature -> rejected before connection mutation
corrupt ECIES ciphertext -> rejected before gzip/Streaming processing
```

This preserves layer ordering and demonstrates that errors are rejected at the correct trust boundary.

---

# 12. Phase J — CLOSE and RESET over the integrated path

CLOSE must not be considered complete on local send alone.

The official Streaming behavior requires the peer to respond with CLOSE before the graceful connection is fully closed.

Required integrated cases:

```text
A sends signed CLOSE
 -> B authenticates and observes remote close
 -> B sends required close response
 -> A processes response
 -> both sides release state after protocol requirements
```

and:

```text
valid signed RESET terminates immediately
invalid/tampered RESET does not destroy valid connection
```

All connection-table, retransmit, reorder, timer, and backlog state must be released deterministically.

---

# 13. Phase K — preserve 0-RTT semantics without faking establishment

I2P Streaming permits the originator to include application data in the initial SYN and to send limited additional data before the SYN response.

That does **not** mean the originator is already `Established`.

Model this distinction explicitly:

```text
OutboundSynSent may send bounded 0-RTT data using sendStreamId = 0
Established means peer SYN response validated and peer stream id known
```

Do not delete 0-RTT support merely to simplify the state machine. It may be staged if the current code does not yet send pre-response data, but the state model must not confuse “permitted to transmit” with “handshake completed.”

---

# 14. Phase L — Streaming protocol/vector evidence

The current packet codec has useful self-tests. Strengthen evidence around the corrected boundaries.

Required frozen/reference evidence:

```text
I2P gzip payload fixture with visible nonzero ports/protocol
initial signed SYN fixture or independently checked packet
stream-id fields for initial SYN
stream-id fields for SYN response
MAX_PACKET_SIZE option position/value
Bob-hash NACK replay binding for initial SYN
canonical signature preimage (signature region zeroed only)
```

If obtaining a pinned Java/i2pd packet fixture is straightforward in-process or from source tests, use it. Do not make a live router or unavailable host lane a closure requirement.

Keep evidence labels exact:

```text
local-spec-fixture
reference-parser-compatible
native-in-process
mixed-router-live
```

Do not call local-spec-fixture evidence “interoperable.”

---

# 15. Phase M — narrow Milestone 6 architecture/evidence review

After all code/tests above are green, perform the review originally required by Plan 123 before starting SAM.

The review should be a concise section in `plans/125-status.md`, not another sprawling corrective campaign.

Verify these product boundaries:

```text
Standard LeaseSet2 typed/validated/storage path          Plan 119
local destination identity + dedicated tunnel pools      Plan 120
ECIES destination session/Garlic layer                   Plan 121
corrected encrypted destination routing                  Plan 124
corrected protocol-6 gzip + Streaming handshake          Plan 125
full local A <-> B Streaming path over tunnels           Plan 125
```

Also record external evidence debt without trying to solve it:

```text
NTCP2 authenticated Q1                    deferred-host-lane-unavailable
external build return Q2                  deferred
mixed-router destination routing          not-yet-proven
mixed-router Streaming                     not-yet-proven
public-network operation                   not-claimed
```

Question for closure is only:

> Is there any remaining local product defect that prevents a future SAM adapter from consuming the destination/Streaming API without bypassing I2P protocol layers?

If no, close Milestone 6 local product and move to SAM planning.

If yes, record the exact narrow defect and stop. Do not invent another broad “runtime transport” campaign by default.

---

# 16. Documentation/status synchronization

On success update:

```text
plans/123-status.md
plans/125-status.md
plans/000-mvp-roadmap.md
plans/118-123-milestone6-router-construction-roadmap.md
README.md
AGENTS.md
docs/architecture/i2pr-client.md
docs/architecture/i2pr-proto.md
docs/protocol-support.md
specs/support.toml
```

Remove stale contradictions such as listing Streaming under “Not implemented” while also claiming a completed local Streaming layer.

Use exact final labels:

```text
plan_122 = passed-corrected-local-destination-routing
plan_123 = passed-corrected-minimal-streaming-local
plan_125 = passed-milestone6-local-corrective-closure
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained
next_product_layer = SAM baseline planning (Milestone 7)
```

Do not state:

```text
interoperable Streaming
mixed-router Streaming passed
network-functional router
```

unless separate evidence actually exists.

---

# 17. Validation commands

At minimum:

```bash
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked -p i2pr-proto --all-targets
cargo +1.95.0 test --locked -p i2pr-client --all-targets
cargo +1.95.0 test --locked -p i2pr-netdb --all-targets
cargo +1.95.0 test --locked -p i2pr-tunnel --all-targets
cargo +1.95.0 test --locked -p i2pr-daemon --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-multipass-interop-boundary.sh
bash scripts/check-constrained-host-lane-boundary.sh
```

The retired Plan 046/rootless-supervisor baseline is not a Plan 125 blocker.

---

# 18. Explicit acceptance criteria

Plan 125 is complete only when all are true:

- [ ] The client payload encoder emits an RFC 1952 gzip member beginning `1f 8b 08`.
- [ ] I2P source port, destination port, xflags, and protocol are encoded in the correct gzip header bytes.
- [ ] Protocol 6 is encoded for Streaming.
- [ ] The gzip trailer uses standard CRC-32 and ISIZE; no custom SHA-256 or compressed-length field remains.
- [ ] A frozen independently-derived payload fixture verifies header metadata and byte order.
- [ ] Bounded gzip decompression rejects malformed/truncated/oversized inputs.
- [ ] Initial originator SYN uses `sendStreamId = 0` and a nonzero originator-selected `receiveStreamId`.
- [ ] Recipient emits a real signed SYNCHRONIZE response with correctly owned stream IDs.
- [ ] Originator does not transition to Established before validating that response.
- [ ] Peer stream ID is learned from the response and used on subsequent packets.
- [ ] Maximum packet payload is negotiated as the minimum supported by both peers.
- [ ] Current Bob-hash SYN replay binding remains enforced.
- [ ] Streaming signatures verify over exact received bytes with only the signature region zeroed.
- [ ] `SystemClock` is corrected or removed; no production clock API remains that returns effectively zero forever.
- [ ] A runtime-neutral adapter feeds `TransportSendRequest` through the corrected Plan 122/124 destination routing path.
- [ ] Inbound authenticated destination Data is decoded through the real I2P gzip client payload layer before Streaming processing.
- [ ] A -> B SYN traverses LS2 -> ECIES -> outbound tunnel -> local router seam -> inbound tunnel -> ECIES -> gzip -> Streaming.
- [ ] B -> A real SYN response traverses the same architecture in reverse.
- [ ] Both sides become Established only after the real response path.
- [ ] Bidirectional application bytes traverse the integrated path exactly once.
- [ ] Loss, reorder, duplicate, replay, gzip corruption, Streaming-signature corruption, and ECIES corruption tests fail/recover at the correct layer.
- [ ] Retransmission remains bounded and deterministic.
- [ ] Graceful CLOSE requires peer close behavior and releases all state.
- [ ] Valid RESET terminates; invalid RESET cannot destroy a valid connection.
- [ ] No direct `VirtualWire<TransportSendRequest>` path is used as Milestone 6 closure evidence.
- [ ] No live NTCP2/SSU2 activation, Docker, VM, namespace, Python interop harness, SAM, I2CP socket API, HTTP/SOCKS proxy, or public-I2P work is introduced.
- [ ] Workspace tests/clippy/docs and boundary scripts are green.
- [ ] The narrow Milestone 6 architecture/evidence review finds no unresolved local protocol-layer bypass required for SAM.
- [ ] Documentation distinguishes local product completion from independent/mixed-router interoperability.

## Final handoff on success

```text
plan_124 = passed-plan122-corrective-closure
plan_125 = passed-milestone6-local-corrective-closure
plan_122 = passed-corrected-local-destination-routing
plan_123 = passed-corrected-minimal-streaming-local
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
router_construction = may-continue
external_acceptance_debt = retained-separately
next_product_layer = SAM baseline planning (Milestone 7)
```

Do not create a further corrective plan unless the Phase-M review identifies one concrete unresolved local defect.

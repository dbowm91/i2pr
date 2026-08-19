# Plan 123 — Milestone 6 minimal streaming core

## Status

- **Ready after Plan 122 closes**.
- Date: **2026-08-19**.
- Parent roadmap: [`118-123-milestone6-router-construction-roadmap.md`](118-123-milestone6-router-construction-roadmap.md).
- Precondition: Plan 122 provides authenticated bidirectional destination
  message delivery over dedicated destination tunnels.
- Primary output: an internal TCP-like reliable byte-stream API over that message
  layer.
- SAM and I2CP remain downstream.

## Objective

Implement the minimum interoperable I2P Streaming protocol core required to open
an authenticated connection between two destinations, move an ordered reliable
bidirectional byte stream over unreliable/unordered I2P messages, and close or
reset it cleanly.

This plan is not a performance-tuning campaign. It must establish correct wire
format, signatures, replay prevention, sequencing, acknowledgement,
retransmission, bounded reordering, basic congestion/window behavior, and
lifecycle cleanup.

Required local product trajectory:

```text
Destination A connect(B)
 -> signed Streaming SYN over Plan 122 destination messaging
 -> B validates SYN and accepts
 -> signed SYN response
 -> stream becomes established
 -> A/B transfer bytes in both directions
 -> injected loss/duplication/reordering is recovered
 -> CLOSE completes cleanly
```

No streaming packet may bypass Plan 122's LeaseSet2/ECIES/destination-tunnel
routing path.

---

# 1. Normative references

Primary references:

```text
https://i2p.net/en/docs/specs/streaming/
https://i2p.net/en/docs/api/streaming/
https://i2p.net/en/docs/specs/i2cp-overview/
```

Current protocol details relevant to the MVP:

- the minimum Streaming packet header is 22 bytes before NACK/option data;
- there is no packet length field; lower layers frame the message;
- SYN uses `SYNCHRONIZE` and requires `FROM_INCLUDED` and
  `SIGNATURE_INCLUDED`;
- signed packets are verified over the full header+payload with the signature
  option bytes zeroed while computing/verifying the signature;
- `MAX_PACKET_SIZE_INCLUDED` is sent with SYN;
- current SYN replay prevention uses NACK count `8` with the receiver's 32-byte
  Destination hash in the NACK field;
- initial SYN normally has `NO_ACK` because the `ackThrough` field is not yet
  meaningful;
- streaming uses I2P protocol number `6`;
- current ordinary MTU guidance remains compatible with a default near 1730
  payload bytes, with lower negotiated value taking precedence.

Implement current behavior, not pre-0.9.58 replay semantics, unless backward
compatibility can be added without compromising the current path.

---

# 2. Code ownership

Keep Streaming in `i2pr-client` with wire-only structures in `i2pr-proto` if
that matches existing architecture.

Recommended split:

```text
crates/i2pr-proto/src/streaming/
    packet.rs
    options.rs

crates/i2pr-client/src/streaming/
    mod.rs
    connection.rs
    listener.rs
    send.rs
    recv.rs
    congestion.rs
    timer.rs
```

Alternative module organization is acceptable, but the dependency rule is not:

```text
wire parsing != connection policy
connection policy != daemon/API listener policy
```

Do not create `i2pr-api`/SAM/I2CP in this plan.

---

# 3. Phase A — internal I2P client payload framing

Plan 122 delivers generic destination payloads. Streaming interoperability also
requires the standard I2P client payload metadata carried in the gzip-compatible
framing used by I2CP/client messaging.

Implement a reusable internal payload envelope before Streaming state machines.
It must encode/decode at least:

```text
source port
destination port
I2P protocol number
payload bytes
integrity framing/CRC required by the current I2P client payload format
```

For Streaming:

```text
protocol = 6
```

Use the repository's existing `flate2` dependency where appropriate. Do not
assume compression is always beneficial; support the valid minimal/effort-zero
form needed for interoperable framing.

The internal destination API should operate on a typed value, not raw magic gzip
header offsets scattered throughout streaming code.

Required tests:

```text
payload_frame_streaming_protocol_6_round_trip
source_destination_ports_round_trip
crc_corruption_rejected
truncated_frame_rejected
bounded_uncompressed_size
gzip_metadata_bytes_match_fixture
```

This framing should later be reusable by I2CP rather than reimplemented there.

---

# 4. Phase B — Streaming packet wire codec

Implement the current packet structure:

```text
sendStreamId      u32
receiveStreamId   u32
sequenceNum       u32
ackThrough        u32
nackCount         u8
NACKs             nackCount * u32
resendDelay       u8
flags             u16
optionSize        u16
optionData        optionSize bytes
payload           remaining bytes
```

Use explicit newtypes for stream IDs/sequence numbers if that prevents identity
mixups.

Hard bounds:

```text
maximum packet bytes
maximum NACK count
maximum option bytes
maximum payload bytes
```

No allocation based solely on untrusted `nackCount` or `optionSize` without
remaining-length and configured-limit checks.

Unknown flag bits 12-15 must be zero for current compatibility. Unsupported
known flags/options must receive deliberate policy rather than being silently
interpreted.

---

# 5. Phase C — flags and option parsing

Required flags/options for the minimal core:

```text
SYNCHRONIZE
CLOSE
RESET
SIGNATURE_INCLUDED
FROM_INCLUDED
MAX_PACKET_SIZE_INCLUDED
NO_ACK
```

Useful/optional initial support:

```text
DELAY_REQUESTED
```

Explicitly deferred unless implementation is already trivial and fully correct:

```text
OFFLINE_SIGNATURE
ECHO/ping
PROFILE_INTERACTIVE
SIGNATURE_REQUESTED
```

Packet option order must follow the specification. Parsing must infer signature
length from the included Destination signing key type for SYN packets.

Do not assume 40-byte DSA signatures. The repository's primary Destination is
expected to use Ed25519/variable-length signatures.

Required malformed tests:

```text
syn_missing_from
syn_missing_signature
syn_missing_max_packet_size
reserved_flag_set
option_order_invalid
option_size_overrun
signature_length_wrong_for_destination
```

---

# 6. Phase D — canonical packet signatures

Implement signed packet verification/construction carefully.

For packets carrying `SIGNATURE_INCLUDED`:

```text
signature preimage = entire streaming header/options/payload
                     with the signature option byte region replaced by zeroes
```

Use Destination signing keys from Plan 120.

Required signed packets:

```text
SYNCHRONIZE
CLOSE
RESET
```

according to current specification requirements.

Verification must operate on the exact received bytes with only the signature
field logically zeroed. Avoid decode -> canonical re-encode -> verify because
that can create signature ambiguity.

Add frozen known-good signed packet fixtures or an independent signing oracle.

---

# 7. Phase E — current SYN replay prevention

Current Streaming requires initiator Alice to include Bob's Destination hash in
the SYN NACK field:

```text
nackCount = 8
NACK bytes = 32-byte Bob Destination hash
```

The receiver Bob must interpret this specially for SYN and require exact match
to its own Destination hash before accepting the signed connection request.

Because the signature covers these bytes, this prevents replaying Alice's valid
SYN to a different destination.

Required tests:

```text
syn_with_correct_bob_hash_accepted
syn_with_wrong_bob_hash_rejected
syn_hash_bytes_covered_by_signature
replayed_syn_to_different_local_destination_rejected
```

Do not apply ordinary NACK semantics to this SYN field.

---

# 8. Phase F — stream identifiers and connection table

Create per-local-destination bounded connection tables.

States should be explicit, for example:

```text
OutboundSynSent
InboundSynReceived
Established
ClosingLocal
ClosingRemote
Reset
Closed
```

Exact states may differ, but invalid transitions must be testable.

Connection keying must handle the protocol's asymmetric stream IDs:

- initial outbound packets may have sender stream ID zero before the peer's ID
  is known;
- each side chooses/owns its receive-side identifier according to protocol
  semantics;
- duplicate/collision handling is explicit.

Bounds:

```text
max streams per local destination
max pending inbound SYNs
max pending outbound connects
max connection-table bytes/count
connection setup timeout
idle timeout
```

Unknown-stream packets may be retained only in a small bounded pre-SYN reorder
buffer for the interval needed by current protocol behavior.

---

# 9. Phase G — connect/listen internal API

Expose an internal socket-like API suitable for future SAM/I2CP adapters.

Conceptually:

```text
StreamingManager::listen(local_port)
StreamingManager::connect(remote_destination, remote_port, local_port)
StreamHandle::read(...)
StreamHandle::write(...)
StreamHandle::close()
```

The exact Rust async API may use bounded channels rather than `AsyncRead`/
`AsyncWrite` initially. Do not force a public API design before the state machine
works.

Port semantics must follow I2P, not privileged TCP port rules. Port zero retains
its I2P meaning.

A listener owns bounded accept backlog and may reject/reset excess incoming
streams.

---

# 10. Phase H — SYN / SYN-ACK establishment

Outbound connect:

```text
application connect(B)
 -> Plan 122 resolves/routs to B
 -> build Streaming SYN
      sendStreamId = 0 as required
      SYNCHRONIZE
      FROM_INCLUDED = A Destination
      SIGNATURE_INCLUDED
      MAX_PACKET_SIZE_INCLUDED
      NO_ACK
      NACK field = B hash replay binding
      optional initial payload
 -> sign
 -> client payload frame protocol 6
 -> Plan 122 send
```

Inbound:

```text
B receives protocol-6 payload
 -> decode Streaming packet
 -> validate replay destination hash
 -> validate A Destination and signature
 -> apply listener/connection limits
 -> allocate stream state
 -> send SYNCHRONIZE response with B's stream metadata/signature
```

The responder may bundle initial response payload.

After valid response, both sides transition to Established with negotiated max
payload size = minimum(local, remote).

---

# 11. Phase I — send sequencing and outbound window

Split application writes into payload chunks no larger than negotiated maximum.

Maintain:

```text
next sequence number
ordered unacked packet map
send window size
last acknowledged sequence
per-packet send timestamp/retry count
```

Everything is bounded by configured stream/window limits.

Use a conservative initial window. A production default may follow current I2P
guidance, but tests must be able to set very small windows deterministically.

No unbounded buffering when the application writes faster than the remote peer
acks. Apply backpressure through the stream handle.

---

# 12. Phase J — receive ordering, ACK, and NACK

Maintain a bounded receive window.

Required behavior:

```text
next expected sequence
in-order packet -> deliver / advance
future packet inside window -> retain boundedly
missing packets -> include NACKs in outgoing ack/data
duplicate packet -> do not redeliver; ack state may be refreshed
packet too far ahead -> drop/reset according to policy without allocating gap-sized storage
```

`ackThrough` and NACK list must produce a precise acknowledgement model.

Generate standalone ACK packets when no reverse data is available within the
configured ACK delay.

Two NACK observations for the same unacked packet should trigger the minimum
fast-retransmit behavior required by current protocol guidance.

---

# 13. Phase K — retransmission and RTO

Implement deterministic retransmission timers using the repository clock/timer
abstractions.

Minimum behavior:

```text
initial RTO bounded/configurable
per-packet retry timer
exponential or protocol-compatible backoff
maximum retransmission count / maximum lifetime
RTT sample from newly acknowledged non-retransmitted packets
bounded RTO minimum/maximum
```

Do not use `sleep()` in tests.

A failed stream eventually resets/closes and releases state.

The first implementation does not need a perfect clone of Java I2P's mature
congestion controller, but it must be stable under ordinary loss and must not
send unbounded retransmissions.

---

# 14. Phase L — minimum congestion/window behavior

Implement enough window control to avoid flooding the destination/tunnel queues.

Minimum acceptable policy:

```text
finite initial window
increase window on successful progress up to hard maximum
reduce window on timeout/fast retransmit
respect peer DELAY_REQUESTED when implemented
never exceed configured max in-flight packets/bytes
```

Keep the policy in one module so later tuning does not mutate wire semantics.

Tests should assert monotonic bounds rather than exact Java timing constants
unless the protocol mandates them.

---

# 15. Phase M — CLOSE and RESET

Implement signed graceful close and abnormal reset.

Required semantics:

```text
CLOSE may be combined with final payload where valid
CLOSE is authenticated/signed per spec
remote CLOSE stops accepting new payload after ordered delivery point
local close eventually releases state after required acknowledgement/timeout
RESET terminates immediately after signature/identity validation
invalid RESET does not destroy a valid stream
```

Ensure no task/channel remains alive indefinitely after both endpoints close.

---

# 16. Phase N — Plan 122 integration

Streaming packets must use the exact Plan 122 delivery stack:

```text
Streaming packet
 -> protocol-6 client payload frame
 -> I2NP Data
 -> ECIES Garlic
 -> destination outbound tunnel
 -> selected remote Lease2 inbound tunnel
 -> remote destination ECIES
 -> Data
 -> client payload frame
 -> Streaming packet
```

Do not add a special streaming shortcut into `i2pr-tunnel`, `i2pr-daemon`, or
ECIES internals.

If the streaming response triggers ECIES NSR/Existing Session state changes,
those remain Plan 121 session-manager behavior beneath Streaming.

---

# 17. Deterministic acceptance trajectories

## 17.1 Establish + small request/reply

```text
A listens/connects to B
A SYN contains initial "GET" bytes
B accepts and SYN response contains initial "OK" bytes
A/B become Established
both applications observe exact initial bytes once
```

## 17.2 Loss

Drop one middle data packet.

Require:

```text
later packet retained boundedly
NACK/timeout causes retransmit
application receives ordered exact byte stream
no duplicate delivery
```

## 17.3 Reordering

Deliver packets N+1 before N.

Require bounded reorder storage and exact ordered application output.

## 17.4 Duplication

Duplicate SYN response, data packet, ACK, and CLOSE independently.

Require idempotent protocol behavior and no duplicated application bytes.

## 17.5 Replay attack

Replay A's valid SYN to Destination C.

Require rejection because the signed NACK field contains B's hash.

## 17.6 Corruption

Corrupt:

```text
client payload CRC/frame
Streaming signature
option length
sequence/ack fields outside permitted state
```

Require typed bounded rejection.

## 17.7 Close/reset

Graceful bidirectional close and forced reset each release all connection,
unacked, reorder, timer, and channel state.

---

# 18. Resource limits

At minimum configure/test:

```text
max streams per destination
max router-wide streams if manager owns one
max listener backlog
max pending SYNs
max packet payload
max option bytes
max NACKs
max send window packets/bytes
max receive reorder packets/bytes
max unacked packets
max retransmissions
max pre-SYN unknown-stream packets
setup timeout
idle timeout
close timeout
RTO min/max
```

Resource exhaustion of one local destination must not consume another
destination's reserved stream capacity unless explicit router-wide budgets are
also reached.

---

# 19. Security/privacy invariants

1. Validate SYN destination replay binding before expensive/retained connection
   state where possible.
2. Validate signed control packets before state-destructive transitions.
3. Do not log payload bytes, destination private material, ECIES session keys,
   or full stream contents.
4. Keep stream traffic within the owning destination context and tunnel pools.
5. Do not create clearnet sockets/listeners in `i2pr-client`; SAM/service tunnels
   will adapt local sockets later.
6. A missing/expired remote LS2 causes a normal Plan 122 routing refresh/failure,
   not a direct transport fallback.
7. Queue limits and retransmit limits are mandatory because streaming is an
   attacker-controlled state-retention surface.

---

# 20. Optional reference evidence

After the deterministic all-i2pr path is green, add a small set of parser/vector
fixtures from a pinned independent Streaming implementation if feasible without
new infrastructure.

Useful evidence:

```text
signed SYN bytes
variable Ed25519 signature option layout
Bob-hash SYN replay field
SYN response
basic ACK/NACK packet
CLOSE/RESET packet
protocol-6 client payload framing
```

Do not block local product closure on a host lane unavailable for unrelated
router transport.

---

# 21. Documentation updates

On closure update:

```text
README.md
AGENTS.md
docs/architecture/i2pr-client.md
docs/protocol-support.md / support authority
specs/support.toml
plans/000-mvp-roadmap.md milestone execution state if Plan 118 did not already
capture the transition
```

Document the distinction:

```text
minimal streaming local product = implemented
independent-router streaming interoperability = not yet claimed
SAM/I2CP = not implemented
```

---

# 22. Validation commands

At minimum:

```bash
cargo fmt --all --check
cargo test --locked -p i2pr-proto --all-targets
cargo test --locked -p i2pr-client --all-targets
cargo test --locked -p i2pr-netdb --all-targets
cargo test --locked -p i2pr-tunnel --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

Use deterministic testkit fault injection for loss/duplication/reordering.

---

# 23. Explicit acceptance criteria

Plan 123 is complete only when:

- [ ] Protocol-6 client payload framing is implemented with ports/integrity and
      bounded decode.
- [ ] Streaming packet codec implements the current 22-byte minimum header plus
      bounded NACKs/options/payload.
- [ ] SYN requires FROM, signature, max-packet-size, and current Bob-hash replay
      binding.
- [ ] Ed25519/variable-length packet signatures verify over exact received bytes
      with the signature field zeroed.
- [ ] SYN replayed to another destination is rejected.
- [ ] Connection/stream tables are bounded per destination.
- [ ] Internal listen/connect API works without SAM/I2CP.
- [ ] SYN/SYN response establishes a bidirectional stream.
- [ ] Negotiated max payload is the lower peer value.
- [ ] Send/unacked queues apply backpressure and hard bounds.
- [ ] Receive reorder buffers are bounded and provide exact in-order bytes.
- [ ] ACK/NACK behavior causes recovery from ordinary packet loss.
- [ ] Duplicate packets never duplicate application bytes.
- [ ] Retransmission/RTO work with deterministic time and bounded retries.
- [ ] Basic window reduction/increase remains within configured limits.
- [ ] CLOSE and RESET are authenticated and release all state.
- [ ] Full streaming traffic uses Plan 122's LS2 -> ECIES -> destination tunnel
      path in both directions.
- [ ] Deterministic loss, duplication, reordering, replay, corruption, and close
      tests pass.
- [ ] No SAM, I2CP, HTTP/SOCKS, SSU2, normal-daemon NTCP2 activation, or direct
      streaming transport shortcut is introduced.
- [ ] Workspace tests/clippy/docs and boundary checks pass.

## Milestone 6 local handoff

On closure:

```text
plan_123 = passed-minimal-streaming-local
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
next_product_layer = SAM baseline planning (original roadmap Milestone 7)
external_acceptance_debt = retained separately
```

Before beginning SAM, perform one narrow Milestone 6 architecture/evidence
review. That review should inspect product boundaries and current external
acceptance opportunities; it must not recreate the Plan 117 harness loop.

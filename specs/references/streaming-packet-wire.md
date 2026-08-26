# Streaming packet wire format — normative provenance

Plan 128 records the normative sources and the exact byte-level rules
the `i2pr-proto::streaming` packet codec implements. This file is the
provenance record for the fixture/reference tests in
`crates/i2pr-proto/tests/plan128_wire.rs`; it is not a tutorial.

## Sources

- I2P Streaming specification (packet layout, flag bit assignment,
  option ordering, MAX_PACKET_SIZE semantics, signature preimage):
  <https://geti2p.net/spec/streaming>
- Proposal 164 (replay prevention: eight NACK words carrying the Bob
  destination hash in the initial SYNCHRONIZE; no replay NACKs in the
  SYNCHRONIZE ACK): <https://geti2p.net/spec/proposals/164-streaming-congestion/>
- Common structures (Destination encoding used by FROM_INCLUDED):
  <https://geti2p.net/spec/common-structures>

## Flag bits (normative assignment)

| Bit | Constant | Value |
| --- | --- | --- |
| 0 | `FLAG_SYNCHRONIZE` | `0x0001` |
| 1 | `FLAG_CLOSE` | `0x0002` |
| 2 | `FLAG_RESET` | `0x0004` |
| 3 | `FLAG_SIGNATURE_INCLUDED` | `0x0008` |
| 4 | `FLAG_SIGNATURE_REQUESTED` | `0x0010` |
| 5 | `FLAG_FROM_INCLUDED` | `0x0020` |
| 6 | `FLAG_DELAY_REQUESTED` | `0x0040` |
| 7 | `FLAG_MAX_PACKET_SIZE_INCLUDED` | `0x0080` |
| 8 | `FLAG_PROFILE_INTERACTIVE` | `0x0100` |
| 9 | `FLAG_ECHO` | `0x0200` |
| 10 | `FLAG_NO_ACK` | `0x0400` |
| 11 | `FLAG_OFFLINE_SIGNATURE` | `0x0800` |
| 12–15 | reserved | `0xF000` (reject on receipt) |

M6 packet policy flag sets:

```text
initial SYN   = 0x04A9  (SYN | SIG | FROM | MAX | NO_ACK)
SYN response  = 0x00A9  (SYN | SIG | FROM | MAX)
CLOSE         = 0x000A  (CLOSE | SIG)
RESET         = 0x000C  (RESET | SIG)
```

CLOSE/RESET do not include FROM; since 0.9.20 verification uses the
signing key retained in connection state.

## Option region

The 2-byte `optionSize` header field gives the total option-data
length. The option area is **not** a TLV list. Structures appear in
this fixed order, each present exactly when its flag is set:

```text
1. DELAY_REQUESTED          2-byte integer
2. FROM_INCLUDED            self-encoded Destination (no prefix)
3. MAX_PACKET_SIZE_INCLUDED 2-byte big-endian integer
4. OFFLINE_SIGNATURE        OfflineSig (rejected: UnsupportedOfflineSignature)
5. SIGNATURE_INCLUDED       raw variable-length Signature (final field)
```

There are no type/length/value records. The historical invented
markers (`type=1 length=4` before MAX_PACKET_SIZE, `type=3 length=N`
before the signature) must never appear on the wire.

## MAX_PACKET_SIZE semantics

The option value is a **2-byte big-endian integer bounding the
Streaming payload only**. It is neither the total packet size nor the
payload minus the 22-byte minimum header. The current I2P default is
`1730` payload bytes (`DEFAULT_ADVERTISED_MAX_PAYLOAD`). Independent
constants:

```text
minimum fixed header size      = 22
negotiated maximum payload     = e.g. 1730 (option default)
option-region ceiling          = 1024
NACK ceiling                   = 64 entries
full encoded packet ceiling    = header + NACKs + options + payload
```

Connection negotiation is `min(local advertised payload max,
remote advertised payload max)`.

## Signature handling

The signature covers the entire packet (header + NACKs + options +
payload) with **the raw Signature field bytes set to zero**. There is
no TLV prefix to zero. The signature length is inferred from signing
context:

```text
with FROM_INCLUDED:  from the Destination's signing key type
without FROM (established connection): from the peer signing key
                     retained in connection state
neither:             fail closed (SignatureContextUnavailable)
```

Outbound construction encodes the complete packet once with a zeroed
signature placeholder of the exact key-derived length, signs those
already-zeroed bytes directly, then patches only the signature bytes
(`StreamingOptions::encode_with_placeholder` +
`install_packet_signature`). Verification copies the wire bytes,
zeroes the exact located signature bytes
(`build_signature_preimage`), and verifies against the FROM or
retained peer signing key.

## Initial SYN / SYN response

Initial originator SYN:

```text
sendStreamId = 0, receiveStreamId = locally selected nonzero id
sequenceNum = 0, flags = 0x04A9 (includes NO_ACK)
nackCount = 8, NACK words = remote Destination hash (Proposal 164)
signature covers the replay hash through the canonical preimage
```

SYN response (Proposal 164 explicitly forbids replay-prevention NACKs
here):

```text
sendStreamId = originator receiveStreamId
receiveStreamId = responder selected nonzero id
sequenceNum = 0, flags = 0x00A9 (no NO_ACK), nackCount = 0
ackThrough is valid and acknowledges the initial SYN
```

## Plan 130 sequence / ACK-NACK / port semantics

Corrective reference record (2026-08-25) for the Plan 130 runtime
changes; sources are the current Streaming Protocol Specification
(i2p.net/en/docs/specs/streaming/, last updated 2023-10, accurate for
0.9.59), Java I2P `apps/streaming/java/src/net/i2p/client/streaming/impl/`
(`ConnectionPacketHandler.java`, `Connection.java`,
`MessageInputStream.java`, branch `master`, inspected 2026-08-25), and
the Streaming Library option documentation.

Sequence numbers:

- The SYN occupies sequence 0; the SYN response occupies sequence 0.
- Ordinary application data starts at sequence **1** and increments by
  one per message except plain ACKs and retransmissions.
- "If the sequenceNum is 0 and the SYN flag is not set, this is a plain
  ACK packet that should not be ACKed" (specification, sequenceNum
  field). A seq-0 non-SYN packet never enters the application receive
  window; a payload attached to that form is dropped.

Acknowledgements:

- `ackThrough` is the highest received sequence number on the stream;
  it is valid including value 0 (which acknowledges the handshake slot)
  and is ignored only when the NO_ACK flag is set (and on the initial
  connection packet). Java processes ACKs whenever
  `!(isSYN && sendStreamId <= 0)` — there is no "zero means absent"
  rule.
- NACKs list sequences strictly below `ackThrough` that were not yet
  received (`MessageInputStream.locked_getNacks`: gaps between the
  contiguous-ready point and the highest received block). Two NACKs of
  one sequence request a fast retransmit.
- Receiver-side generation (Java `MessageInputStream.updateAcks`):
  `ackThrough = highest received block id`, `NACKs = missing blocks in
  (ready point, highest)`; bounded naturally by the reorder window and
  by the one-byte wire count (max 255).
- Sender-side application (Java `Connection.ackPackets`): packets at or
  below `ackThrough` are cleared unless explicitly NACKed; with NACKs
  present the cumulative floor only advances to `lowestNack - 1`;
  duplicate ACKs (below the recorded floor) change nothing.
- Delayed standalone ACK default: 750 ms (`i2p.streaming.initialAckDelay`
  default); a DELAY_REQUESTED value of 0 requests an immediate ack.
- Plain-ACK packets are unsigned, carry no payload, and are never
  acknowledged themselves (no ACK-of-ACK loop).

I2P ports:

- Listener dispatch follows the I2CP demultiplexer contract
  (`I2PSessionDemultiplexer.findListener`): exact `destination_port`
  listener first, then the wildcard listener bound to port 0
  (`PORT_ANY`); if neither exists the message is dropped ("No listener
  found").
- `source_port == 0` (PORT_UNSPECIFIED/PORT_ANY) is legal on inbound
  SYNs; no TCP privileged-port policy applies.
- An established stream retains the local/remote port tuple fixed by
  its SYN and echoes it on every subsequent packet (Java replies set
  `localPort`/`remotePort` from the incoming packet).

Plan 130 implements the sequence-space, NACK-aware cumulative
acknowledgement, `poll_acks` delayed-ACK scheduling, piggyback
suppression, and inbound adapter destination-port authority in
`crates/i2pr-client/src/streaming/`.

## Plan 131 connector ownership and oversized-write rollback

Plan 131 adds three additional correctness checks on top of the
Plan 130 surface (recorded 2026-08-26):

Connection-owned I2P port tuple:

- The `StreamingConnection` established by the SYN handshake owns
  its `local_port` / `remote_port` pair. The wire ClientPayload
  `source_port` / `destination_port` of every subsequent outbound
  `send_data` / `send_close` / `send_reset` is taken from the
  connection's stored tuple; the runtime caller's port arguments
  are assertions and fail closed with typed `PortTupleMismatch`
  **before** any state mutation.
- The inbound `process_inbound_packet` SYN-response branch
  validates the decoded wire `source_port` against the outbound
  connection's `remote_port` and the decoded wire `destination_port`
  against `local_port`. A wrong-port response leaves the connection
  in `OutboundSynSent` and never transitions to `Established`.
- `source_port == 0` is the I2P "unspecified" value and remains
  legal end-to-end; the wildcard listener on port `0` catches the
  delivery for any destination port when no exact listener is bound.

Side-effect-free oversized `send_data` rollback:

- `send_data` validates the negotiated maximum payload size and
  the current send-window backpressure **before** sequence
  allocation, send-window mutation, retransmit tracking, or
  outbound queue mutation. A rejected oversized write consumes
  no sequence number; the next valid packet receives the exact
  contiguous sequence number that would have been assigned before
  the rejected call.

Plan 131 evidence: `crates/i2pr-client/tests/plan131_trajectory.rs`
(7 deterministic trajectories: exact cell replay hits the tunnel
duplicate window; consumed ECIES session-tag replay leaves the
receiver state untouched; fresh ECIES reseal deduplicates at the
Streaming sequence level; established data uses the connection
ports in the wire envelope; source-port-zero works end-to-end;
oversized `send_data` produces no state change and the next valid
packet gets the contiguous sequence; the Plan 130 fixture surface
still composes after every Phase D / E refactor).

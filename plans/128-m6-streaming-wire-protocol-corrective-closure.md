# Plan 128 — Milestone 6 Streaming wire-protocol corrective closure

## Status

- **Execute after Plan 127 closes.**
- Source floor for the known Streaming defects: `523d5dcd87f6c04853a016f7b54e3922697ffb2b`.
- Predecessor: Plan 127 destination-layer closure.
- Successor: `plans/129-m6-integrated-destination-streaming-final-gate.md`.
- Preserve Plan 125's correct RFC 1952 client-payload gzip work and real SYN/SYN-response connection-state progression.

## Objective

Make the `i2pr-proto::streaming` packet wire format and `i2pr-client::streaming` control packets match the current I2P Streaming specification.

This pass is deliberately independent of destination tunnels. Fast direct Streaming tests are appropriate here. Plan 129 will prove the corrected packets over the complete destination stack.

## 1. Normative flag map

The current source-floor constants are shifted/misassigned.

Replace them with the current I2P values exactly:

```text
bit 0   SYNCHRONIZE              0x0001
bit 1   CLOSE                    0x0002
bit 2   RESET                    0x0004
bit 3   SIGNATURE_INCLUDED       0x0008
bit 4   SIGNATURE_REQUESTED      0x0010
bit 5   FROM_INCLUDED            0x0020
bit 6   DELAY_REQUESTED          0x0040
bit 7   MAX_PACKET_SIZE_INCLUDED 0x0080
bit 8   PROFILE_INTERACTIVE      0x0100
bit 9   ECHO                     0x0200
bit 10  NO_ACK                   0x0400
bit 11  OFFLINE_SIGNATURE        0x0800
bits 12-15 reserved              0xF000
```

Required regression assertions should pin every constant numerically.

For the current M6 packet policy:

```text
initial SYN flags       = 0x04A9  (SYN | SIG | FROM | MAX | NO_ACK)
SYN response flags      = 0x00A9  (SYN | SIG | FROM | MAX)
CLOSE flags             = 0x000A  (CLOSE | SIG)
RESET flags             = 0x000C  (RESET | SIG)
```

Do not include FROM in normal current CLOSE/RESET merely because old source code made signature parsing easier.

## 2. Remove invented Streaming option TLVs

The I2P Streaming option area is **not** a list of type/length/value records.

The 2-byte `option size` field in the packet header gives the total option-data length. The flags determine which structures are concatenated and the specification fixes their order.

Canonical option order:

```text
1. DELAY_REQUESTED          2-byte integer, if present
2. FROM_INCLUDED            self-encoded Destination, if present
3. MAX_PACKET_SIZE_INCLUDED 2-byte integer, if present
4. OFFLINE_SIGNATURE        OfflineSig, if present
5. SIGNATURE_INCLUDED       raw variable-length Signature, if present
```

Remove protocol-visible pseudo constants such as `STREAMING_OPTION_MAX_PACKET_SIZE = 1` and `STREAMING_OPTION_SIGNATURE = 3` if they exist only to implement the invented TLV encoding.

Do not emit:

```text
type byte
length byte
value
```

for MAX_PACKET_SIZE or Signature.

## 3. Add a flag-driven option codec

Do not continue manual byte splicing separately in SYN, SYN response, CLOSE, and RESET builders.

Add one bounded typed option codec, for example:

```text
StreamingOptions
StreamingOptionDecodeContext
StreamingOptionLocations
```

Exact names are discretionary.

Encoder input should include only semantic values. It writes them in the normative flag order.

Decoder must parse according to the flags and an explicit context where needed.

Required supported M6 fields:

```text
DELAY_REQUESTED       parse/encode 2-byte value when flag set
FROM_INCLUDED         parse Destination
MAX_PACKET_SIZE       parse/encode 2-byte value
SIGNATURE_INCLUDED    locate/extract variable-length raw Signature
```

`OFFLINE_SIGNATURE` may remain deferred if the repository does not yet support offline Streaming signatures, but the decoder must fail with a typed `UnsupportedOfflineSignature` (or equivalent) before misparsing later fields.

Unknown reserved bits remain rejected.

## 4. Variable-length signature handling

The source-floor parser assumes an Ed25519-style 64-byte signature and locates a final `type + length + signature` TLV. That is not the protocol.

Current rule:

```text
with FROM_INCLUDED:
    infer Signature type/length from the Destination signing key

without FROM_INCLUDED on an established connection:
    infer Signature type/length from the peer signing key retained in connection state
```

The signature is the final option field and contains **only the Signature bytes**.

Refactor the packet/manager boundary so generic structural decoding does not hard-code 64 bytes.

One acceptable architecture:

```text
decode packet fixed header + raw option region
 -> parse options with OptionDecodeContext(peer signing key if needed)
 -> return exact signature byte offset/length
```

Do not create a protocol assumption that all future destinations use Ed25519 merely because local `DestinationIdentity` currently does.

## 5. Correct signature preimage construction

The Streaming signature covers the entire packet header + option data + payload with **the raw Signature field bytes set to zero**.

There is no TLV type/length prefix to zero.

Recommended outbound construction:

```text
determine signing-key signature length
build option region with zero bytes of exactly that signature length at final signature position
encode complete packet once
sign complete zeroed packet
replace only the zero signature bytes with actual signature
```

Avoid the current pattern of signing a shorter packet, adding a synthetic signature option, then signing again.

Verification:

```text
parse exact signature offset/length
copy wire packet
zero exact raw signature bytes
verify against FROM or retained peer signing key
```

Required corruption test flips one signature byte and fails verification.

## 6. Correct MAX_PACKET_SIZE field and MTU semantics

The Streaming option is a **2-byte big-endian integer** and specifies the maximum length of the **Streaming payload only**, not the total packet and not the payload minus the 22-byte base header.

Current I2P default is 1730 payload bytes. The current API documentation notes 1812 as a recommended ECIES value, but the default remains 1730.

For M6:

```text
DEFAULT_ADVERTISED_MAX_PAYLOAD = 1730
```

Do not silently switch to 1812 in this corrective pass. That optimization can be a later intentional policy change.

Separate constants/concepts:

```text
minimum fixed header size = 22
negotiated maximum payload = e.g. 1730
option-region ceiling
NACK ceiling
full encoded packet ceiling = checked sum of header + NACKs + options + payload
```

Do not define `MAX_STREAMING_PAYLOAD_BYTES = 1730 - 22`.

The connection negotiation remains:

```text
negotiated payload max = min(local advertised payload max, remote advertised payload max)
```

Use intentionally different values in tests.

## 7. Correct initial SYN policy

Initial originator SYN:

```text
sendStreamId = 0
receiveStreamId = locally selected nonzero id
sequenceNum = 0
SYNCHRONIZE
FROM_INCLUDED
SIGNATURE_INCLUDED
MAX_PACKET_SIZE_INCLUDED
NO_ACK
NACK count = 8
NACK bytes = remote Destination hash
```

The replay NACK field is Proposal 164 behavior and is required for current peers.

The signature covers the replay hash.

Keep the Plan 125 rule that the originator stays `OutboundSynSent` until a valid peer SYN response arrives.

## 8. Correct SYN-response policy

SYN response:

```text
sendStreamId = originator receiveStreamId
receiveStreamId = responder selected nonzero id
sequenceNum = 0
SYNCHRONIZE
FROM_INCLUDED
SIGNATURE_INCLUDED
MAX_PACKET_SIZE_INCLUDED
NO_ACK = NOT set
NACK count = 0
```

Proposal 164 explicitly says **do not include replay-prevention NACKs in the SYNCHRONIZE ACK**.

`ackThrough` is valid in the response. With the normal initial sequence number 0, the response can acknowledge the initial SYN according to the existing connection logic.

Split source-floor generic `validate_syn_policy()` if needed:

```text
validate_initial_syn(...)
validate_syn_response(...)
```

Do not require the initial-SYN Bob-hash replay binding on a response.

## 9. Correct CLOSE / RESET packet shapes

Current protocol:

```text
CLOSE requires SIGNATURE_INCLUDED
RESET requires SIGNATURE_INCLUDED
FROM is not normally required for RESET since 0.9.20
```

For an established connection, verify signatures with the stored peer signing key even when FROM is absent.

Required tests:

```text
CLOSE has flags 0x000A and raw final signature
RESET has flags 0x000C and raw final signature
CLOSE/RESET verify with retained peer identity
malformed or unknown standalone signed control without identity context fails closed
```

## 10. Preserve the corrected I2P client-payload gzip layer

Do not regress Plan 125's correct client payload envelope:

```text
1f 8b 08
source port bytes 4-5
 destination port bytes 6-7
XFL byte 8
protocol byte 9 == 6
raw DEFLATE
CRC32 + ISIZE
```

Keep bounded decompression, bad CRC/ISIZE rejection, and old zlib/custom-format rejection.

## 11. Packet fixture/reference tests

Add focused wire tests independent of `StreamingManager` behavior.

Required fixed assertions:

```text
all flag numeric values
initial SYN flags == 0x04A9
response flags == 0x00A9
CLOSE flags == 0x000A
RESET flags == 0x000C
MAX_PACKET_SIZE 1730 bytes == 06 c2
initial SYN NACK count == 8 and exact destination hash bytes
response NACK count == 0
option data contains no synthetic TLV tags/lengths
signature is last raw option field
signature preimage differs only by zeroing signature bytes
```

Use an asymmetric/obvious test Destination and MTU values.

Document normative provenance in a narrow file such as:

```text
specs/references/streaming-packet-wire.md
```

## 12. Fast manager-level handshake test

Direct Streaming-only tests may remain here.

Required flow:

```text
A connect -> real canonical SYN
B validates -> pending accept
B accept -> canonical SYN response with zero NACKs/no NO_ACK
A validates -> Established
B Established
```

Then send one ordinary data packet each direction and verify stream-id fields:

```text
A future sendStreamId = B receive id
A receiveStreamId = A receive id
B future sendStreamId = A receive id
B receiveStreamId = B receive id
```

## 13. Validation

Run:

```bash
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked -p i2pr-proto --all-targets
cargo +1.95.0 test --locked -p i2pr-client --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

## 14. Explicit acceptance criteria

- [ ] Every Streaming flag constant matches the current I2P bit assignment.
- [ ] No Streaming option-data TLV type/length records remain.
- [ ] Option data is encoded/decoded in normative flag order.
- [ ] MAX_PACKET_SIZE is exactly two bytes big-endian.
- [ ] 1730 is treated as the default **payload** maximum, not the full packet size.
- [ ] Full packet bounds are checked independently from the negotiated payload MTU.
- [ ] Signatures are raw final option bytes whose length comes from signing-key context.
- [ ] Signature preimage zeroes exactly the raw signature bytes.
- [ ] Initial SYN flags are `0x04A9`, includes NO_ACK, and carries 8 replay NACK words / the remote Destination hash.
- [ ] SYN response flags are `0x00A9`, has no replay NACKs, and does not set NO_ACK.
- [ ] Originator remains `OutboundSynSent` until the valid response.
- [ ] CLOSE/RESET use current flag shapes and verify without synthetic FROM/TLV requirements.
- [ ] Plan 125 RFC1952 gzip tests remain green.
- [ ] No destination-tunnel, SAM, external transport, or live-network work is introduced.

## Handoff on success

Write `plans/128-status.md` with:

```text
plan_123 = passed-corrected-streaming-wire-local
plan_128 = passed-streaming-wire-protocol-corrective-closure
milestone6_local_product = not-closed
next = plans/129-m6-integrated-destination-streaming-final-gate.md
```

Do not claim Streaming interoperability until independent-router evidence exists.
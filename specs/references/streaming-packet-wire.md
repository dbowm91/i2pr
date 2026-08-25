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

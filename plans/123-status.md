# Plan 123 status — corrective closure required

## Current authority

- Status: **`provisional-blocked-on-plan122-correction`**.
- Reopened: **2026-08-24** after post-closure source/protocol audit.
- Original implementation plan: [`123-m6-minimal-streaming-core.md`](123-m6-minimal-streaming-core.md).
- Lower-layer corrective prerequisite: [`124-m6-plan122-destination-routing-corrective-closure.md`](124-m6-plan122-destination-routing-corrective-closure.md).
- Streaming corrective plan of record: [`125-m6-streaming-corrective-and-local-closure.md`](125-m6-streaming-corrective-and-local-closure.md).
- The original Plan 123 code and tests remain useful component work; this status corrects the closure and interoperability claims.

## Why the prior closure is reopened

Four concrete issues prevent the existing `passed-minimal-streaming-core` label
from serving as Milestone 6 closure evidence.

### 1. I2P client payload framing is wrong on wire

The current `i2pr-proto::streaming::payload` codec uses a custom format:

```text
protocol + ports + compressed length + zlib + SHA-256 + CRC32
```

I2P uses a standard RFC 1952 gzip member. The fixed 10-byte gzip header carries
I2P source port, destination port, xflags, and protocol metadata; Streaming uses
protocol 6. Integrity is the standard gzip CRC-32/ISIZE trailer. There is no
extra SHA-256 field and no custom compressed-length prefix.

Normative reference:

```text
https://i2p.net/en/docs/specs/i2cp-overview/
```

### 2. The connection originator is marked Established before the peer SYN reply

`StreamingManager::connect()` currently queues the initial SYN and then performs
an optimistic local transition to `Established`.

The actual protocol requires:

```text
A initial SYN: sendStreamId = 0, receiveStreamId = A-selected nonzero id
B validates SYN and sends a SYNCHRONIZE response with B's selected stream id
A validates that response, learns B's stream id, negotiates MTU
only then is the peer handshake established
```

Normative references:

```text
https://i2p.net/en/docs/specs/streaming/
https://i2p.net/en/docs/api/streaming/
```

### 3. The nominal full Streaming trajectory bypasses Plan 122

The current `plan123_trajectory.rs` transfers `TransportSendRequest` values
between two `StreamingManager`s through an in-memory `VirtualWire`.

That is useful Streaming-only state-machine coverage, but it does not traverse:

```text
LeaseSet2 -> ECIES Garlic -> outbound destination tunnel
 -> router-link seam -> inbound destination tunnel -> ECIES -> Streaming
```

Plan 123 explicitly required that path for closure.

### 4. Production `SystemClock` does not advance meaningfully

The current implementation computes:

```rust
Instant::now().elapsed()
```

from a newly created `Instant`, yielding effectively zero rather than useful
monotonic elapsed time.

Plan 125 must correct or remove that abstraction.

## Retained valid Plan 123 work

Do not discard the existing Streaming implementation wholesale. The following
surfaces remain useful pending correction:

```text
Streaming packet codec and 22-byte base header
flag/option parsing and reserved-bit rejection
canonical signature-preimage helper
signed-control-packet primitives
Bob-hash SYN replay-binding support
bounded connection/listener tables
send/receive windows
ACK/NACK/retransmission policy
congestion bounds
CLOSE/RESET machinery
Streaming-only deterministic fault tests
```

The direct VirtualWire tests may remain as lower-level tests after the corrected
integrated trajectory is added.

## Current state

```text
plan_122 = corrective-reopened-plan124
plan_123 = provisional-blocked-on-plan122-correction
milestone6_local_product = not-closed
milestone6_interoperable = not-claimed
next = plans/124-m6-plan122-destination-routing-corrective-closure.md
then = plans/125-m6-streaming-corrective-and-local-closure.md
```

The previous proposed “Plan 124 streaming runtime adapter + UDP/TCP transport
handoff” is superseded. Do not implement that description. Plan 124 now repairs
the lower destination-routing composition; Plan 125 performs the Streaming
correction and Milestone 6 local closure gate.

## Required eventual closure labels

Only after Plans 124 and 125 pass:

```text
plan_122 = passed-corrected-local-destination-routing
plan_123 = passed-corrected-minimal-streaming-local
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
next_product_layer = SAM baseline planning (Milestone 7)
```

Do not use “interoperable Streaming” for local deterministic evidence.

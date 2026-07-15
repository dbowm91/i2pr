# I2P streaming protocol

Status: **required**  
Primary roadmap milestone: **6**  
Dependencies: destinations, garlic/ECIES, LeaseSets and destination tunnel pools

## Scope

Streaming provides a reliable, ordered, TCP-like byte-stream abstraction over I2P destination messages. It defines connection establishment, stream identifiers, sequence/acknowledgement fields, NACKs, options, signatures, retransmission, flow/congestion behavior, close/reset and packet payloads.

Streaming is end-to-end between destinations. It must not rely on ordered or guaranteed delivery from SSU2, NTCP2, I2NP or tunnels.

## Authoritative sources

- [Streaming protocol specification](https://i2p.net/en/docs/specs/streaming/), pinned in [SOURCES.md](../SOURCES.md), updated 2023-10 and accurate for 0.9.59.
- [Streaming API documentation](https://i2p.net/en/docs/api/streaming/) for option semantics and application behavior.
- Common structures, I2NP and destination/ECIES specifications for framing and signatures.
- Current proposal/specification changes referenced by the streaming document, including ECIES MTU implications.

The protocol contains no negotiated version field. Feature support cannot be inferred reliably from a peer-supplied streaming version, so the MVP should implement a conservative interoperable subset and tolerate specification-permitted optional fields.

## Required MVP subset

### Connection lifecycle

Implement explicit states for:

- outbound SYN sent;
- inbound SYN received/pending accept;
- SYN reply and established;
- half-close/close negotiation;
- reset and terminal cleanup.

Required behavior includes random nonzero stream IDs, initial sequence numbers, destination binding/signature validation, connection timeouts, duplicate SYN handling, simultaneous/late packets and idempotent teardown.

### Packet codec

Implement the current packet header and options required for:

- send/receive stream IDs;
- sequence number and cumulative acknowledgement;
- bounded NACK list;
- resend delay and flags;
- option length and option data;
- SYN, CLOSE, RESET, signatures, destination inclusion and offline signatures where required;
- payload constrained by current end-to-end/tunnel MTU.

The streaming specification has no internal total packet length; lower layers frame the message. The streaming decoder must therefore receive an already bounded message slice and validate option/payload boundaries exactly.

### Reliability and ordering

Implement:

- receive-window tracking and ordered delivery;
- bounded out-of-order buffering;
- cumulative ACK and NACK processing;
- retransmission timers under a monotonic clock;
- RTT/RTO estimation and backoff;
- duplicate packet suppression;
- send/receive flow control;
- congestion control sufficient for interoperability and stability;
- maximum retransmission/connection lifetime;
- keepalive/ping behavior only as required by current peers and API semantics.

Do not equate tunnel loss with peer failure prematurely. Tunnel rotation, garlic session changes and path latency can alter RTT without changing the destination.

### Listener/connect API

The internal client crate should expose bounded asynchronous operations for:

- listen/accept with backlog and per-destination limits;
- connect with destination lookup, deadline and cancellation;
- read/write with backpressure;
- graceful close and reset;
- local/remote destination and protocol/port metadata;
- configuration of a reviewed subset of streaming options.

SAM, I2CP and service tunnels should adapt this API rather than reimplementing streaming state.

## Congestion and resource policy

The first implementation should prioritize correctness and bounded behavior over reproducing every Java tuning heuristic. It must define:

- per-stream send and receive buffer caps;
- per-destination and global stream limits;
- maximum out-of-order packets/bytes;
- ACK/NACK list limits;
- retransmission queue and timer limits;
- fairness across streams and destinations;
- behavior when tunnels disappear or congestion persists.

Congestion-control changes can affect interoperability, network load and fingerprinting. Treat them as protocol-adjacent policy with deterministic simulation and long-duration mixed-router tests.

## Implementation references

- Java I2P: `apps/streaming/java/src`, especially packet codecs, connection manager, sender/receiver and congestion state.
- I2P+: corresponding streaming package; recent pacing, queue and congestion fixes are particularly relevant.
- i2pd: streaming and destination code under `libi2pd_client`.
- Emissary/go-i2p: inspect current companion or in-tree streaming implementation and integration status; do not assume README completeness.

Compare packet signatures/options, MTU calculation, SYN retransmission, ACK/NACK generation, close/reset edge cases, RTO floors/ceilings, out-of-order limits and congestion growth/reduction. Java I2P and I2P+ are one lineage; i2pd provides independent wire evidence.

## Required tests

- Golden packet vectors for minimum header and every supported option/flag combination.
- Invalid option sizes, duplicate/conflicting flags, excessive NACKs and trailing data.
- SYN/SYN-reply loss, duplication, reordering and simultaneous events.
- Data loss, duplication, reordering and delayed ACKs in deterministic simulated networks.
- Sequence-number and stream-ID boundary/wrap behavior defined by the protocol.
- RTT/RTO convergence and exponential backoff without wall-clock sleeps.
- Receive-window and out-of-order buffer exhaustion.
- Graceful close, half-close, reset, timeout and cancellation in every state.
- Tunnel replacement and temporary destination unreachability.
- Bidirectional stream interoperability with Java I2P and i2pd destinations.
- Long-running transfer under representative loss/reordering and low-memory budgets.
- Fuzzing packet/options parser and model-based state-machine testing.

## Deferred and compatibility behavior

- Every Java streaming configuration option: deferred; expose only a reviewed portable subset.
- Raw/repliable datagram protocols: separate protocol surface, not part of streaming.
- Advanced congestion experimentation, multipath streaming and custom ACK schemes: post-MVP experimental.
- Feature behavior that cannot be negotiated and is not broadly deployed: compatibility tests first, disabled by default.
- Zero-copy API promises: deferred until correctness and buffer-lifetime semantics are stable.

## Open decisions

1. Congestion-control algorithm and which behavior is required for practical parity with Java I2P/i2pd.
2. Initial packet MTU calculation across current LeaseSet/ECIES/tunnel formats.
3. Sequence-space and receive-window data structures with bounded memory.
4. Whether destinations permit application-selected ports/protocol values at the first checkpoint.
5. Stream persistence/recovery semantics across destination tunnel-pool rebuilds and router restart.
6. Exact internal API cancellation behavior for pending connect, accept, read and close.
7. How pacing integrates with Tokio without one timer task per packet or stream.
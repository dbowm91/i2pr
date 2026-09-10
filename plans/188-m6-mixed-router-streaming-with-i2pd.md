# Plan 188 — M6 mixed-router Streaming with i2pd

Status: **blocked until Plan 187 passes**.

## 1. Goal

Layer the existing `i2pr-client::streaming` implementation on the real mixed-router destination path proven by Plan 187 and demonstrate bidirectional Streaming interoperability with exact-pinned i2pd-owned destinations.

This is the first plan allowed to claim mixed-router Streaming against one independent family. It does not close M6 until Plan 189 adds the second family.

## 2. Architecture lock

Streaming packets must follow exactly:

```text
i2pr StreamingManager
 -> existing StreamingDestinationAdapter
 -> existing DestinationRouting
 -> ECIES/Garlic
 -> real i2pr outbound destination tunnel
 -> remote LeaseSet2 lease
 -> i2pd destination
```

and the reverse path through i2pr's real inbound destination tunnel and ECIES decryptor.

Do not add a special interop Streaming implementation, direct SAM bridge, direct SSU2 stream path, or harness-only delivery shortcut.

## 3. Reference endpoint

Use exact-pinned unmodified i2pd 2.61.0 (`635b013a612ff47278ef02acf8580a28e10e26c5`). Create the reference Streaming destination through its ordinary SAM API. The reference side may use zero-hop client tunnels in the isolated topology; i2pr counted paths remain real remote one-hop tunnels from Plan 187.

Use simple reference applications behind SAM STREAM:

- echo/half-close fixture for protocol acceptance;
- digest-based larger payload fixture;
- simultaneous sibling connections.

Reference application code must not implement I2P protocol framing.

## 4. Direction A: i2pr initiator -> i2pd responder

Required trajectory:

1. resolve reference LeaseSet2 through the live NetDB path if not cached;
2. `StreamingManager::connect` from an i2pr destination;
3. SYN traverses Plan 187 destination path;
4. i2pd normal Streaming endpoint responds;
5. i2pr reaches `Established` through ordinary packet handling;
6. exchange a small payload and exact digest response;
7. exchange a payload requiring multiple Streaming packets and retransmission bookkeeping;
8. orderly close/half-close according to existing Streaming contract.

## 5. Direction B: i2pd initiator -> i2pr responder

Publish the i2pr destination's real LeaseSet2. Initiate from the i2pd SAM STREAM client to the i2pr destination and prove:

- inbound SYN reaches the correct `StreamingManager` through the real tunnel/Garlic path;
- listener/accept logic returns the normal SYN response;
- establishment succeeds;
- small + multi-packet application data moves both directions;
- close/reset releases the exact connection.

No test-only `accept_from_peer` injection may be used after runtime startup.

## 6. Robustness matrix

Exercise ordinary bounded impairment at the Streaming layer without corrupting lower cryptographic layers:

- one dropped Streaming data packet followed by retransmission;
- one dropped ACK;
- reordering of two admissible Streaming packets;
- duplicate packet;
- delayed packet inside normal timeout window;
- stalled reader/backpressure;
- sibling connections where one is impaired/closed;
- reference process disconnect;
- underlying tunnel replacement while no in-flight protocol invariant is violated.

Prefer deterministic impairment at an existing test seam below/around destination delivery. Do not patch i2pd or build a packet-manipulating network namespace harness.

## 7. Resource and privacy requirements

Keep existing ceilings for:

- Streaming connections;
- send/receive windows;
- retransmission queues;
- destination routing pending bytes;
- tunnel queues.

Evidence records only connection state, counts, sizes, sequence/ACK summaries where non-sensitive, and payload digests—not application plaintext or private keys.

## 8. External runner

Create/extend `tests/integration/m6-interop/run-i2pd.sh` as a small fail-closed lane that composes Plans 184–188 using one exact-pinned i2pd process. It must:

- verify exact commit and clean checkout;
- create fresh unprivileged datadirs;
- prohibit public reseed/network dependency;
- start i2pd and i2pr;
- establish the real SSU2/tunnel/NetDB/destination stack;
- run directions A and B;
- collect sanitized command-derived evidence;
- kill both processes and verify resource baselines;
- exit nonzero on missing/blocked mandatory evidence.

Do not copy historical M3 multi-router harness machinery.

## 9. Acceptance criteria

Plan 188 passes only when:

1. exact-pinned i2pd and normal i2pr daemon establish mixed-router Streaming direction A;
2. direction B establishes through i2pr's normal listener/accept path;
3. both directions traverse real Plan 187 destination tunnels/LeaseSet2/ECIES and are not direct transport/local-fabric shortcuts;
4. small and multi-packet payload digests match in both directions;
5. orderly close/half-close behavior is observed;
6. at least two simultaneous sibling connections remain isolated;
7. drop/ACK-loss/reorder/duplicate robustness behaves within existing bounded retransmission rules;
8. stalled-reader/backpressure and underlying failure do not leak memory/tasks;
9. the fail-closed external runner verifies exact pin/clean checkout and produces sanitized command-derived evidence;
10. all Plans 184–187 lower-layer regressions remain green;
11. full workspace/static floor and exact-head routine CI pass;
12. `plans/188-status.md` records `milestone6_i2pd_streaming_interop = passed` but keeps `milestone6_interoperable = not-yet-claimed`, then advances `next_executable_plan = 189`.

## 10. Stop conditions

Stop for a narrow corrective if failure is attributable to a specific Streaming wire semantic (SYN options, ports, ACK/NACK, CLOSE, retransmission, sequence arithmetic). Preserve a minimized deterministic reproducer and compare against current I2P Streaming specification/reference behavior before changing code. Do not weaken tests or route around Plan 187.

## 11. Handoff

Plan 189 repeats the required mixed-router core against Java I2P as the second independent family and is the only plan allowed to close the retained M6 mixed-router conformance debt.

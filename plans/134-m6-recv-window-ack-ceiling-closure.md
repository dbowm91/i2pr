# Plan 134 — Milestone 6 receive-window ACK ceiling closure

## Status

**Ready for execution.**

- Date: **2026-08-27**.
- Source floor: `3d23c0eb698cbb366c8f7818a7b03bb95a00eb4f`.
- Plan 133 remains the successful evidence/authority closure for the replay, Elligator, and transactional-send corrections it actually proved. This plan reopens Milestone 6 for **one newly discovered concrete Streaming correctness defect** only.
- This pass is not permission to restart the prior Milestone 6 validation series. If this plan passes, Milestone 6 local product work is closed and **Milestone 7 / SAM baseline planning is immediately authorized**.

## 1. Objective

Correct one receive-window invariant:

```text
A Streaming packet rejected as TooFarAhead
  MUST NOT
    advance highest_received
    change ackThrough
    create NACK holes
    enter the reorder buffer
    advance delivered state
    schedule a new ACK
    cause a later piggyback ACK to acknowledge the rejected sequence
```

The intended state transition is:

```text
packet arrives
  -> duplicate/already-delivered check
  -> receive-window admissibility check
       TooFarAhead => return with receive/ACK state unchanged
  -> only accepted packet may advance highest_received
  -> accepted in-order packet is delivered
     OR accepted out-of-order packet is buffered
```

This is a narrow correctness and reliability fix. Do not alter the established Streaming wire format, ACK/NACK semantics, congestion policy, retransmission policy, ECIES behavior, tunnel behavior, or Milestone 6 external-validation policy.

## 2. Defect at the Plan 133 source floor

Target:

```text
crates/i2pr-client/src/streaming/recv_window.rs
RecvWindowPolicy::receive()
```

Current ordering is effectively:

```rust
if sequence < self.next_expected {
    return Duplicate;
}

self.highest_received = max(self.highest_received, sequence);

if sequence == self.next_expected {
    // deliver
}

let max_sequence = self
    .next_expected
    .saturating_add(self.config.max_window_packets as u32);
if sequence >= max_sequence {
    return TooFarAhead;
}

// buffer accepted out-of-order packet
```

The mutation of `highest_received` occurs **before** the `TooFarAhead` rejection.

Therefore a packet can be explicitly dropped by the receive-window policy while still becoming the source of the next `ackThrough` value returned by:

```text
RecvWindowPolicy::ack_view()
```

That violates the meaning of the receiver's ACK state. A rejected packet must not be represented later as received.

### 2.1 Why this matters

`ack_view()` treats `highest_received` as the cumulative ACK ceiling and creates NACKs for holes below it. The sender-side window then clears tracked packets at or below `ackThrough`, except explicitly NACKed sequences.

Because the wire NACK count is bounded, allowing an attacker-controlled far-ahead dropped sequence to inflate `highest_received` can create a false cumulative ACK that covers sequence space the receiver did not accept. This can cause legitimate retransmission state to be discarded prematurely and corrupt stream reliability.

The defect is especially undesirable for i2pr's security goals because an unauthenticated application peer should not be able to drive unbounded or semantically false acknowledgement state merely by choosing an extreme sequence number.

### 2.2 Reference behavior

The current Java I2P Streaming implementation separates admission from receive-state mutation:

```text
MessageInputStream.canAccept(messageId, payloadSize)
  -> applies buffering/window/resource admission checks
  -> caller does not invoke messageReceived() when admission fails

MessageInputStream.messageReceived(messageId, payload)
  -> only then updates _highestBlockId for an accepted packet
```

`MessageInputStream.updateAcks()` derives `ackThrough` from `_highestBlockId`.

The relevant behavioral contract is therefore not "copy Java's exact buffer sizing". The relevant invariant is:

> a packet rejected by receive admission does not advance the highest-received sequence used for acknowledgement generation.

Preserve i2pr's existing bounded receive-window policy; only correct the mutation ordering.

## 3. Hard scope boundaries

### In scope

- `crates/i2pr-client/src/streaming/recv_window.rs`:
  - move/reshape the `TooFarAhead` admission decision so rejected packets are state-inert;
  - minimal comments documenting accepted-only ACK-state mutation.
- focused unit tests for `RecvWindowPolicy`;
- one manager-level regression proving a rejected far-ahead packet cannot poison a later ACK/piggyback ACK;
- documentation/status reconciliation after green validation;
- `plans/000-mvp-roadmap.md` clarification that the currently blocked independent-router/mixed-router interoperability criteria remain external acceptance debt and are not a prerequisite for beginning Milestone 7 in the constrained development environment.

### Explicitly out of scope

Do **not** add or reopen:

- NTCP2 or SSU2 activation;
- live mixed-router validation;
- Emissary, Java, or i2pd harness construction;
- Plan 116/117 or any old host-validation lane;
- Docker, rootless namespaces, VMs, Multipass, QEMU, privileged setup;
- public I2P testing;
- Python protocol/interoperability harnesses;
- new receive-window algorithms;
- adaptive receive-window sizing;
- congestion-control redesign;
- ACK/NACK wire-format changes;
- changes to the 255-NACK wire ceiling;
- changes to sequence-zero/SYN/plain-ACK semantics;
- changes to retransmission timers;
- ECIES, Garlic, LeaseSet2, tunnel, port-ownership, CLOSE, or RESET behavior;
- SAM implementation itself.

Preserve all still-valid Plan 130–133 invariants.

## 4. Phase A — make rejected far-ahead packets ACK-state inert

### A1. Correct `RecvWindowPolicy::receive()` ordering

Preferred structure:

```rust
pub fn receive(&mut self, sequence: u32, payload: Vec<u8>) -> RecvWindowDecision {
    // 1. Already delivered / duplicate.
    if sequence < self.next_expected {
        return RecvWindowDecision::Duplicate { sequence };
    }

    // 2. In-order packet is always within the receive point.
    if sequence == self.next_expected {
        self.note_accepted_sequence(sequence);
        // existing delivery + contiguous reorder drain
        ...
        return Delivered { ... };
    }

    // 3. Out-of-order admission must happen before ACK-state mutation.
    let window_size = self.config.max_window_packets as u32;
    let max_sequence = self.next_expected.saturating_add(window_size);
    if sequence >= max_sequence {
        return RecvWindowDecision::TooFarAhead { sequence };
    }

    // 4. Packet is accepted into the reorder window.
    self.note_accepted_sequence(sequence);
    self.reorder.insert(...);
    RecvWindowDecision::Buffered { sequence }
}
```

A small private helper such as:

```rust
fn note_accepted_sequence(&mut self, sequence: u32)
```

is acceptable if it makes the invariant obvious. It must do nothing except monotonically update `highest_received` for a packet already proven admissible.

An equally small ordering without a helper is fine.

### A2. Do not change the window boundary

Current policy defines the admissible out-of-order range using:

```text
max_sequence = next_expected + max_window_packets
sequence >= max_sequence => TooFarAhead
```

Preserve this exact boundary in Plan 134 unless an existing deterministic test proves it independently wrong.

This plan is about **state mutation after rejection**, not changing how far ahead a packet may be.

### A3. Do not treat a dropped packet as a NACK target

After a `TooFarAhead` result:

- `highest_received` must be unchanged;
- `ack_view().0` (`ackThrough`) must be unchanged;
- the generated NACK set must be unchanged;
- `reorder_count()` must be unchanged;
- `next_expected()` must be unchanged;
- `delivered_count()` must be unchanged.

Do not solve this by inserting the rejected sequence into another bookkeeping structure. Rejection should remain cheap and state-minimal.

## 5. Phase B — focused receive-window regression tests

Add tests in the existing receive-window test location or a narrowly named module. Do not create a new general harness.

### B1. Fresh-window far-ahead packet is completely inert

Construct a small deterministic window, for example:

```text
next_expected = 1
max_window_packets = 4
first rejected sequence = 5
```

Assert:

```text
receive(5, payload) == TooFarAhead { sequence: 5 }
highest_received == None
ack_view == (0, [])
next_expected == 1
reorder_count == 0
delivered_count == 0
```

The exact numbers should follow the existing constructor/config APIs.

### B2. Far-ahead packet cannot overwrite an accepted ACK ceiling

1. Accept/deliver sequence `1`.
2. Snapshot all receive/ACK state.
3. Submit a sequence at or beyond the current far-ahead boundary.
4. Assert `TooFarAhead`.
5. Assert the complete snapshot is unchanged.

At minimum:

```text
highest_received remains 1
ack_view remains (1, [])
next_expected remains 2
reorder_count unchanged
delivered_count unchanged
```

### B3. Far-ahead packet cannot inflate an existing reorder/NACK view

Create a legitimate out-of-order state, e.g.:

```text
sequence 1 delivered
sequence 3 accepted/buffered
```

This should yield an ACK/NACK state equivalent to:

```text
ackThrough = 3
NACK = [2]
```

Then submit a far-ahead sequence and assert the entire ACK view remains exactly the same.

This proves Plan 134 does not accidentally discard valid out-of-order acknowledgement information while fixing the poison case.

### B4. Pin the exact boundary

For a known `next_expected` and window size, test both:

```text
sequence == max_sequence - 1 -> Buffered / accepted
sequence == max_sequence     -> TooFarAhead / state-inert
```

This prevents the fix from silently changing the preexisting receive-window capacity by one packet.

### B5. Extreme sequence does not poison state

Submit `u32::MAX` (or the largest value reachable without violating the test setup) while the receive point is low.

Assert:

- typed `TooFarAhead`;
- no panic;
- no state change;
- no enormous NACK generation;
- `ack_view()` remains bounded to previously accepted state.

This is a regression against attacker-selected sequence extremes, not a request to redesign wraparound semantics.

### Phase B acceptance

- [ ] Fresh far-ahead rejection leaves all receive/ACK state unchanged.
- [ ] An established ACK ceiling cannot be inflated by a rejected packet.
- [ ] Existing valid reorder/NACK state survives a rejected packet unchanged.
- [ ] Existing `max_sequence - 1` / `max_sequence` boundary is preserved exactly.
- [ ] Extreme sequence values remain bounded and state-inert.

## 6. Phase C — manager-level false-ACK regression

A direct policy test proves the bug locally. Add one higher-level StreamingManager regression so a future refactor cannot reintroduce the same defect between the receive window and wire ACK generation.

### C1. Required trajectory

Use an established local Streaming connection through the smallest existing test fixture that exercises production packet processing.

Recommended trajectory:

```text
1. Establish A <-> B Streaming state using existing deterministic helpers.
2. Deliver at least one valid application sequence so B has a known accepted ACK ceiling.
3. Snapshot B's receive-window ack_view.
4. Feed B a syntactically valid packet with the correct stream IDs/ports but a sequence at or beyond B's TooFarAhead boundary.
5. Assert packet processing yields the existing TooFarAhead/non-delivery behavior and does not create application delivery.
6. Assert B's receive-window ack_view is unchanged.
7. Cause B to build its next legitimate outbound packet or standalone/piggyback ACK through production code.
8. Decode that packet and assert its ackThrough/NACK fields reflect only accepted receive state, never the rejected far-ahead sequence.
```

If the manager API surfaces `TooFarAhead` only internally and returns a generic successful observation, assert the receive-window decision through stable state rather than adding a new production error solely for the test.

### C2. No ACK scheduling from the rejected packet

The current manager schedules delayed ACKs only for `Delivered` or `Buffered` receive decisions. Preserve that behavior.

Assert, where existing test-visible APIs permit it, that processing the far-ahead packet does not create/extend a pending standalone ACK deadline.

Do not add a broad public introspection API solely for this assertion. A test-only/read-only accessor is acceptable if the existing manager tests already use that pattern.

### C3. Sender-side consequence test, if inexpensive

If the existing fixture makes this straightforward, additionally prove that the peer's send window does not clear unacked entries merely because the receiver saw and dropped an extreme sequence.

Do not build a new end-to-end harness solely for C3. The mandatory gate is C1; C3 is useful only if it composes naturally from current test helpers.

### Phase C acceptance

- [ ] Production packet processing cannot turn a rejected sequence into a later wire `ackThrough`.
- [ ] A far-ahead drop creates no application delivery.
- [ ] Existing valid ACK/NACK state remains intact.
- [ ] No new ACK-of-drop behavior is introduced.

## 7. Phase D — preserve adjacent Streaming behavior

Run/retain regressions covering the surfaces most likely to be affected by moving receive-window state mutation:

- in-order application delivery;
- accepted out-of-order buffering;
- reorder-buffer drain when the gap arrives;
- duplicate suppression;
- NACK generation for accepted out-of-order packets;
- ACK `0` semantics where applicable to handshake/control behavior;
- delayed standalone ACK scheduling;
- piggyback ACK suppression of pending standalone ACK;
- sequence `0` plain ACK never entering the receive window;
- source/destination port authority;
- CLOSE/RESET behavior;
- Plan 132/133 replay trajectories.

No behavior above should change except that a `TooFarAhead` packet no longer contaminates the ACK ceiling.

## 8. Phase E — local validation gate

Use the pinned Rust 1.95 surface.

At minimum:

```bash
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked -p i2pr-client --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
```

Also run the repository's normal static NTCP2 fixture/vector checks if they remain part of the standard local gate, strictly as regression checks. Do not reopen transport development as a consequence of this plan unless this change directly causes a deterministic regression.

No live network, reference router, privileged host, VM, Docker, rootless namespace, or public-I2P validation is required for Plan 134.

## 9. Phase F — roadmap and authority reconciliation

Only after Phases A–E are green.

### F1. Preserve Plan 133 as successful historical closure evidence

Plan 133 did correctly close its replay/Elligator/transactional-send evidence. Do not rewrite it as a failed plan.

Update `plans/133-status.md` only enough to record that a later concrete receive-window defect was discovered and closed by Plan 134, making Plan 134 the final local Milestone 6 authority.

Recommended historical token:

```text
plan_133 = passed-evidence-authority-superseded-by-plan134-streaming-ack-closure
```

### F2. Final Plan 134 authority

After green acceptance, `plans/134-status.md` must become:

```text
plan_130 = superseded-by-plan131
plan_131 = superseded-by-plan132-and-plan133-final-gates
plan_132 = implementation-landed-evidence-superseded-by-plan133
plan_133 = passed-evidence-authority-superseded-by-plan134-streaming-ack-closure
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure

milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = may-continue
next_product_layer = SAM baseline planning (Milestone 7)
```

### F3. Clarify `plans/000-mvp-roadmap.md`

The original roadmap currently states Milestone 6 exit in terms of:

- bidirectional communication with an independent I2P implementation;
- a mixed-router interoperability checkpoint before feature expansion.

Those remain valid **eventual MVP/external acceptance goals**, but repeated work has established that the current development environment cannot reliably execute the required live/reference-router lane. Plans 118/133 separate that evidence debt from local product progression.

Add a concise current-progression note to the Milestone 6 section. Semantically it should state:

```text
Current development progression policy:
- Milestone 6 local product correctness is gated by the current local closure authority (Plan 134 after this pass).
- Independent-router destination/Streaming/tunnel interoperability remains explicit external acceptance debt.
- That external debt must be closed before final MVP interoperability claims, but it is not a prerequisite for beginning Milestone 7 / SAM in the constrained development environment.
- Do not reopen the retired rootless/VM/Emissary/live-wire harness lanes merely to satisfy the historical sequencing of this roadmap.
```

Do **not** delete the original interoperability requirements or falsely mark them passed. The goal is to distinguish:

```text
local construction progression
vs.
final external interoperability acceptance
```

### F4. Current-facing status docs

Update only current-facing files that still name Plan 133 as the final local Milestone 6 authority, likely:

- `README.md`;
- `AGENTS.md`;
- `docs/architecture/overview.md`;
- `docs/architecture/i2pr-client.md`;
- `docs/protocol-support.md`;
- `specs/support.toml`;
- `.opencode/skills/i2pr-ntcp2-interop/SKILL.md` if it carries milestone authority.

Do not churn unrelated architecture documentation.

## 10. Final acceptance criteria

Plan 134 passes only when every applicable criterion below is true.

### Receive-window correctness

- [ ] `TooFarAhead` is decided before `highest_received` can advance for that packet.
- [ ] A rejected far-ahead packet leaves `highest_received` unchanged.
- [ ] A rejected far-ahead packet leaves `ack_view()` byte-for-byte/element-for-element equivalent to the pre-rejection state.
- [ ] A rejected far-ahead packet leaves `next_expected`, reorder contents/count, and delivered count unchanged.
- [ ] Existing admissibility boundary (`max_sequence - 1` accepted, `max_sequence` rejected) is unchanged.
- [ ] Extreme sequence values do not cause panic, huge NACK generation, or ACK-state inflation.

### Wire/manager correctness

- [ ] Production Streaming processing of a far-ahead packet cannot make a later outbound packet advertise that rejected sequence as `ackThrough`.
- [ ] Rejected far-ahead traffic creates no application delivery.
- [ ] Rejected far-ahead traffic does not schedule/extend a standalone ACK merely because it was observed.
- [ ] Legitimate accepted out-of-order packets still update `ackThrough` and NACKs correctly.

### Regression

- [ ] In-order, reorder, duplicate, ACK/NACK, delayed ACK, piggyback ACK, plain-ACK, ports, CLOSE/RESET, and Plan 132/133 replay tests remain green.
- [ ] Workspace fmt/check/test/clippy/doc are green.
- [ ] Dependency/runtime/fixture static checks are green.
- [ ] No transport/harness/VM/public-network/SAM implementation scope creep is introduced.

### Authority

- [ ] Plan 133 is retained as successful historical evidence but no longer the final current authority.
- [ ] Plan 134 is the sole current local Milestone 6 closure authority.
- [ ] `plans/000-mvp-roadmap.md` clearly distinguishes local progression from outstanding external interoperability acceptance.
- [ ] Mixed-router interoperability remains explicitly unclaimed.
- [ ] Milestone 7 / SAM is identified as the next product layer after Plan 134 passes.

## 11. Required status classification

Until Plan 134 passes:

```text
plan_133 = passed-evidence-authority-reopened-by-plan134-concrete-streaming-defect
plan_134 = ready-for-execution

milestone6_local_product = corrective-reopened
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = hold-for-plan134-recv-window-ack-closure
next = Plan 134
```

After Plan 134 passes:

```text
plan_133 = passed-evidence-authority-superseded-by-plan134-streaming-ack-closure
plan_134 = passed-milestone6-recv-window-ack-ceiling-closure

milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = may-continue
next_product_layer = SAM baseline planning (Milestone 7)
```

At that point **do not create another Milestone 6 plan** unless a new concrete protocol defect is identified. Proceed to Milestone 7 planning/implementation. External interoperability debt remains separate and must not be silently converted into a local progression blocker.

## 12. Handoff execution order

For a smaller-model executor, follow this exact order:

```text
1. Read Plan 134, recv_window.rs, and the existing receive-window/manager tests.
2. Add focused failing unit tests proving TooFarAhead currently contaminates highest_received/ack_view.
3. Move receive-window admission ahead of highest_received mutation; do not change the boundary.
4. Add the manager-level false-ACK regression.
5. Run focused i2pr-client tests.
6. Run the full local Rust/static gate.
7. Only after green, update Plan 133/134 status and the roadmap progression note.
8. Synchronize only current-facing authority wording that is now stale.
9. Inspect the final diff for scope creep.
10. Commit and hand off directly to Milestone 7 / SAM baseline work.
```

This should be a small implementation pass. Do not turn it into a receive-window redesign or another interoperability campaign.

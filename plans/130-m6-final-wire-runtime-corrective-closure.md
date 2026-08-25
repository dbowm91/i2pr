# Plan 130 — Milestone 6 final wire/runtime corrective closure

## Status

**Ready for execution.**

- Date: **2026-08-25**.
- Source floor: `29fb88d36f9794202e88d4b947faed30569c1991`.
- Authority: `plans/126-130-milestone6-final-corrective-roadmap.md`.
- This is the **single remaining Milestone 6 implementation pass**.
- The Plan 129 integrated topology and the successful portions of Plans 126–128 are retained; this plan repairs four narrow defects revealed by the post-closure audit.

Do not mark Milestone 6 closed merely because `plans/129-status.md` historically says `passed-milestone6-integrated-local-product-gate`. Plan 130 supersedes that closure decision until this plan's acceptance criteria are satisfied.

## 1. Objective

Close Milestone 6 with a locally correct, bounded destination + Streaming product path whose wire-visible behavior is compatible with current I2P semantics within the implemented subset.

The required end state is:

```text
Destination ECIES:
  bound NS -> NSR -> ES
  + I2P-compatible randomized Elligator2 on-wire representatives

Streaming:
  SYN sequence 0
  -> SYN response sequence 0
  -> first application data sequence 1
  -> correct cumulative ACK/NACK behavior
  -> standalone delayed/simple ACK when no payload can piggyback

Inbound application routing:
  decoded ClientPayload destination_port
  -> correct local Streaming listener / established port tuple

Integrated evidence:
  persistent destination tunnel roles
  -> persistent duplicate window
  -> full Plan 129 stack in both directions
```

No new transport or external-router lane is required.

## 2. Defects being corrected

### 2.1 Production Elligator2 output has fixed representation choices

At the source floor, `EciesEphemeralKeypair::from_seed_bytes()` canonicalizes the seed and calls the Elligator library with a fixed tweak/representation choice. This is suitable for deterministic test vectors, but not for production on-wire anonymity: I2P reference implementations randomize otherwise-unused representation bits and the valid alternative encoding choice so ephemeral representations look uniformly random rather than implementation-specific.

Required correction: preserve library-backed Elligator2, but split deterministic vector construction from randomized production encoding and prove reference compatibility.

### 2.2 Streaming application sequence numbering starts at zero

At the source floor:

- `SendWindowPolicy::new()` initializes application `next_sequence = 0`;
- `RecvWindowPolicy::new()` expects application sequence `0`;
- Plan 129 application data therefore begins at sequence zero.

For I2P Streaming, SYN occupies sequence zero. Ordinary post-SYN data begins at sequence one. A non-SYN sequence-zero packet is the simple-ACK/control form and must not enter the application receive window.

### 2.3 ACK state is confused with numeric zero and depends on reverse application traffic

The source floor treats `ackThrough == 0` as though acknowledgement information were absent and the integrated loss test clears sender state by emitting reverse application data as an ACK carrier.

Required correction:

- ACK validity is controlled by packet semantics/flags, not by `ackThrough != 0`;
- sequence zero must be acknowledgeable;
- one-way streams need a runtime-neutral standalone delayed ACK path;
- ACK-only packets must not cause ACK-of-ACK loops or application delivery;
- gap/NACK behavior must be reference-driven and bounded.

### 2.4 I2P destination port is decoded but is not authoritative

The inbound adapter currently accepts both the decoded ClientPayload destination port and a separate caller-provided listener port. The caller-provided value determines the listener backlog for a new inbound SYN.

Required correction: the wire `destination_port` and established connection port tuple must own dispatch semantics. A runtime caller may provide the destination owner/manager but must not be able to redirect one I2P port into another listener.

### 2.5 Plan 129 replay test rebuilds the inbound duplicate window

The integrated fixture recreates the receiver's inbound chain before ordinary deliveries. Therefore the test does not prove that an exact router-delivery replay is rejected by the same live tunnel duplicate window.

Required correction: preserve role/window state across ordinary fixture deliveries and distinguish tunnel replay rejection from ECIES/Streaming duplicate handling.

## 3. Hard scope boundaries

### In scope

- `crates/i2pr-crypto/src/ecies.rs` production Elligator2 encoding seam;
- narrowly related ECIES tests/reference note;
- `crates/i2pr-client/src/streaming/` sequence, ACK/NACK, connection port ownership, bounded scheduling;
- `crates/i2pr-client/src/streaming_adapter.rs` inbound port dispatch;
- `crates/i2pr-proto/src/streaming/` only if a typed helper is genuinely required for simple-ACK semantics; do not rewrite the Plan 128 packet codec;
- `crates/i2pr-client/tests/plan129_trajectory.rs` fixture persistence and new full-stack regressions;
- a new focused `plan130_trajectory.rs` if separating the corrective acceptance evidence makes the result clearer;
- docs/status synchronization after green acceptance.

### Out of scope

Do not implement or reopen:

- live NTCP2 or SSU2;
- external mixed-router acceptance as a local closure requirement;
- Plan 116/117 host work;
- rootless namespaces, Docker, VMs, Multipass, QEMU, privileged setup;
- Python harness infrastructure;
- SAM or I2CP socket servers;
- HTTP/SOCKS proxies or service tunnels;
- new destination crypto types, unbound sessions, PQ ratchets, MetaLS2, EncryptedLeaseSet;
- hand-written Elligator2 arithmetic;
- a generic event loop or timer subsystem.

The ACK implementation should remain synchronous/runtime-neutral: the manager records deadlines and a caller polls it with `now_ms`, following the existing `poll_retransmits()` model.

## 4. Phase A — pin independent reference behavior before changing code

The earlier self-to-self tests hid protocol mistakes. Before implementation, capture concise reference evidence for all four areas.

### A1. Elligator2

Confirm from the current I2P ECIES specification plus current Java I2P and i2pd source:

- what bytes participate in the Noise transcript: decoded X25519 public key versus raw representative;
- which representation choices are intentionally randomized on wire;
- handling of the unused/high representation bits;
- whether the library's `RFC9380`, `Randomized`, `Legacy`, tweak, or equivalent APIs reproduce I2P's deployed mapping.

**Do not switch library variants based only on their names.** Prove that representatives generated by the selected production mode decode in Java/i2pd to the exact X25519 public key i2pr intended.

Record the outcome in `specs/references/ecies-destination-ratchet.md` or a narrowly linked addendum.

### A2. Streaming sequence / ACK / NACK

Confirm from current Streaming specification and Java/i2pd behavior:

- initial SYN sequence number;
- SYN-response sequence number;
- first ordinary application sequence number;
- exact definition of a simple ACK packet;
- when `ackThrough == 0` is valid and what it acknowledges;
- when `NO_ACK` suppresses acknowledgement processing;
- standalone ACK delay/default behavior;
- whether an ACK-only packet itself requires an ACK;
- cumulative ACK + NACK interpretation during reordering;
- maximum practical NACK count under the current local bounds;
- handling of data received before the SYN response (0-RTT subset already permitted locally).

Freeze at least one independent byte/semantic fixture that is **not generated by i2pr's own manager**.

### A3. I2P port semantics

Confirm from the current I2CP/Streaming documentation and reference routers:

- exact `source_port == 0` behavior;
- exact `destination_port == 0` behavior;
- listener matching/default/wildcard precedence;
- whether an established stream must retain and validate the original local/remote port tuple on every subsequent ClientPayload.

The implementation must follow the reference result, not infer ordinary TCP socket semantics.

### Phase A acceptance

- [ ] Reference behavior is recorded with pinned source/revision or current official documentation.
- [ ] No implementation choice below depends only on current i2pr self-tests.
- [ ] Elligator library mode is proven I2P-compatible before use in production.

## 5. Phase B — production Elligator2 randomization without breaking deterministic vectors

### B1. Separate deterministic and production construction

Keep a deterministic seam for frozen cryptographic vectors, but production ephemeral generation MUST consume CSPRNG entropy for the on-wire representation choices.

Preferred architecture:

```text
EciesEphemeralKeypair::generate(rng)
  -> generate X25519 ephemeral secret/public
  -> choose I2P-compatible randomized Elligator representation using rng
  -> retain canonical X25519 public key/secret for DH
  -> emit randomized representative

EciesEphemeralKeypair::from_seed_bytes(...)
  -> deterministic test/vector constructor only
  -> documented as non-production representation behavior
```

If the current type stores only a seed + representative, refactor minimally so production does not alter the DH public key merely to obtain output randomization.

### B2. No hand-rolled mapping

The implementation MUST continue to use `curve25519-elligator2` or another already-reviewed dependency API. Do not implement finite-field Elligator math in i2pr.

If the dependency cannot generate I2P-compatible randomized representations, stop and document the incompatibility rather than silently creating a custom algorithm. A narrowly scoped dependency change is allowed only with explicit rationale and review.

### B3. Entropy and compatibility tests

Required tests:

1. deterministic fixed-vector constructor still reproduces the existing Plan 126 KDF/Noise vectors;
2. production generator outputs representatives that the selected independent reference decoder maps to the exact expected X25519 public key;
3. i2pr decoder accepts reference-produced randomized representatives;
4. over a sufficiently large deterministic seeded-CSPRNG test sample, the high two on-wire bits are not fixed and all four values occur;
5. if the mapping exposes a binary alternative branch, both alternatives are observed in the sample or independently pinned fixtures prove both;
6. randomized representation bytes do not change the transcript's canonical decoded X25519 point;
7. retries remain hard-bounded and entropy failures return typed errors;
8. no secret scalar or deterministic seed appears in `Debug`, logs, status output, or public wire fields.

A statistical test here is a regression/fingerprint test, not a cryptographic randomness certification. Keep it deterministic and non-flaky by using a seeded CSPRNG and a generous sample size.

### Phase B acceptance

- [ ] Production representatives no longer have implementation-fixed high bits/alternative behavior.
- [ ] Reference decoder compatibility is proven.
- [ ] Existing NS -> NSR -> ES vectors and lifecycle tests remain green.

## 6. Phase C — correct Streaming sequence-number ownership

### C1. Reserve sequence zero for SYN/simple ACK semantics

Refactor the application send/receive windows so ordinary stream data starts at sequence **1**.

Required invariant:

```text
initial SYN       sequenceNum = 0
SYN response      sequenceNum = 0
simple ACK        sequenceNum = 0, SYNCHRONIZE clear, no app payload
first app packet  sequenceNum = 1
next app packet   sequenceNum = 2
...
```

Do not simply change one constant. Audit every place where sequence zero currently means:

- first application packet;
- `next_expected` initial value;
- retransmission map key;
- cumulative acknowledgement floor;
- reorder/NACK calculation;
- CLOSE/RESET sequence construction;
- 0-RTT buffering.

### C2. Receive-window initialization

After a valid SYN/SYN-response lifecycle, the ordinary receive window must expect application sequence 1. A non-SYN sequence-zero packet must be routed as simple ACK/control and MUST NOT:

- increment `delivered_count`;
- enter the reorder buffer;
- call application delivery;
- advance application `next_expected`.

### C3. 0-RTT

Preserve the existing bounded 0-RTT distinction if it remains within the implemented subset:

- originator may emit pre-response data with `sendStreamId == 0` as required by the current Streaming spec;
- that application data still begins at sequence 1;
- responder buffering/replay constraints stay bounded;
- establishment still requires the authenticated SYN response.

### Phase C acceptance

- [ ] Exact wire tests show SYN=0, response=0, first app=1.
- [ ] Simple ACK sequence zero cannot be delivered as application bytes.
- [ ] Existing connection/reorder/retransmit tests use the corrected sequence space rather than compatibility aliases.

## 7. Phase D — implement spec-correct ACK/NACK behavior

### D1. Represent ACK presence semantically

Remove logic equivalent to:

```text
if ackThrough != 0 { process_ack(); }
```

Numeric zero is a valid cumulative ACK value. Whether to process acknowledgement information must be determined from packet type/flags and the current Streaming rules, especially `NO_ACK`.

Do not add a sentinel value outside the wire format.

### D2. Simple ACK packet

Add an explicit manager-level simple-ACK builder/path using the canonical packet codec.

The exact fields/flags MUST come from Phase A reference evidence, but closure requires:

- sequence number 0;
- no application payload;
- no SYN flag;
- no accidental application delivery on receive;
- valid cumulative ACK/NACK fields;
- no ACK-of-ACK loop.

### D3. Runtime-neutral delayed ACK scheduling

Add narrowly bounded pending-ACK state per connection. Follow the established polling pattern:

```rust
manager.process_inbound_packet(..., now_ms)
// records ACK requirement/deadline

manager.poll_acks(now_ms) -> Vec<TransportSendRequest>
```

Names may differ, but semantics must remain synchronous and deterministic. Do not create a Tokio timer or socket task.

Requirements:

- default ACK delay/reference value follows current I2P behavior documented in Phase A;
- per-connection ACK state is bounded to O(1) or another explicit small ceiling;
- multiple received packets coalesce into one pending ACK where protocol-correct;
- reverse data/control sent before the deadline piggybacks the ACK state and cancels/satisfies the standalone pending ACK when appropriate;
- polling before deadline emits nothing;
- polling after deadline emits at most the required ACK request(s), bounded by connection count;
- closed/reset connections do not leak pending ACK state.

### D4. Cumulative ACK and retransmission clearing

Correct sender tracking so:

- ACK 0 may acknowledge sequence-zero control when applicable;
- ACK 1 clears application sequence 1 unless NACKed according to the specification;
- NACKs preserve missing tracked packets while cumulative ACK advances over received higher sequences, if that is the reference behavior;
- duplicate ACKs are idempotent;
- impossible ACK/NACK combinations fail closed or are ignored according to the reference contract without deleting unsafely unacknowledged data.

### D5. NACK generation during reorder

The current source floor emits empty NACK vectors on ordinary data. Plan 130 must make reorder feedback understandable to conforming peers.

After receiving a later sequence while earlier sequence(s) are missing, generate ACK/NACK fields according to Phase A reference behavior. Bound NACK count by the existing protocol/configured ceiling; do not allocate from attacker-controlled gaps.

### Phase D acceptance

- [ ] A one-way stream can stop retransmitting without reverse application data.
- [ ] `ackThrough == 0` is handled correctly.
- [ ] ACK-only packet is never application data and never causes ACK-of-ACK.
- [ ] Reorder produces independently verified ACK/NACK semantics.
- [ ] Piggyback ACK suppresses an otherwise pending standalone ACK.

## 8. Phase E — make I2P destination-port routing authoritative

### E1. Remove the listener override ambiguity

Refactor `StreamingDestinationAdapter::receive()` so caller input cannot override the decoded ClientPayload destination port.

Preferred public shape:

```rust
StreamingDestinationAdapter::receive(
    recovered_i2np_bytes,
    owning_destination,
    streaming_manager,
    from_destination_hash,
    now_ms,
)
```

The adapter itself decodes:

```text
protocol
source_port
destination_port
streaming packet
```

and passes the wire ports into the Streaming manager.

If an additional registry object is genuinely required, it may identify the owning **Destination**, but it must not substitute an unrelated port.

### E2. Listener matching

Initial SYN dispatch must use the decoded `destination_port` and the exact Phase A port-0/default semantics.

Required typed outcomes/errors should distinguish at least:

- unsupported protocol;
- no matching listener / unacceptable destination port;
- malformed ClientPayload;
- port tuple mismatch on an established connection.

Do not auto-create a listener for an arbitrary unbound destination port merely because a SYN arrives. Existing code that calls `listeners.entry(port).or_default()` must be reviewed carefully; an inbound SYN should reach an actual listener/default route according to policy, not create one by side effect unless the normative API explicitly requires that behavior.

### E3. Persist connection port tuple

A `StreamingConnection` should retain the relevant local/remote I2P ports established by the SYN path. Subsequent client payloads must be checked against that tuple where current I2P semantics require it.

This protects future SAM/I2CP adapters from having to reconstruct port ownership externally.

### E4. Tests

Required tests include:

- exact destination port reaches exact listener;
- packet for port X cannot be queued into listener Y;
- configured port-0/default behavior matches reference semantics;
- source port zero is accepted/rejected according to current I2P rules rather than local TCP assumptions;
- established data with a wrong port tuple is rejected without corrupting connection state;
- unsupported protocol is still returned before Streaming processing.

### Phase E acceptance

- [ ] No caller-controlled listener port can override wire metadata.
- [ ] Future SAM can hand a recovered Data payload to the adapter without duplicating I2P port routing logic.

## 9. Phase F — preserve the live tunnel duplicate window in integration tests

### F1. Persistent inbound chain

Change the Plan 129/130 fixture so `Side` creates its inbound roles once for an established tunnel and ordinary `feed_action()` calls reuse them.

Remove ordinary-delivery behavior equivalent to:

```rust
receiver.inbound = InboundChain::new(receiver.seed);
```

before every action.

Provide an explicit fixture method such as `rebuild_inbound_tunnel_for_test()` only for tests that intentionally model tunnel replacement/expiration.

### F2. Separate duplicate layers

One integrated test must prove all three levels distinctly:

1. **Tunnel replay:** feed the exact same post-OBEP router-delivery/tunnel cells to the same live inbound roles twice; the second copy is rejected/suppressed by the persistent tunnel duplicate window and never reaches ECIES.
2. **ECIES/session replay:** where meaningful, replay a consumed NSR/ES tag and show the session manager rejects it without plaintext.
3. **Streaming duplicate:** freshly re-encrypt/reseal the same already-received Streaming sequence so it legitimately traverses the tunnel and ECIES layers again; Streaming suppresses duplicate application delivery.

Do not cite one of these mechanisms as evidence for another.

### Phase F acceptance

- [ ] Duplicate-window state survives ordinary deliveries in the fixture.
- [ ] Exact tunnel replay rejection is independently observable.
- [ ] Higher-layer duplicate exact-once behavior remains green.

## 10. Phase G — authoritative full-stack Plan 130 trajectories

Add or update tests so at least the following are closure evidence over the complete destination stack.

### G1. Fresh handshake and sequence transition

A fresh A->B stream:

```text
A SYN:              Streaming seq 0
                    bound ECIES New Session
                    full outbound/inbound tunnel path

B SYN response:     Streaming seq 0
                    ackThrough semantics independently correct
                    ECIES New Session Reply
                    full reverse tunnel path

A first data:       Streaming seq 1
                    Existing Session
                    full tunnel path

B second data or A second data:
                    sequence increments normally
```

Assert A remains `OutboundSynSent` until the authenticated response completes the reverse path.

### G2. One-way delayed ACK

After establishment:

1. A sends data sequence 1 to B through the complete stack.
2. B sends **no application data**.
3. Before the ACK deadline, `poll_acks()` emits nothing.
4. After the deadline, B emits one simple ACK request.
5. The ACK traverses gzip -> Data -> ES -> outbound tunnel -> OBEP -> seam -> inbound tunnel -> ECIES -> Data -> gzip -> Streaming.
6. A clears the corresponding retransmission/send-window entry.
7. Neither side receives application bytes from the ACK packet.
8. A does not retransmit the acknowledged data when the RTO expires later.

This test replaces the Plan 129 assumption that reverse app traffic is required as an ACK carrier.

### G3. Piggyback ACK

Deliver A data to B, then have B send application data before its standalone ACK deadline. The B packet must carry the correct ACK/NACK state, and no redundant simple ACK should be emitted afterward unless the protocol requires one.

### G4. Reorder + NACK convergence

Send A application sequences 1 and 2 but deliver sequence 2 first. Verify:

- B does not deliver sequence 2 prematurely;
- B's ACK/NACK state matches the independent fixture/reference;
- the feedback travels the full reverse destination stack;
- after sequence 1 arrives, B delivers application bytes in original order;
- A's retransmission state converges correctly;
- no unbounded gap/NACK generation occurs.

### G5. Port routing

Run the fresh SYN through the full stack with at least two listeners/default-routing configuration as appropriate. Assert the decoded destination port, not a fixture hint, selects the target. Wrong-port traffic must fail without appearing in another backlog.

### G6. Persistent tunnel replay

Use the same live inbound roles for an exact duplicate action and show the second tunnel delivery does not reach dispatcher/ECIES/application state. Then freshly reseal the same Streaming sequence and prove Streaming dedup independently.

### G7. Elligator production path

At least one full fresh NS and one NSR in the integrated trajectory must use the **production randomized** ephemeral generator, not the deterministic vector constructor. Assert the recovered handshake succeeds without making tests depend on a specific randomized representation byte string.

### G8. Retained Plan 129 fault coverage

Re-run or preserve:

- post-OBEP drop -> real retransmission;
- ECIES ciphertext tamper -> no plaintext;
- invalid Streaming signature after valid destination delivery -> reject;
- bad gzip CRC -> reject before Streaming;
- graceful CLOSE over full path;
- RESET over full path and unrelated stream survival;
- resource ceilings and queue bounds.

### Phase G acceptance

Every trajectory above passes without direct client-to-client transfer and without rebuilding protocol state simply to make the next packet acceptable.

## 11. Phase H — status and documentation synchronization

Only after all code/tests are green:

Update the authoritative status surfaces:

- `plans/126-status.md` — note that Plan 130 corrected the production Elligator representation while retaining Plan 126 cryptographic foundation;
- `plans/128-status.md` — note that Plan 130 corrected sequence/ACK semantics while retaining Plan 128 packet codec;
- `plans/129-status.md` — classify the historical gate as superseded by Plan 130 final closure;
- create/finalize `plans/130-status.md`;
- `plans/126-130-milestone6-final-corrective-roadmap.md`;
- README;
- AGENTS.md;
- `docs/protocol-support.md`;
- relevant architecture docs;
- `specs/support.toml`;
- ECIES/Streaming reference notes where implementation evidence changed.

Do not claim mixed-router interoperability.

Desired final authority:

```text
plan_119 = passed-leaseset2-protocol-foundation
plan_120 = passed-destination-lifecycle-and-pools
plan_121 = passed-corrected-ecies-destination-session-layer-local
plan_122 = passed-corrected-local-destination-routing
plan_123 = passed-corrected-streaming-wire-local
plan_124 = passed-corrected-destination-routing-local-closure
plan_125 = superseded-by-final-corrective-line
plan_126 = passed-ecies-destination-ratchet-corrective-foundation
plan_127 = passed-destination-session-routing-final-closure
plan_128 = passed-streaming-wire-protocol-corrective-closure
plan_129 = superseded-by-plan130-final-gate
plan_130 = passed-milestone6-final-wire-runtime-corrective-closure
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = may-continue
next_product_layer = SAM baseline planning (Milestone 7)
```

## 12. Required validation commands

Run from repository root with the pinned toolchain/lockfile:

```bash
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked -p i2pr-crypto --all-targets
cargo +1.95.0 test --locked -p i2pr-proto --all-targets
cargo +1.95.0 test --locked -p i2pr-client --all-targets
cargo +1.95.0 test --locked -p i2pr-tunnel --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
```

Run any existing fixture/vector checks whose inputs were touched. Do **not** add the retired rootless-supervisor/VM/live-router gates to Plan 130 acceptance.

The execution record must list exact test counts and any locally unavailable check distinctly. A missing external CI status is not equivalent to a test failure, but do not claim GitHub CI passed when no status exists.

## 13. Explicit closure criteria

Plan 130 passes only if every item below is true.

### ECIES / anonymity

- [ ] Production Elligator2 output uses reference-compatible randomized representation choices.
- [ ] No hand-written Elligator2 mapping was introduced.
- [ ] Java/i2pd-compatible decode evidence exists for randomized production representatives.
- [ ] Frozen deterministic Plan 126 KDF/ratchet vectors remain green or are narrowly versioned with justified provenance.
- [ ] Full NS -> NSR -> ES behavior remains green.

### Streaming wire/runtime semantics

- [ ] SYN and SYN response use sequence 0.
- [ ] First ordinary application packet uses sequence 1.
- [ ] A non-SYN sequence-zero simple ACK is never application data.
- [ ] `ackThrough == 0` is processed according to flags/packet semantics, not discarded solely because it is zero.
- [ ] A standalone delayed ACK is emitted for a one-way stream and traverses the full destination stack.
- [ ] ACK-only traffic does not create an ACK-of-ACK loop.
- [ ] Piggyback ACK behavior suppresses redundant standalone ACKs where appropriate.
- [ ] Reordering produces current-I2P-compatible bounded ACK/NACK feedback.
- [ ] Retransmission tracking clears only messages actually acknowledged under those semantics.

### I2P port routing

- [ ] Wire `destination_port` selects the listener/default route according to current I2P rules.
- [ ] No caller-supplied listener override can redirect the packet.
- [ ] Established port tuples are enforced where required.
- [ ] Wrong-port traffic cannot enter a different listener backlog or application stream.

### Integrated evidence

- [ ] Plan 129's full destination/tunnel topology remains the closure path.
- [ ] Inbound tunnel roles and duplicate window persist across normal fixture deliveries.
- [ ] Exact tunnel replay is rejected by the same live duplicate window.
- [ ] Freshly resealed duplicate Streaming sequence is suppressed separately at the Streaming layer.
- [ ] Drop/retransmit, reorder, corruption, CLOSE, RESET, and bounds remain green.
- [ ] No direct `VirtualWire<TransportSendRequest>` or equivalent shortcut is cited as closure evidence.
- [ ] No NTCP2/SSU2/live-network requirement was introduced.

### Authority

- [ ] Status/docs no longer contradict the Plan 130 result.
- [ ] `milestone6_local_product = passed` appears as current authority only after all criteria above pass.
- [ ] `milestone6_interoperable = not-yet-claimed` remains explicit.

## 14. Handoff after success

If Plan 130 passes, stop corrective Milestone 6 planning. The next product work is:

```text
Milestone 7 / SAM baseline planning
```

Do not reopen external transport validation as a prerequisite for SAM. Keep mixed-router ECIES/Streaming/tunnel evidence as separate acceptance debt that can be exercised when an environment capable of meaningful independent testing is available.
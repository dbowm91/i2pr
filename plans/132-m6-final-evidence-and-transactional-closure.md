# Plan 132 — Milestone 6 final evidence and transactional closure

## Status

**Ready for execution.**

- Date: **2026-08-27**.
- Source floor: `beb24fd945db3bcbc753c22c112cf3556a04dca6`.
- This plan is the **current Milestone 6 closure authority**. It narrowly reopens the local-product closure recorded by `plans/131-status.md` because the landed Plan 131 replay tests do not exercise the rejection layers they claim to prove, the new Elligator2 decoder accepts non-canonical representatives that deployed I2P decoders reject, and `StreamingManager::send_data()` still mutates the send window before all fallible wire construction has completed.
- Plan 131 remains the implementation baseline. Preserve its production Elligator branch randomization, connection-owned I2P ports, source-port-zero support, SYN-response tuple checking, corrected Streaming sequence/ACK behavior, and oversized-write prevalidation.
- If this pass is green, **stop Milestone 6 corrective work** and proceed directly to Milestone 7 / SAM baseline planning.

## 1. Objective

Close three remaining local correctness/evidence gaps without reopening transport interoperability or building more harness infrastructure:

```text
Elligator2 receive behavior
  = retain Plan 131 randomized production encoder
  + mask the two free high bits
  + reject non-canonical Elligator representatives before decode
  + match I2P/deployed-reference acceptance behavior

Replay evidence
  = exact same tunnel cell is replayed and rejected by the live tunnel window
  + exact same already-consumed ECIES ES ciphertext/tag is delivered in a fresh tunnel cell
    and rejected by ECIES/session processing before plaintext reaches Streaming
  + exact same Streaming sequence is freshly ECIES-sealed under a new valid session tag,
    traverses tunnel + ECIES successfully, and is deduplicated only by Streaming

Transactional send behavior
  = every fallible validation/encoding/signing step occurs before connection/send-window mutation
  + failed local sends leave sequence/window/retransmit/queue/connection state unchanged
```

This is a **local product closure only**. Mixed-router interoperability remains separate acceptance debt and is not a prerequisite for Milestone 7.

## 2. Why Plan 131 is reopened

### 2.1 The three replay trajectories are false-positive evidence

The landed `crates/i2pr-client/tests/plan131_trajectory.rs` names three replay layers, but the tests do not actually drive the retained artifact back into the intended rejection boundary.

#### Tunnel replay test defect

`plan131_exact_cell_replay_hits_tunnel_duplicate_window()` calls `send_data()` a second time and builds a second outbound plan instead of retaining and replaying the exact original tunnel cell. It then stops after `feed_action()` and does not assert a typed `TunnelRoleError::DuplicateCell` at the persistent inbound role.

An unchanged Streaming delivered counter therefore does not prove tunnel replay suppression.

#### ECIES consumed-tag test defect

`plan131_consumed_es_session_tag_replay_rejected_by_session_layer()` calls `send_via_adapter(&first_request, ...)` again. The adapter performs a fresh ECIES Existing Session seal and consumes a fresh outbound session tag, so this is **not** a replay of the consumed tag/ciphertext.

The test also stops after recovering bytes from the tunnel and never dispatches the recovered Garlic envelope through `DestinationDispatcher` / `EciesSessionManager`. The unchanged Streaming counter is therefore guaranteed even if ECIES replay protection were absent.

#### Streaming fresh-reseal test defect

`plan131_fresh_ecies_reseal_is_deduplicated_by_streaming()` extracts the Streaming packet and invokes `StreamingManager::process_inbound_packet()` twice directly. This proves sequence-window dedup in isolation, but bypasses destination routing, Garlic, ECIES, and tunnels. It does not prove that a fresh valid ECIES seal can carry an old Streaming sequence through the real local product path and be rejected only at Streaming.

Plan 132 must replace these with artifacts that are captured once and replayed at precisely the intended layer.

### 2.2 The Plan 131 Elligator decoder is more permissive than deployed I2P

Plan 131 correctly replaced the production encoder with the reviewed `elligator2 = 0.1.0` primitive and randomizes both the inverse-map branch and the two high representation bits.

However, current `decode_representative()` delegates directly to `elligator2::from_representative()`, whose API is a total reverse map for 32-byte inputs after the two free high bits are ignored. i2pr currently rejects all-zero input/output, but does **not** enforce the canonical Elligator representative range used by I2P before decoding.

Pinned reference behavior:

- Official ECIES spec: `https://i2p.net/en/docs/specs/ecies/`
  - `DECODE_ELG2`: mask the two high bits, then decode according to Elligator2.
- Current Java I2P `router/java/src/net/i2p/router/crypto/ratchet/Elligator2.java`:
  - masks byte 31 with `0x3f`;
  - interprets the representative as little endian `r`;
  - rejects `r >= (p - 1) / 2` before the map.
- Current i2pd `libi2pd/Elligator.cpp`:
  - masks the two high bits;
  - enforces the same lower-half representative domain before mapping (its current boundary expression should be recorded exactly in the reference note; do not silently choose a broader acceptance domain).

For Curve25519 `p = 2^255 - 19`, the Java/Elligator lower-half threshold is:

```text
(p - 1) / 2 = 2^254 - 10
```

The receive path must not provide a fingerprinting oracle in which i2pr accepts intentionally non-canonical representatives rejected by the major deployed implementations.

### 2.3 `send_data()` still commits before all fallible wire construction

Plan 131 moved payload/port/backpressure validation before `enqueue_send()`, fixing the original oversized-write bug. But the current order is still:

```text
enqueue_send()                 # mutates next sequence / unacked state
encode_streaming_packet()?     # fallible
encode_client_payload()?       # fallible
install retransmit record
queue request
```

If either encoding call returns `Err`, the API returns failure after consuming a sequence and mutating the send window, with no wire request or retransmit record to correspond to the mutation.

The source comment currently says a wire packet “still goes out” on encode failure, but the `?` operators return before request construction/queueing; that comment is incorrect.

The same mutation-before-fallible-build pattern is present in the adjacent established control APIs:

- `send_close()` may transition to `ClosingLocal` before `build_signed_packet()?`;
- `send_reset()` resets the connection before `build_signed_packet()?`.

Plan 132 must make this local send boundary transactional without creating a general transaction framework.

## 3. Hard scope boundaries

### In scope

- `crates/i2pr-crypto/src/ecies.rs` strict Elligator representative validation around the reviewed decoder;
- `specs/references/elligator2-production-representation.md` receive-domain/reference correction;
- `crates/i2pr-client/tests/plan131_trajectory.rs` correction or replacement of invalid replay claims;
- preferably a new focused `crates/i2pr-client/tests/plan132_trajectory.rs` so historical Plan 131 evidence remains inspectable;
- narrowly scoped test helpers that expose retained tunnel cells / recovered Garlic envelopes without bypassing production roles;
- `crates/i2pr-client/src/streaming/manager.rs` ordering of `send_data`, `send_close`, and `send_reset` so fallible wire construction precedes state mutation;
- minimal `StreamingConnection` / send-window inspection helpers only if required for deterministic assertions;
- `plans/131-status.md`, `plans/132-status.md`, protocol/support/architecture documentation synchronization **after** acceptance is green.

### Explicitly out of scope

Do **not** reopen or add:

- live NTCP2 or SSU2 activation;
- Plan 116/117 reference-router work;
- Emissary/i2pd/Java live interop harness construction;
- Docker, rootless namespaces, VMs, Multipass, QEMU, privileged setup;
- public I2P testing;
- Python protocol/interoperability harnesses;
- generalized fault-injection or transaction frameworks;
- SAM/I2CP implementation before this pass closes;
- HTTP/SOCKS proxies or service tunnels;
- new LeaseSet types, PQ ratchets, MetaLS2, EncryptedLeaseSet, or legacy ElGamal work;
- handwritten Elligator2 / finite-field arithmetic.

Preserve all corrected invariants from Plans 124/127/128/130/131, especially:

- outbound tunnels carry `garlic_i2np_bytes`, never plaintext inner Data;
- sender LS2/type-4 static-key binding and reverse routing remain unchanged;
- production ECIES encoder continues to randomize the deployed-reference inverse-map branch plus the two free high bits;
- deterministic Plan 126 ECIES vectors remain stable;
- SYN and SYN response own sequence 0; first application sequence is 1;
- plain ACK sequence 0 never enters the receive window;
- semantic ACK/NACK and delayed standalone ACK behavior remains unchanged;
- established I2P port tuples remain connection-owned;
- source port 0 remains valid I2P “unspecified”;
- persistent tunnel duplicate windows remain live across deliveries.

## 4. Phase A — strict Elligator2 receive-domain validation

### A1. Pin the exact canonical domain

Before implementation, update the Elligator reference note with:

1. the official ECIES `DECODE_ELG2` rule;
2. the exact Java I2P range check and source revision/path;
3. the exact current i2pd range check and source revision/path;
4. the selected i2pr acceptance rule and why it does not broaden deployed behavior.

For the Java/reference Elligator domain, after masking the free high bits:

```text
r must be strictly less than (p - 1) / 2
p = 2^255 - 19
threshold = 2^254 - 10
```

Treat the representative as **little endian**. Do not accidentally compare the raw byte array lexicographically in little-endian order.

The high two bits are not part of `r` and must not affect validity.

### A2. Add a narrow validator before `elligator2::from_representative`

Preferred shape:

```text
decode_representative(rep)
  -> reject existing forbidden all-zero representation as today
  -> copy bytes
  -> mask copy[31] &= 0x3f
  -> validate masked little-endian r is in I2P canonical domain
  -> reviewed elligator2::from_representative(masked/original-as-required-by-API)
  -> reject forbidden all-zero recovered key as today
  -> return X25519 Montgomery public key
```

The validator may use a fixed threshold constant or a small integer/byte comparison helper. This is bounds validation over a public encoding, **not** an invitation to implement Elligator field operations locally.

Do not add a bigint dependency solely for this comparison unless it is clearly simpler and justified. A reviewed fixed-width comparison is preferable.

Do not change the Plan 131 encoder or switch Elligator crates in this phase.

### A3. Required receive-domain tests

Add deterministic tests for at least:

1. all retained Plan 126/130/131 valid representatives still decode identically;
2. both deployed-reference inverse-map branch fixtures still decode to the same X25519 point;
3. all four legal high-bit variants of one valid representative are accepted and decode identically;
4. a representative whose **masked** `r == (p - 1)/2` is rejected;
5. masked `r > (p - 1)/2` is rejected;
6. the maximum masked 254-bit value is rejected;
7. changing only the two free high bits cannot turn an invalid canonical `r` into a valid one;
8. the typed result is `EciesError::ElligatorDecode` (or one narrowly named equivalent), not a panic;
9. a malformed New Session / New Session Reply carrying a non-canonical representative fails before successful DH/authentication and cannot install session state.

If Java and current i2pd differ by one exact boundary value, document the discrepancy and select the **stricter deployed-compatible** domain for i2pr unless the official Elligator definition unambiguously requires otherwise. Do not broaden acceptance merely to satisfy the dependency's total-map API.

### Phase A acceptance

- [ ] Production Elligator encoding from Plan 131 is unchanged.
- [ ] Non-canonical receive representatives are rejected before the reviewed inverse map is trusted as a protocol parser.
- [ ] Valid Java/i2pd branch/high-bit fixtures still decode exactly.
- [ ] No handwritten finite-field / Elligator transform is introduced.
- [ ] Invalid representations cannot create or advance an ECIES session.

## 5. Phase B — replace replay evidence with real layer-isolated trajectories

The tests must prove **where** a replay is rejected, not merely that no application bytes appeared.

### B0. Build one reusable artifact-preserving test seam

Refactor the Plan 129/130/131 test fixture only as much as necessary to separate these stages:

```text
TransportSendRequest
  -> StreamingDestinationAdapter::send
  -> OutboundDeliveryPlan
  -> outbound participant / OBEP
  -> RouterDeliveryAction containing the exact I2NP Garlic message
  -> inbound gateway creates tunnel cells with caller-supplied deterministic RNG
  -> persistent inbound participant/endpoint recovers exact I2NP Garlic bytes
  -> DestinationDispatcher / EciesSessionManager
  -> queued Data payload
  -> StreamingDestinationAdapter::receive / StreamingManager
```

Useful narrow helpers may include:

```text
obep_action_from_plan(...)
make_inbound_cells(action, rng_seed) -> Vec<OutboundCell>
run_inbound_cells_result(side, cells) -> Result<Option<Vec<u8>>, TunnelRoleError>
dispatch_recovered_garlic(side, bytes) -> InboundDispatchOutcome
```

Do not add a general test harness. The purpose is to retain and replay exact artifacts without accidentally resealing them.

### B1. Layer 1 — exact tunnel-cell replay

Required trajectory:

1. establish A -> B through the existing full local stack;
2. create one valid application packet and one outbound ECIES/tunnel delivery;
3. at B's inbound gateway seam, generate the inbound tunnel cell(s) **once** and retain their exact bytes;
4. process them through B's persistent inbound participant/endpoint and complete the first delivery;
5. replay the **same exact retained `OutboundCell` bytes** into the same persistent inbound role;
6. assert typed `TunnelRoleError::DuplicateCell` at the role/window that owns replay detection;
7. assert ECIES/dispatcher state did not advance during the rejected replay and no additional application bytes were queued.

Forbidden substitutes:

- no second `send_data()`;
- no second adapter seal;
- no regenerating a “similar” cell from the same logical message and calling that an exact replay;
- no proving this only through an unchanged Streaming delivered counter.

### B2. Layer 2 — consumed ECIES Existing Session tag/ciphertext replay

This leg must preserve the **exact sealed ES Garlic bytes** while refreshing only the lower tunnel wrapping.

Required trajectory:

1. establish the NS -> NSR -> ES session through the real destination path;
2. send one small application payload that uses `PlannedOutboundForm::ExistingSession`;
3. retain the exact I2NP Garlic message bytes / exact ES ciphertext+tag emitted by the outbound adapter/OBEP;
4. deliver it once through B's tunnel, dispatcher, ECIES manager, Data queue, and Streaming adapter;
5. assert the original application bytes are delivered exactly once and the ES session tag is consumed;
6. create a **fresh tunnel cell representation** around the exact same retained Garlic/I2NP bytes using a different deterministic inbound-gateway RNG seed (or another production-equivalent lower-layer freshness source);
7. assert the persistent tunnel replay window accepts the fresh cell as new lower-layer traffic and recovers bytes exactly equal to the retained Garlic message;
8. dispatch the recovered **same ES ciphertext/tag** through B's real `DestinationDispatcher` / `EciesSessionManager`;
9. assert session/ECIES rejection before any new plaintext Data payload is queued;
10. assert no second Streaming receive/delivery occurs and all relevant session/dispatcher queue counts remain unchanged.

For a direct session-manager assertion, the exact consumed `ExistingSessionMessage` must return `EciesSessionError::UnknownSessionTag` when submitted to `accept_existing_session()` again. The integrated dispatcher may structurally classify a sufficiently long unknown-tag ciphertext as a candidate New Session because ECIES has no explicit message-type byte; if so, it must still fail closed at ECIES authentication/structure and produce no plaintext. Record the exact typed integrated outcome rather than weakening the test.

Forbidden substitute:

```text
send_via_adapter(&same_request, ...)
```

for this leg, because that consumes a fresh outbound tag and is not a consumed-tag replay.

### B3. Layer 3 — fresh ECIES reseal carrying an old Streaming sequence

This leg intentionally does the opposite of B2: reuse the old Streaming request but require a fresh valid ECIES tag.

Required trajectory:

1. establish the stream and paired ECIES session;
2. create exactly one `TransportSendRequest` for application sequence `N`;
3. pass that request through the full adapter/tunnel/dispatcher/Streaming path once and deliver its application bytes;
4. retain the exact same `TransportSendRequest` / ClientPayload / Streaming packet bytes;
5. call `StreamingDestinationAdapter::send()` again on that retained request **after** the first ES tag has been consumed, using fresh deterministic RNG/lower-layer state as appropriate;
6. prove the second outbound envelope is a new valid Existing Session message with a **different session tag/ciphertext envelope** while the inner Streaming bytes and sequence `N` are identical;
7. pass the fresh seal through outbound tunnel, fresh inbound tunnel cells, dispatcher, ECIES and Data decoding successfully;
8. invoke the normal inbound Streaming adapter;
9. assert Streaming identifies sequence `N` as duplicate and application bytes surface exactly once total.

This test must not invoke `StreamingManager::process_inbound_packet()` directly for the second delivery.

### B4. Explicit negative-control assertions

Each replay test must prove that the earlier layers did **not** satisfy a later layer's test:

- B1: rejection happens before ECIES dispatch.
- B2: tunnel accepts the refreshed lower wrapping; rejection happens at ECIES/session processing before plaintext.
- B3: tunnel and ECIES both succeed; only Streaming suppresses the repeated sequence.

Use counters/queue lengths or typed outcomes at the boundaries. Do not use only “delivered bytes unchanged” as evidence.

### Phase B acceptance

- [ ] Exact same tunnel cell -> typed tunnel duplicate rejection.
- [ ] Same consumed ES ciphertext/tag in a fresh tunnel cell -> ECIES/session rejection and zero plaintext.
- [ ] Same Streaming sequence in a fresh valid ECIES seal -> tunnel+ECIES success, Streaming duplicate suppression.
- [ ] Each test actually drives the artifact into the rejection layer it names.
- [ ] No second mechanism can make the test pass accidentally.

## 6. Phase C — make established send APIs transactional

### C1. `send_data()` — encode before commit

Preferred ordering:

```text
lookup connection immutably
 -> validate connection state
 -> validate caller port assertions
 -> validate payload <= negotiated max
 -> validate send-window capacity
 -> planned_sequence = send_window.next_sequence()
 -> snapshot stream ids / ports / ACK+NACK view
 -> build StreamingPacket using planned_sequence
 -> encode_streaming_packet()?            # fallible, no mutation yet
 -> build ClientPayload
 -> encode_client_payload()?              # fallible, no mutation yet
 -> construct TransportSendRequest         # no mutation
 -> reacquire mutable connection
 -> enqueue_send(payload_len, now_ms)      # first protocol-state commit
 -> assert/verify returned sequence == planned_sequence
 -> install retransmit record
 -> queue request
 -> clear pending piggybacked ACK
 -> return request
```

Because the method owns `&mut self`, no external caller can mutate the same manager between pre-encoding and the commit. Do not add locks or a reservation framework.

If `enqueue_send()` can still fail after the immutable `evaluate()` precheck for an internal reason, verify that it is side-effect-free on failure. If necessary, add one narrow typed invariant error rather than panicking in production.

After the first commit point, every remaining operation must be infallible or have an explicit rollback. Prefer arranging code so no rollback is needed.

Correct the misleading source comment that says an encoded packet still goes out if encoding fails.

### C2. `send_close()` — build before state transition

Current `send_close()` validates the tuple/state, then may call `begin_close()` before `build_signed_packet()?`.

Refactor to:

1. immutably validate tuple and allowed state (`Established` or `ClosingRemote`);
2. snapshot stream IDs, next CLOSE sequence, ACK/NACK view, ports and whether this is a response to remote close;
3. build/sign/encode the complete CLOSE request while the connection state is unchanged;
4. only after successful request construction, transition `Established -> ClosingLocal` or complete the `ClosingRemote` response policy;
5. install retransmit tracking / outbound queue using the already-built request.

Do **not** change CLOSE flags, signature preimage, current sequence policy, or graceful-close semantics in this pass unless a deterministic test exposes an independent existing defect.

### C3. `send_reset()` — build before reset transition

Current `send_reset()` calls `conn.reset()` before `build_signed_packet()?`.

Refactor to:

1. validate tuple/state immutably;
2. snapshot stream IDs, ACK/NACK state and connection-owned ports;
3. build/sign/encode RESET completely;
4. only on success transition the connection to Reset, clear pending ACK state, and queue the request.

Do not change RESET wire flags/sequence semantics here.

### C4. Required transactional regressions

Retain the Plan 131 oversized-write snapshot test and add evidence that the architecture has no mutation-before-`Result` path:

1. oversized `send_data()` changes no sequence/window/retransmit/queue/connection state;
2. port-mismatch `send_data()` changes no state;
3. send-window backpressure changes no state;
4. a deterministic pre-commit packet/envelope encode failure, if naturally reachable through bounded public/test inputs, changes no state;
5. if such an encode failure is provably unreachable after validation, document that proof in the test/source and still retain the encode-before-commit ordering;
6. CLOSE port/state validation failure changes no connection state or queue;
7. RESET port/state validation failure changes no connection state or queue;
8. successful data/CLOSE/RESET requests remain byte-for-byte compatible with retained Plan 128/130 fixtures where frozen.

Do not introduce production-only failpoints to manufacture an encoding failure.

### Phase C acceptance

- [ ] `send_data()` has no fallible wire construction after sequence/send-window commit.
- [ ] `send_close()` has no fallible wire construction after close-state mutation.
- [ ] `send_reset()` has no fallible wire construction after reset-state mutation.
- [ ] Existing port ownership and source-port-zero behavior remain green.
- [ ] Existing retransmit, ACK/NACK, CLOSE/RESET and sequence behavior remains green.

## 7. Phase D — focused final regression gate

Add `crates/i2pr-client/tests/plan132_trajectory.rs` unless correcting the existing Plan 131 file is materially clearer. Prefer the new file so the inaccurate historical test evidence remains visible for audit rather than silently rewritten.

Minimum focused trajectories:

1. `plan132_exact_same_tunnel_cell_is_rejected_by_live_duplicate_window`
2. `plan132_consumed_es_ciphertext_rewrapped_in_fresh_tunnel_is_rejected_by_ecies`
3. `plan132_fresh_es_seal_of_same_streaming_sequence_reaches_streaming_and_deduplicates`
4. `plan132_send_data_failure_paths_are_precommit`
5. `plan132_close_reset_build_before_state_commit`
6. retained source-port-zero / wrong-SYN-response-port smoke coverage if API refactors touch those paths.

Crypto tests must include the strict canonical representative boundary cases from Phase A.

The complete local stack gate must still demonstrate:

```text
Streaming
 -> RFC1952 ClientPayload
 -> I2NP Data
 -> ECIES Garlic (NS -> NSR -> ES)
 -> outbound destination tunnel
 -> local router-link seam
 -> inbound destination tunnel with persistent roles
 -> dispatcher / ECIES
 -> I2NP Data
 -> RFC1952 ClientPayload
 -> Streaming
```

No test may transfer `TransportSendRequest` directly between peer `StreamingManager`s as a substitute for the destination stack.

## 8. Phase E — validation commands

Run the repository's pinned validation surface. At minimum:

```bash
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked -p i2pr-crypto
cargo +1.95.0 test --locked -p i2pr-proto
cargo +1.95.0 test --locked -p i2pr-client
cargo +1.95.0 test --locked -p i2pr-tunnel
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
```

Also rerun any retained repository checks required by the current source floor (including NTCP2 fixture/vector static checks if they are part of the standard repository gate), but **do not** turn those into renewed transport-development work.

No network, privileged-host, VM, Docker or public-I2P command is required for Plan 132.

## 9. Evidence integrity rules

Plan 132 exists partly because green tests were previously interpreted more strongly than the paths they executed. The executor must therefore follow these rules:

1. Test names must describe the artifact actually replayed.
2. A test claiming a typed rejection must assert that typed rejection directly.
3. A test claiming “full stack” must actually traverse every named layer.
4. A test claiming “fresh ECIES reseal” must demonstrate the session tag/envelope changed while inner Streaming sequence/bytes did not.
5. A test claiming “consumed-tag replay” must demonstrate the ECIES ciphertext/tag did **not** change while only the lower tunnel representation did.
6. “No extra application bytes” is a secondary assertion, never the sole proof of a replay layer.
7. Do not update closure/status language until the executable evidence is green.

## 10. Documentation and status synchronization

Only after Phases A–E are green:

- mark `plans/131-status.md` as historical / superseded by Plan 132;
- write `plans/132-status.md` as the current closure record;
- synchronize only the files that currently make Milestone 6/SAM-readiness claims, typically:
  - `README.md`;
  - `AGENTS.md`;
  - `docs/architecture/i2pr-crypto.md`;
  - `docs/architecture/i2pr-client.md`;
  - `docs/architecture/overview.md`;
  - `docs/protocol-support.md`;
  - `specs/references/elligator2-production-representation.md`;
  - `specs/references/streaming-packet-wire.md` if send ordering/API commentary is present there;
  - `specs/support.toml`;
  - relevant skill/status text that currently names Plan 131 as final authority.

Do not rewrite historical Plans 126–131 to pretend their original evidence was different. Their status files may be marked superseded, but the historical implementation record should remain inspectable.

## 11. Final acceptance criteria

Plan 132 passes only if **all** of the following are true.

### Elligator receive correctness

- [ ] Plan 131 production branch/high-bit randomization remains intact.
- [ ] Receive high bits are masked before canonical-domain evaluation.
- [ ] Non-canonical `r >= (p-1)/2` (or the stricter exactly pinned deployed boundary if reference reconciliation requires it) is rejected typed.
- [ ] Valid branch/high-bit reference fixtures still recover the exact intended X25519 point.
- [ ] Malformed representative input cannot install/advance ECIES state.
- [ ] No handwritten Elligator field arithmetic exists in i2pr.

### Replay evidence

- [ ] The exact same retained tunnel cell is replayed and returns typed duplicate-window rejection.
- [ ] The exact same consumed ES ciphertext/session tag, wrapped in fresh tunnel cells, reaches ECIES and is rejected before plaintext.
- [ ] The same Streaming sequence, freshly sealed under a new valid ES tag, passes tunnel+ECIES and is deduplicated by Streaming.
- [ ] Each trajectory asserts the intended rejection boundary directly.

### Transactional sends

- [ ] `send_data()` performs every fallible packet/envelope encoding before send-window mutation.
- [ ] Rejected data writes consume no sequence and leave all tracked state unchanged.
- [ ] `send_close()` builds/signs successfully before any close-state transition.
- [ ] `send_reset()` builds/signs successfully before reset-state transition.
- [ ] Existing successful wire fixtures and behavior remain unchanged.

### Regression / project boundary

- [ ] Plan 130 and still-valid Plan 131 tests remain green.
- [ ] Workspace fmt/check/test/clippy/doc gates are green.
- [ ] Dependency/runtime/fixture boundary checks are green.
- [ ] No transport activation, host-harness, VM, public-network, Python-harness, SAM or proxy work was added.
- [ ] Documentation does not claim mixed-router interoperability.

## 12. Required closure classification

Until Plan 132 passes, the repository authority must be interpreted as:

```text
plan_130 = superseded-by-plan131
plan_131 = corrective-reopened-by-plan132
plan_132 = ready-for-execution

milestone6_local_product = not-closed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = hold-for-plan132-local-closure
next = Plan 132
```

After all acceptance criteria pass, update authority to:

```text
plan_130 = superseded-by-plan131
plan_131 = superseded-by-plan132-final-evidence-and-transactional-gate
plan_132 = passed-milestone6-final-evidence-and-transactional-closure

milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = may-continue
next_product_layer = SAM baseline planning (Milestone 7)
```

At that point **stop correcting Milestone 6** unless a new concrete protocol defect is discovered. Do not demand external transport validation as a prerequisite for SAM. The remaining external interoperability debt is explicitly separate.

## 13. Handoff guidance

This should be one implementation pass, not another planning series.

Recommended execution order:

```text
A. strict Elligator receive-domain check + boundary fixtures
B. artifact-preserving replay helpers
C. three genuine replay trajectories
D. send_data/CLOSE/RESET precommit ordering
E. focused Plan 132 tests
F. full workspace regression
G. status/docs sync
H. close Milestone 6 and begin Milestone 7 planning
```

If a phase exposes an unrelated issue, fix it in this pass only when it is a direct correctness consequence of the touched code and can be closed narrowly. Otherwise record it as later debt rather than expanding Plan 132.

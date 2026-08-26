# Plan 131 — Milestone 6 final local correctness closure

## Status

**Ready for execution.**

- Date: **2026-08-26**.
- Source floor: `8f2b3dfe44b480beb7f411613b8d7089aaeb7a19`.
- This plan is the **current Milestone 6 closure authority** and supersedes the closure decision recorded by `plans/130-status.md` until the acceptance criteria below pass.
- Plan 130 remains the retained implementation baseline. Do **not** undo its corrected Streaming sequence/ACK behavior, wire-derived listener routing, persistent tunnel duplicate windows, or integrated destination topology.
- This is one narrow corrective pass. If it passes, stop Milestone 6 corrective work and proceed to Milestone 7 / SAM baseline planning.

## 1. Objective

Close the remaining local correctness gaps discovered by the post-Plan-130 audit without reopening external transport/interoperability work.

Required end state:

```text
ECIES Elligator2 production encoding
  = deployed-I2P-compatible randomized high bits
  + randomized recoverable alternative branch
  + unchanged canonical X25519 DH point

ECIES replay evidence
  = exact tunnel replay rejected by tunnel duplicate window
  + consumed NSR/ES session-tag replay rejected by ECIES/session layer
  + freshly resealed duplicate Streaming sequence rejected by Streaming

Streaming port ownership
  = connect/SYN establishes the I2P source/destination port tuple
  + subsequent SYN response/data/CLOSE/RESET derive from or validate against that tuple
  + source port 0 remains valid as I2P "unspecified"

Outbound rejection semantics
  = rejected application writes do not consume sequence numbers,
    populate retransmission state, or change outbound queues
```

No mixed-router or public-network claim is required for this local closure.

## 2. Why Plan 130 is reopened

Plan 130 materially corrected Milestone 6, but its final status was stronger than the landed evidence in four places.

### 2.1 Production Elligator2 still has a fixed alternative branch

At the source floor, `EciesEphemeralKeypair::generate()` draws fresh seed bytes and a tweak byte, but `build()` invokes:

```rust
RFC9380::to_representative(&canonical_seed, tweak & 0xc0)
```

Only the two high representation bits are randomized. The recoverable alternate inverse-map branch remains fixed.

The official current I2P ECIES specification explicitly requires 256-bit-looking representatives by ORing two CSPRNG bits into byte 31. Current deployed Java I2P and i2pd production encoders additionally select the recoverable alternative preimage branch unpredictably. Plan 130 itself required production output to avoid implementation-fixed high-bit/**alternative** behavior.

The Plan 130 reference note acknowledges the mismatch: i2pr always emits one canonical branch while accepting both reference branches.

This is therefore a narrow remaining on-wire fingerprint / reference-parity defect, not a reason to rewrite ECIES.

### 2.2 The integrated ECIES consumed-tag replay leg was not proven

Plan 130 required the three duplicate/replay layers to be demonstrated independently:

1. tunnel duplicate window;
2. ECIES consumed NSR/ES session tag;
3. Streaming sequence dedup after a fresh reseal.

The landed Plan 130 integrated trajectory proves (1) and (3), but does not exercise (2) as a distinct full-stack case. `plans/130-status.md` nevertheless says all three were proven.

The implementation may already reject consumed tags correctly. Plan 131 must prove it through the integrated product path rather than infer it from lower-level unit tests.

### 2.3 Established I2P ports are not fully connection-owned

Plan 130 fixed inbound listener redirection: `StreamingDestinationAdapter::receive()` now decodes source/destination ports from ClientPayload and no longer accepts a caller-provided listener override.

However, the manager still accepts caller-supplied `local_port` / `remote_port` for established outbound `send_data()`, `send_close()`, and `send_reset()` and uses those values to construct ClientPayload. The SYN-response receive branch also does not currently receive/validate the decoded ClientPayload port tuple before accepting the authenticated response.

For future SAM/I2CP callers, the established connection must be the authority for its I2P port tuple. Runtime callers should not have to repeatedly supply mutable copies of protocol state.

### 2.4 Oversized `send_data()` mutates state before returning an error

At the source floor, `send_data()` allocates/enqueues a sequence through `conn.enqueue_send()` before checking the negotiated maximum payload. A rejected oversized write can therefore consume a sequence and leave send-window state despite no wire packet being emitted.

Rejected writes must be side-effect-free with respect to sequence allocation, retransmission tracking, connection state, and outbound queue state.

## 3. Hard scope boundaries

### In scope

- `crates/i2pr-crypto/src/ecies.rs` Elligator2 representation generation seam;
- one narrowly justified Elligator2 dependency/API adjustment if required;
- `specs/references/elligator2-production-representation.md` correction;
- `crates/i2pr-client/src/streaming/manager.rs` established port ownership and side-effect ordering;
- narrowly related `StreamingConnection` helpers if needed;
- `crates/i2pr-client/src/streaming_adapter.rs` only if an API signature must change to preserve connection ownership;
- `crates/i2pr-client/tests/plan130_trajectory.rs` only where its retained evidence must be corrected;
- a focused `crates/i2pr-client/tests/plan131_trajectory.rs` for closure evidence;
- ECIES/session tests needed for consumed-tag replay;
- status/reference documentation synchronization after all acceptance tests pass.

### Explicitly out of scope

Do **not** reopen or add:

- live NTCP2 or SSU2 activation;
- Plan 116/117 host/reference-router work;
- Docker, rootless namespaces, VMs, Multipass, QEMU, privileged setup;
- public I2P testing;
- a Python interoperability harness or a general-purpose test framework;
- SAM/I2CP server implementation before Plan 131 closes;
- HTTP/SOCKS proxies or service tunnels;
- new LeaseSet types, PQ ratchets, MetaLS2, EncryptedLeaseSet, or legacy ElGamal paths;
- handwritten finite-field / Elligator2 arithmetic in i2pr.

Preserve these already-correct invariants:

- Plan 124 outbound tunnels carry `garlic_i2np_bytes`, never plaintext inner Data;
- Plan 127 sender LS2 binding / reverse-routing behavior;
- Plan 128 packet flags/options/signature rules;
- Plan 130 SYN=0, SYN response=0, first application sequence=1;
- Plan 130 plain-ACK seq0 control behavior;
- Plan 130 delayed standalone ACK / NACK-aware ACK behavior;
- Plan 130 persistent inbound tunnel duplicate-window state.

## 4. Phase A — pin the Elligator2 reference contract and choose a reviewed API

Before changing crypto code, update the reference note with the precise distinction between:

1. the official ECIES `ENCODE_ELG2()` requirement, which explicitly randomizes the two unused high bits; and
2. deployed Java I2P / i2pd behavior, which also uses one random bit to choose between the two valid recoverable preimage branches.

Pin the current sources/revisions used for the decision.

### A1. Required deployed behavior

The production path must emit both valid reference branch families over time while preserving the exact same decoded X25519 Montgomery public key.

Do not confuse this requirement with changing the X25519 key or transcript point.

### A2. Preferred implementation route

Use a reviewed library API that exposes the branch choice directly.

A concrete candidate to evaluate is the Rust `elligator2` crate's `to_representative(point, tweak)` API, where:

- the tweak's low bit selects the inverse-map branch;
- the tweak's high two bits populate the free representation bits;
- decoding returns the same Montgomery point.

This is a **candidate**, not permission for a blind dependency swap. Verify its mapping against frozen Java I2P / i2pd representatives and the existing Plan 126 vectors before adopting it.

The current `curve25519-elligator2::Randomized` mode is **not** an acceptable shortcut if it changes the derived DH public point. Plan 130 already identified this incompatibility.

Likewise, do not adopt any library's "dirty key" / torsion-added key-generation mode merely because it provides a stronger generic uniformity property. Milestone 6 requires deployed I2P ECIES parity. The canonical X25519 public point used by Noise/DH must remain the one current Java I2P/i2pd recover for the representative.

### A3. If no suitable reviewed API exists

Do not hand-write the missing branch transform.

Instead, the executor may make one narrowly scoped dependency choice, such as:

- replacing the current Elligator mapping dependency with a maintained/reviewed crate that exposes the deployed branch semantics; or
- using a public, reviewed primitive from an existing dependency if it can be proven byte-compatible.

Record license, version, unsafe-code posture, transitive dependency impact, and why the API is preferable to local cryptographic arithmetic.

### Phase A acceptance

- [ ] Official specification behavior and deployed-reference behavior are separately documented.
- [ ] The selected API maps both branch choices back to the exact same X25519 public point.
- [ ] No handwritten finite-field Elligator implementation is introduced.
- [ ] No non-I2P "stronger" Elligator variant silently changes the DH point.

## 5. Phase B — production Elligator branch randomization

Refactor only the ephemeral representation generation seam.

Desired semantics:

```text
production generate(rng)
  -> draw candidate X25519 ephemeral secret
  -> derive canonical X25519 public point
  -> draw one random branch bit + two random high bits
  -> reviewed Elligator inverse map
  -> representative decodes to the exact canonical public point
  -> retain secret + representative

deterministic vector constructor
  -> explicit deterministic branch/high-bit choice
  -> never used by production session establishment
```

The branch bit must be unpredictable for production NS and NSR ephemerals.

### Required tests

1. Existing deterministic Plan 126 KDF/Noise vectors remain byte-for-byte stable, unless a vector constructor must take an explicit branch argument; if so, preserve the historical branch as the default fixture choice.
2. For a fixed representable X25519 point, freeze one representative for each reference branch and prove both decode to the identical point.
3. Freeze/reference Java I2P and i2pd representatives for both branches where available and prove i2pr decodes them.
4. Over a deterministic seeded-CSPRNG sample of production generation, observe both branch classifications and all four high-bit values.
5. The production representative always decodes to `X25519(secret, basepoint)` used by the handshake.
6. A production NS and NSR complete the full Plan 130 destination handshake with either branch.
7. Entropy/retry failure remains bounded and typed.
8. Secrets remain redacted from `Debug`/logs.

The distribution test is only a fingerprint-regression test. Keep it deterministic and non-flaky; it is not a randomness certification.

### Phase B acceptance

- [ ] Production output no longer has an implementation-fixed branch.
- [ ] High-bit randomization remains correct.
- [ ] Both deployed reference branch families are accepted.
- [ ] Canonical Noise/DH public keys and existing NS -> NSR -> ES behavior remain unchanged.

## 6. Phase C — prove consumed-tag ECIES replay independently

Add a full-stack trajectory that isolates the session replay layer from both tunnel duplicate detection and Streaming sequence dedup.

### C1. Required shape

After a normal NS -> NSR establishment, send one Existing Session message through the complete destination stack and allow the receiver to consume its session tag.

Then replay the **same ECIES Existing Session bytes / same consumed session tag**, but put them into a fresh lower-layer delivery so the tunnel duplicate window does not reject the attempt first.

One acceptable pattern is:

```text
original Streaming request
  -> compose/seal ES Garlic bytes
  -> tunnel delivery #1
  -> receiver consumes ES tag and delivers once

same exact sealed ES Garlic bytes
  -> freshly construct/reseal only the lower I2NP/tunnel delivery container
     with a distinct tunnel-cell representation
  -> persistent inbound tunnel accepts it as a new cell
  -> ECIES session manager sees already-consumed tag
  -> typed/session-level rejection
  -> no Data plaintext reaches Streaming
```

Do **not** freshly ECIES-seal the second copy for this leg; that would create a new tag and test Streaming dedup instead.

### C2. Keep the three layers distinct

The final integrated evidence must explicitly contain:

1. exact cell replay -> `TunnelRoleError::DuplicateCell`, no ECIES invocation;
2. fresh tunnel cell carrying consumed ES/NSR tag -> ECIES/session rejection, no plaintext;
3. fresh ECIES reseal of an already-received Streaming sequence -> tunnel and ECIES succeed, Streaming suppresses duplicate application delivery.

Assert queue/counter state at each boundary so one mechanism cannot accidentally satisfy another mechanism's test.

### Phase C acceptance

- [ ] Consumed session-tag replay is independently observable through the integrated path.
- [ ] No recovered Data payload or application bytes result from the ECIES replay.
- [ ] Tunnel replay and Streaming duplicate evidence remain independently green.

## 7. Phase D — make established I2P ports connection-owned

The connection established by the SYN handshake owns its I2P port tuple.

### D1. Outbound data/control API

Preferred API after establishment:

```rust
send_data(connection_id, local_dest, remote, payload, now_ms)
send_close(connection_id, local_dest, remote, now_ms)
send_reset(connection_id, local_dest, remote, now_ms)
```

Derive ClientPayload source/destination ports from the `StreamingConnection`.

If retaining port parameters is necessary for compatibility, they must be treated only as assertions and rejected on mismatch **before any state mutation**. Do not use caller-supplied values as authoritative wire fields after establishment.

### D2. SYN response receive tuple validation

Thread the decoded ClientPayload `source_port` / `destination_port` through the SYN-response branch and validate against the outbound connection established by `connect()`.

For an outbound connection:

```text
connection.local_port  = original client source port
connection.remote_port = target server port

incoming SYN response:
  source_port      == connection.remote_port
  destination_port == connection.local_port
```

Reject a signed/authenticated SYN response whose ClientPayload tuple is wrong without transitioning the connection to `Established`.

### D3. Inbound accept response

`accept_inbound_syn()` already validates its supplied tuple against the stored connection. Prefer deriving the response tuple from the stored connection to eliminate duplicate caller state. If parameters remain, mismatch must fail before transition or outbound queue mutation.

### D4. I2P port-zero semantics

Follow I2CP semantics, not TCP assumptions:

- source port `0` is valid and means unspecified;
- destination listener port `0` is the default/wildcard listener;
- exact nonzero listener match wins over wildcard 0;
- ports 1-1023 are not privileged.

Required source-port-zero trajectory:

```text
A connects with local/source port 0 to B:PORT_B
B receives SYN source=0 destination=PORT_B
B accepts and responds source=PORT_B destination=0
A accepts the response and establishes
subsequent data uses the stored 0 <-> PORT_B tuple
```

### Phase D acceptance

- [ ] Runtime callers cannot alter established I2P ports on data/CLOSE/RESET.
- [ ] SYN response validates the wire tuple before establishment.
- [ ] `source_port == 0` works end-to-end.
- [ ] Exact listener then wildcard-port-0 behavior remains green.
- [ ] Wrong tuple rejection leaves connection and queues unchanged.

## 8. Phase E — make rejected outbound writes transactional

### E1. Fix oversized `send_data()` ordering

Validate negotiated payload size **before** calling any method that allocates a sequence or mutates send-window state.

Required order:

```text
lookup connection
 -> validate state
 -> validate payload <= negotiated max
 -> validate any remaining caller assertions
 -> validate send-window capacity
 -> allocate sequence / mutate send window
 -> encode ClientPayload
 -> install retransmit record
 -> enqueue outbound request
```

If encoding can still fail after sequence allocation, either make all remaining fallible validation occur first or provide a narrow rollback that restores the exact prior window state. Prefer validation-before-mutation.

### E2. Required regression

Capture before-state:

- `send_window.next_sequence()`;
- `send_window.unacked_count()` / bytes;
- manager tracked retransmit count;
- outbound queue length;
- connection state.

Attempt `MAX_NEGOTIATED_PAYLOAD + 1` bytes.

After `PayloadTooLarge`, every captured value must be identical.

Then send a valid packet and assert it receives the sequence number that would have been assigned before the rejected call. No gap is permitted.

### E3. Narrow mutation-before-validation audit

Review the touched established send APIs for the same class of bug. In particular, a port mismatch or obvious local validation failure must not transition CLOSE/RESET state before returning an error.

Do not turn this into a generic transaction framework.

### Phase E acceptance

- [ ] Oversized writes consume no sequence and create no send/retransmit/queue state.
- [ ] Invalid established-port assertions, if the API retains them, are side-effect-free.
- [ ] Valid traffic immediately following a rejected write has contiguous expected sequence numbers.

## 9. Phase F — authoritative Plan 131 closure trajectories

Create `crates/i2pr-client/tests/plan131_trajectory.rs` unless a smaller placement is clearer.

At minimum, closure evidence must include:

### F1. Elligator reference parity

Fresh NS and NSR production handshakes with deterministic seeded RNGs that select different branch values. Assert both establish and decode to their intended X25519 points.

### F2. Three-layer replay separation

One trajectory demonstrating tunnel duplicate, ECIES consumed-tag replay, and Streaming duplicate as three distinct rejection points.

### F3. Source-port-zero connection

Full-stack SYN -> SYN response -> data with client source port 0 and exact server listener.

### F4. SYN-response wrong-port rejection

Deliver a correctly signed/authenticated SYN response inside a ClientPayload with a wrong source or destination port. It must fail with `PortTupleMismatch`; originator remains `OutboundSynSent`; no application delivery/state corruption.

### F5. Established API port ownership

Send data/CLOSE/RESET through connection-owned ports. If old port-taking APIs remain, attempt a mismatched assertion and prove failure before mutation.

### F6. Oversized write rollback/no-op

Prove state snapshots are identical before/after rejection and the next valid application packet gets the expected sequence.

### F7. Retained Plan 130 suite

All Plan 130 trajectories remain green, including:

- SYN 0 -> response 0 -> data 1/2;
- one-way delayed ACK;
- piggyback suppression;
- reorder/NACK convergence;
- exact/wildcard listener routing;
- plain-ACK loop freedom;
- persistent tunnel replay;
- production ECIES establishment.

Also preserve Plan 129 fault coverage: drop/retransmit, corruption, CLOSE, RESET, bounds.

## 10. Required validation

Run from repository root using the pinned lockfile/toolchain:

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

If the Elligator dependency changes, additionally:

- review `Cargo.lock` delta;
- run the full crypto tests with all relevant features;
- record dependency count/feature changes;
- verify no unexpected `unsafe` allowance or native build dependency entered the product graph;
- re-run every frozen Plan 126 ECIES vector.

Do not add retired host/live-router gates to Plan 131 acceptance.

## 11. Explicit closure criteria

Plan 131 passes only if every item below is true.

### ECIES representation / anonymity

- [ ] Production high bits remain randomized.
- [ ] Production branch selection is no longer fixed and matches deployed Java I2P/i2pd semantics.
- [ ] Both branch families decode to the exact intended X25519 public point.
- [ ] No handwritten Elligator finite-field arithmetic exists in i2pr.
- [ ] No nonstandard dirty/torsion public-key scheme is introduced merely to improve generic uniformity.
- [ ] Existing NS -> NSR -> ES vectors/lifecycle remain green.

### ECIES replay

- [ ] Exact tunnel replay is still rejected by the live duplicate window.
- [ ] A fresh tunnel delivery carrying an already-consumed NSR/ES session tag is rejected at the ECIES/session layer.
- [ ] No plaintext/Data/Application payload emerges from that replay.
- [ ] A fresh ECIES reseal of the same Streaming sequence reaches Streaming and is deduplicated there.

### Streaming ports

- [ ] Wire ports establish a persistent connection-owned tuple.
- [ ] Established data/CLOSE/RESET cannot be redirected by caller-supplied ports.
- [ ] SYN response validates its decoded ClientPayload tuple before transitioning Established.
- [ ] Source port 0 is accepted as valid I2P "unspecified" traffic.
- [ ] Destination port 0 wildcard/default semantics remain correct.
- [ ] Wrong-port failures are typed and side-effect-free.

### Outbound rejection safety

- [ ] Oversized `send_data()` checks the negotiated maximum before sequence allocation.
- [ ] A rejected write changes no sequence/window/retransmit/queue/connection state.
- [ ] The next valid message gets the correct contiguous sequence.

### Integrated product

- [ ] Plan 124 `garlic_i2np_bytes` invariant remains intact.
- [ ] Plan 127 sender-LS2 binding and reverse routing remain intact.
- [ ] Plan 130 sequence/ACK/NACK behavior remains intact.
- [ ] Full destination/tunnel topology remains the closure path; no direct client-to-client shortcut is cited as acceptance evidence.
- [ ] No external transport/live-network requirement was introduced.

### Authority

- [ ] `plans/130-status.md` is retained as historical Plan 130 evidence but marked superseded/reopened by Plan 131.
- [ ] Plan 131 gets a final status record listing exact validation results.
- [ ] README, AGENTS.md, architecture/protocol-support docs, skills, and `specs/support.toml` are synchronized only after green acceptance.
- [ ] `milestone6_local_product = passed` is restored as current authority only after all Plan 131 criteria pass.
- [ ] `milestone6_interoperable = not-yet-claimed` remains explicit.

## 12. Desired final authority

After successful execution:

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
plan_130 = superseded-by-plan131-final-local-correctness-gate
plan_131 = passed-milestone6-final-local-correctness-closure
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = may-continue
next_product_layer = SAM baseline planning (Milestone 7)
```

## 13. Handoff after success

If all Plan 131 acceptance criteria pass, **Milestone 6 is locally closed**. Do not create another corrective Milestone 6 plan for unavailable mixed-router evidence.

Proceed directly to:

```text
Milestone 7 — SAM baseline planning / implementation
```

Keep independent-router ECIES/Streaming/tunnel interoperability as separate acceptance debt for an environment that can actually exercise it. Do not block SAM on the retired NTCP2/rootless/VM/harness lanes.
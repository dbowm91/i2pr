# Plan 116 final closure: wire real established state and prove the complete local tunnel trajectory

## Status

- **Ready for execution**.
- Date: 2026-08-18.
- Milestone: **5 — Network tunnel data plane and exploratory tunnels**.
- Implementation floor: `78f1024c47ca5ba110656e9fe2936ca2719c319f`.
- Parent plan: [`116-local-tunnel-data-plane.md`](116-local-tunnel-data-plane.md).
- Prior corrective pass: [`116-completion-correction.md`](116-completion-correction.md).
- Current status authority: [`116-status.md`](116-status.md).
- Plan 117 remains **blocked until this final local closure pass is green**.

This is the terminal Plan 116 closure pass. It is intentionally narrow.

The previous corrective implementation fixed the major Tunnel Message and AES defects, restored the previously ignored tests, added a real secret-bearing pool entry type, and implemented substantial local role machinery. Four closure-level gaps remain:

1. successful `ShortBuildStateMachine` state does not yet transfer its retained real `LayerKeys` / path metadata into `EstablishedMaterial` and the pool;
2. public pool APIs can still manufacture fake established entries using placeholder peers and keys;
3. automatic first-fragment capacity arithmetic is incorrect and is not tested through real cell construction/reassembly;
4. the test named `outbound_to_inbound_tunnel_trajectory` stops at OBEP routing metadata instead of traversing IBGW -> inbound participant(s) -> local inbound endpoint and proving exact byte recovery.

One tightly coupled fragmented-delivery defect must be corrected at the same time: the OBEP currently loses the first fragment's delivery instruction and synthesizes a LOCAL action when a fragmented message completes. A fragmented ROUTER or TUNNEL trajectory cannot be correct while that behavior remains.

No external interoperability work is needed or permitted for this pass.

---

# 1. Hard scope lock

This pass is local, Rust-only, deterministic, runtime-neutral, and transport-neutral.

Do **not** perform or require:

- Emissary/i2pd/Java runtime validation;
- NTCP2 correction or activation;
- SSU2 work;
- Q1/Q2 external-delivery validation;
- rootless/privileged namespaces;
- Docker, Multipass, QEMU, or VMs;
- Python interoperability harness work;
- public I2P network access;
- generic router dispatch construction;
- garlic, LeaseSet, streaming, SAM, I2CP, HTTP, or SOCKS work;
- Plan 117 implementation;
- a new tunnel crate;
- a second Tunnel Message implementation;
- broad refactors unrelated to the five closure defects above.

If an unrelated defect is noticed, record it for later unless it directly prevents the acceptance tests in this plan from passing.

The existing module layout is retained:

```text
crates/i2pr-tunnel/src/short.rs
crates/i2pr-tunnel/src/short_state.rs
crates/i2pr-tunnel/src/established.rs
crates/i2pr-tunnel/src/pool.rs
crates/i2pr-tunnel/src/data.rs
crates/i2pr-tunnel/src/fragment.rs
crates/i2pr-tunnel/src/roles.rs
```

---

# 2. Current implementation facts

At the implementation floor:

- `ShortBuildStateMachine` owns the validated `ShortBuildPath` in `self.path`;
- `prepare()` stores one `HopCryptoContext` per real hop in `self.contexts`;
- every `HopCryptoContext` owns the corresponding derived `LayerKeys`;
- `complete_build()` sets `StatePhase::Established` only after every real hop returned `ShortResponseCode::Accepted`;
- the established outcome exposes only creator tunnel ID + reply plaintext summaries;
- `EstablishedTunnel` / `EstablishedMaterial` already represent the data-plane secret owner;
- `ShortBuildRegistrar::admit_material()` already inserts a supplied `EstablishedMaterial` via `register_*_with_material()`;
- `ShortBuildRegistrar::admit()` is still a legacy non-material API and currently returns a synthetic `Duplicate` result without pool mutation;
- `ExploratoryPool::register_inbound()` / `register_outbound()` are public and currently build placeholder established material;
- `OutboundGatewayRole::fragment()` already composes `fragment_complete_message()` with `build_cells()`;
- the current cross-tunnel test never executes inbound role processing.

This means the required fixes are connections and correctness repairs, not new architecture.

---

# 3. Closure defect F1 — real state-machine material transfer

## Goal

Create one canonical success-only path:

```text
ShortBuildStateMachine reaches Established
 -> consume retained per-hop LayerKeys + validated ShortBuildPath metadata exactly once
 -> EstablishedTunnel
 -> EstablishedMaterial
 -> ShortBuildRegistrar::admit_material
 -> ExploratoryPool::register_*_with_material
 -> real TunnelSlot
```

No caller should have to reconstruct layer keys or established routing state outside the state machine.

## 3.1 Add a typed extraction error

Extend `ShortBuildConstructionError` or introduce a narrowly scoped extraction error with at least:

```text
NotEstablished
EstablishedMaterialAlreadyTaken
EstablishedPathStateInvalid
```

Do not overload `InvalidEvent` with ambiguous strings if a typed state is practical.

## 3.2 Add success-only extraction on `ShortBuildStateMachine`

Preferred API:

```rust
pub fn take_established_material(
    &mut self,
    established_at_seconds: u64,
) -> Result<EstablishedMaterial, ShortBuildConstructionError>
```

Required behavior:

- succeeds only when `self.state == StatePhase::Established`;
- fails before `Established`;
- may succeed exactly once;
- a second call fails with a typed already-taken error;
- consumes/moves the data-plane layer keys out of `self.contexts`;
- after success, the state machine does not retain a second live copy of the established `LayerKeys`;
- preserves real hop order from `self.path.hops`;
- preserves every hop's exact `router_hash`, `receive_tunnel`, role, and configured forwarding target;
- does not expose reply keys, garlic keys, raw request/reply records, or transcript state to the pool.

### 3.2.1 Move keys rather than clone them into long-lived state

`HopCryptoContext` wraps a zeroizing prepared context. If Rust's drop semantics prevent moving `inner.layer_keys` directly, add an internal consuming/take helper, for example:

```rust
fn take_layer_keys(&mut self) -> LayerKeys
```

implemented using `std::mem::replace()` with a zero-value `LayerKeys` placeholder that is immediately dropped with the old context.

Do not solve the ownership seam by cloning every `LayerKeys` into the pool while retaining the originals indefinitely.

Short-lived clones already needed by reply processing are not the target of this rule. The post-success persistent owner must be singular.

## 3.3 Build `EstablishedHop` directly from path + context

For each canonical hop index `i`, combine:

```text
self.path.hops[i]
self.contexts[i]
```

Verify the context hop index matches `i` before extracting.

### Outbound mapping

A validated outbound path is:

```text
Participant* -> OutboundEndpoint
```

For every nonterminal hop:

```rust
EstablishedHop::with_next(
    peer = path.hops[i].router_hash,
    role = Participant,
    receive_tunnel = path.hops[i].receive_tunnel,
    layer_keys = extracted context keys,
    next = EstablishedNextHop {
        router = path.hops[i + 1].router_hash,
        tunnel = path.hops[i].next_tunnel,
    },
)
```

For the terminal OBEP:

```rust
EstablishedHop::terminal(... OutboundEndpoint ...)
```

Do not carry the short-build reply router into the TunnelData next-hop state. OBEP data-plane delivery is message-specific and remains encoded in the Tunnel Message delivery instruction.

### Inbound mapping

A validated inbound path is:

```text
InboundGateway -> Participant*
```

Every remote inbound hop has a next-hop because the final remote hop forwards to the local creator endpoint.

For hop `i < last`:

```text
next.router = path.hops[i + 1].router_hash
next.tunnel = path.hops[i].next_tunnel
```

For the terminal remote inbound hop:

```text
next.router = path.originator_hash
next.tunnel = path.hops[last].next_tunnel
```

The local inbound receive tunnel ID is therefore:

```text
path.hops[last].next_tunnel
```

The inbound external gateway is:

```text
router = path.hops[0].router_hash
receive_tunnel = path.hops[0].receive_tunnel
```

The local creator endpoint is not added to the remote hop vector.

## 3.4 Construct and extract the existing established owner

Create:

```rust
EstablishedTunnel::new(
    direction,
    path.creator_tunnel_id,
    hops,
    established_at_seconds,
    inbound_gateway,
    local_inbound_receive,
)?
.into_extracted()
```

Do not duplicate an independent established-state validator in `short.rs`. Reuse `EstablishedTunnel::new()` for final topology validation.

## 3.5 Make the canonical registrar path explicit

After a caller receives terminal success:

```text
machine.take_established_material(now_seconds)?
 -> registrar.admit_material(material, now_seconds)?
```

Add a focused helper if that improves correctness, e.g.:

```rust
ShortBuildRegistrar::admit_established_machine(&mut machine, now_seconds)
```

but do not create a second material representation.

## 3.6 Remove the fake success semantics from legacy `admit()`

Current behavior is unacceptable:

```text
Established outcome
 -> RegisterOutcome::Duplicate { slot derived from creator tunnel id }
 -> pool unchanged
```

Choose one of these closure-safe options:

### Preferred

Remove the legacy `admit()` surface if no production caller needs it.

### Acceptable compatibility option

Keep it temporarily but make `ShortBuildOutcome::Established` return a typed error such as:

```text
ShortRegistrarError::EstablishedMaterialRequired
```

It must never report successful insertion/duplicate semantics when it has no material and has not mutated the pool.

Do not fabricate a `TunnelSlot` from `TunnelId`.

---

# 4. F1 acceptance tests

Add active tests proving:

1. `take_established_material()` before terminal success fails;
2. a real successful outbound short-build trajectory can take material once;
3. the second take fails;
4. extracted outbound hop count/order/roles/router hashes/receive IDs match the path;
5. extracted outbound nonterminal `next` router/tunnel pairs match the path;
6. outbound OBEP `next == None`;
7. a real successful inbound trajectory extracts `[IBGW, Participant*]` only;
8. inbound first-hop gateway router + receive ID match the path;
9. inbound local receive equals the terminal remote hop's `next_tunnel`;
10. state-machine contexts no longer retain the extracted persistent layer keys after a successful take;
11. `admit_material()` inserts the material into the correct pool direction;
12. pool length increments;
13. returned `TunnelSlot` comes from the pool's monotonic slot allocator;
14. `pool.established(slot)` returns the same topology/key-bearing entry;
15. removal/expiry drops the entry;
16. the legacy `admit()` cannot claim a successful registration without material.

The strongest test should execute the existing real short-build success path rather than directly constructing `EstablishedMaterial` by hand.

---

# 5. Closure defect F2 — eliminate production placeholder established entries

## Goal

After this pass, no production API may create `TunnelState::Established` without real build-derived `EstablishedMaterial`.

At the implementation floor the public methods:

```rust
ExploratoryPool::register_inbound(...)
ExploratoryPool::register_outbound(...)
```

construct placeholder material through `build_placeholder_established()` using zero router hashes and synthetic keys.

That violates the pool ownership invariant.

## 5.1 Restrict metadata-only registration to tests

Preferred shape:

```rust
#[cfg(test)]
impl ExploratoryPool {
    fn register_test_inbound(...)
    fn register_test_outbound(...)
}
```

or migrate existing unit tests to construct deterministic `EstablishedMaterial` fixtures and call the real material-bearing APIs.

If test helpers must remain reusable across crate test modules, expose them under a `#[cfg(test)] pub(crate)` module, never as normal public production methods.

## 5.2 Remove placeholder material from production compilation

`build_placeholder_established()` must either:

- be deleted; or
- be compiled only under `#[cfg(test)]`.

There must be no production code path that inserts placeholder zero-hash peers or synthetic layer keys as an established tunnel.

## 5.3 Keep registration metadata read-only

The pool may continue to expose cloned `TunnelRegistration` snapshots for diagnostics/reply-path selection.

Do not reintroduce a metadata-only production insertion path.

## 5.4 Duplicate/full semantics with material

Because `register_*_with_material()` consumes `EstablishedMaterial`, ensure:

- duplicate detection occurs before insertion;
- rejected material is dropped/zeroized;
- full pool rejection does not create an orphan entry;
- `next_slot` behavior remains deterministic.

It is acceptable for a rejected material object to be dropped after validation; it must not remain retrievable.

---

# 6. F2 acceptance tests

Require active tests proving:

1. all production pool insertions of established tunnels require `EstablishedMaterial`;
2. no `build_placeholder_established` symbol is compiled outside tests;
3. duplicate material insertion leaves one entry;
4. pool-full material insertion leaves existing entries unchanged;
5. rejected material is not retrievable afterward;
6. `select_inbound_reply_path()` still returns the real first remote IBGW router + receive ID;
7. old metadata-only pool behavior needed by tests is preserved only through test helpers.

A source check in the closure record should include:

```bash
rg -n 'build_placeholder_established|pub fn register_inbound\(|pub fn register_outbound\(' crates/i2pr-tunnel/src/pool.rs
```

Any remaining match must be demonstrably `#[cfg(test)]` or otherwise non-production.

---

# 7. Closure defect F3 — correct automatic fragment sizing

## Goal

`TunnelMessageBuilder::fragment_complete_message()` must generate records that actually fit through `build_cells()` for every delivery mode and boundary length.

The decrypted TunnelData body budget after checksum + delimiter is:

```text
1008 - 4 checksum - 1 zero delimiter = 1003 bytes
```

Record overheads are exact and include **all** control / addressing / message-id / length fields.

## 7.1 Canonical overhead table

### Unfragmented

```text
LOCAL  = control 1 + size 2                         = 3
ROUTER = control 1 + router 32 + size 2            = 35
TUNNEL = control 1 + tunnel 4 + router 32 + size 2 = 39
```

### Fragmented first

```text
LOCAL  = control 1 + message_id 4 + size 2                         = 7
ROUTER = control 1 + router 32 + message_id 4 + size 2            = 39
TUNNEL = control 1 + tunnel 4 + router 32 + message_id 4 + size 2 = 43
```

### Follow-on

```text
control 1 + message_id 4 + size 2 = 7
```

Therefore maximum body sizes per cell are:

```text
unfragmented LOCAL   1000
unfragmented ROUTER   968
unfragmented TUNNEL   964
first LOCAL           996
first ROUTER          964
first TUNNEL          960
follow-on              996
```

The global complete I2NP maximum remains the existing repository/spec limit. Do not raise it merely because the fragment sequence space has theoretical room.

## 7.2 Implement one canonical overhead helper

Avoid duplicating arithmetic between `fragment_complete_message()` and `encode_record()`.

Preferred internal helpers:

```rust
fn unfragmented_overhead(delivery: &DeliveryInstruction) -> usize
fn fragmented_first_overhead(delivery: &DeliveryInstruction) -> usize
const FOLLOW_ON_OVERHEAD: usize = 7;
```

or one typed helper that derives the encoded overhead from fragment kind + delivery.

Use checked/saturating arithmetic where appropriate and preserve typed errors.

## 7.3 Generated fragments must be constructible

For a large message:

```text
fragment_complete_message
 -> build_cells
 -> parse every cell
 -> BoundedReassembler
 -> exact original message
```

The test may deliberately shuffle the cell/fragment order after parsing as long as the first-fragment delivery metadata is retained correctly.

## 7.4 Boundary tests for all delivery modes

For LOCAL / ROUTER / TUNNEL test at least:

```text
max unfragmented size - 1
max unfragmented size
max unfragmented size + 1
first-fragment exact capacity
multi-follow-on message
repository maximum message size
repository maximum + 1 => reject
```

Every accepted output from `fragment_complete_message()` must pass `build_cells()`.

No test may stop at checking only the enum variants/fragment count.

---

# 8. Closure defect F4 — preserve delivery metadata across fragmented reassembly

## Goal

A fragmented first record owns the delivery instruction for the complete message. Follow-on fragments do not repeat it. The endpoint must therefore retain that delivery instruction in partial-message state until reassembly completes.

Current OBEP behavior discards it and calls an `action_from_unspecified()` fallback that synthesizes LOCAL delivery.

That fallback must be removed.

## 8.1 Extend reassembly metadata cleanly

Choose one cohesive design:

### Preferred

Let endpoint-side partial state store:

```text
ReassemblyKey
DeliveryInstruction from first fragment
first-fragment message metadata needed for final action
fragment bodies
```

This may be implemented by:

- extending `BoundedReassembler` with an opaque/small metadata value; or
- adding a small endpoint-owned map keyed identically to the reassembler, bounded and expired in lockstep.

Prefer a single owner over parallel state if possible.

Do not copy raw complete message payloads merely to retain delivery metadata.

## 8.2 Required semantics

- fragmented ROUTER completes as ROUTER to the original target router;
- fragmented TUNNEL completes as `TunnelGateway` action to the original gateway router/tunnel;
- fragmented LOCAL completes as LOCAL only where LOCAL is valid;
- follow-on fragments arriving before the first may be retained within existing bounds, but no completed action is emitted until the first-fragment delivery instruction is known;
- conflicting duplicate first-fragment delivery metadata invalidates only that partial message;
- expiry removes retained delivery metadata with the partial body state.

Delete `action_from_unspecified()` if it becomes unused.

---

# 9. F3/F4 acceptance tests

Require active tests proving:

1. automatic fragmentation LOCAL -> cells -> parse -> reassemble exact bytes;
2. automatic fragmentation ROUTER -> endpoint -> correct ROUTER action + exact bytes;
3. automatic fragmentation TUNNEL -> endpoint -> correct target gateway router/tunnel + exact bytes;
4. out-of-order follow-ons reassemble within bounds;
5. exact duplicate follow-on is idempotent;
6. conflicting duplicate invalidates the partial;
7. first-fragment delivery metadata survives until final completion;
8. no completed fragmented action uses a synthetic/unspecified LOCAL fallback;
9. expiry removes both body and delivery metadata;
10. every boundary length generated by `fragment_complete_message()` successfully encodes through `build_cells()`.

---

# 10. Closure defect F5 — execute the complete outbound-to-inbound trajectory

## Goal

Replace the current metadata-only cross-tunnel test with a true local data-plane traversal:

```text
original standard I2NP bytes
 -> local OutboundGatewayRole
 -> outbound participant(s)
 -> OutboundEndpointRole
 -> RouterDeliveryAction::TunnelGateway
 -> construct canonical TunnelGatewayMessage
 -> InboundGatewayRole
 -> inbound participant(s)
 -> LocalInboundEndpointRole
 -> exact original standard I2NP bytes
```

This is the terminal Plan 116 acceptance test.

## 10.1 Use actual role code

Do not manually apply layer transforms in the terminal trajectory except where the production role API itself requires setup.

The test must instantiate and call:

```text
OutboundGatewayRole
OutboundParticipantRole(s)
OutboundEndpointRole
InboundGatewayRole
InboundParticipantRole(s) when present
LocalInboundEndpointRole
```

A one-remote-hop inbound tunnel (`IBGW -> local endpoint`) may be used for one simple test, but at least one terminal acceptance trajectory must include an inbound participant so both inbound remote roles are exercised.

## 10.2 OBEP -> TunnelGateway seam

When the OBEP emits:

```rust
RouterDeliveryAction {
    kind: RouterDeliveryKind::TunnelGateway,
    target_router,
    tunnel_id: Some(id),
    message: original_standard_i2np_bytes,
    ...
}
```

construct the existing canonical `i2pr_proto::TunnelGatewayMessage` using that router/tunnel target and parse/encode the nested standard I2NP message through the existing codec boundary as required by the current API.

Do not bypass `TunnelGatewayMessage` by feeding raw bytes directly into the inbound gateway role.

## 10.3 Inbound gateway

`InboundGatewayRole::process()` must:

- validate the target tunnel ID;
- fragment LOCAL delivery as required;
- choose fresh randomness from the injected test CSPRNG;
- apply the IBGW forward layer;
- emit TunnelData to its configured next router/tunnel.

For a message requiring multiple inbound TunnelData cells, extend the role API rather than silently truncating/rejecting if the closure test needs it. Keep the simplest correct API: a `fragment/process_many` counterpart is preferable to embedding hidden queues.

## 10.4 Inbound participants

For each inbound participant:

```text
validate receive tunnel
validate previous peer lock
validate duplicate token
participant_forward
rewrite next tunnel id
```

Pass the emitted cell to the next role.

## 10.5 Local inbound endpoint

The local endpoint must:

- receive on the explicit creator-side local inbound tunnel ID;
- apply the inverse chain over all remote inbound hop keys in reverse path order;
- parse the recovered Tunnel Message using the recovered IV;
- require LOCAL delivery;
- reassemble all fragments if necessary;
- return the complete nested standard I2NP bytes.

Terminal assertion:

```rust
assert_eq!(recovered_standard_i2np_bytes, original_standard_i2np_bytes);
```

Do not weaken this to routing-metadata equality.

## 10.6 Add a fragmented cross-tunnel trajectory

After the small exact-byte trajectory passes, add a message large enough to require multiple outbound and/or inbound TunnelData cells.

At minimum prove:

```text
large original standard I2NP bytes
 -> outbound fragmentation
 -> OBEP reassembly with TUNNEL delivery preserved
 -> TunnelGateway
 -> inbound fragmentation
 -> inbound participant forwarding
 -> local inbound reassembly
 -> exact original bytes
```

Deliberately reorder at least the inbound endpoint's follow-on cells before final processing to exercise the bounded reassembler.

---

# 11. F5 acceptance tests

Required named tests or equivalent:

```text
outbound_to_inbound_tunnel_trajectory_exact_bytes
outbound_to_inbound_fragmented_trajectory_exact_bytes
```

Require:

1. outbound target gateway router equals the real inbound first hop;
2. outbound target tunnel ID equals the real inbound first-hop receive ID;
3. `TunnelGatewayMessage` is actually constructed/consumed;
4. IBGW role is actually executed;
5. at least one inbound participant role is actually executed;
6. local inbound endpoint is actually executed;
7. final bytes equal original bytes exactly;
8. fragmented variant retains TUNNEL delivery through OBEP reassembly;
9. fragmented variant retains LOCAL semantics inside the inbound tunnel;
10. no comment/test exemption says the full round trip is “left to runtime”.

---

# 12. Additional cleanup allowed only when directly coupled

The following may be corrected while touching the above code because they are directly coupled to closure:

- remove `action_from_unspecified()`;
- move test-only established fixtures under `#[cfg(test)]`;
- correct stale comments that still claim the state machine automatically registers the tunnel when it does not;
- remove imports/types made dead by deleting legacy fake registration APIs;
- replace internal direction-inapplicable placeholder fields with `Option` **only if** needed to eliminate production placeholder construction or simplify the real extraction path.

Do not turn the last item into a general `EstablishedTunnel` redesign. Internal non-exposed sentinel cleanup is secondary to the four core closure blockers.

---

# 13. Required implementation order

Execute exactly in this order:

```text
116-F1 real state-machine established-material extraction
      -> active extraction + registrar/pool tests green

116-F2 eliminate production placeholder pool insertion
      -> material-only production invariant green

116-F3 exact fragmentation capacity arithmetic
      -> generated fragments actually build/parse/reassemble

116-F4 preserve fragmented delivery metadata
      -> fragmented ROUTER/TUNNEL actions green

116-F5 full outbound -> inbound exact-byte trajectory
      -> small trajectory green
      -> fragmented trajectory green

116-F6 full workspace validation + closure authority update
```

Do not begin Plan 117 between these phases.

---

# 14. Validation commands

At minimum run:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-tunnel --lib
cargo test --locked -p i2pr-tunnel --all-targets
cargo test --locked -p i2pr-proto --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
git diff --check
```

Also run source-policy checks:

```bash
rg -n 'Plan 116 provisional scaffolding' crates/i2pr-tunnel/src
rg -n 'build_placeholder_established' crates/i2pr-tunnel/src
rg -n 'action_from_unspecified' crates/i2pr-tunnel/src
rg -n 'full round-trip is left to the runtime|left to the runtime' crates/i2pr-tunnel/src/roles.rs
```

Expected terminal results:

```text
Plan 116 provisional scaffolding matches = 0
production placeholder-established matches = 0
unspecified delivery fallback matches = 0
cross-tunnel test exemption matches = 0
```

If a test-only placeholder helper remains, the status record must name its `#[cfg(test)]` location explicitly and confirm it is absent from the production build.

The known historical/rootless NTCP2 environment limitation remains unrelated. Do not modify the old interoperability harness to make Plan 116 green.

---

# 15. Closure evidence to record

Update `plans/116-status.md` only after all closure criteria pass.

Record:

```text
implementation commit SHA
cargo test -p i2pr-tunnel --lib result
workspace test result
clippy/fmt/doc result
boundary-script result
source-policy grep results
exact names of state-machine extraction tests
exact names of fragmentation boundary tests
exact names of full outbound-to-inbound tests
```

Do not claim a test passed based solely on its presence in source.

No external-router evidence is required for Plan 116 closure.

---

# 16. Final closure criteria

Plan 116 may be restored to:

```text
plan_116 = passed-local-tunnel-data-plane
```

only when **all** of the following are true:

1. `ShortBuildStateMachine` exposes a success-only one-time extraction of real build-derived established material.
2. Extraction consumes the persistent per-hop layer keys from state-machine ownership rather than leaving a duplicate long-lived copy.
3. Extracted outbound topology is `[Participant*, OBEP]` with correct next-hop state and terminal `None`.
4. Extracted inbound topology is `[IBGW, Participant*]` with correct first gateway and explicit local receive tunnel.
5. A successful real short-build trajectory can extract and register material into `ExploratoryPool`.
6. The pool returns a real monotonic `TunnelSlot` and its length changes.
7. Legacy registrar APIs cannot report successful insertion/duplicate semantics without real material.
8. Production pool APIs cannot manufacture established entries from placeholder peers/keys.
9. Placeholder established constructors are deleted or test-only.
10. Automatic fragment sizing includes the exact control/address/message-id/length overhead for LOCAL, ROUTER, and TUNNEL modes.
11. Every accepted `fragment_complete_message()` result is accepted by `build_cells()`.
12. Boundary-size tests exist for all three delivery modes.
13. Generated fragmented cells parse and reassemble to the exact original message.
14. Fragmented delivery metadata survives until reassembly completion.
15. Fragmented ROUTER remains ROUTER.
16. Fragmented TUNNEL remains TUNNEL with the exact gateway router/tunnel.
17. No unspecified-delivery fallback synthesizes LOCAL.
18. The small outbound-to-inbound test executes OBGW, outbound participant(s), OBEP, TunnelGateway, IBGW, inbound participant(s), and local inbound endpoint.
19. The small outbound-to-inbound test returns exact original standard I2NP bytes.
20. A fragmented outbound-to-inbound trajectory also returns exact original bytes.
21. The fragmented cross-tunnel trajectory exercises bounded out-of-order reassembly.
22. All Plan 116 tests are active; no correctness test is ignored/disabled to achieve green CI.
23. `i2pr-tunnel` remains runtime-neutral and transport-neutral.
24. No new NTCP2/Q1/Q2/external interoperability claim is made.
25. Required workspace validation is green except explicitly documented pre-existing historical environment-only blockers unrelated to Plan 116.
26. `plans/116-status.md`, `plans/116-handoff.md`, and current support documentation agree that Plan 116 is closed and Plan 117 is next.

---

# 17. Explicit non-closure outcomes

Do **not** close Plan 116 if any of these remain:

```text
ShortBuildStateMachine Established but no real material extraction
legacy admit() fabricates a successful pool result
public production metadata-only pool registration creates fake material
fragment_complete_message generates a record build_cells rejects
fragmented ROUTER/TUNNEL loses delivery metadata
outbound_to_inbound test stops at OBEP metadata
IBGW is not executed
inbound participant is not executed
local inbound endpoint is not executed
final exact-byte assertion is absent
fragmented exact-byte cross-tunnel trajectory is absent
```

If one item remains, keep:

```text
plan_116 = final-closure-pending
plan_117 = blocked-on-plan116
```

Do not create another broad validation plan. Localize the exact remaining defect.

---

# 18. Handoff after success

Only after this pass succeeds does Plan 117 become the next executable line of work.

The state handed to Plan 117 must be:

```text
short-build construction             = locally-correct + Emissary Q0 passed
established material transfer        = real state-machine -> pool
TunnelData wire format               = locally-canonical
AES tunnel layer                     = locally-round-tripped
fragmentation/reassembly             = bounded + exact
outbound ROUTER trajectory           = exact-byte local pass
outbound -> inbound TUNNEL trajectory = exact-byte local pass
external authenticated delivery      = still deferred
normal daemon NTCP2                  = disabled-and-unenableable
```

Plan 117 may then focus on the smallest available real router-delivery lane and live exploratory/NetDB integration without reopening the Plan 116 data-plane construction loop.

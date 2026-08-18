# Plan 116 terminal cleanup: close duplicate accounting, delivery-metadata integrity, and out-of-order cross-tunnel proof

## Status

- **Ready for execution**.
- Date: 2026-08-18.
- Milestone: **5 — Network tunnel data plane and exploratory tunnels**.
- Current repository head at planning time: `8057c34c7208dd5f561d4ebf2b51b582293e8432`.
- Substantive Plan 116 implementation floor: `0330fb2e9e64dd0877472c930606ab4219ac18a9`.
- Parent plan: [`116-local-tunnel-data-plane.md`](116-local-tunnel-data-plane.md).
- Prior correction: [`116-completion-correction.md`](116-completion-correction.md).
- Prior final-closure pass: [`116-final-closure.md`](116-final-closure.md).
- Current status authority: [`116-status.md`](116-status.md).
- Handoff authority: [`116-handoff.md`](116-handoff.md).
- Plan 117 remains **blocked until this terminal cleanup passes**.

This is not another architecture pass and not another interoperability pass.

The previous Plan 116 closure implementation successfully landed the major local tunnel-data-plane work:

- real `ShortBuildStateMachine -> EstablishedMaterial -> ExploratoryPool` transfer;
- production material-only pool registration;
- corrected Tunnel Message framing and fragment-size arithmetic;
- corrected AES tunnel-layer transforms;
- CSPRNG-backed IV/padding generation;
- first-fragment delivery retention through reassembly;
- exact-byte outbound -> inbound role-level trajectories for both small and fragmented I2NP messages.

A strict post-closure audit found three remaining technical closure defects plus one documentation/evidence defect:

```text
T1 exact duplicate fragments distort aggregate retained-byte accounting
T2 first-fragment delivery metadata is not part of duplicate identity
T3 fragmented cross-tunnel trajectory is exact-byte but not out-of-order
T4 Plan 116 status/handoff/evidence authority is inconsistent
```

These are the only targets of this plan.

No new external evidence is required.

---

# 1. Hard scope lock

This pass is local, Rust-only, deterministic, runtime-neutral, and transport-neutral.

Expected production-code scope:

```text
crates/i2pr-tunnel/src/fragment.rs
crates/i2pr-tunnel/src/roles.rs
```

`data.rs` may be touched only if a test helper or typed metadata API must move there to make the reassembly invariant explicit. Do not redesign Tunnel Message framing.

Expected documentation scope:

```text
plans/116-status.md
plans/116-handoff.md
AGENTS.md / README.md / docs/architecture/i2pr-tunnel.md only if their Plan 116 closure wording must be synchronized after the code is green
```

Do **not** perform or require:

- Emissary, i2pd, or Java I2P execution;
- NTCP2 correction, activation, or advertisement;
- SSU2 work;
- Q1/Q2 external-delivery work;
- rootless namespace work;
- Docker, Multipass, QEMU, or VM work;
- Python interoperability harnesses;
- public I2P network access;
- NetDB/floodfill integration changes;
- garlic, LeaseSet, streaming, SAM, I2CP, SOCKS, or HTTP work;
- a new tunnel crate;
- a new reassembly subsystem;
- a generic router dispatcher;
- Plan 117 implementation.

If an unrelated defect is observed, record it separately. Do not expand this pass.

---

# 2. Current implementation facts

At `0330fb2e9e64dd0877472c930606ab4219ac18a9`:

1. `BoundedReassembler` owns:

```text
partials: BTreeMap<ReassemblyKey, PartialMessage>
aggregate_bytes: usize
aggregate_bytes_max: usize
expiry_ms: u64
now_ms: u64
```

2. `PartialMessage::insert()` already recognizes an identical duplicate body and returns `Ok(())` without inserting another body into `received`.

3. Both `BoundedReassembler::insert()` and `insert_with_delivery()` compute:

```text
increment = fragment.body_len()
```

before insertion and later unconditionally perform:

```text
aggregate_bytes += increment
```

whenever `PartialMessage::insert()` returns `Ok(())`.

Therefore an identical duplicate is logically a no-op in the partial map but still increases aggregate accounting.

4. Aggregate-budget admission is also evaluated with `increment = body_len()` before the implementation knows whether the fragment is an exact duplicate. An exact duplicate may therefore be rejected at the aggregate-byte ceiling despite requiring no additional retained memory.

5. `last_touched_ms` is updated before duplicate identity is established. Repeated exact duplicates can therefore extend a partial message's expiry lifetime without contributing new information.

6. `insert_with_delivery()` stores `first_delivery` only if it is currently `None`:

```text
if First && partial.first_delivery.is_none() {
    partial.first_delivery = delivery;
}
```

An identical first-fragment body with a different delivery instruction is therefore not compared as part of duplicate identity. The existing delivery is silently retained.

7. `OutboundEndpointRole` now uses `insert_with_delivery()` and no longer synthesizes LOCAL delivery after fragmented completion. This behavior must be retained.

8. `outbound_to_inbound_fragmented_trajectory_exact_bytes` now traverses all required roles and proves exact-byte recovery, but the inbound cells are delivered to the local endpoint in canonical generation order.

9. Standalone reassembler tests already prove some out-of-order behavior. Plan 116 closure additionally required an out-of-order **full role-level cross-tunnel trajectory**.

This pass should repair these exact seams rather than rewriting the surrounding subsystem.

---

# 3. T1 — make exact duplicates true no-ops for memory accounting and expiry

## Goal

An exact duplicate fragment must be idempotent in every bounded-resource dimension.

For an exact duplicate:

```text
partial body state     unchanged
aggregate_bytes        unchanged
partial count          unchanged
last_touched_ms        unchanged
first delivery metadata unchanged
completion state       unchanged except normal completion already reached elsewhere
```

A duplicate must not consume budget merely because its body has nonzero length.

A duplicate must not keep an incomplete message alive indefinitely by refreshing its expiry timestamp.

## 3.1 Do not use body length as the accounting delta until insertion is classified

The current sequence:

```text
increment = fragment.body_len()
precheck aggregate += increment
mutate partial
aggregate += increment
```

must be replaced with an insertion classification that distinguishes:

```text
NewUniqueFragment { added_bytes }
ExactDuplicateNoop
Conflict
```

Preferred design:

```rust
enum FragmentInsertDisposition {
    Inserted { added_bytes: usize },
    ExactDuplicate,
}
```

or an equivalent internal type.

The exact type name is not important. The semantic distinction is mandatory.

Do not encode `ExactDuplicate` as `Inserted { added_bytes: 0 }` if doing so obscures expiry/update behavior; a named no-op path is clearer and safer.

## 3.2 Classify before aggregate-budget rejection

An exact duplicate must not fail merely because:

```text
aggregate_bytes == aggregate_bytes_max
```

The implementation must determine whether a fragment is already retained with identical body + relevant metadata before applying any added-byte admission check.

Required ordering for an existing partial:

```text
lookup partial
 -> classify duplicate/conflict/new unique fragment
 -> exact duplicate: return success/no-op immediately
 -> conflict: invalidate partial and return typed error
 -> new unique fragment: calculate exact added_bytes
 -> check per-message + aggregate bounds
 -> mutate partial
 -> aggregate_bytes += added_bytes
```

For a new partial:

```text
capacity check
 -> metadata validation
 -> exact added-byte calculation
 -> aggregate check
 -> create partial
 -> insert
```

Do not clone the entire `PartialMessage` merely to determine whether it exists. The current `self.partials.get(&key).cloned()` copies retained fragment bodies on every insertion. This pass may remove that clone as a directly coupled memory/performance cleanup.

## 3.3 Exact duplicate must not refresh expiry

Only a newly accepted unique fragment should update:

```text
last_touched_ms
```

An exact duplicate must not extend the reassembly lifetime.

Rationale:

- expiry is a bound on incomplete retained state;
- a duplicate provides no new progress;
- allowing duplicates to refresh expiry permits low-cost state pinning.

Conflicting duplicates invalidate the affected partial immediately and therefore do not refresh it.

## 3.4 Aggregate accounting must equal actual retained fragment bytes

After every successful operation:

```text
aggregate_bytes == sum(partial.bytes for every retained partial)
```

This is the invariant to test.

Avoid maintaining two subtly different definitions of retained bytes.

If useful, add a `#[cfg(test)]` assertion/helper that recomputes the sum from `partials` and verifies the cached `aggregate_bytes`; do not expose retained payload state in production merely for tests.

---

# 4. T1 acceptance tests

Add active tests with clear names equivalent to:

```text
exact_duplicate_first_does_not_increase_retained_bytes
exact_duplicate_follow_on_does_not_increase_retained_bytes
exact_duplicate_at_aggregate_limit_is_accepted_as_noop
exact_duplicate_does_not_refresh_expiry
reassembly_completion_returns_aggregate_bytes_to_zero_after_duplicates
```

Minimum required assertions:

### 4.1 First-fragment duplicate

```text
insert first fragment
record retained_bytes
insert exact same first fragment
retained_bytes unchanged
partial count unchanged
```

When using `insert_with_delivery`, the delivery instruction must be exactly equal too.

### 4.2 Follow-on duplicate

```text
insert first
insert follow-on sequence N
record retained_bytes
insert same follow-on sequence N + identical body
retained_bytes unchanged
```

### 4.3 Aggregate ceiling

Construct the reassembler so unique retained bytes exactly consume the configured aggregate budget.

Then resend an exact duplicate.

Expected:

```text
Ok(None) or other normal idempotent success
retained_bytes unchanged
NOT AggregateBytesExceeded
```

### 4.4 Expiry pinning

Example deterministic sequence:

```text
expiry = 1000 ms
now = 0      insert unique first
now = 900    insert exact duplicate
now = 1001   expire_due
```

Expected:

```text
partial expired
```

The duplicate at 900 ms must not move the lifetime origin to 900 ms.

### 4.5 Completion after duplicates

Insert duplicates before the final missing unique fragment.

After reassembly completes:

```text
len() == 0
retained_bytes() == 0
message == exact original bytes
```

No phantom accounting may remain.

---

# 5. T2 — make first-fragment delivery metadata part of duplicate identity

## Goal

The first fragment carries semantic routing state for the entire fragmented message.

For a given `ReassemblyKey`, two first fragments are exact duplicates **only** when both are identical in:

```text
first-fragment body bytes
delivery instruction
```

A body-identical first fragment that changes:

```text
LOCAL <-> ROUTER
LOCAL <-> TUNNEL
ROUTER target hash
TUNNEL target router hash
TUNNEL target tunnel id
```

is a conflicting duplicate and must invalidate only that partial message.

Do not silently keep the first delivery instruction and ignore a conflicting second one.

## 5.1 Represent first-fragment metadata unambiguously

Current `Option<DeliveryInstruction>` is used both for:

```text
first fragment not seen yet
first fragment seen but no delivery supplied
```

Those are different states.

Choose one of these narrow designs:

### Preferred: require first-fragment delivery

`insert_with_delivery()` should reject:

```text
TunnelFragment::First + delivery == None
```

with a typed error such as:

```text
MissingFirstDeliveryInstruction
```

This reflects the Tunnel Message wire model: a first fragmented record carries delivery instructions.

Then `PartialMessage` may retain:

```rust
first_delivery: Option<DeliveryInstruction>
```

where `None` means the first fragment has not yet arrived, not “first arrived without metadata”.

### Alternative

If retaining `None` as an intentionally representable malformed/unspecified first-fragment state is required by an existing lower-level API, add a separate `first_fragment_seen: bool` or typed metadata state so that duplicate comparisons can distinguish:

```text
not seen
seen with None
seen with Some(delivery)
```

Do not leave the current ambiguous state.

## 5.2 Validate follow-on metadata

A follow-on Tunnel Message record has no delivery instruction.

If `insert_with_delivery()` is called with:

```text
TunnelFragment::FollowOn + Some(delivery)
```

prefer a typed fail-closed error such as:

```text
UnexpectedFollowOnDeliveryInstruction
```

This is a directly coupled invariant and prevents callers from introducing routing metadata that cannot exist on the wire.

Do not expand this into a generic metadata framework.

## 5.3 Conflict handling

When a duplicate first fragment conflicts in body or delivery metadata:

```text
remove only that ReassemblyKey partial
subtract its exact retained bytes from aggregate_bytes
remove retained first-delivery state
return typed conflict error
```

The rest of the reassembler remains usable.

Using the existing `ConflictingDuplicate { sequence: 0 }` is acceptable if tests make clear that metadata conflict is included. A more precise variant such as `ConflictingFirstMetadata` is preferable if it improves diagnostics without API churn.

## 5.4 Exact duplicate metadata

A first fragment with:

```text
same message id
same body
same delivery instruction
```

is an exact no-op and inherits all T1 requirements:

```text
no added bytes
no timestamp refresh
no new allocation
no delivery rewrite
```

---

# 6. T2 acceptance tests

Add active tests equivalent to:

```text
exact_duplicate_first_with_same_delivery_is_idempotent
conflicting_first_router_target_invalidates_partial
conflicting_first_tunnel_id_invalidates_partial
conflicting_first_tunnel_gateway_invalidates_partial
missing_first_delivery_fails_closed             # if preferred design is used
unexpected_follow_on_delivery_fails_closed      # if enforced
```

At least one metadata-conflict test must assert:

```text
result is error
reassembler.is_empty()
retained_bytes() == 0
```

At least one exact-metadata duplicate test must assert:

```text
result is success
retained_bytes unchanged
original delivery retained
```

Do not satisfy T2 only with body-conflict tests; routing metadata itself must be varied.

---

# 7. T3 — prove out-of-order behavior through the complete outbound -> inbound role trajectory

## Goal

Keep the existing exact-byte fragmented cross-tunnel test, but make at least one complete role-level trajectory deliver inbound TunnelData cells to `LocalInboundEndpointRole` out of canonical fragment order.

Standalone `BoundedReassembler` tests are useful but are not sufficient for this closure criterion.

Required trajectory remains:

```text
original large standard I2NP bytes
 -> OutboundGatewayRole::forward_cells
 -> outbound participant(s)
 -> OutboundEndpointRole
 -> TUNNEL RouterDeliveryAction
 -> canonical TunnelGatewayMessage
 -> InboundGatewayRole::process_cells
 -> inbound participant(s)
 -> LocalInboundEndpointRole
 -> exact original standard I2NP bytes
```

The only new dimension is reordering before the local endpoint.

## 7.1 Reorder after the inbound participant transform

Preferred implementation in the existing test:

1. Run `InboundGatewayRole::process_cells()` in canonical order.
2. Run each cell through the inbound participant role in canonical order and collect the resulting local-endpoint TunnelData cells in a `Vec`.
3. Assert the vector contains multiple cells; use a payload size that produces enough fragments for a meaningful reorder.
4. Move the canonical first-fragment cell to the end of the endpoint-delivery order.
5. Feed all follow-on cells to the local endpoint first, including the follow-on carrying `is_last = true`.
6. Feed the first-fragment cell last.
7. Assert no message is emitted until all required fragments are present.
8. Assert the eventual recovered standard I2NP bytes exactly equal the original encoded bytes.

Example order for canonical cells:

```text
[first, follow1, follow2, last]
```

Endpoint delivery may be:

```text
[follow1, last, follow2, first]
```

or simply:

```text
[follow1, follow2, last, first]
```

The important property is:

```text
at least one follow-on arrives before first
last may arrive before first
first-delivery metadata is attached later
completion still emits exact bytes once
```

## 7.2 Do not reorder encrypted cells across the wrong semantic boundary

The test should preserve the role contract:

- each IBGW-produced cell must still pass through the inbound participant;
- the endpoint receives participant-output TunnelData addressed to its local receive tunnel;
- do not decrypt or manually edit fragment bytes to create the reorder;
- do not bypass `LocalInboundEndpointRole` or call the reassembler directly in the terminal acceptance test.

## 7.3 Exact-once completion assertion

With out-of-order delivery, do not assume the vector's final canonical fragment will be the completion event.

Track:

```text
completion_count
recovered_bytes
```

Require:

```text
completion_count == 1
recovered_bytes == original_inner_bytes
```

The endpoint must not emit a partial or duplicate completed message.

## 7.4 Preserve TUNNEL and LOCAL semantics

The outbound fragmented trajectory must still prove:

```text
OBEP completion retains TUNNEL delivery
exact target IBGW router retained
exact target inbound tunnel id retained
```

The inbound fragmented trajectory must still prove:

```text
IBGW emits LOCAL delivery into the inbound tunnel
local endpoint accepts only LOCAL
```

Do not weaken existing exact-byte or routing assertions merely to add reordering.

---

# 8. T3 acceptance test

Preferred test name:

```text
outbound_to_inbound_fragmented_out_of_order_trajectory_exact_bytes
```

It may replace `outbound_to_inbound_fragmented_trajectory_exact_bytes` if the existing test is changed to include the reorder, or coexist with it if retaining both ordered and out-of-order coverage is useful.

Required assertions:

1. outbound fragmentation produces more than one cell;
2. OBEP emits exactly one TUNNEL action after reassembly;
3. OBEP target router equals the inbound IBGW;
4. OBEP target tunnel equals the inbound IBGW receive tunnel;
5. canonical `TunnelGatewayMessage` is constructed and consumed;
6. IBGW executes;
7. at least one inbound participant executes;
8. inbound fragmentation produces more than one cell;
9. at least one inbound follow-on is delivered to the local endpoint before the first fragment;
10. local endpoint emits no message before all unique fragments are present;
11. local endpoint emits exactly once;
12. recovered standard I2NP bytes equal the original standard I2NP bytes;
13. decoded recovered Data payload equals the original payload.

---

# 9. T4 — reconcile Plan 116 authority and evidence

## Goal

Do not let planning/status text overstate closure while T1-T3 are pending, and do not leave contradictory handoff instructions after they pass.

At planning time the repository is inconsistent:

```text
plans/116-status.md  -> passed-final-local-closure / Plan117 unblocked
plans/116-handoff.md -> final-closure-pending / Plan117 blocked
```

This planning commit must restore a single authority:

```text
plan_116 = terminal-cleanup-pending
plan_117 = blocked-on-plan116-terminal-cleanup
```

After implementation succeeds, update both files together to:

```text
plan_116 = passed-final-local-closure
plan_117 = unblocked-ready-for-planning
```

## 9.1 Record actual tests, not planned names

The current status file lists several F4 test names that do not exactly match the implemented source tests.

At final closure:

- derive test names from current source;
- record exact test identifiers that were actually executed;
- do not copy aspirational names from a plan;
- record the new duplicate-accounting, metadata-conflict, and out-of-order cross-tunnel tests explicitly.

Suggested evidence commands:

```bash
rg -n '^\s*fn .*duplicate|^\s*fn .*out_of_order|^\s*fn outbound_to_inbound' crates/i2pr-tunnel/src/fragment.rs crates/i2pr-tunnel/src/roles.rs
cargo test --locked -p i2pr-tunnel --lib duplicate -- --nocapture
cargo test --locked -p i2pr-tunnel --lib out_of_order -- --nocapture
cargo test --locked -p i2pr-tunnel --lib outbound_to_inbound -- --nocapture
```

Use exact filters that match the final source; the examples above are not a substitute for recording the actual commands used.

## 9.2 Synchronize handoff-facing documentation

After code passes, inspect at least:

```text
plans/116-status.md
plans/116-handoff.md
AGENTS.md
README.md
docs/architecture/i2pr-tunnel.md
```

Only change documents that contain stale Plan 116 closure/successor text.

The terminal state must not contain both:

```text
Plan 117 blocked
```

and:

```text
Plan 117 unblocked
```

in current-authority sections.

Historical plan text may remain historical if clearly labelled.

---

# 10. Required implementation order

Execute in this order:

```text
116-T1 duplicate mutation/accounting classification
      -> duplicate byte/expiry tests green

116-T2 first-delivery duplicate identity
      -> metadata conflict/idempotence tests green

116-T3 out-of-order complete cross-tunnel trajectory
      -> exact-byte role-level test green

116-T4 full crate/workspace validation
      -> status/handoff/evidence synchronization
```

Do not begin Plan 117 between these phases.

---

# 11. Validation commands

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

The historical rootless/NTCP2 limitation remains unrelated. Do not change the retired interoperability harness merely to make this pass green.

Run targeted tests before the full workspace suite. At minimum prove:

```text
exact duplicate first accounting
exact duplicate follow-on accounting
duplicate at aggregate ceiling
duplicate does not refresh expiry
first-delivery metadata conflict invalidation
exact first-delivery duplicate idempotence
out-of-order fragmented cross-tunnel exact bytes
```

Source-policy checks:

```bash
rg -n 'Plan 116 provisional scaffolding' crates/i2pr-tunnel/src
rg -n 'action_from_unspecified' crates/i2pr-tunnel/src
rg -n '#\[ignore' crates/i2pr-tunnel/src/fragment.rs crates/i2pr-tunnel/src/roles.rs
```

Expected:

```text
Plan116 provisional-scaffolding ignores = 0
unspecified-delivery fallback             = 0
new terminal-cleanup tests ignored        = 0
```

---

# 12. Explicit acceptance criteria

Plan 116 is strictly closed only when all of the following are true:

1. An identical duplicate first fragment does not increase `aggregate_bytes`.
2. An identical duplicate follow-on does not increase `aggregate_bytes`.
3. An exact duplicate is accepted as a no-op even when retained unique bytes already consume the aggregate budget.
4. An exact duplicate does not refresh `last_touched_ms` or otherwise extend partial expiry.
5. After completion of a message that received exact duplicates, `retained_bytes() == 0` and no phantom accounting remains.
6. Cached aggregate accounting equals actual retained unique fragment bytes after insertions, duplicate no-ops, conflicts, expiry, and completion.
7. A duplicate first fragment with the same body and same delivery instruction is idempotent.
8. A duplicate first fragment with the same body but different ROUTER target is rejected and invalidates only that partial.
9. A duplicate first fragment with the same body but different TUNNEL router or tunnel ID is rejected and invalidates only that partial.
10. First-fragment missing-delivery state is unambiguous; preferably it fails closed as malformed.
11. Follow-on delivery metadata cannot silently alter first-fragment routing state.
12. Conflicting first-fragment metadata releases the partial's retained-byte accounting.
13. The existing fragmented outbound -> inbound trajectory still executes every production role.
14. At least one follow-on TunnelData cell reaches `LocalInboundEndpointRole` before the first-fragment cell in the terminal role-level test.
15. The out-of-order endpoint emits no incomplete message.
16. The out-of-order endpoint emits the complete message exactly once.
17. The recovered standard I2NP bytes are exactly equal to the original bytes.
18. The decoded recovered payload is exactly equal to the original payload.
19. Fragmented TUNNEL delivery still retains the exact IBGW router and tunnel ID through OBEP reassembly.
20. Inbound LOCAL semantics remain enforced by `LocalInboundEndpointRole`.
21. No Plan 116 correctness test is ignored, deleted, weakened, or feature-gated to make validation pass.
22. `i2pr-tunnel` remains runtime-neutral and transport-neutral.
23. No new external-router, NTCP2, Q1, Q2, or public-network claim is made.
24. The full workspace validation suite passes except any explicitly documented pre-existing environment-only command unrelated to Plan 116.
25. `plans/116-status.md` and `plans/116-handoff.md` agree on closure state and Plan 117 successor state.
26. Final status records exact names of the tests actually present and executed.

Only then record:

```text
plan_116_duplicate_accounting       = passed-noop-exact-duplicates
plan_116_duplicate_expiry           = passed-no-refresh
plan_116_first_delivery_identity    = passed-conflict-detected
plan_116_fragmented_cross_tunnel    = passed-out-of-order-exact-bytes
plan_116                             = passed-final-local-closure
plan_117                             = unblocked-ready-for-planning
```

---

# 13. Non-closure outcomes

Do **not** close Plan 116 if any of these remain:

```text
exact duplicate increments aggregate_bytes
exact duplicate can fail only because aggregate budget is already full
exact duplicate refreshes partial expiry
completion after duplicates leaves nonzero phantom retained bytes
same first body + different delivery metadata is silently accepted
first-fragment metadata state remains ambiguous in a way that permits overwrite
full fragmented cross-tunnel test still feeds endpoint cells only in canonical order
out-of-order test bypasses production role code or calls reassembler directly
out-of-order exact-byte assertion is absent
status says Plan117 unblocked while handoff says blocked
status lists tests that do not exist under those names
```

If one item remains, keep:

```text
plan_116 = terminal-cleanup-pending
plan_117 = blocked-on-plan116-terminal-cleanup
```

Do not create another broad validation plan. Localize the remaining defect in code.

---

# 14. Handoff after success

Once this pass is green, Plan 116 is finished.

The state handed to Plan 117 should be:

```text
short-build independent native Q0        = passed
short-build established material transfer = real state-machine -> pool
TunnelData framing                        = locally canonical
AES tunnel layer                          = locally round-tripped
fragment sizing                           = exact boundary tested
reassembly bounds                         = unique-byte accurate
exact duplicates                          = no-op / no expiry refresh
first-fragment routing metadata           = duplicate-integrity checked
outbound ROUTER trajectory                = exact-byte local pass
outbound -> inbound TUNNEL trajectory     = exact-byte local pass
fragmented cross-tunnel trajectory        = out-of-order exact-byte local pass
external authenticated delivery           = deferred
normal daemon NTCP2                       = disabled-and-unenableable
```

Plan 117 may then be planned around the smallest available runtime integration step without reopening Plan 116 local data-plane construction.
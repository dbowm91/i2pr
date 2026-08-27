# Plan 133 — Milestone 6 final evidence and authority closure

## Status

**Ready for execution.**

- Date: **2026-08-27**.
- Source floor: `af0f07a0037a639afc2c03af31a1266b99273564`.
- This is the **single final Milestone 6 closure pass** after the Plan 132 implementation.
- Plan 132 landed the substantive protocol corrections: strict Elligator receive-domain validation, artifact-preserving replay seams, and transactional `send_data` / `send_close` / `send_reset` ordering. Do **not** reopen those implementations unless the focused evidence below exposes a concrete defect.
- The remaining gap is narrower: two replay trajectories still do not prove the exact boundary they claim, the Elligator reference note incorrectly states that Java I2P and i2pd agree on the equality boundary, and repository status authority is contradictory.
- If this plan passes, **Milestone 6 is closed locally and Milestone 7 / SAM baseline planning is immediately authorized. Do not create Plan 134 for further Milestone 6 cleanup unless a new concrete protocol defect is discovered.**

## 1. Objective

Finish Milestone 6 by making the landed Plan 132 evidence truthful, mechanically specific, and internally consistent:

```text
B2 consumed-tag replay
  = exact original ES ciphertext/tag
  + fresh lower tunnel wrapping only
  + tunnel success proven
  + integrated dispatcher rejection proven at ECIES/session boundary
  + no plaintext/Streaming delivery

B3 fresh-reseal replay
  = one original Streaming TransportSendRequest sequence N
  + retain the actual first ES envelope that was delivered
  + fresh second ES envelope from the same exact request
  + prove first/second ES tags differ while inner Streaming bytes/sequence are identical
  + prove tunnel + ECIES succeed on second delivery
  + prove only Streaming suppresses sequence N

Elligator reference authority
  = Java equality behavior recorded exactly
  + i2pd equality behavior recorded exactly
  + i2pr's stricter Java-compatible choice documented as deliberate

Milestone authority
  = Plans 131/132 historical state reconciled
  + Plan 133 is final local closure record
  + all Milestone 6 / Milestone 7 documentation agrees
```

This plan is an **evidence and authority closure**, not a new router feature pass.

## 2. Current state and why Plan 133 exists

Plan 132 implementation commit `af0f07a0037a639afc2c03af31a1266b99273564` successfully landed the primary code corrections:

- `crates/i2pr-crypto/src/ecies.rs`
  - masks the two free high Elligator bits;
  - rejects masked representatives outside the selected canonical lower-half domain before `elligator2::from_representative`;
  - retains Plan 131 randomized production encoding.
- `crates/i2pr-client/src/streaming/manager.rs`
  - `send_data()` performs fallible packet/client-payload construction before `enqueue_send()`;
  - `send_close()` builds/signs before close-state mutation;
  - `send_reset()` builds/signs before reset-state mutation.
- `crates/i2pr-client/tests/plan132_trajectory.rs`
  - B1 now retains and replays exact tunnel cells and directly asserts typed tunnel replay rejection;
  - B2 now preserves the same recovered Garlic/ES bytes while refreshing the lower tunnel cells;
  - B3 now sends the same `TransportSendRequest` through a fresh ECIES seal rather than directly calling the peer `StreamingManager`.

Those are meaningful corrections and should be preserved.

However, the post-implementation audit found four closure defects.

### 2.1 B2 does not assert the integrated rejection boundary strongly enough

`plan132_consumed_es_ciphertext_rewrapped_in_fresh_tunnel_is_rejected_by_ecies()` correctly performs the difficult artifact-preservation step, but its integrated assertion is currently only:

```text
replay_outcome is not ExistingSessionProcessed
```

That proves the replay was not accepted as an Existing Session, but it permits unrelated parser/classification failures to satisfy the test. Plan 132 explicitly required the integrated test to record and assert the typed failure boundary rather than treating “no second application delivery” as the primary evidence.

A direct session-manager test already proves the exact retained `ExistingSessionMessage` returns `EciesSessionError::UnknownSessionTag` on its second direct submission. Plan 133 must retain that direct proof **and** make the full dispatcher trajectory assert an ECIES/session-layer rejection outcome.

### 2.2 B3 compares the wrong ECIES envelopes

`plan132_fresh_es_seal_of_same_streaming_sequence_reaches_streaming_and_deduplicates()` performs its first real delivery through `pipe_through_stack()`, which internally creates and consumes an ECIES envelope but does not return it to the test.

The test then creates `second_plan`, and afterwards creates another `first_plan`. Therefore the bytes being compared are actually the **second and third** ECIES seals, not the first delivered envelope and its fresh replay seal.

The test also:

- retains `first_application_payload` but never asserts it against a second payload;
- ignores the exact `DestinationDispatcher` outcome before invoking the Streaming adapter;
- uses a hard-coded `plan132_plan_cell_count() -> 1` helper instead of comparing the actual first and second plan cell counts.

This is close to the intended proof but not authoritative evidence.

### 2.3 The Elligator reference note misstates the equality-boundary agreement

The current reference note says both deployed references reject `r >= (p - 1) / 2`. That is not what the pinned source shows:

- Java I2P: `if (r.compareTo(divide_minus_p_1_2) >= 0) return null;`
  - equality is rejected;
  - accepted domain is `r < (p - 1) / 2`.
- i2pd: `if (BN_cmp(r, p12) <= 0)` then decode;
  - equality is accepted by the code as written;
  - accepted domain is `r <= (p - 1) / 2`.

The i2pd source comment says `r < (p-1)/2`, but the executable condition is `<=`. Preserve that distinction explicitly.

Plan 132 already allowed i2pr to select the stricter deployed-compatible boundary if references differed. The current implementation's strict Java-compatible choice (`r < threshold`) is reasonable and should **not** be broadened merely to make i2pd equality pass. The documentation must simply state the discrepancy truthfully.

### 2.4 Repository authority is contradictory

At the Plan 132 implementation source floor:

- `plans/132-status.md` still says `ready-for-execution`, `milestone6_local_product = not-closed`, and `router_construction = hold-for-plan132-local-closure`;
- `plans/131-status.md` still describes itself as current Milestone 6 authority and says the milestone is passed;
- README / architecture / support material added by the implementation commit describes Plan 132 as passed and Milestone 7 as next.

A milestone cannot be considered closed while its authoritative status files disagree with the rest of the repository.

## 3. Hard scope boundaries

### In scope

- `crates/i2pr-client/tests/plan132_trajectory.rs`:
  - tighten B2 integrated typed rejection evidence;
  - rebuild B3 so the test retains the **actual first delivered ES envelope** and compares it with the fresh second seal;
  - remove brittle/hard-coded evidence helpers if no longer needed.
- `specs/references/elligator2-production-representation.md`:
  - correct Java/i2pd equality-boundary wording;
  - document the selected i2pr strict receive rule accurately.
- `plans/131-status.md`, `plans/132-status.md`, and new `plans/133-status.md` authority synchronization after green validation.
- Minimal README / AGENTS / architecture / protocol-support / support-table / skill wording updates only where they currently identify the wrong final Milestone 6 authority.
- Focused test-only helper refactoring necessary to retain the first and second ES artifacts cleanly.

### Out of scope

Do **not** add or reopen:

- Elligator field arithmetic or another Elligator dependency swap;
- changes to the Plan 132 strict decoder unless a deterministic regression proves it wrong;
- changes to production ECIES branch/high-bit generation;
- changes to Streaming sequence, ACK/NACK, port, CLOSE, or RESET semantics unless a focused regression proves a concrete defect;
- NTCP2/SSU2 activation or transport interoperability work;
- Plan 116/117 or Emissary reference-router work;
- i2pd/Java live harnesses;
- Docker, rootless namespaces, VMs, Multipass, QEMU, privileged setup;
- public I2P network testing;
- Python interoperability harnesses or general testing frameworks;
- SAM implementation itself;
- I2CP, proxies, service tunnels, new LeaseSet types, PQ ratchets, MetaLS2, EncryptedLeaseSet, or legacy ElGamal.

Preserve these already-correct invariants:

- outbound destination tunnels carry `garlic_i2np_bytes`, never plaintext inner Data;
- sender Standard LS2/type-4 X25519 binding and reverse routing remain unchanged;
- production Elligator representatives randomize deployed-reference branch plus high bits;
- deterministic Plan 126 vectors remain stable;
- strict i2pr receive domain remains `masked r < 2^254 - 10` unless a newly reproduced normative contradiction proves otherwise;
- source port `0` remains valid I2P unspecified;
- SYN/SYN-response use sequence `0`, first application packet uses sequence `1`;
- established port tuples remain connection-owned;
- Plan 132 transactional send ordering remains intact;
- persistent tunnel duplicate windows remain persistent across deliveries.

## 4. Phase A — fix B2 consumed Existing Session replay evidence

Target test:

```text
plan132_consumed_es_ciphertext_rewrapped_in_fresh_tunnel_is_rejected_by_ecies
```

### A1. Retain and prove the exact original ECIES artifact

The test must explicitly establish that the first application transmission is an **Existing Session** form before delivery.

Preferred assertions:

```rust
assert_eq!(first_plan.encrypted_message.form_name(), "existing-session");
let retained_es_bytes = first_plan.encrypted_message.message_bytes().to_vec();
let retained_es = ExistingSessionMessage::decode(&retained_es_bytes, MAX_I2NP_PAYLOAD_SIZE)?;
let retained_tag = retained_es.tag;
```

Use the actual available public/test API; do not add production API merely for diagnostics if the existing `EncryptedOutbound` accessor is sufficient.

Then prove:

- the first recovered Garlic carrier is byte-for-byte the plan's `garlic_i2np_bytes` (or equivalently contains the exact `retained_es_bytes` under the normal I2NP Garlic wrapper);
- first dispatch succeeds as `InboundDispatchOutcome::ExistingSessionProcessed { .. }`;
- first Streaming delivery yields the original bytes exactly once.

### A2. Refresh only the lower tunnel representation

Create fresh inbound tunnel cells from the **same retained router delivery / Garlic bytes**, using a new deterministic IBGW RNG seed.

Required assertions:

- first and second tunnel cell bytes differ;
- the fresh cells are accepted by the persistent inbound tunnel roles (no `DuplicateCell`);
- recovered second I2NP Garlic bytes equal the original recovered bytes exactly;
- decoded ECIES message bytes/tag equal `retained_es_bytes` / `retained_tag` exactly.

No call to `StreamingDestinationAdapter::send()` or `send_via_adapter()` is allowed between first and second ECIES dispatch in this B2 replay leg.

### A3. Assert the integrated ECIES/session rejection directly

Pass the recovered replay envelope into the real `DestinationDispatcher`.

The assertion must be a **typed rejected outcome at the ECIES/session boundary**, not merely “not processed”. Prefer:

```rust
assert!(matches!(
    replay_outcome,
    InboundDispatchOutcome::Rejected(InboundDispatchError::Session(_))
));
```

If the exact current dispatcher classification yields a narrower stable variant, assert it. The important contract is:

- lower tunnel accepted the new wrapping;
- dispatcher reached ECIES/session processing;
- ECIES/session processing rejected the repeated ciphertext/tag;
- no plaintext Data clove was queued.

Do **not** accept `Rejected(Codec(_))`, `Rejected(NotGarlic)`, `Rejected(UnknownDestination(_))`, or another earlier/later-layer error as valid B2 evidence unless investigation proves that such a result is intrinsically the ECIES authentication rejection for this exact retained artifact. If so, document why before changing the acceptance rule.

### A4. Keep the direct consumed-tag sanity proof

Retain or strengthen:

```text
plan132_ecies_session_layer_rejects_consumed_tag_directly
```

It must submit the same retained `ExistingSessionMessage` twice and assert exactly:

```text
second == Err(EciesSessionError::UnknownSessionTag)
```

This direct test and the integrated dispatcher test prove complementary things:

- direct test: consumed tag is removed from the session window;
- integrated test: the full local destination path reaches ECIES/session rejection and emits no plaintext.

### A5. Negative-control state assertions

Snapshot before the replay dispatch and verify after rejection:

- destination dispatcher queued-payload count unchanged;
- Streaming delivered count unchanged;
- no call to `receive_next_payload()` is possible because no new Data payload was queued;
- established ECIES session count remains valid and is not replaced by a bogus candidate session;
- no new provisional responder/pending handshake is installed as a side effect of the replay.

Add narrowly scoped read-only count accessors only if existing test-visible APIs cannot observe these facts.

### Phase A acceptance

- [ ] Same ES ciphertext/tag is proven byte-for-byte across both lower-layer deliveries.
- [ ] Fresh tunnel wrapping is proven distinct and accepted.
- [ ] Integrated dispatcher outcome is asserted as typed ECIES/session rejection.
- [ ] Direct second ES submission returns `UnknownSessionTag`.
- [ ] No plaintext/Data/Streaming delivery occurs on replay.
- [ ] No accidental NS/provisional state is installed.

## 5. Phase B — rebuild B3 around the actual first and second ES seals

Target test:

```text
plan132_fresh_es_seal_of_same_streaming_sequence_reaches_streaming_and_deduplicates
```

The test must compare **the ECIES envelope delivered first** with **the one used for the duplicate delivery**. Do not synthesize a third envelope for comparison.

### B1. Create exactly one Streaming request

After the stream and ECIES paired session are established:

```rust
let request_n = streaming.send_data(...)?;
```

Record:

- `request_n.sequence` = `N`;
- `request_n.application_payload.clone()` = exact RFC1952 ClientPayload/Streaming bytes;
- source/destination ports and stream IDs if helpful for diagnostics.

Do **not** call `send_data()` again for the duplicate leg.

### B2. Seal and retain the actual first ES plan

Instead of immediately hiding the first seal inside `pipe_through_stack()`, explicitly call the adapter once:

```text
first_plan = send_via_adapter(&request_n, seed_1, now)
```

Assert:

- `first_plan.encrypted_message.form_name() == "existing-session"`;
- decode/capture `first_es_tag` and `first_es_bytes`;
- retain `first_plan.garlic_i2np_bytes` or the resulting OBEP `RouterDeliveryAction` as needed.

Drive **that exact `first_plan`** through:

```text
outbound participant / OBEP
 -> fresh inbound cells
 -> persistent inbound participant/endpoint
 -> DestinationDispatcher
 -> StreamingDestinationAdapter::receive
```

Required first-delivery assertions:

- dispatcher returns `ExistingSessionProcessed`;
- Streaming adapter dispatches normally;
- application bytes `request_n` carries are delivered exactly once.

### B3. Freshly reseal the exact same request

Only after first delivery/consumption:

```text
second_plan = send_via_adapter(&request_n, seed_2, later_now)
```

Assert:

- `second_plan.encrypted_message.form_name() == "existing-session"`;
- decode `second_es_tag`;
- `second_es_tag != first_es_tag`;
- `second_plan.encrypted_message.message_bytes() != first_es_bytes`;
- the inner `TransportSendRequest` is literally the same `request_n`, so:
  - sequence remains exactly `N`;
  - `application_payload` bytes remain byte-for-byte unchanged;
  - stream IDs and ClientPayload ports remain unchanged.

Where useful, decode the retained RFC1952 ClientPayload/Streaming bytes to assert sequence `N` directly. Do not rely only on a Rust struct field if the encoded bytes are easy to decode through the production codec.

Compare actual first/second plan cell counts directly:

```rust
assert_eq!(first_plan.cells.len(), second_plan.cells.len());
```

Delete `plan132_plan_cell_count() -> 1` if it is no longer useful. A hard-coded one-cell assumption is not evidence and may become wrong as packet sizing evolves.

### B4. Prove tunnel and ECIES succeed on the second seal

Drive `second_plan` through the complete lower stack.

Required assertions before invoking Streaming:

1. tunnel roles return success / recovered Garlic carrier;
2. dispatcher returns exactly:

```text
InboundDispatchOutcome::ExistingSessionProcessed { .. }
```

This proves:

- the tunnel did not reject it as a replay;
- the new ES tag was valid and accepted;
- ECIES authentication/decryption succeeded;
- a Data payload reached the destination queue.

Only then pop the queued Data payload and invoke the normal inbound Streaming adapter.

### B5. Prove only Streaming deduplicates sequence N

Required second-delivery assertions:

- inbound adapter returns normal Streaming dispatch, not a codec/protocol failure;
- receiver's Streaming receive window does not advance application delivery for the repeated `N`;
- `drain_delivered()` yields no second copy of the application bytes;
- total application delivery count remains exactly one.

This is the decisive negative control: every lower layer succeeded, so only Streaming sequence-window duplicate suppression can explain the missing second application delivery.

### Phase B acceptance

- [ ] There is exactly one `send_data()` request for sequence `N`.
- [ ] The first plan retained by the test is the plan actually delivered first.
- [ ] The second plan is a fresh ES seal of that same request.
- [ ] First and second ES tags/ciphertexts differ.
- [ ] Inner Streaming encoded bytes and sequence are identical.
- [ ] Second tunnel delivery succeeds.
- [ ] Second dispatcher outcome is exactly `ExistingSessionProcessed`.
- [ ] Streaming alone suppresses the second sequence `N`.
- [ ] No third ECIES seal is created merely for comparison.
- [ ] Hard-coded one-cell evidence helper is removed or replaced with actual plan comparison.

## 6. Phase C — correct the Elligator reference authority note

Target:

```text
specs/references/elligator2-production-representation.md
```

### C1. Record executable reference behavior exactly

The note must distinguish source **comments** from executable **conditions**.

Required wording, semantically:

```text
Java I2P:
  executable check rejects r >= (p-1)/2
  accepted domain: r < (p-1)/2

current pinned i2pd:
  source comment says r < (p-1)/2
  executable check enters decode branch when BN_cmp(r, p12) <= 0
  accepted domain as coded: r <= (p-1)/2
```

Do not state that both reject equality.

Pin the repository/branch/path/revision used if the current reference note already records that provenance. If only branch/path is currently pinned, add the exact inspected commit SHA when readily available; do not delay the pass for an unnecessary broad source archaeology exercise.

### C2. Document i2pr's selected rule as deliberate strictness

Keep current i2pr behavior:

```text
masked r < (p-1)/2
```

Document why:

- it exactly matches Java I2P's stricter receive acceptance;
- it is a subset of i2pd's coded acceptance domain, so it does not broaden i2pr beyond either major implementation;
- equality is an edge-domain representation and rejecting it avoids an implementation-specific acceptance oracle;
- the official `DECODE_ELG2` format description does not require i2pr to broaden its parser to accept this single equality case.

Do not claim cross-router interoperability from this local decision.

### C3. Keep boundary tests aligned with the selected rule

Current tests rejecting equality should remain green. Rename comments that say “every deployed decoder rejects equality” to the accurate statement that **i2pr deliberately follows the stricter Java-compatible boundary**.

No production crypto arithmetic change is expected in Phase C.

### Phase C acceptance

- [ ] Java executable equality behavior is correct in docs.
- [ ] i2pd executable equality behavior is correct in docs.
- [ ] Any conflicting i2pd source comment is identified as a comment/code discrepancy.
- [ ] i2pr strict choice is documented as deliberate, not falsely described as unanimous reference behavior.
- [ ] Existing strict decoder and valid representative fixtures remain unchanged and green.

## 7. Phase D — targeted regression and evidence-integrity gate

After Phases A–C, run the focused tests first.

At minimum the Plan 132/133 evidence must include:

```text
plan132_exact_same_tunnel_cell_is_rejected_by_live_duplicate_window
plan132_consumed_es_ciphertext_rewrapped_in_fresh_tunnel_is_rejected_by_ecies
plan132_fresh_es_seal_of_same_streaming_sequence_reaches_streaming_and_deduplicates
plan132_ecies_session_layer_rejects_consumed_tag_directly
plan132_send_data_oversized_failure_is_precommit
plan132_send_data_port_tuple_mismatch_is_precommit
plan132_send_close_valid_succeeds_after_build
plan132_send_close_port_tuple_mismatch_is_precommit
plan132_send_reset_port_tuple_mismatch_is_precommit
```

Add Plan 133-specific test names only if they make the corrected evidence materially clearer. Prefer correcting the existing Plan 132 tests so there is one authoritative implementation of each replay trajectory. Historical Plan 131 tests should remain untouched as audit history.

### Evidence-integrity rules

A green test is acceptable only when all claims in its name are proven by direct assertions:

- “same tunnel cell” -> exact retained cell bytes are reused;
- “fresh tunnel” -> tunnel bytes are demonstrably different and tunnel processing succeeds;
- “consumed ES” -> exact ciphertext/tag are demonstrably unchanged;
- “rejected by ECIES” -> integrated dispatch returns an ECIES/session rejection variant;
- “fresh ES” -> new ES tag/ciphertext demonstrably differs from the actual first delivered ES;
- “reaches Streaming” -> dispatcher/ECIES success is asserted before Streaming is invoked;
- “deduplicates” -> encoded Streaming sequence is the same and no second application delivery occurs.

Never use only a final unchanged delivery counter to infer an earlier-layer rejection.

## 8. Phase E — full local validation gate

Use the pinned Rust 1.95 surface:

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

If the repository's normal local gate still includes static NTCP2 fixture/vector checks, run them as regression checks only. Do not turn a failure into renewed transport implementation unless the failure is caused by this pass.

No privileged/network/VM/public-I2P validation is required.

## 9. Phase F — reconcile milestone authority only after green validation

This phase is mandatory. Do not leave the repository with multiple files claiming incompatible authority.

### F1. `plans/131-status.md`

Retain as historical evidence and change its authority language so it no longer says it is current.

Desired classification at minimum:

```text
plan_131 = superseded-by-plan132-and-plan133-final-gates
```

Its historical implementation summary may remain, but add a concise pointer explaining that replay evidence and final authority were corrected later.

### F2. `plans/132-status.md`

Convert from the stale `ready-for-execution` registration record to an executed historical status.

Recommended classification:

```text
plan_132 = passed-implementation-corrections-superseded-by-plan133-evidence-authority-gate
```

Record that Plan 132 successfully landed:

- strict Elligator receive-domain validation;
- real retained-artifact replay seams;
- transactional established send ordering;

but required Plan 133 to correct B2/B3 evidence strength, the Java/i2pd equality note, and final authority synchronization.

Do not call Plan 132 a failed protocol implementation; the remaining issue is the closure evidence/authority layer.

### F3. `plans/133-status.md`

After all acceptance criteria pass, update the Plan 133 registration to:

```text
plan_130 = superseded-by-plan131
plan_131 = superseded-by-plan132-and-plan133-final-gates
plan_132 = passed-implementation-corrections-superseded-by-plan133-evidence-authority-gate
plan_133 = passed-milestone6-final-evidence-authority-closure

milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = may-continue
next_product_layer = SAM baseline planning (Milestone 7)
```

### F4. Synchronize current-facing documentation

Search for current-final-authority claims involving Plan 131/132 or Milestone 6 and update only those that are now stale. Likely files include:

- `README.md`;
- `AGENTS.md`;
- `docs/architecture/overview.md`;
- `docs/architecture/i2pr-client.md`;
- `docs/architecture/i2pr-crypto.md`;
- `docs/protocol-support.md`;
- `specs/support.toml`;
- `.opencode/skills/i2pr-ntcp2-interop/SKILL.md` if it carries project-status authority.

Current-facing docs should identify **Plan 133** as the final local Milestone 6 closure authority.

Historical plan files should not be rewritten to pretend their original evidence was different.

## 10. Final acceptance criteria

Plan 133 passes only if every item below is true.

### B2 consumed-tag replay

- [ ] First outbound form is explicitly proven `ExistingSession`.
- [ ] Exact first ES ciphertext/tag is retained.
- [ ] Fresh lower tunnel cells differ from first cells.
- [ ] Fresh lower tunnel cells are accepted by the live tunnel roles.
- [ ] Fresh wrapping recovers exactly the same Garlic/ES bytes.
- [ ] Integrated dispatcher returns a typed ECIES/session rejection.
- [ ] Direct repeated `ExistingSessionMessage` returns `UnknownSessionTag`.
- [ ] No plaintext Data payload, Streaming receive, or bogus session state is created by replay.

### B3 fresh ES reseal / Streaming dedup

- [ ] Exactly one `send_data()` request is created for sequence `N`.
- [ ] Actual first delivered `OutboundDeliveryPlan` is retained by the test.
- [ ] Second plan freshly seals the exact same request.
- [ ] First and second forms are both `ExistingSession`.
- [ ] First and second ES tags/ciphertext envelopes differ.
- [ ] Inner encoded ClientPayload/Streaming bytes and sequence `N` are identical.
- [ ] No third comparison seal is generated.
- [ ] Second tunnel delivery succeeds.
- [ ] Second dispatcher result is exactly `ExistingSessionProcessed`.
- [ ] Normal inbound Streaming adapter receives the repeated packet.
- [ ] Streaming application delivery occurs only once total.
- [ ] Hard-coded one-cell comparison helper is removed/replaced by actual first/second plan comparison.

### Elligator reference record

- [ ] Java `>=` equality rejection is recorded accurately.
- [ ] i2pd executable `<=` acceptance is recorded accurately.
- [ ] i2pd comment/code discrepancy is explicit.
- [ ] i2pr strict `< threshold` policy is documented as deliberate Java-compatible strictness.
- [ ] No production Elligator arithmetic or decoder broadening is introduced.

### Transactional send / retained product invariants

- [ ] Plan 132 transactional `send_data()` ordering remains intact.
- [ ] Plan 132 CLOSE/RESET build-before-mutation ordering remains intact.
- [ ] Plan 130/131/132 still-valid regressions remain green.
- [ ] Source-port-zero and connection-owned port behavior remain green.
- [ ] Plan 124 `garlic_i2np_bytes` tunnel invariant remains green.

### Authority / regression

- [ ] `plans/131-status.md` no longer claims current authority.
- [ ] `plans/132-status.md` no longer says `ready-for-execution` after execution.
- [ ] `plans/133-status.md` is the sole current Milestone 6 local closure record.
- [ ] README/AGENTS/architecture/protocol-support/support-table/skill status agree on Plan 133 and Milestone 7 next.
- [ ] Workspace fmt/check/test/clippy/doc all pass.
- [ ] Static dependency/runtime/fixture checks pass.
- [ ] No mixed-router interoperability claim is added.
- [ ] No transport/harness/VM/public-network/SAM implementation scope creep is added.

## 11. Required closure classification

Until Plan 133 passes:

```text
plan_130 = superseded-by-plan131
plan_131 = corrective-history-under-plan133
plan_132 = implementation-landed-evidence-authority-incomplete
plan_133 = ready-for-execution

milestone6_local_product = not-closed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = hold-for-plan133-final-evidence-authority-closure
next = Plan 133
```

After Plan 133 passes:

```text
plan_130 = superseded-by-plan131
plan_131 = superseded-by-plan132-and-plan133-final-gates
plan_132 = passed-implementation-corrections-superseded-by-plan133-evidence-authority-gate
plan_133 = passed-milestone6-final-evidence-authority-closure

milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = may-continue
next_product_layer = SAM baseline planning (Milestone 7)
```

At that point **stop Milestone 6 corrective work**. External transport/destination interoperability evidence remains separate debt and is not a prerequisite for beginning SAM.

## 12. Handoff execution order

For a smaller-model executor, follow this exact order and do not broaden the task:

```text
1. Read Plan 133 and current Plan 132 tests/status.
2. Fix B2 typed integrated rejection assertion + state negative controls.
3. Rebuild B3 to retain/deliver the actual first plan and compare it directly with the second fresh seal.
4. Remove the hard-coded one-cell helper if no longer necessary.
5. Correct the Java/i2pd equality-boundary wording and comments/tests describing it.
6. Run focused i2pr-client + i2pr-crypto tests.
7. Run the full Rust 1.95 workspace/static validation gate.
8. Only after green: reconcile Plans 131/132/133 status and current-facing documentation.
9. Verify `git diff` contains no transport/harness/SAM implementation scope creep.
10. Commit the implementation/closure and hand off directly to Milestone 7 / SAM baseline planning.
```

Do not create another Milestone 6 planning series if these criteria pass.
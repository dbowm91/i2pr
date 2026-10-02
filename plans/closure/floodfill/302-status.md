# Plan 302 status: passed

- Plan: `plans/implementation/floodfill/302-m12-floodfill-reply-wire-form-and-replication-corrective.md`
- Closure token: `passed-m12-floodfill-reply-wire-form-and-replication-answered`
- Corrective: none required. Successor for matrix execution: Plan 303 (registered).
- Stop classification: n/a — the single bounded attempt executed and its findings
  are fully explained. The lane still stops at matrix F for a structural,
  documented reason owned by Plan 303.

## 1. What was executed

All seven Plan 302 §4 work packages, on head `389846f`:

1. **Wire-form fix** (`crates/i2pr-daemon/src/floodfill.rs`): `encode_transport`
   builds the 9-byte short-transport envelope with checked ms→s conversion
   (fail closed on overflow); the tunnel-embedded inner clove stays
   standard-encoded via `encode_inner_standard`. Four outer call sites
   (StoreAck, direct LookupReply, outer TunnelGateway, DirectFlood).
2. **Wire-form pin tests**: every reply shape asserts short-transport decode,
   sane seconds expiration within a 60 s horizon, session-layer offset
   equality (`raw[5..9]`), and standard-parse rejection (fails pre-fix).
3. **Reference cross-check**: hand-built 9-byte-header vector locks the
   `I2NPProtocol.h` layout against the session parse offsets, no encoder
   involved; plus `encode_transport` overflow fail-closed rows.
4. **Replication observability**: the `#[ignore]`-gated driver records the
   insert-outcome class (`inserted`/`replaced`/`idempotent`/`stale`/
   `conflict`/`capacity`) and the offered/absent split per accepted store,
   emitted as the `publisher-store-insert-outcome` TSV row right after
   matrix A so it survives a later stop.
5. **Seeded-replica idempotency tests** (local, no network):
   `fresh_publisher_store_inserts_and_plans_direct_replication` (Inserted +
   ack + candidate + ≥1 zero-token DirectFlood) and
   `seeded_replica_idempotent_store_acks_without_replication` (Idempotent +
   ack + no candidate + zero floods) — the exact lane triple.
6. **One bounded attempt**: fresh frozen budget 1
   (`MAX_ATTEMPTS=1`, `run-i2pd.sh` line 40, printed `attempt budget: 1
   (frozen)`), exact-pinned i2pd 2.61.0 at
   `635b013a612ff47278ef02acf8580a28e10e26c5`, loopback-only, reseed
   disabled. Evidence root
   `target/interop/m12-floodfill-evidence-plan302` (kept distinct from the
   Plan 278 root).
7. **No-guarantee clause honored**: the attempt surfaced no new protocol
   defect — the remaining matrix-F stop is a structural lane trigger
   artifact (see §3), not a defect.

## 2. Live results (sanitized counts only)

```
phase 1: controlled activation passes; published 680-byte caps=f RouterInfo
i2pr observed: store_accepted=1, store_ack_delivered=1, lookup_answered=13,
  lookup_hits=2, lookup_dsrm=9, direct_store_replicas=0, dispatch_errors=[],
  role_state="Active"
insert outcome: ["idempotent"], replication_offered=0, replication_absent=1
reference c.log: "Publishing our RouterInfo to lnIh. reply token=1063665646"
              -> "Publishing confirmed. reply token=1063665646"
reference "Message <id> expired" drops for i2pr-sent messages: 0 (was 19)
reference "Can't find floodfill" loop: absent (was looping)
```

What this proves:

- **The §12.1 wire defect is fixed and consumed live.** The reference
  confirms the publish (previously zero confirmations in 114 s) and drops
  nothing. `lookup_hits` moved 0 → 2: record answers are consumed too.
- **The §12.2 question is answered, live and locally.** The publisher store
  is an idempotent re-publication of the seeded client record
  (`outcome=["idempotent"]`), so no replication candidate is offered — by
  design (`floodfill_service.rs:576-580` excludes Idempotent; the ack is
  still owed). The local test pair proves the mechanism without spending
  budget; the live row confirms the lane instance matches it. This is the
  Plan 302 §8 "lane artifact documented with the local test as proof"
  branch. No second defect exists.

## 3. Why matrix F still stops (owned by Plan 303, not a defect)

The lane seeds the publisher's key (`replication-targets-seeded count=3`,
including the reference client) and the client republishes byte-identical
bytes, so the store is idempotent on every run and replication can never
trigger from the publisher store. Matrix F as written is structurally
unpassable in this lane shape. Changing the seed set is a lane-semantics
change, deliberately out of Plan 302 §5 scope ("observability rows only"
for the driver). Plan 303 owns the trigger change plus matrix execution.

## 4. Verification

- `cargo fmt --all --check` clean.
- `cargo check --locked --workspace --all-targets` clean.
- `cargo test --locked --workspace --all-targets -- --test-threads=1`:
  **3249 passed, 31 ignored, 0 failed** (109 suites; +4 rows vs the Plan 278
  head: 2 lib pin rows, 2 lifecycle idempotency rows).
- `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` clean.
- `RUSTDOCFLAGS="-D warnings" cargo doc` + workspace doc-tests clean.
- `scripts/check-dependency-direction.sh`, `check-runtime-boundaries.sh`,
  `check-m12-floodfill-boundaries.sh` all pass.
- Fix commit: `389846f`.

## 5. Security / resource review

- Narrow production delta: one encoder function plus its three outer call
  sites; no guard, policy, budget, declaration, or threshold change.
- Overflow fails closed (`checked_add`, `u32::try_from` → `EncodingFailure`,
  never a wrapped expiration). Bounded 30 s horizon unchanged.
- No secret material touched; evidence rows are counts/category labels only
  (hashes redacted to the existing TSV convention; no payloads, no keys).
- No `unsafe`, no new dependencies, no runtime-boundary change.

## 6. Unblock audit

- Plan 278 stays stopped (budget spent, matrix unexecuted) — unchanged.
- Plan 279 stays blocked — now on the Plan 303 matrix-execution path.
- Plans 284/285 stay retained — unchanged.
- No capability, version, or RouterInfo advertisement change of any kind.
- Plan 303 registered as the matrix-execution successor: fresh publisher
  trigger (unseeded publisher key) + matrix A–I under its own frozen budget.
  It must not widen any spent budget.

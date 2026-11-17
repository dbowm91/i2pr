# Plan 507 — the NetDB retention policy: prune what the router would refuse to accept

**Status:** registered
**Subsystem:** `network-participation`
**Predecessors:** Plan 103 (§3.4's explicit prune), Plan 185 (the one-hop
exploratory tunnels), Plan 488 (the peer-profile store), Plan 491 (the
API-version floors), Plan 503 (the live DSM version gate), Plan 504 (the
liveness scheduler), Plan 505 (the exploratory build driver), Plan 506 (the
peer-knowledge reconcile)
**Closure record:** `plans/closure/network-participation/507-status.md`

---

## 1. Objective

**The router's peer records must not outlive the router's own acceptance
window, and the router's NetDB must turn over instead of freezing.**

One bounded outcome: the daemon **prunes** the records its own freshness
policy would refuse, on a cadence the router already has, so that

- a RouterInfo the router would refuse to **accept** today is not still
  **retained**, selectable, or askable tomorrow; and
- a store that reaches its quota **makes room** rather than refusing every
  new peer forever.

## 2. Why ready

- **The mechanism exists.** `RouterInfoStore::prune(now, max_age_ms)`
  (`crates/i2pr-netdb/src/store.rs:251`) is the tree's own retention
  operation, and Plan 103 §3.4 states the design rule it embodies: *explicit
  prune over silent eviction*. It is deterministic (the caller supplies
  `now`), it releases the byte accounting through the existing `remove`, and
  it returns the number of records removed.
- **The policy input exists.** `DEFAULT_MAX_AGE` — 24 hours — is the
  window the router's **own** `RouterInfoValidationPolicy` already refuses
  to accept past (`crates/i2pr-netdb/src/router_info.rs:20`, applied at
  `:176`). The prune window is therefore **derived from the tree's existing
  acceptance rule**, not invented by this plan.
- **The cadence exists.** Plan 506's reconcile already runs on the NetDB
  participation maintenance period (`config.netdb.participation_maintenance_period_ms`),
  which is the router's one peer-cadence source of truth. The prune reuses
  it; there is no second knob.
- **The hard dependencies are closed.** Plan 506 is closed
  (`passed-peer-knowledge-reconcile`); the reconcile it delivered is the
  thing whose inputs the prune must keep fresh.
- **No interface dependency is outstanding.** The prune touches only
  `i2pr-netdb`'s existing `RouterInfoStore` API and the daemon's existing
  composition.

## 3. Current implementation evidence

### 3.1 The retention operation has no production caller

`RouterInfoStore::prune` is defined at
`crates/i2pr-netdb/src/store.rs:251`. The tree's own search finds exactly
**one** call site: `crates/i2pr-netdb/src/store.rs:503`, which is the
operation's **own unit test**. There is **no production caller**.

That is the Plan 185 → Plan 504 → Plan 505 → Plan 506 pattern again: the
mechanism is complete and only its **input** is missing.

### 3.2 Both live stores grow without bound and freeze at their quota

Two `RouterInfoStore` instances are live in the daemon, and neither is ever
pruned.

**The bootstrap NetDB.** `crates/i2pr-daemon/src/bootstrap.rs` owns it. It
is fed by:

- every accepted unsolicited `DatabaseStore`
  (`Bootstrap::ingest_authenticated_router_store`,
  `crates/i2pr-daemon/src/bootstrap.rs:288`), which is the **live** path
  Plan 506's duplicate-ingest fix touched;
- the reseed pipeline.

Its quota is the daemon's `[netdb] max_records` / `max_encoded_bytes`
(`crates/i2pr-daemon/src/lib.rs:4941`; defaults 4 096 records and 4 MiB,
`crates/i2pr-daemon/src/config.rs:1126`).

**The destination coordinator's authoritative store.**
`crates/i2pr-daemon/src/destination_tunnels.rs` owns it, and it is fed by:

- the startup seed (`insert_validated_router_info`,
  `crates/i2pr-daemon/src/destination_tunnels.rs:698`), and
- **Plan 506's reconcile**, which on every cadence copies the bootstrap's
  validated RouterInfos into it.

Its quota is `RouterInfoStoreConfig::default()` — the same 4 096 records
and 4 MiB.

`RouterInfoStore::insert` refuses a new record at the quota
(`crates/i2pr-netdb/src/store.rs:158`) with
`InsertOutcome::CapacityExceeded`, and **never evicts**. So once a store is
full, the router **refuses every new peer, permanently**: the NetDB stops
learning, and the destination coordinator's candidate population stops
growing. Plan 506 recorded exactly this as a limitation — *"the destination
store refuses at capacity and never evicts, so a router whose destination
store is full will keep refusing new peers there"* — and left it open as
*"the destination store's capacity and eviction policy"*.

### 3.3 The retention rule and the acceptance rule disagree

The router's own acceptance policy **refuses** a RouterInfo published more
than 24 hours ago (`crates/i2pr-netdb/src/router_info.rs:20`,
`RouterInfoValidationPolicy::default_const` at `:176`; the stale refusal is
`RouterInfoValidationError::Stale` at `:115`).

Nothing applies that window to **retention**. So the router will refuse to
**accept** a peer's RouterInfo today while still **holding** a copy of that
peer's RouterInfo from a week ago — and that stale copy is still:

- selectable as a **tunnel-build peer** (Plan 491's gate reads the peer's
  published version from the record);
- selectable as a **DSM target** (Plan 503's gate reads the authoritative
  store);
- selectable as a **lookup target** (the destination coordinator's
  `select_floodfill_candidates`);
- and, for the bootstrap, part of the **floodfill advertiser count** the
  router reports in its own snapshot.

This is the same shape as the gap Plan 491 found and Plan 503 closed for the
DSM paths: **the acceptance rule was enforced; the retention rule was not.**

### 3.4 The peer-profile store is bounded already, and self-corrects

`crates/i2pr-netdb/src/profile_store.rs` holds a second `RouterInfoStore`
(`self.peers`). It **does** have an eviction policy — `evict_worst`
(`crates/i2pr-netdb/src/profile_store.rs:1037`) removes the least useful
profile when the profile ceiling or a tier ceiling is reached, and
`ProfileStore::remove` releases the record and its bytes together.

It is therefore **bounded** and cannot freeze. It can still hold a stale
record, but it **self-corrects**: Plan 505's driver records an unreachable
or misbehaving peer, the profile's tier drops, and the eviction policy then
makes room for it. A stale peer in the profile store is a *quality* question
with an existing mechanism; a frozen NetDB is a *liveness* question with
none. This plan fixes the liveness question and records the other.

### 3.5 The neighbour: Plan 503's lookup-target finding

Plan 503 recorded that *"the lookup-target selection is not gated"* — the
router still asks a below-floor floodfill for a *lookup*. That finding is a
**different** gap: it is about which peers the router may **ask**, not about
which peers it **retains**. The specification's API-version table
(`crates/i2pr-proto/src/common/version.rs:341`) carries **two** obligation
kinds — `constrains_tunnel_peers` and `constrains_dsm_targets` — and
**neither** constrains a lookup target, so there is no **derived** floor
for one. Plan 503 was right to leave that to its own plan-of-record: it
needs a **product decision** about which floodfills the router may ask at
all, and inventing one here would shrink the router's reach without a
protocol basis. It stays open and is **not** this plan's subject.

## 4. Invariants that must not regress

1. **No new task, timer, listener or socket.** The prune is a step on the
   daemon product's **existing** tick, beside Plan 506's reconcile.
2. **The prune is bounded.** It walks a bounded store and removes at most
   the store's record count.
3. **The prune never removes a record younger than the window.** A record
   inside the window is exactly one the router would still accept.
4. **The prune never removes the router's own RouterInfo.** The local
   record is not in the store (`Bootstrap::local` is a separate field), so
   this is structural — and it must be asserted.
5. **The byte accounting is released with the record.** `remove` already
   does this; a prune must not leak capacity.
6. **Every prune is counted.** A prune that silently drops peers is a
   silent skip. The counters carry counts and stable reason labels only:
   no peer identity, no address, no key, no payload.
7. **Fail closed.** If the prune cannot run, the store freezes and the
   condition is counted — the router never evicts to make room and never
   reports a zero it did not measure.
8. **Plan 491's version gate is not weakened.** The prune removes records;
   it never selects and never writes a selection. A record it removes was
   already ineligible to be *asked* under Plan 491's gate in the common case,
   and the gate itself is unchanged.
9. **Plan 506's reconcile keeps its invariants.** The prune runs **before**
   the reconcile's inserts, so the reconcile's writes have room, and the
   reconcile's snapshot is taken **after** the prune, so it copies the
   fresh set and never re-copies a record the prune removed.
10. **No wire change and no new dependency.** The prune is a local
    operation over records the router already holds.

## 5. Scope

### In scope

- A **production prune** of the bootstrap NetDB and of the destination
  coordinator's authoritative store.
- The **window**: the tree's own `DEFAULT_MAX_AGE`, so acceptance and
  retention agree. **No new configuration key.**
- The **cadence**: the existing NetDB participation maintenance period.
  **No new configuration key.**
- The **ordering**: prune the bootstrap, prune the destination store, then
  reconcile — so the reconcile copies only fresh records and its writes have
  room.
- The **counters**: records pruned per store, and the refusal of a prune
  that could not run, published through the inspection seam.
- The **documentation** of what the prune changes: the router's reported
  NetDB counts, and the floodfill-exhaustion consequence (§7).

### Explicitly out of scope

- **The lookup-target version gate** (Plan 503's finding). It needs a
  product decision with no protocol basis, and it is a different axis:
  which peers the router may **ask**, not which it **retains**.
- **The peer-profile store's age policy.** It is bounded and it
  self-corrects through the observation tiers (§3.4). Changing it would
  discard observation history, which is a different trade-off.
- **The destination store's quota values.** The prune changes what happens
  *at* the quota; it does not raise or lower the quota.
- **A RouterInfo-kind NetDB lookup**, Plan 497 limitation 5's read half,
  Plan 487 F3's destination-inbound half, Plan 491 F2 and F4, Plan 490
  Medium #3, Plan 502 M1, Plan 503's lookup-target gate, and the
  stock-peer question — all remain open with their own reasons.
- **The reseed pipeline's own retention.** The reseed loader's own bounds
  are unchanged.

## 6. Production changes

| File | Change |
| --- | --- |
| `crates/i2pr-daemon/src/bootstrap.rs` | a prune method that prunes the store and refreshes the snapshot |
| `crates/i2pr-daemon/src/destination_tunnels.rs` | a prune method on the coordinator |
| `crates/i2pr-daemon/src/peer_reconcile.rs` | the prune step, the ordering against the reconcile, and the counters |
| `crates/i2pr-daemon/src/service_product.rs` | the product drives the prune on its existing tick |
| `crates/i2pr-daemon/src/i2pcontrol_inspection.rs` | the prune counters published, following the existing `publish_*` / `*()` pattern |
| `crates/i2pr-daemon/tests/netdb_retention_boundary.rs` | **new** — the static guard |
| `crates/i2pr-daemon/tests/netdb_retention_live.rs` | **new** — the out-of-crate lane |

**`crates/i2pr-daemon/src/config.rs` is not changed.** The window is the
tree's own acceptance window and the cadence is the existing participation
period; there is no new key. (Plan 506's plan-of-record originally listed
`config.rs` and was corrected when the code landed, because the same
reasoning applied there.)

**`crates/i2pr-netdb` is not changed.** `prune` already exists and already
releases the byte accounting; the plan adds a **caller**, not a mechanism.
This is a deliberate constraint: changing the shared `RouterInfoStore`
would change the semantics of every store in the tree, and the plan's
subject is the *daemon's* retention, not the store's API.

## 7. Design decisions the plan must make explicit

These are the decisions the implementation has to settle, recorded here so
they are not made silently.

### 7.1 The window is the acceptance window

The prune window is `DEFAULT_MAX_AGE` — 24 hours — **not** a new value. The
invariant is *"the router retains exactly what it would accept"*. A second
value would be a second source of truth and could disagree with the
validator, which is the defect class this plan exists to remove.

### 7.2 The cadence is the participation maintenance period

The prune runs on the same tick and the same period as Plan 506's
reconcile. One cadence, one source of truth, and the prune and the
reconcile can never drift apart in time.

### 7.3 The ordering is prune, prune, reconcile

1. Prune the **bootstrap** under its lock, and take the reconcile's
   snapshot **under the same lock** — so the snapshot is the *fresh* set and
   the two cannot skew.
2. Prune the **destination store** — so the reconcile's writes have room.
3. **Reconcile** — insert the fresh snapshot into the destination store and
   learn it into the profile store.

A record the prune removed from the bootstrap is therefore never copied
into the destination store, and a record the destination store still holds
from an earlier tick is removed before the new set arrives.

### 7.4 The floodfill-exhaustion consequence is recorded, not designed away

A RouterInfo older than the window is pruned, **including a floodfill's**.
If every floodfill the router knows goes stale at once, the router has no
lookup target and no publication target until it learns new ones. That is
the honest consequence of an age policy, and it is the same posture a real
NetDB takes: a peer that stops republishing is a peer the network has
forgotten. The plan records it as a **finding**, and the mitigation is
visible rather than silent: the router's own reported floodfill count drops,
and the reconcile and the reseed path repopulate the set.

The plan does **not** protect floodfills from the prune, because protecting
them would mean the router retains records it would refuse to accept — the
exact inconsistency this plan removes.

### 7.5 A prune that cannot run is counted

If the bootstrap's lock is poisoned, or the destination store cannot be
locked, the prune does not run, the store keeps its contents, and the
condition is counted under a stable reason label. It is never silently
skipped, and the router never reports a prune it did not perform.

## 8. Work packages

### WP1 — the feasibility artefact

An executable artefact, before any production change, that answers the
question by running code:

> Does pruning the bootstrap's and the destination store's records with the
> tree's own acceptance window actually make the stores **turn over** — does
> a store at quota accept a **new** peer after a prune, and does a store
> with fresh records keep them?

The artefact drives the **real** `RouterInfoStore`, the **real**
`Bootstrap` and the **real** `DestinationTunnelCoordinator`, and asserts:

- a store at quota refuses a new peer **before** the prune and **accepts**
  it after;
- a record inside the window survives the prune;
- a record outside the window does not;
- the byte accounting is released with the removed record;
- the router's own local RouterInfo is never a prune victim;
- the prune is **deterministic** for a fixed `now`.

The artefact carries its own **negative controls**: a prune that removes
everything, a prune with a zero window, and a prune that skips the byte
release must each fail a named assertion. The plan **stops** if the
artefact cannot show a store turning over.

### WP2 — the bootstrap prune

A prune method on `Bootstrap` that prunes the store with the window and
refreshes the snapshot, so the router's reported counts stay true.

### WP3 — the destination-store prune

A prune method on `DestinationTunnelCoordinator` that prunes its
authoritative store with the same window.

### WP4 — the prune step and the ordering

The prune runs on the product's existing tick, in the order §7.3 sets out:
prune the bootstrap and snapshot under one lock, prune the destination
store, then reconcile.

### WP5 — the counters and the inspection seam

The counters carry counts and stable reason labels only; an unpublished or
poisoned slot is a **gap**, never a fabricated zero.

### WP6 — the controls

A static guard (`netdb_retention_boundary.rs`) and an out-of-crate live
lane (`netdb_retention_live.rs`), following the technique Plans 503, 504,
505 and 506 established. The guard is **structural**: a named function's own
body must contain the required call, matched with its open parenthesis;
comments stripped; `#[cfg(test)]` blocks excluded; an absent function
fails closed. Every rule carries an inverted probe on a copy of the real
source, and each probe asserts that the mutation applied.

### WP7 — the documentation

`docs/architecture/i2pr-daemon.md`, `specs/CONFORMANCE.md`,
`specs/support.toml`, and the registry and roadmap rows.

## 9. Failure, cancellation, restart, and contention semantics

| Condition | Behaviour |
| --- | --- |
| The bootstrap's lock is poisoned | the prune does not run; counted under a stable reason; the store keeps its contents |
| The destination store cannot be locked | the prune does not run; counted; the reconcile still runs |
| A record is outside the window | removed; counted; its bytes are released |
| A record is inside the window | kept, always |
| The local RouterInfo | never in the store; never a victim |
| The store is below its quota | the prune still runs and removes the stale records; the quota is not the trigger |
| The store is at its quota | the prune makes room **before** the reconcile's writes, so a new peer is accepted rather than refused |
| A concurrent ingest | the prune and the ingest both take the bootstrap's lock; one wins, the other waits; neither corrupts the accounting |
| The process restarts | the prune is a step, so there is nothing to restore; the stores are rebuilt from the network and the reseed path |
| The tick does not run | the prune does not run; the store freezes; the condition is visible in the counters, never silent |

**No lock is held across an await**, and the prune holds one lock at a time,
in the order §7.3 sets out.

## 10. Compatibility and migration

- **No migration.** The prune removes records the router would refuse to
  accept; it changes no format, no key and no configuration.
- **A peer the router knew yesterday may be unknown today.** That is the
  intended behaviour: the record was outside the router's own acceptance
  window. A peer that republishes is re-learned and stays.
- **No configuration key** is added or removed, so no deployment needs a
  configuration change and no operator can widen the window.
- **The router's reported NetDB counts** (the bootstrap snapshot) become
  smaller and truthful. Anything that read those counts as a growth
  expectation must read them as a *current* count.

## 11. Required tests

1. **The feasibility artefact** (WP1), with its negative controls.
2. **The static guard**: the prune runs on the existing tick; the prune
   owns no task and no timer; the window is the tree's acceptance window
   and not a new constant; the cadence is the existing participation
   period; the ordering is prune-prune-reconcile; the prune uses
   `RouterInfoStore::prune` and not a hand-rolled removal; the counters are
   published and the seam returns a gap rather than a zero; the byte
   accounting is released; the local RouterInfo is never a victim.
3. **The live lane**: a real `Bootstrap` and a real
   `DestinationTunnelCoordinator`, a fixed seed and an explicit `now_ms`,
   no environment variable, no `#[ignore]`, no stock peer, no sleeping.
4. **The existing lanes unchanged**: Plan 491's, Plan 497's, Plan 503's,
   Plan 504's, Plan 505's and Plan 506's guard and live lanes, plus the
   daemon's lib suite.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-tooling-inventory.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

Focused:

```text
cargo test --locked -p i2pr-daemon --test netdb_retention_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test netdb_retention_live -- --test-threads=1
cargo test --locked -p i2pr-daemon --lib peer_reconcile -- --test-threads=1
cargo test --locked -p i2pr-daemon --test peer_reconcile_boundary --test peer_reconcile_live -- --test-threads=1
cargo test --locked -p i2pr-daemon --test version_floor_boundary --test version_eligibility_live -- --test-threads=1
cargo test --locked -p i2pr-daemon --test exploratory_driver_boundary --test exploratory_driver_live -- --test-threads=1
```

## 13. Documentation updates

- `docs/architecture/i2pr-daemon.md` — the retention owner, the window, the
  cadence, the ordering, the counters, and the floodfill-exhaustion
  consequence.
- `specs/CONFORMANCE.md` — the Plan 507 section, including the explicit
  statement that the router now retains exactly what it accepts, and that
  **no stock-peer evidence** exists.
- `specs/support.toml` — one `experimental` surface with
  `advertised = false`.
- `plans/registry.md`, `plans/subsystems/network-participation-roadmap.md` —
  the Plan 507 rows.

## 14. Acceptance criteria and stop conditions

### 14.1 Acceptance

1. A store at its quota **makes room** and accepts a new peer after a
   prune, where it refused before.
2. A record inside the acceptance window survives; a record outside it does
   not.
3. The router's **local** RouterInfo is never a prune victim.
4. The byte accounting is released with every removed record.
5. The prune runs on the **existing** tick: the product gains **no** task,
   no timer, no listener and no socket.
6. The window is the tree's **acceptance** window and the cadence is the
   **existing** participation period: **no new configuration key**.
7. The reconcile's snapshot is the **pruned** set, and the reconcile's
   writes have room.
8. Every prune is **counted by reason**, and an unpublished slot is a
   **gap**, never a fabricated zero.
9. Plan 491's version gate and Plan 506's reconcile invariants do not
   regress.
10. Nothing is advertised; `Cargo.toml` and `Cargo.lock` are
    byte-identical.
11. The static guard and the live lane are green, and every existing lane
    still passes unchanged.

### 14.2 Stop conditions

The plan stops, and the gap is recorded with the exact boundary, if any of
these holds:

- WP1's artefact cannot show a store turning over.
- Closing it would require a wire-format change.
- Closing it would require a new dependency.
- Closing it would require changing `RouterInfoStore`'s API, or its
  semantics for any store outside the daemon's two.
- Closing it would require a **new configuration key** — the window and the
  cadence must both be derived from what the tree already has.
- Closing it would require holding two locks at once, or a lock across an
  await.
- Closing it would require changing the peer-profile store's policy.
- Closing it would require weakening Plan 491's version gate or Plan 506's
  reconcile invariants.

### 14.3 What this plan does **not** close

- **The lookup-target version gate** (Plan 503's finding) — it needs a
  product decision with no protocol basis (§3.5).
- **The peer-profile store's age policy** — bounded and self-correcting
  (§3.4).
- **A RouterInfo-kind NetDB lookup.** The router performs none in
  production.
- **Plan 497 limitation 5's read half.** The routing table's only reader
  remains Plan 433's never-constructed `NetDbTunnelCoordinator`.
- **Plan 487 F3's destination-inbound half.** It needs tunnel rekeying,
  which the tree does not implement at all.
- **Plan 491 F2 and F4, Plan 490 Medium #3, Plan 502 M1, Plan 503's
  lookup-target gate.** Unchanged.
- **The stock-peer question.** Whether an ordinary public router sends
  unsolicited stores the router will accept is unproven.

## 15. Closure evidence required

1. The WP1 feasibility artefact, with its measured answer.
2. The requirement-to-evidence matrix, with every requirement mapped to a
   `file:line` and to an executable check.
3. The full routine floor, green, with the exact command list and the step
   count.
4. The focused verification table.
5. The invariant, failure, migration and security reviews.
6. The limitations, the findings by severity, and the disposition of the
   destination store's capacity limitation Plan 506 recorded.
7. The unblock audit.

## 16. Handoff notes

**The freeze is the defect to watch.** A router that has been up for a week
stops learning peers, and no counter moves. That is why the prune's
counters are published and why the guard asserts that the prune runs on the
existing tick: an installed-but-never-called prune is exactly the failure
mode Plans 504, 505 and 506 each found in their own predecessor.

**The floodfill-exhaustion consequence is real and is not designed away.**
See §7.4.

**Do not add a window constant.** The whole point is that the router
retains exactly what it accepts. A second value is a second source of truth
and can disagree with the validator.

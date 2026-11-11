# Plan 540 — the NetDB's prune must not walk the population

**Status:** registered. **Subsystem:** network-participation.
**Registered:** 2026-11-11. **Successor to:** Plan 539.

## 1. Objective

The router's NetDB lock must not be held across a **population walk** to prune.

The validated store's prune finds the stale records by walking **every**
retained record, and the daemon's peer reconcile calls it **under the router's
NetDB lock**. The prune is on the router's **live maintenance cadence**, so the
router's NetDB lock is held across an O(population) walk, on a path that every
other NetDB reader queues behind.

The plan makes the store maintain an **age index** — the retained records'
publication times, ordered — on exactly the paths that insert and remove a
record, so the prune walks only the **stale** records. The prune's cost becomes
proportional to what it **removes**, not to the population. The prune's
**signature** and its **semantics** are unchanged: it still removes every
record past the window, in one call.

## 2. Why ready

The tree already has every seam this plan needs, and every predecessor in this
line has left the shape in place.

**The store already accounts one number the same way.** Plan 538 made the
floodfill advertiser count **accounted, not derived**: the store maintains it on
exactly the paths that insert and remove a record, and `stats()` reads the
maintained counter instead of walking the record map
(`crates/i2pr-netdb/src/store.rs:92`). This plan applies the **same discipline**
to the second number the prune needs — the record's age — and the same shape of
proof: the lane proves the **behaviour**, the guard proves the **shape**.

**The prune's signature is already pinned, and this plan does not move it.**
Plan 507's retention boundary guard pins the store's prune call
(`crates/i2pr-daemon/tests/netdb_retention_boundary.rs:54`) and the store's own
prune signature, so the daemon's call site and the store's public surface stay
exactly as they are. Plan 507's guard needs **no** extension: the prune keeps
its name, its parameters and its return, and the bootstrap keeps calling
`self.store.prune(Date::from_millis(now_ms), window_ms)`
(`crates/i2pr-daemon/src/bootstrap.rs:292`).

**The store's records are already ordered.** The store's records live in a
`BTreeMap` keyed by the router's hash
(`crates/i2pr-netdb/src/store.rs:84`), and the router's hash is `Ord`
(`crates/i2pr-netdb/src/router_info.rs:38`), so an ordered index of
`(publication time, hash)` is a natural second collection beside it.

**The line's other plans have already made every other NetDB read bounded.**
Plan 536 removed the relay target's answer from the lock, Plan 537 removed the
relay requester's outbound step from it, Plan 538 removed the bootstrap's
population walk from it, and Plan 539 removed the routing table's population
from it. The prune is the **last** operation that holds the lock across an
unbounded walk.

## 3. Current implementation evidence

### 3.1 The prune walks every record

`RouterInfoStore::prune` (`crates/i2pr-netdb/src/store.rs:291`) finds the stale
records by scanning the whole record map:

```rust
let stale_keys: Vec<RouterHash> = self
    .records
    .iter()
    .filter_map(|(key, record)| {
        let age = now
            .as_millis()
            .saturating_sub(record.published().as_millis());
        if age > max_age_ms { Some(*key) } else { None }
    })
    .collect();
```

The scan is `crates/i2pr-netdb/src/store.rs:293`. It touches every retained
record, whether or not the record is stale.

### 3.2 The prune runs under the router's NetDB lock

The daemon's bootstrap owns the only validated `RouterInfoStore` in the daemon
(`crates/i2pr-daemon/src/bootstrap.rs:215`), and its mutex **is** the router's
NetDB lock. The bootstrap's prune calls the store's prune
(`crates/i2pr-daemon/src/bootstrap.rs:292`), and the reconcile calls the
bootstrap's prune:

```rust
PruneOutcome::Removed(guard.prune(now_ms, RETENTION_WINDOW_MS))
```

at `crates/i2pr-daemon/src/peer_reconcile.rs:424`, under the guard taken at
`crates/i2pr-daemon/src/peer_reconcile.rs:421`.

### 3.3 The prune is on the router's live maintenance cadence

The reconcile's `run` calls `prune_stores` before it takes its snapshot, so the
prune is part of the reconcile the product already drives. The prune is
therefore on the router's live path, not a start-up task.

### 3.4 So the router's NetDB lock is held across a population walk

While the prune scans, the router's NetDB lock is held. Every other NetDB
reader queues behind it: the routing table's population, the reconcile's own
snapshot, the discovery tick, the peer pool and the publication path.

### 3.5 What the plan does not claim

The prune still **removes** the stale records, and removing a record still
costs what removing it costs. The plan does not claim the prune is free; it
claims the prune's cost is proportional to what it removes, and not to the
population it did not remove.

## 4. Invariants

1. **The prune removes every record past the window, in one call.** The age
   index changes how the stale records are **found**, never **which** records
   are removed. A prune that removes nothing and a prune that removes
   everything stale are the same prune they were before.
2. **The prune's cost is proportional to the number of stale records, not to
   the population.** A prune that removes nothing must not examine the
   population.
3. **The age index is maintained, not derived.** It is updated on exactly the
   paths that insert and remove a record: the **new-key** path, the
   **replacement** path and the **removal** path. It is never rebuilt by a
   scan.
4. **The index is exact.** Every retained record has exactly one index entry,
   and every index entry names a retained record. No sampling, no cap and no
   estimate: a stale record that is missing from the index would never be
   removed, and that is a silent retention failure the lane must catch.
5. **The prune's public surface is unchanged.** `pub fn prune(&mut self, now:
   Date, max_age_ms: u64) -> usize` keeps its name, its parameters and its
   return, and Plan 507's pinned call site and signature stay exactly as they
   are.
6. **The store's other public surfaces are unchanged.** `insert`, `remove`,
   `get`, `contains`, `len`, `is_empty`, `encoded_bytes`, `config`, `stats`,
   `iter` and `floodfill_advertisers` keep their signatures and their results.
7. **The byte accounting and the floodfill advertiser count are unchanged.**
   The prune still releases the record's bytes and its advertiser with the
   record, and Plan 538's maintained count is still maintained.
8. **The router's own RouterInfo is never a prune victim.** It lives beside
   the store, not inside it, and the plan does not move it.
9. **The daemon's prune call site is unchanged.** The bootstrap still calls
   `self.store.prune(Date::from_millis(now_ms), window_ms)` under the
   bootstrap's lock, and the reconcile's shape — prune, then snapshot, then
   learn — is unchanged.
10. **No configuration key is added, and no dependency is added.** The index's
    budget is not a knob: the index is not a budget, it is an index.
11. **No new task, timer, listener or socket.** The prune rides the reconcile
    the product already drives.
12. **No advertisement.** `router.version` stays `0.9.62` and the inventory's
    advertised count stays **zero**.
13. **Plan 507's, Plan 538's and Plan 539's lanes and guards stay
    byte-identical.** This plan changes the store's prune and the index it
    maintains, not the relay roles' semantics, not the store's accounting and
    not the composition root's population.

## 5. Scope

### 5.1 In scope

- `crates/i2pr-netdb/src/store.rs` — the store's age index and the prune that
  walks it.

### 5.2 Out of scope

- The store's **selection** paths. `floodfill_advertisers()`
  (`crates/i2pr-netdb/src/store.rs:313`) still yields exactly the advertisers,
  and a selection over the floodfill advertisers is a walk by its nature.
- The store's **statistics**. `stats()` stays O(1) and walks no record map,
  as Plan 538 made it.
- The **daemon's** prune call site, the bootstrap's public surface and the
  reconcile's shape.
- The store's byte accounting and Plan 538's floodfill advertiser count.

### 5.3 Out of scope, with the owner named

- **The router's validated store is still the bootstrap's store.**
  (`crates/i2pr-daemon/src/bootstrap.rs:215`) The bootstrap owns the only
  validated store in the daemon, so the router's NetDB lock is still the
  bootstrap's mutex. **Owner:** the successor that gives the router a NetDB
  owner of its own. This plan does not change that, and it **reduces** what
  that successor must do: once the prune's cost is proportional to what it
  removes, the NetDB lock is no longer held across a population walk, so
  separating the ownership is no longer the thing that makes the lock
  dangerous.
- **The composition root's shape is proven statically, not behaviourally.**
  **Owner:** the successor that makes the composition root's relay arm
  reachable from a test.
- **The production SSU2 peer-test driver** — Plan 530's Low finding 2.
- **The `B` advertisement and `caps=f`** — their own owners.
- **The stock-peer half** — Plan 493's authorized two-host topology.

## 6. Required production changes

### 6.1 `crates/i2pr-netdb/src/store.rs`

1. `RouterInfoStore` (`crates/i2pr-netdb/src/store.rs:83`) gains a
   **private** field: an ordered set of the retained records' publication times
   and keys. The field is private, so the store's public surface is
   unchanged.

2. The index is maintained on **exactly** the paths that insert and remove a
   record. The insert path (`crates/i2pr-netdb/src/store.rs:126`) and the
   removal path (`crates/i2pr-netdb/src/store.rs:207`):

   - the **new-key** path adds the incoming record's entry;
   - the **replacement** path removes the existing record's entry and adds the
     incoming one, because a replacement may carry a different publication
     time;
   - the **removal** path removes the removed record's entry.

   The refusal paths — the stale replacement, the idempotent insert, the
   conflict and the capacity refusal — change nothing, so they change no
   entry.

3. `prune` walks the **index's stale prefix** and removes those records. The
   prefix is exact: a record is stale when
   `now.as_millis().saturating_sub(record.published().as_millis()) > max_age_ms`,
   which is the same condition as a publication time strictly before
   `now.as_millis().saturating_sub(max_age_ms)`. The index is ordered by
   publication time first, so the stale records are a contiguous prefix of it
   and the walk is proportional to the number of stale records.

4. The prune's **signature** is unchanged and the store's other public
   surfaces are unchanged.

## 7. Design decisions

### 7.1 The index is maintained, never rebuilt

A derived index — one rebuilt by a scan — would move the population walk into
the index's construction and make the plan pointless. The index is maintained
on the same paths that maintain the byte total and the floodfill advertiser
count, which is the discipline Plan 538 established.

### 7.2 The index is exact, not approximate

An approximate index would be a silent retention failure: a stale record that
the index misses is never removed, and the router's NetDB keeps a peer it has
decided to forget. There is no sampling, no cap and no estimate.

### 7.3 The stale prefix is exact, and the arithmetic is saturating

The prune's original predicate is `age > max_age_ms`, with `age` computed by a
saturating subtraction. The prefix form is `published < now - max_age_ms`,
with the subtraction saturating too. The two agree in every case, including
the two edge cases: a record published in the future has a saturated age of
zero and is not stale, and a `now` below the window has no stale record at
all. The lane proves both edge cases against the store's own iterator.

### 7.4 The prune's signature does not move

Plan 507 pins the store's prune signature and the daemon's call site. Moving
the signature would mean extending a closed plan's guard for no gain: the
signature is not what makes the prune expensive, the **walk** is.

### 7.5 The index is bounded by the record ceiling

The index holds one entry per retained record, so it is bounded by the same
`max_records` ceiling as the record map. Its own size is small beside the
records it indexes: an entry is a publication time and a router hash, where a
record is a whole signed `RouterInfo`.

### 7.6 The lane proves the behaviour and the guard proves the shape

The prune's cost is a property of its **shape**, and the shape is not
observable from outside the store. So the lane proves what the prune *does* —
that it removes exactly the stale set, that the index never drifts, that the
accounting is released — and the guard proves what the prune *is* — that it
walks the index and not the record map. This is the same split Plan 538 used
for `stats()`.

### 7.7 No configuration key

The index is not a budget, so it has no knob. A second knob would be a second
source of truth, which Plan 507's rule 6 forbids.

## 8. Work packages

1. The plan of record, the registry's row and the roadmap's row.
2. The production change in `crates/i2pr-netdb/src/store.rs`.
3. The lane and the guard.
4. The documentation, the inventory and the plan's amendment log.
5. The floor, the closure record and the registry's closed row.

## 9. Failure, cancellation, restart and contention semantics

**Failure.** The index is plain data beside the record map, and every mutation
path updates both. A refusal updates neither. There is no new failure mode:
the prune's result is the same set the prune removed before.

**Cancellation.** The change touches no async path. The store's mutation
methods are synchronous and the prune is synchronous, so there is nothing to
cancel and no lock held across an await.

**Restart.** Nothing is persisted. The index is rebuilt with the store, from
the store's own inserts, so a restart cannot inherit a stale index.

**Contention.** The router's NetDB lock is no longer held across a population
walk to prune. The prune's hold is proportional to the number of peers it
actually removes, and the prune's cost when it removes nothing is a lookup of
the index's prefix rather than a scan of the population. The store's
**selection** path is still a scan, because a selection over the floodfill
advertisers is a walk by its nature, and that keeps its named owner.

## 10. Compatibility and migration

**No wire change.** The prune removes the same records it removed before.

**No configuration migration.** **No key is added.** The daemon's configuration
surface is unchanged, and the NetDB configuration keeps exactly its twelve keys.

**No persisted state.** The index is derived from the store's contents and is
never persisted, so there is nothing to migrate.

**No change to the netdb crate's public surface.** No `i2pr-netdb` item is
removed, renamed or re-signatured, and the store's new field is private.

**No change to the daemon's public surface.** The bootstrap's prune, the
reconcile's shape and the counters are unchanged.

**No change to the manifest.** `Cargo.toml` and `Cargo.lock` stay
byte-identical: **no dependency is added**.

## 11. Required tests

### 11.1 The lane

`crates/i2pr-daemon/tests/netdb_prune_walk.rs` — the prune over the **real**
store, with an **independent** computation of the stale set. The lane must
prove:

1. the prune removes **exactly** the records the store's own iterator says are
   stale, and the count matches;
2. the prune removes **every** stale record in **one** call, even when the
   population is far larger than the number of stale records;
3. the prune removes **nothing** when nothing is stale, and the store's
   contents are unchanged;
4. the index does not drift: a record inserted and then made stale **is**
   removed, and a record that is **replaced** with a newer publication time is
   removed at the **new** time and not at the old one;
5. the **edge cases** agree with the store's own iterator: a record published
   in the future is never stale, and a `now` below the window removes
   nothing;
6. the prune releases the record's **bytes** and its **floodfill advertiser**
   with the record;
7. the store's **statistics** are unchanged by the prune, and the store's
   selection still yields exactly the advertisers.

The lane computes the stale set **independently**, by walking the store's own
iterator, and never by reading the index — so the lane and the store's own
bookkeeping are two different things that must agree.

### 11.2 The guard

`crates/i2pr-daemon/tests/netdb_prune_walk_boundary.rs` — a static guard that
is the authority for the **shape**. It must have an inverted probe for every
rule and a fail-closed probe for every item a rule names. The rules:

1. the store **maintains** an age index, and the index is updated on **every**
   path that inserts or removes a record;
2. the prune walks the **index** and **not** the record map;
3. the prune's **signature** is unchanged, and the store's other public
   surfaces are unchanged;
4. the daemon's prune call site and the reconcile's shape are unchanged;
5. **no** configuration key, **no** dependency and **no** task, timer, listener
   or socket is added;
6. Plan 507's, Plan 538's and Plan 539's lanes and guards stay
   **byte-identical**.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-daemon --test netdb_prune_walk -- --test-threads=1
cargo test --locked -p i2pr-daemon --test netdb_prune_walk_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test netdb_retention_feasibility --test netdb_retention_live -- --test-threads=1
cargo test --locked -p i2pr-daemon --test netdb_retention_boundary --test floodfill_statistic_accounting --test floodfill_statistic_accounting_boundary --test routing_table_population_lock --test routing_table_population_lock_boundary -- --test-threads=1
cargo test --locked --workspace --all-targets --no-fail-fast -- --test-threads=1
python3 scripts/check-tooling-inventory.py
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

plus the routine floor in `AGENTS.md`, with this plan's two steps added.

## 13. Documentation updates

- `specs/support.toml` — a new experimental, non-advertised surface
  `netdb.prune-walk`.
- `specs/CONFORMANCE.md` — the count paragraph and a section for this plan.
- `specs/QUALIFICATION-GATES.md` — the gates this plan's lane and guard
  discharge.
- `docs/architecture/i2pr-netdb.md` — the store's age index and the prune's
  walk.
- `docs/architecture/i2pr-daemon.md` — the prune's call site under the router's
  NetDB lock.
- The plan's own amendment log.

## 14. Acceptance criteria and stop conditions

The plan is **accepted** when all of the following hold:

1. the prune removes exactly the stale set, and the lane measures it against an
   independent computation;
2. the prune walks the index and not the record map, and the guard proves it;
3. the index never drifts, and the lane proves it through the prune's own
   behaviour;
4. the prune's signature, the store's public surfaces, the daemon's call site
   and the reconcile's shape are unchanged;
5. no configuration key, dependency, task, timer, listener or socket is added,
   and `Cargo.toml` and `Cargo.lock` stay byte-identical;
6. `router.version` stays `0.9.62`, and the inventory's advertised count stays
   **zero**;
7. the full floor is green, with the closure record's table complete;
8. the guard's inverted probes each fail as intended and the tree is restored
   byte-identically after each.

**Stop conditions.** The plan stops and reports if:

- the prune's semantics would change — the index changes how the stale records
  are found, never which records are removed;
- Plan 507's, Plan 538's or Plan 539's guard would have to change — the plan's
  design exists precisely so that it does not;
- the floor's `cargo deny check advisories bans sources` fails;
- a stock peer would be required to prove any of the above — the stock-peer
  half stays blocked on Plan 493 and is **not** attempted here.

## 15. Closure evidence required

- The implementation commit, the lane-and-guard commit, the documentation
  commit and the closure commit, each named in the closure record.
- The **complete** floor table, every row with its result.
- The lane's rows, each with its result.
- The guard's rows, each with its result.
- The negative controls: each mutation, what it was expected to break, what it
  did break, and the SHA-256 proof that the tree was restored byte-identically.
- The requirement-to-evidence matrix, one row per requirement.
- The invariant review, the failure review, the migration review and the
  security review.
- The limitations, the findings by severity, the roadmap disposition and the
  unblock audit.

## 16. Handoff notes

- The plan is **experimental** and default-off. Do not describe the change as
  released, supported or advertised.
- The delivery is confined to the controlled loopback profile.
- The stock-peer half stays blocked on Plan 493's authorized two-host
  topology.
- Plan 507's, Plan 538's and Plan 539's closed closure records are **not**
  rewritten. The discharge lives in this plan's closure record and in the
  registry's row.
- The plan **extends** no other plan's guard: Plan 507's, Plan 538's and Plan
  539's lanes and guards stay byte-identical.
- The plan adds no dependency, so `Cargo.toml` and `Cargo.lock` stay
  byte-identical.

## 17. Amendment log

### 17.1 — 2026-11-11, registration

Plan 540 is registered as the successor to Plan 539 in the router's NetDB-lock
line: the validated store's prune still finds the stale records by walking
**every** retained record, and the daemon's peer reconcile still calls it
**under the router's NetDB lock**, so the lock is held across a population walk
on the router's live maintenance cadence.

The scope is the store's **prune** and the index it maintains, not the store's
selection paths, not the store's statistics and not the daemon's call site. The
plan makes the store maintain an **age index** on exactly the paths that
insert and remove a record, so the prune walks only the **stale** records and
its cost is proportional to what it removes.

The prune's **signature** and its **semantics** are unchanged, and Plan 507's
pinned call site and signature stay exactly as they are, so no predecessor's
guard is extended. The population is still the same set, the accounting is
still released with the record, and the router's own RouterInfo is still never
a prune victim.

The plan adds no store, no lock, no owner, no configuration key and no timer,
it changes no transport file and no runtime file, it adds no dependency, it
leaves `Cargo.toml` and `Cargo.lock` byte-identical, and it advertises nothing.

No closed closure record is rewritten by this plan. The corrections this plan
finds, if any, live in this plan's closure record and in the registry's row.

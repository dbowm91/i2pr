# Plan 539 — the routing table's population must not hold the router's NetDB lock

## 1. Objective

The router's **routing table** must be populated **outside** the router's
NetDB lock, and the absence of a population must never be **skipped
silently**.

Two facts about the tree force this. First, the composition root populates the
table by walking **every** floodfill advertiser the NetDB holds — an
O(population) walk — and it does that walk **under the router's NetDB lock**.
Second, and worse, the walk's body calls the routing table's own `index`,
which takes the **routing table's mutex**. So the router's NetDB lock
**wraps** the routing table's mutex, for the whole population, on the router's
**live** maintenance cadence.

The plan makes the population a **short, bounded read** whose guard is
released before the read returns, so the indexing happens with **no** NetDB
lock held and the router's NetDB lock never nests inside the routing table's.
And it makes the absence of a population a **typed, counted refusal** rather
than a silent skip.

## 2. Why ready

- The routing table's machinery is **complete**: the table, the bucket set, the
  signed-evidence rule, the receipts and the counters are all in the tree and
  green (`crates/i2pr-daemon/src/netdb_participation.rs`).
- The **shape of the fix is already in the tree, thirty lines below the
  defect**. Plan 505's `learn_bootstrap_peers`
  (`crates/i2pr-daemon/src/lib.rs:2174`) takes the bootstrap's lock for **one**
  self-contained read (`crates/i2pr-daemon/src/lib.rs:2184`), lets the guard
  drop before the read returns, and learns its batch **afterwards** — and its
  own comment says why: *"the snapshot is taken under the bootstrap's lock and
  the batch is learned outside it, so the two locks are never held at once."*
  The routing table's population is the same kind of work against the same
  lock, and it does the opposite.
- The **precedent** for the fix is already in the tree. Plan 537's relay
  requester reads its material through a daemon-local trait, takes the
  bootstrap's lock for one self-contained read and releases it before the read
  returns, and the signature and the transport's writes happen afterwards.
  Plan 538's NetDB statistic is accounted the same way, and it removed the last
  O(population) **statistic** from under the lock. This plan removes the last
  O(population) **indexing** from under it: the store's selection walk stays,
  because a selection over the floodfill advertisers is a walk by its nature,
  but the walk that **indexes** them — the part that takes the routing table's
  mutex — does not.
- The defect is **internal**. No stock peer, no second host and no external
  topology is needed to find it, to fix it or to prove it.
- The **owner is named**. Plans 536, 537 and 538 all named the same successor
  for the router's NetDB **ownership**; this plan is the *lock scope* of the
  same lock, and it is the last O(population) walk the survey found on it.

## 3. Current implementation evidence

### 3.1 The population is a walk of every floodfill advertiser

The composition root's routing-table population
(`crates/i2pr-daemon/src/lib.rs:2144`) walks the store's advertisers
(`crates/i2pr-daemon/src/lib.rs:2155`):

```rust
for peer in bootstrap.store().floodfill_advertisers() {
    self.routing_table.index(peer);
}
```

`RouterInfoStore::floodfill_advertisers`
(`crates/i2pr-netdb/src/store.rs:313`) yields every retained record whose own
signed RouterInfo advertises the `f` capability, so the walk is O(the router's
peer population).

### 3.2 The walk runs under the router's NetDB lock

The same function takes the bootstrap's mutex — the router's NetDB lock —
before the walk:

```rust
let Ok(bootstrap) = bootstrap.lock() else {
    return;
};
```

The bootstrap's mutex guards the only `RouterInfoStore` in the daemon
(`crates/i2pr-daemon/src/bootstrap.rs:215`), so the walk is a population walk
**under the router's NetDB lock**.

### 3.3 The walk's body takes the routing table's own mutex

`RoutingTableConsumer::index`
(`crates/i2pr-daemon/src/netdb_participation.rs:752`) takes the table's inner
lock for the insert:

```rust
pub fn index(&self, validated: &ValidatedRouterInfo) -> Option<TableInsertOutcome> {
    self.inner
        .lock()
        .ok()
        .map(|mut guard| guard.driver.index(validated))
}
```

and the driver's `index` reaches the bucket set's `insert_signed`
(`crates/i2pr-netdb/src/bucket_set.rs:420`), which computes the day's routing
position and inserts the entry.

### 3.4 So the router's NetDB lock wraps the routing table's mutex

This is the defect's sharpest edge, and it is the **same shape** as Plan 536's
recorded finding for the runtime's state lock: the router's NetDB lock is
taken, and then a **second** lock is taken while it is held. The routing
table's mutex is also taken on its own by the lookup path and by the clock
advance, so the two locks are genuinely independent and the nesting is real.

### 3.5 The population runs on the router's live maintenance cadence

The population is not a startup-only step. It runs at startup
(`crates/i2pr-daemon/src/lib.rs:1981`) and again on **every** tick of the
participation maintenance owner (`crates/i2pr-daemon/src/lib.rs:2032`).

### 3.6 The consequence

Every tick, the router's NetDB lock is held across an O(population) walk and
across the routing table's mutex. The NetDB discovery tick, the publication
tick, the peer reconcile, the peer pool, the relay target's read and the relay
requester's read all queue behind it — and the routing table's own lookups
queue behind the NetDB lock, because the population holds it.

**What stays under the lock, and what does not.** The plan does **not** claim
to remove the store's selection walk: the source must copy the advertisers out
while it holds the lock, and a selection over the floodfill advertisers is a
walk by its nature. What the plan removes is the **indexing** — the walk that
hands each advertiser to the routing table, which is the part that takes the
routing table's mutex, once per advertiser, and the part that computes the
day's routing position and emits the receipt. That is the part the lock is
held across today, and it is the expensive part.

### 3.7 The shape of the fix is already in the tree

Plan 505's `learn_bootstrap_peers`
(`crates/i2pr-daemon/src/lib.rs:2174`) reads the bootstrap's material the way
this plan proposes to read the population's
(`crates/i2pr-daemon/src/lib.rs:2184`):

```rust
// The snapshot is taken under the bootstrap's lock and the batch is
// learned outside it, so the two locks are never held at once.
let records = {
    let Ok(bootstrap) = bootstrap.lock() else {
        return;
    };
    bootstrap.validated_router_info_snapshot()
};
self.profiles.with(|profiles| {
    profiles.advance_time(now_ms);
    profiles.learn_all(records);
});
```

That function is **thirty lines below** the defect, in the same file, and it
already says what this plan has to say about the router's NetDB lock. The
routing table's population is the same kind of work against the same lock, and
it does the opposite.

## 4. Invariants

1. **The population is read under a short lock and indexed with no lock
   held.** The router's NetDB lock is taken for one self-contained read whose
   guard is released before the read returns, and every index call happens
   after the read returns.
2. **The router's NetDB lock never wraps the routing table's mutex.** No code
   path may hold the router's NetDB lock while the routing table's inner lock
   is acquired.
3. **The population is the same set.** Every validated peer the NetDB holds
   whose **own** signed record advertises the `f` capability is indexed, and no
   other peer is.
4. **The table still indexes each peer with its own signed evidence.** The
   source yields the **records**, never a peer's word about another peer, and
   the table's own refusal of a non-advertiser is unchanged.
5. **The population's absence is never skipped silently.** The population runs
   whenever the owner exists. A missing store, or a poisoned lock, is a
   **typed, counted refusal**, not an absent pass.
6. **The refusal is a fact about this router, never about a peer.** It is
   counted under its own reason and is never written into a peer's profile.
7. **The copy is bounded.** The population the source hands over is bounded by
   the NetDB's configured record ceiling, and it is a **copy**: the store's
   own records are not moved out of it.
8. **The routing table's existing surface is unchanged.** `index`,
   `index_suggested`, the existing counters and the receipts are exactly what
   the tree already had.
9. **The store's selection surface is unchanged.** `floodfill_advertisers()`
   still yields exactly the advertisers.
10. **No configuration key is added, and no task, timer, listener or socket is
    added.** The population rides the **existing** tick, as it does today.
11. **No advertisement.** `router.version` stays `0.9.62` and the inventory's
    advertised count stays **zero**.
12. **Plan 536's, Plan 537's and Plan 538's lanes and guards stay
    byte-identical.** This plan changes the composition root's population
    **shape**, not the relay roles' semantics, and the predecessors' evidence
    must remain exactly as it was.

## 5. Scope

### 5.1 In scope

- The composition root's **routing-table population**: a daemon-local trait the
  population reads through, with one implementation for the validated store
  itself and one for the router's NetDB.
- The **typed refusal** for a population that cannot be read, and its counter.

### 5.2 Out of scope

- The **store's selection** path: `floodfill_advertisers()`
  (`crates/i2pr-netdb/src/store.rs:313`). A selection over the floodfill
  advertisers is a walk by its nature, and Plan 538 already said so.
- The **routing table's** own internals: the bucket set, the signed-evidence
  rule, the receipts and the lookups.
- The **discovery spine** and the **peer-profile store**.
- The **transport** and the **runtime**. No file under `crates/i2pr-transport/`
  and no file under `crates/i2pr-runtime/` is touched.
- The stock-peer half, which stays blocked on Plan 493.

### 5.3 Out of scope, with the owner named

- **The router's validated store is still the bootstrap's store.** The
  bootstrap owns the only `RouterInfoStore` in the daemon
  (`crates/i2pr-daemon/src/bootstrap.rs:215`). Separating the router's NetDB
  from the bootstrap's lifetime is a larger change, and this plan does not
  attempt it: it removes the **lock scope** and the **silent skip**, and
  leaves the ownership alone. **Owner:** the successor that gives the router a
  NetDB owner of its own — the same successor Plans 536, 537 and 538 named.
- **The store's prune is still O(population) under the router's NetDB lock.**
  A prune must examine every record, so that work is inherent to the
  operation. Moving it out of the lock is a consequence of the router having a
  **NetDB owner of its own**. **Owner:** the same successor.
- **The lock scope is proven statically, not behaviourally.** Plan 536's
  closure record recorded the same gap for the target role: the composition
  root's drain is not reachable from a test without a second production
  composition. This plan does not build that seam either. **Owner:** the same
  successor that makes the composition root's relay arm reachable.
- **The production SSU2 peer-test driver** — Plan 530's Low finding 2.
- **The `B` advertisement and `caps=f`** — their own owners.
- **The stock-peer half** — Plan 493's authorized two-host topology.

## 6. Required production changes

### 6.1 `crates/i2pr-daemon/src/netdb_participation.rs`

1. A **daemon-local** trait:

   ```rust
   /// The material the routing table's population is drawn from.
   ///
   /// The population is the router's **floodfill advertisers**: every
   /// validated peer the NetDB holds whose **own** signed record advertises
   /// the `f` capability. The table indexes each one with **its own signed
   /// evidence**, so the source must yield the **records**, never a peer's
   /// word about another peer.
   pub trait RoutingTablePopulationSource {
       /// Returns the advertisers to index.
       fn advertisers(&self) -> Result<Vec<ValidatedRouterInfo>, RoutingPopulationRefusal>;
   }
   ```

   It is a **daemon-local** trait, so implementing it for the netdb crate's own
   `RouterInfoStore` is inside the orphan rule.

2. `impl RoutingTablePopulationSource for i2pr_netdb::RouterInfoStore`, whose
   method returns the store's advertisers as owned records. The store's own
   selection is **unchanged**.

3. The **production** source, `ValidatedStorePopulationSource`, holding
   `Option<&Arc<Mutex<crate::bootstrap::Bootstrap>>>` and a reference to the
   owner's refusal counter. Its read takes the bootstrap's lock, performs
   **one** read, and **releases the guard before the read returns**. Without
   a bootstrap, or with a poisoned lock, it returns
   `RoutingPopulationRefusal::StoreUnavailable` and **counts** that refusal
   under its own reason.

4. A new refusal, `RoutingPopulationRefusal::StoreUnavailable`, for a source
   that cannot answer at all, and its own counter. The counter lives on the
   participation owner (`crates/i2pr-daemon/src/lib.rs:1873`), beside the
   counters that owner already keeps, and the production source increments
   **that** counter when it refuses. The counter is reported in the owner's
   **stop message**, beside the projection counters, so a refused population
   is visible without a new configuration key and without a new listener.

   **Why the source counts and the owner holds.** The owner has **no public
   surface** — `NetDbParticipation` is private and exposes only
   `pub(crate) fn discovery` — so a counter the lane cannot reach would be a
   number nobody could observe. The source holds the owner's counter by
   reference, so the number still lives with the owner that owns the routing
   table, and the **lane** can construct a source over a counter of its own
   and observe the refusal being counted.

### 6.2 `crates/i2pr-daemon/src/lib.rs`

1. `populate_routing_table` takes the **source** in place of the bootstrap, and
   it **indexes with no NetDB lock held**: the read returns first, and the
   indexing loop runs afterwards.

2. The two call sites — the startup population
   (`crates/i2pr-daemon/src/lib.rs:1981`) and the tick's population
   (`crates/i2pr-daemon/src/lib.rs:2032`) — build the **production source** over
   the router's bootstrap and pass it in.

3. The population's **absence** is counted under its own reason on the
   participation owner.

## 7. Design decisions

### 7.1 The index happens outside the lock, and the lock is short

The population's work — the bucket insert, the day's routing position, the
receipt — is the expensive part, and it is the part that takes the routing
table's mutex. The read is the cheap part. The plan keeps the cheap part under
the lock and moves the expensive part out of it, exactly as Plan 537 keeps the
reads under the lock and moves the signature out.

### 7.2 The source yields the records, not the keys

The table's `insert_signed` refuses a peer whose own record does not advertise
`f` (`crates/i2pr-netdb/src/bucket_set.rs:420`), and that check is the
**signed-evidence rule**: the table never accepts one peer's advertisement of
another as evidence for it. Handing the table a bare hash would move the check
out of the table and into the reader, and the rule would no longer be the
table's own. The source therefore yields the **records**, and the table's
check is unchanged.

### 7.3 The copy is bounded and it is a copy

The source hands over a **copy** of the advertisers, bounded by the NetDB's
configured record ceiling. The store's own records are not moved out of it, and
the population does not become a second, unbounded collection.

### 7.4 The population's absence is never skipped silently

A router that cannot read its population is a **fact about this router**. The
plan makes that fact **typed and counted** rather than an absent pass, exactly
as Plan 537 made the requester's refusal to ask typed and counted.

### 7.5 The trait keeps the predecessors' evidence exactly as it was

The trait is implemented **for the store itself**, so a lane that passes
`&store` still compiles and still passes, and the predecessors' guards — which
read the relay roles' modules — still read all of their strings. This is the
reason the plan does not introduce a second, differently named entry point.

### 7.6 No second store, no second lock, no new owner

The plan adds no store, no lock and no owner. It reuses the bootstrap's mutex
for the one read it already needed, and it stops holding it for the work it
did not need it for.

## 8. Work packages

| Work package | What it carries |
| --- | --- |
| WP1 | the population-source trait, the store's implementation, the production source, the new refusal and its counter |
| WP2 | the composition root's population: the source in place of the bootstrap, and the indexing outside the lock |
| WP3 | the lane and the guard |
| WP4 | the documentation, the inventory and the plan's amendment log |
| WP5 | the closure record and the registry's closed rows |

## 9. Failure, cancellation, restart and contention semantics

**Failure.** A source that cannot answer returns
`RoutingPopulationRefusal::StoreUnavailable`, the population counts it under
its own reason, and the pass ends. A poisoned bootstrap lock is the same typed
refusal. Nothing is skipped without a trace.

**Cancellation.** The population is a synchronous step inside the product's
existing tick. It awaits nothing and holds no lock across an await, so a
cancellation cannot strand it.

**Restart.** Nothing is persisted. The routing table is live state and is
rebuilt empty; a restart repopulates it on the next tick.

**Contention.** The bootstrap's mutex is now taken for one self-contained read
at a time and released before the indexing. The NetDB discovery tick, the
publication tick, the peer reconcile, the peer pool and the relay roles can no
longer be held behind an O(population) walk and the routing table's mutex. And
because the indexing is the only thing that takes the routing table's mutex,
the router's NetDB lock no longer wraps it.

## 10. Compatibility and migration

**No wire change.** The table's entries, its buckets, its receipts and its
lookups are exactly what the tree already produced.

**No configuration migration.** **No key is added.** The daemon's
configuration surface is unchanged.

**No persisted state.** The population is live state and is not persisted.

**No change to the routing table's existing surface.** `index`,
`index_suggested`, the existing counters and the receipts are unchanged.

**No change to the store's surface.** `floodfill_advertisers()` is unchanged.

## 11. Required tests

### 11.1 The lane

`crates/i2pr-daemon/tests/routing_table_population_lock.rs` — the population
over the **production** source, and the behaviour with and without a store.
The lane must prove:

1. the production source's read **takes** the router's NetDB lock and **releases**
   it before it returns — the lane holds the lock from another thread and the
   read completes the moment that thread lets go, and the returned material
   then indexes into the table **while the lock is held again**, which it can
   only do if the indexing needs no lock;
2. the population **runs** when the router holds no store, and the refusal is
   **typed**, counted under its own reason, and not skipped — the lane reads
   the counter it handed the source and observes it increment;
3. a **poisoned** NetDB lock is the same typed refusal;
4. the population is still the **same set** — every advertiser the store holds
   is indexed, and the table refuses a peer that does not advertise `f`;
5. the indexing happens with **no** NetDB lock held — the lane measures that
   with the same lock the first item uses;
6. the population **mutates nothing** in the store and writes no peer profile;
7. every refusal the population can reach is typed and counted.

The lane proves the **source's** behaviour. It cannot see the composition
root's private population, so the **shape** — that the composition root holds
no NetDB lock across its indexing — is the guard's claim, and the guard says so
in its own words.

### 11.2 The guard

`crates/i2pr-daemon/tests/routing_table_population_lock_boundary.rs` — a
static guard that is the authority for the **shape**. It must have an inverted
probe for every rule and a fail-closed probe for every item a rule names. The
rules:

1. the population is read through the **source**, and the composition root's
   population holds **no** NetDB lock across the indexing;
2. the production source's read **releases** the lock before it returns;
3. the population's **absence** is a **typed** refusal, and it is **counted**
   under its own reason;
4. the population yields the **records**, and the table's signed-evidence rule
   is **untouched**;
5. the routing table's existing surface and the store's selection surface are
   **untouched**;
6. **no** configuration key is added, **no** task, timer, listener or socket is
   added, and the population rides the **existing** tick;
7. Plan 536's, Plan 537's and Plan 538's lanes and guards stay
   **byte-identical**.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-daemon --test routing_table_population_lock -- --test-threads=1
cargo test --locked -p i2pr-daemon --test routing_table_population_lock_boundary -- --test-threads=1
cargo test --locked --workspace --all-targets --no-fail-fast -- --test-threads=1
python3 scripts/check-tooling-inventory.py
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

plus the routine floor in `AGENTS.md`, with this plan's two steps added.

## 13. Documentation updates

- `specs/support.toml` — a new experimental, non-advertised surface
  `daemon.routing-table-population-lock`.
- `specs/CONFORMANCE.md` — the count paragraph and a section for this plan.
- `specs/QUALIFICATION-GATES.md` — the gates this plan's lane and guard
  discharge.
- `docs/architecture/i2pr-daemon.md` — the composition root's population
  shape.
- `docs/architecture/i2pr-netdb.md` — the routing table's signed-evidence rule,
  unchanged.
- The plan's own amendment log.

## 14. Acceptance criteria and stop conditions

The plan is **accepted** when all of the following hold:

1. the population is indexed with **no** NetDB lock held, and the lane
   measures it;
2. the population runs without a store and **counts** a typed refusal;
3. the population is the **same set**, and the table's signed-evidence rule is
   unchanged;
4. the routing table's and the store's surfaces are unchanged, and no
   configuration key, task, timer, listener or socket is added;
5. `router.version` stays `0.9.62`, and the inventory's advertised count stays
   **zero**;
6. the full floor is green, with the closure record's table complete;
7. the guard's inverted probes each fail as intended and the tree is restored
   byte-identically after each.

**Stop conditions.** The plan stops and reports if:

- the population cannot be read outside the lock without changing the table's
  signed-evidence rule — the rule is not this plan's to move;
- a predecessor's lane or guard would have to change — the plan's design
  exists precisely so that it does not;
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
- Plan 536's, Plan 537's and Plan 538's closed closure records are **not**
  rewritten. The discharge lives in this plan's closure record and in the
  registry's row.
- The plan **extends** no other plan's guard: Plan 536's, Plan 537's and
  Plan 538's lanes and guards stay byte-identical.

## 17. Amendment log

### 17.1 — 2026-11-11, registration

Plan 539 is registered as the successor to Plan 538 in the router's NetDB-lock
line: the composition root still populates the router's routing table by
walking **every** floodfill advertiser the NetDB holds, and it does that walk
**under the router's NetDB lock** — while the walk's body takes the routing
table's own mutex, so the router's NetDB lock **wraps** it.

The scope is the composition root's **population**, not the routing table's
internals and not the store's selection path. The plan makes the population a
**short, bounded read** whose guard is released before the read returns, so
the indexing happens with **no** NetDB lock held and the router's NetDB lock
never nests inside the routing table's. And it makes the absence of a
population a **typed, counted refusal** rather than a silent skip.

The population is still **the same set**, and the table still indexes each
peer with **its own signed evidence**: the source yields the records, never a
peer's word about another peer, so the table's own rule is untouched.

The plan adds no store, no lock, no owner, no configuration key and no timer,
it changes no transport file and no runtime file, and it leaves `Cargo.toml`
and `Cargo.lock` byte-identical.

No closed closure record is rewritten by this plan. The corrections this plan
finds, if any, live in this plan's closure record and in the registry's row.

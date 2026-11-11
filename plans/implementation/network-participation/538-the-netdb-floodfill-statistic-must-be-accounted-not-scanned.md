# Plan 538 — the NetDB's floodfill statistic must be accounted, not scanned

## 1. Objective

The router's validated `RouterInfo` store must report its **floodfill advertiser
count** from an **accounted** number, not from a **scan** of every record it
holds, and the daemon's bootstrap must never hold the router's NetDB lock across
a population scan to obtain that number.

Two facts about the tree force this. First, the store already accounts its
**encoded byte total** incrementally, on exactly the mutation paths that insert
and remove a record, and it accounts nothing else: the floodfill advertiser
count — the second number the same struct reports about the same set — is
derived by iterating **every** record on **every** call to `stats()`. Second, the
daemon's bootstrap reads that statistic while holding the router's NetDB lock,
and it does so **twice** per state computation, because `compute_state` refreshes
the snapshot (which reads the statistics) and then reads them again.

The plan makes the store account the floodfill count the way it already accounts
its byte total, and makes the bootstrap compute the statistics **once** per
operation. The count stays **exact** — not an estimate, not a sample, not a
cap.

## 2. Why ready

- The store's **accounting pattern already exists**. `RouterInfoStore` maintains
  `total_encoded_bytes` incrementally on insert and on remove
  (`crates/i2pr-netdb/src/store.rs:118` and
  `crates/i2pr-netdb/src/store.rs:191`), with checked arithmetic and an
  `expect` on underflow. The floodfill count is the same kind of number about
  the same set, on the same paths.
- The **precedent** for the daemon-side fix is already in the tree: the
  bootstrap already refreshes its snapshot at the points where the store
  changed (`crates/i2pr-daemon/src/bootstrap.rs:293`,
  `crates/i2pr-daemon/src/bootstrap.rs:331`,
  `crates/i2pr-daemon/src/bootstrap.rs:419`,
  `crates/i2pr-daemon/src/bootstrap.rs:443` and
  `crates/i2pr-daemon/src/bootstrap.rs:523`), so the statistics are already
  computed **at** the mutation and not on a poll.
- The defect is **internal**. No stock peer, no second host and no external
  topology is needed to find it, to fix it or to prove it.
- The **line** is open and named. Plans 536 and 537 both removed O(population)
  work from **under** the router's NetDB lock in the relay roles; this is the
  same lock's remaining population scan, in the daemon's maintenance path.

## 3. Current implementation evidence

### 3.1 The store accounts its bytes and scans for its floodfill count

`RouterInfoStore` (`crates/i2pr-netdb/src/store.rs:83`) holds three fields:

```rust
pub struct RouterInfoStore {
    records: BTreeMap<RouterHash, ValidatedRouterInfo>,
    total_encoded_bytes: usize,
    config: RouterInfoStoreConfig,
}
```

`total_encoded_bytes` is **maintained**: `insert` adds and subtracts it on the
new-key and the replacement paths (`crates/i2pr-netdb/src/store.rs:153` and
`crates/i2pr-netdb/src/store.rs:172`) and `remove` subtracts it
(`crates/i2pr-netdb/src/store.rs:192`).

The floodfill count is **derived**. `stats()`
(`crates/i2pr-netdb/src/store.rs:225`) computes it by walking the map:

```rust
pub fn stats(&self) -> RouterInfoStoreStats {
    let floodfill_advertiser_count = self
        .records
        .values()
        .filter(|record| record.advertises_floodfill())
        .count();
    // ...
}
```

`records` is a `BTreeMap` (`crates/i2pr-netdb/src/store.rs:84`), so the walk is
O(the store's population). `advertises_floodfill`
(`crates/i2pr-netdb/src/router_info.rs:334`) is a capability-string check, so the
walk is cheap per record and still linear overall.

### 3.2 The bootstrap reads the statistic while holding the router's NetDB lock

The bootstrap's mutex **is** the router's NetDB lock: it guards the only
`RouterInfoStore` in the daemon (`crates/i2pr-daemon/src/bootstrap.rs:215`).
`Bootstrap::refresh_snapshot` (`crates/i2pr-daemon/src/bootstrap.rs:553`) reads
the statistics:

```rust
fn refresh_snapshot(&mut self) {
    let stats = self.store.stats();
    self.snapshot.record_count = stats.record_count;
    self.snapshot.encoded_bytes = stats.total_encoded_bytes;
    self.snapshot.floodfill_advertisers = stats.floodfill_advertiser_count;
    self.snapshot.last_reseed_summary = self.last_reseed_summary.clone();
}
```

and it is called at **five** sites — the prune
(`crates/i2pr-daemon/src/bootstrap.rs:293`), the authenticated ingest
(`crates/i2pr-daemon/src/bootstrap.rs:331`), the cache loader
(`crates/i2pr-daemon/src/bootstrap.rs:419`), the state computation
(`crates/i2pr-daemon/src/bootstrap.rs:443`) and the reseed path
(`crates/i2pr-daemon/src/bootstrap.rs:523`). Every one of those calls runs
**under the router's NetDB lock**, because every one of them takes `&mut self`
on the bootstrap.

### 3.3 The state computation performs the scan twice

`Bootstrap::compute_state` (`crates/i2pr-daemon/src/bootstrap.rs:442`) reads:

```rust
fn compute_state(&mut self, policy: BootstrapPolicy) -> BootstrapState {
    self.refresh_snapshot();
    let stats = self.store.stats();
    // ...
}
```

`refresh_snapshot` has already called `self.store.stats()` — the second call is
the same scan again, under the same lock, on the same unchanged store. The
values the function then reads (`stats.record_count` and
`stats.floodfill_advertiser_count`) are the values the snapshot already holds,
because the store cannot change between the two calls: `compute_state` holds
`&mut self`.

### 3.4 The population scan is on the router's live maintenance path

The prune is a **cadenced, live** operation. The peer reconcile takes the
router's NetDB lock (`crates/i2pr-daemon/src/peer_reconcile.rs:421`) and calls
`guard.prune(now_ms, RETENTION_WINDOW_MS)`
(`crates/i2pr-daemon/src/peer_reconcile.rs:424`), and `Bootstrap::prune`
(`crates/i2pr-daemon/src/bootstrap.rs:291`) refreshes the snapshot immediately
after the store's own prune — so the router's NetDB lock is held across the
store's population walk on every reconcile pass that prunes. A second prune
path reaches the same call (`crates/i2pr-daemon/src/peer_reconcile.rs:442`).

The authenticated ingest is on the same lock: the reconcile ingests under
`bootstrap.lock()` (`crates/i2pr-daemon/src/peer_reconcile.rs:663`) and the
ingest refreshes the snapshot at
`crates/i2pr-daemon/src/bootstrap.rs:331`.

### 3.5 The consequence

Every NetDB user — the discovery tick, the publication tick, the peer
reconcile, the peer pool, the relay target's read and the relay requester's
read — waits behind a population walk whose only purpose is to count a number
the store could have counted as it went. The walk is repeated twice inside
`compute_state`, so the startup path and each reseed path pay for it twice.

## 4. Invariants

1. **The count is accounted, not derived.** The store maintains the floodfill
   advertiser count on the same paths that maintain the byte total, and the
   reported count equals the number of retained records that advertise the `f`
   capability at **every** point in time.
2. **The count is exact.** No sampling, no cap, no "at least", no estimate. The
   reported number is the true number.
3. **`stats()` is O(1).** It reads maintained counters and never walks the
   record map.
4. **The statistics are computed once per operation.** No daemon path computes
   the store's statistics twice for one result.
5. **The router's NetDB lock is never held across a population scan to obtain a
   statistic.**
6. **The store's public surface is unchanged.** `stats()` keeps its name, its
   return type and its fields; `RouterInfoStoreStats` keeps its fields.
7. **The bootstrap's snapshot semantics are unchanged.** The same fields, the
   same values, the same refresh points and the same `BootstrapState` decisions.
8. **No public API is removed or renamed.** No `i2pr-netdb` item disappears.
9. **No configuration key is added.** The daemon's configuration surface is
   unchanged, and `Cargo.toml` and `Cargo.lock` stay byte-identical.
10. **No new task, timer, listener or socket.** The statistic is maintained by
    the store's existing mutation paths.
11. **No advertisement.** `router.version` stays `0.9.62` and the inventory's
    advertised count stays **zero**.
12. **The selection paths are untouched.** `floodfill_advertisers()`
    (`crates/i2pr-netdb/src/store.rs:273`) still yields the advertisers, and the
    lookup policy's ordering core
    (`crates/i2pr-netdb/src/lookup_policy.rs:290`) still walks them — those are
    **selection** operations, which are scans by their nature, and this plan
    does not move them.

## 5. Scope

### 5.1 In scope

- The store's **floodfill advertiser accounting**: the maintained counter, the
  insert and remove paths that maintain it, and `stats()`'s O(1) read.
- The bootstrap's **duplicate statistics read**: one computation per operation.

### 5.2 Out of scope

- The store's **selection** paths: `floodfill_advertisers()`
  (`crates/i2pr-netdb/src/store.rs:273`), `iter()`
  (`crates/i2pr-netdb/src/store.rs:241`) and the lookup policy's ordering core
  (`crates/i2pr-netdb/src/lookup_policy.rs:290`). A selection over the floodfill
  advertisers is a scan by its nature, and it is bounded by the policy's
  candidate ceiling.
- The store's **prune** (`crates/i2pr-netdb/src/store.rs:251`). A prune must
  examine every record to know which are stale; that is the prune's own work,
  not a statistic.
- The **routing table's** nearest-floodfill helper
  (`crates/i2pr-netdb/src/routing.rs:164`), which is `#[allow(dead_code)]` and
  has no production caller.
- The **relay roles**, the **transport** and the **runtime**. No file under
  `crates/i2pr-transport/` and no file under `crates/i2pr-runtime/` is touched.
- The stock-peer half, which stays blocked on Plan 493.

### 5.3 Out of scope, with the owner named

- **The store's prune is still O(population) under the router's NetDB lock.**
  The plan removes the *statistic* scan; the prune's own examination of every
  record remains, because a prune must examine every record. Moving the prune
  out of the lock is a consequence of the router having a **NetDB owner of its
  own**, which is the larger successor Plans 536 and 537 both named.
  **Owner:** the successor that gives the router a NetDB owner of its own — the
  same successor Plan 536 named.
- **The router's validated store is still the bootstrap's store.** The bootstrap
  owns the only `RouterInfoStore` in the daemon
  (`crates/i2pr-daemon/src/bootstrap.rs:215`). Separating the router's NetDB
  from the bootstrap's lifetime is a larger change, and this plan does not
  attempt it. **Owner:** the same successor.
- **The lock scope is proven statically, not behaviourally.** Plan 536's closure
  record recorded the same gap for the target role: the composition root's
  drain is not reachable from a test without a second production composition.
  This plan does not build that seam either. **Owner:** the same successor that
  makes the composition root's relay arm reachable.
- **The production SSU2 peer-test driver** — Plan 530's Low finding 2.
- **The `B` advertisement and `caps=f`** — their own owners.
- **The stock-peer half** — Plan 493's authorized two-host topology.

## 6. Required production changes

### 6.1 `crates/i2pr-netdb/src/store.rs`

1. `RouterInfoStore` gains a **maintained** field,
   `floodfill_advertiser_count: usize`, alongside `total_encoded_bytes`
   (`crates/i2pr-netdb/src/store.rs:85`).

2. `insert` maintains it:
   - the **new-key** path adds one when the incoming record advertises the `f`
     capability;
   - the **replacement** path adjusts by the delta between the existing
     record's capability and the incoming record's capability, because the
     replacement may change it;
   - the **stale**, **idempotent**, **conflict** and **capacity-exceeded**
     paths change nothing, because they do not change the set.

3. `remove` subtracts one when the removed record advertised the `f`
   capability, and the store's own `prune` goes through `remove`, so the prune
   maintains the count as it removes.

4. `stats()` reads the maintained counter and **no longer walks the map**. The
   method keeps its name, its return type and its fields.

5. The accounting uses the same **checked arithmetic** discipline the byte
   total already uses, and it never goes below zero.

### 6.2 `crates/i2pr-daemon/src/bootstrap.rs`

1. `refresh_snapshot` takes the statistics it refreshes **as an argument**, so
   the caller computes them **once** and the snapshot's counts and the
   computation's counts are the same numbers by construction.

2. Every caller of `refresh_snapshot` — the prune
   (`crates/i2pr-daemon/src/bootstrap.rs:293`), the authenticated ingest
   (`crates/i2pr-daemon/src/bootstrap.rs:331`), the cache loader
   (`crates/i2pr-daemon/src/bootstrap.rs:419`), the state computation
   (`crates/i2pr-daemon/src/bootstrap.rs:443`) and the reseed path
   (`crates/i2pr-daemon/src/bootstrap.rs:523`) — reads the statistics once and
   passes them in.

3. `compute_state` (`crates/i2pr-daemon/src/bootstrap.rs:442`) **no longer**
   calls `self.store.stats()` a second time. It uses the one computation it
   made, and its `BootstrapState` decisions are unchanged.

The bootstrap's public surface — `snapshot()`, `diagnostics()`, `store_summary()`
and the `BootstrapSnapshot` fields — is unchanged.

## 7. Design decisions

### 7.1 The count is maintained where the set is maintained

The store already knows the byte total at every point in time because it
updates it when the set changes. The floodfill count is a second fact about the
same set, and the plan puts it on the same footing: the store updates it when
the set changes. The alternative — computing it on demand — is what the tree
does today, and it is the defect.

### 7.2 The statistic is not a sample and never a cap

The count is **exact**. A sampled or capped count would change the bootstrap's
`BootstrapState` decision, which compares the floodfill advertiser count against
the configured floor (`crates/i2pr-daemon/src/bootstrap.rs:451`), and would
therefore change when the router believes it is ready. An exact count keeps the
decision identical.

### 7.3 The daemon computes the statistics once

`compute_state` read the statistics twice, once through the refresh and once
directly. Both reads saw the same store, because the function holds `&mut self`.
Passing the one computation into the refresh makes that structural: the
snapshot's counts and the decision's counts are the same numbers, and there is
no second read to drift.

### 7.4 The refresh points do not change

The plan does **not** make the snapshot lazy. The bootstrap already refreshes
the snapshot at the points where the store changed, and the startup report is
read by the daemon for the reseed's minimum-new-records
(`crates/i2pr-daemon/src/lib.rs:5949`) and for the pipeline's log line
(`crates/i2pr-daemon/src/lib.rs:5957`). Moving the refresh would change when the
report is true. With the statistic accounted, the refresh
is O(1) and costs nothing to keep.

### 7.5 The selection paths stay scans

`floodfill_advertisers()` and the lookup policy's ordering core exist to
**select** from the floodfill advertisers, and a selection is a walk by its
nature. The plan changes what the store **reports**, not how it **selects**. The
lookup policy's candidate ceiling
(`crates/i2pr-netdb/src/lookup_policy.rs:290`) is unchanged.

### 7.6 The count's own failures are the store's failures

A count that cannot be maintained would be a defect in the store's accounting,
and the plan makes that impossible to hide: the lane computes the true count by
an **independent** walk and compares it with the reported count after every
mutation path, so a divergence fails a row rather than quietly under-reporting
the router's floodfill population.

### 7.7 No second store, no second lock, no new owner

The plan adds no store, no lock and no owner. It changes how one number is
maintained inside the store the daemon already has, and it removes one
redundant read in the bootstrap that already owns it.

## 8. Work packages

| Work package | What it carries |
| --- | --- |
| WP1 | the store's maintained floodfill count, its insert and remove maintenance, and `stats()`'s O(1) read |
| WP2 | the bootstrap's single computation per operation |
| WP3 | the lane and the guard |
| WP4 | the documentation, the inventory and the plan's amendment log |
| WP5 | the closure record and the registry's closed rows |

## 9. Failure, cancellation, restart and contention semantics

**Failure.** The store's accounting uses checked arithmetic and cannot go
negative. A count that diverged from the set would be a defect in the store,
and the lane proves it cannot: the reported count is compared with an
independent walk after every mutation path. The daemon has no new failure mode,
because the daemon's decisions are made on the same numbers as before.

**Cancellation.** The change touches no async path. The store's mutation
methods are synchronous and the bootstrap's refresh is synchronous, so there is
nothing to cancel and no lock held across an await.

**Restart.** Nothing is persisted. The count is rebuilt from the store's own
contents as the store is rebuilt, so a restart cannot inherit a stale count.

**Contention.** The router's NetDB lock is no longer held across a population
walk to obtain a statistic. The prune's own examination of every record remains,
because a prune must examine every record; that work is named in §5.3 with its
owner. The NetDB discovery tick, the publication tick, the peer reconcile, the
peer pool and the relay roles are no longer queued behind a count.

## 10. Compatibility and migration

**No wire change.** The RouterInfo's bytes, the store's contents and the
bootstrap's decisions are exactly what the tree already produced.

**No configuration migration.** **No key is added.** The daemon's configuration
surface is unchanged.

**No persisted state.** The count is derived from the store's contents and is
never persisted, so there is nothing to migrate.

**No change to the netdb crate's public surface.** `stats()` keeps its name,
its return type and its fields, and no `i2pr-netdb` item is removed or
renamed.

**No change to the bootstrap's public surface.** `snapshot()`, `diagnostics()`,
`store_summary()` and the `BootstrapSnapshot` fields are unchanged, and the
`BootstrapState` decisions are unchanged.

## 11. Required tests

### 11.1 The lane

`crates/i2pr-netdb/tests/floodfill_statistic_accounting.rs` — the store's
accounting, against an **independent** count. The lane must prove:

1. the reported count equals an independently computed count after a **new-key**
   insert, and after an **idempotent** insert, and after a **conflict**, and
   after a **stale** insert, and after a **capacity-exceeded** insert;
2. the reported count follows a **replacement** that changes the `f`
   capability, in both directions;
3. the reported count follows a **remove**, and a **prune** that removes several
   records at once;
4. the reported count is **exact** for a store with a large population, and the
   report equals the independent count, not a bound;
5. the store's **selection** surface is unchanged — `floodfill_advertisers()`
   still yields exactly the advertisers, and `iter()` still yields every record;
6. the daemon's bootstrap reports the same count and reaches the same
   `BootstrapState` as before, and its statistics are computed **once** per
   operation.

### 11.2 The guard

`crates/i2pr-netdb/tests/floodfill_statistic_accounting_boundary.rs` — a static
guard that is the authority for the **shape**. It must have an inverted probe
for every rule and a fail-closed probe for every item a rule names. The rules:

1. `RouterInfoStore` **maintains** a floodfill advertiser count, and `stats()`
   reads it — `stats()`'s body walks **no** record map;
2. `insert` maintains the count on the **new-key** and the **replacement**
   paths, and `remove` maintains it on the removal path;
3. the count never goes below zero, and the arithmetic is checked;
4. the daemon's bootstrap computes the store's statistics **once** per
   operation, and `compute_state` does not read them a second time;
5. the bootstrap's **public surface** is unchanged — `snapshot()`,
   `diagnostics()`, `store_summary()` and the `BootstrapSnapshot` fields;
6. the store's **selection** surface is unchanged —
   `floodfill_advertisers()` and `iter()`;
7. **no** configuration key is added, **no** task, timer, listener or socket is
   added, and `Cargo.toml` and `Cargo.lock` stay **byte-identical**;
8. `router.version` stays `0.9.62`, and the inventory's advertised count stays
   **zero**.

Plan 536's lane and guard, and Plan 537's lane and guard, must stay
**byte-identical**: this plan changes the store's accounting and the bootstrap's
read, not the relay roles' semantics, and the predecessors' evidence must remain
exactly as it was.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-netdb --test floodfill_statistic_accounting -- --test-threads=1
cargo test --locked -p i2pr-netdb --test floodfill_statistic_accounting_boundary -- --test-threads=1
cargo test --locked -p i2pr-netdb --all-targets
cargo test --locked --workspace --all-targets --no-fail-fast -- --test-threads=1
python3 scripts/check-tooling-inventory.py
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

plus the routine floor in `AGENTS.md`, with this plan's two steps added.

## 13. Documentation updates

- `specs/support.toml` — a new experimental, non-advertised surface
  `netdb.floodfill-statistic-accounting`.
- `specs/CONFORMANCE.md` — the count paragraph and a section for this plan.
- `specs/QUALIFICATION-GATES.md` — the gates this plan's lane and guard
  discharge.
- `docs/architecture/i2pr-netdb.md` — the store's accounting shape.
- `docs/architecture/i2pr-daemon.md` — the bootstrap's snapshot shape.
- The plan's own amendment log.

## 14. Acceptance criteria and stop conditions

The plan is **accepted** when all of the following hold:

1. the reported count is **exact** after every mutation path, and the lane
   proves it against an independent count;
2. `stats()` is **O(1)** and the guard proves it walks no record map;
3. the bootstrap computes the statistics **once** per operation, and the
   `BootstrapState` decisions are unchanged;
4. the store's and the bootstrap's **public surfaces** are unchanged, and no
   configuration key, task, timer, listener or socket is added;
5. `router.version` stays `0.9.62`, and the inventory's advertised count stays
   **zero**;
6. the full floor is green, with the closure record's table complete;
7. the guard's inverted probes each fail as intended and the tree is restored
   byte-identically after each.

**Stop conditions.** The plan stops and reports if:

- the count cannot be maintained exactly — an inexact count would change when
  the router believes it is ready, and that decision is not this plan's to
  move;
- the store's or the bootstrap's public surface would have to change — the plan
  exists precisely so that it does not;
- a predecessor's lane or guard would have to change — the plan's design exists
  precisely so that it does not;
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
- Plan 536's closed closure record is **not** rewritten. The discharge of its
  Low finding 1 lives in this plan's closure record and in the registry's row.
- Plan 537's closed closure record is **not** rewritten. The discharge of its
  named out-of-scope items lives in this plan's closure record and in the
  registry's row.
- The plan **extends** no other plan's guard: Plan 536's lane and guard, and
  Plan 537's lane and guard, stay byte-identical.

## 17. Amendment log

### 17.1 — 2026-11-11, registration

Plan 538 is registered as the successor to Plan 537 in the router's NetDB-lock
line: the daemon's bootstrap still holds the router's NetDB lock across a
population walk to obtain a statistic, and still computes that statistic twice
per state computation.

The scope is the store's **accounting** and the bootstrap's **redundant read**,
not the store's selection paths and not the prune's own examination of every
record. The store already maintains its encoded byte total incrementally on
exactly the paths that insert and remove a record; the floodfill advertiser
count — the second number the same struct reports about the same set — is
derived by walking every record on every call to `stats()`.

The plan makes the count maintained and exact, makes `stats()` O(1), and makes
the bootstrap compute the statistics once per operation. It adds no store, no
lock, no owner, no configuration key and no timer, it changes no transport file
and no runtime file, and it leaves `Cargo.toml` and `Cargo.lock` byte-identical.

No closed closure record is rewritten by this plan. The corrections this plan
finds, if any, live in this plan's closure record and in the registry's row.

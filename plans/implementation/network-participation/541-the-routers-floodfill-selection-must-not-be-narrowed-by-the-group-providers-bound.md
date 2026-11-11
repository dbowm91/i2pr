# Plan 541 — the router's floodfill selection must not be narrowed by the group provider's bound

**Status:** registered. **Subsystem:** network-participation.
**Registered:** 2026-11-11. **Successor to:** Plan 540.

## 1. Objective

The router's **floodfill selection** must see the router's **floodfill
advertisers**.

The router's *authoritative* floodfill selection store — the
`DestinationTunnelCoordinator`'s store, which `candidate_hashes` and
`version_gated_candidate_hashes` read — is fed
`Bootstrap::validated_router_info_snapshot()`, and that snapshot is the
**first 256 records in ascending `RouterHash` order**. The bound exists for
the **group provider**'s RouterInfo supply. The floodfill advertisers are
distributed across the hash space, so the selection store holds an arbitrary
subset of the NetDB — typically **no** advertiser at all — and the router then
refuses its own LeaseSet2 publication and its destination build hops, even
though its NetDB holds thousands of floodfill advertisers.

The plan makes the store maintain an **advertiser index** and makes the
selection store's feed carry the **advertisers**. The store's own public
surfaces, the group provider's bound, the peer-profile store's feed and the
three other consumers of the bounded snapshot are unchanged.

## 2. Why ready

The tree already has every seam this plan needs, and the discipline this plan
applies is one the store already follows.

**The store already maintains one advertiser fact the same way.** Plan 538 made
the floodfill advertiser **count** accounted rather than derived: the store
maintains it on exactly the paths that insert and remove a record, and the
adjustment is made at the same three decision points where this plan maintains
the advertiser **set** (`crates/i2pr-netdb/src/store.rs:186`,
`crates/i2pr-netdb/src/store.rs:209`, `crates/i2pr-netdb/src/store.rs:238`).
This plan applies the same discipline to the set the count counts.

**The store's records are already ordered.** The store's records live in a
`BTreeMap` keyed by the router's hash
(`crates/i2pr-netdb/src/store.rs:84`), and the router's hash is `Ord`
(`crates/i2pr-netdb/src/router_info.rs:38`), so an ordered set of the
advertisers' hashes is a natural third collection beside the record map and
Plan 540's age index.

**No predecessor's guard needs an extension.** Plan 538's guard requires the
store's selection to keep filtering on the capability
(`crates/i2pr-daemon/tests/floodfill_statistic_accounting_boundary.rs:668`),
and this plan keeps that filter, so the selection's observable behaviour is
exactly what a scan of the record map would yield. Plan 506's guard pins the
reconcile's `fn snapshot` body and its `self.insert_all(&records).await;` and
`self.learn_all(&records);` call sites
(`crates/i2pr-daemon/tests/peer_reconcile_boundary.rs:1072`), and this plan
leaves all of them exactly as they are: the advertiser feed is a **new** call
beside them, not a change to them. Plan 507's, Plan 538's, Plan 539's and
Plan 540's lanes and guards stay byte-identical.

**The copy is already bounded.** The advertiser snapshot is bounded by the
NetDB's own record ceiling — 4 096 records and 4 MiB by default
(`crates/i2pr-daemon/src/config.rs:1141`) — so the feed cannot become an
unbounded copy.

## 3. Current implementation evidence

### 3.1 The group provider's bound narrows the selection store

`Bootstrap::validated_router_info_snapshot`
(`crates/i2pr-daemon/src/bootstrap.rs:341`) returns

```rust
self.store
    .iter()
    .take(MAX_GROUP_PROVIDER_ROUTER_INFOS)
    .map(|(_, record)| record.clone())
    .collect()
```

with `MAX_GROUP_PROVIDER_ROUTER_INFOS` at
`crates/i2pr-daemon/src/bootstrap.rs:53` and the bound applied at
`crates/i2pr-daemon/src/bootstrap.rs:344`. The store's `iter()` is the record
map's own iteration, and the record map is keyed by the router's hash, so the
snapshot is the **256 lowest router hashes** the NetDB holds. The bound is the
group provider's; the snapshot is not only the group provider's.

The snapshot has **five** consumers. Three of them want *any* peers and keep
the bounded prefix:

- the controlled request-driven NTCP2 lookup's candidate pool
  (`crates/i2pr-daemon/src/lib.rs:211`);
- the peer-profile learn cadence (`crates/i2pr-daemon/src/lib.rs:2214`);
- the controlled NTCP2 dial's peer snapshot
  (`crates/i2pr-daemon/src/lib.rs:3320`).

Two of them want the **floodfill advertisers**, and both get the prefix
instead:

- the Destination-groups seed, which becomes the coordinator's authoritative
  store (`crates/i2pr-daemon/src/lib.rs:4427`,
  `crates/i2pr-daemon/src/lib.rs:4542`,
  `crates/i2pr-daemon/src/service_product.rs:2910`);
- the reconcile's feed into that same store
  (`crates/i2pr-daemon/src/peer_reconcile.rs:459`,
  `crates/i2pr-daemon/src/peer_reconcile.rs:474`).

### 3.2 The selection reads the coordinator's store

`DestinationTunnelCoordinator::candidate_hashes`
(`crates/i2pr-daemon/src/destination_tunnels.rs:978`) and
`version_gated_candidate_hashes`
(`crates/i2pr-daemon/src/destination_tunnels.rs:997`) both call
`select_floodfill_candidates(&self.store, ...)`
(`crates/i2pr-daemon/src/destination_tunnels.rs:984`), and the selection's
core walks the store's floodfill advertisers
(`crates/i2pr-netdb/src/lookup_policy.rs:290`). The coordinator's store is
therefore the **authority** the router's build-hop selection reads.

### 3.3 The live path selects, then fetches the hop's record

The live LeaseSet2 publication selects its floodfill targets from that store
(`crates/i2pr-daemon/src/service_product.rs:10331`) and then fetches the
selected hop's record from the same store
(`crates/i2pr-daemon/src/service_product.rs:7905`). When the store does not
carry the hop, the path **fails closed** with *"selected first hop is absent
from the validated RouterInfo store"*
(`crates/i2pr-daemon/src/service_product.rs:7910`). A router whose selection
store holds no advertiser therefore refuses its own publication, and it does so
**honestly**: the refusal is a fact about the router's own store.

### 3.4 The learned records never reach the selection store

The live NetDB discovery spine ingests into the **bootstrap's** store
(`crates/i2pr-daemon/src/service_product.rs:4334`). Nothing carries a learned
record into the coordinator's store: the reconcile re-inserts the **prefix**,
and the prefix does not change when the NetDB grows. A floodfill the router
learns *after* startup is therefore invisible to the selection, unless its
hash happens to be among the 256 lowest.

### 3.5 What the plan does not claim

The plan does not claim that the router's selection becomes **complete**: the
advertisers the router holds are the advertisers it knows, and a router that
has learned one floodfill has one candidate. The plan does not claim that the
**ownership** of the NetDB changes: the router's validated store is still the
bootstrap's store, and that stays the named owner of the successor. The plan
does not claim anything about the peer-profile store's feed, which keeps the
bounded prefix.

## 4. Invariants

1. **The coordinator's authoritative store holds the router's floodfill
   advertisers.** Every advertiser the NetDB holds reaches the coordinator's
   store, up to the NetDB's own record ceiling. A floodfill the router has
   learned is selectable.
2. **The advertiser index is maintained, not derived.** It is updated on
   exactly the paths that insert and remove a record: the **new-key** path, the
   **replacement** path and the **removal** path. It is never rebuilt by a
   scan.
3. **The index is exact.** Every advertiser has exactly one entry and every
   entry names an advertiser. An advertiser the index missed would be
   invisible to the selection, which is a silent selection failure.
4. **The selection still filters on the capability.** `floodfill_advertisers()`
   keeps filtering on `advertises_floodfill()`, so a peer whose own signed
   `RouterInfo` does not advertise `f` is never yielded, whatever the index
   says.
5. **The selection's order is unchanged.** The selection still yields the
   advertisers in ascending `RouterHash` order.
6. **The store's public surfaces are unchanged.** `prune`, `insert`, `remove`,
   `get`, `contains`, `len`, `is_empty`, `encoded_bytes`, `config`, `stats`,
   `iter` and `floodfill_advertisers` keep their signatures and their results.
7. **The byte accounting and the floodfill advertiser count are unchanged.**
   The count is still maintained by the same three decision points, and the
   bytes are still released with the record.
8. **The group provider's bound is unchanged.**
   `MAX_GROUP_PROVIDER_ROUTER_INFOS` still bounds the group's snapshot, and the
   NTCP2 lookup's candidate pool, the peer-profile learn cadence and the
   controlled NTCP2 dial keep the bounded prefix.
9. **The coordinator's seed carries the advertisers.** The Destination-groups
   seed reads the advertisers, not the group provider's prefix.
10. **The reconcile's coordinator feed carries the advertisers.** The
    reconcile adds them to the coordinator's store through the canonical write
    API, and the peer-profile store's feed is unchanged.
11. **No configuration key is added, and no dependency is added.** The
    advertiser set is not a budget; it is a set.
12. **No new task, timer, listener or socket.** The feed rides the reconcile
    the product already drives.
13. **No advertisement.** `router.version` stays `0.9.62` and the inventory's
    advertised count stays **zero**.
14. **Plan 507's, Plan 538's, Plan 539's and Plan 540's lanes and guards stay
    byte-identical.** This plan changes the store's selection and the
    coordinator's feed, not the prune, not the count, not the composition
    root's population and not the relay roles' semantics.

## 5. Scope

### 5.1 In scope

- `crates/i2pr-netdb/src/store.rs` — the store's advertiser index, the
  selection that walks it, and the advertiser snapshot.
- `crates/i2pr-daemon/src/bootstrap.rs` — the advertiser snapshot the daemon
  reads.
- `crates/i2pr-daemon/src/lib.rs` — the Destination-groups seed.
- `crates/i2pr-daemon/src/peer_reconcile.rs` — the reconcile's advertiser
  snapshot and the coordinator's feed.

### 5.2 Out of scope

- The store's **prune** — Plan 540 made it walk the age index, and this plan
  does not touch it.
- The store's **statistics** — `stats()` stays O(1) and walks no record map.
- The store's **byte accounting** and Plan 538's floodfill advertiser count.
- The **group provider's bound**, and the three consumers that want any peers.
- The **peer-profile store's feed** — it keeps the bounded prefix. Its own
  narrowness is recorded as an out-of-scope item with an owner named.
- The **daemon's prune call site**, the bootstrap's public surface and the
  reconcile's existing `fn snapshot`, `fn insert_all` and `fn learn_all`.

### 5.3 Out of scope, with the owner named

- **The router's validated store is still the bootstrap's store.**
  (`crates/i2pr-daemon/src/bootstrap.rs:215`) The bootstrap owns the only
  authoritative validated store in the daemon, so the router's NetDB lock is
  still the bootstrap's mutex. **Owner:** the successor that gives the router a
  NetDB owner of its own. This plan does not change that, and it **reduces**
  what that successor must do: once the selection sees the advertisers, the
  lock's readers are not silently starved of the peers they select from.
- **The store's selection copy still happens under the router's NetDB lock.**
  The advertiser snapshot is a copy of the advertisers' records, taken under
  that lock. **Owner:** the same successor, whose NetDB owner could hand the
  selection the records it needs without a copy.
- **The peer-profile store's feed is still the bounded prefix.**
  (`crates/i2pr-daemon/src/peer_reconcile.rs:484`) The profiles learn an
  arbitrary hash-ordered subset of the NetDB. **Owner:** the successor that
  gives the router a NetDB owner of its own, which can give the profiles the
  peers they need.
- **The composition root's shape is proven statically, not behaviourally.**
  **Owner:** the successor that makes the composition root's relay arm
  reachable from a test.
- **The production SSU2 peer-test driver** — Plan 530's Low finding 2.
- **The `B` advertisement and `caps=f`** — their own owners.
- **The stock-peer half** — Plan 493's authorized two-host topology.

## 6. Required production changes

### 6.1 `crates/i2pr-netdb/src/store.rs`

1. `RouterInfoStore` (`crates/i2pr-netdb/src/store.rs:83`) gains a **private**
   field: an ordered set of the retained floodfill advertisers' hashes. The
   field is private, so the store's public surface is unchanged.

2. The index is maintained on **exactly** the paths that insert and remove a
   record, at the **same three decision points** where the floodfill
   advertiser count is adjusted:

   - the **new-key** path gains the advertiser when the incoming record
     advertises `f`;
   - the **replacement** path swaps the entry when the capability changes,
     because a replacement may publish a different capability set;
   - the **removal** path releases the entry with the record.

   The refusal paths — the stale replacement, the idempotent insert, the
   conflict and the capacity refusal — change nothing, so they change no
   entry.

3. `floodfill_advertisers()` (`crates/i2pr-netdb/src/store.rs:343`) walks the
   maintained index instead of scanning the record map, and **still filters**
   on `advertises_floodfill()`. The filter is what makes the selection exact:
   a peer whose own record does not advertise `f` is never yielded, whatever
   the index says. The index must therefore be a superset of the advertisers —
   and it is exactly the advertisers, so the two agree, which the lane proves.

4. The store gains one accessor, `floodfill_advertiser_snapshot()`, which
   returns the advertisers' records. The copy is bounded by the store's own
   record ceiling.

### 6.2 `crates/i2pr-daemon/src/bootstrap.rs`

5. The bootstrap gains `floodfill_advertiser_snapshot()`, which reads the
   store's advertisers. `validated_router_info_snapshot()` is **unchanged**:
   the group provider's bound stays exactly what it is, and the three
   consumers that want any peers keep it.

### 6.3 `crates/i2pr-daemon/src/lib.rs` and `crates/i2pr-daemon/src/peer_reconcile.rs`

6. The Destination-groups seed (`crates/i2pr-daemon/src/lib.rs:4427`) reads the
   **advertisers**, so the coordinator's authoritative store starts with the
   router's floodfills rather than with an arbitrary hash-ordered prefix.

7. The reconcile gains an advertiser snapshot, taken under the bootstrap's lock
   and **released** before it returns, and a write into the coordinator's store
   through the **canonical write API**, under the coordinator's lock only. The
   reconcile's existing `fn snapshot`, `fn insert_all` and `fn learn_all` are
   **unchanged**, so Plan 506's guard needs no extension. The advertiser feed
   is a **new** call beside them.

## 7. Design decisions

### 7.1 The index is maintained, never rebuilt

A derived index — one rebuilt by a scan — would move the population walk into
the index's construction and make the plan pointless. The index is maintained
at the same three decision points where the count is maintained, so the count
and the set can never disagree about *which* records are advertisers.

### 7.2 The index is exact, not approximate

An approximate index would be a silent selection failure: an advertiser the
index missed would never be selectable, and the router would refuse a
publication it could have made. There is no sampling, no cap and no estimate.

### 7.3 The selection keeps its filter

The filter is redundant against an exact index, and it is kept on purpose. It
is the **table's** rule — a peer is selected on its own signed evidence, not
on the index's word — and keeping it means the selection's observable
behaviour is unchanged, so Plan 538's guard needs no extension and a reader of
the store cannot mistake the index for the truth.

### 7.4 The feed is a new call, not a change to the old one

The reconcile's existing calls are Plan 506's guard's anchors. Changing what
`records` holds would change them and force an extension. The advertiser feed
is therefore a **separate** call, beside the existing ones, and the
coordinator's store ends up holding both the prefix and the advertisers — of
which only the advertisers matter, because the selection filters on the
capability.

### 7.5 The other consumers keep the prefix

The NTCP2 lookup's candidate pool, the peer-profile learn cadence and the
controlled NTCP2 dial want *any* peers, and the bounded prefix is the right
shape for them. The plan does not change them.

### 7.6 The copy is bounded, and the bound is the store's own ceiling

The advertiser snapshot is bounded by the NetDB's record ceiling — 4 096
records and 4 MiB by default. The copy is larger than the prefix's copy, and
the plan says so plainly: the feed's cost is proportional to the number of
floodfill advertisers, not to the population, because the store maintains the
advertiser index.

### 7.7 The lane proves the behaviour and the guard proves the shape

The index's exactness and the coordinator's selection are observable from
outside the store; the shape — that the index is maintained on every path and
that the selection walks it — is not. So the lane proves what the selection
**does** and the guard proves what the store **is**.

## 8. Work packages

1. **WP1 — the store.** The advertiser index, the selection that walks it, and
   the advertiser snapshot.
2. **WP2 — the daemon's read.** The bootstrap's advertiser snapshot.
3. **WP3 — the feed.** The Destination-groups seed and the reconcile's
   advertiser feed.
4. **WP4 — the lane.** A lane that proves the selection is exact, that the
   index never drifts, and that an advertiser outside the prefix is reachable
   through the coordinator.
5. **WP5 — the guard.** A guard that proves the shape and that no predecessor's
   evidence changed.
6. **WP6 — the negative controls.** Twelve controls that mutate the real tree
   and observe the named row fail.
7. **WP7 — the documentation and the inventory.**
8. **WP8 — the floor and the closure record.**

## 9. Failure, cancellation, restart and contention semantics

**Failure.** The plan adds no new failure mode to the router. The selection
still fails closed when the store holds no eligible candidate, and the
publication still refuses with a typed reason. The new copy can fail only if
the bootstrap's lock is poisoned, which the reconcile already handles as an
unavailable snapshot.

**Cancellation.** The plan touches no async path. The advertiser snapshot is
synchronous and bounded, the guard is dropped before the read returns, and no
lock is held across an await.

**Restart.** Nothing is persisted. The advertiser index is derived from the
store's own contents and is rebuilt with the store, so a restart cannot
inherit a stale index. The coordinator's store is rebuilt from the seed and
the reconcile's feed.

**Contention.** The reconcile takes the bootstrap's lock twice per pass now —
once for the group's bounded snapshot and once for the advertiser snapshot —
and each hold is a single bounded read. The coordinator's lock is taken once
for the advertiser write, and the two locks are never held at once. The
selection's cost inside the coordinator's lock is unchanged.

## 10. Compatibility and migration

**No wire change.** The router selects the same floodfills it could select
before; it can now also select the ones the bounded prefix hid.

**No configuration migration.** **No key is added.** The daemon's
configuration surface is unchanged, and the NetDB configuration keeps exactly
its twelve keys.

**No persisted state.** The advertiser index lives inside the store and is
rebuilt with it, so there is nothing to migrate.

**No change to the netdb crate's public surface.** No `i2pr-netdb` item is
removed or renamed. The store's new field is **private**, the selection keeps
its signature, and the new accessor is additive.

**No change to the daemon's public surface.** The bootstrap's new accessor is
additive; the reconcile's new methods are private.

**No change to the manifest or the lock.** `Cargo.toml` and `Cargo.lock` stay
byte-identical.

## 11. Required tests

### 11.1 The lane

`crates/i2pr-daemon/tests/floodfill_selection_visibility.rs` — the selection
over the **real** store, with an **independent** computation of the advertiser
set. The lane must prove:

1. the selection yields **exactly** the advertisers the store's own iterator
   says advertise `f`, and the count matches;
2. the index does not drift: an advertiser that is inserted is selected, and a
   replacement that **gains** the capability is selected;
3. a replacement that **loses** the capability is **not** selected, and a
   removal drops the advertiser from the selection;
4. the advertiser snapshot carries the advertisers and **not** the group
   provider's prefix;
5. an advertiser **outside** the prefix is still reachable through the
   coordinator's store and through its selection;
6. the coordinator selects the advertiser it was fed;
7. the selection's order is unchanged;
8. a non-advertiser is never selected.

The lane computes the advertiser set **independently**, by walking the store's
own iterator, and never by reading the index — so the lane and the store's own
bookkeeping are two different things that must agree.

### 11.2 The guard

`crates/i2pr-daemon/tests/floodfill_selection_visibility_boundary.rs` — a
static guard that is the authority for the **shape**. It must have an inverted
probe for every rule and a fail-closed probe for every item a rule names. The
rules:

1. the store **maintains** an advertiser index, and the index is updated on
   **every** path that inserts or removes a record;
2. the selection walks the **index** and **still filters** on the capability;
3. the coordinator's seed and the reconcile's feed carry the **advertisers**;
4. the group provider's bound and the three other consumers are **unchanged**;
5. **no** configuration key, **no** dependency and **no** task, timer, listener
   or socket is added, and **nothing** is advertised;
6. Plan 507's, Plan 538's, Plan 539's and Plan 540's lanes and guards stay
   **byte-identical**.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-daemon --test floodfill_selection_visibility -- --test-threads=1
cargo test --locked -p i2pr-daemon --test floodfill_selection_visibility_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test peer_reconcile_boundary --test netdb_retention_boundary --test floodfill_statistic_accounting --test floodfill_statistic_accounting_boundary --test routing_table_population_lock --test routing_table_population_lock_boundary --test netdb_prune_walk --test netdb_prune_walk_boundary -- --test-threads=1
cargo test --locked --workspace --all-targets --no-fail-fast -- --test-threads=1
python3 scripts/check-tooling-inventory.py
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

plus the routine floor in `AGENTS.md`, with this plan's two steps added.

## 13. Documentation updates

- `specs/support.toml` — a new experimental, non-advertised surface
  `netdb.floodfill-selection-visibility`.
- `specs/CONFORMANCE.md` — the count paragraph and a section for this plan.
- `specs/QUALIFICATION-GATES.md` — the gates this plan's lane and guard
  discharge.
- `docs/architecture/i2pr-netdb.md` — the store's advertiser index and the
  selection that walks it.
- `docs/architecture/i2pr-daemon.md` — the coordinator's feed and the live
  selection.
- The plan's own amendment log.

## 14. Acceptance criteria and stop conditions

The plan is **accepted** when all of the following hold:

1. the coordinator's authoritative store holds the router's floodfill
   advertisers, and an advertiser outside the group provider's prefix is
   reachable through the live selection;
2. the selection yields exactly the advertisers the store's contents imply,
   and the lane measures it against an independent computation;
3. the index never drifts, and the lane proves it through the selection's own
   behaviour;
4. the selection still filters on the capability, and the store's public
   surfaces are unchanged;
5. the group provider's bound and the three other consumers are unchanged;
6. no configuration key, dependency, task, timer, listener or socket is added,
   and `Cargo.toml` and `Cargo.lock` stay byte-identical;
7. `router.version` stays `0.9.62`, and the inventory's advertised count stays
   **zero**;
8. the full floor is green, with the closure record's table complete;
9. the guard's inverted probes each fail as intended and the tree is restored
   byte-identically after each.

**Stop conditions.** The plan stops and reports if:

- the selection's **observable** behaviour would change — the plan changes
  which peers the selection can see, never what it yields for a peer it can
  already see;
- Plan 507's, Plan 506's, Plan 538's, Plan 539's or Plan 540's guard would have
  to change — the plan's design exists precisely so that it does not;
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
- Plan 506's, Plan 507's, Plan 538's, Plan 539's and Plan 540's closed closure
  records are **not** rewritten. The discharge lives in this plan's closure
  record and in the registry's row.
- The plan **extends** no other plan's guard: Plan 507's, Plan 538's, Plan
  539's and Plan 540's lanes and guards stay byte-identical.
- The plan adds no dependency, so `Cargo.toml` and `Cargo.lock` stay
  byte-identical.
- A successor must **not** remove the selection's capability filter to "simplify"
  it, and must not rebuild the advertiser index by a scan: the index's
  exactness is a selection guarantee, not an optimization.

## 17. Amendment log

### 17.1 — 2026-11-11, registration

Plan 541 is registered as the successor to Plan 540 in the router's NetDB line:
the router's **authoritative** floodfill selection store is fed the group
provider's **hash-ordered 256-record prefix**, so the selection sees an
arbitrary subset of the NetDB rather than the router's floodfill advertisers,
and the router refuses its own LeaseSet2 publication and its destination build
hops even when its NetDB holds thousands of floodfills.

The scope is the store's **selection** and the **feed** into the coordinator's
authoritative store, not the store's prune, not its statistics, not its
accounting, and not the group provider's bound. The plan makes the store
maintain an **advertiser index** on exactly the paths that maintain the record
map and the advertiser count, so the selection walks the index and the
selection's feed carries the advertisers.

The selection's **observable** behaviour is unchanged: it still filters on the
capability, it still yields the advertisers in ascending `RouterHash` order,
and Plan 538's guard needs no extension. Plan 506's guard pins the reconcile's
existing calls, so the advertiser feed is a **new** call beside them rather
than a change to them. Plan 507's, Plan 538's, Plan 539's and Plan 540's lanes
and guards stay byte-identical.

The plan adds no store, no lock, no owner, no configuration key and no timer,
it changes no transport file and no runtime file, it adds no dependency, it
leaves `Cargo.toml` and `Cargo.lock` byte-identical, and it advertises nothing.

No closed closure record is rewritten by this plan. The corrections this plan
finds, if any, live in this plan's closure record and in the registry's row.

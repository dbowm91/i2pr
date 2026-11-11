# Plan 542 — the NetDB's retention must not depend on the service-tunnel posture

**Status:** registered. **Subsystem:** network-participation.
**Registered:** 2026-11-11. **Successor to:** Plan 541.

## 1. Objective

The router's NetDB must **turn over** in every posture in which the router is
running, and not only in the one posture in which a service-tunnel group is
enabled.

`RouterInfoStore::insert` **refuses** at the quota and **never evicts**
(`crates/i2pr-netdb/src/store.rs:211`), and the tree's own documentation of the
prune states the consequence in as many words: *"a NetDB that filled up would
refuse every new peer, permanently, and would keep a record the router's own
validation policy would refuse to accept today"*
(`crates/i2pr-daemon/src/bootstrap.rs:283`). The operation that prevents that —
`Bootstrap::prune` — has exactly **one** production caller,
`PeerReconcile::prune_bootstrap`
(`crates/i2pr-daemon/src/peer_reconcile.rs:432`), and `PeerReconcile` is
constructed **only** inside `ServiceProduct`
(`crates/i2pr-daemon/src/service_product.rs:1267`). The daemon's only
`ServiceProductSpec` is built inside `let mut owner = if has_enabled_groups`
(`crates/i2pr-daemon/src/lib.rs:4413`), where

```text
has_enabled_groups = service_runtime_active = service_tunnels_active(config)
                   = i2pcontrol.enabled
                     || (sam.enabled && sam_product_mode == RouterNetwork)
                     || any service tunnel enabled        (crates/i2pr-daemon/src/lib.rs:1500)
```

and every one of those is `false` by default
(`crates/i2pr-daemon/src/config.rs:1381`, `:1273`, `:1542`).

So in the posture where the router runs SSU2 and nothing else — a router that
publishes its RouterInfo and accepts its peers' validated records — the NetDB
is **write-only**. It fills to `netdb.max_records` (4 096 by default,
`crates/i2pr-daemon/src/config.rs:1141`) or `netdb.max_encoded_bytes` (4 MiB,
`crates/i2pr-daemon/src/config.rs:1145`) and then refuses **every** subsequent
peer for as long as the router runs. The retention prune is not merely
narrowed; it is **unreachable**.

This plan gives the retention prune an owner that runs whenever the NetDB is
enabled, so the router's peer knowledge turns over in every posture. It adds no
configuration key, no second retention policy, no new timer and no new
listener.

## 2. Why ready

**The tree already states the defect and the remedy.** The prune's own doc
comment names the failure mode (`crates/i2pr-daemon/src/bootstrap.rs:283`) and
the operation needed to prevent it already exists, is already bounded, is
already deterministic, and already takes its window from the router's own
acceptance window (`crates/i2pr-daemon/src/peer_reconcile.rs:81`). What is
missing is a **caller**, not a mechanism.

**The store's removal path already does the work.** `Bootstrap::prune`
(`crates/i2pr-daemon/src/bootstrap.rs:291`) delegates to the store's prune and
refreshes the reported snapshot, and the store's prune (`crates/i2pr-netdb/src/store.rs:350`)
walks Plan 540's maintained age index to the stale prefix. Nothing in the store
needs to change.

**A cadence owner already exists and already holds the bootstrap handle.** The
NetDB participation owner receives `bootstrap` at registration
(`crates/i2pr-daemon/src/lib.rs:2395`) and is registered **unconditionally** as
an `Optional` service (`crates/i2pr-daemon/src/lib.rs:2392`), so the service is
always on the graph; only its *body* is inert by default, because
`participation.start(...)` is called under `if enabled`
(`crates/i2pr-daemon/src/lib.rs:2424`) where `enabled = config.netdb.routing_table_enabled`
(`crates/i2pr-daemon/src/lib.rs:2398`). The owner is therefore the natural home
for a step that must run whenever the NetDB does.

**No predecessor's guard needs an extension.** Plan 507's lane and guard pin the
reconcile's ordering and its `PruneOutcome` reasons
(`crates/i2pr-daemon/tests/netdb_retention_live.rs`,
`crates/i2pr-daemon/tests/netdb_retention_boundary.rs`); this plan changes
nothing in `peer_reconcile.rs`. Plan 506's guard pins the reconcile's
`fn snapshot`, `fn insert_all` and `fn learn_all` literals
(`crates/i2pr-daemon/tests/peer_reconcile_boundary.rs:1072`); this plan leaves
all of them byte-identical. Plan 539's guard pins the population source
(`crates/i2pr-daemon/tests/routing_table_population_lock_boundary.rs`); this plan
does not touch `ValidatedStorePopulationSource`. Plan 540's and Plan 541's
guards pin the store's index maintenance and the selection's filter; this plan
changes neither.

**The window is already one source of truth.** `RETENTION_WINDOW_MS` is derived
from the validation policy through a `const fn`
(`crates/i2pr-daemon/src/peer_reconcile.rs:81`), so a second caller cannot
drift from the acceptance rule. The plan **moves** the constant's ownership; it
does not restate the window.

## 3. Current implementation evidence

### 3.1 The store refuses at the quota and never evicts

```text
// crates/i2pr-netdb/src/store.rs:211
if self.records.len() >= self.config.max_records {
    return InsertOutcome::CapacityExceeded;
}
if self
    .total_encoded_bytes
    .checked_add(encoded_len)
    .is_none_or(|total| total > self.config.max_total_encoded_bytes)
{
    return InsertOutcome::CapacityExceeded;
}
```

There is no eviction path. The store's only removals are `remove`
(`crates/i2pr-netdb/src/store.rs:249`) and `prune`
(`crates/i2pr-netdb/src/store.rs:350`), and `remove` has **no** production
caller in the daemon — every `.remove(&…)` in `crates/i2pr-daemon/src` names a
different map. So the prune is the router's **only** way to make room.

The defaults are 4 096 records and 4 MiB
(`crates/i2pr-daemon/src/config.rs:1141`, `:1145`), and `netdb.enabled` is
**true** by default (`crates/i2pr-daemon/src/config.rs:1137`).

### 3.2 The prune has exactly one production caller

```text
// crates/i2pr-daemon/src/peer_reconcile.rs:425
fn prune_bootstrap(&self, now_ms: u64) -> PruneOutcome {
    let Some(bootstrap) = self.bootstrap.as_ref() else {
        return PruneOutcome::NotApplicable;
    };
    let Ok(mut guard) = bootstrap.lock() else {
        return PruneOutcome::Unavailable;
    };
    PruneOutcome::Removed(guard.prune(now_ms, RETENTION_WINDOW_MS))
}
```

It is called from `PeerReconcile::run` (`crates/i2pr-daemon/src/peer_reconcile.rs:347`),
which is called from the product tick (`crates/i2pr-daemon/src/service_product.rs:4584`).

### 3.3 The reconcile exists only inside `ServiceProduct`

`PeerReconcile::new` has one production caller,
`build_peer_reconcile` (`crates/i2pr-daemon/src/service_product.rs:1267`),
called from `ServiceProduct::start` and
`ServiceProduct::start_over_existing_daemon`
(`crates/i2pr-daemon/src/service_product.rs:2585`, `:2905`). The daemon builds a
`ServiceProductSpec` at exactly one site
(`crates/i2pr-daemon/src/lib.rs:4485`), inside

```text
// crates/i2pr-daemon/src/lib.rs:4392
let has_enabled_groups = service_runtime_active;
…
// crates/i2pr-daemon/src/lib.rs:4413
let mut owner = if has_enabled_groups {
```

so the reconcile — and with it the prune — is composed **only** when a
service-tunnel group is enabled.

### 3.4 Every group is off by default

`i2pcontrol.enabled` (`crates/i2pr-daemon/src/config.rs:1381`), `sam.enabled`
(`:1273`) and `service_tunnels.enabled` (`:1542`) all default to `false`. The
SSU2 posture — `ssu2.enabled = true` with no group — therefore composes no
`ServiceProduct`, calls no reconcile, and prunes nothing.

### 3.5 The store is fed in that posture anyway

The bootstrap's store is fed from three live sites that do not depend on a
group:

- the reseed and the NetDB cache (`crates/i2pr-netdb-persist/src/reseed_ingest.rs:198`,
  `crates/i2pr-daemon/src/bootstrap.rs:429`);
- the authenticated unsolicited `DatabaseStore`
  (`crates/i2pr-daemon/src/lib.rs:5209`, through
  `Bootstrap::ingest_authenticated_router_store`);
- the discovery spine's RouterInfo reply
  (`crates/i2pr-daemon/src/service_product.rs:4334` — group-only, and the only
  one of the three that is).

So the population grows in every posture and turns over in one.

### 3.6 The participation owner is on the graph but inert

`register_netdb_participation_service` is called unconditionally
(`crates/i2pr-daemon/src/lib.rs:968`) and registers an `Optional` service with
`StartupRequirement::Optional` (`crates/i2pr-daemon/src/lib.rs:2438`). Its body
runs `participation.start(bootstrap.as_ref(), …)` under
`if enabled` (`crates/i2pr-daemon/src/lib.rs:2424`), where
`enabled = config.netdb.routing_table_enabled` (`crates/i2pr-daemon/src/lib.rs:2398`),
which defaults to `false`. The owner therefore has the bootstrap handle and a
supervised cancellation scope, and does no per-pass work by default.

### 3.7 What the plan does not claim

- It does **not** claim the router's NetDB is correct in the group posture. It
  is: Plan 507's prune runs there.
- It does **not** claim the routing table, the exploration job or the peer
  profile store should be enabled in the SSU2 posture. Those are separate
  features with their own gates, and this plan leaves every one of them exactly
  as gated as it is.
- It does **not** claim a stock-peer measurement. The defect and the fix are
  both local.

## 4. Invariants

1. **The retention prune runs whenever the NetDB does.** A router with
   `netdb.enabled = true` prunes its NetDB on the participation cadence,
   whatever its service-tunnel posture.
2. **There is exactly one retention window.** The window is derived from the
   router's own acceptance policy through the existing `const fn`; the plan
   moves the constant, it does not restate it, and it adds no configuration
   key.
3. **The prune keeps its semantics.** It removes every record past the window,
   in one call, through the store's own `prune`, which walks Plan 540's age
   index.
4. **The prune's cost stays proportional to what it removes.** The new caller
   adds no scan: it holds the bootstrap's lock for the same bounded index walk
   the reconcile already holds it for.
5. **The lock discipline is unchanged.** The bootstrap's lock is taken for the
   prune and released before anything else is taken; no lock is held across an
   await, and no two locks are held at once.
6. **The routing table, the exploration job, the profile store and the
   discovery spine keep their existing gates.** Not one of them becomes active
   in a posture where it is inactive today.
7. **The prune is counted by reason.** A prune that runs, one that is not
   applicable and one whose lock is unavailable are each counted, exactly as
   the reconcile counts them.
8. **The router never prunes its own record.** The router's own `RouterInfo`
   lives beside the store, not inside it, and Plan 507's rule stands.
9. **No new task, timer, listener or socket.** The step rides the cadence owner
   the supervisor already runs.
10. **No configuration key is added, and no dependency is added.**
11. **No advertisement.** `router.version` stays `0.9.62`, `caps` is unchanged,
    and the inventory's advertised count stays **zero**.
12. **Plan 506's, Plan 507's, Plan 538's, Plan 539's, Plan 540's and Plan 541's
    lanes and guards stay byte-identical.**

## 5. Scope

### 5.1 In scope

- `crates/i2pr-daemon/src/netdb_participation.rs` — the retention step on the
  participation owner, its counters, and the window's new home.
- `crates/i2pr-daemon/src/lib.rs` — starting the owner's cadence independently
  of the routing-table switch, leaving the routing table's population gated.
- `crates/i2pr-daemon/src/peer_reconcile.rs` — the constant's ownership moves;
  the reconcile's own behaviour is unchanged.

### 5.2 Out of scope

- The store — Plan 540 and Plan 541 changed it, and this plan does not.
- The reconcile's ordering, its other steps, and its `PruneOutcome` reasons.
- The routing table's population, the exploration job, the discovery spine and
  the profile store.
- The store's quota defaults.
- The peer-profile store's bounded feed.

### 5.3 Out of scope, with the owner named

- **The router's validated store is still the bootstrap's store.**
  (`crates/i2pr-daemon/src/bootstrap.rs:215`) **Owner:** the successor that
  gives the router a NetDB owner of its own. This plan **reduces** what that
  successor must do: the retention policy will no longer be a property of the
  composition root's shape.
- **The participation owner is inert by default for its table and exploration
  features.** That is deliberate and stays. **Owner:** Plan 433's blocked
  composition for the routing table's reader.
- **The production SSU2 peer-test driver** — Plan 530's Low finding 2.
- **The `B` advertisement and `caps=f`** — their own owners.
- **The stock-peer half** — Plan 493's authorized two-host topology.

## 6. Required production changes

### 6.1 `crates/i2pr-daemon/src/netdb_participation.rs`

1. The retention step: a method on the participation owner that takes the
   bootstrap handle, prunes under its lock, releases it, and records the
   outcome as a `PruneOutcome`-shaped count — reused from
   `crate::peer_reconcile::PruneOutcome` so the two callers cannot disagree
   about what a prune outcome *is*.
2. The window: `RETENTION_WINDOW_MS` moves here, or is re-exported from here, so
   there is one name for one policy. The `const fn` derivation is not restated.
3. The counters: three saturating counters — runs, removed, unavailable — on
   the owner's existing counter set, published through the inspection seam
   exactly as Plan 507's reconcile counters are.

### 6.2 `crates/i2pr-daemon/src/lib.rs`

4. The cadence owner starts its **retention** step whenever
   `config.netdb.enabled` is true, independently of
   `config.netdb.routing_table_enabled`. The routing table's population, the
   exploration job and the profile feed stay behind their existing switch:
   `participation.start(...)` keeps its `enabled` gate for those, and the
   retention step is called outside it.
5. The retention step rides the owner's **existing** cadence loop
   (`run_profile_maintenance`'s period, or the owner's own maintenance task),
   so no second timer is created. If the owner's loop is itself behind the
   switch, the retention step gets the smallest possible loop that the
   supervisor already owns — the same period, the same child scope, the same
   cancellation.

### 6.3 `crates/i2pr-daemon/src/peer_reconcile.rs`

6. The constant's definition moves to `netdb_participation.rs` and the
   reconcile imports it. The reconcile's `prune_bootstrap`,
   `prune_destinations`, `snapshot`, `insert_all`, `learn_all` and `run` are
   **byte-identical** apart from the import, so Plan 506's and Plan 507's
   guards are untouched.

## 7. Design decisions

### 7.1 The prune gets its own caller, not a second policy

The defect is a missing **caller**. The operation, its window, its determinism,
its cost and its lock discipline are all already right. The plan adds a caller
and nothing else.

### 7.2 The window is moved, never restated

`RETENTION_WINDOW_MS` is derived from the validation policy through a `const
fn`, so the retention rule cannot drift from the acceptance rule. A second
caller that restated the window would create the second source of truth Plan
507 exists to remove. The constant therefore moves to the module that now owns
the policy, and the reconcile imports it.

### 7.3 The retention step is unconditional; the features around it stay gated

`netdb.enabled` is what makes the router hold a NetDB at all
(`crates/i2pr-daemon/src/config.rs:1137`), and retention is a property of
holding one. The routing table, the exploration job and the profile feed are
**features over** that NetDB, and they keep their switches. Conflating the two
is the defect: today the router's ability to *forget* is gated on its ability
to *use*.

### 7.4 The step rides the existing cadence

The owner already has a supervised cadence under a child scope
(`crates/i2pr-daemon/src/lib.rs:2006`). Creating a second timer for the same
period would be a second thing to cancel, a second thing to bound and a second
thing to get wrong. The step is a call inside the loop that exists.

### 7.5 The outcome type is shared, not duplicated

The reconcile's `PruneOutcome` already distinguishes *ran*, *not applicable*
and *unavailable*. A second enum with the same three states would inevitably
diverge in a reason label or a counter. The retention step reuses it.

### 7.6 The lane proves the behaviour; the guard proves the reachability

The lane builds a real `Bootstrap` at its quota, fills it, shows that the next
insert is refused, drives the **production** owner, and shows that the store
accepts the peer afterwards. The lane cannot see *which* composition the owner
is in, so the guard proves the reachability statically: that the retention step
has a caller outside `peer_reconcile.rs`, that the caller is not behind the
routing-table switch, and that the window has one definition.

### 7.7 What the lane cannot see, and what the guard therefore owns

The lane constructs the owner directly, so it observes the step's **behaviour**
and not its **reachability in the daemon's graph**. Plan 507's lane had exactly
this shape and constructed `PeerReconcile` by hand — which is why the
reachability defect survived it. This plan's guard, not its lane, is the
authority for reachability.

## 8. Work packages

1. **WP1 — the retention step.** The owner's prune method and its counters.
2. **WP2 — the constant's move.** One definition, imported by both callers.
3. **WP3 — the composition.** The retention step outside the routing-table
   switch, on the existing cadence.
4. **WP4 — the lane.** Fill a real store to its quota, prove the refusal, drive
   the production owner, prove the peer is accepted afterwards.
5. **WP5 — the guard.** The reachability rules, each with an inverted probe.
6. **WP6 — the negative controls.** Each mutates the real tree and observes its
   named row fail.
7. **WP7 — the documentation and the inventory.**
8. **WP8 — the floor and the closure record.**

## 9. Failure, cancellation, restart and contention semantics

**Failure.** A prune that cannot run is **counted**, never fatal, and the store
keeps its contents — the reconcile's existing contract. A poisoned bootstrap
lock is `Unavailable`, not a panic. The retention step is `Optional` in the
startup sense: a router that cannot prune still starts and still serves.

**Cancellation.** The step is synchronous and short. It takes the bootstrap's
lock and releases it before returning; no lock is held across an await. The
supervisor's child scope cancels the cadence, exactly as it cancels the
owner's other steps.

**Restart.** Nothing is persisted. The age index is rebuilt with the store, so
a restart cannot inherit a stale index, and the first pass repairs a store that
filled while a previous run had no prune.

**Contention.** The step holds the router's NetDB lock for Plan 540's bounded
index walk — proportional to what it removes, not to the population. It adds
one such hold per cadence to the composition; in the SSU2 posture it replaces
**zero** holds, which is the point.

## 10. Compatibility and migration

**No wire change.** The router refuses and accepts exactly the records it would
have, minus the ones past its own acceptance window.

**No configuration migration.** **No key is added.** The NetDB configuration
keeps exactly its keys, and the retention window remains derived rather than
configured.

**No persisted state.** The index lives inside the store.

**No change to the netdb crate's public surface.** `i2pr-netdb` is unchanged.

**No change to the daemon's public surface.** The retention step is a method on
an existing owner; the counters are additive.

**No change to the manifest or the lock.** `Cargo.toml` and `Cargo.lock` stay
byte-identical.

**A behaviour change in one posture, deliberately.** A router running SSU2
alone will now drop records past its acceptance window. That is the defect
being fixed, and it is a change the router's own validation policy already
demanded: the record it keeps is one it would refuse today.

## 11. Required tests

### 11.1 The lane

`crates/i2pr-daemon/tests/netdb_retention_posture.rs` — the retention step over
a **real** store and the **production** owner. The lane must prove:

1. a store filled to its quota **refuses** the next insert, and the refusal is
   `CapacityExceeded`;
2. after the owner's retention step runs, a record past the window is gone and
   the peer the store refused is **accepted**;
3. a record inside the window **survives** the step;
4. the step is **idempotent**: a second pass over a store with nothing stale
   removes nothing and is counted as a run;
5. a poisoned bootstrap lock is counted **unavailable**, and the store keeps
   its contents;
6. the router's own `RouterInfo` is never a victim;
7. the removal count the step reports equals the number of records the store's
   own iterator says were stale — computed **independently**, before the step;
8. the step is bounded: it never removes a record inside the window, whatever
   the population.

The lane computes the stale set **independently**, by walking the store's own
iterator, and never by reading the age index.

### 11.2 The guard

`crates/i2pr-daemon/tests/netdb_retention_posture_boundary.rs` — a static guard
that is the authority for **reachability**. It must have an inverted probe for
every rule and a fail-closed probe for every item a rule names. The rules:

1. the retention step has a caller **outside** `peer_reconcile.rs`;
2. that caller is **not** behind the routing-table switch — the step is called
   where `enabled` is not the condition;
3. the retention window has **exactly one** definition, and both callers import
   it;
4. the reconcile's `prune_bootstrap`, `prune_destinations`, `snapshot`,
   `insert_all`, `learn_all` and `run` are unchanged, and its ordering is
   unchanged;
5. the routing table's population, the exploration job, the profile feed and
   the discovery spine keep their existing gates, and **no** configuration key
   is added;
6. **no** new task, timer, listener or socket is added, and nothing is
   advertised;
7. Plan 506's, Plan 507's, Plan 538's, Plan 539's, Plan 540's and Plan 541's
   lanes and guards stay **byte-identical**.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-daemon --test netdb_retention_posture -- --test-threads=1
cargo test --locked -p i2pr-daemon --test netdb_retention_posture_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test netdb_retention_live --test netdb_retention_boundary --test peer_reconcile_boundary --test floodfill_statistic_accounting_boundary --test routing_table_population_lock_boundary --test netdb_prune_walk_boundary --test floodfill_selection_visibility_boundary -- --test-threads=1
cargo test --locked --workspace --all-targets --no-fail-fast -- --test-threads=1
python3 scripts/check-tooling-inventory.py
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

plus the routine floor in `AGENTS.md`, with this plan's two steps added.

## 13. Documentation updates

- `specs/support.toml` — a new experimental, non-advertised surface
  `netdb.retention-posture`.
- `specs/CONFORMANCE.md` — the count paragraph and a section for this plan.
- `specs/QUALIFICATION-GATES.md` — the gates this plan's lane and guard
  discharge.
- `docs/architecture/i2pr-daemon.md` — the retention step's new owner and the
  posture it is no longer gated on.
- `docs/architecture/i2pr-netdb.md` — the store's retention is reached in every
  posture.
- The plan's own amendment log.

## 14. Acceptance criteria and stop conditions

The plan is **accepted** when all of the following hold:

1. a router with `netdb.enabled = true` and no service-tunnel group prunes its
   NetDB on the participation cadence;
2. a store at its quota accepts a peer after the step runs, and the step's
   removal count equals an independently computed stale set;
3. a record inside the window survives, and the router's own `RouterInfo` is
   never a victim;
4. the retention window has exactly one definition and no configuration key is
   added;
5. the routing table, the exploration job, the profile store and the discovery
   spine keep their existing gates;
6. no new task, timer, listener or socket is added, and `Cargo.toml` and
   `Cargo.lock` stay byte-identical;
7. `router.version` stays `0.9.62`, and the inventory's advertised count stays
   **zero**;
8. the full floor is green, with the closure record's table complete;
9. the guard's inverted probes each fail as intended and the tree is restored
   byte-identically after each.

**Stop conditions.** The plan stops and reports if:

- the fix would require enabling the routing table, the exploration job or the
  profile store in a posture where they are off — that is a different plan's
  scope, and the retention policy must not be smuggled in behind it;
- the fix would require a new configuration key — the window is derived, and a
  key would create the second source of truth Plan 507 removed;
- Plan 506's, Plan 507's, Plan 538's, Plan 539's, Plan 540's or Plan 541's
  guard would have to change — the plan's design exists precisely so that it
  does not;
- the floor's `cargo deny check advisories bans sources` fails;
- a stock peer would be required to prove any of the above.

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

- The plan is **experimental** and the retention policy is internal. Do not
  describe the change as released, supported or advertised.
- The delivery is confined to the controlled loopback profile.
- The defect is **reachability**, not mechanism: do not "fix" it by making the
  prune faster or by widening the store's quota.
- Do not enable the routing table, the exploration job or the profile store to
  make the retention step reachable. The retention policy is a property of
  holding a NetDB, and the features over it keep their switches.
- Plan 506's, Plan 507's, Plan 538's, Plan 539's, Plan 540's and Plan 541's
  closed closure records are **not** rewritten. The discharge lives in this
  plan's closure record and in the registry's row.
- The plan **extends** no other plan's guard.
- The plan adds no dependency, so `Cargo.toml` and `Cargo.lock` stay
  byte-identical.
- A successor must **not** restate the retention window. It is derived from the
  validation policy through a `const fn`, and a second value would be a second
  source of truth.

## 17. Amendment log

### 17.1 — 2026-11-11, registration

Plan 542 is registered as the successor to Plan 541 in the router's NetDB line:
the router's retention prune has exactly **one** production caller, and that
caller is composed only when a service-tunnel group is enabled, so a router
that runs SSU2 alone fills its NetDB to its quota and then refuses every
subsequent peer **permanently**.

The scope is the prune's **reachability**, not its mechanism. The store's
prune, its window, its determinism, its cost and its lock discipline are all
already right; the plan adds a caller that runs whenever the NetDB does, moves
the window to one definition, and leaves every feature over the NetDB gated
exactly as it is.

The plan **extends** no other plan's guard: Plan 506's, Plan 507's, Plan 538's,
Plan 539's, Plan 540's and Plan 541's lanes and guards stay byte-identical.

The plan adds no store, no lock, no configuration key, no second timer and no
listener, it changes no transport file and no runtime file, it adds no
dependency, it leaves `Cargo.toml` and `Cargo.lock` byte-identical, and it
advertises nothing.

No closed closure record is rewritten by this plan. The corrections this plan
finds, if any, live in this plan's closure record and in the registry's row.

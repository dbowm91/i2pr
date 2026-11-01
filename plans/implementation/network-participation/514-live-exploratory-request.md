# Plan 514 — the live exploratory request

| Field | Value |
| --- | --- |
| Plan | 514 |
| Subsystem | network-participation |
| Status | registered |
| Discharges | the exploratory request's floodfill selection (Plan 513's spine asks an arbitrary floodfill, not the closest one) and `netdb.exploration_enabled` (a parsed, defaulted, cross-validated configuration key that no production code reads) |
| Predecessors | Plan 489 (the floodfill-indexed routing table and the exploration job), Plan 497 (the routing table's consumer and its closest-first selection), Plan 509 (the lookup-target API-version decision), Plan 513 (the live NetDB discovery spine) |
| Reference pins | i2pd `2.61.0` = `635b013a612ff47278ef02acf8580a28e10e26c5`; Java I2P `2.13.0` = `9134f808337b401e8e53c73734c81fab04280c9d` |
| Advertises | nothing. `router.version` stays `0.9.62`, `caps` and the published RouterInfo are unchanged, `Cargo.toml` and `Cargo.lock` stay byte-identical |

## 1. Objective

Make the router's **exploratory request** ask the **closest** floodfill it
knows, and make the operator's switch over it **real**.

Plan 513 landed the live discovery spine: the router now starts an exploratory
lookup, ingests the peers the answer names, and resolves one of them into a
signed `RouterInfo`. The spine is live, but the request it sends is aimed
wrongly, and the operator's control over it does nothing.

Two defects, one subject:

1. **The exploratory request asks an arbitrary floodfill.** The live path picks
   the **lowest-hash** floodfill advertiser in the flat store
   (`crates/i2pr-daemon/src/lib.rs:2074`) and sends the request to it. The
   routing table's **closest-first** selection — the mechanism Plan 489 built
   and Plan 513 gave a reader — is not consulted on this path at all. The
   reference asks the floodfill **closest to the random search key**
   (`libi2pd/NetDbRequests.cpp:479`).
2. **`netdb.exploration_enabled` is a dead key.** It is parsed, defaulted to
   `false`, and cross-validated against `netdb.routing_table_enabled`
   (`crates/i2pr-daemon/src/config.rs:3867`). **No production code reads it.**
   An operator who sets `routing_table_enabled = true` and leaves
   `exploration_enabled = false` — the default — gets DHT explorations anyway.

A router that always asks the same peer is not participating in a DHT; it is
polling one host. And a configuration key that lies is worse than no key.

## 2. Why ready

1. **Both defects are on landed code.** The spine is Plan 513's; the table's
   selection is Plan 489's and Plan 497's; the configuration key is older
   still. Nothing here is blocked on an external environment.
2. **The machinery is complete and unused.** `RoutingTableConsumer::begin_exploration`
   (`crates/i2pr-daemon/src/netdb_participation.rs:880`) already selects the
   exploration's peers through the table's closest-first selection
   (`:2433-2439` → `plan_optimized_lookup`). It has **no production caller**;
   its only callers are the module's own `#[cfg(test)]` block and the tests.
3. **The routing key is already computed.** `drive_discovery` already derives
   the search key's daily routing key
   (`crates/i2pr-daemon/src/lib.rs:2082-2086`) and already hands it to
   `begin_exploration` (`:2089`). The routing key the table's selection needs
   is the one the code **already has** and does not use.
4. **The precedent exists.** Plans 496, 502, 510, 512 and 513 each took a
   source-side defect that a blocked plan had recorded, and closed it in its
   own plan-of-record.
5. **Plan 513's own record leaves the thread open.** The exploratory request's
   selection is not mentioned among Plan 513's closures because Plan 513's
   subject was the *spine*, not the *aim*.

## 3. Current implementation evidence

### 3.1 The live exploratory request asks the lowest-hash floodfill

`crates/i2pr-daemon/src/lib.rs`, `NetDbParticipation::drive_discovery`:

```rust
let Some(floodfill) = store.floodfill_advertisers().next().map(|peer| peer.key()) else {
    return;
};
```

- `crates/i2pr-netdb/src/store.rs:273-277` — `floodfill_advertisers()` filters
  the store's validated records for `advertises_floodfill()`.
- `crates/i2pr-netdb/src/store.rs:84` — the store's records are a
  `BTreeMap<RouterHash, ValidatedRouterInfo>`, so `values()` iterates in
  **key order** and `.next()` is the **lowest-hash** floodfill advertiser.
- The choice therefore does not depend on the search key at all: every
  exploration asks the **same** peer, until a smaller-hash floodfill enters the
  store.

The reference does the opposite. `libi2pd/NetDbRequests.cpp:472` draws a fresh
random hash per exploration and `:479` selects the target:

```cpp
auto floodfill = netdb.GetClosestFloodfill (randomHash, dest->GetExcludedPeers ());
```

`libi2pd/NetDb.cpp:1308-1320` shows what "closest" means there: the target's
**routing key** is derived (`:1312`, `CreateRoutingKey`) and the floodfill
bucket set's closest-first search runs on it (`:1316`,
`m_Floodfills.FindClosest (destKey, num, ...)`), skipping unreachable and
already-asked peers.

### 3.2 The table's closest-first selection exists, and has no production caller

- `crates/i2pr-daemon/src/netdb_participation.rs:880-896` —
  `RoutingTableConsumer::begin_exploration`, the table-based entry point.
- `crates/i2pr-daemon/src/netdb_participation.rs:2433-2439` —
  `exploration_candidates` is `self.plan(store, routing_key, &[]).queryable_hashes()`.
- `crates/i2pr-netdb/src/optimized_lookup.rs:94-134` — `plan_optimized_lookup`
  runs `table.select(routing_key, table.len())` (`:107`) and keeps only peers
  that `advertises_floodfill()` (`:118`); a non-floodfill is **refused** and
  counted, and a peer the store does not hold is **pending**.
- `crates/i2pr-netdb/src/bucket_set.rs:515-541` — `select` sorts by
  `distance(entry.position, target)` and breaks ties by the lower `RouterHash`,
  so the order is deterministic and independent of the table's internals.

The only production call to `begin_exploration` is the **spine's**, at
`crates/i2pr-daemon/src/netdb_participation.rs:1378`, and it takes the
floodfill **as a parameter** — so it never asks the table either. The table's
closest-first selection is reachable today only from `#[cfg(test)]` and from
`crates/i2pr-daemon/tests/`.

### 3.3 The operator's exploration switch reads nothing

- `crates/i2pr-daemon/src/config.rs:250-251` — the `[netdb] exploration_enabled`
  field, with `default_netdb_exploration_enabled`.
- `crates/i2pr-daemon/src/config.rs:1154-1156` — that default is `false`.
- `crates/i2pr-daemon/src/config.rs:3867-3871` — the validator refuses
  `exploration_enabled = true` while `routing_table_enabled = false`, so the
  key is **cross-validated** and then never consulted.
- `crates/i2pr-daemon/src/config.rs:249-251` and `:1797-1800` — the field's own
  doc comments already state the contract the wiring is missing: *"whether the
  router runs exploratory lookups"*, and *"Exploration is **off by default**,
  and it cannot be enabled while the routing table is off"*.
- `crates/i2pr-daemon/src/lib.rs:2340` — the participation service's own gate is
  `config.netdb.routing_table_enabled`, and `:2366-2373` starts the owner only
  when it is set.
- `crates/i2pr-daemon/src/lib.rs:2011` — the owner drives
  `drive_discovery` on its cadence, with no second gate.

A search across `crates/` finds `exploration_enabled` in `config.rs` and in one
test's fixture — and in **no** production read.

### 3.4 Plan 509's decision is not enforced on the two new live lookup paths

Plan 509 decided that **the lookup target is not version-gated**
(`plans/closure/network-participation/509-status.md`). Its guard makes that
decision structural by naming the selection sites:

- `crates/i2pr-daemon/tests/lookup_target_version_boundary.rs:340-361` —
  `UNGATED_SELECTION_SITES`: `begin_lease_lookup_inner`, and the lookup
  engine's `start`, `start_with_preferred` and `advance_active`.
- `crates/i2pr-daemon/tests/lookup_target_version_boundary.rs:364-379` —
  `GATED_SELECTION_BODIES`: the same sites, which must not contain the
  version-gated selection.

Plan 513 added two live lookup paths — `begin_routerinfo_lookup`
(`crates/i2pr-daemon/src/netdb_seam.rs:705`) and `begin_exploratory_lookup`
(`:834`) — and **neither is named in either list**. The decision is therefore
unenforced on them: adding the version-gated selection to either would change
which floodfills the router can ask, silently, with every guard still green.

## 4. Invariants that must not regress

1. **The RouterInfo-kind resolution stays live.** Plan 513's spine, its
   deliveries, its reply ingestion and the suggestion rule are untouched.
2. **The table may only reorder, never narrow.** The RouterInfo-kind path's
   preference keeps its Plan 497 M1 rule.
3. **Plan 102's authority amendment is honoured.** The RouterInfo-kind request
   still rides the **outbound** exploratory tunnel and takes its reply through
   the **inbound** one.
4. **Every fallback is counted by reason**, on the table's counters and on the
   spine's. No fallback is silent.
5. **Plan 509's decision holds on every live lookup path**, including the two
   Plan 513 added.
6. **No peer is blamed.** Nothing here writes a peer's fault into a profile,
   and a local condition is never recorded against a peer.
7. **No unbounded state.** The selection is bounded by the table's own bounds
   and by the lookup's own budget.
8. **No advertisement.** `router.version` stays `0.9.62`; `caps` and the
   published RouterInfo are unchanged; the inventory's advertised count stays
   zero; `Cargo.toml` and `Cargo.lock` stay byte-identical.

## 5. Scope

**In scope.**

- **WP1 — the closest-floodfill selection.** The exploratory request's target
  comes from the routing table's closest-first selection, keyed by the search
  key's routing key, with the reason counted when the table cannot answer and
  **no silent arbitrary-peer fallback**.
- **WP2 — the operator's switch.** `netdb.exploration_enabled` gates the
  exploratory request, and the default preserves today's observable behaviour
  for an operator who enabled the routing table.
- **WP3 — the guard.** A new lane and a new structural guard pinning WP1 and
  WP2, and extending Plan 509's lookup-target decision to the two live lookup
  paths Plan 513 added.
- The records, the controls, the floor and the closure.

**Out of scope, and why.**

- **A stock-peer qualification.** The mechanism is proven over the tree's own
  surfaces; the external half stays Plan 493's.
- **The `caps=f` advertisement.** Plan 494's, gated on Plan 493.
- **The exploration's peer *budget* and *advance*.** The budget, the deadline
  and the rotation are Plan 489/500/512's and are unchanged.
- **The `NetDbTunnelCoordinator`'s composition.** Plan 433 owns it and it is
  blocked.
- **Removing a configuration key.** WP2 makes the key real; it does not
  remove it, so no operator's file stops parsing.

## 6. Production changes

| File | Change |
| --- | --- |
| `crates/i2pr-daemon/src/netdb_participation.rs` | the spine's exploratory start asks the routing table for the **closest** floodfill to the search key's routing key, and refuses the request — counted by reason — when the table names none. |
| `crates/i2pr-daemon/src/lib.rs` | the live discovery cycle derives the exploratory request's target from the table instead of the flat store's iteration order, and honours the operator's exploration switch. |
| `crates/i2pr-daemon/src/config.rs` | `netdb.exploration_enabled` becomes the exploratory request's real gate, with the default the tree already documents. |

The plan does not add a dependency, and `Cargo.toml` and `Cargo.lock` stay
byte-identical.

## 7. Design decisions the plan must make explicit

### D1 — The exploratory request asks the table's closest floodfill, or none.

The target is the table's closest-first answer for the search key's routing
key. When the table names no floodfill, the request is **refused and counted**
— it is not silently redirected to an arbitrary peer. The reference's own
behaviour agrees: i2pd calls `RequestComplete (randomHash, nullptr)` when
`GetClosestFloodfill` returns nothing (`libi2pd/NetDbRequests.cpp:502-503`),
rather than asking a substitute.

### D2 — The table's answer is used as a target, not as a preference.

The RouterInfo-kind path consumes the table as a **preference** that reorders
the flat store's selection and never narrows it. The exploratory request is
different: it asks **one** peer, and the right one is the closest. So the
table's answer is the target itself. The no-narrowing rule is not violated:
the table's candidates are a subset of the store's floodfills, and the plan
adds no narrowing to the **RouterInfo** path.

### D3 — The operator's switch is honoured as the tree already documents it.

`netdb.exploration_enabled` gates the exploratory request, and **its default
stays `false`**. The tree already states this contract in two places:

- `crates/i2pr-daemon/src/config.rs:249-251` — the field's own doc comment
  reads *"Plan 489: whether the router runs exploratory lookups."*
- `crates/i2pr-daemon/src/config.rs:1797-1800` — the resolved field's doc
  comment reads *"Exploration is **off by default**, and it cannot be enabled
  while the routing table is off: an exploration with nowhere to put the peers
  it discovers is refused at parse time."*
- `crates/i2pr-daemon/src/config.rs:3867-3871` — the validator enforces the
  second half of that sentence at parse time.

So the key was never ambiguous: Plan 489 designed an **opt-in** exploration
with the routing table as its prerequisite, and the wiring was simply never
done. The plan therefore wires it exactly as documented and moves **no**
default:

- `routing_table_enabled = false` (the default) → no table, no participation
  owner, no spine, no exploration — unchanged;
- `routing_table_enabled = true`, `exploration_enabled` unset (default
  `false`) → the table is maintained and persisted, and **no** exploration
  runs — **changed**, and this is the defect being fixed: today the
  exploration runs anyway, against the key's documented contract;
- `routing_table_enabled = true`, `exploration_enabled = true` → the full
  discovery spine, as Plan 513 landed it — unchanged.

The one observable change is the second row, and it is the point of WP2. The
plan records it as a finding, and the correction to Plan 513's claim that the
spine runs unconditionally lives in **this** closure record and in Plan 513's
registry row — Plan 513's closure record is **not** rewritten.

### D4 — The switch gates the exploration only, never the resolution.

The RouterInfo-kind resolution is Plan 513's half and it is the spine's
**production reader** for the routing table. If `exploration_enabled = false`
gated the whole spine, the table would have **no** production reader again and
Plan 497's **M2** would reopen. So the switch gates the *exploratory request*
only, and the resolution stays live.

### D5 — Every fallback is counted, and a fallback is never silent.

The table already counts `Disabled`, `NoCandidate` and `Unavailable`. WP1 adds
the spine's own refusal counter, so a table that cannot answer ends the
request **honestly** instead of quietly asking somebody else.

### D6 — Plan 509's decision is enforced on every live lookup path.

The two paths Plan 513 added are named in Plan 509's guard, so the
"the lookup target is not version-gated" decision holds on them as well. The
guard's own premise check is extended so a change to the specification's
table is reported rather than assumed away.

### D7 — The routing table's production reader stays reachable.

Plan 497's **M2** was a *structural* finding: the table was populated and
persisted and **nothing asked it**. Plan 513 gave it a production reader — the
RouterInfo-kind resolution's preference. This plan must not reopen M2, so:

- the switch gates the **exploratory request** only, never the resolution
  (D4);
- Plan 497's M2 discharge is stated as what it is — the reader is **reachable
  in production code**, not that it fires in every posture. The table itself
  is operator opt-in (`routing_table_enabled` defaults to `false`), so "the
  reader does not fire in the default posture" was already true of Plan 513's
  discharge and is not reopened here;
- the closure record says plainly that with the exploration switched off the
  table is maintained and persisted but unread, which is the operator's
  choice and is recorded rather than hidden.

### D8 — Nothing is advertised, and the inventory's figure moves honestly.

The inventory's figures move only because the tree now does the work. The
plan's surface moves to `experimental` only when the lane has proved it, and
nothing is advertised.

## 8. Work packages

**WP1 — the closest-floodfill selection.** The spine's exploratory start takes
the store and the search key's routing key, asks the routing table for the
closest floodfill, and either addresses the request to that peer or refuses it
and counts the reason. The flat store's iteration order is no longer a
selection.

**WP2 — the operator's switch.** The configuration key gates the exploratory
request, the default moves as D3 says, the validator's existing cross-check
is kept, and the configuration's own tests are updated to the new meaning.

**WP3 — the lane.** A new lane that drives the mechanism end to end over the
tree's own surfaces: the request's target is the **closest** floodfill and not
the lowest-hash one; the table's refusal is counted and the request is
refused rather than redirected; the operator's switch stops and starts the
exploration; and Plan 509's decision still holds on the two new paths.

**WP4 — the guard.** A structural guard pinning WP1 and WP2, and extending
Plan 509's guard to the two live lookup paths Plan 513 added. Every rule has
an inverted probe, and an absent function fails closed.

**WP5 — the records, the controls, the floor and the closure.**

## 9. Failure, cancellation, restart, and contention semantics

- **A table that names no floodfill** ends the exploratory request with the
  spine's own reason and is counted. It is never redirected to an arbitrary
  peer.
- **A poisoned table** is reported as `Unavailable` — its own reason, distinct
  from "the table held no candidate".
- **A disabled table** cannot happen on this path: the participation owner
  only starts when the table is enabled, so the spine's own gate is the
  table's.
- **A floodfill that does not answer** is handled by the exploration's existing
  deadline and budget (Plan 489/500/512's), unchanged.
- **A cancellation** — the daemon shutting down — stops the tick; the spine
  holds no task of its own, so there is nothing to leak.
- **Contention.** The routing table's driver is behind the consumer's own
  lock; the plan adds no lock that is held across an await.

## 10. Compatibility and migration

**No configuration key is added, and none is removed, and no default moves.**
`netdb.exploration_enabled` becomes real, with the default the tree already
documents.

**One operator-visible behaviour changes, and it is the defect.** An operator
who set `netdb.routing_table_enabled = true` and left `netdb.exploration_enabled`
at its documented default of `false` receives **no** exploratory request after
this plan, where before they received one. That request was aimed at an
arbitrary floodfill — the defect WP1 fixes — and the key that was supposed to
switch it off was never read. D3 sets out the full posture table.

No persisted state changes. The published RouterInfo, `caps` and
`router.version` are untouched. A router built before this plan and one built
after it interoperate: the request is an ordinary I2NP message.

## 11. Required tests

| Test | Where | Asserts |
| --- | --- | --- |
| the exploratory request's target is the **closest** floodfill, not the lowest-hash one | lane | WP1's core claim |
| the target follows the search key's routing key, not the store's iteration order | lane | the selection is keyed |
| a table that names no floodfill **refuses** the request and counts the reason | lane | D1 and D5 |
| the request is never redirected to an arbitrary peer | lane | no silent fallback |
| the operator's switch stops the exploration | lane | WP2 |
| the operator's switch off leaves the RouterInfo-kind resolution live | lane | D4 — Plan 497's M2 stays discharged |
| Plan 509's decision holds on the two live lookup paths Plan 513 added | guard | D6 |
| the guard's rules, each with an inverted probe | guard | the shape is pinned |

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-daemon --test netdb_discovery_live -- --test-threads=1
cargo test --locked -p i2pr-daemon --test netdb_discovery_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test lookup_target_version_boundary -- --test-threads=1
cargo test --locked -p i2pr-daemon --test netdb_exploration_target -- --test-threads=1
cargo test --locked -p i2pr-daemon --test netdb_exploration_target_boundary -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-tooling-inventory.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

The full routine floor from `AGENTS.md` is run before handoff, and its result
is recorded in the closure record.

## 13. Documentation updates

- `specs/QUALIFICATION-GATES.md` — what the pinned references do to choose an
  exploratory request's target, with the i2pd `file:line` citations at the
  exact pin and Java's caveat.
- `specs/CONFORMANCE.md` — the Plan 514 section.
- `specs/support.toml` — the exploratory lookup's surface, and the operator
  switch's own surface if the inventory does not already carry one.
- `docs/architecture/i2pr-daemon.md`, `docs/architecture/i2pr-netdb.md` — the
  selection, the refusal and the switch.
- `plans/subsystems/network-participation-roadmap.md` — Plan 514's row and
  §12.
- `plans/registry.md` — Plan 514's row.

## 14. Acceptance criteria and stop conditions

**Accepted when** the lane proves the closest-floodfill selection, the refusal
and its count, and the operator's switch end to end; the guard's rules hold
with their inverted probes; Plan 509's guard covers the two new paths; the
floor is green; the controls are applied and reverted byte-for-byte; and the
inventory's figures move only because the tree now does the work.

**Stop conditions** — the plan stops and records, rather than continuing:

- if the closest-floodfill selection cannot be shown not to narrow the
  RouterInfo-kind path's selection;
- if the operator's switch cannot be made real without removing a key, which
  would break an operator's file;
- if making the switch real would leave the routing table without a production
  reader, which would reopen Plan 497's **M2**;
- if any control leaves the tree changed, or the floor fails for a reason this
  plan does not own.

## 15. Closure evidence required

The floor's full table; the requirement-to-evidence matrix; the commands run
with their exact results; the invariant, failure, migration and security
reviews; the limitations; the findings by severity; the roadmap disposition;
and the unblock audit — including the **exact** future evidence that would
close the **stock-peer** half.

## 16. Handoff notes

- The subject is the **request**, not the spine: the spine landed; the request
  is aimed wrongly and cannot be turned off.
- The routing key the selection needs is **already computed** and already
  passed to `begin_exploration`; the fix is to use it.
- The no-narrowing rule belongs to the **RouterInfo-kind** path. The
  exploratory request asks **one** peer, and the closest one is the right one.
- Plan 509's decision must reach the two live lookup paths Plan 513 added, or
  it is a decision without teeth.
- The switch's default **does not move**. The tree already says "off by
  default"; the wiring was missing, not the design.
- The next free plan number after this one is **515**.

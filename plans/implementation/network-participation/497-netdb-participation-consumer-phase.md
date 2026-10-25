# Plan 497 — The NetDB participation consumer phase: routing-table-driven lookup selection

| Field | Value |
| --- | --- |
| Plan | 497 |
| Subsystem | `network-participation` (consumer phase for Plans 488 and 489) |
| State | `registered-source-consumer` |
| Classification | capability (a live router selects lookup peers from its own Kademlia routing table) + invariant (the flat store stays the authority; every fallback is counted) |
| Predecessors | Plan 488 (`passed-peer-profile-store`), Plan 489 (`passed-floodfill-indexed-routing-table`), Plan 496 (`passed-ntcp2-private-lan-operator-profile-and-gate-aware-activation`) |
| Recorded open findings this plan discharges | Plan 488 **F3** ("wire `run_profile_maintenance` into the daemon's service graph"), Plan 488 **F4** ("persist the profile projection at startup"), Plan 488 **F1** ("add an `i2pr-netdb` section to `scripts/check-runtime-boundaries.sh`"), Plan 489 §11.2 ("the routing table is not wired into the router's tunnel building") |
| Registry row | `plans/registry.md` |

## 1. Objective

Make the NetDB participation substrate **live**: a daemon that runs the peer-profile
maintenance owner, maintains the floodfill-indexed routing table from its own NetDB,
and asks that table — not the flat peer map — to choose the floodfills it queries.

One bounded outcome: **when the routing table is enabled and holds a verified
floodfill for the key, the router's live `DatabaseLookup` asks that floodfill, and
the router can prove the peer it asked came from the table and not from the flat
store.**

Today the substrate exists and is exercised, but nothing in the daemon calls it.
Plan 489's own closure record says so: *"The routing table is not wired into the
router's tunnel building. By design. The table and the optimized lookup exist and
are exercised; the consumer that would use them to build tunnels is a later
phase."* Plan 488's closure record names the same consumer phase as an **open,
recorded** finding.

## 2. Why ready

- Plan 488 closed the peer-profile store, its failure model, its score, its bounded
  selection policy, its projection format, and the periodic maintenance owner
  `run_profile_maintenance` — and recorded that the maintenance owner **has no
  consumer** (F3) and that the projection **is not written to disk** (F4).
- Plan 489 closed the routing table, the exploration job, the optimized lookup
  plan, and the lookup-engine extension `start_with_candidates` — and recorded
  that the table **is not wired into the router's tunnel building** (§11.2).
- The `[netdb]` configuration keys for the routing table and the exploration job
  already exist and **default to off**.
- The daemon's live lookup path is already a real, counted path:
  `NetDbTunnelManager::begin_tunnel_lookup` → `NetDbSeam::begin_lookup` →
  `LookupEngine::start`, and it selects with
  `i2pr_netdb::lookup_policy::select_floodfill_candidates` over the **flat store**.
- Plan 496 closed the NTCP2 operator profile, so a lab host can now bring both
  transports up on an operator-supplied private LAN. This plan does not need that,
  but a future external lane will.

**The gap is source-side.** It needs no external peer, no stock router and no
public network: the router must select its own lookup targets from its own state.

## 3. Current implementation evidence

- `crates/i2pr-daemon/src/netdb_participation.rs` — `PeerParticipationDriver`,
  `run_profile_maintenance`, `RoutingTableDriver` (`index`, `index_suggested`,
  `begin_lookup`, `advance_time`, `plan`), the receipt vocabulary, and the
  projection format.
- `crates/i2pr-netdb/src/optimized_lookup.rs` — `plan_optimized_lookup`, the
  queryable/pending split, the non-floodfill refusal, and the closest-first order.
- `crates/i2pr-netdb/src/lookup_engine.rs` — `start_with_candidates`, which draws
  the lookup's candidates from an explicit ordered list.
- `crates/i2pr-daemon/src/netdb_tunnels.rs` — `begin_tunnel_lookup`, the live
  lookup start, which currently calls `select_floodfill_candidates` over the flat
  store and then `seam.begin_lookup`.
- `crates/i2pr-netdb/src/lookup_policy.rs` — `select_floodfill_candidates`, the
  flat-store selection this plan supersedes **when the table has an answer**.
- `grep` over the daemon finds **no caller** of `run_profile_maintenance`,
  `PeerParticipationDriver`, `RoutingTableDriver`, or `plan_optimized_lookup`
  outside their own module and their own tests.

## 4. Invariants that must not regress

1. **Default-off.** `netdb.routing_table_enabled` is `false` by default. With it
   off, the daemon's behaviour must be identical to today: the same peers are
   asked, in the same order, from the flat store.
2. **No regression.** The routing table is a **cache of derived state**; the flat
   store is the **authority**. A lookup that would have succeeded must still
   succeed. The table may only ever *improve* the choice, never remove a
   candidate the flat store could serve.
3. **Every fallback is counted by reason.** A table miss that falls back to the
   flat store is a counted outcome with a named reason, not a silent skip. The
   same discipline the peer-eligibility gate uses.
4. **Fail closed.** A table that cannot yield a queryable candidate for the key
   does not fail the lookup; it falls back, and the fallback is counted.
5. **No floor is lowered.** No existing configuration key changes meaning, and
   the one new key is additive, bounded and default-off.
6. **Bounded.** The table's entry and bucket bounds, the maintenance cadence, the
   candidate count and the projection size are all bounded. No unbounded channel
   or queue is introduced.
7. **Explicit ownership and cancellation.** Every spawned task has an owner and a
   cancellation path; a degraded participant degrades its own feature and nothing
   else.
8. **Nothing is published and nothing is advertised.** `specs/support.toml`,
   `Cargo.toml`, `Cargo.lock`, `caps`, the published RouterInfo and
   `router.version` are unchanged. The routing table is internal state.

## 5. Scope

### 5.1 In scope

1. **A daemon-owned participation service.** A service in the daemon's service
   graph that owns the peer-profile driver and the routing-table driver, runs
   `run_profile_maintenance` on a bounded cadence, and populates the routing
   table from the daemon's own NetDB floodfill advertisers.
2. **Routing-table population.** Index each validated floodfill with `Signed`
   evidence; accept a peer's suggestion as `Suggested` evidence; refuse a peer
   whose own record does not advertise `f`; rotate on the UTC day boundary.
3. **Table-driven lookup selection.** The NetDB tunnel-building
   coordinator asks the routing table how it would order the floodfills it
   queries, and uses that order to **reorder** the flat store's selection.
   The candidate *set* is never narrowed: the flat store remains the
   authority, the target stays excluded, and a peer the flat store could
   serve is still served. When the table cannot answer, the coordinator
   uses the flat store's selection exactly as today and counts the
   fallback by reason.
4. **The persisted projection at startup (Plan 488 F4).** Load the bounded,
   sanitized, digest-framed profile projection at startup when it is present and
   current, and discard it otherwise.
5. **The `i2pr-netdb` boundary section (Plan 488 F1).** `scripts/check-runtime-boundaries.sh`
   gains an `i2pr-netdb` section, because the crate is currently unpoliced by
   that script.
6. **The discriminator.** A test that proves the peer the coordinator asked came
   from the routing table and **not** from the flat store — the same
   discriminator Plan 489's mesh lane proved for the driver, applied to the
   tunnel-building coordinator.

### 5.2 Explicitly out of scope

- **Composing the NetDB / destination-tunnel coordinator into the daemon's
  service graph.** That is Plan 433's scope — its work package 2 composes the
  iterative `DatabaseLookup` over the transport manager, and its work package 4
  composes the owned destination tunnel pools. Plan 433 is registered `blocked`
  on Plans 431 and 432, which need an external acquisition contract this host
  does not have. Doing that composition here would be doing blocked work under
  another number, so this plan delivers the **seam** Plan 433's composition will
  use and leaves the composition itself exactly where it is. The consequence is
  stated plainly in §6 and in the closure record: after this plan the routing
  table drives the NetDB tunnel-building coordinator's candidate selection, and
  the coordinator's own placement in the daemon's service graph remains Plan
  433's open work.
- The stock-peer NetDb exploration lane (Plan 489 §11.1). It needs two routers
  that can reach each other: the authorized two-host topology Plan 493 owns.
- Any public or Internet-facing behaviour. The routing table is internal state.
- The `caps=f` advertisement and the floodfill role (Plan 494) — its source side
  is already present; its advertisement stays gated on Plan 493.
- Any change to the I2NP wire format, the RouterInfo format, the selection
  policy's scoring rules, or the peer-eligibility gate.
- IPv6.
- Any change to `specs/support.toml`, the published RouterInfo, or
  `router.version`.

## 6. Required production changes

| Crate / file | Change |
| --- | --- |
| `crates/i2pr-daemon/src/netdb_participation.rs` | A consumer seam: an owned handle that the daemon service drives and the lookup path consults; the fallback counters. |
| `crates/i2pr-daemon/src/netdb_tunnels.rs` | `begin_tunnel_lookup` consults the routing table first and reorders the flat store's selection by it, counting the fallback. |
| `crates/i2pr-daemon/src/lib.rs` | Register the participation service in the service graph; wire the shared handle into the lookup path. |
| `crates/i2pr-daemon/src/config.rs` | One additive, bounded, default-off maintenance-cadence key. |
| `crates/i2pr-netdb/src/lookup_engine.rs` | A **preferred-order** lookup entry point: the same flat-store selection, reordered so the routing table's peers lead. The candidate set is never reduced, and the preference is a priority prefix rather than a closed list, so a peer ingested mid-lookup is still queried. Plan 489's `start_with_candidates` is unchanged. |
| `scripts/check-runtime-boundaries.sh` | The `i2pr-netdb` section (Plan 488 F1). |

No new workspace dependency. `Cargo.toml` and `Cargo.lock` stay byte-identical.

### 6.1 Why a new engine entry point rather than Plan 489's

Plan 489 delivered `start_with_candidates`, which draws a lookup's
candidates from an **explicit closed list** and never consults the flat
store behind the caller's back. That is the correct behaviour for the
Plan 489 exploratory lookup, whose peer set *is* the routing table, and
Plan 489's closure record states it as a closed contract.

The live router lookup is different. It draws its candidates from the flat
store on **every** advance, so a peer ingested while the lookup runs is
queried. Reusing the closed-list entry point on the live path would
freeze the candidate set at lookup start and would silently narrow it to
the table's peers — a regression against invariant 2 below.

The no-regression review during implementation found exactly that, so
Plan 497 adds its own entry point rather than weakening Plan 489's. The
new path keeps the flat store's selection as the authority, reorders it by
the table's preference, and keeps consulting the store on every advance.
Plan 489's entry point and its contract are untouched, and a test pins
the difference.

## 7. Work packages

1. **WP1 — the consumer seam.** A handle the daemon owns that exposes the
   routing table's plan for a key and the counters for every outcome, without
   exposing the table's internals.
2. **WP2 — the daemon service.** Register a service that owns the profile driver
   and the table driver, runs the bounded maintenance cadence, populates the
   table from the NetDB's floodfill advertisers, and stops cleanly on
   cancellation. It degrades its own feature and nothing else.
3. **WP3 — table-driven selection.** The lookup path asks the table first; the
   flat store remains the authority; every fallback is counted by reason.
4. **WP4 — the projection at startup.** Load, verify and discard per Plan 488's
   documented rules.
5. **WP5 — the boundary section.** The `i2pr-netdb` rules in
   `check-runtime-boundaries.sh`, with the positive controls the script's other
   sections use.
6. **WP6 — tests.** The discriminator, the fallback counters, the default-off
   equivalence, the projection round trip, the degradation path.
7. **WP7 — documentation and closure.** The architecture deep-dives, the
   conformance statement, the tooling inventory, the closure record, the
   registry and the roadmap rows.

## 8. Failure, cancellation, restart, and contention semantics

- **Failure.** A malformed projection is discarded, not applied. A table that
  cannot answer falls back to the flat store and counts the fallback. A
  maintenance tick that fails degrades the participation feature; the router's
  lookups continue from the flat store.
- **Cancellation.** The maintenance owner is owned by the service's `ChildScope`
  and stops on its cancellation token; no ownerless task and no unbounded queue.
- **Restart.** The table is rebuilt from the NetDB after a restart, or restored
  from a current projection. A projection from another UTC day is discarded.
- **Contention.** The lookup path and the maintenance owner both touch the
  table. The table is owned by one handle with a lock, and the lookup path holds
  it only long enough to read a plan — never across a socket operation.

## 9. Compatibility and migration

- **No migration.** No stored format changes and no data-directory layout change.
  The projection format is Plan 488's, unchanged.
- **Backward compatible.** With `netdb.routing_table_enabled = false` — the
  default — every existing deployment behaves exactly as it does today.
- **Additive configuration.** One new key inside the existing
  `deny_unknown_fields` `[netdb]` section.

## 10. Required tests

- **The discriminator.** A lookup asks a peer that is in the routing table and
  **not** in the flat store's floodfill set; the router must ask the table's
  peer. A driver that ignored the table would ask the flat store's peer and
  fail the assertion. This is the test that proves the wiring is real.
- **The fallback.** A table that is empty, disabled, or that holds no queryable
  candidate for the key falls back to the flat store, and the fallback is
  counted by reason.
- **Default-off equivalence.** With the table disabled, the selected peer and
  its order are identical to the flat-store selection.
- **The no-regression row.** The candidate *set* is never narrowed by the
  table: the flat store's selection with the table attached holds exactly the
  same peers as without it, and a lookup whose table peer leads still reaches
  the store-only peer afterwards.
- **The mid-lookup row.** A peer ingested while the lookup runs is still
  queried. Plan 489's closed-list entry point does not reach it — that
  difference is pinned by a test so the two contracts cannot drift together.
- **The projection.** A current projection is loaded; a corrupt or stale one is
  discarded and counted.
- **Degradation.** A degraded participant does not take the router down, and the
  lookups still succeed from the flat store.
- **The boundary section.** The new `i2pr-netdb` rules have positive controls
  and are negative-tested.
- The routine floor in `AGENTS.md` passes in full.

## 11. Exact verification commands

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
bash scripts/check-console-boundaries.sh
bash scripts/check-console-browser-security.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-adr-number-uniqueness.py
python3 scripts/check-tooling-inventory.py
python3 scripts/check-license-metadata.py
python3 scripts/check-router-readiness-contract.py
python3 scripts/check-router-readiness-contract.py --self-test
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

Focused, before the floor:

```text
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1
```

## 12. Documentation updates

- `docs/architecture/i2pr-daemon.md` — the participation service and the
  table-driven selection.
- `docs/architecture/i2pr-netdb.md` — the consumer of the optimized plan.
- `specs/CONFORMANCE.md` — the exact claim: the router selects its lookup
  targets from its own routing table when enabled; no capability promotion; no
  advertisement.
- `docs/architecture/tooling.md` — the new `i2pr-netdb` boundary section.
- `AGENTS.md` — only if the floor gains a step.

## 13. Acceptance criteria and stop conditions

### 13.1 Acceptance

- With the routing table enabled and populated, the NetDB tunnel-building
  coordinator asks a floodfill that came from the table, and the test proves it
  did **not** come from the flat store.
- With the routing table disabled — the default — the selected peer and its
  order are identical to today's behaviour.
- Every fallback to the flat store is counted by reason.
- The maintenance owner runs in the daemon's service graph and stops cleanly.
- The projection is loaded when current and discarded otherwise.
- The `i2pr-netdb` boundary section exists, with positive controls.
- The full routine floor passes.
- `git diff --stat -- specs/support.toml Cargo.toml Cargo.lock` is empty.

### 13.2 Stop conditions

- Stop and record a finding if the change would make a lookup fail that
  succeeded before.
- Stop and record a finding if the routing table would be consulted when it is
  disabled.
- Stop and record a finding if any fallback would be silent.
- Stop and record a finding if the table or the maintenance owner would be
  unbounded, or if a task would outlive its owner.
- Stop and record a finding if the change would publish, advertise, or promote a
  capability.
- Stop and record a finding if the change would compose the NetDB / destination
  tunnel coordinator into the daemon's service graph — that is Plan 433's
  blocked scope, and doing it here would be doing blocked work under another
  number.
- Stop and record a finding if the floor cannot be run to completion.

## 14. Closure evidence required

- Implementation commits.
- A requirement-to-evidence matrix mapping every §5.1 requirement to the exact
  test or command that proves it, with the discriminator named explicitly.
- Every command from §11 with its outcome, labelled local or CI truthfully.
- Invariant, failure, migration, and security reviews, including the
  **no-regression** review and the **fallback-counted** review.
- Documentation evidence.
- Known limitations, stated plainly: the stock-peer lane remains Plan 493's;
  the table's evidence is a private controlled mesh, not a public-network
  measurement.
- Unresolved findings by severity.
- The roadmap disposition and the **unblock audit**, which must record the
  disposition of the Plan 488 and Plan 489 findings this plan discharges.

## 15. Handoff notes

- **Do not** modify `specs/support.toml`, `Cargo.toml`, or `Cargo.lock`.
- **Do not** weaken `scripts/check-*` to make a check pass.
- **Do not** promote the support inventory, the published RouterInfo, or
  `router.version`.
- The routing table is **internal** state. Nothing about it is advertised.
- Plan 493's two-family public qualification remains blocked on the authorized
  two-host topology. This plan does not move it.
- The four recorded open findings this plan discharges (Plan 488 F1, F3, F4 and
  Plan 489 §11.2) are closed **only** if the corresponding acceptance row holds.
  A partial delivery leaves its finding open and recorded.

## Status

`passed-netdb-retention-policy`

Plan 507 is closed. The router's peer records no longer **outlive the
router's own acceptance window**, and the NetDB **turns over** instead of
freezing at its quota.

**The gap was a missing caller, not a missing mechanism.** `RouterInfoStore::prune`
(`crates/i2pr-netdb/src/store.rs`) is the tree's own retention operation —
Plan 103 §3.4 calls it *explicit prune over silent eviction* — and it had
exactly **one** caller: its own unit test. No production path pruned.
`RouterInfoStore::insert` **refuses** at the quota with
`InsertOutcome::CapacityExceeded` and **never evicts**, so once a store was
full the router **refused every new peer, permanently**, and kept records
the router's own validator would refuse to accept today. The acceptance rule
and the retention rule disagreed.

**The plan's own ordering wording was corrected against what landed.** The
plan of record said *prune the bootstrap and snapshot under one lock, prune
the destination store, then reconcile*. What shipped is *prune the
bootstrap, prune the destination store, snapshot, insert, learn* — because
Plan 506's reconcile takes its three locks **one at a time and never
nested**, and a prune that held the bootstrap's lock across the snapshot
would have nested them. The correction lives here, in the registry row and
in the roadmap row; **the plan of record is not rewritten**, and no closed
record is rewritten.

**The feasibility question was answered by running code.** Plan 507's first
work package was an executable artefact that could stop the plan, and it
answered **yes**: the same store accepts a new peer after a prune, where it
refused before. The artefact **caught an imprecision in the plan's own
reasoning** — the prune compares each record's own **publication** age, not
its acceptance time, so two peers published at the same instant are both
stale at the same moment.

**`i2pr-netdb` is not changed.** The plan adds a *caller*, not a mechanism.

**Nothing is promoted.** `router.version` stays `0.9.62`; `caps` and the
published RouterInfo are unchanged; the support inventory's advertised count
stays **zero**; `Cargo.toml` and `Cargo.lock` are byte-identical.

## Commits

| Commit | Subject |
| --- | --- |
| `6aeb232f` | docs(plans): register Plan 507, the NetDB retention policy |
| `a1d50e63` | docs(plans): register Plan 507 in the registry and the roadmap |
| `8255d964` | test(daemon): the Plan 507 WP1 feasibility artefact for the NetDB retention |
| `be19929a` | feat(daemon): prune the NetDB on the router's own acceptance window |
| `3eb5f320` | test(daemon): the Plan 507 retention guard and the live retention lane |
| `ff28e433` | docs(plans): the Plan 507 conformance, deep-dive and support entry |
| `<closure>` | docs(plans): close Plan 507 and advance the network-participation roadmap |

## What changed

### The gap

`RouterInfoStore::prune(now, max_age_ms)` had **exactly one caller**: its
own unit test at `crates/i2pr-netdb/src/store.rs`. Both live stores — the
bootstrap NetDB and the destination coordinator's authoritative store —
grew without bound and **froze** at their quota, because
`RouterInfoStore::insert` refuses at the quota and never evicts.

The retention rule also **disagreed** with the acceptance rule. The router's
validation policy **refuses** a RouterInfo published more than
`DEFAULT_MAX_AGE` (24 h) ago, and then **retained** one indefinitely: a
week-old record was still selectable as a tunnel-build peer, a DSM target
and a lookup target, and still counted towards the router's own reported
floodfill count. This is the same shape as the gap Plan 491 found and Plan
503 closed for the DSM paths — **the acceptance rule was enforced; the
retention rule was not.**

### The retention window is the router's own acceptance window

`peer_reconcile::RETENTION_WINDOW_MS` is **derived** from
`i2pr_netdb::ValidationPolicy::default_const().max_age` through a `const
fn`, so the router retains exactly what it accepts and the constant cannot
drift from the policy without a compile error. A second value would be a
second source of truth, which is the defect this plan removes.

There is **no new configuration key**: `config.rs` is untouched, and the
static guard refuses a `retention` or `max_age` key there.

### The prune rides the reconcile's existing cadence

The prune is **folded into** Plan 506's `PeerReconcile::run`, so the prune
and the reconcile can never drift apart in time. The router gains no second
knob, no second task and no second timer. The product's existing tick
drives it; `service_product.rs` needed only a documentation change.

### The order is prune, prune, snapshot, insert, learn

The prune runs **before** the reconcile's snapshot, so a record the prune
removed is **never carried into the destination store**. The prune calls the
tree's own `RouterInfoStore::prune` through a bounded method on `Bootstrap`
and on `DestinationTunnelCoordinator`.

### The locks are one at a time and never nested

The bootstrap's lock is taken and released, then the destination
coordinator's lock is tried and released. The step takes **no await at
all**, so no lock is ever held across one.

The destination store's lock is **tried, never waited for**: a store
another task is using is left alone for that pass, the condition is
**counted** under the stable reason label `prune-unavailable`, and the
reconcile still runs. The store turns over on the next pass.

### Every prune is counted

`pruned-bootstrap`, `pruned-destination` and `prune-unavailable` carry
counts and stable reason labels only — no peer, address, key or payload —
and they ride the **same** inspection seam Plan 506 established, so an
unpublished or poisoned slot stays a **gap**, never a fabricated zero.

### Three things the prune deliberately does not do

- It does not prune the **peer-profile store**, which has its own eviction
  policy and self-corrects through the observation tiers. An age prune there
  would be a second policy for the same store.
- It does not reach the router's **own RouterInfo**, which lives beside the
  store rather than inside it.
- It does not **select**. It removes peer *records*, so Plan 491's version
  gate stays at **selection**.

### The consequence of the policy is recorded, not designed away

Pruning a stale floodfill can leave the router with **no lookup target**.
Protecting floodfills would mean retaining records the router would refuse
to accept — the exact inconsistency this plan removes — so the consequence
is recorded, not designed away.

## Requirement-to-evidence matrix

The guard's rule names are the **landed** names in
`crates/i2pr-daemon/tests/netdb_retention_boundary.rs`.

| Requirement | Evidence |
| --- | --- |
| The retention window is the router's own acceptance window | `crates/i2pr-daemon/src/peer_reconcile.rs`; guard rules `the_retention_window_must_be_the_acceptance_window`, `the_window_rule_fires_when_the_window_is_restated` |
| The prune is the tree's own explicit prune | `crates/i2pr-daemon/src/bootstrap.rs`, `crates/i2pr-daemon/src/destination_tunnels.rs`; guard rules `the_prune_must_be_the_trees_own_explicit_prune`, `the_prune_rule_fires_when_the_tree_prune_is_not_called` |
| The prune runs before the reconcile's snapshot | `crates/i2pr-daemon/src/peer_reconcile.rs`; guard rules `the_prune_must_run_before_the_snapshot`, `the_ordering_rule_fires_when_the_prune_is_dropped` |
| The locks are one at a time and never nested | `crates/i2pr-daemon/src/peer_reconcile.rs`; guard rules `the_prune_locks_must_be_one_at_a_time`, `the_lock_rule_fires_when_a_prune_step_takes_another_lock` |
| Every prune is counted by reason | `crates/i2pr-daemon/src/peer_reconcile.rs`; guard rules `every_prune_must_be_counted`, `the_counter_rule_fires_when_a_count_is_dropped`, `the_counter_label_rule_fires_when_a_label_is_renamed` |
| No new configuration key | `crates/i2pr-daemon/src/config.rs` is untouched; guard rules `the_retention_policy_must_add_no_configuration_key`, `the_configuration_rule_fires_when_a_key_is_added` |
| The router's own RouterInfo is never a prune victim | `crates/i2pr-daemon/src/bootstrap.rs`; guard rules `the_local_router_info_must_never_be_a_prune_victim`, `the_local_record_rule_fires_when_the_prune_reaches_the_local_record` |
| The prune never selects; Plan 491's gate stays at selection | `crates/i2pr-daemon/src/netdb_participation.rs`; guard rules `the_prune_must_never_select`, `the_selection_rule_fires_when_the_prune_selects` |
| Plan 506's reconcile invariants survive | `crates/i2pr-daemon/src/peer_reconcile.rs`; guard rules `the_reconcile_invariants_must_survive`, `the_invariant_rule_fires_when_a_reconcile_step_is_dropped` |
| The product drives the prune on its existing tick | `crates/i2pr-daemon/src/service_product.rs`; guard rules `the_product_must_drive_the_prune_on_its_existing_tick`, `the_tick_rule_fires_when_the_reconcile_owns_a_task` |
| An absent function fails closed | guard rule `an_absent_function_fails_closed` |
| The rules are structural, not whole-file searches | guard rule `the_rules_are_structural_and_not_whole_file_searches` |
| The feasibility question is answered by running code | `crates/i2pr-daemon/tests/netdb_retention_feasibility.rs` — 9 tests |
| The product prunes both live stores | `crates/i2pr-daemon/tests/netdb_retention_live.rs` — `the_product_prunes_the_bootstrap_and_the_destination_store` |
| A store at its quota refuses before the prune and accepts after | `crates/i2pr-daemon/tests/netdb_retention_live.rs` — `a_store_at_its_quota_refuses_a_new_peer_before_the_prune_and_accepts_it_after` |
| A record inside the window survives | `crates/i2pr-daemon/tests/netdb_retention_live.rs` — `a_record_inside_the_window_survives_the_prune` |
| The router's own RouterInfo is never a victim | `crates/i2pr-daemon/tests/netdb_retention_live.rs` — `the_router_own_router_info_is_never_a_prune_victim` |
| The prune runs before the snapshot | `crates/i2pr-daemon/tests/netdb_retention_live.rs` — `the_prune_runs_before_the_snapshot` |
| A prune that cannot run is counted, never silent | `crates/i2pr-daemon/tests/netdb_retention_live.rs` — `a_poisoned_bootstrap_lock_is_counted_and_never_silent` |
| The prune counters reach the public inspection seam | `crates/i2pr-daemon/tests/netdb_retention_live.rs` — `the_prune_counters_reach_the_public_inspection_seam` |
| The prune is deterministic for a fixed `now` | `crates/i2pr-daemon/tests/netdb_retention_live.rs` — `the_prune_is_deterministic_for_a_fixed_now` |
| The controls are not vacuous | the four negative controls, verified on the real tree |
| Nothing is advertised | `specs/support.toml` — 99 surfaces, 0 advertised |
| No new dependency | `git diff --exit-code <base> HEAD -- Cargo.toml Cargo.lock` |

## Commands run

### The routine floor

<!-- FLOOR-TABLE -->

### The focused verification

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked -p i2pr-daemon --all-targets` | PASS |
| `cargo clippy --locked -p i2pr-daemon --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked -p i2pr-daemon --lib -- --test-threads=1` | PASS — 966 passed, 3 ignored |
| `cargo test --locked -p i2pr-daemon --test netdb_retention_feasibility --test netdb_retention_boundary --test netdb_retention_live -- --test-threads=1` | PASS — 9 + 26 + 8 |
| `cargo test --locked -p i2pr-daemon --test peer_reconcile_boundary --test peer_reconcile_live -- --test-threads=1` | PASS — 28 + 7 (Plan 506 unchanged) |
| `cargo test --locked -p i2pr-daemon --test exploratory_driver_boundary --test exploratory_driver_live --test tunnel_liveness_boundary --test tunnel_liveness_live -- --test-threads=1` | PASS — 15 + 8 + 26 + 9 |
| `cargo test --locked -p i2pr-daemon --test version_floor_boundary --test version_eligibility_live --test version_eligibility_mesh -- --test-threads=1` | PASS — 28 + 11 + 51 |
| `cargo test --locked -p i2pr-daemon --test exploratory_build_live --test destination_tunnel_unit --test netdb_tunnel_live -- --test-threads=1` | PASS — 11 + 9 + 4 |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 51 tests |

### The four negative controls, verified on the real tree

Each control was applied to the **real** production source, run against the
**real** lanes, and reverted; the file was restored **byte-for-byte** after
each, and the restoration was asserted.

| Control | Mutation | Failing tests | Named failures |
| --- | --- | --- | --- |
| A | the prune call is removed from `fn run` | 9 | `the_product_prunes_the_bootstrap_and_the_destination_store`, `the_prune_runs_before_the_snapshot`, `the_prune_must_run_before_the_snapshot`, `the_landed_tree_holds_every_rule`, … |
| B | the prune call moves after the reconcile's snapshot | 7 | `the_prune_runs_before_the_snapshot`, `the_product_prunes_the_bootstrap_and_the_destination_store`, `a_store_at_its_quota_refuses_a_new_peer_before_the_prune_and_accepts_it_after`, … |
| C | the retention window collapses to zero | 7 | `a_record_inside_the_window_survives_the_prune`, `the_retention_window_must_be_the_acceptance_window`, `the_product_prunes_the_bootstrap_and_the_destination_store`, … |
| D | the destination-store prune count is dropped | 5 | `every_prune_must_be_counted`, `the_product_prunes_the_bootstrap_and_the_destination_store`, `a_store_at_its_quota_refuses_a_new_peer_before_the_prune_and_accepts_it_after`, … |

The WP1 feasibility artefact carries **two more** controls of its own,
recorded in its module comment: a prune that removes everything regardless
of the window, and a prune that removes the record without releasing its
bytes. The first failed five of the artefact's nine tests, the second
failed two.

### Three weaknesses found while building the controls

They are recorded because each one **would have made a control pass
vacuously**.

1. **The counter probe renamed the token instead of removing it.** The rule
   asserts that `fn prune_stores` *contains* `pruned_destination`, and the
   probe replaced the field's first occurrence with
   `pruned_destination_removed` — a string that still **contains** the
   token. The probe passed while the rule never fired. Fixed: the probe now
   removes the token outright, and it asserts the removal.
2. **The tick probe added the wrong forbidden construct.** The rule forbids
   `tokio::spawn`, and the probe inserted a function named `spawn_task` —
   which the rule does not forbid. The probe passed while the rule never
   fired. Fixed: the probe now inserts `tokio::spawn` and asserts it did.
3. **The live lane's clock was derived from the window.** The lane computed
   its prune clock as `NOW_MS + RETENTION_WINDOW_MS + 3 600 000` and
   published its surviving record *at that clock*, so the record's age was
   always zero and a **collapsed window was invisible**: control C failed
   only the guard's rules, never the live lane. Fixed: the surviving record
   is now published **half a window before** the prune clock, so a
   collapsed window takes it with it, and control C now fails
   `a_record_inside_the_window_survives_the_prune`.

## Invariant review

| Invariant | How it is held |
| --- | --- |
| No unbounded collection | the prune adds no collection; it removes from two stores the router already owns |
| No unbounded queue or channel | none added |
| Every task has explicit ownership and cancellation | **no task was added** — the prune is a step inside the reconcile, on the existing tick |
| No listener, no socket, no timer | none added; asserted by the guard |
| No lock is held across an await | the prune step takes **no await at all** |
| The locks are taken one at a time and never nested | asserted structurally by the guard; the bootstrap's lock is released before the destination's is tried |
| Plan 491's version gate is not weakened | the prune removes records, it never selects; asserted by the guard |
| A refusal is counted, never silently skipped | every outcome has a reason label and a counter, including a prune that could not run |
| No dependency direction change | `scripts/check-dependency-direction.sh` passes; no new dependency |
| No capability advertisement | `specs/support.toml` — 99 surfaces, **0 advertised** |
| No production wire change | none — the prune touches the router's internal stores only |
| `i2pr-netdb` is unchanged | the plan adds a *caller*, not a mechanism; the guard pins the NetDB store's own prune signature |

## Failure review

| Failure | Behaviour |
| --- | --- |
| The bootstrap's lock is poisoned | the prune does not run; counted as `prune-unavailable`; the store keeps its contents; the reconcile's snapshot fails the same way and is counted as `source-unavailable` |
| The destination store's lock is held by another task | the prune does not run for that pass; counted as `prune-unavailable`; the reconcile still runs; the store turns over on the next pass |
| The bootstrap handle is absent | there is no store to prune; nothing is counted as a failure, and the reconcile's own `source-unavailable` already reports the gap |
| A record is outside the window | removed; counted; its bytes are released |
| A record is inside the window | kept, always |
| The local RouterInfo | never in the store; never a victim |
| The store is below its quota | the prune still runs and removes the stale records; the quota is not the trigger |
| The store is at its quota | the prune makes room **before** the reconcile's writes, so a new peer is accepted rather than refused |
| A concurrent ingest | the prune and the ingest both take the bootstrap's lock; one wins, the other waits; neither corrupts the accounting |
| The process restarts | the prune is a step, so there is nothing to restore; the stores are rebuilt from the network and the reseed path |
| The tick does not run | the prune does not run; the store freezes; the condition is visible in the counters, never silent |

**Nothing fails open.** Every refusal has a reason label and a counter, and
no path silently skips a peer or reports a prune that did not happen.

## Migration review

There is **no migration**. The prune adds no stored state, changes no
on-disk format, and renames no configuration key:

- `config.rs` is **untouched**; `[netdb] participation_maintenance_period_ms`
  keeps its meaning and its bounds;
- no file under the router's data directory changes shape;
- the two stores' existing records are **not** rewritten: the prune removes
  records the router would refuse to accept and touches nothing else;
- a router that upgrades simply finds that its NetDB now **turns over**
  instead of freezing, and that a peer it accepted more than 24 hours ago
  is no longer retained;
- the router's own RouterInfo is unaffected.

## Security review

- **No new secret, key or token** is introduced, derived or persisted. The
  prune removes RouterInfo records the router already holds.
- **No new identifier crosses a boundary.** The prune is entirely internal.
- **The counters carry no peer identity, address, key or payload.** They are
  counts and stable reason labels, and the labels are a closed set the
  router defines.
- **The prune is bounded.** It walks two bounded stores and removes at most
  their contents; the work is proportional to the store, not to the
  network.
- **The prune writes nothing to the network.** No I2NP message, no lookup,
  no lease, no publication.
- **No new configuration key** means no new operator-controlled surface
  that could be mistaken for an advertisement.
- **Nothing is advertised.** `router.version` stays `0.9.62`; `caps` and the
  published RouterInfo are unchanged; the support inventory's advertised
  count stays **zero**.
- **The retention window is a shortening, not an extension.** A peer the
  router would refuse to accept is removed sooner, never later. No record
  is retained longer than the acceptance rule allows.

## Documentation and operations

- `specs/support.toml` — one `experimental` surface,
  `netdb.retention-policy`, with `advertised = false`. The inventory moves
  from **98 surfaces (92 experimental, 5 deferred, 1 ready)** to **99
  surfaces (93 experimental, 5 deferred, 1 ready)**; **0 advertised**.
- `specs/CONFORMANCE.md` — the Plan 507 section, with the explicit statement
  that **no stock-peer evidence** exists and that the floodfill-exhaustion
  consequence is recorded rather than designed away.
- `docs/architecture/i2pr-daemon.md` — the `src/peer_reconcile.rs` module
  row (with `RETENTION_WINDOW_MS` added to its symbol list), the rows for
  the three other files Plan 507 changed, the Plan 507 section, and the
  test-file counts (106 → 108 files; 67 → 69 named).
- `plans/registry.md` and `plans/subsystems/network-participation-roadmap.md`
  — the Plan 507 rows, updated to `closed`.

**Operations.** The prune is observable through the inspection seam's
`peer_reconcile_counters()` and `peer_reconcile_counters_snapshot()`. The
reason labels an operator will care about are `pruned-bootstrap`,
`pruned-destination` and `prune-unavailable`. A rising
`prune-unavailable` count with a flat `pruned-destination` count would mean
the destination store's lock is contended on every pass; a rising
`pruned-bootstrap` count means the NetDB is turning over.

## Limitations

- **The destination store's quota values are unchanged** (4 096 records and
  4 MiB by default). The plan changes what happens *at* the quota, not the
  quota itself.
- **Pruning a stale floodfill can leave the router with no lookup target.**
  Protecting floodfills would mean retaining records the router would
  refuse to accept, so the consequence is recorded rather than designed
  away.
- **The peer-profile store is not pruned.** It has its own eviction policy
  and self-corrects through the observation tiers.
- **The prune compares a record's own publication age, not its acceptance
  time.** Two peers published at the same instant are stale at the same
  moment, whatever time the router accepted them. This was found by the
  WP1 artefact, not assumed.
- **A record published in the future is never pruned.** The age is
  computed with saturating subtraction, so a future publication reads as
  age zero.
- **The counters are not persisted.** A restart empties them; the stores'
  own contents persist.
- **The router still asks a below-floor floodfill for a lookup.** Plan 503's
  lookup-target API-version gate is a **product decision** with no protocol
  basis — the specification's table constrains neither a lookup target nor
  a DSM target beyond the gates already in place — and it stays open.
- **No stock-peer evidence.** Whether an ordinary public router's peer set
  survives a 24-hour retention window is **unproven**. The external lanes
  stay open until Plan 493's authorized topology is available.

## Findings by severity

### Medium

- **The daemon deep-dive's module-table line counts are stale for 40 of the
  63 rows that carry one** (measured by comparing every row against the
  file). Plan 507 updated the **four** rows it changed and left the rest:
  the drift is a separate concern with its own plan-of-record, and
  correcting thirty-six unrelated rows here would bury the change. **Not
  fixed.**
- **The registry's prose surface tally was stale by one plan.** It read
  "of 97 surface entries … (91 `experimental`, 5 `deferred`, 1 `ready")" and
  was last touched by Plan 505's commit — Plan 506 added a surface and did
  not bump it. Plan 507 corrected it to **99 surfaces (93 experimental, 5
  deferred, 1 ready)**. **Fixed here; the mechanism that let it drift is
  not.**

### Low

- **Two of the guard's four new probes were initially wrong** in a way that
  would have made them pass vacuously, and one live-lane assertion was
  blind to a collapsed window. All three are recorded above and fixed; the
  live lane's blind spot was found by a negative control, not by review.
- **The prune's `try_lock` is a deferral, not a wait.** A pass that could
  not take the destination store's lock leaves the stale records in place
  for that pass. The condition is counted, and the store turns over on the
  next pass, so the effect is bounded by the cadence.

### Recorded, not defects

- **The plan of record's ordering wording was wrong** and is corrected
  forward, in this record and in the two rows. The plan of record is not
  rewritten, and no closed record is rewritten.
- **The floodfill-exhaustion consequence** of the policy is a recorded
  consequence of removing the inconsistency, not a defect in the removal.

## Roadmap disposition

`plans/subsystems/network-participation-roadmap.md` — the Plan 507 row
moves from `registered` to `closed`, with the closure record and the
commits, and §12 records the disposition.

The roadmap's next open item on this line is the **peer-knowledge**
remainder: Plan 497 limitation 5's read half, the RouterInfo-kind lookup,
and Plan 503's lookup-target version gate. None of them is reachable from
the source side alone, and none is attempted here.

## Unblock audit

Plan 507 unblocks **nothing that was blocked**, and it is honest about that.
It discharges one recorded limitation of a closed plan (Plan 506's *the
destination store refuses at capacity and never evicts*) and it closes a
gap that no plan had recorded.

Every item it leaves open is listed in the table above with the reason it
stays open, and each of those reasons is **external or a separate
plan-of-record** — not a piece of this plan that was quietly skipped:

- the external-environment blocks (Plans 433, 435, 436, 437, 438, 439, 493,
  494) need an authorized two-host topology this repository does not have;
- the stock-peer questions need that same topology;
- Plan 503's lookup-target gate is a **product decision** with no protocol
  basis, and inventing a floor would shrink the router's reach without
  cause;
- the destination store's quota values, the floodfill-exhaustion
  consequence, and the peer-profile store's age policy each change what the
  router retains or offers, and each therefore needs its own
  plan-of-record.

The plan's own stop conditions were all held: no wire-format change, no new
dependency, no change to `i2pr-netdb`, no weakening of Plan 491's gate, no
second configuration key, no second task or timer, no two locks held at
once, no lock held across an await, and no change to the destination
store's quota values.

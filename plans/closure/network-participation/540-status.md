# Plan 540 — the NetDB's prune must not walk the population

**Closure record.** The plan of record is
`plans/implementation/network-participation/540-the-netdbs-prune-must-not-walk-the-population.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 540 is **closed**. The validated store's prune finds the stale records
through a **maintained age index** instead of a walk over every retained
record, so the router's NetDB lock is no longer held across an O(population)
walk on the router's live maintenance cadence. The prune's **semantics** are
unchanged: it still removes every record past the window, in one call, and the
index changes how the stale records are *found*, never *which* records are
removed.

Classification: **corrective** (the daemon's peer reconcile called the
store's prune **under the router's NetDB lock** — the bootstrap's mutex, which
*is* the router's NetDB lock — and the prune found the stale records by
walking **every** retained record, so the lock was held across an O(population)
walk on the router's live maintenance cadence). **Subsystem:**
network-participation. **Closure date:** 2026-11-11.

Nothing is promoted: `router.version` stays `0.9.62`, the inventory's
advertised count stays **zero**, and `Cargo.toml` and `Cargo.lock` stay
byte-identical.

## What the plan closed

**The store maintains an age index.** `RouterInfoStore`
(`crates/i2pr-netdb/src/store.rs:83`) carries a **private** field
`ages: BTreeSet<(u64, RouterHash)>`
(`crates/i2pr-netdb/src/store.rs:98`) beside its record map
(`crates/i2pr-netdb/src/store.rs:84`). The field is private, so the store's
public surface is unchanged.

**The index is maintained on exactly the paths that maintain the record map.**
The insert path is at `crates/i2pr-netdb/src/store.rs:141` and the removal path
at `crates/i2pr-netdb/src/store.rs:229`:

- the **new-key** path adds the incoming record's entry
  (`crates/i2pr-netdb/src/store.rs:207`);
- the **replacement** path releases the existing record's entry
  (`crates/i2pr-netdb/src/store.rs:183`) and adds the incoming one
  (`crates/i2pr-netdb/src/store.rs:184`), because a replacement may carry a
  different publication time;
- the **removal** path releases the removed record's entry
  (`crates/i2pr-netdb/src/store.rs:236`).

The refusal paths — the stale replacement, the idempotent insert, the conflict
and the capacity refusal — change nothing, so they change no entry. The index
is **never rebuilt by a scan**: no path that maintains it walks the record map.

**The prune walks the index.** The prune is at
`crates/i2pr-netdb/src/store.rs:314`. It computes the threshold at
`crates/i2pr-netdb/src/store.rs:325` and walks the index's ordered range
(`crates/i2pr-netdb/src/store.rs:328`) to its stale prefix
(`crates/i2pr-netdb/src/store.rs:329`). The index is ordered by publication
time first, so the stale records are a **contiguous prefix** of it and the
walk is proportional to what the prune removes, not to the population it does
not remove.

**The prune's semantics are unchanged.** It still removes every record past
the window, in one call. The prune's **signature** is unchanged, and Plan 507's
pinned call site and signature stay exactly as they are: the bootstrap's own
prune wrapper is at `crates/i2pr-daemon/src/bootstrap.rs:291` and still calls
`self.store.prune(Date::from_millis(now_ms), window_ms)`
(`crates/i2pr-daemon/src/bootstrap.rs:292`) under the bootstrap's lock, and the
reconcile's shape — prune, then snapshot, then learn — is unchanged
(`crates/i2pr-daemon/src/peer_reconcile.rs:417`,
`crates/i2pr-daemon/src/peer_reconcile.rs:424`). No predecessor's guard is
extended.

**The accounting survives the prune.** The prune goes through the store's own
`remove`, so the record's bytes and Plan 538's floodfill advertiser count are
released with the record. `stats()` stays O(1) and walks no record map
(`crates/i2pr-netdb/src/store.rs:293`), and the store's selection path is
unchanged (`crates/i2pr-netdb/src/store.rs:343`).

## What the plan did not close

- **The router's validated store is still the bootstrap's store.** The bootstrap
  owns the only `RouterInfoStore` in the daemon
  (`crates/i2pr-daemon/src/bootstrap.rs:215`). Separating the router's NetDB
  from the bootstrap's lifetime is a larger change and was not attempted here.
  This is Plan 536's recorded **Low** finding 1, carried forward unchanged.
  **Owner:** the successor that gives the router a NetDB owner of its own. This
  plan **reduces** what that successor must do: once the prune's cost is
  proportional to what it removes, the NetDB lock is no longer held across a
  population walk, so separating the ownership is no longer the thing that
  makes the lock dangerous.
- **The store's selection paths are still scans.** `floodfill_advertisers()`
  (`crates/i2pr-netdb/src/store.rs:343`) yields exactly the advertisers, and a
  selection over the floodfill advertisers is a walk by its nature.
- **The composition root's shape is proven statically, not behaviourally.** The
  prune's call site is private to the daemon's own module, so the lane cannot
  see it and the **shape** — that the prune walks the index and not the record
  map — is the guard's claim. **Owner:** the successor that makes the
  composition root's relay arm reachable from a test.
- **The production SSU2 peer-test driver** — Plan 530's Low finding 2.
- **The `B` advertisement and `caps=f`** — their own owners.
- **The stock-peer half** — Plan 493's authorized two-host topology.

## Implementation commits

| Commit | What it carries |
| --- | --- |
| `b6cb3817` | the plan of record, the registry's row and the roadmap's row |
| `360b59dd` | the production change: the store's age index, its three maintenance paths and the prune's index walk |
| `9a1c515a` | the lane and the guard |
| `a12e6199` | the documentation, the inventory, the plan's amendment log and the citation-drift pointer table |

The floor ran on the committed tree `a12e6199`. The closure commit adds **only**
this record and the two closed rows, exactly as Plan 538's and Plan 539's closure
commits did.

The plan of record's **invariants** are at
`plans/implementation/network-participation/540-the-netdbs-prune-must-not-walk-the-population.md:115`,
its **required production changes** at
`plans/implementation/network-participation/540-the-netdbs-prune-must-not-walk-the-population.md:195`,
its **design decisions** at
`plans/implementation/network-participation/540-the-netdbs-prune-must-not-walk-the-population.md:229`,
its **lane** at
`plans/implementation/network-participation/540-the-netdbs-prune-must-not-walk-the-population.md:329`,
its **guard** at
`plans/implementation/network-participation/540-the-netdbs-prune-must-not-walk-the-population.md:356`,
its **closure evidence** at
`plans/implementation/network-participation/540-the-netdbs-prune-must-not-walk-the-population.md:433`,
its **amendment log** at
`plans/implementation/network-participation/540-the-netdbs-prune-must-not-walk-the-population.md:492`,
the netdb deep-dive's account of the age index at
`docs/architecture/i2pr-netdb.md:1509`, the daemon deep-dive's account of the
prune's call site at `docs/architecture/i2pr-daemon.md:4501`, the qualification
gates at `specs/QUALIFICATION-GATES.md:2194` and the conformance account at
`specs/CONFORMANCE.md:4607`.

## Requirement-to-evidence matrix

| # | Requirement | Evidence |
| --- | --- | --- |
| 1 | the prune removes every record past the window, in one call | the lane's rows `the_prune_removes_exactly_the_stale_set`, `the_prune_removes_every_stale_record_in_one_call`, `the_prune_removes_nothing_when_nothing_is_stale`, `a_record_published_in_the_future_is_never_stale` and `a_now_below_the_window_removes_nothing` |
| 2 | the prune's cost is proportional to what it removes, not to the population | the guard's rule `the_prune_walks_the_index_and_not_the_record_map`, and the production change at `crates/i2pr-netdb/src/store.rs:328` |
| 3 | the age index is maintained, not derived | the guard's rule `the_store_maintains_an_age_index`; the production change at `crates/i2pr-netdb/src/store.rs:207`, `crates/i2pr-netdb/src/store.rs:184` and `crates/i2pr-netdb/src/store.rs:236` |
| 4 | the index is exact | the lane's rows `the_index_does_not_drift_on_an_insert` and `a_replacement_is_removed_at_its_new_time`; the guard's rule `the_store_maintains_an_age_index` |
| 5 | the prune's public surface is unchanged | the guard's rule `the_prune_signature_and_the_surfaces_are_unchanged`; the production change at `crates/i2pr-netdb/src/store.rs:314` |
| 6 | the store's other public surfaces are unchanged | the guard's rule `the_prune_signature_and_the_surfaces_are_unchanged`; the lane's rows `the_statistics_and_the_selection_survive_the_prune` and `the_prune_is_deterministic` |
| 7 | the byte accounting and the floodfill advertiser count are unchanged | the lane's row `the_prune_releases_the_bytes_and_the_advertiser` |
| 8 | the router's own RouterInfo is never a prune victim | the production change at `crates/i2pr-netdb/src/store.rs:83` — the store's own contents are unchanged |
| 9 | the daemon's prune call site and the reconcile's shape are unchanged | the guard's rule `the_call_site_and_the_reconcile_are_unchanged`; the production change at `crates/i2pr-daemon/src/bootstrap.rs:292` |
| 10 | no configuration key is added, and no dependency is added | the guard's rule `no_new_key_and_nothing_is_advertised`, and the two digests below |
| 11 | no new task, timer, listener or socket | the guard's rule `no_new_key_and_nothing_is_advertised` |
| 12 | no advertisement | the guard's rule `no_new_key_and_nothing_is_advertised`; `crates/i2pr-netdb/src/local.rs:172` |
| 13 | Plan 507's, Plan 538's and Plan 539's lanes and guards stay byte-identical | the guard's rule `the_predecessors_evidence_is_untouched`, and the six digests below |

The six digests the guard pins are Plan 507's lane
(`709ec1b47e731bd71edb7ca65f4ef889e4695f36cf5a012cf7d70b5ceae2c5da`) and guard
(`5ef83044dcb8036e0ac2ccc8e7e9cb9bf6b7a21127b822b6e634da6170fdd78d`), Plan
538's lane
(`d1ce001d1e7562ca84b33cd9b4b0bca04325565b7cfef2c7504645825c8bc613`) and guard
(`3d059e0690a0ab52b5c54619c0b5220eac8f340168187c2541f12f597c518fa4`), and Plan
539's lane
(`068410ef125b2122b1acf56866abff78010f6fe20dc4ef933d80a7c4e6d99103`) and guard
(`60ef63b653835cbb1af7943aea64662fe6970eed5827b1266a92994dea2ae300`). A later
change to any of them must change the pinned constant **and** say why.

The manifest and the lock are byte-identical to the tree Plan 539 closed on, and
the digests the record carries are the manifest's
(`39c44e33add62693eef8866220de17dcfe137437b62732e5044fcaefc4870269`) and the
lock's
(`f0cec0685c4ed460c7d63273cc318f7a683dc2357735d7edaa6dfa1ac243d833`).

## Focused verification

| Command | Outcome |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked -p i2pr-daemon --test netdb_prune_walk -- --test-threads=1` | PASS — 10 rows |
| `cargo test --locked -p i2pr-daemon --test netdb_prune_walk_boundary -- --test-threads=1` | PASS — 13 rows |
| `cargo test --locked -p i2pr-daemon --lib -- --test-threads=1` | PASS — 1001 rows |
| `cargo test --locked -p i2pr-netdb --all-targets` | PASS |
| `cargo test --locked -p i2pr-daemon --test netdb_retention_feasibility --test netdb_retention_live --test netdb_retention_boundary --test floodfill_statistic_accounting --test floodfill_statistic_accounting_boundary --test routing_table_population_lock --test routing_table_population_lock_boundary -- --test-threads=1` | PASS — 9 rows, 1 row, 26 rows, 14 rows, 32 rows, 10 rows and 24 rows |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 scripts/check-license-metadata.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 51 tests |
| `python3 /tmp/540-negative.py` | PASS — 12 controls, each firing the surface that owns the defect, each restored byte-identically |

The closure record's **citations** are verified mechanically, not by reading.
The verifier recomputes every cited line from the tree and requires the record
to name exactly that line:

- the **closure record's** citations are recomputed against the **current
  tree** — the first line that carries the cited text is the line the record
  must name;
- the **plan of record's** section 3 citations are recomputed against the
  **registration commit** `b6cb3817`, because section 3 is the evidence of the
  tree *before* the plan and is never rewritten;
- the **lane's and the guard's rows** are checked by name, so the record
  cannot claim a row the evidence file does not carry;
- the **manifest's and the lock's digests** are checked, so the record cannot
  claim byte-identity the tree does not have.

The section 3 check is **derived** from the plan's own citations rather than
curated from a list: every `path:line` the plan's section 3 carries must
resolve, at the registration commit, to a line that really exists there. A
curated list of citations the plan does not make would be an assertion about
the plan rather than a check of it.

## The routine floor

The floor ran on the committed tree `a12e6199` and it is **green**. Every
step is recorded in `/tmp/540-floor.log`, with each step's own log under
`/tmp/540f-<step>.log`. The floor is a plain sequence of `step` runs that
records each step's exit status; no step is skipped, no `|| true`, no
`continue-on-error` and no filename filtering. The workspace step runs with
`--no-fail-fast`, so every test binary's result is reported rather than the
run stopping at the first failure.

Plan 540's floor is Plan 539's floor plus two steps -- the daemon lane and the
daemon guard for the NetDB's prune. **No other step changed**, and **no other
plan's guard file is touched**: Plan 540 extends no predecessor's guard, and
Plan 507's, Plan 536's, Plan 537's, Plan 538's and Plan 539's lanes and guards
stay byte-identical.

| Step | Outcome |
| --- | --- |
| `floor-tree-clean` | PASS |
| `fmt` | PASS |
| `check` | PASS |
| `app-siblings` | PASS |
| `app-fixture-static` | PASS |
| `app-fixture-copy` | PASS |
| `app-fixture-rebuild` | PASS |
| `plan522-peer-pool-lane` | PASS |
| `plan522-peer-pool-boundary` | PASS |
| `plan522-peer-pool-unit` | PASS |
| `transport-ssu2-unit` | PASS |
| `runtime-unit` | PASS |
| `netdb-unit` | PASS |
| `plan523-introducer-lane` | PASS |
| `plan523-introducer-boundary` | PASS |
| `plan524-introducer-request-lane` | PASS |
| `plan524-introducer-request-boundary` | PASS |
| `plan520-publication-lane` | PASS |
| `plan520-publication-boundary` | PASS |
| `plan525-introducer-publication-lane` | PASS |
| `plan525-introducer-publication-boundary` | PASS |
| `plan526-introducer-advertisement-lane` | PASS |
| `plan526-introducer-advertisement-boundary` | PASS |
| `plan521-peer-test-role-boundary` | PASS |
| `plan527-reachability-contradiction-lane` | PASS |
| `plan527-reachability-contradiction-boundary` | PASS |
| `plan528-peer-test-verdict-lane` | PASS |
| `plan528-peer-test-verdict-boundary` | PASS |
| `plan529-observed-address-lane` | PASS |
| `plan529-observed-address-boundary` | PASS |
| `plan530-explicit-bind-lane` | PASS |
| `plan530-explicit-bind-boundary` | PASS |
| `plan531-relay-requester-lane` | PASS |
| `plan531-relay-requester-boundary` | PASS |
| `plan532-relay-target-lane` | PASS |
| `plan532-relay-target-boundary` | PASS |
| `plan533-relay-establishment-lane` | PASS |
| `plan533-relay-establishment-boundary` | PASS |
| `plan534-relay-session-peer-lane` | PASS |
| `plan534-relay-session-peer-boundary` | PASS |
| `plan535-explicit-bind-refresh-lane` | PASS |
| `plan535-explicit-bind-refresh-boundary` | PASS |
| `plan536-relay-target-lock-lane` | PASS |
| `plan536-relay-target-lock-boundary` | PASS |
| `plan537-relay-requester-lock-lane` | PASS |
| `plan537-relay-requester-lock-boundary` | PASS |
| `plan538-floodfill-statistic-lane` | PASS |
| `plan538-floodfill-statistic-boundary` | PASS |
| `plan539-routing-table-population-lane` | PASS |
| `plan539-routing-table-population-boundary` | PASS |
| `plan540-netdb-prune-lane` | PASS |
| `plan540-netdb-prune-boundary` | PASS |
| `test-workspace` | PASS |
| `clippy` | PASS |
| `doc` | PASS |
| `test-doc` | PASS |
| `dependency-direction` | PASS |
| `plan-number-uniqueness` | PASS |
| `adr-number-uniqueness` | PASS |
| `portable-service-tunnel-api` | PASS |
| `portable-service-tunnel-consumer` | PASS |
| `planning-tests` | PASS |
| `runtime-boundaries` | PASS |
| `console-boundaries` | PASS |
| `console-browser-security` | PASS |
| `tooling-inventory` | PASS |
| `license-metadata` | PASS |
| `managed-app-private-client-seams` | PASS |
| `router-readiness-contract` | PASS |
| `router-readiness-self-test` | PASS |
| `core-router-recovery-contract` | PASS |
| `core-router-recovery-self-test` | PASS |
| `plan449-ssu2-preflight` | PASS |
| `plan495-peer-eligibility` | PASS |
| `plan476-gate-binding` | PASS |
| `plan476-gate-binding-self-test` | PASS |
| `managed-app-process-boundary` | PASS |
| `managed-app-process-boundary-self-test` | PASS |
| `managed-app-package-boundary` | PASS |
| `managed-app-package-boundary-self-test` | PASS |
| `managed-app-policy-boundary` | PASS |
| `managed-app-policy-boundary-self-test` | PASS |
| `managed-app-secured-sandbox` | PASS |
| `managed-app-secured-sandbox-self-test` | PASS |
| `service-tunnel-boundaries` | PASS |
| `managed-app-gateway-boundary` | PASS |
| `managed-app-manager-boundary` | PASS |
| `m11-per-epoch-composition` | PASS |
| `service-anonymity-boundaries` | PASS |
| `fixture-manifest` | PASS |
| `ntcp2-vectors` | PASS |
| `ssu2-vectors` | PASS |
| `i2cp-vectors` | PASS |
| `ntcp2-interoperability` | PASS |
| `constrained-host-lane-boundary` | PASS |
| `m11-transit-boundaries` | PASS |
| `m11-transit-qualification-evidence` | PASS |
| `sam-acceptance-evidence` | PASS |
| `ssu2-acceptance-evidence` | PASS |
| `i2cp-acceptance-evidence` | PASS |
| `i2pcontrol-acceptance-evidence` | PASS |
| `els2-type11-transcript-boundary` | PASS |
| `els2-live-lane-evidence` | PASS |
| `encrypted-service-consumer-caller` | PASS |
| `outproxy-request-path` | PASS |
| `outproxy-wire-lane-evidence` | PASS |
| `config-secret-hygiene` | PASS |
| `workflow-validity` | PASS |
| `floodfill-type5-serve` | PASS |
| `service-tunnel-acceptance-evidence` | PASS |
| `exploratory-tunnel-evidence` | PASS |
| `netdb-tunnel-evidence` | PASS |
| `destination-tunnel-evidence` | PASS |
| `streaming-tunnel-evidence` | PASS |
| `m6-mixed-router-acceptance-evidence` | PASS |
| `m12-floodfill-qualification-evidence` | PASS |
| `m12-floodfill-boundaries` | PASS |
| `ntcp2-harness` | PASS |
| `cargo-deny` | PASS |

The floor's own summary is **119 passed, 0 failed**, and the table above lists every step it ran.

The floor's own summary lines are:

```text
FLOOR PASS 119
FLOOR FAIL 0
FLOOR ALL GREEN
```

## The negative controls

Twelve controls mutated the **real tree**, ran the lane or the guard, and
observed the expected row fail. Every control was then restored from the bytes
saved before the mutation, and the SHA-256 of every touched file was compared
with the value it had before, so the tree is provably byte-identical
afterwards. Every control **compiles**: a control that broke the build would
prove nothing about the row it names.

| Control | What it restored | What it broke |
| --- | --- | --- |
| NC-1 | the new-key path does not index the record | the lane's row `the_index_does_not_drift_on_an_insert` |
| NC-2 | the replacement path does not index the new publication time | the lane's row `a_replacement_is_removed_at_its_new_time` |
| NC-3 | the removal path does not release the index entry | the guard's rule `the_store_maintains_an_age_index` |
| NC-4 | the prune walks the record map again | the guard's rule `the_prune_walks_the_index_and_not_the_record_map` |
| NC-5 | the prune's signature moves | the guard's rule `the_prune_signature_and_the_surfaces_are_unchanged` |
| NC-6 | the daemon's call site moves | the guard's rule `the_call_site_and_the_reconcile_are_unchanged` |
| NC-7 | a configuration key appears | the guard's rule `no_new_key_and_nothing_is_advertised` |
| NC-8 | the manifest changes | the guard's rule `no_new_key_and_nothing_is_advertised` |
| NC-9 | a predecessor's guard changes | the guard's rule `the_predecessors_evidence_is_untouched` |
| NC-10 | the prune's window is doubled, so the stale records survive | the lane's row `the_prune_removes_exactly_the_stale_set` |
| NC-11 | the stale prefix is never taken | the lane's row `the_prune_removes_every_stale_record_in_one_call` |
| NC-12 | the new-key path indexes the epoch, so every record is stale | the lane's row `the_prune_removes_nothing_when_nothing_is_stale` |

The controls cover **both** halves of the evidence. NC-3 through NC-9 restore a
defect the **guard** is the authority for — the shape. NC-1, NC-2, NC-10, NC-11
and NC-12 restore a defect the **lane** is the authority for — the prune's
behaviour — and they are the ones that move the **material** rather than the
structure, which is what makes the lane's independent observation worth having.

**Two of the plan's defects are not lane-observable, and the guard is their
authority.** The removal path's index release (NC-3) is not lane-observable,
because a stale index entry names a record that is already gone and the prune's
own walk is unaffected. The prune's **shape** (NC-4) is not lane-observable
either, because the pre-Plan-540 record walk has exactly the pre-Plan-540
semantics. The controls are **not** weakened to force a lane row to fail: a
control that the lane cannot observe is a control for the guard, and saying
otherwise would be a false claim about the evidence. The plan's amendment log,
row 17.2.2, records this.

**NC-5 is the control that keeps the whole tree compiling.** The mutation
moves the prune's signature, so every call site in the tree has to move with
it — the daemon's own sources, the store's unit tests and the evidence lanes.
The control was pre-checked with `cargo check --locked -p i2pr-netdb -p
i2pr-daemon --all-targets`, which passed with **zero** warnings, so the
mutation is a pure signature move and not a build break.

## Acceptance criteria

| # | The plan's criterion | Outcome |
| --- | --- | --- |
| 1 | the prune removes exactly the stale set, and the lane measures it against an independent computation | MET — the lane's rows `the_prune_removes_exactly_the_stale_set`, `the_prune_removes_every_stale_record_in_one_call` and `the_prune_removes_nothing_when_nothing_is_stale`; NC-10, NC-11 and NC-12 restore the defect |
| 2 | the prune walks the index and not the record map, and the guard proves it | MET — the guard's rule `the_prune_walks_the_index_and_not_the_record_map`; NC-4 restores the defect |
| 3 | the index never drifts, and the lane proves it through the prune's own behaviour | MET — the lane's rows `the_index_does_not_drift_on_an_insert` and `a_replacement_is_removed_at_its_new_time`; the guard's rule `the_store_maintains_an_age_index`; NC-1, NC-2 and NC-3 restore the defect |
| 4 | the prune's signature, the store's public surfaces, the daemon's call site and the reconcile's shape are unchanged | MET — the guard's rules `the_prune_signature_and_the_surfaces_are_unchanged` and `the_call_site_and_the_reconcile_are_unchanged`; NC-5 and NC-6 restore the defect |
| 5 | no configuration key, dependency, task, timer, listener or socket is added, and `Cargo.toml` and `Cargo.lock` stay byte-identical | MET — the guard's rule `no_new_key_and_nothing_is_advertised`, and the two digests above; NC-7 and NC-8 restore the defect |
| 6 | `router.version` stays `0.9.62`, and the inventory's advertised count stays **zero** | MET — the guard's rule `no_new_key_and_nothing_is_advertised`, and `crates/i2pr-netdb/src/local.rs:172` |
| 7 | the full floor is green, with the closure record's table complete | MET — the table above |
| 8 | the guard's inverted probes each fail as intended and the tree is restored byte-identically after each | MET — the guard's inverted probes `the_inverted_probe_fires_when_the_index_is_not_maintained`, `the_inverted_probe_fires_when_the_removal_does_not_release_the_entry`, `the_inverted_probe_fires_when_the_prune_walks_the_record_map`, `the_inverted_probe_fires_when_the_prune_signature_moves`, `the_inverted_probe_fires_when_the_call_site_moves`, `the_inverted_probe_fires_when_a_configuration_key_appears` and `the_inverted_probe_fires_when_a_predecessor_changes`, and the twelve controls above |

## Invariant review

| # | The plan's invariant | Review |
| --- | --- | --- |
| 1 | the prune removes every record past the window, in one call | **Held.** The prune's body is unchanged except for how it *finds* the stale records, and the lane's rows `the_prune_removes_exactly_the_stale_set` and `the_prune_removes_every_stale_record_in_one_call` observe that every stale record is removed in one call. The two edge cases are observed too — a record published in the future is never stale (`a_record_published_in_the_future_is_never_stale`) and a `now` below the window removes nothing (`a_now_below_the_window_removes_nothing`). The lane's stale set is computed **independently**, by walking the store's own iterator, so the lane's expectation and the store's own bookkeeping are two different things that must agree. |
| 2 | the prune's cost is proportional to the number of stale records, not to the population | **Held.** The prune walks the index's ordered range to its stale prefix (`crates/i2pr-netdb/src/store.rs:328`), and the guard's rule `the_prune_walks_the_index_and_not_the_record_map` forbids the record-map walk outright. NC-4 restores the record-map walk and fires the rule. |
| 3 | the age index is maintained, not derived | **Held.** The three maintenance points are `crates/i2pr-netdb/src/store.rs:207`, `crates/i2pr-netdb/src/store.rs:184` and `crates/i2pr-netdb/src/store.rs:236`, and the guard's rule `the_store_maintains_an_age_index` requires the insert path to carry **two** index inserts and the removal path to carry an index release, and forbids a record-map scan in either path. |
| 4 | the index is exact | **Held.** The lane's rows `the_index_does_not_drift_on_an_insert` and `a_replacement_is_removed_at_its_new_time` observe the index from the prune's own behaviour, and NC-1, NC-2 and NC-12 restore the three ways it can drift. |
| 5 | the prune's public surface is unchanged | **Held.** `pub fn prune(&mut self, now: Date, max_age_ms: u64) -> usize` keeps its name, its parameters and its return at `crates/i2pr-netdb/src/store.rs:314`, and the guard's rule `the_prune_signature_and_the_surfaces_are_unchanged` pins it. NC-5 moves the signature and fires the rule. |
| 6 | the store's other public surfaces are unchanged | **Held.** The guard's rule `the_prune_signature_and_the_surfaces_are_unchanged` pins eleven signatures, and the lane's rows `the_statistics_and_the_selection_survive_the_prune` and `the_prune_is_deterministic` observe that the statistics and the selection survive the prune and that two identical stores prune identically. |
| 7 | the byte accounting and the floodfill advertiser count are unchanged | **Held.** The prune goes through the store's own `remove`, and the lane's row `the_prune_releases_the_bytes_and_the_advertiser` observes that the bytes and the advertiser are released with the record. |
| 8 | the router's own RouterInfo is never a prune victim | **Held.** The router's own record lives beside the store, not inside it, and the plan does not move it. The lane's rows never place the router's own record in the store. |
| 9 | the daemon's prune call site and the reconcile's shape are unchanged | **Held.** The bootstrap still calls `self.store.prune(Date::from_millis(now_ms), window_ms)` at `crates/i2pr-daemon/src/bootstrap.rs:292`, and the reconcile still prunes, then snapshots, then learns (`crates/i2pr-daemon/src/peer_reconcile.rs:417`). The guard's rule `the_call_site_and_the_reconcile_are_unchanged` pins all of it. NC-6 moves the call site and fires the rule. |
| 10 | no configuration key is added, and no dependency is added | **Held.** The guard's rule `no_new_key_and_nothing_is_advertised` rejects the tokens `prune`, `retention`, `age_index` and `prune_budget` in the daemon's configuration, and requires the manifest's and the lock's digests to be unchanged. NC-7 and NC-8 restore the defect and fire the rule. |
| 11 | no new task, timer, listener or socket | **Held.** The guard's rule `no_new_key_and_nothing_is_advertised` rejects `tokio::`, `std::net`, `std::fs`, `std::thread`, `JoinHandle` and `spawn(` in the changed module, and the plan adds no runtime construct anywhere. |
| 12 | no advertisement | **Held.** `crates/i2pr-netdb/src/local.rs:172` still declares `0.9.62`, and the inventory advertises nothing: the guard's rule `no_new_key_and_nothing_is_advertised` requires that no surface in `specs/support.toml` carries `advertised = true`. |
| 13 | Plan 507's, Plan 538's and Plan 539's lanes and guards stay byte-identical | **Held.** The guard's rule `the_predecessors_evidence_is_untouched` pins all six digests, and NC-9 restores a predecessor's guard and fires it. |

## Failure, cancellation, restart, and contention review

**Failure.** The change touches no error path. The prune's failure modes are
exactly the ones it had: a store that cannot be pruned, and a record that
cannot be removed. The prune returns the number of records it removed, and
the count is the same count it returned before.

**Cancellation.** The change touches no async path. The prune is synchronous
and short, it holds no lock other than the bootstrap's own, and no lock is
held across an await.

**Restart.** Nothing is persisted. The index is derived from the store's own
contents and is rebuilt from scratch when the store is constructed, so a
restart cannot inherit a stale index. The index is not a cache of anything the
tree persists.

**Contention.** The router's NetDB lock is no longer held across the prune's
O(population) walk: the prune walks the index's stale prefix, so the lock is
held for a walk proportional to what the prune removes. The store's selection
path still walks the advertisers under the same lock, because a selection over
the floodfill advertisers is a walk by its nature, and it keeps its named
owner in **What the plan did not close**.

## Migration and compatibility review

**No wire change.** The prune removes exactly the records it removed before.

**No configuration migration.** **No key is added.** The daemon's configuration
surface is unchanged, and the NetDB configuration keeps exactly its twelve
keys.

**No persisted state.** The index lives inside the store and is rebuilt with
it, so there is nothing to migrate.

**No change to the netdb crate's public surface.** No `i2pr-netdb` item is
removed or renamed. The store's new field is **private**, and the prune's
signature and the store's other public signatures are unchanged.

**No change to the daemon's public surface.** The bootstrap's prune, its call
site and the reconcile's shape are exactly what the tree already had.

**No change to the manifest or the lock.** `Cargo.toml` and `Cargo.lock` are
byte-identical to the tree Plan 539 closed on, and the digests above are the
tree's. No requirement is added to any manifest.

## Security review

**No secret is touched.** The index holds publication times and router
hashes. Nothing in this plan reads, writes or derives from a private key, a
signing seed or a session secret.

**No new disclosure.** The index is derived from records the router already
holds, and it is **private** to the store.

**No new surface is reachable.** The plan adds no listener, no port, no socket
and no configuration key, so it opens no new way in.

**No peer is blamed.** The plan writes nothing into a peer's profile and
creates no peer-side condition.

**Fail closed.** The guard fails closed on every item it names: an index that
is not maintained, a prune that walks the record map, a prune whose signature
moved, a call site that moved, a configuration key, a changed manifest and a
changed predecessor's evidence each make the rule that names them fail. The
lane fails closed too: the prune's count, its stale set and the store's
contents after the prune are all asserted, so a prune that removed too little
or too much fails rather than passes.

## Limitations

- The lane proves the prune's **behaviour**: that it removes exactly the stale
  set, that the index never drifts, that the accounting is released and that
  the prune is deterministic. It cannot see the prune's **shape** — that it
  walks the index and not the record map — so that is the **guard's** claim,
  and the guard's claim is a static one.
- The plan's effect on the lock's **contention** is structural rather than
  measured: no timing assertion is made, because a timing assertion under a
  shared disk would be flaky and would prove less than the shape does.
- The store's selection path still walks the advertisers under the router's
  NetDB lock, and the router's validated store is still the bootstrap's store.
  The plan does not claim otherwise, and both keep their named owner.
- The **citation drift** this plan's production change caused in
  `crates/i2pr-netdb/src/store.rs` is recorded in the plan's amendment log as
  an **additive pointer table**. The closed closure records are not rewritten,
  so a reader who follows an old line number finds the pointer rather than the
  construct.

## Findings by severity

### None — nothing was found that blocks the plan

### Medium — discharged

- **The store's prune was O(population) under the router's NetDB lock.** The
  daemon's peer reconcile called the store's prune under the bootstrap's
  mutex, which *is* the router's NetDB lock, and the prune found the stale
  records by walking **every** retained record. The prune is on the router's
  live maintenance cadence, so the lock was held across the population walk on
  every reconcile pass that prunes, and every other NetDB reader queued behind
  it. **Discharged in this plan**: the store maintains an age index, the prune
  walks the index's stale prefix, and the lock is held for a walk proportional
  to what the prune removes.
- **The plan's own negative control for the window was not observable.** The
  first control halved the window, which puts the threshold exactly at the
  lane's fresh records' publication time and leaves the prune's outcome
  unchanged — a control that does not change the outcome proves nothing.
  **Discharged in this plan**: the control doubles the window instead, so the
  stale records survive the prune and the lane's independent stale set
  disagrees with it. The plan's amendment log, row 17.2.1, records this.

### Low — recorded by this plan

- **The router's validated store is still the bootstrap's store.**
  (`crates/i2pr-daemon/src/bootstrap.rs:215`) This is Plan 536's recorded
  **Low** finding 1, carried forward unchanged. **Owner:** the successor that
  gives the router a NetDB owner of its own.
- **The store's selection path is still a walk under the router's NetDB lock.**
  (`crates/i2pr-netdb/src/store.rs:343`) A selection over the floodfill
  advertisers is a walk by its nature. **Owner:** the same successor.
- **The composition root's shape is proven statically, not behaviourally.**
  This is Plan 536's recorded **Low** finding 2, carried forward unchanged.
  **Owner:** the successor that makes the composition root's relay arm
  reachable from a test.
- **The production SSU2 peer-test driver** — Plan 530's Low finding 2.
- **The `B` advertisement and `caps=f`** — their own owners.
- **The stock-peer half** — Plan 493's authorized two-host topology.

## The citation drift this plan's production change caused

The plan's production change is **40 insertions and 10 deletions** in
`crates/i2pr-netdb/src/store.rs`, and it moves the lines that Plan 538's and
Plan 539's **closed** closure records cite. Those records are **not** rewritten.

**What was done.** The plan's amendment log, section 17.2, carries an
**additive pointer table**: one row per citation, naming the construct, the
line the closed record cited, and the line the same construct carries now.
Plan 540's own closure record cites the **post-change** lines, and the plan's
section 3 and section 6 keep their **pre-change** anchors, which the closure
record's verifier checks against the registration commit `b6cb3817`.

**What was deliberately not done.** Plan 538's and Plan 539's closed closure
records are not rewritten, and their committed prose is not reworded. The
pointer table is the correction, and it is additive.

Two documentation edits were made **line-count preserving** on purpose, so that
the citations the closed records make into the documents still resolve to the
same line: the netdb deep dive's stale sentence is amended in place rather
than removed, and the count paragraph in `specs/CONFORMANCE.md` leads Plan
540's own section rather than joining the Plan 514 ledger.

## Roadmap disposition

The **network-participation** roadmap's row for Plan 540 moves from
`registered` to `closed`, and the registry's row moves from `registered` to
`closed`. No roadmap row is reopened, renumbered or rewritten, and no other
plan's row changes.

The plan **advances** the network-participation line's lock-contention work:
Plan 536 removed the relay target's answer from the router's NetDB lock, Plan
537 removed the relay requester's outbound step from it, Plan 538 removed the
daemon's bootstrap's **population walk** from it, Plan 539 removed the
**composition root's population** from it, and Plan 540 removed the **store's
prune** from it. The line's remaining named owner is unchanged: the successor
that gives the router a NetDB owner of its own.

## Unblock audit

- **Plan 540's own findings** are discharged above, and the ones that remain
  have a named owner.
- **Plan 536's recorded Low finding 1** — the router's validated store is still
  the bootstrap's store — is **carried forward unchanged**, with the same owner.
- **Plan 536's recorded Low finding 2** — the composition root's shape is
  proven statically — is **carried forward unchanged**, with the same owner.
- **Plan 537's, Plan 538's and Plan 539's named out-of-scope items** — the
  NetDB ownership, the composition root's reachability and the store's
  selection — are **carried forward unchanged**, with the same owner.
- **Plan 530's Low finding 2** — the production SSU2 peer-test driver — stays
  with its owner.
- **Plan 493's stock-peer half** stays blocked externally, on Plan 493's
  authorized two-host topology. This plan needs no stock peer.
- **No plan is unblocked by this one.** The plan changes no advertisement, no
  capability, no version and no configuration, so it opens no gate for another
  plan.

## Handoff notes

- The plan is **experimental** and default-off. Do not describe the change as
  released, supported or advertised.
- The delivery is confined to the controlled loopback profile.
- The stock-peer half stays blocked on Plan 493's authorized two-host
  topology.
- Plan 507's, Plan 536's, Plan 537's, Plan 538's and Plan 539's closed closure
  records are **not** rewritten. The discharge of their named out-of-scope items
  lives in this plan's closure record and in the registry's row.
- The plan **extends** no other plan's guard. Plan 507's, Plan 538's and Plan
  539's lanes and guards stay byte-identical, and the guard pins all six
  digests.
- A successor must **not** "simplify" the prune back into a record-map walk, and
  must not rebuild the index by a scan: the index's exactness is a retention
  guarantee, not an optimization.
- The tree's next step in this line is the successor that gives the router a
  **NetDB owner of its own**. That change is larger than this plan and is not
  attempted here.
- **The shared disk is the machine's tightest resource.** The build cache lives
  outside the repository at
  `/home/sugarwookie/.cache/i2pr-plan535-target`, and `CARGO_INCREMENTAL=0` and
  `CARGO_GC_AUTO_FREQUENCY=never` are set, so a successor's builds do not add
  to the pressure. Most of the disk is **other** sessions' data; do not delete
  it.

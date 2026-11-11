# Plan 539 — the routing table's population must not hold the router's NetDB lock

**Closure record.** The plan of record is
`plans/implementation/network-participation/539-the-routing-tables-population-must-not-hold-the-routers-netdb-lock.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 539 is **closed**. The router's routing table is populated by a **short,
bounded read** whose guard is released before the read returns, so the indexing
happens with **no** NetDB lock held and the router's NetDB lock never wraps the
routing table's mutex. And the population's **absence** is a **typed, counted
refusal** rather than a silent skip.

Classification: **corrective** (the composition root walked every floodfill
advertiser the NetDB holds **under the router's NetDB lock**, and the walk's body
took the routing table's own mutex, so the NetDB lock **wrapped** the routing
table's — on the router's live maintenance cadence; and a router that held no
store simply returned, without indexing, without refusing and without counting).
**Subsystem:** network-participation. **Closure date:** 2026-11-11.

Nothing is promoted: `router.version` stays `0.9.62`, the inventory's
advertised count stays **zero**, and `Cargo.toml` and `Cargo.lock` stay
byte-identical.

## What the plan closed

**The population is read through a source.** The composition root's
`populate_routing_table` (`crates/i2pr-daemon/src/lib.rs:2169`) takes a
`source: &dyn crate::netdb_participation::RoutingTablePopulationSource`
(`crates/i2pr-daemon/src/lib.rs:2171`) instead of the bootstrap, and the trait
is at `crates/i2pr-daemon/src/netdb_participation.rs:663`. The trait is
implemented **for the store itself**
(`crates/i2pr-daemon/src/netdb_participation.rs:676`), so a lane that passes a
`&store` still compiles and still passes.

**The guard is released before the read returns.** The production source is
`ValidatedStorePopulationSource`
(`crates/i2pr-daemon/src/netdb_participation.rs:694`), its constructor is at
`crates/i2pr-daemon/src/netdb_participation.rs:699` and its implementation of
the trait is at `crates/i2pr-daemon/src/netdb_participation.rs:713`. Its read
takes the bootstrap's lock, performs **one** read, and drops the guard before
it returns (`crates/i2pr-daemon/src/netdb_participation.rs:727`).

**The shape is the one the tree already used.** Thirty lines below the defect,
Plan 505's `learn_bootstrap_peers`
(`crates/i2pr-daemon/src/lib.rs:2203`) takes the bootstrap's lock for one
self-contained read, lets the guard drop before the read returns, and learns
its batch afterwards. The plan uses the shape that is already there.

**The indexing happens with no NetDB lock held.** The composition root indexes
the returned advertisers **after** the read returns, so the router's NetDB lock
never wraps the routing table's mutex.

**The population's absence is a typed, counted refusal.**
`RoutingPopulationRefusal` (`crates/i2pr-daemon/src/netdb_participation.rs:643`)
is the refusal's type, a missing store and a poisoned lock are the **same**
refusal, and the participation owner holds the counter
(`crates/i2pr-daemon/src/lib.rs:1901`) while the **source** increments it. The
count is reported in the owner's stop message
(`crates/i2pr-daemon/src/lib.rs:2336`). The refusal is a fact about **this
router**, never about a peer, and it is never written into a peer's profile.

**The population is the same set.** The source yields the **records**, never a
peer's word about another peer, so the table's own signed-evidence rule — a peer
whose own signed `RouterInfo` does not advertise `f` is refused
(`crates/i2pr-netdb/src/bucket_set.rs:420`) — stays the **table's** rule, and
the store's selection surface (`crates/i2pr-netdb/src/store.rs:313`) is
unchanged: it still filters on the capability, and the capability itself is
still `advertises_floodfill` (`crates/i2pr-netdb/src/router_info.rs:334`).

**The population rides the existing tick.** It runs at startup and again on
every tick of the participation maintenance owner, exactly as it did before. No
store, no lock, no owner, no configuration key, no task, no timer, no listener
and no socket was added, and the routing table's own surface — `index`
(`crates/i2pr-daemon/src/netdb_participation.rs:845`) and its counters — is
exactly what the tree already had.

## What the plan did not close

- **The router's validated store is still the bootstrap's store.** The
  bootstrap owns the only `RouterInfoStore` in the daemon
  (`crates/i2pr-daemon/src/bootstrap.rs:215`). Separating the router's NetDB
  from the bootstrap's lifetime is a larger change and was not attempted here.
  This is Plan 536's recorded **Low** finding 1, carried forward unchanged.
  **Owner:** the successor that gives the router a NetDB owner of its own.
- **The store's prune is still O(population) under the router's NetDB lock.**
  A prune must examine every record, so this is inherent to the operation. This
  plan removes the *population's* walk, not the prune's own examination of every
  record. **Owner:** the same successor.
- **The composition root's shape is proven statically, not behaviourally.**
  The composition root's population is private, so the lane cannot see it and
  the **shape** — that the composition root holds no NetDB lock across its
  indexing — is the guard's claim. This is Plan 536's recorded **Low** finding
  2, carried forward unchanged. **Owner:** the successor that makes the
  composition root's relay arm reachable from a test.
- **The production SSU2 peer-test driver** — Plan 530's Low finding 2.
- **The `B` advertisement and `caps=f`** — their own owners.
- **The stock-peer half** — Plan 493's authorized two-host topology.

## Implementation commits

| Commit | What it carries |
| --- | --- |
| `d8dee02f` | the plan of record, the registry's row and the roadmap's row |
| `fcdd090c` | the production change: the population source, the typed refusal, the owner's counter and the composition root's new shape |
| `23ab23aa` | the lane and the guard |
| `b38239ae` | the guard's fifth rule, which now checks the store's selection **filter** and not only its signature, and the plan's section 6.2, corrected to say that the **source** increments the owner's counter |
| `d3070f83` | the lane's fail-fast repair: every lock the lane wants to **hold** is taken with `try_lock`, so a source that leaked the guard fails the lane immediately instead of blocking it |
| `5ef0e09d` | the documentation, the inventory, the plan's amendment log, the plan's section 11.1 settled to the method the lane uses, and the removal of the three **duplicated** Plan 538 sections |

The floor ran on the committed tree `5ef0e09d`. The closure commit adds **only**
this record and the two closed rows, exactly as Plan 538's closure commit did.

The plan of record's **invariants** are at
`plans/implementation/network-participation/539-the-routing-tables-population-must-not-hold-the-routers-netdb-lock.md:164`,
its **required production changes** at
`plans/implementation/network-participation/539-the-routing-tables-population-must-not-hold-the-routers-netdb-lock.md:246`,
its **design decisions** at
`plans/implementation/network-participation/539-the-routing-tables-population-must-not-hold-the-routers-netdb-lock.md:311`,
its **lane** at
`plans/implementation/network-participation/539-the-routing-tables-population-must-not-hold-the-routers-netdb-lock.md:404`,
its **guard** at
`plans/implementation/network-participation/539-the-routing-tables-population-must-not-hold-the-routers-netdb-lock.md:436`,
its **amendment log** at
`plans/implementation/network-participation/539-the-routing-tables-population-must-not-hold-the-routers-netdb-lock.md:569`,
the netdb deep-dive's account of the unchanged signed-evidence rule at
`docs/architecture/i2pr-netdb.md:1491`, the daemon deep-dive's account of the
composition root's new shape at `docs/architecture/i2pr-daemon.md:4460`, the
qualification gates at `specs/QUALIFICATION-GATES.md:2157` and the conformance
account at `specs/CONFORMANCE.md:4553`.

## Requirement-to-evidence matrix

| # | Requirement | Evidence |
| --- | --- | --- |
| 1 | the population is read under a short lock and indexed with no lock held | the guard's rules `the_population_is_read_through_the_source` and `the_source_releases_the_lock_before_it_returns`; the lane's rows `the_read_releases_the_lock_before_it_returns` and `the_indexing_needs_no_netdb_lock` |
| 2 | the router's NetDB lock never wraps the routing table's mutex | the guard's rule `the_population_is_read_through_the_source`, and the production change at `crates/i2pr-daemon/src/lib.rs:2169` |
| 3 | the population is the same set | the lane's rows `the_population_is_the_same_set` and `the_source_reads_the_advertisers` |
| 4 | the table still indexes each peer with its own signed evidence | the lane's row `the_table_refuses_a_peer_that_does_not_advertise_f`; the guard's rule `the_table_still_refuses_a_non_advertiser` |
| 5 | the population's absence is never skipped silently | the lane's rows `an_absent_store_is_a_typed_refusal` and `a_poisoned_lock_is_the_same_typed_refusal`; the guard's rule `the_absence_is_a_typed_and_counted_refusal` |
| 6 | the refusal is a fact about this router, never about a peer | the lane's rows `an_absent_store_is_a_typed_refusal` and `every_refusal_is_counted_under_its_own_reason`; the production change at `crates/i2pr-daemon/src/lib.rs:1901` |
| 7 | the copy is bounded and it is a copy | the production change at `crates/i2pr-daemon/src/netdb_participation.rs:727`; the lane's row `the_population_mutates_nothing` |
| 8 | the routing table's existing surface is unchanged | the guard's rule `the_existing_surfaces_are_untouched`; the lane's row `the_store_is_its_own_source` |
| 9 | the store's selection surface is unchanged | the guard's rule `the_existing_surfaces_are_untouched` |
| 10 | no configuration key is added, and no task, timer, listener or socket is added | the guard's rule `no_new_surface_and_the_population_rides_the_existing_tick` |
| 11 | no advertisement | the guard's rule `no_new_surface_and_the_population_rides_the_existing_tick`; `crates/i2pr-netdb/src/local.rs:172` |
| 12 | Plan 536's, Plan 537's and Plan 538's lanes and guards stay byte-identical | the guard's rule `the_predecessors_evidence_is_untouched`, and the six digests below |

The six digests the guard pins are Plan 536's lane
(`911b19d357d9eecaab10bcd5026f4098ae195a54c7a44ec389e80a01b72d4d43`) and guard
(`ea2ec6f7a6874f896a6c05a7f8c32352fb6320f90b4b35a34fda925087fe594a`), Plan
537's lane
(`b6728b985b07a2c58697f508c2ae09e6c710cf8507e45cd4108ed1088fd6df2f`) and guard
(`145ec3bfdaa629538c0741fd894f7659ebaa56b4bf23591968556cf4c18d5213`), and Plan
538's lane
(`d1ce001d1e7562ca84b33cd9b4b0bca04325565b7cfef2c7504645825c8bc613`) and guard
(`3d059e0690a0ab52b5c54619c0b5220eac8f340168187c2541f12f597c518fa4`). A later
change to any of them must change the pinned constant **and** say why.

The manifest and the lock are byte-identical to the tree Plan 538 closed on, and
the digests the record carries are the manifest's
`39c44e33add62693eef8866220de17dcfe137437b62732e5044fcaefc4870269` and the
lock's
`f0cec0685c4ed460c7d63273cc318f7a683dc2357735d7edaa6dfa1ac243d833`.

## Focused verification

| Command | Outcome |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked --workspace --all-targets` | PASS |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked -p i2pr-daemon --test routing_table_population_lock -- --test-threads=1` | PASS — 10 rows |
| `cargo test --locked -p i2pr-daemon --test routing_table_population_lock_boundary -- --test-threads=1` | PASS — 24 rows |
| `cargo test --locked -p i2pr-daemon --lib -- --test-threads=1` | PASS — 1001 rows |
| `cargo test --locked -p i2pr-netdb --all-targets` | PASS |
| `cargo test --locked -p i2pr-daemon --test ssu2_relay_target_lock --test ssu2_relay_target_lock_boundary --test ssu2_relay_requester_lock --test ssu2_relay_requester_lock_boundary --test floodfill_statistic_accounting --test floodfill_statistic_accounting_boundary -- --test-threads=1` | PASS — 7 rows, 29 rows, 7 rows, 33 rows, 14 rows and 32 rows |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 scripts/check-license-metadata.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 51 tests |
| `python3 /tmp/539-negative.py` | PASS — 12 controls, each firing the surface that owns the defect, each restored byte-identically |

The closure record's **citations** are verified mechanically, not by reading.
The verifier recomputes every cited line from the tree and requires the record
to name exactly that line:

- the **closure record's** citations are recomputed against the **current
  tree** — the first line that carries the cited text is the line the record
  must name;
- the **plan of record's** section 3 citations are recomputed against the
  **registration commit** `d8dee02f`, because section 3 is the evidence of the
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

The floor ran on the committed tree `5ef0e09d` and it is **green**. Every
step is recorded in `/tmp/539-floor.log`, with each step's own log under
`/tmp/539f-<step>.log`. The floor is a plain sequence of `step` runs that
records each step's exit status; no step is skipped, no `|| true`, no
`continue-on-error` and no filename filtering. The workspace step runs with
`--no-fail-fast`, so every test binary's result is reported rather than the
run stopping at the first failure.

Plan 539's floor is Plan 538's floor plus two steps -- the daemon lane and
the daemon guard for the routing table's population. **No other step
changed**, and **no other plan's guard file is touched**: Plan 539 extends
no predecessor's guard, and Plan 536's, Plan 537's and Plan 538's lanes and
guards stay byte-identical.

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

The floor's own summary is **117 passed, 0 failed**, and the table above lists every step it ran.

## The negative controls

Twelve controls mutated the **real tree**, ran the lane or the guard, and
observed the expected row fail. Every control was then restored from the bytes
saved before the mutation, and the SHA-256 of every touched file was compared
with the value it had before, so the tree is provably byte-identical
afterwards. Every control **compiles**: a control that broke the build would
prove nothing about the row it names.

| Control | What it restored | What it broke |
| --- | --- | --- |
| NC-1 | the population reads the bootstrap under the NetDB lock | the guard's rule `the_population_is_read_through_the_source` |
| NC-2 | the source copies the advertisers differently | the guard's rule `the_source_releases_the_lock_before_it_returns` |
| NC-3 | one refusal path stops counting | the guard's rule `the_absence_is_a_typed_and_counted_refusal` |
| NC-4 | the table's signed-evidence rule is weakened | the guard's rule `the_table_still_refuses_a_non_advertiser` |
| NC-5 | the store's selection stops filtering on the capability | the guard's rule `the_existing_surfaces_are_untouched` |
| NC-6 | a configuration key is added | the guard's rule `no_new_surface_and_the_population_rides_the_existing_tick` |
| NC-7 | the inventory advertises a surface | the guard's rule `no_new_surface_and_the_population_rides_the_existing_tick` |
| NC-8 | a predecessor's guard changes | the guard's rule `the_predecessors_evidence_is_untouched` |
| NC-9 | the source stops counting the refusal | the lane's row `an_absent_store_is_a_typed_refusal` |
| NC-10 | the source returns every record, not only the advertisers | the lane's row `the_population_is_the_same_set` |
| NC-11 | the source mutates the store it read | the lane's row `the_population_mutates_nothing` |
| NC-12 | the source leaks the guard, so the lock is still held | the lane's row `the_read_releases_the_lock_before_it_returns` |

The controls cover **both** halves of the evidence. NC-1 through NC-8 restore a
defect the **guard** is the authority for — the shape. NC-9 through NC-12
restore a defect the **lane** is the authority for — the source's behaviour — and
they are the ones that move the **material** rather than the structure, which is
what makes the lane's independent observation worth having.

**NC-12 is the control that found the lane's own defect.** The mutation leaks
the guard, so the lock is still held when the read returns. The lane's first
draft took the bootstrap's lock **blocking** wherever it wanted to *hold* it, so
a leaked guard left the mutex locked for ever and the lane's own tests blocked
on it instead of failing: the control hung for ten minutes and never reported.
A lane that can hang is not evidence, because the control has to see the row
**fail**. Every lock the lane wants to hold is now taken with `try_lock`, so the
mutation fails the lane **immediately**, and the control fires.

## Acceptance criteria

| # | The plan's criterion | Outcome |
| --- | --- | --- |
| 1 | the population is indexed with **no** NetDB lock held, and the lane measures it | MET — the lane's rows `the_read_releases_the_lock_before_it_returns` and `the_indexing_needs_no_netdb_lock`, and the guard's rules `the_population_is_read_through_the_source` and `the_source_releases_the_lock_before_it_returns`; NC-1, NC-2 and NC-12 restore the defect |
| 2 | the population runs without a store and **counts** a typed refusal | MET — the lane's rows `an_absent_store_is_a_typed_refusal`, `a_poisoned_lock_is_the_same_typed_refusal` and `every_refusal_is_counted_under_its_own_reason`; the guard's rule `the_absence_is_a_typed_and_counted_refusal`; NC-3 and NC-9 restore the defect |
| 3 | the population is the **same set**, and the table's signed-evidence rule is unchanged | MET — the lane's rows `the_population_is_the_same_set` and `the_table_refuses_a_peer_that_does_not_advertise_f`; the guard's rules `the_table_still_refuses_a_non_advertiser` and `the_existing_surfaces_are_untouched`; NC-4, NC-5 and NC-10 restore the defect |
| 4 | the routing table's and the store's surfaces are unchanged, and no configuration key, task, timer, listener or socket is added | MET — the guard's rules `the_existing_surfaces_are_untouched` and `no_new_surface_and_the_population_rides_the_existing_tick`; NC-5 and NC-6 restore the defect |
| 5 | `router.version` stays `0.9.62`, and the inventory's advertised count stays **zero** | MET — the guard's rule `no_new_surface_and_the_population_rides_the_existing_tick`, and NC-7 |
| 6 | the full floor is green, with the closure record's table complete | MET — the table above |
| 7 | the guard's inverted probes each fail as intended and the tree is restored byte-identically after each | MET — the twelve controls above |

## Invariant review

| # | The plan's invariant | Review |
| --- | --- | --- |
| 1 | the population is read under a short lock and indexed with no lock held | **Held.** The source's read ends at `crates/i2pr-daemon/src/netdb_participation.rs:727`, where the guard is dropped before the read returns, and the composition root's indexing loop runs after the read returns at `crates/i2pr-daemon/src/lib.rs:2169`. The lane's row `the_indexing_needs_no_netdb_lock` indexes **while** the bootstrap's lock is held. |
| 2 | the router's NetDB lock never wraps the routing table's mutex | **Held.** The composition root holds no NetDB lock across its indexing, and the guard's rule `the_population_is_read_through_the_source` forbids the shape it would need to violate this. NC-1 restores the violation and fires the rule. |
| 3 | the population is the same set | **Held.** The source yields the store's own selection, and the lane's row `the_population_is_the_same_set` compares the population with the store's own selection, key for key. NC-10 restores a population of every record and fires the row. |
| 4 | the table still indexes each peer with its own signed evidence | **Held.** The source yields the **records**, and the table's own rule at `crates/i2pr-netdb/src/bucket_set.rs:420` is untouched. NC-4 restores the weakened rule and fires the guard's rule. |
| 5 | the population's absence is never skipped silently | **Held.** A missing store and a poisoned lock are the same typed refusal, and the lane's rows `an_absent_store_is_a_typed_refusal` and `a_poisoned_lock_is_the_same_typed_refusal` observe them. NC-3 and NC-9 restore the uncounted path and fire. |
| 6 | the refusal is a fact about this router, never about a peer | **Held.** The refusal is counted in the participation owner's own counter, and nothing in the changed code writes a peer's profile. The lane's row `the_population_mutates_nothing` observes that the read changes nothing. |
| 7 | the copy is bounded and it is a copy | **Held.** The source copies the advertisers out and the store's own records are not moved out of it; the copy is bounded by the NetDB's configured record ceiling. |
| 8 | the routing table's existing surface is unchanged | **Held.** `index` keeps its signature at `crates/i2pr-daemon/src/netdb_participation.rs:845`, the counters are unchanged, and the guard's rule `the_existing_surfaces_are_untouched` pins them. |
| 9 | the store's selection surface is unchanged | **Held.** `floodfill_advertisers()` keeps its signature at `crates/i2pr-netdb/src/store.rs:313`, and the guard's rule `the_existing_surfaces_are_untouched` now also requires the selection's **body** to filter on `advertises_floodfill()`. NC-5 restores the unfiltered selection and fires the rule. |
| 10 | no configuration key is added, and no task, timer, listener or socket is added | **Held.** The population rides the existing startup and the existing tick, and the guard's rule `no_new_surface_and_the_population_rides_the_existing_tick` forbids the infrastructure constructs. NC-6 restores a configuration key and fires it. |
| 11 | no advertisement | **Held.** `crates/i2pr-netdb/src/local.rs:172` still declares `0.9.62`, and the inventory advertises nothing. NC-7 restores an advertised surface and fires the guard's rule. |
| 12 | Plan 536's, Plan 537's and Plan 538's lanes and guards stay byte-identical | **Held.** The guard's rule `the_predecessors_evidence_is_untouched` pins all six digests, and NC-8 restores a predecessor's guard and fires it. |

## Failure, cancellation, restart, and contention review

**Failure.** The population's read has one failure mode: the store is absent or
the lock is poisoned. Both are the **same** typed refusal, counted under its own
reason, so a router that cannot read its population says so instead of skipping
the population in silence. The daemon has no new failure mode: the population
indexes the same set it indexed before.

**Cancellation.** The change touches no async path. The population's read is
synchronous and short, the guard is dropped before the read returns, and no lock
is held across an await.

**Restart.** Nothing is persisted. The population is rebuilt from the store's own
contents whenever the owner ticks, so a restart cannot inherit a stale
population or a stale refusal count.

**Contention.** The router's NetDB lock is no longer held across the population's
O(population) walk, and it no longer **wraps** the routing table's mutex, so the
routing table's own lookups no longer queue behind the NetDB lock. The store's
selection copy still happens under the lock — a selection over the advertisers
is a walk by its nature — and the store's **prune** is still a population walk
under the same lock, because a prune must examine every record; both keep their
named owners in **What the plan did not close**.

## Migration and compatibility review

**No wire change.** The population indexes the same peers, with the same signed
evidence, and the table's own refusal is unchanged.

**No configuration migration.** **No key is added.** The daemon's configuration
surface is unchanged, and the NetDB configuration keeps exactly its twelve keys.

**No persisted state.** The population is derived from the store's contents and
is never persisted, so there is nothing to migrate.

**No change to the netdb crate's public surface.** No `i2pr-netdb` item is
removed or renamed, and the store's selection path is unchanged.

**No change to the routing table's public surface.** `index`, `index_suggested`,
the counters and the receipts are exactly what the tree already had.

**No change to the manifest or the lock.** `Cargo.toml` and `Cargo.lock` are
byte-identical to the tree Plan 538 closed on, and the digests above are the
tree's. No requirement is added to any manifest.

## Security review

**No secret is touched.** The population is public RouterInfo material. Nothing
in this plan reads, writes or derives from a private key, a signing seed or a
session secret.

**No new disclosure.** The population is the same set the router already held,
and the refusal count is a number about **this router**, reported in the owner's
own stop message.

**No new surface is reachable.** The plan adds no listener, no port, no socket
and no configuration key, so it opens no new way in.

**No peer is blamed.** The refusal is a fact about **this router's** NetDB, not
about any peer. The plan writes nothing into a peer's profile and creates no
peer-side condition.

**Fail closed.** The guard fails closed on every item it names: a population
that is not read through the source, a source that holds the lock across the
return, an absence that is not typed and counted, a weakened signed-evidence
rule, a changed surface, a new configuration key, an advertised surface and a
changed predecessor's evidence each make the rule that names them fail.

## Limitations

- The lane proves the **source's** behaviour: that its read releases the guard
  before it returns, that the absence is typed and counted, and that the
  population is the same set. It cannot see the composition root's **private**
  population, so that the composition root holds no NetDB lock across its
  indexing is the **guard's** claim, and the guard's claim is a static one.
- The plan's effect on the lock's **contention** is structural rather than
  measured: no timing assertion is made, because a timing assertion under a
  shared disk would be flaky and would prove less than the shape does. The
  lane's `try_lock` is a deterministic observation, not a stopwatch.
- The store's selection copy still happens under the router's NetDB lock, and
  the store's prune remains O(population) under it. The plan does not claim
  otherwise, and both keep their named owner.
- The three **duplicated** Plan 538 sections this plan removed were a
  documentation defect, not a behavioural one: the first copy of each was
  already correct, and every citation that named it still resolves.

## Findings by severity

### None — nothing was found that blocks the plan

### Medium — discharged

- **The router's NetDB lock wrapped the routing table's mutex.** The
  composition root walked every floodfill advertiser the NetDB holds **under**
  the router's NetDB lock, and the walk's body called the routing table's own
  `index`, which takes the routing table's own mutex. The population ran at
  startup and again on **every** tick of the participation maintenance owner, so
  the NetDB discovery tick, the publication tick, the peer reconcile, the peer
  pool and the relay roles all queued behind it, and the routing table's own
  lookups queued behind the NetDB lock. **Discharged in this plan**: the
  population is a short bounded read whose guard is released before it returns,
  and the indexing happens with no NetDB lock held.
- **The population's absence was a silent skip.** A router that held no
  `Bootstrap` returned without indexing, without refusing and without counting,
  so a population that could not be read left no trace at all. **Discharged in
  this plan**: the absence is the typed refusal `StoreUnavailable`, counted
  under its own reason in the participation owner's counter.
- **The lane could hang instead of failing.** The lane took the bootstrap's lock
  **blocking** wherever it wanted to *hold* it, so the control for a leaked
  guard blocked for ever and never reported. A lane that can hang is not
  evidence. **Discharged in this plan**: every lock the lane wants to hold is
  taken with `try_lock`, so a leaked guard fails the lane **immediately**, and
  NC-12 fires.

### Low — recorded by this plan

- **The router's validated store is still the bootstrap's store.**
  (`crates/i2pr-daemon/src/bootstrap.rs:215`) This is Plan 536's recorded
  **Low** finding 1, carried forward unchanged. **Owner:** the successor that
  gives the router a NetDB owner of its own.
- **The store's prune is still O(population) under the router's NetDB lock.**
  A prune must examine every record, so this is inherent to the operation.
  **Owner:** the same successor.
- **The composition root's shape is proven statically, not behaviourally.**
  This is Plan 536's recorded **Low** finding 2, carried forward unchanged.
  **Owner:** the successor that makes the composition root's relay arm
  reachable from a test.
- **The production SSU2 peer-test driver** — Plan 530's Low finding 2.
- **The `B` advertisement and `caps=f`** — their own owners.
- **The stock-peer half** — Plan 493's authorized two-host topology.

## The lane's own fail-fast repair

The negative control for a leaked guard found a defect in the plan's **own
evidence**, and the repair is recorded in the plan's amendment log, row 17.2.5,
as well as in the plan's section 11.1, which is settled to the method the lane
actually uses.

The lane's first draft took the bootstrap's lock **blocking** wherever it wanted
to *hold* it. The mutation that leaks the guard therefore left the mutex locked
for ever, and the lane's own tests blocked on it instead of failing. The
control hung for ten minutes and never reported, because the control has to see
the row **fail**: a hang is not a result.

**What was done.** Every lock the lane wants to hold is now taken with
`try_lock`, which fails **immediately** rather than blocking. The ten rows are
unchanged in what they prove — the release proof was already a `try_lock`
observation — and they now fail fast instead of deadlocking. The poisoning test
keeps its blocking lock on purpose, because that test poisons the mutex itself.

**What was deliberately not done.** The control's timeout was **not** used to
turn a hang into a pass. A control that times out reports **nothing**, and a
control that reports nothing is not evidence; the lane was made to fail instead.

## The duplicated Plan 538 sections

Plan 538's documentation commit `74302efe` applied its patches **twice**, and
three documents carried two **identical** copies of Plan 538's section:
`specs/QUALIFICATION-GATES.md`'s `## 37.`, `docs/architecture/i2pr-netdb.md`'s
section and `docs/architecture/i2pr-daemon.md`'s section.

**What was done.** Only the **second** copy of each was removed, so every
citation that names the first copy — including Plan 538's closure record, which
is **not** rewritten — still resolves to the same line. The correction is
recorded in this plan's amendment log, row 17.2.3, and in this record.

**What was deliberately not done.** Plan 538's closed closure record is not
rewritten, and its committed documentation is not rewritten either: the
duplicated sections are removed rather than reworded, so the first copy's text
is exactly what Plan 538 published.

## Roadmap disposition

The **network-participation** roadmap's row for Plan 539 moves from `registered`
to `closed`, and the registry's row moves from `registered` to `closed`. No
roadmap row is reopened, renumbered or rewritten, and no other plan's row
changes.

The plan **advances** the network-participation line's lock-contention work:
Plan 536 removed the relay target's answer from the router's NetDB lock, Plan
537 removed the relay requester's outbound step from it, Plan 538 removed the
daemon's bootstrap's **population walk** from it, and Plan 539 removed the
**composition root's population** from it. The line's remaining named owner is
unchanged: the successor that gives the router a NetDB owner of its own.

## Unblock audit

- **Plan 539's own findings** are discharged above, and the ones that remain
  have a named owner.
- **Plan 536's recorded Low finding 1** — the router's validated store is still
  the bootstrap's store — is **carried forward unchanged**, with the same owner.
- **Plan 536's recorded Low finding 2** — the composition root's shape is
  proven statically — is **carried forward unchanged**, with the same owner.
- **Plan 537's and Plan 538's named out-of-scope items** — the NetDB ownership,
  the composition root's reachability and the store's prune — are **carried
  forward unchanged**, with the same owner.
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
- Plan 536's, Plan 537's and Plan 538's closed closure records are **not**
  rewritten. The discharge of their named out-of-scope items lives in this
  plan's closure record and in the registry's row.
- The plan **extends** no other plan's guard. Plan 536's, Plan 537's and Plan
  538's lanes and guards stay byte-identical, and the guard pins all six
  digests.
- Plan 538's duplicated documentation sections are removed by this plan, and
  Plan 538's committed documentation is **not** reworded.
- A successor must **not** weaken the lane's `try_lock` usage: a lane that can
  block is a lane whose controls cannot report.
- The tree's next step in this line is the successor that gives the router a
  **NetDB owner of its own**. That change is larger than this plan and is not
  attempted here.
- **The shared disk is the machine's tightest resource.** The floor's first run
  on this tree failed one step with a **linker bus error** -- the disk was at
  100% with 253 MiB free -- not with a code defect. The floor was re-run from
  a **cleaned** build cache (`cargo clean` released 67.5 GiB) and every step
  passed. The cache lives outside the repository at
  `/home/sugarwookie/.cache/i2pr-plan535-target`, and `CARGO_INCREMENTAL=0` and
  `CARGO_GC_AUTO_FREQUENCY=never` are set, so a successor's builds do not add
  to the pressure. Most of the disk is **other** sessions' data; do not delete
  it.

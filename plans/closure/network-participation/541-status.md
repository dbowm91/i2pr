# Plan 541 — the router's floodfill selection must not be narrowed by the group provider's bound

**Closure record.** The plan of record is
`plans/implementation/network-participation/541-the-routers-floodfill-selection-must-not-be-narrowed-by-the-group-providers-bound.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 541 is **closed**. The router's **authoritative** floodfill selection
store — the `DestinationTunnelCoordinator`'s store, which `candidate_hashes`
and `version_gated_candidate_hashes` read — now holds the router's floodfill
**advertisers**, and not an arbitrary hash-ordered prefix of the NetDB. The
store maintains its own **advertiser index**, and the selection walks that
index instead of the record map.

The selection's **observable** behaviour is unchanged: it still filters on the
record's own `f` capability and still yields the advertisers in ascending
`RouterHash` order. What changed is **which** peers the selection can see at
all.

Classification: **corrective** (the router's authoritative floodfill selection
store was fed `Bootstrap::validated_router_info_snapshot()` — the **first 256
records in ascending `RouterHash` order**, a bound that belongs to the **group
provider**'s RouterInfo supply — so the selection store held an arbitrary
subset of the NetDB and normally held **no** advertiser at all, and the router
then failed closed with `NoEligibleFloodfill` while its own NetDB held
thousands of floodfill advertisers). **Subsystem:** network-participation.
**Closure date:** 2026-11-11.

Nothing is promoted: `router.version` stays `0.9.62`, the inventory's
advertised count stays **zero**, and `Cargo.toml` and `Cargo.lock` stay
byte-identical.

## What the plan closed

**The store maintains an advertiser index.** `RouterInfoStore`
(`crates/i2pr-netdb/src/store.rs:83`) carries a **private** field
`advertisers: BTreeSet<RouterHash>`
(`crates/i2pr-netdb/src/store.rs:111`) beside its record map
(`crates/i2pr-netdb/src/store.rs:84`) and its age index
(`crates/i2pr-netdb/src/store.rs:98`). The field is private, so the store's
public surface is unchanged.

**The index is maintained on exactly the paths that maintain the record map and
the floodfill advertiser count.** The insert path is at
`crates/i2pr-netdb/src/store.rs:155` and the removal path at
`crates/i2pr-netdb/src/store.rs:249`:

- the **new-key** path notes the incoming record's capability
  (`crates/i2pr-netdb/src/store.rs:230`);
- the **replacement** path notes it again
  (`crates/i2pr-netdb/src/store.rs:206`), because a replacement may carry a
  different capability set;
- the **removal** path releases the entry with the record
  (`crates/i2pr-netdb/src/store.rs:261`).

The note itself is one private helper
(`crates/i2pr-netdb/src/store.rs:274`). It is **idempotent**, so the count and
the set can never disagree about which records are advertisers, and it is made
on **every** replacement rather than only when the capability changed. The
refusal paths — the capacity refusal, the idempotent insert and the conflict —
change nothing, so they change no entry. The index is **never rebuilt by a
scan**.

**The selection walks the index.** `floodfill_advertisers()`
(`crates/i2pr-netdb/src/store.rs:379`) walks the maintained index instead of
the record map, so the walk is proportional to the number of advertisers and
not to the population.

**The selection still filters on the capability.** The index's word never
replaces the record's own evidence: a peer whose own signed `RouterInfo` does
not advertise `f` is never yielded, whatever the index says. The filter is
what makes the selection **exact**, and it is the defence in depth for the
index's exactness. The selection's **order** is unchanged — the index is
ordered, so the advertisers are still yielded in ascending `RouterHash` order.

**The advertiser snapshot is the selection's copy.**
`floodfill_advertiser_snapshot()`
(`crates/i2pr-netdb/src/store.rs:397`) publishes the advertisers' records, and
`Bootstrap::floodfill_advertiser_snapshot()`
(`crates/i2pr-daemon/src/bootstrap.rs:356`) exposes them. The copy is bounded
by the store's own record ceiling.

**The coordinator's seed carries the advertisers.** The Destination-groups seed
reads `bootstrap.floodfill_advertiser_snapshot()`
(`crates/i2pr-daemon/src/lib.rs:4433`), not the group provider's prefix.

**The reconcile's coordinator feed carries the advertisers.** The reconcile
takes the advertiser snapshot under the bootstrap's lock and **releases** it
(`crates/i2pr-daemon/src/peer_reconcile.rs:482`), and writes the advertisers
into the coordinator's store through the canonical write API
(`crates/i2pr-daemon/src/peer_reconcile.rs:510`), called from
(`crates/i2pr-daemon/src/peer_reconcile.rs:367`). The two locks are never held
at once, a refusal at capacity is counted, and the batch continues, so one
peer at capacity never aborts the reconcile.

**The group provider's bound is unchanged.**
`MAX_GROUP_PROVIDER_ROUTER_INFOS` still bounds the group's snapshot
(`crates/i2pr-daemon/src/bootstrap.rs:53`), the group's own snapshot still
takes the prefix
(`crates/i2pr-daemon/src/bootstrap.rs:341`), and the three other consumers of
the bounded snapshot — the controlled NTCP2 lookup's candidate pool, the
peer-profile learn cadence and the controlled NTCP2 dial — keep it.

## What the plan did not close

- **The router's validated store is still the bootstrap's store.** The bootstrap
  owns the only `RouterInfoStore` in the daemon
  (`crates/i2pr-daemon/src/bootstrap.rs:215`). Separating the router's NetDB
  from the bootstrap's lifetime is a larger change and was not attempted here.
  This is Plan 536's recorded **Low** finding 1, carried forward unchanged.
  **Owner:** the successor that gives the router a NetDB owner of its own. This
  plan **reduces** what that successor must do: once the selection sees the
  advertisers, the lock's readers are not silently starved of the peers they
  select from.
- **The advertiser snapshot is still a copy taken under the router's NetDB
  lock.** The snapshot is a copy of the advertisers' records. **Owner:** the
  same successor, whose NetDB owner could hand the selection the records it
  needs without a copy.
- **The peer-profile store's feed is still the bounded prefix.** The profiles
  learn an arbitrary hash-ordered subset of the NetDB. **Owner:** the same
  successor.
- **The composition root's shape is proven statically, not behaviourally.** The
  wiring that carries the advertisers from the NetDB to the coordinator's store
  is private to the daemon's own modules, so the lane cannot see it and the
  **shape** is the guard's claim. **Owner:** the successor that makes the
  composition root's relay arm reachable from a test.
- **The production SSU2 peer-test driver** — Plan 530's Low finding 2.
- **The `B` advertisement and `caps=f`** — their own owners.
- **The stock-peer half** — Plan 493's authorized two-host topology.

## Implementation commits

| Commit | What it carries |
| --- | --- |
| `d73deb56` | the plan of record, the registry's row and the roadmap's row |
| `e0b9d853` | the production change: the store's advertiser index, its three maintenance paths, the selection that walks it, the advertiser snapshot, and the daemon's seed and reconcile feed — together with the lane and the guard |
| `36b16161` | the documentation, the inventory, the plan's amendment log and the citation-drift pointer tables |

The floor ran on the committed tree `36b16161`. The closure commit adds **only**
this record and the two closed rows, exactly as Plan 538's, Plan 539's and Plan
540's closure commits did.

The plan of record's **invariants** are at
`plans/implementation/network-participation/541-the-routers-floodfill-selection-must-not-be-narrowed-by-the-group-providers-bound.md:149`,
its **required production changes** at
`plans/implementation/network-participation/541-the-routers-floodfill-selection-must-not-be-narrowed-by-the-group-providers-bound.md:243`,
its **design decisions** at
`plans/implementation/network-participation/541-the-routers-floodfill-selection-must-not-be-narrowed-by-the-group-providers-bound.md:296`,
its **lane** at
`plans/implementation/network-participation/541-the-routers-floodfill-selection-must-not-be-narrowed-by-the-group-providers-bound.md:413`,
its **guard** at
`plans/implementation/network-participation/541-the-routers-floodfill-selection-must-not-be-narrowed-by-the-group-providers-bound.md:437`,
its **closure evidence** at
`plans/implementation/network-participation/541-the-routers-floodfill-selection-must-not-be-narrowed-by-the-group-providers-bound.md:517`,
its **amendment log** at
`plans/implementation/network-participation/541-the-routers-floodfill-selection-must-not-be-narrowed-by-the-group-providers-bound.md:582`,
the netdb deep-dive's account of the advertiser index at
`docs/architecture/i2pr-netdb.md:1543`, the daemon deep-dive's account of the
coordinator's feed at `docs/architecture/i2pr-daemon.md:4532`, the qualification
gates at `specs/QUALIFICATION-GATES.md:2228` and the conformance account at
`specs/CONFORMANCE.md:4663`.

## Requirement-to-evidence matrix

| # | Requirement | Evidence |
| --- | --- | --- |
| 1 | the coordinator's authoritative store holds the router's floodfill advertisers | the lane's rows `the_selection_yields_exactly_the_advertisers`, `an_advertiser_outside_the_prefix_is_still_reachable` and `the_coordinator_selects_the_advertiser_it_was_fed`; the guard's rule `the_coordinator_is_fed_the_advertisers` |
| 2 | the advertiser index is maintained, not derived | the guard's rule `the_store_maintains_an_advertiser_index`; the production change at `crates/i2pr-netdb/src/store.rs:206`, `crates/i2pr-netdb/src/store.rs:230` and `crates/i2pr-netdb/src/store.rs:261` |
| 3 | the index is exact | the lane's rows `the_index_does_not_drift_on_an_insert` and `a_replacement_that_gains_the_capability_is_selected`; the guard's rule `the_store_maintains_an_advertiser_index` |
| 4 | the selection still filters on the capability | the lane's row `a_non_advertiser_is_never_selected`; the guard's rule `the_selection_walks_the_index_and_still_filters`; the production change at `crates/i2pr-netdb/src/store.rs:379` |
| 5 | the selection's order is unchanged | the lane's row `the_selection_order_is_unchanged` |
| 6 | the store's public surfaces are unchanged | the guard's rule `the_store_maintains_an_advertiser_index`; the lane's row `a_replacement_that_loses_the_capability_is_not_selected` |
| 7 | the byte accounting and the floodfill advertiser count are unchanged | the production change at `crates/i2pr-netdb/src/store.rs:289`; the guard's rule `the_store_maintains_an_advertiser_index` |
| 8 | the group provider's bound and the three other consumers are unchanged | the guard's rule `the_group_providers_bound_is_unchanged`; the production change at `crates/i2pr-daemon/src/bootstrap.rs:53` |
| 9 | the coordinator's seed carries the advertisers | the guard's rule `the_coordinator_is_fed_the_advertisers`; the production change at `crates/i2pr-daemon/src/lib.rs:4433` |
| 10 | the reconcile's coordinator feed carries the advertisers | the guard's rule `the_coordinator_is_fed_the_advertisers`; the production change at `crates/i2pr-daemon/src/peer_reconcile.rs:367` |
| 11 | no configuration key is added, and no dependency is added | the guard's rule `no_new_key_and_nothing_is_advertised`, and the two digests below |
| 12 | no new task, timer, listener or socket | the guard's rule `no_new_key_and_nothing_is_advertised` |
| 13 | no advertisement | the guard's rule `no_new_key_and_nothing_is_advertised`; `crates/i2pr-netdb/src/local.rs:172` |
| 14 | Plan 506's, Plan 507's, Plan 538's, Plan 539's and Plan 540's lanes and guards stay byte-identical | the guard's rule `the_predecessors_evidence_is_untouched`, and the ten digests below |

The ten digests the guard pins are Plan 506's lane
(`ffabd287ec94cdcd044e58784b97dd5ae9f920ce016627d2792f87801b5ef28f`) and guard
(`33db28ccc76a52338b381b20c9d44ca2df9152d90d4f56a29a4279ad4c4eaafc`), Plan
507's lane
(`709ec1b47e731bd71edb7ca65f4ef889e4695f36cf5a012cf7d70b5ceae2c5da`) and guard
(`5ef83044dcb8036e0ac2ccc8e7e9cb9bf6b7a21127b822b6e634da6170fdd78d`), Plan
538's lane
(`d1ce001d1e7562ca84b33cd9b4b0bca04325565b7cfef2c7504645825c8bc613`) and guard
(`3d059e0690a0ab52b5c54619c0b5220eac8f340168187c2541f12f597c518fa4`), Plan
539's lane
(`068410ef125b2122b1acf56866abff78010f6fe20dc4ef933d80a7c4e6d99103`) and guard
(`60ef63b653835cbb1af7943aea64662fe6970eed5827b1266a92994dea2ae300`), and Plan
540's lane
(`369a69a0ce776fbb44f3a0bfec164a7296755e19ccea01f3f0fd7ca6a6739468`) and guard
(`da81058d9e32b1e36332e3f031788c93b87c1430dcbb1d753759c227634586a4`). A later
change to any of them must change the pinned constant **and** say why.

The manifest and the lock are byte-identical to the tree Plan 540 closed on, and
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
| `cargo test --locked -p i2pr-daemon --test floodfill_selection_visibility -- --test-threads=1` | PASS — 10 rows |
| `cargo test --locked -p i2pr-daemon --test floodfill_selection_visibility_boundary -- --test-threads=1` | PASS — 13 rows |
| `cargo test --locked -p i2pr-daemon --test floodfill_selection_visibility --test floodfill_selection_visibility_boundary --test floodfill_statistic_accounting --test floodfill_statistic_accounting_boundary --test peer_reconcile_boundary --test netdb_retention_boundary --test netdb_prune_walk --test netdb_prune_walk_boundary --test routing_table_population_lock --test routing_table_population_lock_boundary -- --test-threads=1` | PASS — 10 rows, 13 rows, 14 rows, 32 rows, 28 rows, 26 rows, 10 rows, 13 rows, 10 rows and 24 rows |
| `cargo test --locked -p i2pr-daemon --lib -- --test-threads=1` | PASS — 1001 rows, 3 ignored |
| `cargo test --locked -p i2pr-netdb --all-targets` | PASS |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 scripts/check-license-metadata.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 51 tests |
| `python3 /tmp/541-negative.py` | PASS — 16 controls, each firing the surface that owns the defect, each restored byte-identically |

The closure record's **citations** are verified mechanically, not by reading.
The verifier recomputes every cited line from the tree and requires the record
to name exactly that line:

- the **closure record's** citations are recomputed against the **current
  tree** — the line that carries the cited text is the line the record must
  name;
- the **plan of record's** section 3 citations are recomputed against the
  **registration commit** `d73deb56`, because section 3 is the evidence of the
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

The floor ran on the committed tree `36b16161` and it is **green**. Every
step is recorded in `/tmp/541-floor.log`, with each step's own log under
`/tmp/541f-<step>.log`. The floor is a plain sequence of `step` runs that
records each step's exit status; no step is skipped, no `|| true`, no
`continue-on-error` and no filename filtering. The workspace step runs with
`--no-fail-fast`, so every test binary's result is reported rather than the
run stopping at the first failure.

Plan 541's floor is Plan 540's floor plus two steps -- the daemon lane and the
daemon guard for the router's floodfill selection. **No other step changed**,
and **no other plan's guard file is touched**: Plan 541 extends no
predecessor's guard, and Plan 506's, Plan 507's, Plan 538's, Plan 539's and
Plan 540's lanes and guards stay byte-identical.

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
| `plan541-floodfill-selection-lane` | PASS |
| `plan541-floodfill-selection-boundary` | PASS |
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

The floor's own summary is **121 passed, 0 failed**, and the table above lists every step it ran.

The floor's own summary lines are:

```text
FLOOR PASS 121
FLOOR FAIL 0
FLOOR ALL GREEN
```

## The negative controls

Sixteen controls mutated the **real tree**, ran the lane or the guard, and
observed the expected row fail. Every control was then restored from the bytes
saved before the mutation, and the SHA-256 of every touched file was compared
with the value it had before, so the tree is provably byte-identical
afterwards. Every control **compiles**: a control that broke the build would
prove nothing about the row it names.

Every control names exactly **one** row and runs that row **by name**, so a
control cannot pass because some *other* row failed. A mutation that breaks
the build, a row that passes, and a tree that is not restored are all
failures.

| Control | What it restored | What it broke |
| --- | --- | --- |
| NC-1 | the new-key path does not index the advertiser | the lane's row `the_index_does_not_drift_on_an_insert` |
| NC-2 | the replacement path does not index the advertiser | the lane's row `a_replacement_that_gains_the_capability_is_selected` |
| NC-3 | the advertiser snapshot is the group provider's prefix | the lane's row `the_snapshot_carries_the_advertisers_and_not_the_prefix` |
| NC-4 | the selection walks the index backwards | the lane's row `the_selection_order_is_unchanged` |
| NC-5 | the selection drops the capability filter | the guard's rule `the_selection_walks_the_index_and_still_filters` |
| NC-6 | the removal path does not release the advertiser's entry | the guard's rule `the_store_maintains_an_advertiser_index` |
| NC-7 | the selection scans the record map instead of the index | the guard's rule `the_selection_walks_the_index_and_still_filters` |
| NC-8 | the Destination-groups seed keeps the group provider's prefix | the guard's rule `the_coordinator_is_fed_the_advertisers` |
| NC-9 | the reconcile stops feeding the coordinator the advertisers | the guard's rule `the_coordinator_is_fed_the_advertisers` |
| NC-10 | the reconcile's advertiser snapshot reads the prefix | the guard's rule `the_coordinator_is_fed_the_advertisers` |
| NC-11 | the bootstrap publishes the group provider's prefix | the guard's rule `the_coordinator_is_fed_the_advertisers` |
| NC-12 | the group provider's bound moves | the guard's rule `the_group_providers_bound_is_unchanged` |
| NC-13 | a configuration key appears | the guard's rule `no_new_key_and_nothing_is_advertised` |
| NC-14 | the controlled router version moves | the guard's rule `no_new_key_and_nothing_is_advertised` |
| NC-15 | the manifest changes | the guard's rule `no_new_key_and_nothing_is_advertised` |
| NC-16 | a predecessor's guard changes | the guard's rule `the_predecessors_evidence_is_untouched` |

The controls cover **both** halves of the evidence. NC-1 through NC-4 restore
a defect the **lane** is the authority for — the selection's *behaviour* — and
they are the ones that move the **material** rather than the structure, which
is what makes the lane's independent observation worth having. NC-5 through
NC-16 restore a defect the **guard** is the authority for — the *shape* of the
fix and the wiring that carries it.

**Three of the plan's defects are not lane-observable, and the guard is their
authority.** The removal path's index release (NC-6) is not lane-observable,
because a stale index entry names a record that is already gone and the
selection filters on the record's own capability. The **wiring** that carries
the advertisers (NC-8 through NC-11) is not lane-observable either, because the
lane feeds the coordinator itself. The capability **filter** (NC-5) is not
lane-observable, because the maintained index already holds only the
advertisers. The controls are **not** weakened to force a lane row to fail: a
control that the lane cannot observe is a control for the guard, and saying
otherwise would be a false claim about the evidence.

**NC-5 is the control that corrected the plan's own evidence claim.** The
control was first written as a lane control, and the first sweep's log shows
the lane row **passed** with the filter removed — the mutation changed
nothing the lane can see. The control was corrected to a guard control rather
than the lane weakened. The plan's amendment log, row 17.2.1, records this.

## Acceptance criteria

| # | The plan's criterion | Outcome |
| --- | --- | --- |
| 1 | the coordinator's authoritative store holds the router's floodfill advertisers, and an advertiser outside the group provider's prefix is reachable through the live selection | MET — the lane's rows `an_advertiser_outside_the_prefix_is_still_reachable` and `the_coordinator_selects_the_advertiser_it_was_fed`; the guard's rule `the_coordinator_is_fed_the_advertisers`; NC-3, NC-8, NC-9, NC-10 and NC-11 restore the defect |
| 2 | the selection yields exactly the advertisers the store's contents imply, and the lane measures it against an independent computation | MET — the lane's rows `the_selection_yields_exactly_the_advertisers` and `a_non_advertiser_is_never_selected`; NC-1 restores the defect |
| 3 | the index never drifts, and the lane proves it through the selection's own behaviour | MET — the lane's rows `the_index_does_not_drift_on_an_insert`, `a_replacement_that_gains_the_capability_is_selected` and `a_removal_drops_the_advertiser_from_the_selection`; the guard's rule `the_store_maintains_an_advertiser_index`; NC-1, NC-2 and NC-6 restore the defect |
| 4 | the selection still filters on the capability, and the store's public surfaces are unchanged | MET — the lane's row `a_non_advertiser_is_never_selected`; the guard's rules `the_selection_walks_the_index_and_still_filters` and `the_store_maintains_an_advertiser_index`; NC-5 and NC-7 restore the defect |
| 5 | the group provider's bound and the three other consumers are unchanged | MET — the guard's rule `the_group_providers_bound_is_unchanged`; NC-12 restores the defect |
| 6 | no configuration key, dependency, task, timer, listener or socket is added, and `Cargo.toml` and `Cargo.lock` stay byte-identical | MET — the guard's rule `no_new_key_and_nothing_is_advertised`, and the two digests above; NC-13, NC-14 and NC-15 restore the defect |
| 7 | `router.version` stays `0.9.62`, and the inventory's advertised count stays **zero** | MET — the guard's rule `no_new_key_and_nothing_is_advertised`, and `crates/i2pr-netdb/src/local.rs:172`; NC-14 restores the defect |
| 8 | the full floor is green, with the closure record's table complete | MET — the table above |
| 9 | the guard's inverted probes each fail as intended and the tree is restored byte-identically after each | MET — the guard's inverted probes `the_inverted_probe_fires_when_the_index_is_not_maintained`, `the_inverted_probe_fires_when_the_removal_does_not_release_the_entry`, `the_inverted_probe_fires_when_the_selection_scans_the_record_map`, `the_inverted_probe_fires_when_the_selection_drops_the_filter`, `the_inverted_probe_fires_when_the_seed_keeps_the_prefix`, `the_inverted_probe_fires_when_a_configuration_key_appears` and `the_inverted_probe_fires_when_a_predecessor_changes`, and the sixteen controls above |

## Invariant review

| # | The plan's invariant | Review |
| --- | --- | --- |
| 1 | the coordinator's authoritative store holds the router's floodfill advertisers | **Held.** The seed reads `bootstrap.floodfill_advertiser_snapshot()` (`crates/i2pr-daemon/src/lib.rs:4433`) and the reconcile feeds the advertisers through the canonical write API (`crates/i2pr-daemon/src/peer_reconcile.rs:367`). The lane's rows `an_advertiser_outside_the_prefix_is_still_reachable` and `the_coordinator_selects_the_advertiser_it_was_fed` observe the outcome from the coordinator's own store. |
| 2 | the advertiser index is maintained, not derived | **Held.** The three maintenance points are `crates/i2pr-netdb/src/store.rs:206`, `crates/i2pr-netdb/src/store.rs:230` and `crates/i2pr-netdb/src/store.rs:261`, and the guard's rule `the_store_maintains_an_advertiser_index` requires the insert path to carry **two** notes and the removal path to carry a release, and forbids a record-map scan in any of the three. |
| 3 | the index is exact | **Held.** The lane's rows `the_index_does_not_drift_on_an_insert` and `a_replacement_that_gains_the_capability_is_selected` observe the index from the selection's own behaviour, and NC-1, NC-2 and NC-6 restore the three ways it can drift. |
| 4 | the selection still filters on the capability | **Held.** `floodfill_advertisers()` keeps its `advertises_floodfill()` filter at `crates/i2pr-netdb/src/store.rs:379`, and the guard's rule `the_selection_walks_the_index_and_still_filters` forbids the filter's removal. NC-5 removes the filter and fires the rule. |
| 5 | the selection's order is unchanged | **Held.** The index is ordered, so the advertisers are still yielded in ascending `RouterHash` order. The lane's row `the_selection_order_is_unchanged` observes it, and NC-4 reverses the walk and fires the row. |
| 6 | the store's public surfaces are unchanged | **Held.** The store's new field and its new helper are **private**, and `prune`, `insert`, `remove`, `get`, `contains`, `len`, `is_empty`, `encoded_bytes`, `config`, `stats`, `iter` and `floodfill_advertisers` keep their signatures. The only **public** addition is `floodfill_advertiser_snapshot()`, which is new and additive. The guard's rule `the_store_maintains_an_advertiser_index` pins the selection's body. |
| 7 | the byte accounting and the floodfill advertiser count are unchanged | **Held.** The count is still maintained by the same three decision points — `crates/i2pr-netdb/src/store.rs:289` is the adjustment helper — and the plan adds no accounting. The lane's row `a_replacement_that_loses_the_capability_is_not_selected` observes that the count follows the capability. |
| 8 | the group provider's bound is unchanged | **Held.** `MAX_GROUP_PROVIDER_ROUTER_INFOS` still bounds the group's snapshot (`crates/i2pr-daemon/src/bootstrap.rs:53`), the group's snapshot still takes the prefix (`crates/i2pr-daemon/src/bootstrap.rs:341`), and the three other consumers keep it. The guard's rule `the_group_providers_bound_is_unchanged` pins all of it. NC-12 moves the bound and fires the rule. |
| 9 | the coordinator's seed carries the advertisers | **Held.** The seed reads `bootstrap.floodfill_advertiser_snapshot()` (`crates/i2pr-daemon/src/lib.rs:4433`), and the guard's rule `the_coordinator_is_fed_the_advertisers` forbids the prefix there. NC-8 restores the prefix and fires the rule. |
| 10 | the reconcile's coordinator feed carries the advertisers | **Held.** The reconcile reads the advertisers under the bootstrap's lock and releases it (`crates/i2pr-daemon/src/peer_reconcile.rs:482`), and writes them through `insert_validated_router_info` (`crates/i2pr-daemon/src/peer_reconcile.rs:510`). The peer-profile store's feed is unchanged. NC-9, NC-10 and NC-11 restore the defect. |
| 11 | no configuration key is added, and no dependency is added | **Held.** The guard's rule `no_new_key_and_nothing_is_advertised` rejects the tokens `prune`, `retention`, `age_index`, `prune_budget`, `advertiser_index` and `floodfill_selection` in the daemon's configuration, and requires the manifest's and the lock's digests to be unchanged. NC-13 and NC-15 restore the defect. |
| 12 | no new task, timer, listener or socket | **Held.** The guard's rule `no_new_key_and_nothing_is_advertised` rejects `tokio::`, `std::net`, `std::fs`, `std::thread`, `JoinHandle` and `spawn(` in the changed module, and the plan adds no runtime construct anywhere. The feed rides the reconcile the product already drives. |
| 13 | no advertisement | **Held.** `crates/i2pr-netdb/src/local.rs:172` still declares `0.9.62`, and the inventory advertises nothing: the guard's rule `no_new_key_and_nothing_is_advertised` requires that no surface in `specs/support.toml` carries `advertised = true`. NC-14 restores the defect. |
| 14 | Plan 506's, Plan 507's, Plan 538's, Plan 539's and Plan 540's lanes and guards stay byte-identical | **Held.** The guard's rule `the_predecessors_evidence_is_untouched` pins all ten digests, and NC-16 restores a predecessor's guard and fires it. |

## Failure, cancellation, restart, and contention review

**Failure.** The change touches no error path. The store's failure modes are
exactly the ones it had: a store at capacity, a record that cannot be
accounted, and a snapshot that cannot be taken. The advertiser feed writes
through the canonical write API, so a refusal at capacity is **counted** and
the batch continues — one peer at capacity never aborts the reconcile. The
bootstrap's lock is taken for the advertiser snapshot and released before the
coordinator's lock is taken, so a poisoned bootstrap lock is a **source
unavailable** condition the reconcile already counts, not a panic.

**Cancellation.** The change touches no async path beyond the reconcile's own
existing awaited calls. The advertiser snapshot is taken under the bootstrap's
lock and released before the coordinator's lock is taken; no lock is held
across an await, and the two locks are never held at once.

**Restart.** Nothing is persisted. The advertiser index is derived from the
store's own contents and is rebuilt from scratch when the store is
constructed, so a restart cannot inherit a stale index. The index is not a
cache of anything the tree persists.

**Contention.** The router's NetDB lock is now held for a walk proportional to
the number of advertisers rather than to the population, and the selection
store is fed the advertisers, so the readers that queue behind the lock are no
longer starved of the peers they select from. The advertiser snapshot is still
a **copy** taken under that lock, and the router's validated store is still
the bootstrap's store; both keep their named owner in **What the plan did not
close**.

## Migration and compatibility review

**No wire change.** The selection yields the same peers it always yielded for
a peer it could already see, in the same order.

**No configuration migration.** **No key is added.** The daemon's configuration
surface is unchanged, and the NetDB configuration keeps exactly its keys.

**No persisted state.** The index lives inside the store and is rebuilt with
it, so there is nothing to migrate.

**No change to the netdb crate's public surface.** No `i2pr-netdb` item is
removed or renamed. The store's new field and its new helper are **private**,
and the prune's signature and the store's other public signatures are
unchanged. The one **public** addition is `floodfill_advertiser_snapshot()`,
which is additive and carries the selection's own result.

**No change to the daemon's public surface.** The bootstrap's new accessor is
additive; the reconcile's new methods are private.

**No change to the manifest or the lock.** `Cargo.toml` and `Cargo.lock` are
byte-identical to the tree Plan 540 closed on, and the digests above are the
tree's. No requirement is added to any manifest.

## Security review

**No secret is touched.** The index holds router hashes. Nothing in this plan
reads, writes or derives from a private key, a signing seed or a session
secret.

**No new disclosure.** The index is derived from records the router already
holds, and it is **private** to the store. The advertiser snapshot is a copy of
records the router already holds in its own NetDB, and it travels over the
router's own in-process pipes, not over a socket.

**No new surface is reachable.** The plan adds no listener, no port, no socket
and no configuration key, so it opens no new way in. The selection store is
still loopback-only and internal.

**No peer is blamed.** The plan writes nothing into a peer's profile and
creates no peer-side condition. A peer that does not advertise `f` is still
never selected, and a peer whose capability the router cannot compare is still
refused by the eligibility gate that Plan 491 closed.

**Fail closed.** The guard fails closed on every item it names: an index that
is not maintained, a selection that scans the record map, a selection that
drops the filter, a feed that carries the prefix, a bound that moved, a
configuration key, a moved version, a changed manifest and a changed
predecessor's evidence each make the rule that names them fail. The lane fails
closed too: the selection's exact set, its count, its order and the
snapshot's membership are all asserted against an independently computed
expectation, so a selection that yielded too little or too much fails rather
than passes.

## Limitations

- The lane proves the selection's **behaviour**: that it yields exactly the
  advertisers the store's contents imply, that the index never drifts, that a
  non-advertiser is never selected, that the order is unchanged and that an
  advertiser outside the group provider's prefix is reachable. It cannot see
  the selection's **shape** — that it walks the index and not the record map —
  so that is the **guard's** claim, and the guard's claim is a static one.
- The lane cannot see the **wiring** that carries the advertisers from the
  NetDB to the coordinator's store: it feeds the coordinator itself. The
  wiring's shape is the **guard's** claim, and it is a static one.
- The plan does not claim that the router now **publishes** or **builds**
  anything it could not before. It claims that the router's selection store
  now **holds** the advertisers its selection reads. Whether the router's
  publication and its destination build hops succeed against a live network is
  a stock-peer question, and the stock-peer half is blocked externally on
  Plan 493's authorized two-host topology.
- The plan's effect on the router's NetDB lock's **contention** is structural
  rather than measured: no timing assertion is made, because a timing
  assertion under a shared disk would be flaky and would prove less than the
  shape does.
- The **citation drift** this plan's production change caused in
  `crates/i2pr-netdb/src/store.rs`, `crates/i2pr-daemon/src/peer_reconcile.rs`,
  `crates/i2pr-daemon/src/bootstrap.rs` and `crates/i2pr-daemon/src/lib.rs` is
  recorded in the plan's amendment log as **additive pointer tables**. The
  closed closure records are not rewritten, so a reader who follows an old
  line number finds the pointer rather than the construct.

## Findings by severity

### None — nothing was found that blocks the plan

### Medium — discharged

- **The router's authoritative floodfill selection store held an arbitrary
  subset of the NetDB, and normally held no advertiser at all.** The store was
  fed `Bootstrap::validated_router_info_snapshot()` — the **first 256 records
  in ascending `RouterHash` order** — and that bound belongs to the **group
  provider**'s RouterInfo supply. The floodfill advertisers are distributed
  across the hash space, so the selection store's advertisers were whatever the
  hash order happened to select, and the router then refused its own LeaseSet2
  publication and its destination build hops with `NoEligibleFloodfill` while
  its own NetDB held thousands of floodfill advertisers. **Discharged in this
  plan**: the store maintains an advertiser index, the selection walks it, and
  the seed and the reconcile's feed carry the **advertisers**.
- **The plan's own negative control for the selection's capability filter was
  not observable.** The control was first written as a lane control, and the
  maintained index already holds only the advertisers, so removing the filter
  changed nothing the lane can see — a control that does not change the
  outcome proves nothing. **Discharged in this plan**: the control is a guard
  control, tripping `the_selection_walks_the_index_and_still_filters`, and
  the first sweep's log is the evidence. The plan's amendment log, row 17.2.1,
  records this.

### Low — recorded by this plan

- **The router's validated store is still the bootstrap's store.**
  (`crates/i2pr-daemon/src/bootstrap.rs:215`) This is Plan 536's recorded
  **Low** finding 1, carried forward unchanged. **Owner:** the successor that
  gives the router a NetDB owner of its own.
- **The advertiser snapshot is still a copy taken under the router's NetDB
  lock.** (`crates/i2pr-daemon/src/peer_reconcile.rs:482`) **Owner:** the same
  successor.
- **The peer-profile store's feed is still the bounded prefix.**
  (`crates/i2pr-daemon/src/peer_reconcile.rs:484` in the tree Plan 541
  registered) The profiles learn an arbitrary hash-ordered subset of the
  NetDB. **Owner:** the same successor.
- **The composition root's shape is proven statically, not behaviourally.**
  This is Plan 536's recorded **Low** finding 2, carried forward unchanged.
  **Owner:** the successor that makes the composition root's relay arm
  reachable from a test.
- **The production SSU2 peer-test driver** — Plan 530's Low finding 2.
- **The `B` advertisement and `caps=f`** — their own owners.
- **The stock-peer half** — Plan 493's authorized two-host topology.

## The citation drift this plan's production change caused

The plan's production change is **115 insertions and 7 deletions** across
`crates/i2pr-netdb/src/store.rs` (54 and 2),
`crates/i2pr-daemon/src/bootstrap.rs` (11 and 0),
`crates/i2pr-daemon/src/lib.rs` (6 and 1) and
`crates/i2pr-daemon/src/peer_reconcile.rs` (44 and 4), and it moves the lines
that Plans 534's, 538's, 539's and 540's **closed** closure records cite. Those records are **not** rewritten.

**What was done.** The plan's amendment log, section 17.2, carries **additive
pointer tables**: one row per citation, naming the construct, the line the
closed record cited, and the line the same construct carries now. Plan 541's
own closure record cites the **post-change** lines, and the plan's section 3
and section 6 keep their **pre-change** anchors, which the closure record's
verifier checks against the registration commit `d73deb56`.

**What was deliberately not done.** Plans 534's, 538's, 539's and 540's closed
closure records are not rewritten, and their committed prose is not reworded.
The pointer tables are the correction, and they are additive.

Five documentation edits were made **line-count preserving** or **appended**, on
purpose, so that the citations the closed records make into the documents still
resolve to the same line: the netdb deep dive's stale sentence is amended in
place, the count paragraph in `specs/CONFORMANCE.md` leads Plan 541's own
section at the end of the document, the qualification gates' new section is
appended at the end, and both deep dives' new sections are placed after the
plan they follow.

## Roadmap disposition

The **network-participation** roadmap's row for Plan 541 moves from
`registered` to `closed`, and the registry's row moves from `registered` to
`closed`. No roadmap row is reopened, renumbered or rewritten, and no other
plan's row changes.

The plan **advances** the network-participation line's selection work: the
router's authoritative floodfill selection now sees the router's own floodfill
advertisers, so the router can select the floodfills it has actually learned
instead of an arbitrary hash-ordered subset of its NetDB. The line's remaining
named owner is unchanged: the successor that gives the router a NetDB owner of
its own.

## Unblock audit

- **Plan 541's own findings** are discharged above, and the ones that remain
  have a named owner.
- **Plan 536's recorded Low finding 1** — the router's validated store is still
  the bootstrap's store — is **carried forward unchanged**, with the same owner.
- **Plan 536's recorded Low finding 2** — the composition root's shape is
  proven statically — is **carried forward unchanged**, with the same owner.
- **Plan 537's, Plan 538's, Plan 539's and Plan 540's named out-of-scope items**
  — the NetDB ownership, the composition root's reachability and the store's
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
- Plans 534's, 536's, 537's, 538's, 539's and 540's closed closure records are
  **not** rewritten. The discharge of their named out-of-scope items lives in
  this plan's closure record and in the registry's row.
- The plan **extends** no other plan's guard. Plan 506's, Plan 507's, Plan
  538's, Plan 539's and Plan 540's lanes and guards stay byte-identical, and
  the guard pins all ten digests.
- A successor must **not** remove the selection's capability filter to
  "simplify" it, and must not rebuild the advertiser index by a scan: the
  index's exactness and the filter are a selection guarantee, not an
  optimization.
- A successor must **not** widen the group provider's bound to widen the
  selection. The two are different authorities: the bound is the group's
  supply, and the selection's authority is the advertiser set.
- The tree's next step in this line is the successor that gives the router a
  **NetDB owner of its own**. That change is larger than this plan and is not
  attempted here.
- **The shared disk is the machine's tightest resource.** The build cache lives
  outside the repository at
  `/home/sugarwookie/.cache/i2pr-plan535-target`, and `CARGO_INCREMENTAL=0` and
  `CARGO_GC_AUTO_FREQUENCY=never` are set, so a successor's builds do not add
  to the pressure. Most of the disk is **other** sessions' data; do not delete
  it.

# Plan 543 — the participation ledger must not exhaust the exploratory pool

**Closure record.** The plan of record is
`plans/implementation/network-participation/543-the-participation-ledger-must-not-exhaust-the-exploratory-pool.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 543 is **closed**. A peer's participation is now a fact about a
**window**, so the router's exploratory build driver keeps having peers to
build through once it has built through them.

`ParticipationLedger` records how often each peer has been selected, and the
selection **skips** any peer whose counter has reached the cap
(`crates/i2pr-netdb/src/selection.rs:357`). The counter **saturates** at the
cap, and the ledger's only other release was eviction of the **oldest** entry
when it is **full**. The ledger tracks
`MAX_PARTICIPATION_TRACKED_PEERS = 4_096`
(`crates/i2pr-netdb/src/selection.rs:37`) peers while the profile store's
population is the bounded 256-record prefix the reconcile feeds it, so the
population is a few hundred and does not churn — eviction never fired, and no
counter was ever released. After at most `256 × 8 = 2 048` selections the
exploratory build driver's selection
(`crates/i2pr-daemon/src/exploratory_driver.rs:796`), its only live peer
source, was **permanently empty**.

Classification: **corrective** (a participation fact was recorded as a fact
about all time, and the ledger's size bound could not release it because the
population never reached it). **Subsystem:** network-participation.
**Closure date:** 2026-11-11.

Nothing is promoted: `router.version` stays `0.9.62`, the inventory's
advertised count stays **zero**, and `Cargo.toml` and `Cargo.lock` stay
byte-identical.

## What the plan closed

**The window is derived.** `PARTICIPATION_WINDOW_MS`
(`crates/i2pr-netdb/src/selection.rs:181`) is a `const` derived through the
`const fn` `participation_window_secs`
(`crates/i2pr-netdb/src/selection.rs:184`), so it cannot be configured and
cannot be restated.

**The ledger's entry carries an instant.** `ParticipationLedger`
(`crates/i2pr-netdb/src/selection.rs:194`) holds a private
`stamps: BTreeMap<RouterHash, u64>`
(`crates/i2pr-netdb/src/selection.rs:201`) beside its counters, and
`record` (`crates/i2pr-netdb/src/selection.rs:235`) stamps the entry with the
caller's instant. Eviction releases the stamp with the counter.

**The ledger releases an old entry.** `reclaim`
(`crates/i2pr-netdb/src/selection.rs:267`) drops every entry whose last
selection is outside the window, together with its eviction-order slot, and
returns how many it released. `last_recorded_at`
(`crates/i2pr-netdb/src/selection.rs:283`) is the observable consequence: a
released peer has no instant at all.

**The release rides the clock the driver already advances.** The driver calls
`self.ledger.reclaim(now_ms)` inside `advance_time`
(`crates/i2pr-daemon/src/netdb_participation.rs:235`), and both recording
sites stamp the driver's own clock
(`crates/i2pr-daemon/src/netdb_participation.rs:384` and `:440`). The
destination coordinator stamps its own clock too
(`crates/i2pr-daemon/src/destination_tunnels.rs:779`). This is Plan 508's
shape for a fact set that must not grow for ever: the release is a **check on
a tick that already exists**.

**The selection's own surfaces are unchanged.** The skip rule still reads the
counter map (`crates/i2pr-netdb/src/selection.rs:357`), the cap is still
`DEFAULT_MAX_PARTICIPATION = 8`
(`crates/i2pr-netdb/src/selection.rs:40`), and the entry bound is still
`4_096` (`crates/i2pr-netdb/src/selection.rs:37`). A released entry leaves the
map, so a released peer reads zero exactly as a peer that has never been
selected: a peer used **recently** is still skipped, and the plan changes only
how long an over-used peer stays over-used.

## What the plan did not close

- **The peer-profile store's feed is still the bounded prefix.** The
  population still does not churn, and this plan's release is what makes that
  survivable. **Owner:** the successor that gives the router a NetDB owner of
  its own.
- **The exploratory posture is still not dialable against a public
  deployment.** The dial scope is loopback and RFC1918. **Owner:** the
  transport line and Plan 433's blocked composition.
- **The router's validated store is still the bootstrap's store.** Plan 536's
  recorded **Low** finding 1, carried forward unchanged. **Owner:** the same
  successor.
- **The production SSU2 peer-test driver** — Plan 530's Low finding 2.
- **The `B` advertisement and `caps=f`** — their own owners.
- **The stock-peer half** — Plan 493's authorized two-host topology.

## Implementation commits

| Commit | What it carries |
| --- | --- |
| `8200ad23` | the plan of record, the registry's row and the roadmap's row |
| `a42569c3` | the production change: the window, the entry's instant, the reclaim, and the driver's clock — together with the lane and the guard |
| `8ee4f385` | the documentation, the inventory and the plan's amendment log |

The floor ran on the committed tree `8ee4f385`. The closure commit adds **only**
this record and the two closed rows.

The plan of record's **invariants** are at
`plans/implementation/network-participation/543-the-participation-ledger-must-not-exhaust-the-exploratory-pool.md:164`,
its **required production changes** at
`plans/implementation/network-participation/543-the-participation-ledger-must-not-exhaust-the-exploratory-pool.md:216`,
its **design decisions** at
`plans/implementation/network-participation/543-the-participation-ledger-must-not-exhaust-the-exploratory-pool.md:237`,
its **lane** at
`plans/implementation/network-participation/543-the-participation-ledger-must-not-exhaust-the-exploratory-pool.md:315`,
its **guard** at
`plans/implementation/network-participation/543-the-participation-ledger-must-not-exhaust-the-exploratory-pool.md:333`,
its **closure evidence** at
`plans/implementation/network-participation/543-the-participation-ledger-must-not-exhaust-the-exploratory-pool.md:405`,
its **amendment log** at
`plans/implementation/network-participation/543-the-participation-ledger-must-not-exhaust-the-exploratory-pool.md:459`,
the qualification gates at `specs/QUALIFICATION-GATES.md:2297` and the
conformance account at `specs/CONFORMANCE.md:4793`.

## Requirement-to-evidence matrix

| # | Requirement | Evidence |
| --- | --- | --- |
| 1 | a peer inside the window is still skipped | the lane's row `a_peer_inside_the_window_is_still_skipped`; NC-1 and NC-3 restore the defect |
| 2 | a peer outside the window is selectable again | the lane's row `a_peer_outside_the_window_is_selectable_again`; NC-1 restores the defect |
| 3 | the pool is not permanently exhausted | the lane's row `the_pool_is_not_permanently_exhausted`; NC-3 restores the defect |
| 4 | the window restarts from the new selection | the lane's row `a_released_peer_is_recorded_with_the_new_instant`; NC-2 and NC-4 restore the defect |
| 5 | the counter still saturates at the cap | the lane's row `the_counter_still_saturates_at_the_cap`; NC-7 restores the defect |
| 6 | the ledger still respects its entry bound | the lane's row `the_ledger_still_respects_its_entry_bound` |
| 7 | the reclaim is exact and reports what it released | the lane's row `the_reclaim_is_exact_and_counts_what_it_released`; NC-2 and NC-4 restore the defect |
| 8 | the ungated selection is unchanged | the lane's row `the_selection_without_a_ledger_is_unchanged` |
| 9 | the entry carries an instant, `record` stamps it, and eviction releases it | the guard's rule `the_entry_carries_an_instant`; NC-5 restores the defect |
| 10 | the driver reclaims on its own clock, and every recording site stamps it | the guard's rule `the_driver_reclaims_on_its_clock`; NC-1 restores the defect |
| 11 | the window is derived and not restated | the guard's rule `the_window_is_derived_and_not_restated`; NC-6 restores the defect |
| 12 | the cap, the entry bound and the skip rule are unchanged | the guard's rule `the_bounds_and_the_skip_rule_are_unchanged`; NC-7 restores the defect |
| 13 | the selection's other gates are unchanged, and no configuration key is added | the guard's rule `the_selection_keeps_its_gates`; NC-8 restores the defect |
| 14 | no task, timer, listener or socket is added, and nothing is advertised | the guard's rule `nothing_new_runs_and_nothing_is_advertised`; NC-9 and NC-10 restore the defect |
| 15 | the sixteen predecessor lanes and guards stay byte-identical | the guard's rule `the_predecessors_evidence_is_untouched` |

The sixteen digests the guard pins are Plan 505's lane
(`5d15348eca4be8cce785537bf322f07c5fbe9a482ee0668b4f88c5c0c345ff79`) and guard
(`3a0efb877ad7b5626130bccb97450e1a326502acd486a3496a59cd3c43644622`), Plan
506's lane
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
(`60ef63b653835cbb1af7943aea64662fe6970eed5827b1266a92994dea2ae300`), Plan
540's lane
(`369a69a0ce776fbb44f3a0bfec164a7296755e19ccea01f3f0fd7ca6a6739468`) and guard
(`da81058d9e32b1e36332e3f031788c93b87c1430dcbb1d753759c227634586a4`), Plan
541's lane
(`85190546dd7e371ae140cc4e9b964caa32100a3ca44c830986034593b7cfeb41`) and guard
(`351080b5f5f219a5b54a7a732556185e1ef18b108b4e948e5597d68c8b02ded7`), and Plan
542's lane
(`0a0657a444fd5de046bb7163b6d988bc75ec4f30b63770c5b93248b352a8184e`) and guard
(`6a610f3f3af2926870ca4d468f8ce45a80d7ba400c27c6e3eb013461d7689800`). A later
change to any of them must change the pinned constant **and** say why.

The manifest and the lock are byte-identical to the tree Plan 542 closed on, and
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
| `cargo test --locked -p i2pr-daemon --test participation_release -- --test-threads=1` | PASS — 8 rows |
| `cargo test --locked -p i2pr-daemon --test participation_release_boundary -- --test-threads=1` | PASS — 15 rows |
| `cargo test --locked -p i2pr-daemon --test participation_release --test participation_release_boundary --test exploratory_driver_boundary --test netdb_participation_mesh -- --test-threads=1` | PASS — 8, 15, 16 and 1 rows |
| `cargo test --locked -p i2pr-netdb --all-targets` | PASS |
| `cargo test --locked -p i2pr-daemon --test netdb_retention_posture_boundary --test floodfill_selection_visibility_boundary --test netdb_prune_walk_boundary --test routing_table_population_lock_boundary --test floodfill_statistic_accounting_boundary --test peer_reconcile_boundary -- --test-threads=1` | PASS — 13, 13, 13, 24, 32 and 28 rows |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 51 tests |
| `python3 /tmp/543-negative.py` | PASS — 10 controls, each firing the surface that owns the defect, each restored byte-identically |

The closure record's **citations** are verified mechanically, not by reading.
The verifier recomputes every cited line from the tree and requires the record
to name exactly that line; it checks the lane's and the guard's rows by name;
and it checks the manifest's, the lock's and the guard's sixteen pinned digests
against the tree.

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
| `plan542-retention-posture-lane` | PASS |
| `plan542-retention-posture-boundary` | PASS |
| `plan543-participation-release-lane` | PASS |
| `plan543-participation-release-boundary` | PASS |
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

The floor's own summary is **125 passed, 0 failed**, and the table above lists every step it ran.

The floor's own summary lines are:

```text
FLOOR PASS 125
FLOOR FAIL 0
FLOOR ALL GREEN
```

## The negative controls

Ten controls mutated the **real tree**, ran the lane or the guard, and observed
the expected row fail. Every control was then restored from the bytes saved
before the mutation, and the SHA-256 of every touched file was compared with the
value it had before, so the tree is provably byte-identical afterwards. Every
control **compiles** and names exactly **one** row, which it runs **by name**.

| Control | What it restored | What it broke |
| --- | --- | --- |
| NC-1 | the driver stops reclaiming | the lane's row `a_peer_outside_the_window_is_selectable_again` |
| NC-2 | the entry is never stamped | the lane's row `the_reclaim_is_exact_and_counts_what_it_released` |
| NC-3 | the reclaim never treats an entry as stale | the lane's row `the_pool_is_not_permanently_exhausted` |
| NC-4 | the reclaim keeps the released entry's stamp | the lane's row `the_reclaim_is_exact_and_counts_what_it_released` |
| NC-5 | eviction stops releasing the stamp with the counter | the guard's rule `the_entry_carries_an_instant` |
| NC-6 | the driver restates the window | the guard's rule `the_window_is_derived_and_not_restated` |
| NC-7 | the participation cap moves | the guard's rule `the_bounds_and_the_skip_rule_are_unchanged` |
| NC-8 | a configuration key appears | the guard's rule `the_selection_keeps_its_gates` |
| NC-9 | the controlled router version moves | the guard's rule `nothing_new_runs_and_nothing_is_advertised` |
| NC-10 | the manifest changes | the guard's rule `nothing_new_runs_and_nothing_is_advertised` |

**The controls corrected two of their own designs.** NC-4's first target was
the "window restarts" row, which cannot see a leftover stamp because `record`
overwrites it on re-selection; the control now targets the **exactness** row,
which asserts the released peer has no instant at all. NC-5's first mutation
removed the ledger's `stamps` field, which does not compile and therefore
proved nothing; it now breaks the guard's **eviction clause**. The plan's
amendment log, rows 17.2.5 and 17.2.6, records both.

## Acceptance criteria

| # | The plan's criterion | Outcome |
| --- | --- | --- |
| 1 | a peer outside the window is selectable again, and one inside it is still skipped | MET — the lane's rows `a_peer_inside_the_window_is_still_skipped` and `a_peer_outside_the_window_is_selectable_again`; NC-1 and NC-3 restore the defect |
| 2 | a pool in which every peer is at the cap becomes non-empty again once the clock passes the window | MET — the lane's row `the_pool_is_not_permanently_exhausted`; NC-3 restores the defect |
| 3 | the counter's saturation and the ledger's entry bound are unchanged | MET — the lane's rows `the_counter_still_saturates_at_the_cap` and `the_ledger_still_respects_its_entry_bound`; the guard's rule `the_bounds_and_the_skip_rule_are_unchanged`; NC-7 restores the defect |
| 4 | the window has one definition and no configuration key is added | MET — the guard's rules `the_window_is_derived_and_not_restated` and `the_selection_keeps_its_gates`; NC-6 and NC-8 restore the defect |
| 5 | no task, timer, listener or socket is added, and `Cargo.toml` and `Cargo.lock` stay byte-identical | MET — the guard's rule `nothing_new_runs_and_nothing_is_advertised`, and the two digests above; NC-9 and NC-10 restore the defect |
| 6 | `router.version` stays `0.9.62`, and the inventory's advertised count stays **zero** | MET — the guard's rule `nothing_new_runs_and_nothing_is_advertised`; NC-9 restores the defect |
| 7 | the full floor is green, with the closure record's table complete | MET — the table above |
| 8 | the guard's inverted probes each fail as intended and the tree is restored byte-identically after each | MET — the guard's eight inverted probes `the_inverted_probe_fires_when_the_entry_has_no_instant`, `the_inverted_probe_fires_when_the_driver_does_not_reclaim`, `the_inverted_probe_fires_when_the_window_is_restated`, `the_inverted_probe_fires_when_the_cap_moves`, `the_inverted_probe_fires_when_the_skip_rule_is_replaced`, `the_inverted_probe_fires_when_a_configuration_key_appears`, `the_inverted_probe_fires_when_a_timer_appears` and `the_inverted_probe_fires_when_a_predecessor_changes`, and the ten controls above |

## Invariant review

| # | The plan's invariant | Review |
| --- | --- | --- |
| 1 | a saturated count is released by time, not by luck | **Held.** `reclaim` (`crates/i2pr-netdb/src/selection.rs:267`) drops every entry outside the window, and the driver calls it on the clock it already advances (`crates/i2pr-daemon/src/netdb_participation.rs:235`). NC-1 and NC-3 restore the two ways it could stop firing. |
| 2 | the window is derived, not configured | **Held.** `PARTICIPATION_WINDOW_MS` is `participation_window_secs() * 1_000` (`crates/i2pr-netdb/src/selection.rs:181`), and the helper is a `const fn` (`:184`). The guard counts the definitions and forbids a restated literal; NC-6 restores the defect. |
| 3 | the release is a check at use time | **Held.** It runs inside `advance_time`, which the driver already calls, so no tick, task or timer is added. The guard's rule `nothing_new_runs_and_nothing_is_advertised` rejects an ownerless spawn in either module. |
| 4 | the ledger stays bounded | **Held.** The entry bound is unchanged (`crates/i2pr-netdb/src/selection.rs:37`), and a released entry leaves the map rather than being zeroed, so the bound is what bounds it. The lane's row `the_ledger_still_respects_its_entry_bound` observes it. |
| 5 | the skip rule keeps its meaning | **Held.** The skip rule still reads the counter map (`crates/i2pr-netdb/src/selection.rs:357`), so a released peer reads zero and a recently used peer is still skipped. The guard's rule `the_bounds_and_the_skip_rule_are_unchanged` pins it; NC-7 restores the defect. |
| 6 | the selection's order and its other gates are unchanged | **Held.** The version gate and the tier gate are untouched, and the ordering lives in `lookup_policy.rs`, which the plan does not change. The guard's rule `the_selection_keeps_its_gates` pins all of it. |
| 7 | no peer is blamed | **Held.** The ledger is the router's own record of its own selections; nothing is written into a peer's profile. |
| 8 | no advertisement | **Held.** `crates/i2pr-netdb/src/local.rs:172` still declares `0.9.62`, and no surface in `specs/support.toml` carries `advertised = true`. NC-9 restores the defect. |
| 9 | the predecessors' evidence stays byte-identical | **Held.** The guard pins all sixteen digests. |

## Failure, cancellation, restart, and contention review

**Failure.** The change adds no failure mode. A peer with no entry reads zero,
exactly as it does today, and `reclaim` is total over the entries it holds.

**Cancellation.** Nothing async changes. The ledger is synchronous and
`i2pr-netdb` stays runtime-neutral: the guard rejects a socket, a thread and an
ownerless spawn in both changed modules.

**Restart.** The ledger is in memory and is rebuilt with the driver, so a
restart clears every count — which is the behaviour the plan wants, and the
strongest possible release.

**Contention.** None. The ledger is owned by the driver behind the existing
handle and takes no lock this plan adds.

## Migration and compatibility review

**No wire change.**

**No configuration migration.** **No key is added**, and the window is derived
rather than configured.

**No persisted state.** The ledger is not persisted, and the projection the
profile store restores does not carry participation counts.

**No change to the netdb crate's public surface beyond the ledger's own API.**
`record` gains the caller's instant, `reclaim` and `last_recorded_at` are new,
and `counts()` keeps its exact signature and type — which is why the skip rule
and every caller of the counter view are byte-identical.

**No change to the daemon's public surface.** The driver's `advance_time` keeps
its signature; the coordinator's `record_hop_selection` keeps its signature.

**No change to the manifest or the lock.**

## Security review

**No secret is touched.** The ledger holds router hashes and instants.

**No new disclosure.** `last_recorded_at` is a local reading; the counts name
no peer beyond the hash the router already holds, and nothing is written into a
digest, a receipt or a log.

**No new surface is reachable.** The plan adds no listener, no port, no socket
and no configuration key.

**No peer is blamed.** The ledger is the router's own record of its own
selections. A released peer is not "forgiven" for anything: the release says the
router's **own** usage is old enough not to count.

**Fail closed.** The guard fails closed on every item it names: an entry with no
instant, a driver that does not reclaim, a restated window, a moved cap, a
replaced skip rule, a configuration key, a moved version, a changed manifest and
a changed predecessor's evidence each make the rule that names them fail. The
lane fails closed too: it asserts both directions — a peer inside the window is
skipped **and** one outside it is selectable — so a release that fired too early
would fail rather than pass.

## Limitations

- The lane proves the release through the **real** selection, but it drives the
  driver directly; it cannot see that the daemon starts the driver. That is the
  exploratory posture's gate, which this plan does not change.
- The window's **value** is a judgement, not a measurement: thirty minutes is
  longer than the router's own maintenance cadence and short enough that a pool
  used evenly stays usable. No timing assertion is made, because a timing
  assertion under a shared disk would be flaky and would prove less than the
  derivation does.
- The plan does not claim the exploratory posture is usable against a public
  deployment, and it does not claim a stock-peer measurement.
- **The release is what makes a non-churning population survivable, not what
  fixes it.** The feed is still the bounded prefix, and the successor that gives
  the router a NetDB owner of its own is still the owner of that.

## Findings by severity

### None — nothing was found that blocks the plan

### Medium — discharged

- **The exploratory build driver's peer pool was permanently exhaustible.** A
  fixed population, a saturating cap, and a ledger bound the population could
  never reach: after at most `256 × 8` selections the selection was empty for
  ever, and the router stopped building exploratory tunnels for as long as it
  ran. **Discharged in this plan**: a participation fact is a fact about a
  window, and the release rides the clock the driver already advances.
- **Two of the plan's own controls proved nothing as first written.** One
  targeted a row that could not observe its mutation, and one did not compile.
  Both were corrected rather than the evidence weakened. **Discharged in this
  plan**: the amendment log's rows 17.2.5 and 17.2.6 record both, and the final
  sweep is ten of ten.

### Low — recorded by this plan

- **The peer-profile store's feed is still the bounded prefix.**
  **Owner:** the successor that gives the router a NetDB owner of its own.
- **The exploratory posture is still not dialable against a public
  deployment.** **Owner:** the transport line and Plan 433's blocked
  composition.
- **The router's validated store is still the bootstrap's store.** Plan 536's
  recorded **Low** finding 1. **Owner:** the same successor.
- **The production SSU2 peer-test driver** — Plan 530's Low finding 2.
- **The `B` advertisement and `caps=f`** — their own owners.
- **The stock-peer half** — Plan 493's authorized two-host topology.

## Roadmap disposition

The **network-participation** roadmap's row for Plan 543 moves from
`registered` to `closed`, and the registry's row moves from `registered` to
`closed`. No roadmap row is reopened, renumbered or rewritten.

The plan **advances** the network-participation line's selection work: Plan 541
made the router's floodfill selection see its own advertisers, Plan 542 gave
the retention policy every posture, and Plan 543 makes a peer's participation a
fact about a window — so the exploratory driver's pool cannot be exhausted for
ever. The line's remaining named owner is unchanged: the successor that gives
the router a NetDB owner of its own.

## Unblock audit

- **Plan 543's own findings** are discharged above, and the ones that remain
  have a named owner.
- **Plan 536's recorded Low finding 1** is **carried forward unchanged**, with
  the same owner.
- **Plan 537's, Plan 538's, Plan 539's, Plan 540's, Plan 541's and Plan 542's
  named out-of-scope items** — the NetDB ownership, the composition root's
  reachability, the advertiser copy and the profile feed — are **carried
  forward unchanged**, with the same owner.
- **Plan 530's Low finding 2** stays with its owner.
- **Plan 493's stock-peer half** stays blocked externally. This plan needs no
  stock peer.
- **No plan is unblocked by this one.** The plan changes no advertisement, no
  capability, no version and no configuration.

## Handoff notes

- The plan is **experimental** and default-off. Do not describe the change as
  released, supported or advertised.
- The delivery is confined to the controlled loopback profile.
- The defect was **permanent exhaustion**, not slowness. Do not "fix" it by
  raising the cap or by shrinking the ledger: a higher cap moves the exhaustion
  later, and a smaller ledger evicts peers the policy never intended to release.
- Do not feed the profile store from the advertiser set to fix this. That is a
  different plan with a different owner, and this release works whatever the
  feed is.
- Do not restate the window, and do not make the skip rule read a raw map that
  can go stale: a stale entry that is never released is a permanent pool
  exhaustion.
- Plans 505's, 506's, 507's, 538's, 539's, 540's, 541's and 542's closed closure
  records are **not** rewritten.
- The plan **extends** no other plan's guard, and the guard pins all sixteen
  predecessor digests.
- **Never commit a tree while a tree-mutating script is running.** Plan 542
  records the same lesson; it bit once and was repaired.
- **The shared disk is the machine's tightest resource.** The build cache lives
  outside the repository at `/home/sugarwookie/.cache/i2pr-p542`. Most of the
  disk is **other** sessions' data; do not delete it.

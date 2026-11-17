# Plan 542 — the NetDB's retention must not depend on the service-tunnel posture

**Closure record.** The plan of record is
`plans/implementation/network-participation/542-the-netdbs-retention-must-not-depend-on-the-service-tunnel-posture.md`.
This record is the authority on what the plan closed, what it did not, and on
what the evidence is.

## Status

Plan 542 is **closed**. The router's **retention prune** runs whenever its
NetDB does, instead of only when a service-tunnel group is enabled.

`RouterInfoStore::insert` **refuses** at the quota and **never evicts**
(`crates/i2pr-netdb/src/store.rs:211`), and the tree's own documentation of the
prune states the consequence in as many words: *"a NetDB that filled up would
refuse every new peer, permanently, and would keep a record the router's own
validation policy would refuse to accept today"*
(`crates/i2pr-daemon/src/bootstrap.rs:283`). The operation that prevents that —
`Bootstrap::prune` (`crates/i2pr-daemon/src/bootstrap.rs:291`) — had exactly
**one** production caller, `PeerReconcile::prune_bootstrap`
(`crates/i2pr-daemon/src/peer_reconcile.rs:425`), and `PeerReconcile` is
constructed **only** inside `ServiceProduct`, whose only `ServiceProductSpec`
is built inside `if has_enabled_groups`. That gate is
`service_tunnels_active(config)` — I2PControl, router-network SAM, or an
enabled service tunnel — and every one of those is **false** by default.

Classification: **corrective** (the router's retention policy was gated on the
router's *ability to use* its NetDB rather than on its *holding* one, so a
router running SSU2 alone had a write-only NetDB that filled to its quota and
then refused every subsequent peer for as long as it ran). **Subsystem:**
network-participation. **Closure date:** 2026-11-11.

Nothing is promoted: `router.version` stays `0.9.62`, the inventory's
advertised count stays **zero**, and `Cargo.toml` and `Cargo.lock` stay
byte-identical.

## What the plan closed

**The step has a second caller.** `prune_netdb`
(`crates/i2pr-daemon/src/netdb_participation.rs:650`) takes the bootstrap
handle, prunes under its lock, and releases it. It applies the router's **own**
window — Plan 507's `RETENTION_WINDOW_MS`
(`crates/i2pr-daemon/src/peer_reconcile.rs:81`), **imported** rather than
restated — so the two callers cannot drift apart.

**The outcome type is shared.** `PruneOutcome` is now `pub`
(`crates/i2pr-daemon/src/peer_reconcile.rs:252`), so the two callers cannot
disagree about what a prune outcome *is*. An absent store is
`NotApplicable` and a lock that cannot be taken is `Unavailable`; neither is a
silent success.

**The cadence is the one the router already has.** `run_netdb_retention`
(`crates/i2pr-daemon/src/netdb_participation.rs:684`) rides the participation
maintenance period under the service's child scope. A period outside the bound
is `InvalidPeriod` (`crates/i2pr-daemon/src/netdb_participation.rs:666`), never
silently clamped. There is no second cadence, no second knob, no new timer, no
listener and no ownerless task.

**The switch is the NetDB's own.** The service reads
`netdb_retention_enabled = config.netdb.enabled`
(`crates/i2pr-daemon/src/lib.rs:2457`) as a **separate** binding from the
routing table's `enabled`, and starts the cadence at
`crates/i2pr-daemon/src/lib.rs:2503` — outside the routing table's
`if enabled` block. The cadence itself is `fn start_retention`
(`crates/i2pr-daemon/src/lib.rs:1979`).

**The counters are the owner's.** `NetDbRetentionCounters`
(`crates/i2pr-daemon/src/netdb_participation.rs:601`) is held by the
participation owner (`crates/i2pr-daemon/src/lib.rs:1897`), and
`NetDbRetentionSnapshot`
(`crates/i2pr-daemon/src/netdb_participation.rs:614`) is published with the
owner's other counts when the service stops. A pass that could not run is a
count, never a silent skip.

**The reconcile is unchanged.** Its `prune_bootstrap`
(`crates/i2pr-daemon/src/peer_reconcile.rs:425`), `prune_destinations`,
`snapshot`, `insert_all`, `learn_all` and `run` keep their bodies and their
order, so Plan 506's and Plan 507's guards needed no extension.

## What the plan did not close

- **The router's validated store is still the bootstrap's store.** Plan 536's
  recorded **Low** finding 1, carried forward unchanged. **Owner:** the
  successor that gives the router a NetDB owner of its own. This plan
  **reduces** what that successor must do: the retention policy is no longer a
  property of the composition root's shape.
- **The step's reachability is proven statically, not behaviourally.** The lane
  constructs the owner directly, so it observes what the step *does* and not
  that the daemon starts it. That is the guard's claim. **Owner:** the
  successor that makes the composition root's service body reachable from a
  test.
- **The participation owner's table and exploration features stay inert by
  default.** That is deliberate and unchanged. **Owner:** Plan 433's blocked
  composition for the routing table's reader.
- **The production SSU2 peer-test driver** — Plan 530's Low finding 2.
- **The `B` advertisement and `caps=f`** — their own owners.
- **The stock-peer half** — Plan 493's authorized two-host topology.

## Implementation commits

| Commit | What it carries |
| --- | --- |
| `5a77d234` | the plan of record, the registry's row and the roadmap's row |
| `05bef9ed` | the production change: the retention step, its counters, its cadence, and the composition root's switch |
| `00366ce4` | the lane and the guard, after the plan's own controls found the lane testing the wrong function |
| `f5c458ba` | the repair of a control that a commit captured while the sweep was running |
| `ccaa870c` | the documentation, the inventory and the plan's amendment log |

The floor ran on the committed tree `ccaa870c`. The closure commit adds **only**
this record and the two closed rows, exactly as Plans 538's, 539's, 540's and
541's closure commits did.

The plan of record's **invariants** are at
`plans/implementation/network-participation/542-the-netdbs-retention-must-not-depend-on-the-service-tunnel-posture.md:205`,
its **required production changes** at
`plans/implementation/network-participation/542-the-netdbs-retention-must-not-depend-on-the-service-tunnel-posture.md:273`,
its **design decisions** at
`plans/implementation/network-participation/542-the-netdbs-retention-must-not-depend-on-the-service-tunnel-posture.md:311`,
its **lane** at
`plans/implementation/network-participation/542-the-netdbs-retention-must-not-depend-on-the-service-tunnel-posture.md:427`,
its **guard** at
`plans/implementation/network-participation/542-the-netdbs-retention-must-not-depend-on-the-service-tunnel-posture.md:450`,
its **closure evidence** at
`plans/implementation/network-participation/542-the-netdbs-retention-must-not-depend-on-the-service-tunnel-posture.md:537`,
its **amendment log** at
`plans/implementation/network-participation/542-the-netdbs-retention-must-not-depend-on-the-service-tunnel-posture.md:599`,
the daemon deep-dive's account of the retention posture at
`docs/architecture/i2pr-daemon.md:4572`, the qualification gates at
`specs/QUALIFICATION-GATES.md:2263` and the conformance account at
`specs/CONFORMANCE.md:4731`.

## Requirement-to-evidence matrix

| # | Requirement | Evidence |
| --- | --- | --- |
| 1 | the retention prune runs whenever the NetDB does | the guard's rules `the_step_has_a_caller_outside_the_reconcile` and `the_caller_is_not_behind_the_routing_table_switch`; the production change at `crates/i2pr-daemon/src/lib.rs:2503` |
| 2 | a store at its quota accepts a peer after the step runs | the lane's rows `the_store_at_its_quota_refuses_the_next_peer` and `the_peer_the_store_refused_is_accepted_after_the_step` |
| 3 | the step's removal count equals an independently computed stale set | the lane's row `the_step_removes_exactly_the_stale_records` |
| 4 | a record at the window's boundary survives | the lane's row `a_record_at_the_boundary_survives` |
| 5 | the step is idempotent | the lane's row `the_step_is_idempotent` |
| 6 | an absent store and a poisoned lock are named, never silent | the lane's rows `the_step_names_an_absent_store_rather_than_reporting_a_prune` and `a_poisoned_lock_is_counted_and_the_store_keeps_its_contents` |
| 7 | the router's own identity is never a victim | the lane's row `the_routers_own_record_is_never_a_victim` |
| 8 | the cadence runs the step and counts it, and refuses an out-of-bound period | the lane's rows `the_cadence_runs_the_step_and_counts_it` and `a_period_outside_the_bound_is_refused` |
| 9 | the retention window has exactly one definition | the guard's rule `the_window_has_one_definition`; the production change at `crates/i2pr-daemon/src/peer_reconcile.rs:81` |
| 10 | the reconcile is unchanged, and its ordering is unchanged | the guard's rule `the_reconcile_is_unchanged` |
| 11 | the features over the NetDB keep their existing gates | the guard's rule `the_features_keep_their_gates` |
| 12 | no configuration key is added, and no dependency is added | the guard's rules `the_features_keep_their_gates` and `nothing_new_runs_and_nothing_is_advertised`, and the two digests below |
| 13 | no new task, timer, listener or socket | the guard's rule `nothing_new_runs_and_nothing_is_advertised` |
| 14 | no advertisement | the guard's rule `nothing_new_runs_and_nothing_is_advertised`; `crates/i2pr-netdb/src/local.rs:172` |
| 15 | Plan 506's, Plan 507's, Plan 538's, Plan 539's, Plan 540's and Plan 541's lanes and guards stay byte-identical | the guard's rule `the_predecessors_evidence_is_untouched`, and the twelve digests below |

The twelve digests the guard pins are Plan 506's lane
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
(`da81058d9e32b1e36332e3f031788c93b87c1430dcbb1d753759c227634586a4`), and Plan
541's lane
(`85190546dd7e371ae140cc4e9b964caa32100a3ca44c830986034593b7cfeb41`) and guard
(`351080b5f5f219a5b54a7a732556185e1ef18b108b4e948e5597d68c8b02ded7`). A later
change to any of them must change the pinned constant **and** say why.

The manifest and the lock are byte-identical to the tree Plan 541 closed on, and
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
| `cargo test --locked -p i2pr-daemon --test netdb_retention_posture -- --test-threads=1` | PASS — 10 rows |
| `cargo test --locked -p i2pr-daemon --test netdb_retention_posture_boundary -- --test-threads=1` | PASS — 14 rows |
| `cargo test --locked -p i2pr-daemon --test netdb_retention_posture --test netdb_retention_posture_boundary --test netdb_retention_live --test netdb_retention_boundary --test peer_reconcile_boundary --test floodfill_statistic_accounting_boundary --test routing_table_population_lock_boundary --test netdb_prune_walk_boundary --test floodfill_selection_visibility_boundary -- --test-threads=1` | PASS — 10, 14, 1, 13, 28, 32, 24, 13 and 13 rows |
| `cargo test --locked -p i2pr-daemon --lib -- --test-threads=1` | PASS — 1001 rows, 3 ignored |
| `cargo test --locked -p i2pr-netdb --all-targets` | PASS |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 scripts/check-license-metadata.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 51 tests |
| `python3 /tmp/542-negative.py` | PASS — 11 controls, each firing the surface that owns the defect, each restored byte-identically |

The router's own unit-test suite needs the managed-app sibling binaries, and
`AGENTS.md` says so: without them, eighteen `app_runtime_qualification` rows
fail because their `assert_fresh` staleness guard fails closed. The floor's
`app-siblings`, `app-fixture-static`, `app-fixture-copy` and
`app-fixture-rebuild` steps run before the tests for exactly this reason, and
with them built the suite is **1001 passed, 0 failed, 3 ignored**.

The closure record's **citations** are verified mechanically, not by reading.
The verifier recomputes every cited line from the tree and requires the record
to name exactly that line:

- the **closure record's** citations are recomputed against the **current
  tree** — the line that carries the cited text is the line the record must
  name;
- the **plan of record's** section 3 citations are recomputed against the
  **registration commit** `5a77d234`;
- the **lane's and the guard's rows** are checked by name;
- the **manifest's and the lock's digests**, and the guard's twelve pinned
  digests, are checked against the tree.

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

The floor's own summary is **123 passed, 0 failed**, and the table above lists every step it ran.

The floor's own summary lines are:

```text
FLOOR PASS 123
FLOOR FAIL 0
FLOOR ALL GREEN
```

## The negative controls

Eleven controls mutated the **real tree**, ran the lane or the guard, and
observed the expected row fail. Every control was then restored from the bytes
saved before the mutation, and the SHA-256 of every touched file was compared
with the value it had before, so the tree is provably byte-identical
afterwards. Every control **compiles** and names exactly **one** row, which it
runs **by name**.

| Control | What it restored | What it broke |
| --- | --- | --- |
| NC-1 | the step stops calling the store's prune | the lane's row `the_step_removes_exactly_the_stale_records` |
| NC-2 | the step prunes with a window twice its own | the lane's row `the_peer_the_store_refused_is_accepted_after_the_step` |
| NC-3 | the cadence stops counting what the step removed | the lane's row `the_cadence_runs_the_step_and_counts_it` |
| NC-4 | the service body stops starting the cadence | the guard's rule `the_step_has_a_caller_outside_the_reconcile` |
| NC-5 | the retention switch becomes another NetDB switch | the guard's rule `the_caller_is_not_behind_the_routing_table_switch` |
| NC-6 | the cadence restates the window instead of reading it | the guard's rule `the_window_has_one_definition` |
| NC-7 | the reconcile stops pruning | the guard's rule `the_reconcile_is_unchanged` |
| NC-8 | a configuration key appears | the guard's rule `the_features_keep_their_gates` |
| NC-9 | the controlled router version moves | the guard's rule `nothing_new_runs_and_nothing_is_advertised` |
| NC-10 | the manifest changes | the guard's rule `nothing_new_runs_and_nothing_is_advertised` |
| NC-11 | a predecessor's guard changes | the guard's rule `the_predecessors_evidence_is_untouched` |

**The controls did real work before the plan closed.** Three of them found
defects the plan would otherwise have shipped:

- NC-1 and NC-2 revealed that the lane's first draft called `Bootstrap::prune`
  directly in two rows, so a mutation that stopped the retention step from
  pruning **at all** left those rows **passing**. Every row now drives the
  production step.
- NC-4 revealed that the guard's first rule looked for `run_netdb_retention(`
  anywhere in the composition root, and the cadence's own body keeps that name,
  so deleting the service body's call left the rule satisfied. The rule now
  requires the **service body** to start the cadence.
- NC-6's first probe restated the window with `pub const` in a function body,
  which is `E0449` — a hard error — so it broke the build instead of firing the
  rule. A control that breaks the build proves nothing about the row it names.
  The probe was rebuilt to compile.

The plan's amendment log, rows 17.2.1, 17.2.2 and 17.2.7, records all three.

## Acceptance criteria

| # | The plan's criterion | Outcome |
| --- | --- | --- |
| 1 | a router with `netdb.enabled = true` and no service-tunnel group prunes its NetDB on the participation cadence | MET — the guard's rules `the_step_has_a_caller_outside_the_reconcile` and `the_caller_is_not_behind_the_routing_table_switch`; NC-4 and NC-5 restore the defect |
| 2 | a store at its quota accepts a peer after the step runs, and the step's removal count equals an independently computed stale set | MET — the lane's rows `the_store_at_its_quota_refuses_the_next_peer`, `the_peer_the_store_refused_is_accepted_after_the_step` and `the_step_removes_exactly_the_stale_records`; NC-1 and NC-2 restore the defect |
| 3 | a record inside the window survives, and the router's own `RouterInfo` is never a victim | MET — the lane's rows `a_record_at_the_boundary_survives` and `the_routers_own_record_is_never_a_victim` |
| 4 | the retention window has exactly one definition and no configuration key is added | MET — the guard's rules `the_window_has_one_definition` and `the_features_keep_their_gates`; NC-6 and NC-8 restore the defect |
| 5 | the routing table, the exploration job, the profile store and the discovery spine keep their existing gates | MET — the guard's rule `the_features_keep_their_gates` |
| 6 | no new task, timer, listener or socket is added, and `Cargo.toml` and `Cargo.lock` stay byte-identical | MET — the guard's rule `nothing_new_runs_and_nothing_is_advertised`, and the two digests above; NC-9 and NC-10 restore the defect |
| 7 | `router.version` stays `0.9.62`, and the inventory's advertised count stays **zero** | MET — the guard's rule `nothing_new_runs_and_nothing_is_advertised`, and `crates/i2pr-netdb/src/local.rs:172`; NC-9 restores the defect |
| 8 | the full floor is green, with the closure record's table complete | MET — the table above |
| 9 | the guard's inverted probes each fail as intended and the tree is restored byte-identically after each | MET — the guard's seven inverted probes `the_inverted_probe_fires_when_the_step_has_no_outside_caller`, `the_inverted_probe_fires_when_the_step_restates_the_window`, `the_inverted_probe_fires_when_the_caller_is_behind_the_routing_table_switch`, `the_inverted_probe_fires_when_the_reconciles_ordering_changes`, `the_inverted_probe_fires_when_a_configuration_key_appears`, `the_inverted_probe_fires_when_a_socket_appears` and `the_inverted_probe_fires_when_a_predecessor_changes`, and the eleven controls above |

## Invariant review

| # | The plan's invariant | Review |
| --- | --- | --- |
| 1 | the retention prune runs whenever the NetDB does | **Held.** The service reads `config.netdb.enabled` as its own binding (`crates/i2pr-daemon/src/lib.rs:2457`) and starts the cadence at `crates/i2pr-daemon/src/lib.rs:2503`, outside the routing table's block. The guard pins both, and NC-4 and NC-5 restore the two ways it could regress. |
| 2 | there is exactly one retention window | **Held.** The constant stays at `crates/i2pr-daemon/src/peer_reconcile.rs:81`, still derived through the `const fn`, and the new caller **imports** it. The guard's rule `the_window_has_one_definition` counts the definitions and forbids the step restating a literal; NC-6 restores the defect. |
| 3 | the prune keeps its semantics | **Held.** The step calls the same `Bootstrap::prune` (`crates/i2pr-daemon/src/bootstrap.rs:291`) the reconcile calls, so the store's own prune and Plan 540's age index do the work. |
| 4 | the prune's cost stays proportional to what it removes | **Held.** The step adds no scan: it holds the router's NetDB lock for the same bounded index walk the reconcile already holds it for. |
| 5 | the lock discipline is unchanged | **Held.** `prune_netdb` takes the bootstrap's lock, prunes, and returns — the guard is dropped before the function returns, and no lock is held across an await. |
| 6 | the features over the NetDB keep their existing gates | **Held.** `participation.start(...)` is still behind `if enabled`, and the retention start is a separate block after it. The guard's rule `the_features_keep_their_gates` pins the order. |
| 7 | the prune is counted by reason | **Held.** `NetDbRetentionCounters` counts runs, removed, unavailable and not-applicable, and the owner publishes them with its other counts when the service stops. |
| 8 | the router never prunes its own record | **Held.** The router's own `RouterInfo` lives beside the store, and the lane's row `the_routers_own_record_is_never_a_victim` observes that the step empties the store without touching the local identity. |
| 9 | no new task, timer, listener or socket | **Held.** The cadence runs under the service's existing child scope on the existing period. The guard's rule `nothing_new_runs_and_nothing_is_advertised` rejects the socket and ownerless-spawn constructs in the changed modules; NC-9 and NC-10 restore the advertisement defects. |
| 10 | no configuration key is added, and no dependency is added | **Held.** The guard's rule `the_features_keep_their_gates` rejects the retention tokens in the daemon's configuration, and the manifest's and the lock's digests are unchanged. NC-8 restores the key. |
| 11 | no advertisement | **Held.** `crates/i2pr-netdb/src/local.rs:172` still declares `0.9.62`, and no surface in `specs/support.toml` carries `advertised = true`. |
| 12 | the predecessors' evidence stays byte-identical | **Held.** The guard's rule `the_predecessors_evidence_is_untouched` pins all twelve digests, and NC-11 restores a predecessor's guard. |

## Failure, cancellation, restart, and contention review

**Failure.** The step adds no new failure mode. A store that cannot be pruned
is counted `Unavailable` and keeps its contents; a composition with no store is
counted `NotApplicable`; a cadence period outside the bound is `InvalidPeriod`
rather than a silent clamp. The service is `Optional` with
`StartupRequirement::Optional`, so a router that cannot prune still starts and
still serves.

**Cancellation.** The cadence runs under the service's child scope and returns
`Cancelled` when the scope cancels, exactly as the profile maintenance owner
does. The step itself is synchronous, takes the bootstrap's lock, and releases
it before returning; no lock is held across an await, and the two locks are
never held at once.

**Restart.** Nothing is persisted. The age index is rebuilt with the store, so a
restart cannot inherit a stale index, and the first pass repairs a store that
filled while a previous run had no prune.

**Contention.** The step holds the router's NetDB lock for Plan 540's bounded
index walk — proportional to what it removes, not to the population. In the
posture this plan fixes, it **replaces zero** holds, because there were none.

## Migration and compatibility review

**No wire change.** The router refuses and accepts exactly the records it would
have, minus the ones past its own acceptance window.

**No configuration migration.** **No key is added.** The retention window
remains derived rather than configured, and the cadence is the participation
period the router already has.

**No persisted state.** The index lives inside the store.

**No change to the netdb crate's public surface.** `i2pr-netdb` is unchanged.

**No change to the daemon's public surface.** The retention step is a module
function, the counters are additive, and the only changed visibility is
`PruneOutcome`, which was private and is now `pub` — additive, and no closed
guard pins it.

**No change to the manifest or the lock.** `Cargo.toml` and `Cargo.lock` are
byte-identical to the tree Plan 541 closed on.

**A behaviour change in one posture, deliberately.** A router running SSU2 alone
now drops records past its acceptance window. That is the defect being fixed,
and it is what the router's own validation policy already demanded: the record
it kept is one it would refuse today.

## Security review

**No secret is touched.** The step holds router hashes and publication times.
Nothing in this plan reads, writes or derives from a private key, a signing
seed or a session secret.

**No new disclosure.** The counters carry counts only — no peer hash, no
address, no key. They are reported with the owner's other counts when the
service stops.

**No new surface is reachable.** The plan adds no listener, no port, no socket
and no configuration key, so it opens no new way in.

**No peer is blamed.** The step writes nothing into a peer's profile and
creates no peer-side condition. A record it removes is one the router's own
acceptance policy would refuse today; the removal is the router's own fact, not
the peer's.

**Fail closed.** The guard fails closed on every item it names: a step with no
outside caller, a caller behind the wrong switch, a restated window, a changed
reconcile, a configuration key, a moved version, a changed manifest and a
changed predecessor's evidence each make the rule that names them fail. The lane
fails closed too: the removal count is compared with an independently computed
stale set, so a step that removed too little or too much fails rather than
passes.

## Limitations

- The lane proves the step's **behaviour** on a store it constructs itself. It
  cannot see that the daemon **starts** the step, so that is the **guard's**
  claim, and the guard's claim is a static one.
- The plan does not claim that the router's NetDB is correctly sized for a
  public deployment. It claims that the router's retention policy now applies in
  every posture; whether four thousand records is the right ceiling is a
  separate question with its own owner.
- The plan's effect on lock contention is structural rather than measured: in
  the posture it fixes, there was no prune at all, so there is no timing
  claim to make.
- Plan 507's lane had the same shape as this plan's lane and constructed
  `PeerReconcile` by hand, which is why the reachability defect survived it.
  This plan does not rewrite Plan 507's record; it records the correction here
  and in the registry's row.
- **One commit captured a negative control.** `00366ce4` was made while the
  sweep was running, so it recorded the NC-4 probe instead of the production
  call; `f5c458ba` repaired it. The rule the floor already states — never
  rewrite or commit a tree while a tree-mutating script is running — is the
  lesson, and the plan's amendment log, row 17.2.6, records it.

## Findings by severity

### None — nothing was found that blocks the plan

### Medium — discharged

- **The router's NetDB was write-only in every posture without a service-tunnel
  group.** `RouterInfoStore::insert` refuses at the quota and never evicts, and
  the only production caller of the prune was composed inside
  `if has_enabled_groups`, which is false by default for I2PControl, SAM and
  every service tunnel. An SSU2-only router therefore filled its NetDB and then
  refused every subsequent peer for as long as it ran. **Discharged in this
  plan**: the retention step has a second caller that runs whenever the NetDB
  does.
- **The plan's own lane was testing the wrong function, and the plan's own
  guard could not tell a started cadence from a dead one.** Both were found by
  the plan's negative controls before closure, and both were fixed in the
  evidence rather than worked around. **Discharged in this plan**: every lane
  row drives the production step, and the guard requires the service body to
  start the cadence.

### Low — recorded by this plan

- **The router's validated store is still the bootstrap's store.** Plan 536's
  recorded **Low** finding 1, carried forward unchanged. **Owner:** the
  successor that gives the router a NetDB owner of its own.
- **The step's reachability is proven statically, not behaviourally.** Plan
  536's recorded **Low** finding 2, carried forward unchanged. **Owner:** the
  successor that makes the composition root's service body reachable from a
  test.
- **The production SSU2 peer-test driver** — Plan 530's Low finding 2.
- **The `B` advertisement and `caps=f`** — their own owners.
- **The stock-peer half** — Plan 493's authorized two-host topology.

## Roadmap disposition

The **network-participation** roadmap's row for Plan 542 moves from
`registered` to `closed`, and the registry's row moves from `registered` to
`closed`. No roadmap row is reopened, renumbered or rewritten, and no other
plan's row changes.

The plan **advances** the network-participation line's retention work: Plan 507
made the router's peer records not outlive its own acceptance window, and Plan
542 makes that policy apply in **every** posture the router can run in. The
line's remaining named owner is unchanged: the successor that gives the router
a NetDB owner of its own.

## Unblock audit

- **Plan 542's own findings** are discharged above, and the ones that remain
  have a named owner.
- **Plan 536's recorded Low findings 1 and 2** are **carried forward
  unchanged**, with the same owners.
- **Plan 537's, Plan 538's, Plan 539's, Plan 540's and Plan 541's named
  out-of-scope items** — the NetDB ownership, the composition root's
  reachability, the store's selection and the advertiser copy — are **carried
  forward unchanged**, with the same owner.
- **Plan 530's Low finding 2** — the production SSU2 peer-test driver — stays
  with its owner.
- **Plan 493's stock-peer half** stays blocked externally, on Plan 493's
  authorized two-host topology. This plan needs no stock peer.
- **No plan is unblocked by this one.** The plan changes no advertisement, no
  capability, no version and no configuration, so it opens no gate for another
  plan.

## Handoff notes

- The plan is **experimental** and the retention policy is internal. Do not
  describe the change as released, supported or advertised.
- The delivery is confined to the controlled loopback profile.
- The defect was **reachability**, not mechanism. Do not "fix" it by making the
  prune faster or by widening the store's quota.
- Do not enable the routing table, the exploration job or the profile store to
  make the retention step reachable. The retention policy is a property of
  holding a NetDB, and the features over it keep their switches.
- Do not move `RETENTION_WINDOW_MS`. Plan 507's closed guard pins its
  derivation, and a second definition would be the second source of truth Plan
  507 removed. A new caller **imports** it.
- Plans 506's, 507's, 538's, 539's, 540's and 541's closed closure records are
  **not** rewritten. The discharge lives in this plan's closure record and in
  the registry's row.
- The plan **extends** no other plan's guard, and the guard pins all twelve
  predecessor digests.
- **Never commit a tree while a tree-mutating script is running.** One commit in
  this plan did, and a follow-up commit repaired it.
- The tree's next step in this line is the successor that gives the router a
  **NetDB owner of its own**. That change is larger than this plan and is not
  attempted here.
- **The shared disk is the machine's tightest resource.** The build cache lives
  outside the repository at `/home/sugarwookie/.cache/i2pr-p542`, and
  `CARGO_INCREMENTAL=0` and `CARGO_GC_AUTO_FREQUENCY=never` are set. Most of the
  disk is **other** sessions' data; do not delete it.

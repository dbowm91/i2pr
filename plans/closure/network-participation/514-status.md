# Plan 514 — the live exploratory request

**Closure record.** The plan of record is
`plans/implementation/network-participation/514-live-exploratory-request.md`. This record is the authority on what the plan closed,
what it did not, and on what the evidence is.

## Status

Plan 514 is **closed**. The router's exploratory request is now aimed at the
**closest** floodfill it knows, and the operator's switch over it is **real**.

Two defects, one subject:

1. **The aim.** The live path picked the **lowest-hash** floodfill advertiser
   in the flat store and addressed the request to it. The store is a
   `BTreeMap` keyed by the peer's hash, so its first advertiser is the
   lowest-hash one, and the choice did not depend on the search key at all:
   every exploration asked the **same** peer. The routing table's
   closest-first selection — the mechanism Plan 489 built and Plan 513 gave a
   reader — was **not consulted** on that path at all, and the table's own
   exploration entry point had **no production caller**.
2. **The switch.** `netdb.exploration_enabled` was parsed, defaulted to
   `false`, and cross-validated against `netdb.routing_table_enabled` — and
   **no production code read it**. The exploration ran whenever the routing
   table was on.

A router that always asks the same peer is not participating in a DHT; it is
polling one host. And a configuration key that lies is worse than no key.

**Nothing is promoted.** `router.version` stays `0.9.62`; `caps` and the
published RouterInfo are unchanged; `Cargo.toml` and `Cargo.lock` stay
byte-identical; the inventory's advertised count stays **zero**.

**The floor ran twice.** The first run failed in this plan's own lane, and
the failure was **not** a product defect: the plan's negative-controls
harness restored a mutated file with a metadata-preserving copy, so cargo
never rebuilt it, and the floor linked a binary built from the last
control's mutation. The diagnosis, the evidence and the mitigation are in
*Verification* below; the second floor passed on the rebuilt tree.

**Plan 497's M2 is not reopened.** The switch gates the **exploratory
request** only; the RouterInfo-kind resolution stays above it, so the routing
table keeps its production reader. Plan 497's closure record is **not**
rewritten, and neither is Plan 513's.

## Scope

| | |
| --- | --- |
| Plan | 514 |
| Subsystem | network-participation |
| Discharges | the exploratory request's floodfill selection, and `netdb.exploration_enabled` |
| Predecessors | Plan 489 (the floodfill-indexed routing table and the exploration job), Plan 497 (the routing table's consumer and its closest-first selection), Plan 509 (the lookup-target API-version decision), Plan 513 (the live NetDB discovery spine) |
| Reference pins | i2pd `2.61.0` = `635b013a612ff47278ef02acf8580a28e10e26c5`; Java I2P `2.13.0` = `9134f808337b401e8e53c73734c81fab04280c9d` |

## Baseline at registration

The tree at `2a20845c` (Plan 514's registration):

- `crates/i2pr-daemon/src/lib.rs` — `NetDbParticipation::drive_discovery` chose
  the peer with `store.floodfill_advertisers().next()` and had no gate.
- `crates/i2pr-daemon/src/netdb_participation.rs` — the spine's exploratory
  start took the floodfill **as a parameter**; the table's own exploration
  entry point, `RoutingTableConsumer::begin_exploration`, had no production
  caller.
- `crates/i2pr-daemon/src/config.rs` — `exploration_enabled` was declared,
  defaulted and cross-validated, and read by nothing.
- `crates/i2pr-daemon/tests/lookup_target_version_boundary.rs` — Plan 509's
  guard named the LeaseSet2 path and the lookup engine's three selection
  sites, and **not** the two live lookup paths Plan 513 had added.

## Implementation commits

| Commit | Subject |
| --- | --- |
| `2a20845c` | docs(plans): register Plan 514, the live exploratory request |
| `ccc0a029` | feat(daemon): aim the exploratory request at the closest floodfill, and honour the operator's switch |
| `daa8d2da` | docs(specs): correct the reference's peer-exclusion reading for Plan 514 |

The closing commit is the one that adds this record and the registry and
roadmap rows.

## Requirement-to-evidence matrix

| Requirement | Evidence |
| --- | --- |
| the exploratory request's target is the **closest** floodfill | `crates/i2pr-daemon/tests/netdb_exploration_target.rs`, `the_exploratory_target_is_the_closest_floodfill` — the lane computes the expected target itself, from the public `daily_routing_key` and the reference's XOR-distance rule, so the assertion is not the table checking its own answer |
| the target follows the **search key**, not a fixed peer | the same lane, `the_target_follows_the_search_key` |
| the target is **not** the lowest-hash floodfill | the same lane, `the_target_is_not_the_lowest_hash_floodfill` — the old behaviour, pinned as gone |
| a floodfill the table does not hold is **never** the target | the same lane, `a_store_only_floodfill_is_never_the_target` |
| a table that names **no** floodfill refuses the request and counts it | the same lane, `a_table_that_names_no_floodfill_refuses_the_request_and_counts_it` |
| a **disabled** table refuses the request and counts it | the same lane, `a_disabled_table_refuses_the_request_and_counts_it` |
| a refused request is **never** redirected to an arbitrary peer | the same lane, `a_refused_request_is_never_redirected_to_an_arbitrary_peer` |
| the operator's switch starts and stops the exploratory request | `crates/i2pr-daemon/src/lib.rs`, `the_operator_switch_starts_and_stops_the_exploratory_request` — a crate-internal test, because the switch lives in the composition root's private cycle |
| the switch gates the **exploration only** | `crates/i2pr-daemon/tests/netdb_exploration_target_boundary.rs`, `the_switch_gates_the_exploration_only` — the resolution branch must precede the switch |
| the target comes from the **table** | the same guard, `the_spine_asks_the_table_for_its_target` |
| the flat store's iteration order is **never** a selection | the same guard, `the_spine_never_falls_back_to_the_flat_store_order` and `the_composition_never_selects_the_floodfill` |
| the refusal is **counted** | the same guard, `the_refusal_is_counted` |
| the configuration key is **consumed** | the same guard, `the_configuration_key_is_consumed` |
| Plan 509's decision holds on the new live paths | `crates/i2pr-daemon/tests/lookup_target_version_boundary.rs` — four further sites are named |
| Plan 514's surface is **not advertised** | `crates/i2pr-daemon/tests/netdb_discovery_boundary.rs`, rule 12 |
| nothing is advertised | `specs/support.toml` — the advertised count stays zero; `Cargo.toml` and `Cargo.lock` are byte-identical |

## Work packages and outcomes

**WP1 — the closest-floodfill selection.** Landed. The spine's exploratory
start asks the routing table for the closest verified floodfill for the
search key's routing key. The routing key the router **already computed**
for the request is the one the table's selection needs, so nothing new had to
be derived. A table that names none ends the request, counted.

**WP2 — the operator's switch.** Landed. `netdb.exploration_enabled` is read
and gates the exploratory request. The switch's **default does not move**:
the key's own documentation already said the exploration is off by default,
so the wiring was missing, not the design.

**WP3 — the lane.** Landed. Seven tests, all green, public API only, fixed
seeds, an explicit `now_ms`, and no socket, environment variable, `#[ignore]`,
stock peer or sleeping.

**WP4 — the guard.** Landed. Nine rules and eight inverted probes, all green.
Every rule asserts that a *named function's own body* contains the required
*call*; comments are stripped, `#[cfg(test)]` blocks are excluded, and an
absent function fails closed.

**WP5 — the records, the controls, the floor and the closure.** Landed.

## Invariant review

| Invariant | Held | How |
| --- | --- | --- |
| The RouterInfo-kind resolution stays live | yes | Plan 513's lane still passes unchanged, and the switch check is placed **after** the resolution branch |
| The table may only reorder, never narrow | yes | Plan 513's `the_table_preference_reorders_and_never_narrows_the_selection` still passes; this plan's selection is a **target**, and D2 states why that is a different question |
| Plan 102's authority amendment is honoured | yes | the exploratory request is still delivered **directly**, and the RouterInfo-kind request still rides the **outbound** exploratory tunnel |
| Every fallback is counted by reason | yes | the table's own counters answer `Disabled` / `NoCandidate` / `Unavailable`; the spine adds `explorations_refused_no_target` |
| Plan 509's decision holds on every live lookup path | yes | four further sites are named in its guard |
| No peer is blamed | yes | the tree writes no peer fault into any profile; the refusals are the router's own |
| No unbounded state | yes | the selection is bounded by the table's own bounds and the lookup's own budget; no new collection is added |
| No advertisement | yes | the inventory's advertised count stays zero; `router.version` stays `0.9.62` |

## Verification

### Focused runs

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | **PASS** |
| `cargo check --locked -p i2pr-daemon --all-targets` | **PASS** |
| `cargo clippy --locked -p i2pr-daemon --all-targets --all-features -- -D warnings` | **PASS** |
| `cargo test --locked -p i2pr-daemon --test netdb_exploration_target -- --test-threads=1` | **PASS** — 7 passed |
| `cargo test --locked -p i2pr-daemon --test netdb_exploration_target_boundary -- --test-threads=1` | **PASS** — 17 passed |
| `cargo test --locked -p i2pr-daemon --test netdb_discovery_live -- --test-threads=1` | **PASS** — 15 passed |
| `cargo test --locked -p i2pr-daemon --test netdb_discovery_boundary -- --test-threads=1` | **PASS** — 24 passed |
| `cargo test --locked -p i2pr-daemon --test lookup_target_version_boundary -- --test-threads=1` | **PASS** — 12 passed |
| `cargo test --locked -p i2pr-daemon --lib the_operator_switch -- --test-threads=1` | **PASS** — 1 passed |
| `cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1` | **PASS** — 0 failed |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | **PASS** — 51 tests, OK |
| `python3 scripts/check-global-plan-number-uniqueness.py` | **PASS** |
| `python3 scripts/check-tooling-inventory.py` | **PASS** |
| `bash scripts/check-config-secret-hygiene.sh` | **PASS** |

### The AGENTS.md routine floor

Run on the closing tree, after the tree-cleanliness step. The floor gains
three steps over Plan 513's 130 — `plan514-exploratory-target`,
`plan514-exploratory-target-boundary` and `plan514-switch-unit` — plus the
`floor-tree-clean` step described below. Plan 509's guard is already in the
floor as `plan509-lookup-target-boundary`, so Plan 514's extension to it is
covered there.

The floor was run **twice**. The first run failed in this plan's own lane,
and the failure was a stale build artifact rather than a product defect; it
is diagnosed in full in the section below.

### The first floor's lane failure, and what it was

The first floor run on this plan **failed**, and the failure was **not** a
product defect. It is recorded here in full, because an unexplained failure
is not noise.

The floor reported failures in one binary only — `plan514-exploratory-target`
— and in it, three of the lane's seven tests: the closest-floodfill
assertion, the search-key assertion, and the lowest-hash test, which
panicked with *"some key reaches a peer other than the lowest hash"*. The
symptom was precise: the lane's **expected** target was the **lowest-hash**
floodfill for **every** search key, while the router's actual target was
not.

That symptom is the signature of one specific mutation, and this plan's own
negative controls contain it. **Control L** replaces the lane's
closest-wins return

```rust
best.expect("the lane's candidate set is never empty").0
```

with

```rust
candidates.iter().copied().min().expect("the lane's candidate set is never empty")
```

— the lane's expected target becomes the **lowest hash**, which is exactly
what the failing run observed, down to the panic's message.

The controls harness had run against the real tree shortly before the
floor. It applies each mutation to the real file, runs the detector, and
then **restores** the file. The restore used `shutil.copy2`, which copies
the file's **metadata**, so the restored file kept the mtime it had
*before* the mutation — which was **older** than the build artifact cargo
had just produced from the mutation. Cargo's freshness check is mtime-based,
so it treated the restored file as unchanged and **never rebuilt it**. The
floor's build therefore reused the lane's test binary as the control had
left it: the binary still contained control L's mutated lane.

The evidence that the **product** was clean in that same run is in the
floor's own log. Plan 509's guard (`lookup_target_version_boundary`, 12
passed) and Plan 513's boundary guard (`netdb_discovery_boundary`, 24
passed) both passed in the same workspace run. Control J mutates
`netdb_seam.rs` — a **production** file — in a way Plan 509's guard is
built to catch, and it passed, so the production library the floor linked
was the clean one. Only the lane's own test binary was stale.

The lane behaved correctly throughout: it caught a mutation it was never
supposed to be running against, and it caught it by failing.

Two things were changed, and the floor was re-run:

1. The harness now restores a file by **writing its bytes**, which stamps a
   fresh mtime, so cargo rebuilds the crate after every control. The twelve
   controls were re-run on the real tree with the fixed harness: **12/12
   caught by an assertion**, every file reverted byte-for-byte with its
   SHA-256 asserted, and the tree clean afterwards.
2. This plan's floor run begins with a `floor-tree-clean` step that
   **fails** unless `git status --porcelain` is empty, so a floor can never
   again run on a tree it does not claim.

The second floor ran on the rebuilt tree, and it is the floor table below.

The full routine floor from `AGENTS.md`, run on the closing tree:

| # | Step | Outcome | Seconds |
| --- | --- | --- | --- |
| 1 | `floor-tree-clean` | **PASS** | 0 |
| 2 | `cargo-fmt` | **PASS** | 3 |
| 3 | `cargo-check` | **PASS** | 6 |
| 4 | `cargo-build-siblings` | **PASS** | 0 |
| 5 | `cargo-build-static-fixture` | **PASS** | 0 |
| 6 | `cargo-copy-static-fixture` | **PASS** | 0 |
| 7 | `cargo-rebuild-plain-fixture` | **PASS** | 0 |
| 8 | `cargo-test-workspace` | **PASS** | 1270 |
| 9 | `cargo-clippy` | **PASS** | 13 |
| 10 | `cargo-doc` | **PASS** | 6 |
| 11 | `cargo-test-doc` | **PASS** | 4 |
| 12 | `check-dependency-direction` | **PASS** | 0 |
| 13 | `check-global-plan-number-uniqueness` | **PASS** | 0 |
| 14 | `check-adr-number-uniqueness` | **PASS** | 0 |
| 15 | `check-portable-service-tunnel-api` | **PASS** | 0 |
| 16 | `check-portable-service-tunnel-consumer` | **PASS** | 6 |
| 17 | `planning-tests` | **PASS** | 7 |
| 18 | `check-runtime-boundaries` | **PASS** | 1 |
| 19 | `check-console-boundaries` | **PASS** | 1 |
| 20 | `check-console-browser-security` | **PASS** | 0 |
| 21 | `check-tooling-inventory` | **PASS** | 0 |
| 22 | `check-license-metadata` | **PASS** | 0 |
| 23 | `check-managed-app-private-client-seams` | **PASS** | 0 |
| 24 | `check-router-readiness-contract` | **PASS** | 0 |
| 25 | `check-router-readiness-contract-self` | **PASS** | 0 |
| 26 | `check-core-router-recovery-contract` | **PASS** | 0 |
| 27 | `check-core-router-recovery-contract-self` | **PASS** | 0 |
| 28 | `plan449-ssu2-preflight-self` | **PASS** | 0 |
| 29 | `plan495-peer-eligibility-self` | **PASS** | 0 |
| 30 | `plan495-peer-eligibility-profile-drift` | **PASS** | 0 |
| 31 | `plan476-gate-binding` | **PASS** | 1 |
| 32 | `plan476-gate-binding-self` | **PASS** | 0 |
| 33 | `check-managed-app-package-boundary` | **PASS** | 0 |
| 34 | `check-managed-app-package-boundary-self` | **PASS** | 0 |
| 35 | `check-managed-app-policy-boundary` | **PASS** | 1 |
| 36 | `check-managed-app-policy-boundary-self` | **PASS** | 0 |
| 37 | `check-managed-app-secured-sandbox` | **PASS** | 0 |
| 38 | `check-managed-app-secured-sandbox-self` | **PASS** | 0 |
| 39 | `check-service-tunnel-boundaries` | **PASS** | 0 |
| 40 | `check-managed-app-gateway-boundary` | **PASS** | 0 |
| 41 | `check-managed-app-manager-boundary` | **PASS** | 0 |
| 42 | `check-m11-per-epoch-composition` | **PASS** | 0 |
| 43 | `check-service-anonymity-boundaries` | **PASS** | 0 |
| 44 | `check-fixture-manifest` | **PASS** | 0 |
| 45 | `check-ntcp2-vectors` | **PASS** | 0 |
| 46 | `check-ssu2-vectors` | **PASS** | 0 |
| 47 | `check-i2cp-vectors` | **PASS** | 0 |
| 48 | `check-ntcp2-interoperability` | **PASS** | 0 |
| 49 | `check-constrained-host-lane-boundary` | **PASS** | 1 |
| 50 | `check-m11-transit-boundaries` | **PASS** | 2 |
| 51 | `check-m11-transit-qualification-evidence` | **PASS** | 1 |
| 52 | `check-sam-acceptance-evidence` | **PASS** | 0 |
| 53 | `check-ssu2-acceptance-evidence` | **PASS** | 0 |
| 54 | `check-i2cp-acceptance-evidence` | **PASS** | 0 |
| 55 | `check-i2pcontrol-acceptance-evidence` | **PASS** | 1 |
| 56 | `check-els2-type11-transcript-boundary` | **PASS** | 0 |
| 57 | `check-els2-live-lane-evidence` | **PASS** | 2 |
| 58 | `check-encrypted-service-consumer-caller` | **PASS** | 0 |
| 59 | `check-outproxy-request-path` | **PASS** | 0 |
| 60 | `check-outproxy-wire-lane-evidence` | **PASS** | 0 |
| 61 | `check-config-secret-hygiene` | **PASS** | 0 |
| 62 | `check-workflow-validity` | **PASS** | 0 |
| 63 | `check-floodfill-type5-serve` | **PASS** | 0 |
| 64 | `check-service-tunnel-acceptance-evidence` | **PASS** | 1 |
| 65 | `check-exploratory-tunnel-evidence` | **PASS** | 0 |
| 66 | `check-netdb-tunnel-evidence` | **PASS** | 0 |
| 67 | `check-destination-tunnel-evidence` | **PASS** | 1 |
| 68 | `check-streaming-tunnel-evidence` | **PASS** | 4 |
| 69 | `check-m6-mixed-router-acceptance-evidence` | **PASS** | 6 |
| 70 | `check-m12-floodfill-qualification-evidence-self` | **PASS** | 0 |
| 71 | `check-m12-floodfill-boundaries-self` | **PASS** | 1 |
| 72 | `ntcp2-harness-tests` | **PASS** | 0 |
| 73 | `plan488-mesh-lane` | **PASS** | 0 |
| 74 | `plan488-evidence-deterministic` | **PASS** | 0 |
| 75 | `plan489-mesh-lane` | **PASS** | 1 |
| 76 | `plan489-evidence-deterministic` | **PASS** | 0 |
| 77 | `plan490-mesh-lane` | **PASS** | 0 |
| 78 | `plan490-evidence-deterministic` | **PASS** | 0 |
| 79 | `plan490-ssu2-transport-suite` | **PASS** | 6 |
| 80 | `plan490-transport-boundary-guard` | **PASS** | 0 |
| 81 | `plan491-mesh-lane` | **PASS** | 1 |
| 82 | `plan491-evidence-deterministic` | **PASS** | 0 |
| 83 | `plan491-boundary-guard` | **PASS** | 0 |
| 84 | `plan491-proto-suite` | **PASS** | 0 |
| 85 | `plan491-netdb-suite` | **PASS** | 88 |
| 86 | `plan491-client-suite` | **PASS** | 24 |
| 87 | `plan492-posture-suite` | **PASS** | 1 |
| 88 | `plan492-m11-lane` | **PASS** | 4 |
| 89 | `plan502-transit-composition-lane` | **PASS** | 0 |
| 90 | `plan502-transit-composition-unit` | **PASS** | 1 |
| 91 | `plan502-transit-boundary-self` | **PASS** | 0 |
| 92 | `plan496-private-lan-profile` | **PASS** | 0 |
| 93 | `plan496-ntcp2-activation-gate` | **PASS** | 0 |
| 94 | `plan497-routing-consumer-lane` | **PASS** | 1 |
| 95 | `plan497-netdb-preferred-order` | **PASS** | 0 |
| 96 | `plan497-daemon-participation-unit` | **PASS** | 2 |
| 97 | `plan498-introducer-handoff-lane` | **PASS** | 1 |
| 98 | `plan498-introducer-mesh-lane` | **PASS** | 1 |
| 99 | `plan498-bootstrap-resolver-row` | **PASS** | 25 |
| 100 | `plan498-runtime-relay-accessor` | **PASS** | 4 |
| 101 | `plan503-version-eligibility-live` | **PASS** | 1 |
| 102 | `plan503-version-eligibility-unit` | **PASS** | 0 |
| 103 | `plan504-liveness-boundary` | **PASS** | 1 |
| 104 | `plan504-liveness-live` | **PASS** | 0 |
| 105 | `plan504-liveness-unit` | **PASS** | 0 |
| 106 | `plan505-driver-boundary` | **PASS** | 1 |
| 107 | `plan505-driver-live` | **PASS** | 2 |
| 108 | `plan505-driver-unit` | **PASS** | 1 |
| 109 | `plan506-reconcile-boundary` | **PASS** | 5 |
| 110 | `plan506-reconcile-live` | **PASS** | 0 |
| 111 | `plan507-retention-feasibility` | **PASS** | 1 |
| 112 | `plan507-retention-boundary` | **PASS** | 1 |
| 113 | `plan507-retention-live` | **PASS** | 0 |
| 114 | `plan508-creator-retention-feasibility` | **PASS** | 1 |
| 115 | `plan508-creator-retention-boundary` | **PASS** | 1 |
| 116 | `plan508-creator-retention-live` | **PASS** | 0 |
| 117 | `plan509-lookup-target-decision` | **PASS** | 1 |
| 118 | `plan509-lookup-target-boundary` | **PASS** | 0 |
| 119 | `plan510-managed-staging-load-lane` | **PASS** | 1 |
| 120 | `plan510-managed-reason-boundary` | **PASS** | 0 |
| 121 | `plan510-tunnelmanager-black-box` | **PASS** | 3 |
| 122 | `plan511-version-ceiling-derivation` | **PASS** | 0 |
| 123 | `plan511-version-ceiling-boundary` | **PASS** | 0 |
| 124 | `plan512-destination-liveness-live` | **PASS** | 0 |
| 125 | `plan512-destination-liveness-boundary` | **PASS** | 0 |
| 126 | `plan489-peer-budget-receipt` | **PASS** | 1 |
| 127 | `plan513-discovery-live` | **PASS** | 2 |
| 128 | `plan513-discovery-boundary` | **PASS** | 0 |
| 129 | `plan513-discovery-unit` | **PASS** | 2 |
| 130 | `plan513-discovery-seam-unit` | **PASS** | 1 |
| 131 | `plan514-exploratory-target` | **PASS** | 7 |
| 132 | `plan514-exploratory-target-boundary` | **PASS** | 0 |
| 133 | `plan514-switch-unit` | **PASS** | 0 |
| 134 | `cargo-deny` | **PASS** | 1 |

**134 steps, 134 PASS, 0 FAIL.**

### The false-pass audit

Each guard rule was checked for a way to pass **without** the property it
claims:

- A rule that asserts a *call* can be satisfied by a **comment** — the rules
  read the code view, with comments stripped, so a doc comment that merely
  names a construct never satisfies a rule.
- A rule can be satisfied by the **wrong** definition — the rules are scoped
  by `impl` block and by `fn` name, and the module owns three same-named
  exploration starts.
- A rule can be satisfied by a **test** — `#[cfg(test)]` blocks are excluded
  before the scan.
- A rule can pass when the function is **absent** — an absent function fails
  closed.
- The lane can pass **vacuously** — the lane computes the expected target
  itself, from the public `daily_routing_key` and the reference's
  XOR-distance rule, rather than reading the answer back out of the table.

Each of these was exercised by an inverted probe, and each probe is in the
guard's own test suite.

### The controls

| # | The mutation | Caught by | Outcome |
| --- | --- | --- | --- |
| A | caught: the spine must ask the table for its target | the new guard, rule 2 | **CAUGHT** |
| B | caught: the spine must never fall back to the flat store's order | the new guard, rule 3 | **CAUGHT** |
| C | caught: the composition root must not select the floodfill itself | the new guard, rule 4 | **CAUGHT** |
| D | caught: a request the table cannot target must be counted | the new guard, rule 5 | **CAUGHT** |
| E | caught: the operator's switch must be read | the new guard, rule 6 | **CAUGHT** |
| F | caught: the switch must gate the exploration only | the new guard, rule 7 | **CAUGHT** |
| G | caught: the configuration key must be consumed by the daemon | the new guard, rule 8 | **CAUGHT** |
| H | caught: the accessor must ask the table for its candidates | the new guard, rule 1 | **CAUGHT** |
| I | caught: the exploratory target must not be version-gated | Plan 509's guard | **CAUGHT** |
| J | caught: the RouterInfo-kind resolution must not be version-gated | Plan 509's guard | **CAUGHT** |
| K | caught: the surface must not be advertised | Plan 513's guard, rule 12 | **CAUGHT** |
| L | caught: the lane's expected target must be the closest, not the lowest hash | the lane | **CAUGHT** |

**12 controls, 12 caught by an assertion, 0 build-refused, 0 vacuous.** Every mutation was reverted byte-for-byte and the file's SHA-256 was asserted after each revert.

## Migration and compatibility

**No configuration key is added, and none is removed, and no default moves.**
`netdb.exploration_enabled` becomes real, with the default the tree already
documents.

**One operator-visible behaviour changes, and it is the defect.** An operator
who set `netdb.routing_table_enabled = true` and left
`netdb.exploration_enabled` at its documented default of `false` receives
**no** exploratory request after this plan, where before they received one.
That request was aimed at an arbitrary floodfill — the defect WP1 fixes — and
the key that was supposed to switch it off was never read.

| `routing_table_enabled` | `exploration_enabled` | What runs |
| --- | --- | --- |
| `false` (the default) | anything | no table, no participation owner, no spine, no exploration |
| `true` | `false` (the default) | the table is maintained and persisted; **no** exploration |
| `true` | `true` | the full discovery spine |

No persisted state changes. The published RouterInfo, `caps` and
`router.version` are untouched. A router built before this plan and one built
after it interoperate: the request is an ordinary I2NP message.

## Security and contention

The change **narrows** what the router trusts in one respect and widens it in
none: the exploratory request is now addressed to a peer the router holds
with signed evidence, rather than to an arbitrary peer from the flat store's
iteration order. The reply's sender is still checked against the peer the
exploration actually asked, and a reply from any other peer introduces
**nothing**.

The lock order is unchanged and acyclic: the spine's lock is taken before the
routing table's, and the table's consumer never calls back into the spine.
The selection is a pure read of the table's own snapshot, under the table's
own lock, and holds no lock across an await.

No secret is read, logged or serialized by this plan. The switch is a
boolean. No peer identity is written into any profile.

## Documentation and operational changes

- `specs/support.toml` — a new surface, `daemon.netdb-exploratory-target`,
  and a correction to Plan 513's surface note, which described an aim the tree
  did not have at Plan 513. The inventory holds **106 surfaces, 101
  experimental, 4 deferred, 1 ready, 0 advertised**.
- `specs/CONFORMANCE.md` — the Plan 514 section.
- `specs/QUALIFICATION-GATES.md` — §13, the references' evidence for the
  exploratory request's target, with the i2pd `file:line` citations at the
  exact pin and Java's caveat.
- `docs/architecture/i2pr-daemon.md` — the module-table rows for the two
  changed files, their description cells, a new section, and every line
  citation into the two changed files re-pointed by **exact-content match**
  against the live file and re-verified.
- `plans/registry.md` and `plans/subsystems/network-participation-roadmap.md`
  — Plan 514's rows.

## Deviations from the plan of record

The plan of record is **not** rewritten. The deviations are recorded here.

1. **`crates/i2pr-daemon/src/config.rs` did not change.** The plan's §6
   listed it among the production changes. The key already existed, already
   defaulted and already cross-validated; the defect was that nothing **read**
   it, so the fix was in the consumer, not in the declaration. The plan's own
   §7 D3 reached the same conclusion — *"the wiring was missing, not the
   design"* — and this is its consequence.
2. **A second guard file changed.** The plan's §5 and §8 named Plan 509's
   guard. Plan 513's **advertisement** rule (rule 12) had to be extended too,
   because it pins the surfaces that must not be advertised, and Plan 514
   adds one. Without it, Plan 514's surface could be advertised with every
   guard green.
3. **Plan 509's guard needed four new sites, not two.** The plan's §11 named
   "the two live lookup paths Plan 513 added". The exploratory path's own
   selection site — `RouterInfoLookup::start_with_candidates` — was not among
   them, and neither was this plan's own `closest_floodfill`. All four are
   named now.
4. **The `i2pr-netdb` deep-dive needed no change.** The plan's §13 listed it.
   **No netdb crate file changed**, so it had nothing to correct; its claims
   about the table's closest-first selection were already accurate.
5. **Two of the plan's §11 tests are not in the lane.** The plan listed the
   operator's switch among the **lane's** tests. The switch lives in the
   composition root's private cycle, so a public-API lane cannot reach it:
   the switch's behaviour is proved by a **crate-internal** test
   (`the_operator_switch_starts_and_stops_the_exploratory_request`), and the
   "the resolution stays above the switch" property is proved by the
   **guard's ordering rule** (`the_switch_gates_the_exploration_only`), which
   is where an ordering property belongs. Both are in the floor.
6. **A Plan 513 lane test was re-scoped.** The lane's
   `a_suggestion_from_a_peer_the_table_does_not_hold_is_refused` drove a
   scenario the new selection makes unreachable: an exploration that asks a
   peer the table cannot vouch for. The **rule** is unchanged and still
   pinned — by the table's own unit test and by Plan 513's structural guard —
   and the lane now asserts it where it lives, on the table's own public
   entry point, beside the new structural property.
7. **The controls harness's restore mechanism changed, and the floor was
   run twice.** The plan's §12 asks for the routine floor and records its
   result. The first run failed, and the failure was a **stale build
   artifact** the plan's own controls had left behind — the harness restored
   a mutated file with a metadata-preserving copy, so cargo never rebuilt
   it. The harness now restores by writing the file's bytes, the twelve
   controls were re-run with it, the floor now asserts a clean tree before
   it runs, and the floor was run again on the rebuilt tree. **Both** results
   are recorded: the first run's failure and its diagnosis above, the second
   run's table below. No production file changed as a result of the first
   run, and no product defect was found in it.

## Known limitations

1. **The exploration's rate is far below the reference's.** The reference
   asks `1..9` exploratory requests per `27..55` seconds
   (`libi2pd/NetDbRequests.cpp:521-531`: `numRouters = 800/numRouters`,
   clamped to `9`, and `EXPLORATORY_REQUEST_INTERVAL = 55` at
   `libi2pd/NetDbRequests.h:32`). `i2pr` asks **one** per participation
   tick, and the tick is `netdb.participation_maintenance_period_ms` —
   `60` seconds by default. The structure is the same: one exploratory
   request, one random key, one closest floodfill. The **rate** is not, and
   a router that explores an order of magnitude less often learns the
   network an order of magnitude more slowly.
2. **The exploration's declared budget is above what the path uses.**
   `netdb.exploration_max_peers` defaults to `8`, but the exploratory path
   asks **one** peer per exploration. That is **not** a divergence: the
   reference's exploratory request asks one floodfill too. The advance to
   the *next* closest floodfill is gated on a destination that is **not**
   exploratory (`libi2pd/NetDbRequests.cpp:261`:
   `if (!dest->IsExploratory () && count < MAX_NUM_REQUEST_ATTEMPTS)`), so
   the reference's exploratory request never advances either. The declared
   budget is a **ceiling**, not a target, and it is the same shape as the
   reference's `MAX_NUM_REQUEST_ATTEMPTS`.
3. **The stock-peer half is external.** A run against a pinned stock router
   that answers a real exploratory request is blocked on Plan 493's
   authorized two-host topology. Nothing here claims it.
4. **The Java reading does not exist.** The pinned Java reference is not
   installed on this host and the local jars carry no `router/netdb` package,
   so there is **no** Java reading of how Java chooses the floodfill for an
   exploratory request. Recorded in `specs/QUALIFICATION-GATES.md` §13.5.

## Findings by severity

### Medium — open

**M1 — The negative-controls harness left a stale build artifact behind, and
the first floor run measured it instead of the tree.** The harness applied
each mutation to the real file and restored it with a metadata-preserving
copy, so the restored file's mtime stayed **older** than the artifact cargo
had just built from the mutation. Cargo's freshness check is mtime-based, so
it never rebuilt the crate, and the floor linked a binary built from **control
L's mutated lane**. The floor's three lane failures were therefore **false**
— the product was clean in that run, which Plan 509's guard (12 passed) and
Plan 513's guard (24 passed) show, and control J's production-file mutation
is one Plan 509's guard catches. The harness now restores by writing the
file's bytes, which stamps a fresh mtime; the twelve controls were re-run
with it (**12/12 caught by an assertion**), the floor was re-run on the
rebuilt tree, and the floor now begins with a step that fails on a dirty
tree. Recorded as **M1** because the failure mode is a **false** product
failure — the kind that invites a wrong "fix" to correct code — and because
any successor harness that restores a mutated file by copying it will
reproduce it.

**F1 — The exploration's rate is far below the reference's.** The reference
asks `1..9` exploratory requests per `27..55` seconds; `i2pr` asks **one**
per participation tick, `60` seconds by default. A router that explores an
order of magnitude less often learns the network an order of magnitude more
slowly, and the NetDB's population grows at the exploration's rate.

The reference's reading was checked before this finding was written, and it
matters what it shows. `m_ExcludedPeers` is **per request**
(`libi2pd/NetDbRequests.h:50`, `:76`), holding the router itself when it is
a floodfill (`libi2pd/NetDbRequests.cpp:29`), the routers that answered
(`:48`) and the floodfill that was asked (`:59`), cleared on reset (`:73`) —
so there is **no** cross-exploration "already asked" set. And the advance to
the **next closest** floodfill is gated on a **non**-exploratory destination
(`libi2pd/NetDbRequests.cpp:261`), so the reference's exploratory request
asks exactly **one** floodfill — the closest for its random key — and does
not advance. `i2pr`, after this plan, does exactly that too. There is
therefore **no** divergence in the exploratory request's peer count, and the
finding is the rate.

**Recorded for a successor**, not fixed here: this plan's subject is the
*aim* of the request and the operator's switch over it. The rate is the
participation owner's cadence, and the exploration's budget and advance
belong to Plan 489/500/512.

### Low — open

**L1 — The daemon deep-dive's remaining module-row drift.** The module
table's line counts remain stale for the rows this plan did not touch. This
plan re-corrected only the two rows for the files it changed, following Plan
511's and Plan 512's precedent; the pre-existing drift audit remains open.

**L2 — The re-scoped Plan 513 lane test.** The scenario it drove is
unreachable through the public path now. The rule is still pinned by the
table's own unit test and by Plan 513's structural guard, so this is a
coverage-shape change and not a lost property — but a reader comparing the
two lane files will notice that the test's scenario changed.

### Informational

**R1 — The inventory's figures move because the tree moved.** The inventory
holds **106 surfaces, 101 experimental, 4 deferred, 1 ready, 0 advertised**.
Nothing is advertised.

**R2 — The reference's divergence.** The reference's **direct** fallback (a
type-2 lookup when the exploratory tunnels are unavailable) is still **not**
implemented; §12.5 of `specs/QUALIFICATION-GATES.md` records that. This plan
does not change it.

## Roadmap disposition

`plans/subsystems/network-participation-roadmap.md` — Plan 514's row moves
from **registered** to **closed**, with the closure path, the implementation
commit and the delivery in the last cell, following the house convention for a
closed row.

`plans/registry.md` — Plan 514's row moves from **registered** to **closed**.

**Plan 497's M2 is not reopened.** The switch gates the exploratory request
only; the RouterInfo-kind resolution stays above it, so the routing table
keeps its production reader. Plan 497's closure record is **not** rewritten,
and neither is Plan 513's: the correction to Plan 513's claim that the spine
runs unconditionally lives in **this** record and in Plan 513's registry row.

## Unblock audit

| Open item | Owner | What would close it |
| --- | --- | --- |
| **M1** — the controls harness's stale artifact | closed by this plan's re-run | the harness restores by writing bytes, the floor asserts a clean tree, and the floor was re-run on the rebuilt tree |
| **F1** — the exploration's rate | a successor plan | a cadence that asks the reference's `1..9` exploratory requests per `27..55` seconds, proved over the tree's own surfaces |
| **L1** — the deep-dive's module-row drift | a successor plan | the drift audit, re-run over the whole table |
| **L2** — the re-scoped Plan 513 lane test | closed by this plan | the rule is pinned by the table's unit test and Plan 513's guard |
| the **stock-peer** half | Plan 493's authorized two-host topology | a run against a pinned stock router that answers a real exploratory request, with the sanitized counts and hashes in the evidence file |
| **Plan 491's F2**, **Plan 497's** stock-peer half, **Plan 487's F3** stock-peer half | external | the same topology; none is claimed here |

**The next free plan number is 515.**

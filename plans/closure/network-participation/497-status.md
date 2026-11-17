# Plan 497 — The NetDB participation consumer phase: routing-table-driven lookup selection

## Status

| Field | Value |
| --- | --- |
| Plan | 497 |
| Subsystem | `network-participation` |
| Closure token | `passed-netdb-participation-consumer-phase` |
| Plan of record | `plans/implementation/network-participation/497-netdb-participation-consumer-phase.md` |
| Closure record | `plans/closure/network-participation/497-status.md` |
| Predecessors | Plan 488 (`passed-peer-profile-store`), Plan 489 (`passed-floodfill-indexed-routing-table`), Plan 496 (`passed-ntcp2-private-lan-operator-profile-and-gate-aware-activation`) |
| Floor | 93 steps, 93 PASS, 0 FAIL, on tree `96225f73` |

Plan 497 is the **consumer phase** for Plans 488 and 489. It discharges three of
the four findings it set out to discharge and partially discharges the fourth.
The net effect: the daemon now owns the peer-profile store and the
floodfill-indexed routing table, and the NetDB tunnel-building coordinator asks
the routing table how it would order the peers it queries.

**What is closed**

- **Plan 488 F1** — `scripts/check-runtime-boundaries.sh` now carries an
  `i2pr-netdb` section with the script's positive controls.
- **Plan 488 F3** — `run_profile_maintenance` now runs in the daemon's service
  graph, owned by the `netdb-participation` service.
- **Plan 488 F4** — the peer-profile projection is persisted and loaded at
  startup when it is present and current, and discarded and counted otherwise.
- **Plan 489 §11.2, the coordinator half** — the routing table now drives the
  NetDB tunnel-building coordinator's candidate selection, and the discriminator
  proves it.

**What is not closed**

- **Plan 489 §11.2, the production half** — `NetDbTunnelCoordinator` is still
  not created by the daemon. That composition is Plan 433's work package 2 and
  work package 4, and Plan 433 is registered `blocked` on Plans 431 and 432.
  Plan 497 delivers the seam Plan 433's composition will consume; it does not
  duplicate blocked work under another number. The consequence is stated plainly
  in §9 (Limitations) and in the unblock audit (§13).

**Nothing was promoted.** `specs/support.toml`, `Cargo.toml` and `Cargo.lock`
are byte-identical; `router.version` stays `0.9.62`; `caps` and the published
RouterInfo are unchanged; the routing table is internal state and nothing about
it is advertised.

## Commits

| Commit | Subject | Contents |
| --- | --- | --- |
| `6e06846b` | docs(plans): register Plan 497, the NetDB participation consumer phase | The plan of record, the registry row and the network-participation roadmap row. |
| `7183f0fb` | feat(network-participation): land the Plan 497 NetDB participation consumer phase | `crates/i2pr-netdb/src/lookup_engine.rs`, `crates/i2pr-daemon/src/netdb_participation.rs`, `netdb_seam.rs`, `netdb_tunnels.rs`, `lib.rs`, `config.rs`, the new `crates/i2pr-daemon/tests/netdb_routing_consumer.rs`, the one-line compile fix in `crates/i2pr-daemon/tests/netdb_integration.rs`, and the `i2pr-netdb` section in `scripts/check-runtime-boundaries.sh`. |
| `96225f73` | docs(architecture): document the Plan 497 NetDB participation consumer phase | `docs/architecture/i2pr-daemon.md`, `docs/architecture/i2pr-netdb.md`, `docs/architecture/tooling.md`, `specs/CONFORMANCE.md`. |

## What changed

### `crates/i2pr-netdb/src/lookup_engine.rs`

A new preferred-order entry point, `RouterInfoLookup::start_with_preferred`
(`lookup_engine.rs:373`).

- The candidate **set** is never reduced. The engine computes exactly the same
  flat-store selection `start` computes — same selector, same policy, same
  exclusions — and only reorders it. The target stays excluded, and a preferred
  peer the flat store would not have served is ignored: the table may reorder
  the selection, never widen it.
- The preference is a **priority prefix**, not a closed list. On every later
  advance the engine re-selects against the authoritative store and puts the
  preferred peers first (`lookup_engine.rs:588`), so a peer ingested while the
  lookup runs is still queried.
- Plan 489's `start_with_candidates` (`lookup_engine.rs:324`) is **unchanged**
  and keeps its closed-list contract. A test pins the difference.

### `crates/i2pr-daemon/src/netdb_participation.rs`

The consumer seam, `RoutingTableConsumer`.

- `candidates(store, routing_key, excluded)` returns either
  `RoutingTableCandidates::Table(Vec<RouterHash>)` — the table's closest-first
  verified floodfills for the key — or
  `RoutingTableCandidates::Fallback(RoutingTableFallback)`, whose reason is
  `Disabled`, `NoCandidate` or `Unavailable` (a poisoned table).
- Every fallback is counted by reason:
  `RoutingTableCounters { answered, fell_back_disabled, fell_back_no_candidate,
  fell_back_unavailable }`. The counters live **outside** the table's lock on
  purpose: a poisoned table cannot answer, and that fallback must still be
  counted. A fallback that is not counted is a silent fallback.

### `crates/i2pr-daemon/src/netdb_tunnels.rs`

`NetDbTunnelCoordinator::with_routing_table` attaches the shared consumer, and
`begin_tunnel_lookup` consults it.

- The flat store's selection is computed **first** and is always the
  authority. The table's answer only reorders it.
- A table that cannot answer does not fail the lookup: the coordinator uses the
  flat store's selection exactly as today, and the fallback is counted.

### `crates/i2pr-daemon/src/netdb_seam.rs`

`begin_lookup_with_preferred` replaces the seam's call into the engine. With
`preferred = None` the path is byte-for-byte the old one.

### `crates/i2pr-daemon/src/lib.rs`

The `NetDbParticipation` service, registered in the service graph under the name
`netdb-participation` with `ServiceClassification::Optional` **and**
`.startup_requirement(i2pr_core::StartupRequirement::Optional)`.

- It owns the peer-profile driver and the shared `RoutingTableConsumer`.
- It runs `run_profile_maintenance` and a routing-table owner on the configured
  cadence, both under the service's child scope, so both have an owner and a
  cancellation path.
- It populates the table from the daemon's own NetDB floodfill advertisers, each
  indexed with its **own signed evidence**. A peer whose own record does not
  advertise `f` is refused.
- It loads the persisted projection at startup when it is present and current,
  and discards and counts it otherwise.
- With `netdb.routing_table_enabled = false` — the default — the service is
  **inert**: no population, no tick, no persistence.

### `crates/i2pr-daemon/src/config.rs`

One additive, bounded, default-off key inside the existing `deny_unknown_fields`
`[netdb]` section: `netdb.participation_maintenance_period_ms`, default
`60_000` ms, bounded to `[1_000, 3_600_000]` ms. A value outside that range is
refused at parse time with a `ConfigError::Semantic` naming the field. No
existing key changes meaning.

### `scripts/check-runtime-boundaries.sh`

The `i2pr-netdb` section (Plan 488 F1): six raw scans plus the normalised scan
with twelve rule categories, a positive control per category, a negative
control, and a per-category completeness assertion. The script's floors were
**raised**, not relaxed: the positive-control floor from 14 to 30 and the
raw-scan floor from 17 to 23.

### `crates/i2pr-daemon/tests/netdb_routing_consumer.rs` (new)

Six rows: the discriminator, its control, the disabled-table fallback and its
count, the empty-table fallback and its count, the store-unchanged row, and the
candidate-set row.

## Requirement-to-evidence matrix

Every row of the plan's §5.1 is mapped to the exact test or command that proves
it. The **discriminator** is named explicitly.

| # | §5.1 requirement | Evidence |
| --- | --- | --- |
| 1 | A daemon-owned participation service that owns the profile driver and the table driver, runs the bounded maintenance cadence, populates the table from the daemon's own NetDB, and stops cleanly on cancellation. | `crates/i2pr-daemon/src/lib.rs`: `NetDbParticipation`, `register_netdb_participation_service`. Tests: `the_default_off_netdb_participation_owner_is_inert`, `the_enabled_netdb_participation_owner_indexes_and_answers`, `the_periodic_owner_stops_on_cancellation` (existing Plan 488 row), `the_periodic_owner_refuses_an_out_of_bound_period` (existing Plan 488 row). |
| 2 | Routing-table population: index each validated floodfill with `Signed` evidence; accept a suggestion as `Suggested`; refuse a peer whose own record does not advertise `f`; rotate on the UTC day boundary. | `populate_routing_table` (`src/lib.rs`) indexes every `bootstrap.store().floodfill_advertisers()` record with its own signed evidence; `RoutingTableConsumer::index_suggested` is the only suggestion path, and the table refuses a suggestion whose source is not itself indexed with signed evidence. Tests: `the_netdb_participation_owner_refuses_a_peer_that_is_not_a_floodfill` (daemon lib), `an_unverified_source_cannot_introduce_a_peer` and `the_table_rotates_onto_a_new_day` (existing Plan 489 rows in `netdb_participation.rs`). |
| 3 | Table-driven lookup selection, with the flat store as the authority and every fallback counted by reason. | `NetDbTunnelCoordinator::with_routing_table` + `begin_tunnel_lookup` (`src/netdb_tunnels.rs`); `NetDbSeam::begin_lookup_with_preferred` (`src/netdb_seam.rs`); `RouterInfoLookup::start_with_preferred` (`src/lookup_engine.rs`). Tests: **`the_routing_table_selects_the_lookup_peer_and_the_flat_store_does_not`** (the discriminator), `without_the_routing_table_the_flat_store_selects` (the control), `a_disabled_table_falls_back_to_the_flat_store_and_counts_the_reason`, `an_empty_table_falls_back_to_the_flat_store_and_counts_the_reason`, `the_routing_table_does_not_narrow_the_flat_store_selection`, `the_routing_table_reorders_the_selection_without_narrowing_it` (coordinator unit test), `a_preferred_peer_is_asked_first`, `a_preferred_peer_the_flat_store_would_not_serve_is_ignored`, `the_preferred_path_never_asks_the_target`, `the_preferred_path_still_reaches_a_peer_ingested_mid_lookup`, `the_closed_candidate_list_does_not_reach_a_peer_ingested_mid_lookup`, `a_disabled_consumer_falls_back_and_counts_the_reason`, `a_poisoned_table_falls_back_and_counts_the_reason`, `the_consumer_answers_from_the_table_and_counts_it`. |
| 4 | The persisted projection at startup (Plan 488 F4): load it when present and current, discard it otherwise. | `load_projection` / `apply_projection` / `discard_projection` / `persist_projection` in `src/lib.rs`. "Current" is carried by the **file name** (`peer-profiles/profile-projection-<yyyyMMdd>.txt`), because Plan 488's `ProfileProjection` has no UTC-day field; the projection **bytes** are unchanged. Tests: `the_netdb_participation_owner_applies_a_current_projection`, `the_netdb_participation_owner_discards_a_corrupt_projection`, `the_netdb_participation_owner_discards_a_projection_from_another_day`. |
| 5 | The `i2pr-netdb` boundary section (Plan 488 F1). | `scripts/check-runtime-boundaries.sh`, the `i2pr-netdb` section. Floor step `check-runtime-boundaries`: `i2pr-netdb runtime-neutrality: ok (12 rules, per-rule positive controls passed)`. The section carries a positive control per rule category, a negative control over the crate's legitimate surface, and a per-category completeness assertion. |
| 6 | The discriminator. | **`the_routing_table_selects_the_lookup_peer_and_the_flat_store_does_not`** in `crates/i2pr-daemon/tests/netdb_routing_consumer.rs`. The lane holds **alpha** (in the routing table and in the flat store) and **zeta** (in the flat store only); the routing key is zeta's own key, so the flat store's nearest-first choice is unambiguously zeta. With the table attached the lookup asks **alpha** and never zeta; without the table the lookup asks **zeta**. The control row is what makes the discriminator sharp: without it, the first row would prove nothing. |

## Commands run

### Focused, before the floor

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `CARGO_BUILD_JOBS=4 cargo check --locked -p i2pr-netdb -p i2pr-daemon --all-targets` | clean |
| `CARGO_BUILD_JOBS=4 cargo test --locked -p i2pr-netdb --lib -- lookup_engine` | 18 passed; 0 failed |
| `CARGO_BUILD_JOBS=4 cargo test --locked -p i2pr-daemon --lib -- netdb_tunnels` | 4 passed; 0 failed |
| `CARGO_BUILD_JOBS=4 cargo test --locked -p i2pr-daemon --lib -- netdb_participation` | 44 passed; 0 failed |
| `CARGO_BUILD_JOBS=4 cargo test --locked -p i2pr-daemon --test netdb_routing_consumer -- --test-threads=1` | 6 passed; 0 failed |
| `CARGO_BUILD_JOBS=4 cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1` | PASS (exit 0) |
| `CARGO_BUILD_JOBS=4 cargo test --locked -p i2pr-daemon --all-targets --no-fail-fast -- --test-threads=1` | PASS (exit 0) |
| `CARGO_BUILD_JOBS=4 cargo clippy --locked -p i2pr-daemon -p i2pr-netdb --all-targets --all-features -- -D warnings` | clean |
| `python3 scripts/check-router-readiness-contract.py` | `router-readiness contract checks passed` |
| `python3 scripts/check-router-readiness-contract.py --self-test` | `router-readiness contract self-test passed` |
| `bash scripts/check-runtime-boundaries.sh` | `runtime boundary checks passed` (exit 0) |
| `python3 scripts/check-tooling-inventory.py` | `check-tooling-inventory: every published figure matches the tree` |
| `python3 scripts/check-global-plan-number-uniqueness.py` | `Global plan-number ownership check passed.` |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | 51 tests, OK |
| `git diff --stat -- specs/support.toml Cargo.toml Cargo.lock` | **empty** |

### The routine floor

`bash /tmp/i2pr-floor-497-run1.sh` → `/tmp/i2pr-floor-497-run1.log`, on tree
`96225f73`. **93 steps, 93 PASS, 0 FAIL.** The floor is the repository's own
routine list from `AGENTS.md` (90 steps) plus three Plan 497 lanes:
`plan497-routing-consumer-lane`, `plan497-netdb-preferred-order` and
`plan497-daemon-participation-unit`.

Per-step outcomes are in the log. The steps that carry this plan's weight:

| Step | Result |
| --- | --- |
| `cargo-fmt` | PASS |
| `cargo-check` | PASS |
| `cargo-test-workspace` | PASS |
| `cargo-clippy` | PASS |
| `cargo-doc` | PASS |
| `cargo-test-doc` | PASS |
| `check-runtime-boundaries` | PASS — `i2pr-netdb runtime-neutrality: ok (12 rules, per-rule positive controls passed)` |
| `check-dependency-direction` | PASS |
| `check-global-plan-number-uniqueness` | PASS |
| `check-tooling-inventory` | PASS |
| `planning-tests` | PASS |
| `plan497-routing-consumer-lane` | PASS |
| `plan497-netdb-preferred-order` | PASS |
| `plan497-daemon-participation-unit` | PASS |
| `cargo-deny` | PASS |

Every one of these steps ran **locally** on this host. Nothing in this closure
is a CI claim.

## Invariant review

The plan's §4 invariants, each with its evidence.

1. **Default-off.** `netdb.routing_table_enabled` is `false` by default. With it
   off the daemon's behaviour is identical to today: the `netdb-participation`
   service is inert, and the lookup path never consults the table. Evidence:
   `the_default_off_netdb_participation_owner_is_inert`;
   `without_the_routing_table_the_flat_store_selects`; the config default test.
2. **No regression.** The routing table is a cache of derived state; the flat
   store is the authority. A lookup that would have succeeded still succeeds,
   and the candidate set is never narrowed. Evidence:
   `the_routing_table_does_not_narrow_the_flat_store_selection`,
   `the_routing_table_reorders_the_selection_without_narrowing_it`,
   `a_preferred_peer_the_flat_store_would_not_serve_is_ignored`,
   `the_preferred_path_never_asks_the_target`.
   **This invariant was not satisfied by the first implementation.** See §10
   (Findings by severity, M1) — the no-regression review caught it before the
   floor ran, and the implementation was changed.
3. **Every fallback is counted by reason.** Evidence:
   `a_disabled_consumer_falls_back_and_counts_the_reason`,
   `a_poisoned_table_falls_back_and_counts_the_reason`,
   `a_disabled_table_falls_back_to_the_flat_store_and_counts_the_reason`,
   `an_empty_table_falls_back_to_the_flat_store_and_counts_the_reason`. The
   reason vocabulary is three named values, and the counters live outside the
   lock so a poisoned table's fallback cannot escape the count.
4. **Fail closed.** A table that cannot yield a queryable candidate does not
   fail the lookup; it falls back and the fallback is counted. The same
   discipline the peer-eligibility gate uses.
5. **No floor is lowered.** No existing configuration key changes meaning; the
   one new key is additive, bounded and default-off. The boundary script's floors
   were **raised** (positive-control 14→30, raw-scan 17→23), not lowered.
6. **Bounded.** The table's entry and bucket bounds, the maintenance cadence
   (`[1_000, 3_600_000]` ms), the candidate ceilings (`MAX_PINNED_CANDIDATES`
   64, the policy's candidate ceiling 32, the policy's suggested-hash ceiling),
   the projection size (`DEFAULT_MAX_PROJECTION_BYTES`), the startup directory
   scan (64 entries) and the startup file read (ceiling + 1 bytes) are all
   bounded. No unbounded channel or queue is introduced.
7. **Explicit ownership and cancellation.** The maintenance owner and the
   routing-table owner are two children under the service's `ChildScope`; both
   stop on the service's cancellation token. A child that cannot be registered
   is logged and degrades the participation feature and nothing else. The
   lookup path holds the table's lock only long enough to read a plan — never
   across a socket operation.
8. **Nothing is published and nothing is advertised.** `specs/support.toml`,
   `Cargo.toml`, `Cargo.lock`, `caps`, the published RouterInfo and
   `router.version` are unchanged. The routing table is internal state.
   Evidence: `git diff --stat -- specs/support.toml Cargo.toml Cargo.lock` is
   empty.

## Failure review

| Failure | Behaviour | Evidence |
| --- | --- | --- |
| A malformed projection at startup | Discarded and counted; never applied; never a startup failure. | `the_netdb_participation_owner_discards_a_corrupt_projection` |
| An oversized projection | Discarded and counted; the read is bounded at ceiling + 1 bytes, so an oversized file is refused rather than read. | `an_oversized_projection_is_discarded_and_counted` (Plan 488 row), `read_bounded` |
| A projection built for another UTC day | Discarded, counted, and removed. | `the_netdb_participation_owner_discards_a_projection_from_another_day` |
| An unreadable projection file | Discarded and counted. | `apply_projection` |
| A table with no queryable candidate for the key | Falls back to the flat store; counted under `NoCandidate`. | `an_empty_table_falls_back_to_the_flat_store_and_counts_the_reason` |
| A disabled table | Falls back to the flat store; counted under `Disabled`. | `a_disabled_table_falls_back_to_the_flat_store_and_counts_the_reason` |
| A poisoned table | Cannot answer; falls back; counted under `Unavailable`. | `a_poisoned_table_falls_back_and_counts_the_reason` |
| A maintenance tick that fails | The participation feature degrades; the router's lookups continue from the flat store. | The service is `Optional` in both senses and the consumer's fallback path is the store. |
| A child that cannot be registered | Logged as a warning; the feature degrades and nothing else does. | `register_netdb_participation_service`, `NetDbParticipation::start` |
| The coordinator is absent from the daemon | The shared consumer has no production reader; the router behaves exactly as before. | The coordinator's only construction sites are test code. |

## Migration review

- **No migration.** No stored format changes. The projection bytes are exactly
  Plan 488's encoding; the daemon adds no framing of its own.
- **No data-directory layout change for an existing deployment.** The
  projection is written to `<data_dir>/peer-profiles/`, a new subdirectory of
  the existing data directory. With the routing table off — the default — the
  service is inert and never creates it.
- **Backward compatible.** With `netdb.routing_table_enabled = false`, the
  daemon's behaviour is identical to today, and the lookup path takes the same
  branch it took before.
- **Additive configuration.** One new key inside the existing
  `deny_unknown_fields` `[netdb]` section. No existing key changes meaning, and
  no existing key is reinterpreted.

## Security review

- **No secret is logged or serialised.** The receipts Plan 488 established are
  sanitized counts only; Plan 497 adds no receipt vocabulary. The projection is
  sanitized and digest-framed. The service logs counts, reasons and a warning
  — never a peer identity.
- **Hostile bytes are bounded.** The startup scan examines at most 64 directory
  entries; the file read is capped at the projection ceiling plus one byte, so
  an oversized file is refused rather than read; the file name is parsed
  strictly (an eight-character all-ASCII-digit day key), and a name that does
  not parse is skipped rather than interpreted.
- **Evidence is never inferred from another peer.** The table indexes a peer
  only on that peer's own signed `f` advertisement. A peer's *suggested*
  evidence is a separate path, and the suggesting peer must itself be indexed
  with signed evidence or the table refuses the suggestion. The daemon passes
  an empty suggestion list today — Plan 489's exploration is not composed into
  the service graph — but the rule is enforced by construction so a suggestion
  cannot be mistaken for evidence when that source arrives.
- **The crate boundary is now enforced by the routine floor.** The
  `i2pr-netdb` section keeps the crate runtime-neutral: no Tokio, no async
  functions, no sockets, no filesystem, no clock, no tasks, and no dependency on
  a persistence, transport or runtime owner.
- **No capability promotion, no advertisement.** `specs/support.toml` stays at
  94 surfaces (88 experimental, 5 deferred, 1 ready) with 0 advertised;
  `router.version` stays `0.9.62`; `caps` and the published RouterInfo are
  unchanged. The routing table is internal state and nothing about it is
  advertised or published.
- **No network behaviour change.** The new code opens no socket, binds no port,
  spawns no process, and changes no wire format.

## Documentation and operations

- `docs/architecture/i2pr-daemon.md` — the routing-table consumer, the
  `netdb-participation` service, the startup sequence, the additive key, the
  projection handling, the table-driven selection, and the coordinator
  limitation.
- `docs/architecture/i2pr-netdb.md` — the preferred-order entry point, the two
  differing ordering metrics, and the relationship to Plan 489's closed-list
  entry point.
- `docs/architecture/tooling.md` — the `i2pr-netdb` boundary section and its
  raised floors.
- `specs/CONFORMANCE.md` — the exact claim and the exact non-claims.
- No operator procedure is needed: the feature is default-off and the one new
  key is documented in the daemon deep-dive.

## Limitations

Stated plainly, because the closure record is the authority a later reader will
check first.

1. **The coordinator is not composed into the daemon.** `NetDbTunnelCoordinator`
   is not created by the daemon today — every construction of it in the tree is
   in test code. Plan 497 delivers the **seam** Plan 433's composition will
   consume; Plan 433 owns the composition and is registered `blocked` on Plans
   431 and 432. Until Plan 433 lands, the shared routing-table consumer is
   reachable only from the daemon crate: the table is populated and persisted
   in a live daemon, and no live lookup consults it yet.
2. **The evidence is a private controlled mesh, not a public-network
   measurement.** The discriminator, the fallback rows and the boundary rows
   are all in-process. No public-network claim is made.
3. **The stock-peer NetDb exploration lane remains Plan 493's.** The
   exploration needs two routers that can reach each other: the authorized
   two-host topology Plan 493 owns.
4. **The `caps=f` advertisement remains Plan 494's**, gated on Plan 493.
5. **The peer-profile store stays empty in a live daemon.** `ProfileStore::adopt`
   consumes the `RouterInfoStore` by value and the daemon's NetDB must keep it,
   so the participation owner keeps its own store. The peer-profile *records*
   are therefore empty until an observation source is wired — outside this
   plan's scope. The routing table, which is what this plan was asked to
   populate from the daemon's NetDB, is populated.
6. **The projection's currency is carried by the file name**, not by the
   projection format. Plan 488's `ProfileProjection` has no UTC-day field and
   the plan did not change the format.
7. **The peer budget in `LookupPolicy` is declared but not enforced by the
   engine** — see §10, L2. The attempts are bounded by the candidate list
   instead. This is pre-existing and unchanged by this plan.

## Findings by severity

### High

None open. No high-severity finding was recorded by this plan.

### Medium

- **M1 — The first implementation of the table-driven selection violated the
  plan's own no-regression invariant, and the no-regression review caught it
  before the floor ran.** The first implementation replaced the flat store's
  candidate list with the routing table's list. Because the table's queryable
  set is always a **subset** of the flat store's floodfill set — `store.get()`
  and `store.floodfill_advertisers()` filter on the same predicate — that
  strictly *removed* candidates the flat store could serve, and Plan 489's
  `start_with_candidates` freezes the list at lookup start, so a peer ingested
  while the lookup runs would never have been queried. **Fixed before the
  floor:** the selection now computes the flat store's selection first and
  only reorders it, and the plan gained its own engine entry point
  (`start_with_preferred`) whose preference is a priority prefix rather than a
  closed list. Plan 489's entry point was left untouched and a test pins the
  difference. Evidence: `the_routing_table_does_not_narrow_the_flat_store_selection`,
  `the_routing_table_reorders_the_selection_without_narrowing_it`,
  `the_preferred_path_still_reaches_a_peer_ingested_mid_lookup`,
  `the_closed_candidate_list_does_not_reach_a_peer_ingested_mid_lookup`.
- **M2 — The shared routing-table consumer has no production reader.** The
  daemon populates and persists the table, and the coordinator asks it, but the
  coordinator is not in the daemon's service graph. Plan 433 owns that
  composition and is blocked. This is recorded rather than worked around:
  Plan 497 does not duplicate blocked work under another number.

### Low

- **L1 — The `i2pr-api` boundary section's published rule count is off by
  one.** `scripts/check-runtime-boundaries.sh:762` prints `(8 rules, ...)`
  while `i2pr_api_patterns` carries seven entries, and `AGENTS.md` repeats the
  "8-rule" figure. Pre-existing and unrelated to this plan; it needs its own
  plan-of-record because it changes a published figure.
- **L2 — `LookupPolicy::max_peers_per_lookup` is declared but not enforced by
  the lookup engine.** The accessor exists and the policy validates the
  relationship to `max_candidates_considered`, but no code path reads the
  budget; the attempts are bounded by the candidate list instead. Pre-existing,
  unchanged by this plan, and out of its scope. It is recorded because this
  plan touched the engine's candidate selection and the reader should know the
  bound that actually holds.
- **L3 — `docs/architecture/i2pr-daemon.md` still carries stale module-row line
  counts** outside the rows this plan touched. Pre-existing; `AGENTS.md` already
  records it as a known drift.

### Informational

- **I1 — The crate-local boundary guard remains in place.**
  `crates/i2pr-netdb/tests/profile_boundary.rs` duplicates a rule that now
  belongs in the shared script. Plan 488's informational finding F5 expected
  the duplication to be removed once F1 closed. It was left in place: removing
  an executable guard is not this plan's change, and keeping it costs nothing
  but a duplicated scan.

## Roadmap disposition

| Roadmap | Disposition |
| --- | --- |
| `plans/subsystems/network-participation-roadmap.md` | Plan 497's row moves to `closed` with the closure token `passed-netdb-participation-consumer-phase` and the closure record path. No other row moves. |
| `plans/subsystems/core-router-recovery-roadmap.md` | Unchanged. Plan 497 did not discharge a core-router-recovery row: the coordinator's composition remains Plan 433's blocked scope. |

## Unblock audit

The plan's §14 requires the unblock audit to record the disposition of the Plan
488 and Plan 489 findings this plan set out to discharge.

| Finding | Disposition |
| --- | --- |
| Plan 488 **F1** — `scripts/check-runtime-boundaries.sh` has no `i2pr-netdb` section. | **Closed.** The section exists with the script's positive controls, a negative control and a per-category completeness assertion, and the script's floors were raised. Floor step `check-runtime-boundaries` passes. |
| Plan 488 **F3** — the daemon's periodic maintenance owner has no consumer. | **Closed.** `run_profile_maintenance` runs in the daemon's service graph, owned by the `netdb-participation` service's child scope, and stops on cancellation. |
| Plan 488 **F4** — the projection is not written to disk. | **Closed.** The projection is persisted under the daemon's data directory and loaded at startup when it is present and current, and discarded and counted otherwise. |
| Plan 489 **§11.2** — "the routing table is not wired into the router's tunnel building." | **Partially discharged.** The routing table now drives the NetDB tunnel-building coordinator's candidate selection and the discriminator proves it. The finding's **production** half — the coordinator's placement in the daemon's service graph — stays **open and recorded** against Plan 433, which is registered `blocked` on Plans 431 and 432. |

**Nothing was moved to `ready`.** The audit considered every registered plan
that lists Plan 497 as a dependency, and every one of them is blocked on an
external environment this host does not have:

| Plan | State after this closure | Why it stays blocked |
| --- | --- | --- |
| 433 | `blocked` | Its work package 2 and 4 — composing the NetDB / destination-tunnel coordinator into the daemon's service graph — are exactly the production half left open above. It needs Plans 431 and 432. |
| 435 | `blocked` | The external two-family public evidence on an authorized two-host topology. |
| 439 | `blocked` | Its external environment. |
| 493 | `blocked` | The authorized two-host topology; the stock-peer lane and the `caps=f` advertisement wait on it. |
| 494 | `blocked` | Gated on Plan 493. |

Plan 488's and Plan 489's own closure records are **left exactly as written**.
They are the historical record of those plans; the disposition of their findings
is recorded here and in the registry rows, which is where a later reader looks.

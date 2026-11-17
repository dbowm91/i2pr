# Plan 489 — Floodfill-indexed routing table, exploration job, and optimized lookup

**Status:** `passed-floodfill-indexed-routing-table`
**Date closed:** 2026-10-23
**Subsystem:** network participation (phase 2 of 7)
**Implementation plan:** [`plans/implementation/network-participation/489-floodfill-indexed-routing-table-and-exploration.md`](../../implementation/network-participation/489-floodfill-indexed-routing-table-and-exploration.md)
**Roadmap:** [`plans/subsystems/network-participation-roadmap.md`](../../subsystems/network-participation-roadmap.md)
**Implementation commits:** `40a15a87` — *feat(network-participation): plan 489 floodfill-indexed routing table, exploration job, and optimized lookup*; `921fbe12` — *test(network-participation): cover the plan 489 routing table, exploration, and optimized lookup*; `25ea2f4a` — *docs(network-participation): document the plan 489 routing table, exploration, and optimized lookup*; `b0fb92fc` — *fix(network-participation): export the plan 489 pinned-candidate bound for rustdoc* (a documentation defect the floor caught, committed separately so this record can report a green floor honestly; it is **not** part of Plan 489's deliverable); `c39da7cd` — *fix(deps): update ctutils for RUSTSEC-2026-0385* (a dependency-security fix the floor caught, committed separately for the same reason; it is **not** part of Plan 489's deliverable)
**Predecessor:** Plan 488 (`passed-peer-profile-store`), whose peer profile store, failure model, score and selection policy this phase consumes.
**Owner boundary:** `crates/i2pr-netdb` (runtime-neutral table, job, plan), `crates/i2pr-netdb-persist` (optional persistence), `crates/i2pr-daemon` (driver, receipts, configuration).

---

## 1. Scope

Plan 489 replaces the flat peer map's *selection inputs* with a **bounded,
day-keyed Kademlia routing table over the floodfill set only**, and supplies
the two consumers that draw from it: a bounded exploration job, and a pure
optimized lookup plan.

Delivered:

- a **routing table** that indexes floodfills and nothing else, keyed by the
  day's routing key, with a distance-bucketed partition, deterministic
  furthest-first eviction, and closest-first selection;
- **floodfill evidence** as a provenance record — a peer whose own signed
  `RouterInfo` advertises `f` is `Signed`; a peer a verified floodfill named
  is `Suggested` until its own record is resolved;
- a **daily rotation** that recomputes every position against the new UTC
  day without letting the table grow, and a **retention rule** that stops a
  restart from resurrecting an expired table;
- an **exploration job** with a peer budget, a deadline, an outstanding-request
  table with a hard ceiling, and cancel-safe release;
- an **optimized lookup plan** that splits the table's peers into *queryable*
  and *pending*, refuses a peer whose resolved record shows it is not a
  floodfill, and orders both lists closest-first;
- a **lookup-engine extension** that lets a lookup draw its candidates from
  an explicit ordered list instead of the flat store, without weakening the
  existing contract;
- a **daemon-side routing-table driver** that indexes, explores, drives the
  optimized lookup and emits sanitized receipts, with an explicit clock;
- a **`[netdb]` configuration section** whose routing-table and exploration
  keys both default to off;
- **optional persistence** of the table as a bounded, sanitized, digest-framed
  projection;
- a **private controlled-mesh lane** and its sanitized evidence artifact.

Deliberately **not** delivered, by design:

- no network role. The router still enters the network only through a reseed,
  still carries no other router's traffic, and is still **not a participant**;
- no advertisement of any kind. `router.version`, `caps`, the published
  RouterInfo, and the support inventory's advertisement flags are unchanged;
- no wiring of the routing table or the optimized lookup into the router's
  tunnel building. Plan 489 delivers the table and the policy that draws from
  it; the wiring is the consumer's change and belongs to the participation
  phases that follow;
- no Kademlia table, no introducer role, no version floor, no transit posture
  and no floodfill advertisement. Those are Plans 490, 491, 492 and 494, and
  they are explicitly out of this phase's scope.

---

## 2. Baseline at registration

At registration (`86d7a070`) `crates/i2pr-netdb/src/routing.rs` had XOR
distance and a daily key but **no bucket set**: the NetDb peer map was still
the flat `RouterInfoStore` (Plan 103) — a bounded map with no tiers, no
day-keyed partition and no notion of a routing-table membership. The
selection primitives in `lookup_policy.rs` ordered candidates by distance
under a candidate ceiling, a peer budget and an exclusion set; they drew
their candidates from the flat store's floodfill advertisers, not from a
curated routing set. `lookup_engine.rs` knew two lookup kinds — `RouterInfo`
(wire type 2) and `LeaseSet2` (wire type 1) — and had no way to accept an
explicit candidate list.

The daemon had no routing-table owner, no exploration job, and no
`[netdb]` key for either. `i2pr-netdb-persist` owned the cache loader, the
reseed ingestor and the floodfill record envelope; it had no routing-table
artifact.

---

## 3. Implementation commits

| Commit | Subject | Content |
| --- | --- | --- |
| `40a15a87` | feat(network-participation): plan 489 floodfill-indexed routing table, exploration job, and optimized lookup | `i2pr-netdb`: `bucket_set.rs`, `exploration.rs`, `optimized_lookup.rs`, the `LookupKind::Exploratory` variant and `start_with_candidates` in the lookup engine. `i2pr-netdb-persist`: `routing_table.rs`. `i2pr-daemon`: the routing-table driver in `netdb_participation.rs` and the `[netdb]` configuration keys. |
| `921fbe12` | test(network-participation): cover the plan 489 routing table, exploration, and optimized lookup | The private controlled-mesh lane `crates/i2pr-daemon/tests/netdb_routing_mesh.rs`, the sanitized evidence artifact, and the extended crate-local static boundary guard. |
| `25ea2f4a` | docs(network-participation): document the plan 489 routing table, exploration, and optimized lookup | The four architecture deep-dives, `specs/CONFORMANCE.md` and `specs/support.toml`. |
| `b0fb92fc` | fix(network-participation): export the plan 489 pinned-candidate bound for rustdoc | The floor's `cargo doc` step rejected an intra-doc link to a private item. Not part of the deliverable. |
| `c39da7cd` | fix(deps): update ctutils for RUSTSEC-2026-0385 | The floor's `cargo deny` step flagged a newly published advisory against a transitive RustCrypto dependency. Not part of the deliverable. |

---

## 4. Requirement-to-evidence matrix

| # | Requirement | Evidence | Where |
| --- | --- | --- | --- |
| R1 | **Runtime neutrality.** The new modules gain no `tokio::*`, no `std::net`, no `std::fs`, no `async fn`, and no clock read. | The crate-local static guard scans every production source of `i2pr-netdb` and forbids `tokio`, `std::net`, `std::fs`, `async fn`, `tokio::spawn`, `JoinHandle`, `insert_unchecked`, `std::time::Instant` and `std::time::SystemTime`. The guard asserts that the three Plan 489 modules are among the files it scans. | `crates/i2pr-netdb/tests/profile_boundary.rs` — `the_crate_sources_carry_no_forbidden_construct`, `the_guard_scans_the_plan_489_modules`, `the_guard_reports_a_clock_read` |
| R2 | **Bounded everything, with a release path on every drop path.** | The table's total and per-bucket ceilings, the job's peer budget, deadline and outstanding ceiling, the plan's two ceilings, the projection's byte ceiling, the receipts' cap, and the persisted artifact's ceiling are all hard limits. Every removal — explicit, eviction, rotation, clear, cancel, timeout, reply — releases its accounting. | `bucket_set.rs` — `the_table_honours_a_capacity_of_one`, `the_table_honours_the_exact_load`, `the_table_refuses_the_max_plus_one`, `the_bucket_residue_is_released_on_every_drop_path`; `exploration.rs` — `the_exploration_releases_every_drop_path`, `the_outstanding_table_is_bounded`; `routing_table.rs` — `the_projection_is_bounded` |
| R3 | **The existing lookup contract is not weakened.** | `start_with_candidates` is an addition. The deadline, the peer budget, the duplicate coalescing, the suggestion merging and the fail-closed terminal states are unchanged, and the pinned list is truncated to `MAX_PINNED_CANDIDATES`. A lookup that used to fail closed still fails closed. | `lookup_engine.rs` — `start_with_candidates`, `advance_after_reply`; the crate's existing 13 `lookup_engine` unit tests still pass unchanged |
| R4 | **Exact-consumption decodes and checked arithmetic; a malformed input is a typed error, never a panic and never a silent drop.** | The projection decoder validates the magic, the version, the record count, the exact length and the body digest, and every length computation is checked. A corrupt, truncated, oversized or over-long projection is a typed `RoutingTableProjectionError`. The exploration's reply accounting counts dropped suggestions rather than discarding them silently. | `bucket_set.rs` — `a_corrupt_projection_is_refused`, `a_truncated_projection_is_refused`, `an_oversized_projection_is_refused`, `an_invalid_capacity_is_refused`; `exploration.rs` — `suggestions_are_capped_and_the_rest_are_counted` |
| R5 | **Only floodfills are indexed.** | `insert_signed` refuses a peer whose own `RouterInfo` does not advertise `f` and counts the refusal. `insert_suggested` accepts a suggestion only from a peer already indexed with `Signed` evidence. A `Suggested` entry is promoted when its own record is resolved, and `retain_verified` evicts a peer whose resolved record shows it is not a floodfill. | `bucket_set.rs` — `a_non_floodfill_is_rejected_and_counted`, `a_suggestion_needs_a_verified_source`, `a_suggestion_is_upgraded_by_its_own_signed_evidence`, `a_verified_non_floodfill_is_evicted`; `optimized_lookup.rs` — `a_known_non_floodfill_is_refused`; the mesh lane's refusal assertion |
| R6 | **The answering side does not change.** | The plan adds no server-side behaviour. The `DatabaseSearchReply` the peer sends is consumed by the existing engine's `handle_search_reply`; the `DatabaseStore` the peer sends is consumed by the existing engine's `handle_database_store`. The only engine change is the exploratory guard: a `DatabaseStore` may not complete an exploratory lookup. | `lookup_engine.rs` — `handle_database_store`'s `LookupKind::Exploratory` guard; the mesh lane drives the existing `handle_search_reply` and `handle_database_store` paths |
| R7 | **No advertisement.** | `router.version`, `caps` and the published RouterInfo are unchanged. The new support entry is `experimental` with `advertised = false`, and the inventory still has zero advertised surfaces. The `[netdb]` keys default to off. | `specs/support.toml` — `netdb.floodfill-indexed-routing-table`; the inventory check (91 surfaces, 0 advertised); `config.rs` — `netdb_defaults_apply_when_section_is_omitted` |
| R8 | **The bucket set: distance ordering, deterministic eviction, bounded select.** | The table splits into 256 buckets by the count of leading zero bits of the XOR distance from the anchor; within a bucket the peers are ordered by distance to the anchor; eviction removes the furthest peer with the highest `RouterHash` breaking a tie; a peer that is not strictly closer is refused. | `bucket_set.rs` — `eviction_is_deterministic`, `the_table_honours_its_bucket_bound`, `the_table_honours_its_total_bound`, `the_bucket_index_saturates` |
| R9 | **The date-keyed rotation and the retention rule.** | Positions are `daily_routing_key(router_hash, date)`. `rotate` recomputes every position against the new day and the new anchor, and the entry count never grows across a rotation. A table built for an earlier day is recognised as expired, and the persisted projection carries its day. | `bucket_set.rs` — `the_day_rotation_recomputes_positions_without_growing_the_table`, `the_day_rotation_is_a_no_op_inside_one_day`, `an_expired_table_is_recognised`; the daemon's `a_restart_does_not_resurrect_an_expired_table` |
| R10 | **The exploration job: budget, deadline, cancel-safe.** | A peer budget, a deadline, an outstanding table with a hard ceiling, a per-reply suggestion ceiling, and a cancel that releases every slot while consuming the budget. A timeout releases the slot but keeps the query counted. | `exploration.rs` — `the_peer_budget_caps_the_exploration`, `the_deadline_ends_the_exploration`, `cancelling_releases_every_slot_and_stops_the_job`, `a_timeout_releases_the_slot_but_keeps_the_budget_spent`, `a_peer_is_never_issued_twice`; the daemon's `the_exploration_budget_ends_the_job` |
| R11 | **The optimized lookup: the queryable/pending split and the closest-first choice.** | The plan splits the table's peers into queryable (the router holds the `RouterInfo`) and pending (it does not), refuses a peer whose resolved record shows it is not a floodfill, and orders both lists by XOR distance with the lower `RouterHash` breaking a tie. A pending peer is never asked: the lookup returns `NeedsPeerResolution` instead. | `optimized_lookup.rs` — `a_known_floodfill_is_queryable`, `a_known_non_floodfill_is_refused`, `the_plan_is_closest_first`, `excluded_peers_are_never_offered`, `the_plan_is_bounded`; the daemon's `a_lookup_without_a_resolved_peer_needs_resolution` |
| R12 | **The daemon driver and the receipts.** | The driver indexes, explores, drives the lookup and emits receipts that carry counts and a digest and **never** a peer identity. The lookup is not dispatchable without a reply path. | `crates/i2pr-daemon/src/netdb_participation.rs` — `the_receipts_never_name_a_peer`, `a_lookup_without_a_reply_path_is_not_dispatchable`, `the_exploration_request_is_an_exploratory_lookup`, `the_lookup_kind_is_the_router_info_kind`; the mesh lane's receipt and evidence assertions |
| R13 | **The `[netdb]` configuration section, off by default.** | Six new keys, all defaulting to off or to their documented bounds, with the exploration refused at parse time when the routing table is off. | `config.rs` — `netdb_defaults_apply_when_section_is_omitted`, `the_routing_table_and_the_exploration_can_be_enabled`, `the_exploration_needs_the_routing_table`, `the_routing_table_bounds_are_enforced`, `the_exploration_bounds_are_enforced` |
| R14 | **Persistence across a restart: bounded, sanitized, and expired-aware.** | The projection is bounded before it reaches the byte cache, carries its day, and is digest-framed. A missing, corrupt, oversized or malformed projection is discarded and counted, never a startup failure. A reply that was in flight before a restart cannot complete a lookup afterwards. | `routing_table.rs` — `the_projection_round_trips_through_the_cache`, `an_absent_projection_is_not_an_error`, `a_corrupt_projection_is_discarded`, `the_projection_can_be_removed`; the daemon's `a_reply_from_before_a_restart_does_not_complete_a_lookup` |
| R15 | **The private-mesh lane, with source-originated receipts and a sanitized artifact.** | The lane drives the driver, not a hand-fed counter: the peer the lookup asks is asserted to be a member of the routing table and is asserted **not** to be the store-only peer, which is the discriminator between a table-driven lookup and a hand-supplied peer. The artifact holds counts and digests only and is proven byte-identical across runs by the floor. | `crates/i2pr-daemon/tests/netdb_routing_mesh.rs`; `tests/integration/network-participation/evidence/plan489/result.json`; the floor's `plan489-mesh-lane` and `plan489-evidence-deterministic` steps |

---

## 5. Work packages and outcomes

| Work package | Outcome | State |
| --- | --- | --- |
| WP1 — the vocabulary: tiers, states, bounds, typed errors | `ProfileTier`-style ordering is not reused; Plan 489 introduces its own vocabulary — `FloodfillEvidence`, `TableEntry`, `TableCandidate`, `TableInsertOutcome`, `BucketSetConfig`, `BucketSetConfigError`, `RoutingTableProjectionError` — each with a bounded value domain and a typed refusal. | done |
| WP2 — the bucket set | `BucketSet` in `crates/i2pr-netdb/src/bucket_set.rs`: a 256-bucket partition anchored at the router's own daily routing key, deterministic furthest-first eviction with a documented tie-break, a bounded total, closest-first selection, and a bounded sanitized projection. | done |
| WP3 — the daily rotation and the retention rule | `BucketSet::rotate` recomputes every position against the new day and the new anchor without growing the table; `is_expired` recognises a table built for an earlier day; the projection carries its day so a restart cannot resurrect it. | done |
| WP4 — the exploration job | `ExplorationJob` in `crates/i2pr-netdb/src/exploration.rs`: a peer budget, a deadline, an outstanding table with a hard ceiling, a per-reply suggestion ceiling, and cancel-safe release. The request itself reuses the existing lookup engine with the new `LookupKind::Exploratory` (wire type 3). | done |
| WP5 — the optimized lookup | `plan_optimized_lookup` in `crates/i2pr-netdb/src/optimized_lookup.rs`: a pure function over the table and the flat store that produces the queryable/pending split and refuses a non-floodfill. The engine gains `start_with_candidates`, which draws its candidates from the plan's order. | done |
| WP6 — restart continuity | `crates/i2pr-netdb-persist/src/routing_table.rs`: the bounded, sanitized, digest-framed projection over the byte cache, with an absent/corrupt/oversized projection discarded rather than fatal. The daemon's `load_projection` counts the discard. | done |
| WP7 — the private-mesh lane | `crates/i2pr-daemon/tests/netdb_routing_mesh.rs` and the committed artifact `tests/integration/network-participation/evidence/plan489/result.json`. | done |

### Design decisions

1. **The table indexes floodfills, and the evidence is a provenance record
   rather than a trust signal.** A peer's own signed `RouterInfo` is the only
   thing that can put it in the table with `Signed` evidence. A suggestion
   from a peer is accepted only when that peer is itself `Signed`. This is
   stricter than "the table holds floodfill hashes": it is the only shape in
   which the "only floodfills are indexed" rule is actually enforceable at
   the moment of insertion, because a peer that has just been *named* by
   someone else has not been observed yet.

2. **The anchor is the router's own daily routing key.** The partition is
   anchored at `daily_routing_key(local_router_hash, date)` — the libp2p
   Kademlia shape, and the one the plan's handoff notes point at. The anchor
   decides the partition and which peer is evicted first; it never decides
   which peer a lookup prefers, because selection is target-relative.

3. **A rotation recomputes positions; it does not empty the table.** The
   plan's phrase "the retention rule that drops the previous day's table" is
   implemented as: the *persisted* previous-day table is dropped (the
   projection carries its day, and a restart against a different day does not
   use it), while the in-memory peer set survives the boundary with its
   positions recomputed. The alternative — clearing the table at every UTC
   midnight — would leave the router with an empty routing set every day and
   would make the entry count trivially bounded rather than meaningfully
   bounded. The record states this reading explicitly because the plan's
   wording admits both.

4. **Selection is a full bounded scan with a final sort, not a bucket walk.**
   The bucket structure governs the *persistence* discipline — the partition,
   the per-bucket capacity and the eviction. Selection needs the true
   closest peers to an arbitrary target, and a bucket walk from the target's
   bucket cannot guarantee that: peers closest to the target are spread
   across the anchor's buckets, not concentrated in one. The implementation
   therefore sorts the bounded candidate set by distance to the target. The
   sort is bounded by `MAX_TABLE_SIZE` (1 024) and is deterministic.

5. **`SCORE`-style reuse was rejected; the Plan 488 score is not reused.**
   Plan 488's `ProfileScore` ranks a peer's *observed behaviour*. Plan 489
   needs a *membership* decision — is this peer in the routing set, and on
   what evidence — and the two would have to be conflated into one number
   for no benefit. The two vocabularies stay separate.

6. **The lookup engine gains a candidate source, not a replacement.** The
   plan's invariant is that the existing lookup contract is not weakened. The
   implementation adds `start_with_candidates` and `advance_after_reply` and
   leaves `start`, `advance_after_attempt_timeout`, the deadline, the peer
   budget, the duplicate coalescing and the suggestion merging untouched. The
   pinned path never re-selects against the flat store behind the caller's
   back, and the candidate list is truncated to `MAX_PINNED_CANDIDATES`.

7. **An exploratory lookup is never completed by a `DatabaseStore`.** Its
   answer is a `DatabaseSearchReply`; a `DatabaseStore` that happens to name
   the random search key must not complete the exploration, because the
   peer list is the only thing the exploration consumes.

8. **The driver keeps an explicit clock, like its Plan 488 sibling.** Every
   time input — the publication date, the day boundary, the exploration
   deadline — is supplied by the caller, so the table's contents and the
   exploration's behaviour are reproducible. The wall clock is read only by a
   periodic owner, and Plan 489 does not add one.

9. **The Plan 489 driver extends the Plan 488 module rather than adding a
   second one.** The plan names `crates/i2pr-daemon/src/netdb_participation.rs`
   and offers "new, or extended from Plan 488's module". Extending keeps one
   explicit-clock convention and one receipt discipline in one place, which
   is what a reviewer of the two phases will want.

10. **The runtime-neutrality guard stayed crate-local.** The guard already
    scanned every production source of `i2pr-netdb`; Plan 489 extends it
    with a clock read and with an assertion that the three new modules are
    among the files it scans. Adding a section to
    `scripts/check-runtime-boundaries.sh` would change a published CI surface
    and needs its own plan-of-record; the same reasoning Plan 488 recorded
    still stands.

---

## 6. Invariant review

The plan states seven invariants. Each is reviewed against the delivered
code.

1. **Runtime neutrality.** Held. The three new `i2pr-netdb` modules import
   only `std::collections`, `std::fmt`, `i2pr_proto` and the crate's own
   modules. They read no clock, open no socket and touch no file. The
   crate-local guard enforces it, with an inverted probe for the clock read
   and an assertion that the new modules are scanned.

2. **Bounded everything, with a release path on every drop path.** Held. The
   table's total and per-bucket ceilings, the job's peer budget, deadline
   and outstanding ceiling, the plan's two ceilings, the projection's byte
   ceiling, the receipts' cap and the persisted artifact's ceiling are hard
   limits, and each removal path releases its accounting together — the
   table's entry map and its bucket partition are released in the same call,
   so no residue can survive a removal.

3. **The existing lookup contract is not weakened.** Held. The engine's
   public surface grew; nothing was narrowed. The deadline, the peer budget,
   the duplicate coalescing and the suggestion merging are the same code
   paths as before, and the fail-closed terminal states are unchanged.

4. **Exact-consumption decodes and checked arithmetic.** Held. The
   projection decoder validates the magic, the version, the record count,
   the exact expected length and the body digest, and every length
   computation uses checked arithmetic. The reply path uses the existing
   engine's exact-consumption decoders.

5. **Only floodfills are indexed.** Held. A peer whose own signed
   `RouterInfo` does not advertise `f` is refused and counted. A suggestion
   is accepted only from a `Signed` peer. A peer whose resolved record shows
   it is not a floodfill is evicted by `retain_verified` and refused by the
   optimized plan.

6. **The answering side does not change.** Held. No server-side behaviour
   was added. The plan consumes the existing `DatabaseSearchReply` and
   `DatabaseStore` paths.

7. **No advertisement.** Held. `router.version`, `caps` and the published
   RouterInfo are unchanged; the new support entry is `experimental` and
   `advertised = false`; the inventory still has zero advertised surfaces;
   the `[netdb]` keys default to off.

---

## 7. Verification

### 7.1 Focused runs

| Command | Outcome |
| --- | --- |
| `cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1` | **395 passed, 0 failed, 0 ignored** — 285 in-crate unit tests and 110 integration tests |
| `cargo test --locked -p i2pr-daemon --lib netdb_participation -- --test-threads=1` | **29 passed, 0 failed** — 17 Plan 489 routing-driver tests and 12 Plan 488 driver tests |
| `cargo test --locked -p i2pr-daemon --lib -- --test-threads=1` | **842 passed, 0 failed, 3 ignored** — the whole daemon library, including the 18 black-box app-runtime qualification tests |
| `cargo test --locked -p i2pr-daemon --test netdb_routing_mesh -- --test-threads=1` | **1 passed, 0 failed** — the private controlled-mesh lane; writes the evidence artifact |
| `cargo test --locked -p i2pr-netdb-persist --all-targets` | **13 passed, 0 failed** — including the four routing-table persistence tests |
| `cargo test --locked -p i2pr-netdb --test profile_boundary` | **6 passed, 0 failed** — the static boundary guard, with its inverted probes |

### 7.2 The AGENTS.md routine floor

The AGENTS.md routine floor ran on the closing tree as **72 steps: 72 PASS,
0 FAIL**. The tree the floor ran against is the closing tree — the last
commit before this record is `c39da7cd` — and the log is
`/tmp/i2pr-floor-final2.log`.

The floor is the AGENTS.md list **plus** the two Plan 489 steps, which are
**additions, not replacements**: the Plan 488 steps are still there, and the
Plan 489 steps are appended after them. The two new steps are:

- `plan489-mesh-lane` — `cargo test --locked -p i2pr-daemon --test
  netdb_routing_mesh -- --test-threads=1`, the private controlled-mesh
  lane; it writes the evidence artifact.
- `plan489-evidence-deterministic` — `git diff --exit-code -- tests/
  integration/network-participation/evidence/plan489/result.json`, which
  proves the lane's artifact is byte-identical across runs.

One host accommodation was applied and is recorded here rather than hidden:
the cargo steps ran with `CARGO_BUILD_JOBS=4`. This host has 15 GiB of RAM
and 16 cores, and the daemon's ~60 test binaries do not fit when 16 rustc
processes run at once — the first run was OOM-killed (see §7.4). The build
parallelism is a **host** property; no floor line, check, threshold or
linter was changed, relaxed or skipped.

| # | Step | Outcome |
| --- | --- | --- |
| 1 | `cargo-fmt` | **PASS** (4s) |
| 2 | `cargo-check` | **PASS** (0s) |
| 3 | `cargo-build-siblings` | **PASS** (0s) |
| 4 | `cargo-build-static-fixture` | **PASS** (0s) |
| 5 | `cargo-copy-static-fixture` | **PASS** (0s) |
| 6 | `cargo-rebuild-plain-fixture` | **PASS** (1s) |
| 7 | `cargo-test-workspace` | **PASS** (2290s) |
| 8 | `cargo-clippy` | **PASS** (28s) |
| 9 | `cargo-doc` | **PASS** (26s) |
| 10 | `cargo-test-doc` | **PASS** (10s) |
| 11 | `check-dependency-direction` | **PASS** (0s) |
| 12 | `check-global-plan-number-uniqueness` | **PASS** (0s) |
| 13 | `check-adr-number-uniqueness` | **PASS** (0s) |
| 14 | `check-portable-service-tunnel-api` | **PASS** (0s) |
| 15 | `check-portable-service-tunnel-consumer` | **PASS** (7s) |
| 16 | `planning-tests` | **PASS** (9s) |
| 17 | `check-runtime-boundaries` | **PASS** (1s) |
| 18 | `check-console-boundaries` | **PASS** (1s) |
| 19 | `check-console-browser-security` | **PASS** (0s) |
| 20 | `check-tooling-inventory` | **PASS** (0s) |
| 21 | `check-license-metadata` | **PASS** (0s) |
| 22 | `check-managed-app-private-client-seams` | **PASS** (0s) |
| 23 | `check-router-readiness-contract` | **PASS** (0s) |
| 24 | `check-router-readiness-contract-self` | **PASS** (0s) |
| 25 | `check-core-router-recovery-contract` | **PASS** (0s) |
| 26 | `check-core-router-recovery-contract-self` | **PASS** (0s) |
| 27 | `plan449-ssu2-preflight-self` | **PASS** (0s) |
| 28 | `check-managed-app-package-boundary` | **PASS** (0s) |
| 29 | `check-managed-app-package-boundary-self` | **PASS** (0s) |
| 30 | `check-managed-app-policy-boundary` | **PASS** (0s) |
| 31 | `check-managed-app-policy-boundary-self` | **PASS** (0s) |
| 32 | `check-managed-app-secured-sandbox` | **PASS** (0s) |
| 33 | `check-managed-app-secured-sandbox-self` | **PASS** (0s) |
| 34 | `check-service-tunnel-boundaries` | **PASS** (0s) |
| 35 | `check-managed-app-gateway-boundary` | **PASS** (0s) |
| 36 | `check-managed-app-manager-boundary` | **PASS** (1s) |
| 37 | `check-m11-per-epoch-composition` | **PASS** (0s) |
| 38 | `check-service-anonymity-boundaries` | **PASS** (0s) |
| 39 | `check-fixture-manifest` | **PASS** (0s) |
| 40 | `check-ntcp2-vectors` | **PASS** (0s) |
| 41 | `check-ssu2-vectors` | **PASS** (0s) |
| 42 | `check-i2cp-vectors` | **PASS** (9s) |
| 43 | `check-ntcp2-interoperability` | **PASS** (0s) |
| 44 | `check-constrained-host-lane-boundary` | **PASS** (0s) |
| 45 | `check-m11-transit-boundaries` | **PASS** (1s) |
| 46 | `check-m11-transit-qualification-evidence` | **PASS** (1s) |
| 47 | `check-sam-acceptance-evidence` | **PASS** (0s) |
| 48 | `check-ssu2-acceptance-evidence` | **PASS** (0s) |
| 49 | `check-i2cp-acceptance-evidence` | **PASS** (0s) |
| 50 | `check-i2pcontrol-acceptance-evidence` | **PASS** (0s) |
| 51 | `check-els2-type11-transcript-boundary` | **PASS** (0s) |
| 52 | `check-els2-live-lane-evidence` | **PASS** (2s) |
| 53 | `check-encrypted-service-consumer-caller` | **PASS** (0s) |
| 54 | `check-outproxy-request-path` | **PASS** (0s) |
| 55 | `check-outproxy-wire-lane-evidence` | **PASS** (0s) |
| 56 | `check-config-secret-hygiene` | **PASS** (0s) |
| 57 | `check-workflow-validity` | **PASS** (0s) |
| 58 | `check-floodfill-type5-serve` | **PASS** (1s) |
| 59 | `check-service-tunnel-acceptance-evidence` | **PASS** (0s) |
| 60 | `check-exploratory-tunnel-evidence` | **PASS** (0s) |
| 61 | `check-netdb-tunnel-evidence` | **PASS** (0s) |
| 62 | `check-destination-tunnel-evidence` | **PASS** (0s) |
| 63 | `check-streaming-tunnel-evidence` | **PASS** (6s) |
| 64 | `check-m6-mixed-router-acceptance-evidence` | **PASS** (3s) |
| 65 | `check-m12-floodfill-qualification-evidence-self` | **PASS** (0s) |
| 66 | `check-m12-floodfill-boundaries-self` | **PASS** (1s) |
| 67 | `ntcp2-harness-tests` | **PASS** (0s) |
| 68 | `plan488-mesh-lane` | **PASS** (0s) |
| 69 | `plan488-evidence-deterministic` | **PASS** (0s) |
| 70 | `plan489-mesh-lane` | **PASS** (1s) |
| 71 | `plan489-evidence-deterministic` | **PASS** (0s) |
| 72 | `cargo-deny` | **PASS** (1s) |

### 7.3 The advisories the floor caught

`cargo deny check advisories bans sources` — **PASS** on the closing tree.

The Plan 489 tree introduces **no new dependency**: the work uses only crates
already in the locked graph (`i2pr-crypto`, `i2pr-proto`, `i2pr-storage`,
`rand_chacha`, `rand_core`, `flate2`, `serde_json`, `tempfile`, `thiserror`,
`sha2`). The plan's own dependency surface is therefore unchanged.

The floor nevertheless caught **one advisory** on the closing tree, and it is
recorded here rather than quietly absorbed:

- **RUSTSEC-2026-0385 — `ctutils` < 0.4.4.** `ctutils::Choice`'s mask
  generation for the unsigned integer sizes is `const fn`, so it cannot reuse
  the `asm!`-backed mask generation in `cmov`; before 0.4.4 the generated
  code could include a branch on a secret value on commonly used platforms,
  and the `u128` comparisons `Choice` provides were branching on 32-bit
  targets. `ctutils` is a transitive RustCrypto dependency of
  `curve25519-dalek`; it is **not** introduced by Plan 489 and was locked at
  0.4.2 before this closure. The fix (`c39da7cd`) is the patch bump
  `ctutils` 0.4.2 → 0.4.5, the same shape Plan 488's closure used for
  `webpki-roots` 1.0.9 → 1.0.10 and `rustls-webpki` 0.103.15 → 0.103.16.
  No API change, no new dependency, no feature or license change.

The lockfile change was re-verified by the **whole** floor, not by the
advisory step alone: the closing tree's floor ran again from the top after
`c39da7cd`.

### 7.4 The failures the floor caught, and what each cost

Three failures were observed on the way to a green floor. None of them is a
defect in the delivered behaviour, and none of them was suppressed.

**1. The workspace test build was OOM-killed, not failed.** The first floor
run reported `cargo-test-workspace FAIL`. The cause is visible in the log and
is not a code defect: every failing unit is
`process didn't exit successfully: ... (signal: 15, SIGTERM: termination
signal)` — the host's rustc processes were killed while the daemon's ~60 test
binaries were compiled in parallel. The host has 15 GiB of RAM and 16 cores.
The re-run with `CARGO_BUILD_JOBS=4` compiled and ran the same tree
successfully. The build parallelism is a **host** property, not a property of
the tree; no floor line, check or threshold was changed.

**2. `cargo doc` rejected a private intra-doc link.** The first floor run
reported `cargo-doc FAIL` with
`public documentation for 'start_with_candidates' links to private item
'MAX_PINNED_CANDIDATES'`. This one *is* a defect in the delivered change, and
it is a real one: the bound lives in the private `lookup_engine` module while
`start_with_candidates` is re-exported publicly, so the crate's public
documentation could not name the limit it imposes. The fix (`b0fb92fc`)
re-exports the bound, because it *is* part of the crate's public contract — it
caps the candidate list a caller may pin — rather than unlinking the
reference or weakening the rustdoc lint. The re-run of
`RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` is
**clean**. The finding is recorded in §12 as *Medium — found and fixed in this
closure*.

**3. The app-runtime qualification tests could not find their sibling
binaries.** The first standalone re-run of the workspace test reported 18
failures in `app_runtime_qualification`, every one of them of the form
`i2pr-app-fixture-manager was not found at target/debug/...` or
`i2pr-app-fixture-secured was not found at target/debug/...`. This is the
trap `AGENTS.md` documents: the daemon's black-box qualification execs **real
sibling executables**, so the sibling binaries and the static fixture must be
built first. The cause here was the *first* failure: the OOM-killed build had
left the target directory in a state where the next build rebuilt the
`i2pr-app-fixture` package and removed the uplifted binaries instead of
reusing them. Running the documented procedure in full —
`cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd
-p i2pr-appctl`, then
`cargo rustc --locked -p i2pr-app-fixture --bin i2pr-app-fixture -- -C
target-feature=+crt-static`, then the copy to
`target/debug/i2pr-app-fixture-secured`, then the plain rebuild — restored all
three binaries, and the 18 tests then passed
(`18 passed, 0 failed`). A separate control confirmed that a workspace build
does **not** remove the uplifted binaries once the target directory is
consistent: `cargo test --locked --workspace --all-targets --no-run` left
`i2pr-app-fixture`, `i2pr-app-fixture-manager` and
`i2pr-app-fixture-secured` in place. No Plan 489 change is implicated in any
part of this.

### 7.5 The false-pass audit

| Check | Result |
| --- | --- |
| Does any test pass without exercising the production path? | No. The mesh lane drives `RoutingTableDriver`, the `i2pr-netdb` engine and the byte cache; it never writes a receipt or an artifact by hand. |
| Could the mesh lane distinguish a table-driven lookup from a hand-supplied peer? | No — and the lane proves it cannot. `zeta` is in the flat store and **not** in the routing table, and the lane asserts the peer asked is a table member and is **not** `zeta`. A driver that ignored the table would ask `zeta` and fail. |
| Does any artifact carry a peer identity? | No. The lane renders the full receipt list and the full evidence JSON and asserts that neither contains any of the six peers' hashes. The receipts carry counts and a digest; the artifact carries counts and digests. |
| Is the evidence artifact deterministic? | Yes. The floor's `plan489-evidence-deterministic` step runs the lane and then `git diff --exit-code` on the artifact; the lane is seeded, so the artifact is byte-identical across runs. |
| Does the boundary guard fail open? | No. It carries its own inverted probes: a synthetic violating source must be reported, a clock read must be reported, and a source that only *mentions* a forbidden construct in a comment or inside a `#[cfg(test)]` block must not. |
| Is any forbidden pattern used to make a check pass? | No. No `|| true`, no `continue-on-error`, no filename filtering, no fake peer environment, no broad exclusion, no early-return-success, and no production wire change to go green. |
| Is any test gated on an external environment? | No. Every Plan 489 test is local, in-process and deterministic. |

---

## 8. Migration and compatibility

- **No wire change.** The `DatabaseLookup` body is the existing one; the only
  new wire value is the *lookup type* `3`, which is the I2P standard
  exploratory lookup type and is selected by the new `LookupKind::Exploratory`
  variant. No existing kind's wire code moved.
- **No persisted-format change.** The Plan 103 `RouterInfoStore` and the
  Plan 104/105 persistence artifacts are untouched. The routing table's
  projection is a **new** artifact with its own magic prefix
  (`i2pr-rtable`), its own version byte and its own digest; it cannot be
  confused with an existing cache entry, and the byte cache's fixed
  64-hex-character filename keeps it in its own namespace.
- **No configuration change for an existing operator.** The six new
  `[netdb]` keys default to off or to their documented bounds, and the
  section is `deny_unknown_fields`, so a configuration that never mentioned
  them parses unchanged.
- **No behavioural change for an existing deployment.** The table and the
  optimized lookup are not wired into the router's tunnel building. The
  only production paths Plan 489 adds are inert until a consumer calls them.
- **The lookup engine's public surface grew.** `start_with_candidates`,
  `advance_after_reply`, `LookupKind::Exploratory` and
  `MAX_PINNED_CANDIDATES` are additions. No existing signature changed, and
  no existing behaviour was narrowed.

---

## 9. Security and contention

- **No secrets.** The table holds `RouterHash`es and evidence labels. The
  receipts hold counts and a digest. The projection holds `RouterHash`es,
  evidence labels, a day index and a digest. No private key, no session
  secret, no signing seed, no token and no payload appears in any of them.
  The mesh lane asserts this against the rendered receipts and the rendered
  artifact.
- **No peer identity in a receipt.** A receipt that could name a peer is not
  a receipt. The receipts are constructed from counts only, and the digest
  covers the counts.
- **The table is not a trust store.** The evidence records *provenance*, not
  trust. A `Signed` entry means "this peer published `f`", not "this peer is
  good". A malicious peer can therefore be in the table, exactly as it can be
  in the flat store; what the table guarantees is narrower — the table holds
  floodfills and nothing else.
- **A suggestion cannot widen the table.** Only a `Signed` peer may
  introduce another peer, so a peer that is not a verified floodfill cannot
  grow the table, directly or transitively.
- **The bound is enforced on the path that grows it.** The table's total and
  per-bucket ceilings are checked before the insertion, and the exploration's
  outstanding ceiling is checked before an issue. There is no path that
  appends to an unbounded container.
- **Contention.** The driver is single-threaded and owns its table, its two
  engines and its receipts; no lock is held across an `.await`. The plan's
  crate-level types are `Send`/`Sync` by construction — they hold only owned
  data — and the crate carries `#![forbid(unsafe_code)]`.
- **No new listener, no new port, no new process.** The driver owns no
  socket. The `[netdb]` keys do not enable anything by themselves.
- **The persisted artifact is bounded before it is written.** The projection
  is length-checked against the byte cache's own ceiling and against the
  table's projection ceiling before it reaches the filesystem, so an
  oversized table is refused rather than written.

---

## 10. Documentation and operational changes

- `docs/architecture/i2pr-netdb.md` — the three new modules in the module
  table and the public surface, a new key-contract section for the routing
  table, the exploration job and the optimized lookup, the new `MAX_*` bound
  constants with their sites, and the updated test inventory.
- `docs/architecture/i2pr-netdb-persist.md` — the optional routing-table
  persistence, its bounds and its discard policy.
- `docs/architecture/i2pr-daemon.md` — the routing-table driver, the new
  `[netdb]` configuration keys and their defaults, and the new configuration
  hard caps.
- `docs/architecture/overview.md` — the netdb and daemon ownership
  paragraphs.
- `specs/CONFORMANCE.md` — a new section stating what is and is not claimed.
- `specs/support.toml` — the plan status keys and one new surface entry,
  `netdb.floodfill-indexed-routing-table`, `experimental` and
  `advertised = false`. The inventory moves from 90 to 91 surfaces, still
  with 0 advertised.
- **Operational.** Nothing to deploy and nothing to watch: the table and the
  optimized lookup are not wired into the router, and the configuration keys
  that expose them default to off.

---

## 11. Known limitations

1. **The lane uses in-process synthetic peers, not a stock i2pd peer.** The
   plan's "Required tests" names a pinned single-family lane against i2pd
   2.61.0. Plan 489 does not provide it. A stock-peer NetDb exploration lane
   needs two routers that can reach each other, which is the authorized,
   independently addressed two-host topology that **Plan 493** owns and that
   Plan 431 stopped at. Running a stock-peer lane here would mean
   fabricating a topology this repository is not authorized to create. The
   lane delivered is a private controlled mesh, the same shape Plan 488
   accepted for the same class of requirement, and the discriminator it
   proves — the candidate came from the table and not from the flat store —
   is a property of the driver, not of the peer's implementation. Recorded
   as an open finding in §12.

2. **The routing table is not wired into the router's tunnel building.**
   By design. The table and the optimized lookup exist and are exercised;
   the consumer that would use them to build tunnels is a later phase.

3. **The table must be rebuilt after every restart** unless the persisted
   projection is loaded, and the projection is only useful within the UTC
   day it was built for. The table is therefore a cache of derived state,
   not a durable store. This is stated in the module's own documentation.

4. **The exploration's suggestion accounting trusts the answering peer.**
   A peer may name peers that are not floodfills. The table accepts them
   only with `Suggested` evidence, and they are never queried until their
   own record is resolved and shows `f` — so a lying peer can cost the
   table some entries, but it cannot make a non-floodfill queryable.

5. **The `Suggested` evidence has no expiry of its own.** A `Suggested`
   entry that is never resolved stays in the table until the table's own
   bound evicts it. The bound is the only pressure. A per-entry age for
   unresolved suggestions would be a reasonable refinement, and it is not
   part of this phase.

6. **The `advance_after_reply` transition is a naming addition.** The
   lookup engine's state machine has one "move to the next candidate"
   transition; Plan 489 added a second name for it so a caller can describe
   an answered attempt honestly. The behaviour is the existing one.

7. **Bandwidth metering remains unresolved**, in this repository and in both
   reference routers. Plan 489 does not need it — the table indexes the
   *floodfill set*, not a capacity class — but Plan 494's tier derivation
   will still have to face it.

---

## 12. Findings by severity

### Medium — found and fixed in this closure

**F0 — The public documentation could not name the limit it imposes.** The
floor's `cargo doc` step rejected
`public documentation for 'start_with_candidates' links to private item
'MAX_PINNED_CANDIDATES'`. The bound is part of the crate's public contract —
it caps the candidate list a caller may pin — so the fix re-exports it
(`b0fb92fc`) rather than unlinking the reference or weakening the lint. The
step is clean on the re-run.

### Medium — open

**F1 — The crate-local boundary guard is the only runtime-neutrality
enforcement for `i2pr-netdb`.** `scripts/check-runtime-boundaries.sh` still
has **no `i2pr-netdb` section**, so the crate's neutrality is enforced only
by `crates/i2pr-netdb/tests/profile_boundary.rs`. Plan 488 recorded the same
gap; Plan 489 widened the guard's scope (it now also forbids a clock read)
without closing the underlying gap, because adding a section to a published
CI script needs its own plan-of-record and the rule must be **fixed**, not
weakened. The gap is a coverage hole, not a licence to add the forbidden
edge. Owner: a future plan-of-record in the same family as the transport-scoped
rule the `AGENTS.md` known-gaps list already records.

**F2 — No stock-peer lane.** See §11.1. The private controlled mesh proves
the driver's behaviour but not interoperability with a reference router's
`DatabaseSearchReply`. The reference behaviour that matters here — that a
`DatabaseSearchReply` names the peers closest to the requested key, and that
an exploratory lookup carries wire lookup type `3` — is taken from the I2P
specification and from the pinned i2pd 2.61.0 source, and is asserted
offline, but it is not exercised against a live peer. Closing it needs the
Plan 493 topology.

### Low — open

**F3 — The routing-table driver has no production consumer.** The driver
exists, is exercised and is bounded, but nothing in the router calls it yet.
The same finding shape as Plan 488's F3, and it belongs to the first
consumer phase.

**F4 — The projection is not written to disk at startup.** The persistence
exists and is exercised in the `i2pr-netdb-persist` crate, but the daemon
does not yet load or save the table at startup, so the restart-continuity
path is available but not taken. This belongs to the first consumer phase.

### Informational

**I1 — The rotation's reading of "drops the previous day's table".** The
plan's wording admits two readings; this closure implements the one recorded
in §5, decision 3, and states it explicitly rather than letting the code
imply it.

**I2 — The lookup engine grew a second name for one transition.** See
§11.6.

### No findings

- No secret appeared in a receipt, an artifact, a log line or a projection.
- No unbounded channel, queue or container was introduced.
- No boundary script was weakened, and no forbidden pattern was used to make
  a check pass.
- No dependency was added.
- No advertisement was made, and no capability, version or RouterInfo
  behaviour was claimed beyond the tested subset.

---

## 13. Roadmap disposition

The network-participation roadmap's phase 2 is closed. The sequence now
reads **488 (closed) → 489 (closed) → 490 (ready)**.

- **Phase 2 — the floodfill-indexed routing table and the exploration job
  (Plan 489): closed** as `passed-floodfill-indexed-routing-table`. The
  table, the exploration job, the optimized lookup plan, the engine
  extension, the daemon driver, the configuration keys, the persistence and
  the lane are delivered and evidenced.
- **Phase 3 — the SSU2 introducer relay role (Plan 490): ready.** Plan 489
  delivers the routing set the introducer would be selected from, and the
  introducer's own relay mechanics are untouched by it.
- Phases 4–7 (Plan 491 version floors, Plan 492 transit posture, Plan 493
  two-family qualification, Plan 494 floodfill role and `caps=f`) remain
  registered. Plan 494 is still blocked on Plan 493's two-family evidence,
  and `caps=f` must stay refused until then.

**Carried into the next phase.** The two open findings that a consumer
phase should close: F3 (no production consumer) and F4 (the projection is
not yet written at startup). The two findings that need their own
plan-of-record: F1 (the `i2pr-netdb` boundary section) and F2 (the
stock-peer lane, which needs the Plan 493 topology).

---

## 14. Unblock audit

The audit covers every registered plan that lists Plan 489 as a hard or
interface dependency, and the roadmap's dependency graph.

| Plan | Depends on 489? | Other hard dependencies | Interface dependency stable? | Disposition |
| --- | --- | --- | --- | --- |
| 490 — SSU2 introducer relay role | yes — the introducer is selected from the routing set | Plan 489 closed; Plan 431's transport activation is not required for the role's mechanics | yes — the routing set's membership, evidence and selection surface are stable and documented | **ready** |
| 491 — API version floors and peer eligibility | yes — eligibility draws on the routing set | Plan 489 closed | yes — the table's public surface is stable | ready (unchanged) |
| 492 — transit participation posture | yes — the posture draws on the routing set | Plan 489 closed | yes | ready (unchanged) |
| 493 — two-family public participation qualification | yes — the qualification observes the routing set | Plan 489 closed; **Plan 431's transport activation and the authorized two-host topology are still open** | yes | **blocked** (unchanged) |
| 494 — floodfill role and `caps=f` advertisement | yes — the floodfill role draws on the routing set | Plan 489 closed; **Plan 493's two-family evidence is still open** | yes | **blocked** (unchanged) |

No plan needed a corrective pass. No plan is silently unblocked: the two
blocked plans were already blocked before Plan 489 closed, for reasons Plan
489 does not touch, and both stay blocked.

---

## 15. Open items

| Item | Severity | Owner | Note |
| --- | --- | --- | --- |
| F1 — `check-runtime-boundaries.sh` has no `i2pr-netdb` section | Medium | a future plan-of-record | The rule must be fixed, not weakened. The crate-local guard covers the crate meanwhile. |
| F2 — no stock-peer (pinned i2pd 2.61.0) exploration lane | Medium | Plan 493's topology | Needs the authorized two-host topology; a stock-peer lane cannot be fabricated here. |
| F3 — the routing-table driver has no production consumer | Low | the first consumer phase | 490–494 own the wiring. |
| F4 — the projection is not yet written at startup | Low | the first consumer phase | The persistence exists and is exercised; the daemon does not call it yet. |
| I1 — the rotation reading | Informational | none | Recorded in §5, decision 3. |
| I2 — `advance_after_reply` | Informational | none | A naming addition over the existing transition. |

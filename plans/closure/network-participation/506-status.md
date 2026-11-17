## Status

`passed-peer-knowledge-reconcile`

Plan 506 is closed. The router now **carries the peers it accepts on a live
session into the two stores it actually builds with** — the destination
coordinator's authoritative store and the peer-profile store — and the
population Plan 505's exploratory driver draws from **grows** instead of
being frozen at the startup snapshot.

**The gap was a missing feed, not a missing mechanism.** Three production
sites ingest an authenticated unsolicited `DatabaseStore` into the bootstrap
NetDB, so a peer the router accepts on a live session *is* known to the
bootstrap. But `insert_validated_router_info` had exactly **one** production
caller — the startup seed — and `learn_bootstrap_peers` had exactly **one**
production caller, also at startup. A peer learned after startup was
therefore invisible to both stores the router builds with.

**The framing was corrected, and the correction matters.** Plan 505 recorded
that "the validated store is still not refreshed from live NetDB lookups".
That phrasing was wrong in a way that changed the work: the router does
**not** learn peers from a NetDB lookup at all. A `DatabaseStore` carries
exactly one record, a `DatabaseSearchReply` carries peer **hashes only**, and
the production router runs **no** RouterInfo-kind lookup — the only
`begin_lookup` caller is Plan 433's never-constructed `NetDbTunnelCoordinator`.
The live-learned RouterInfos are exactly the unsolicited `DatabaseStore`s, so
Plan 506's subject is the **reconcile**, not a lookup. Plan 505's closure
record is **not** rewritten; the correction lives here, in the deep-dive's
forward pointer, and in the registry row.

**The feasibility question was answered by running code.** Plan 506's first
work package was an executable artefact that could stop the plan. It drove
the **real** `Bootstrap`, the **real** `DestinationTunnelCoordinator` and the
**real** `PeerParticipationDriver`, and the peer arrived through the
production unsolicited-store path. The answer was **yes** — and the artefact
is **not vacuous**, which was measured rather than asserted.

**A real defect was fixed, not just recorded.** The unsolicited store was
ingested **twice** for every message the router did not serve as a
floodfill: the second site's condition was a strict **subset** of the
first's, so the second write re-validated and re-applied the record and
produced `InsertOutcome::Idempotent`. Every accepted unsolicited store was
written twice. The two remaining ingest sites are in **mutually exclusive**
branches, so exactly one can run for any message.

**Nothing is promoted.** `router.version` stays `0.9.62`; `caps` and the
published RouterInfo are unchanged; the support inventory's advertised count
stays **zero**; `Cargo.toml` and `Cargo.lock` are byte-identical.

## Commits

| Commit | Subject |
| --- | --- |
| `bba19ecc` | docs(plans): register Plan 506, the peer-knowledge reconcile |
| `3efe34dd` | docs(plans): register Plan 506 in the registry and the roadmap |
| `b56ba67e` | feat(daemon): the peer-knowledge reconcile, and the WP1 feasibility gate |
| `9bdb0b89` | docs(plans): correct the Plan 506 production-change table |
| `d512c4ae` | fix(daemon): ingest the unsolicited RouterInfo store once, not twice |
| `6ec1890d` | feat(daemon): wire the peer-knowledge reconcile into the live product |
| `ac9d23f2` | test(daemon): the Plan 506 reconcile guard and the live reconcile lane |
| `f091d7b8` | docs(plans): the Plan 506 conformance, deep-dive and support entry |
| `<closure>` | docs(plans): close Plan 506 and advance the network-participation roadmap |

## What changed

### The gap

The router accepts a peer's RouterInfo on a **live** session. Three
production sites in `crates/i2pr-daemon/src/lib.rs` ingest an authenticated
unsolicited `DatabaseStore` into the bootstrap NetDB. The two stores the
router actually **builds with** were not:

- the **destination coordinator's authoritative store** — the store the
  destination build path reads its peers from — was seeded **once at
  startup** by `insert_validated_router_info`, and never again;
- the **peer-profile store** — the store Plan 505's driver selects from and
  records observations into — was fed **once at startup** by
  `learn_bootstrap_peers`, and never again.

So a peer the router learned after startup was known **only to the
bootstrap**, and the population the build path drew from never grew.

### The framing correction: the router learns peers from unsolicited stores, not from lookups

Plan 505's limitation said the validated store "is still not refreshed from
live NetDB lookups". Read literally, that describes a missing refresh from a
lookup path. The tree says otherwise:

- `DatabaseStoreMessage` carries **exactly one** record
  (`crates/i2pr-proto/src/i2np/netdb.rs`), and `DatabaseSearchReplyMessage`
  carries peer **hashes only** — no RouterInfo bytes at all;
- the production router runs **no** RouterInfo-kind lookup. The only
  `begin_lookup` caller is Plan 433's never-constructed
  `NetDbTunnelCoordinator`
  (`crates/i2pr-daemon/src/netdb_tunnels.rs`);
- therefore the live-learned RouterInfos the router holds are **exactly** the
  unsolicited `DatabaseStore`s.

The consequence is that "refresh the store from live lookups" was the wrong
work item. The right one is a reconcile over the bootstrap's validated
RouterInfos — which is what Plan 506 built. Adding a RouterInfo-kind lookup
is a protocol-composition change with **its own** plan-of-record, and this
plan does not attempt it.

### One bounded operation, owned by the product

`crates/i2pr-daemon/src/peer_reconcile.rs` owns the reconcile. `run` is a
**step**, not a task: it holds no timer, owns no cancellation, and adds no
listener and no socket.

1. **Snapshot** the bootstrap's validated RouterInfos under the bootstrap's
   lock, and **release** that lock.
2. **Insert** each record into the destination store through the canonical
   `insert_validated_router_info`.
3. **Learn** the same records into the peer-profile store through the
   canonical `learn_all`.

One snapshot, one cadence, no skew between the two stores. The snapshot is
the bootstrap's own **bounded** publication, so a large NetDB cannot become
an unbounded copy.

### The three locks are taken in one order and are never nested

The bootstrap's lock is released before the destination coordinator's lock is
taken, and the destination coordinator's lock is released before the
peer-profile store's lock is taken. The peer-profile closure never awaits, so
no lock is held across an await.

This is asserted **structurally**, not by a whole-file token search. The
guard asserts that the source order of the three steps in `fn run` is the
release order, and that **each step's body may not name another step's
lock** — so `fn snapshot` cannot hold the destination coordinator's or the
peer-profile store's lock while it holds the bootstrap's, and so on. A
token search would not have caught this: all three locks appear in the file.

### Every outcome is counted by reason, and a refusal is never fatal

A capacity refusal, a conflict and a stale replacement are **counted** and
the batch **continues**. This is deliberately *unlike* the startup seed,
which treats a capacity refusal as fatal — the seed runs once, at a moment
when a refusal is worth surfacing; the reconcile runs on a cadence over a
store that may be at capacity for an unrelated reason, and one bad record
must never abort the batch.

The counters carry counts and **stable reason labels** only: no peer
identity, no address, no key material, no payload.

### The reconcile writes peer records, never a selection

Plan 491's version gate lives at **selection**, not at ingestion. The
reconcile writes peer **records** and therefore feeds the gate rather than
bypassing it: a refreshed peer below the floor is still **refused** at
selection and its refusal is still counted. Both the peer-profile store's
version-gated selection and the destination projection's per-record gate are
asserted to be intact by the guard.

### The cadence is the existing participation maintenance period

The composition root copies `config.netdb.participation_maintenance_period_ms`
out of the configuration and hands it to the product. There is **one** source
of truth and **no second configuration knob**: `config.rs` is **untouched**
and no new key is added. The reconcile's own bound is the same range the
configuration parser already enforces
(`[1 000 ms, 3 600 000 ms]`), so the reconcile's refusal cannot fire on a
configured router.

### A real defect was fixed: the unsolicited store was ingested twice

The second ingest site's condition — a `DatabaseStore` body with a bootstrap
store — was a strict **subset** of the first site's, so the second write was
never needed. Every accepted unsolicited store was written twice, and the
second write re-validated the record and re-applied it to the store to
produce `InsertOutcome::Idempotent`.

The removal is behaviour-preserving apart from the write count: the first
site already set `product_effect` for the qualification observer, and the
second site only emitted a debug log the first site's `info!` log already
covered. The two remaining sites are in **mutually exclusive** branches — the
`!ssu2_config.enabled` branch and the SSU2-enabled path — so exactly one can
run for any message.

### The counters are published, and the seam reports a gap

The product refreshes a counter cell on its tick; the composition root
publishes the same `Arc` into the inspection seam. An unpublished or
poisoned slot is a **gap**, never a fabricated zero.

### The selection-set change is a recorded finding

The destination projection is a **hash-ordered 256-prefix**
(`crates/i2pr-daemon/src/destination_peers.rs`: `store.iter().take(256)`).
Adding peers therefore does not merely add candidates — it can change
**which** peers the build path sees. Changing the ordering is out of scope;
recording the behaviour is not. This is a Medium finding below.

## Requirement-to-evidence matrix

The guard's rule names are the **landed** names in
`crates/i2pr-daemon/tests/peer_reconcile_boundary.rs`.

| Requirement | Evidence |
| --- | --- |
| The composition hands the product the bootstrap handle | `crates/i2pr-daemon/src/lib.rs`; guard rule `the_composition_must_hand_the_product_the_bootstrap_handle` |
| The composition hands the product the peer-profile handle | `crates/i2pr-daemon/src/lib.rs`; guard rule `the_composition_must_hand_the_product_the_peer_profile_handle` |
| The cadence is the existing participation maintenance period, not a second knob | `crates/i2pr-daemon/src/lib.rs`; `crates/i2pr-daemon/src/config.rs` is untouched; guard rule `the_cadence_must_be_the_existing_participation_period` |
| The product owns the reconcile and the spec carries it | `crates/i2pr-daemon/src/service_product.rs` |
| The product reconciles on its **existing** tick | `crates/i2pr-daemon/src/service_product.rs`; guard rule `the_product_must_reconcile_on_its_existing_tick` |
| The reconcile owns no task and no timer | `crates/i2pr-daemon/src/peer_reconcile.rs`; guard rules `the_reconcile_must_not_own_a_task_or_a_timer`, `the_reconcile_must_not_own_a_timer` |
| The locks are taken in one order and never nested | `crates/i2pr-daemon/src/peer_reconcile.rs`; guard rules `the_lock_order_must_be_snapshot_then_insert_then_learn`, `the_snapshot_must_not_hold_another_lock`, `the_insert_must_not_hold_the_bootstrap_lock`, `the_learn_must_not_hold_the_destination_lock` |
| The reconcile writes through the canonical write APIs | `crates/i2pr-daemon/src/peer_reconcile.rs`; guard rules `the_reconcile_must_write_through_the_canonical_write_apis`, `the_reconcile_must_learn_through_the_canonical_write_api` |
| The reconcile never writes a selection | `crates/i2pr-daemon/src/peer_reconcile.rs`; guard rule `the_reconcile_must_not_write_a_selection` |
| The reconcile is cadenced and the cadence is bounded | `crates/i2pr-daemon/src/peer_reconcile.rs`; guard rules `the_reconcile_must_be_cadenced`, `the_cadence_must_be_bounded` |
| A capacity refusal is counted, not fatal | `crates/i2pr-daemon/src/peer_reconcile.rs`; guard rules `a_capacity_refusal_must_be_counted_not_fatal`, `the_insert_must_not_abort_the_batch_on_a_refusal` |
| The unsolicited store is ingested once | `crates/i2pr-daemon/src/lib.rs`; guard rule `the_unsolicited_store_must_be_ingested_once` |
| The counters are published, and the seam reports a gap rather than a zero | `crates/i2pr-daemon/src/i2pcontrol_inspection.rs`; guard rules `the_counters_must_be_published_through_the_inspection_seam`, `the_seam_reader_must_return_a_gap_not_a_zero` |
| Plan 491's version gate stays at selection | `crates/i2pr-daemon/src/netdb_participation.rs`, `crates/i2pr-daemon/src/destination_peers.rs`; guard rules `the_version_gate_must_stay_at_selection`, `the_per_record_version_gate_must_stay_on_the_destination_path` |
| The feasibility question is answered by running code | `crates/i2pr-daemon/src/peer_reconcile.rs` — the in-module feasibility artefact |
| The feasibility artefact is not vacuous | the three negative controls, verified on the real tree |
| A live-learned peer is known to both build-with stores | `crates/i2pr-daemon/tests/peer_reconcile_live.rs` — `a_live_learned_peer_is_known_to_both_build_with_stores` |
| A below-floor peer stays refused and counted | `crates/i2pr-daemon/tests/peer_reconcile_live.rs` — `a_below_floor_peer_stays_refused_after_the_reconcile` |
| The reconcile reads a bounded snapshot | `crates/i2pr-daemon/tests/peer_reconcile_live.rs` — `the_reconcile_reads_a_bounded_snapshot` |
| The counters are reachable through the public inspection seam | `crates/i2pr-daemon/tests/peer_reconcile_live.rs` — `the_reconcile_counters_are_reachable_through_the_public_inspection_seam` |
| The lock order holds under contention | `crates/i2pr-daemon/tests/peer_reconcile_live.rs` — `the_reconcile_completes_while_the_stores_are_busy` |
| Nothing is advertised | `specs/support.toml` — 98 surfaces, 0 advertised |
| No new dependency | `git diff --exit-code <base> HEAD -- Cargo.toml Cargo.lock` |

## Commands run

### The routine floor

110 steps, run on the closing tree. The script is
`/tmp/i2pr-floor-506-run1.sh`; the log is `/tmp/i2pr-floor-506-run1.log`.
It is Plan 505's 108-step floor plus two Plan 506 steps
(`plan506-reconcile-boundary` and `plan506-reconcile-live`).

| # | Step | Result | Seconds |
| --- | --- | --- | --- |
| 1 | `cargo-fmt` | PASS | 3 |
| 2 | `cargo-check` | PASS | 7 |
| 3 | `cargo-build-siblings` | PASS | 0 |
| 4 | `cargo-build-static-fixture` | PASS | 1 |
| 5 | `cargo-copy-static-fixture` | PASS | 0 |
| 6 | `cargo-rebuild-plain-fixture` | PASS | 0 |
| 7 | `cargo-test-workspace` | PASS | 1297 |
| 8 | `cargo-clippy` | PASS | 2 |
| 9 | `cargo-doc` | PASS | 10 |
| 10 | `cargo-test-doc` | PASS | 3 |
| 11 | `check-dependency-direction` | PASS | 0 |
| 12 | `check-global-plan-number-uniqueness` | PASS | 0 |
| 13 | `check-adr-number-uniqueness` | PASS | 0 |
| 14 | `check-portable-service-tunnel-api` | PASS | 0 |
| 15 | `check-portable-service-tunnel-consumer` | PASS | 6 |
| 16 | `planning-tests` | PASS | 7 |
| 17 | `check-runtime-boundaries` | PASS | 1 |
| 18 | `check-console-boundaries` | PASS | 1 |
| 19 | `check-console-browser-security` | PASS | 0 |
| 20 | `check-tooling-inventory` | PASS | 0 |
| 21 | `check-license-metadata` | PASS | 0 |
| 22 | `check-managed-app-private-client-seams` | PASS | 0 |
| 23 | `check-router-readiness-contract` | PASS | 0 |
| 24 | `check-router-readiness-contract-self` | PASS | 1 |
| 25 | `check-core-router-recovery-contract` | PASS | 0 |
| 26 | `check-core-router-recovery-contract-self` | PASS | 0 |
| 27 | `plan449-ssu2-preflight-self` | PASS | 0 |
| 28 | `plan495-peer-eligibility-self` | PASS | 0 |
| 29 | `plan495-peer-eligibility-profile-drift` | PASS | 0 |
| 30 | `plan476-gate-binding` | PASS | 0 |
| 31 | `plan476-gate-binding-self` | PASS | 1 |
| 32 | `check-managed-app-package-boundary` | PASS | 0 |
| 33 | `check-managed-app-package-boundary-self` | PASS | 0 |
| 34 | `check-managed-app-policy-boundary` | PASS | 0 |
| 35 | `check-managed-app-policy-boundary-self` | PASS | 0 |
| 36 | `check-managed-app-secured-sandbox` | PASS | 0 |
| 37 | `check-managed-app-secured-sandbox-self` | PASS | 0 |
| 38 | `check-service-tunnel-boundaries` | PASS | 0 |
| 39 | `check-managed-app-gateway-boundary` | PASS | 0 |
| 40 | `check-managed-app-manager-boundary` | PASS | 0 |
| 41 | `check-m11-per-epoch-composition` | PASS | 0 |
| 42 | `check-service-anonymity-boundaries` | PASS | 1 |
| 43 | `check-fixture-manifest` | PASS | 0 |
| 44 | `check-ntcp2-vectors` | PASS | 0 |
| 45 | `check-ssu2-vectors` | PASS | 0 |
| 46 | `check-i2cp-vectors` | PASS | 0 |
| 47 | `check-ntcp2-interoperability` | PASS | 0 |
| 48 | `check-constrained-host-lane-boundary` | PASS | 0 |
| 49 | `check-m11-transit-boundaries` | PASS | 3 |
| 50 | `check-m11-transit-qualification-evidence` | PASS | 1 |
| 51 | `check-sam-acceptance-evidence` | PASS | 0 |
| 52 | `check-ssu2-acceptance-evidence` | PASS | 0 |
| 53 | `check-i2cp-acceptance-evidence` | PASS | 1 |
| 54 | `check-i2pcontrol-acceptance-evidence` | PASS | 0 |
| 55 | `check-els2-type11-transcript-boundary` | PASS | 0 |
| 56 | `check-els2-live-lane-evidence` | PASS | 2 |
| 57 | `check-encrypted-service-consumer-caller` | PASS | 0 |
| 58 | `check-outproxy-request-path` | PASS | 0 |
| 59 | `check-outproxy-wire-lane-evidence` | PASS | 0 |
| 60 | `check-config-secret-hygiene` | PASS | 0 |
| 61 | `check-workflow-validity` | PASS | 0 |
| 62 | `check-floodfill-type5-serve` | PASS | 0 |
| 63 | `check-service-tunnel-acceptance-evidence` | PASS | 1 |
| 64 | `check-exploratory-tunnel-evidence` | PASS | 0 |
| 65 | `check-netdb-tunnel-evidence` | PASS | 0 |
| 66 | `check-destination-tunnel-evidence` | PASS | 1 |
| 67 | `check-streaming-tunnel-evidence` | PASS | 5 |
| 68 | `check-m6-mixed-router-acceptance-evidence` | PASS | 4 |
| 69 | `check-m12-floodfill-qualification-evidence-self` | PASS | 0 |
| 70 | `check-m12-floodfill-boundaries-self` | PASS | 0 |
| 71 | `ntcp2-harness-tests` | PASS | 0 |
| 72 | `plan488-mesh-lane` | PASS | 1 |
| 73 | `plan488-evidence-deterministic` | PASS | 0 |
| 74 | `plan489-mesh-lane` | PASS | 0 |
| 75 | `plan489-evidence-deterministic` | PASS | 0 |
| 76 | `plan490-mesh-lane` | PASS | 0 |
| 77 | `plan490-evidence-deterministic` | PASS | 0 |
| 78 | `plan490-ssu2-transport-suite` | PASS | 6 |
| 79 | `plan490-transport-boundary-guard` | PASS | 0 |
| 80 | `plan491-mesh-lane` | PASS | 1 |
| 81 | `plan491-evidence-deterministic` | PASS | 0 |
| 82 | `plan491-boundary-guard` | PASS | 0 |
| 83 | `plan491-proto-suite` | PASS | 1 |
| 84 | `plan491-netdb-suite` | PASS | 87 |
| 85 | `plan491-client-suite` | PASS | 24 |
| 86 | `plan492-posture-suite` | PASS | 1 |
| 87 | `plan492-m11-lane` | PASS | 4 |
| 88 | `plan502-transit-composition-lane` | PASS | 0 |
| 89 | `plan502-transit-composition-unit` | PASS | 0 |
| 90 | `plan502-transit-boundary-self` | PASS | 0 |
| 91 | `plan496-private-lan-profile` | PASS | 1 |
| 92 | `plan496-ntcp2-activation-gate` | PASS | 0 |
| 93 | `plan497-routing-consumer-lane` | PASS | 0 |
| 94 | `plan497-netdb-preferred-order` | PASS | 1 |
| 95 | `plan497-daemon-participation-unit` | PASS | 2 |
| 96 | `plan498-introducer-handoff-lane` | PASS | 1 |
| 97 | `plan498-introducer-mesh-lane` | PASS | 0 |
| 98 | `plan498-bootstrap-resolver-row` | PASS | 25 |
| 99 | `plan498-runtime-relay-accessor` | PASS | 5 |
| 100 | `plan503-version-eligibility-live` | PASS | 1 |
| 101 | `plan503-version-eligibility-unit` | PASS | 0 |
| 102 | `plan504-liveness-boundary` | PASS | 0 |
| 103 | `plan504-liveness-live` | PASS | 1 |
| 104 | `plan504-liveness-unit` | PASS | 0 |
| 105 | `plan505-driver-boundary` | PASS | 0 |
| 106 | `plan505-driver-live` | PASS | 3 |
| 107 | `plan505-driver-unit` | PASS | 0 |
| 108 | `plan506-reconcile-boundary` | PASS | 5 |
| 109 | `plan506-reconcile-live` | PASS | 1 |
| 110 | `cargo-deny` | PASS | 0 |

**110 steps, 110 PASS, 0 FAIL.**


### The focused verification

| Command | Result |
| --- | --- |
| `cargo test --locked -p i2pr-daemon --test peer_reconcile_boundary -- --test-threads=1` | 28 passed |
| `cargo test --locked -p i2pr-daemon --test peer_reconcile_live -- --test-threads=1` | 7 passed |
| `cargo test --locked -p i2pr-daemon --lib peer_reconcile -- --test-threads=1` | 9 passed |
| `cargo test --locked -p i2pr-daemon --lib -- --test-threads=1` | 966 passed, 3 ignored |
| `cargo test --locked -p i2pr-daemon --test exploratory_driver_boundary --test exploratory_driver_live -- --test-threads=1` | 15 + 8 passed |
| `cargo test --locked -p i2pr-daemon --test tunnel_liveness_boundary --test tunnel_liveness_live -- --test-threads=1` | 26 + 9 passed |
| `cargo test --locked -p i2pr-daemon --test version_floor_boundary --test version_eligibility_live --test version_eligibility_mesh -- --test-threads=1` | 28 + 11 + 51 passed |
| `cargo test --locked -p i2pr-daemon --test exploratory_build_live --test destination_tunnel_unit --test netdb_tunnel_live -- --test-threads=1` | 11 + 9 + 4 passed |
| `cargo fmt --all --check` | clean |
| `cargo clippy --locked -p i2pr-daemon --all-targets --all-features -- -D warnings` | clean |
| `python3 scripts/check-tooling-inventory.py` | every published figure matches the tree |
| `python3 scripts/check-global-plan-number-uniqueness.py` | passed |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | 51 OK |

### The three negative controls, verified on the real tree

Each control mutated the real tree, was run, and was then reverted; the tree
was re-verified green after each.

| Control | Mutation | Observed failure |
| --- | --- | --- |
| The reconcile learns | remove the `learn_all(` call | 4 tests fail; the first with *"the peer must be selectable after the reconcile"* |
| The reconcile inserts | replace `insert_validated_router_info(record.clone())` with a constant | 5 tests fail; the first with *"the peer must be insertable into the destination store by the reconcile"* |
| The gate is not weakened | raise the below-floor peer's published version to `0.9.62` | 1 test fails with *"a below-floor peer must stay refused after the reconcile"* |

### Two guard bugs found while building the guard

Both are recorded because either would have made the guard pass vacuously,
and a guard that passes vacuously is worse than no guard.

- **The block anchor carried its own opening brace.** The block helper
  searches for the first `{` *after* the anchor, so an anchor ending in `{`
  made the composition rules read the wrong block — they could never have
  fired. Fixed by anchoring on the statement text without the brace.
- **The lock-order rule used the `?` operator** on its three step lookups.
  A **missing** step propagated `None` out of the rule and reported
  nothing: a fail-open. The rule now fails closed and says so.

## Invariant review

| Invariant | How it is held |
| --- | --- |
| No unbounded collection | the reconcile holds only the last run's timestamp and its counters; the snapshot is the bootstrap's own bounded publication |
| No unbounded queue or channel | none added |
| Every task has explicit ownership and cancellation | **no task was added** — the reconcile is a step on the existing tick |
| No listener, no socket, no timer | none added; asserted by the guard |
| The locks are taken in one order and never nested | asserted structurally by the guard; the peer-profile closure never awaits |
| Plan 491's version gate is not weakened | the reconcile writes records, not selections; both gates are asserted by the guard |
| A refusal is counted, never silently skipped | every outcome has a reason label and a counter |
| No dependency direction change | `scripts/check-dependency-direction.sh` passes; no new dependency |
| No capability advertisement | `specs/support.toml` — 98 surfaces, **0 advertised** |
| No production wire change | none — the reconcile reads and writes the router's internal stores only |

## Failure review

| Failure | Behaviour |
| --- | --- |
| The bootstrap store cannot be read (poisoned lock, or absent) | counted as `source-unavailable`; the reconcile returns `SourceUnavailable` and the tick is unaffected |
| The peer-profile store cannot be locked | counted as `store-unavailable`; the destination inserts that already happened stand |
| The destination store is at capacity | the record is refused and **counted**; the batch continues |
| The peer-profile store is at capacity | the record is refused and counted; the destination insert stands |
| A record conflicts with an existing one | refused and counted; the existing record is preserved |
| A record is older than the one held | refused as stale and counted |
| The cadence has not elapsed | the tick is skipped and counted as `cadence-skips`; nothing is read or written |
| The cadence is out of range | refused at construction; the product's start fails with a provisioning error — a configuration error, and the configuration parser already refuses the same value |
| The peer-profile handle or the bootstrap handle is absent | the reconcile owns no source, or no destination; the missing half is counted, never faked |
| The reconcile itself faults | the tick's reconcile step is a bounded, non-failing operation; the counters carry the reason |

**Nothing fails open.** Every refusal has a reason label and a counter, and
no path silently skips a peer.

## Migration review

There is **no migration**. The reconcile adds no stored state, changes no
on-disk format, and renames no configuration key:

- `config.rs` is **untouched**; `[netdb] participation_maintenance_period_ms`
  keeps its meaning and its bounds;
- no file under the router's data directory changes shape;
- the destination store's contents **grow** (or are refused at capacity),
  and the peer-profile store's contents grow;
- a router that upgrades simply finds that a peer it already accepted is now
  known to the stores it builds with;
- the two stores' existing records are **not** rewritten: the reconcile
  applies the same canonical write APIs, so an unchanged record is
  recognised as byte-identical and counted as idempotent.

## Security review

- **No new secret, key or token** is introduced, derived or persisted. The
  reconcile moves RouterInfo records the router already holds between stores
  it already owns.
- **No new identifier crosses a boundary.** The reconcile is entirely
  internal: it reads the bootstrap and writes the router's own stores.
- **The counters carry no peer identity, address, key or payload.** They are
  counts and stable reason labels, and the labels are a closed set the
  router defines.
- **The snapshot is bounded** by the bootstrap's own publication cap, so a
  hostile NetDB cannot make the reconcile allocate without limit.
- **The reconcile writes nothing to the network.** No I2NP message, no
  lookup, no lease, no publication.
- **No new configuration key** means no new operator-controlled surface that
  could be mistaken for an advertisement.
- **Nothing is advertised.** `router.version` stays `0.9.62`; `caps` and the
  published RouterInfo are unchanged; the support inventory's advertised
  count stays **zero**.
- **The duplicate-ingest removal is a security-positive change**: the
  unsolicited store was validated and applied twice per message, and one
  application is now enough.

## Documentation and operations

- `specs/support.toml` — one `experimental` surface,
  `netdb.peer-knowledge-reconcile`, with `advertised = false`. The inventory
  moves from **97 surfaces (91 experimental, 5 deferred, 1 ready)** to **98
  surfaces (92 experimental, 5 deferred, 1 ready)**; **0 advertised**.
- `specs/CONFORMANCE.md` — the Plan 506 section, with the explicit statement
  that the router learns peers from **unsolicited stores**, not from
  lookups, and that **no stock-peer evidence** exists.
- `docs/architecture/i2pr-daemon.md` — the `src/peer_reconcile.rs` module
  row and the Plan 506 section; a forward pointer into Plan 505's paragraph
  recording that its "not refreshed from live NetDB lookups" sentence was
  wrong and that the population limitation it described is discharged.
- `plans/registry.md` and `plans/subsystems/network-participation-roadmap.md`
  — the Plan 506 rows, updated to `closed`.

**Operations.** The reconcile is observable through the inspection seam's
`peer_reconcile_counters()` and `peer_reconcile_counters_snapshot()`. The
reason labels an operator will care about are `source-unavailable`,
`store-unavailable`, `capacity-exceeded`, `learned-capacity-exceeded` and
`conflict`. A rising `cadence-skips` count with a flat `runs` count would
mean the tick is not advancing; a rising `source-unavailable` count would
mean the bootstrap store is not readable.

## Limitations

- **The reconcile's population is the bootstrap's validated RouterInfos.**
  That is the right population — it is what the router has actually
  accepted — but it is still bounded by the bootstrap store's own quota and
  by the bootstrap's 256-record snapshot cap.
- **The destination store's capacity policy is unchanged.** It refuses at
  capacity and never evicts, so a router whose destination store is full
  will keep refusing new peers there. The reconcile counts the refusal; it
  does not add eviction, because changing what the store retains is a
  separate decision.
- **The projection's ordering is unchanged.** The hash-ordered 256-prefix
  means a refresh can change *which* peers the build path sees, not only how
  many. Recorded as a finding; not redesigned here.
- **The router performs no RouterInfo-kind NetDB lookup.** The refreshed
  population is the unsolicited stores the network sends. A lookup is its own
  plan-of-record.
- **No stock-peer evidence.** Whether an ordinary public router sends
  unsolicited stores this router will accept is **unproven**. The external
  lanes stay open until Plan 493's authorized topology is available.
- **Plan 497 limitation 5's read half is open.** The profile store is now
  fed and populated, but the routing table's only reader remains Plan 433's
  never-constructed `NetDbTunnelCoordinator`.
- **Plan 487 F3's destination-inbound half is open.** It needs tunnel
  rekeying, which the tree does not implement at all — no `rekey` symbol in
  `i2pr-proto`, `i2pr-tunnel` or `i2pr-client`, and `TunnelDataMessage` has no
  rekey payload.
- **The counters are not persisted.** A restart empties them; the stores'
  own contents persist.

## Findings by severity

### Medium

- **The selection set can change under a refresh.** The destination
  projection is a hash-ordered 256-prefix, so a peer the router learns can
  **evict** a peer the build path already saw, purely by hash order. This
  is a behaviour change the operator did not ask for, and it is the risk to
  watch. Changing the ordering is out of scope for this plan; the behaviour
  is recorded here and in the conformance text.
- **No stock-peer evidence of the whole loop.** A controlled mesh proves the
  mechanism; it does not prove that an ordinary public router sends
  unsolicited stores this router will accept, nor that the accept path
  reaches the reconcile.
- **The reconcile runs only where the product exists.** The product is
  constructed when the service-tunnel groups are active. A router that runs
  without them owns no reconcile, and its stores stay at the startup
  snapshot. This is the same scoping the destination coordinator already has.

### Low

- **The startup seed and the reconcile overlap.** `learn_bootstrap_peers`
  still runs at participation startup, and the reconcile's first tick
  learns the same records — the second learn is idempotent and counted, but
  the startup learn is no longer the only writer. It was **not** removed,
  because Plan 505's guard requires it in `fn start`, and because the
  participation owner exists whether or not the product does.
- **The counters are not persisted** across a restart.
- **The guard's module table in the deep-dive is stale** for 34 of its 59
  per-file line counts. This drift predates the plan; the plan did not
  widen it materially and did not fix it, because re-deriving 34 rows is a
  separate concern from this plan's subject. The **test-file** counts in the
  same section were corrected here, because the plan changed them.

### Recorded, not defects

- **The composition's `bootstrap` handle is cloned** before the borrow that
  shadows it. The clone is of an `Arc`, so both refer to the same store.
- **The reconcile's first tick runs before the participation owner's
  startup learn** in some orderings. Both learn the same records and the
  second is idempotent, so the store's contents do not depend on the order.

## The startup-only limitation Plan 505 recorded, and the other open items

**Plan 505's closure record §"Limitations" said:**

> **The driver's peer population is the bootstrap snapshot.** The validated
> store is still **not refreshed from live NetDB lookups**: RouterInfos
> learned from a live lookup are not inserted into it. The driver therefore
> names peers the router knew at startup.

**Its disposition.** **Partly discharged, and the sentence's premise is
corrected.** The population is no longer the startup snapshot: a peer the
router accepts on a live session is carried into the peer-profile store and
the destination store by the reconcile. But the sentence's premise was wrong
in a way that mattered — the router performs **no** RouterInfo-kind lookup,
so there was never a "live lookup" population to insert. The live-learned
RouterInfos are exactly the unsolicited stores, and those are now carried.

Plan 505's closure record is **not** rewritten. The correction lives in this
record, in the deep-dive's forward pointer, and in the registry row — the
same discipline Plan 503 applied to Plan 491 and Plan 504 applied to
Plan 185.

**The other open items, unchanged by this plan.**

| Item | State |
| --- | --- |
| Plan 491 F2 — no stock-peer evidence | **open**; needs Plan 493's authorized two-host topology |
| Plan 491 F4 — the intermittent `i2pcontrol_tunnels` staging failure | **open**; belongs to the I2PControl line |
| Plan 487 F2 — the SAM session-lifetime policy | Plan 485's recorded decision |
| Plan 487 F3's destination-inbound half | **open**; needs tunnel rekeying, which the tree does not implement |
| Plan 490 Medium #3 — the stock-peer introducer lane | **open**; external |
| Plan 497 limitation 5's read half | **open**; Plan 433's never-constructed `NetDbTunnelCoordinator` |
| Plan 502 M1 — the creator-correlation `BTreeSet`s grow without bound | **open** |
| Plan 503's lookup-target API-version gate | **open**; the router still asks a below-floor floodfill for a *lookup* |
| Plans 433, 435, 436, 437, 438, 439, 493, 494 | **blocked** on external environments |
| A RouterInfo-kind NetDB lookup | **open**; its own plan-of-record |
| The destination store's eviction policy | **open**; its own decision |
| The projection's ordering | **open**; the behaviour is recorded, the redesign is not |

## Roadmap disposition

`plans/subsystems/network-participation-roadmap.md` — the Plan 506 row moves
from `registered` to `closed`, with the closure record and the commits.

The roadmap's next open item is the **peer-knowledge** line's remainder: the
read half of Plan 497 limitation 5, the RouterInfo-kind lookup, and the
destination store's capacity policy. None of them is reachable from the
source side alone, and none is attempted here.

## Unblock audit

Plan 506 unblocks **nothing that was blocked**, and it is honest about that.
It discharges one limitation of an open plan (Plan 505's startup-only
population) and it fixes one product defect (the duplicate ingest).

Every item it leaves open is listed in the table above with the reason it
stays open, and each of those reasons is **external or a separate
plan-of-record** — not a piece of this plan that was quietly skipped:

- the external-environment blocks (Plans 433, 435, 436, 437, 438, 439, 493,
  494) need an authorized two-host topology this repository does not have;
- the stock-peer questions need that same topology;
- the RouterInfo-kind lookup, the destination store's eviction policy and the
  projection's ordering each change what the router stores or offers, and
  each therefore needs its own plan-of-record;
- Plan 433's composition is Plan 433's blocked scope, and Plan 506's stop
  conditions name it explicitly.

The plan's own stop conditions (§13.2) were all held: no wire-format change,
no new dependency, no weakening of Plan 491's gate or Plan 505's carve-out,
no two locks held at once, no lock held across an await, no composition of
`NetDbTunnelCoordinator`, and no change to the destination store's eviction
or to the projection's ordering.

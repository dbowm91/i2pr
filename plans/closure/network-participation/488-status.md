# Plan 488 — Peer profile store, tiering, failure model, score, and selection policy

**Status:** `passed-peer-profile-store`
**Date closed:** 2026-10-20
**Subsystem:** network participation (phase 1 of 7)
**Implementation plan:** [`plans/implementation/network-participation/488-peer-profile-store.md`](../../implementation/network-participation/488-peer-profile-store.md)
**Roadmap:** [`plans/subsystems/network-participation-roadmap.md`](../../subsystems/network-participation-roadmap.md)
**Implementation commits:** `c796bb0f` — *feat(network-participation): plan 488 peer profile store, failure model, and selection policy*; `256037ec` — *test(network-participation): cover plan 488 store adoption, replacement, and batch learn*; `a6bd6154` — *test(network-participation): cover the plan 488 selection capacity trio*; `8f6df1bf` — *fix(deps): update rustls-webpki and webpki-roots for RUSTSEC-2026-0381/0382* (a dependency-security fix the floor caught, committed separately so this record can report a green floor honestly; it is **not** part of Plan 488's deliverable)
**Predecessor:** none. Plan 488 is the first phase of the network-participation roadmap and has no Plan-488 predecessor.
**Owner boundary:** `crates/i2pr-netdb` (runtime-neutral vocabulary, store, policy) and `crates/i2pr-daemon` (driver, receipts).

---

## 1. Scope

Plan 488 replaces the flat, undifferentiated peer map with a **bounded, tiered
peer profile store** and supplies the **selection inputs** the later
participation phases need.

Delivered:

- a **capacity tier** per peer, derived from the peer's signed `caps` option;
- a **failure model** that distinguishes a peer that was never tried from a
  peer that was tried and did not answer, with bounded counters;
- a **bounded, explainable score** whose ordering rule is a Plan 488
  requirement rather than an implementation detail;
- a **maintenance pass** that demotes an aged failure and then drops it, with
  counted transitions;
- a **bounded, sanitized persisted projection** that is discarded — never
  fatal — when it is corrupt or oversized;
- a **pure, deterministic selection policy** with a seeded shuffle, a /16
  diversity rule, and a bounded per-peer participation cap;
- a **daemon-side driver** that feeds the store, runs the maintenance pass on
  the daemon's clock, and emits sanitized receipts;
- a **private controlled-mesh lane** and its sanitized evidence artifact.

Deliberately **not** delivered, by design:

- no network role. The router still enters the network only through a
  reseed, still carries no other router's traffic, and is still **not a
  participant**;
- no advertisement of any kind. `router.version`, `caps`, the published
  RouterInfo, and the support inventory's advertisement flags are unchanged;
- no wiring of the selection policy into the router's tunnel building. Plan
  488 delivers the policy and its inputs; the wiring is the consumer's change
  and belongs to the participation phases that follow.

---

## 2. Baseline at registration

At registration (`86d7a070`) the peer store was
`crates/i2pr-netdb/src/store.rs`'s `RouterInfoStore`: a **flat** bounded map
(4 096 records / 4 MiB by default) with **no tiers, no failure tracking, no
score, and no selection policy**. `crates/i2pr-netdb/src/routing.rs` had
XOR distance and a daily key but **no bucket set**; the selection primitives
in `lookup_policy.rs` ordered candidates by distance under a candidate
ceiling, a peer budget and an exclusion set, with no notion of a peer's
observed responsiveness.

Consequences recorded at registration, and now discharged:

| Registration gap | Closed by |
|---|---|
| No tiered peer store | `ProfileStore` in `crates/i2pr-netdb/src/profile_store.rs` |
| No failure model | `ProfileState` + bounded counters in `crates/i2pr-netdb/src/profile.rs` |
| No explainable score | `ProfileScore` with named credits and penalties |
| No selection policy | `select_candidates` in `crates/i2pr-netdb/src/selection.rs` |
| No persisted profile state | `ProfileProjection` (bounded, sanitized, digest-protected) |
| No daemon-side ownership or receipts | `crates/i2pr-daemon/src/netdb_participation.rs` |
| No executable runtime-neutrality guard for `i2pr-netdb` | `crates/i2pr-netdb/tests/profile_boundary.rs` |

---

## 3. Implementation commits

| Commit | Subject | Contents |
|---|---|---|
| `c796bb0f` | `feat(network-participation): plan 488 peer profile store, failure model, and selection policy` | `crates/i2pr-netdb/src/profile.rs`, `profile_store.rs`, `selection.rs`, `crates/i2pr-netdb/tests/profile_boundary.rs`, `crates/i2pr-daemon/src/netdb_participation.rs`, `crates/i2pr-daemon/tests/netdb_participation_mesh.rs`, `tests/integration/network-participation/evidence/plan488/result.json`, the `lib.rs` re-export changes, and the documentation updates listed in §10. |
| `256037ec` | `test(network-participation): cover plan 488 store adoption, replacement, and batch learn` | Coverage for `ProfileStore::adopt` (the seam the next phase uses), the replacement path's re-classification, `PeerParticipationDriver::learn_all`, and `PeerParticipationHandle::with`. No behaviour change. |
| `a6bd6154` | `test(network-participation): cover the plan 488 selection capacity trio` | The Plan 488 offline controls name three load points — capacity 1, exact load, max+1 — and two of the three had a direct test. The capacity-1 and max+1 cases are now explicit. No behaviour change. |
| `8f6df1bf` | `fix(deps): update rustls-webpki and webpki-roots for RUSTSEC-2026-0381/0382` | **Not a Plan 488 deliverable.** The floor's `cargo deny check advisories bans sources` step failed on two advisories published after the previous green floor run. The fix is a patch-level bump of two already-locked crates in the daemon's TLS path: `webpki-roots` 1.0.9 → 1.0.10 (workspace manifest; versions stay centralized) and `rustls-webpki` 0.103.15 → 0.103.16 (transitive). No API change, no new dependency, no feature or license change. |

---

## 4. Requirement-to-evidence matrix

Every requirement in the Plan 488 implementation plan, and where it is
satisfied and proved.

| # | Requirement | Implementation | Evidence |
|---|---|---|---|
| R1 | Runtime neutrality: `i2pr-netdb` gains no `tokio::*`, no `std::net`, no `std::fs`, no `async fn` | `crates/i2pr-netdb/src/{profile,profile_store,selection}.rs` are pure | `crates/i2pr-netdb/tests/profile_boundary.rs` — 4 tests: production-source scan, plus the inverted probe (a synthetic violating source **must** be reported) and the negative controls (a comment or a `#[cfg(test)]` block that only *names* a forbidden construct **must not** be reported) |
| R2 | Bounded everything; release on every drop path | `ProfileScore` `[0, 1000]`; `MAX_CONSECUTIVE_FAILURES` 32, `MAX_TOTAL_FAILURES` 1 000, `MAX_ANSWERED_COUNT` 1 000; `DEFAULT_MAX_PROFILES` 4 096; per-tier ceilings; `MAX_SELECTION_COUNT` 64; `MAX_PARTICIPATION_TRACKED_PEERS` 4 096; `DEFAULT_MAX_RECEIPTS` 256; `DEFAULT_MAX_PROJECTION_BYTES` 256 KiB | `profile.rs` unit tests `score_is_bounded_at_both_ends`, `failure_counters_saturate_at_their_ceiling`, `answered_counter_saturates_at_its_ceiling`; `profile_store.rs` `dropping_a_peer_releases_its_byte_accounting`, `the_projection_carries_no_address_or_key_material`; `selection.rs` `the_count_is_clamped_to_the_bounded_maximum`, `the_ledger_is_bounded`; `netdb_participation.rs` `the_receipt_log_is_bounded` |
| R3 | Validated insertion only; no unchecked path | `ProfileStore::insert(ValidatedRouterInfo, u64)` is the only insertion path; the crate exposes no `insert_unchecked` | `profile_store.rs` `invariant_every_profile_has_a_validated_peer_record`, `adoption_classifies_every_retained_peer`, `a_replacement_reclassifies_the_peer_and_keeps_its_observed_state`; `profile_boundary.rs` scans the crate sources for `insert_unchecked` (none) |
| R4 | The store's existing ceilings hold | `ProfileStoreConfig.router_info` carries `RouterInfoStoreConfig` unchanged; `ProfileStore::adopt` keeps the adopted store's own ceilings and clamps the profile ceiling to the record ceiling | `profile_store.rs` `invariant_the_existing_store_ceilings_still_hold` (record ceiling **and** byte ceiling) |
| R5 | No secret in receipts | `ParticipationReceipt` carries counts, tier/state labels and a SHA-256 digest over those counts only | `netdb_participation.rs` `learning_a_peer_records_a_receipt_without_naming_it`; `crates/i2pr-daemon/tests/netdb_participation_mesh.rs` asserts the rendered receipts contain no peer hash, and that the evidence file contains no peer hash |
| R6 | No advertisement | Nothing writes `router.version`, `caps`, a published RouterInfo, or a transport address; the new support entry is `advertised = false` | `profile_store.rs` `invariant_the_profile_store_advertises_nothing`; `specs/support.toml` `netdb.peer-profile-store` (`advertised = false`); `specs/CONFORMANCE.md` §"NetDb peer profile store (Plan 488)" |
| R7 | Determinism: same store contents + same policy input → same selection | `select_candidates` sorts by `(rank desc, key asc)`, shuffles with a caller-supplied seed through the crate's own `splitmix64`, and returns by `(rank desc, key asc)` | `selection.rs` `the_selection_is_deterministic_for_the_same_input`, `a_different_seed_selects_a_different_set_when_the_store_allows_it`; `profile_store.rs` `invariant_the_projection_encoding_is_deterministic`; the floor step `plan488-evidence-deterministic` proves the evidence artifact is byte-identical across runs |
| R8 | Tiering policy with a defined classification | `PeerCapacityClass::from_capabilities` scans the whole `caps` option for a class letter, accepting both the class-first order (`"Nf"`, pinned i2pd 2.61.0) and the class-last order (`"fN"`, this repository's own controlled builders); `ProfileTier::from_capacity_class` maps `N`/`O`/`P`/`X` to `HighCapacity`, `K`/`L`/`M` to `Standard`, and no class to `Untracked` | `profile.rs` `capacity_class_is_parsed_from_either_caps_order`, `capacity_class_does_not_collide_with_flag_letters`, `tier_uses_the_pinned_java_floodfill_floor` |
| R9 | The failure model distinguishes never-tried from did-not-answer, with bounded counters and age thresholds | `ProfileState`; `Profile::observe_*`; `ProfileStore::maintain` | `profile_store.rs` `never_tried_is_distinct_from_did_not_answer`, `maintenance_demotes_then_drops_an_aged_failure`, `maintenance_is_idempotent`, `an_untried_peer_ages_out_without_ever_being_contacted` |
| R10 | The score is bounded and explainable, and a peer that answered without the data outranks a peer that did not answer | `ProfileScore` with named credits/penalties and `SCORE_BASE` | `profile.rs` `answered_empty_outranks_did_not_answer`, `answered_outranks_answered_empty_and_refused`, `misbehaving_is_penalised_harder_than_unreachable`, `score_is_bounded_at_both_ends`; `profile_store.rs` `answering_outranks_not_answering` |
| R11 | Selection is pure, bounded, diverse, and honours the participation cap | `select_candidates` + `ParticipationLedger` | `selection.rs` `a_capacity_of_one_selects_the_single_peer`, `an_exact_load_returns_the_requested_count`, `a_max_plus_one_request_returns_what_the_store_holds`, `a_request_beyond_the_store_returns_what_exists`, `the_diversity_rule_keeps_one_peer_per_prefix`, `the_participation_cap_excludes_a_capped_peer`, `a_responsive_peer_outranks_a_silent_one`, `a_minimum_rank_filters_a_silent_peer_out`, `the_floodfill_requirement_filters_the_selection`, `an_excluded_peer_is_never_selected`, `the_minimum_tier_filters_the_selection` |
| R12 | The daemon driver feeds the store, runs the maintenance pass on the daemon's clock, and emits receipts | `PeerParticipationDriver` (with `learn` and `learn_all`), `PeerParticipationHandle`, `run_profile_maintenance` | `netdb_participation.rs` `the_maintenance_pass_runs_only_when_due`, `the_periodic_owner_stops_on_cancellation`, `the_periodic_owner_refuses_an_out_of_bound_period`, `the_receipt_log_is_bounded`, `a_batch_learn_emits_one_receipt_and_summarises_the_outcomes`, `the_driver_adopts_an_existing_store`, `the_shared_handle_applies_a_closure_to_the_driver` |
| R13 | The persisted projection is bounded and sanitized; a corrupt or oversized one is discarded with a counted receipt, never a startup failure | `ProfileProjection::encode`/`decode` with a SHA-256 body digest; `PeerParticipationDriver::load_projection` | `profile_store.rs` `the_projection_round_trips`, `a_corrupt_projection_is_refused_with_a_typed_error`, `an_oversized_projection_is_refused`; `netdb_participation.rs` `a_corrupt_projection_is_discarded_and_counted`, `an_oversized_projection_is_discarded_and_counted`, `a_projection_round_trips_through_the_driver` |
| R14 | The private-mesh lane: one peer answers, one stops answering, one answers with the wrong data; the observed state is visible in source-originated receipts; the selection demonstrably prefers the responsive peer | `crates/i2pr-daemon/tests/netdb_participation_mesh.rs` | the lane itself, plus the committed artifact `tests/integration/network-participation/evidence/plan488/result.json` (counts and digests only) |
| R15 | Every invariant has an inverted probe that fails when the invariant is broken | the probes named in R1–R7 above | `crates/i2pr-netdb/tests/profile_boundary.rs` carries the scanner's own inverted probe; the remaining probes are negative assertions that fail if the corresponding guard is removed |

---

## 5. Work packages and outcomes

| WP | Outcome | Status |
|---|---|---|
| WP1 — profile vocabulary | `profile.rs`: `ProfileTier` (hand-written `Ord`, stronger-is-greater), `ProfileState` (7 states), `ProfileScore` (bounded `[0, 1000]`, `SCORE_BASE` 100), `PeerCapacityClass`, `ProfileError`, `network_prefix16`. 18 unit tests. | done |
| WP2 — tiering | The tier is derived from the peer's signed `caps`; `Profile::promote` refuses a promotion without capacity evidence (`ProfileError::TierPromotionWithoutEvidence`); demotion is one tier per step. | done |
| WP3 — failure model | `NeverTried` / `Answered` / `AnsweredEmpty` / `Refused` / `Misbehaving` / `Unanswered` / `Unreachable`, with bounded counters and an explicit `is_responsive` / `is_aged_failure` split. | done |
| WP4 — score | Named credits and penalties; the Plan 488 ordering rule is asserted directly rather than implied. | done |
| WP5 — selection policy | Pure, deterministic, seeded shuffle, /16 diversity, bounded participation ledger. 14 unit tests. | done |
| WP6 — maintenance and persistence | Demote-then-drop maintenance with counted transitions; bounded, sanitized, digest-protected projection; restore reapplies the observed state and counts orphaned entries. | done |
| WP7 — private-mesh lane | Three-peer in-process mesh; evidence artifact committed and proven byte-identical across runs by the floor. | done |

**Design decisions recorded during implementation** (each is a decision the
plan left to the implementer):

1. **The capacity tier is derived from the signed `caps` option, not from a
   local measurement.** Bandwidth metering is unresolved in this router and in
   both reference routers; an honest "unknown class" (`Untracked`) is the
   correct outcome, not a guessed class. The tier is self-asserted data and
   is treated as a hint, never as trust.
2. **The class letter is found by scanning the whole `caps` option**, not by
   position. The pinned i2pd 2.61.0 emits the class first (`"Nf"`); this
   repository's own controlled builders append it last (`"fN"`). The class
   letters do not collide with the flag letters either implementation uses.
3. **The score starts from `SCORE_BASE` (100), not from zero.** With a zero
   base every penalty clamps to zero, a misbehaving peer and an unreachable
   peer both score zero, and the ranking cannot express "worse than
   unreachable". This was caught by the inverted probe
   `misbehaving_is_penalised_harder_than_unreachable` during implementation
   and corrected before the tree was closed.
4. **`ProfileTier`'s `Ord` is hand-written** so that a *stronger* tier
   compares greater. The derived order would have made `Untracked` the
   greatest tier, silently inverting the `min_tier` filter. Caught by
   `the_minimum_tier_filters_the_selection` and corrected.
5. **The projection digest covers the body verbatim.** The first
   implementation hashed a re-joined line list, which dropped the trailing
   newline and made every round trip fail. Caught by
   `the_projection_round_trips` and corrected.
6. **The daemon driver keeps an explicit clock** (`advance_time` /
   `tick(now_ms)`), matching the daemon's existing convention
   (`NetDbTunnelCoordinator::advance_time`, `Bootstrap::run(..., now_seconds)`).
   The driver never reads a wall clock itself, so every observation,
   maintenance pass and selection is reproducible. The wall clock is read
   only by the periodic owner, which is the daemon's one timer-bearing
   component.
7. **The selection policy is not wired into tunnel building.** Wiring it
   without its own evidence would change existing behaviour on a
   self-asserted signal.
8. **A bulk import learns through `learn_all`, not `learn`.** One receipt per
   learned peer would churn the bounded receipt log with an aggregate
   receipt per peer; `learn_all` inserts the batch and emits one receipt
   with a per-outcome summary.

---

## 6. Invariant review

Each Plan 488 invariant, and how it was discharged.

1. **Runtime neutrality.** The three new `i2pr-netdb` modules are pure:
   no `tokio::*`, no `std::net`, no `std::fs`, no `async fn`, no task
   ownership. `crates/i2pr-netdb/tests/profile_boundary.rs` scans the crate's
   production sources for all of them plus `insert_unchecked`, and carries
   its own inverted probe.
   *Gap recorded:* `scripts/check-runtime-boundaries.sh` still has **no
   `i2pr-netdb` section**, so the routine floor does not enforce this
   invariant independently. The crate-local guard covers the crate, but a
   repo-wide script section would be the durable fix. That needs its own
   plan-of-record (see §12, F1).
2. **Bounded everything.** The score, every counter, every age threshold,
   every retention ceiling (profiles, per-tier profiles, receipts, the
   participation ledger, the selection count, the projection size) is a named
   constant or a configured ceiling. The maintenance pass releases the peer
   record **and** its byte accounting together, on every drop path.
3. **Validated insertion only.** The only insertion path consumes a
   `ValidatedRouterInfo`; the crate exposes no unchecked bypass; a profile
   exists only for a peer the store holds.
4. **The store's existing ceilings hold.** The Plan 103 record ceiling
   (4 096) and byte ceiling (4 MiB) keep their defaults and semantics. The
   tiering is layered on top of them, never instead of them.
5. **No secret in receipts.** Receipts carry counts, tier and state labels,
   and a digest over those counts. The mesh lane asserts that a receipt
   rendered in full carries no peer hash, and that the committed evidence
   artifact carries no peer hash.
6. **No advertisement.** No `router.version`, `caps`, published RouterInfo,
   transport address, or advertisement flag changed. The one new support
   entry is `status = "experimental"`, `advertised = false`.
7. **Determinism.** The selection is a pure function of the store contents
   and the constraints; the projection encoding is byte-stable; the evidence
   artifact is byte-identical across runs (proved by the floor step
   `plan488-evidence-deterministic`).

---

## 7. Verification

All commands were run **locally** on the closing tree (`c796bb0f`) on
2026-10-20, from the repository root. No CI run is claimed.

### 7.1 Focused runs

| Command | Result |
|---|---|
| `cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1` | **349 passed, 0 failed, 0 ignored** (241 in-crate unit tests + 108 integration tests across 7 integration files) |
| `cargo test --locked -p i2pr-daemon --test netdb_participation_mesh -- --test-threads=1` | **1 passed, 0 failed** — the private controlled-mesh lane; writes the evidence artifact |
| `cargo test --locked -p i2pr-daemon --lib netdb_participation -- --test-threads=1` | **12 passed, 0 failed** — the driver's unit tests |

### 7.2 The AGENTS.md routine floor

The full routine floor was executed on the closing tree. Every step is
recorded with its outcome; the transcript is `/tmp/i2pr-floor.log` on the
authoring host.

Every floor line was executed locally on the closing tree: **70 steps,
70 PASS, 0 FAIL**. **CI was not run this turn** (no CI trigger is
available in this environment) — that is labelled truthfully, and no claim in
this record depends on it.

| Floor step | Outcome |
| --- | --- |
| `cargo fmt --all --check` | PASS — silent on a clean tree |
| `cargo check --locked --workspace --all-targets` | PASS (3s) |
| `cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl` | PASS (1s) |
| `cargo rustc --locked -p i2pr-app-fixture --bin i2pr-app-fixture -- -C target-feature=+crt-static` | PASS (0s) |
| `cp target/debug/i2pr-app-fixture target/debug/i2pr-app-fixture-secured` | PASS (1s) |
| `cargo build --locked -p i2pr-app-fixture --bin i2pr-app-fixture` | PASS (0s) |
| `cargo test --locked --workspace --all-targets -- --test-threads=1` | PASS (1115s) |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | PASS (0s) |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps` | PASS (1s) |
| `cargo test --locked --workspace --doc` | PASS (3s) |
| `bash scripts/check-dependency-direction.sh` | PASS (0s) |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS (0s) |
| `python3 scripts/check-adr-number-uniqueness.py` | PASS (0s) |
| `python3 scripts/check-portable-service-tunnel-api.py` | PASS (0s) |
| `bash scripts/check-portable-service-tunnel-consumer.sh` | PASS (7s) |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS (7s) |
| `bash scripts/check-runtime-boundaries.sh` | PASS (0s) |
| `bash scripts/check-console-boundaries.sh` | PASS (0s) |
| `bash scripts/check-console-browser-security.sh` | PASS (0s) |
| `python3 scripts/check-tooling-inventory.py` | PASS (1s) |
| `python3 scripts/check-license-metadata.py` | PASS (0s) |
| `python3 scripts/check-managed-app-private-client-seams.py` | PASS (0s) |
| `python3 scripts/check-router-readiness-contract.py` | PASS (0s) |
| `python3 scripts/check-router-readiness-contract.py --self-test` | PASS (0s) |
| `python3 scripts/check-core-router-recovery-contract.py` | PASS (0s) |
| `python3 scripts/check-core-router-recovery-contract.py --self-test` | PASS (0s) |
| `python3 scripts/plan449-ssu2-preflight.py --self-test` | PASS (0s) |
| `python3 scripts/check-managed-app-package-boundary.py` | PASS (0s) |
| `python3 scripts/check-managed-app-package-boundary.py --self-test` | PASS (0s) |
| `python3 scripts/check-managed-app-policy-boundary.py` | PASS (0s) |
| `python3 scripts/check-managed-app-policy-boundary.py --self-test` | PASS (0s) |
| `python3 scripts/check-managed-app-secured-sandbox.py` | PASS (0s) |
| `python3 scripts/check-managed-app-secured-sandbox.py --self-test` | PASS (0s) |
| `bash scripts/check-service-tunnel-boundaries.sh` | PASS (0s) |
| `python3 scripts/check-managed-app-gateway-boundary.py` | PASS (0s) |
| `python3 scripts/check-managed-app-manager-boundary.py` | PASS (0s) |
| `bash scripts/check-m11-per-epoch-composition.sh` | PASS (1s) |
| `bash scripts/check-service-anonymity-boundaries.sh` | PASS (0s) |
| `bash scripts/check-fixture-manifest.sh` | PASS — silent by design; it prints only on a mismatch |
| `bash scripts/check-ntcp2-vectors.sh` | PASS (0s) |
| `bash scripts/check-ssu2-vectors.sh` | PASS (0s) |
| `bash scripts/check-i2cp-vectors.sh` | PASS (0s) |
| `bash scripts/check-ntcp2-interoperability.sh` | PASS (0s) |
| `bash scripts/check-constrained-host-lane-boundary.sh` | PASS (0s) |
| `bash scripts/check-m11-transit-boundaries.sh` | PASS (1s) |
| `bash scripts/check-m11-transit-qualification-evidence.sh` | PASS (1s) |
| `bash scripts/check-sam-acceptance-evidence.sh` | PASS (0s) |
| `bash scripts/check-ssu2-acceptance-evidence.sh` | PASS (0s) |
| `bash scripts/check-i2cp-acceptance-evidence.sh` | PASS (0s) |
| `bash scripts/check-i2pcontrol-acceptance-evidence.sh` | PASS (0s) |
| `bash scripts/check-els2-type11-transcript-boundary.sh` | PASS (0s) |
| `bash scripts/check-els2-live-lane-evidence.sh` | PASS (2s) |
| `bash scripts/check-encrypted-service-consumer-caller.sh` | PASS (0s) |
| `bash scripts/check-outproxy-request-path.sh` | PASS (0s) |
| `bash scripts/check-outproxy-wire-lane-evidence.sh` | PASS (0s) |
| `bash scripts/check-config-secret-hygiene.sh` | PASS (0s) |
| `python3 scripts/check-workflow-validity.py` | PASS (0s) |
| `bash scripts/check-floodfill-type5-serve.sh` | PASS (0s) |
| `bash scripts/check-service-tunnel-acceptance-evidence.sh` | PASS (1s) |
| `bash scripts/check-exploratory-tunnel-evidence.sh` | PASS (0s) |
| `bash scripts/check-netdb-tunnel-evidence.sh` | PASS (0s) |
| `bash scripts/check-destination-tunnel-evidence.sh` | PASS (0s) |
| `bash scripts/check-streaming-tunnel-evidence.sh` | PASS (5s) |
| `bash scripts/check-m6-mixed-router-acceptance-evidence.sh` | PASS (3s) |
| `bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test` | PASS (0s) |
| `bash scripts/check-m12-floodfill-boundaries.sh --self-test` | PASS (1s) |
| `python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_execution_lane.py'` | PASS (0s) |
| `cargo test --locked -p i2pr-daemon --test netdb_participation_mesh -- --test-threads=1` | PASS (0s) |
| `git diff --exit-code -- tests/integration/network-participation/evidence/plan488/result.json` | PASS — the lane's evidence artifact is byte-identical across runs |
| `cargo deny check advisories bans sources` | PASS (`advisories ok, bans ok, sources ok`) |

**The floor includes the AGENTS.md-documented Linux fixture procedure**
(`cargo-build-static-fixture` → `cargo-copy-static-fixture` →
`cargo-rebuild-plain-fixture`). The daemon's in-crate qualification test
resolves `target/debug/i2pr-app-fixture-secured` as a sibling binary and
fails closed when it is absent, so both fixture variants must exist before
the workspace test runs. On this host the workspace test caught that the
secured variant was missing; the three steps above produce it.

**Two floor steps are specific to Plan 488 and are additions to the
AGENTS.md floor, not replacements.** `plan488-mesh-lane` runs the private
controlled-mesh lane on its own, and `plan488-evidence-deterministic` asserts
that the lane's committed evidence artifact is **byte-identical** after a
fresh run — the determinism claim is therefore executed, not merely asserted.

**The documentation step was executed in the `VAR=value command` shell form,
not in the `env VAR=value command` form.** On this host `env` resolves to
`~/.local/bin/env`, a PATH shim that **ignores its arguments and never execs
the command**, so the `env` form exits 0 having built nothing — a silent
false pass. That was found during the Plan 487 close-out and is recorded
there as a host-environment finding. The floor script used for this closure
uses the correct form, and the documentation step's output was audited for
emptiness. The floor's other two output-free steps are silent **by design** —
`cargo fmt --all --check` prints nothing on a clean tree, and
`check-fixture-manifest.sh` prints only on a mismatch — and both were
confirmed independently.


### 7.3 The advisories the floor caught

The floor's `cargo deny check advisories bans sources` step **failed** on the
first closing-tree run with two advisories that had been published after the
previous green floor run:

- **RUSTSEC-2026-0382** — `rustls-webpki` 0.103.15 may accept certificate
  paths containing distrusted emSign intermediate public keys whose private
  keys are now public; fixed in 0.103.16.
- **RUSTSEC-2026-0381** — `webpki-roots` 1.0.9 still bundles the emSign ECC
  Root CA - G3; fixed in 1.0.10.

Both crates sit in the daemon's TLS path (the Proposal 170 I2PControl TLS
listener and the outbound HTTPS control sources). **Neither was introduced by
Plan 488** — Plan 488 added no dependency and touched no TLS code — but a
milestone must not close over a known security advisory in the locked graph,
so the fix was applied and committed separately (`8f6df1bf`) before this
closure. The fix is a patch-level bump of two crates that were already
locked; the workspace version requirement stays centralized, no new
dependency was added, and no feature or license changed. The floor was then
re-run in full on the corrected tree, and the table in §7.2 is that run.

### 7.4 Two host-environment failures, and what they cost

Neither of these is a repository defect and neither is a Plan 488 defect.
Both are recorded because a closure that hides them would be dishonest about
how the evidence was produced.

**7.4.1 The filesystem filled during the build (Low).** The first closing-tree
floor attempt failed in three steps — the workspace test's link, clippy, and
the documentation build — all with `No space left on device (os error 28)`
and `database or disk is full`. The repository's build directory had
accumulated **69.7 GiB** of stale artifacts across many floor runs with
different lockfiles, and the host's filesystem ran out of room while the
workspace test was linking. `cargo clean` released 78 157 files (69.7 GiB),
and the floor was re-run from a clean tree with a disk sampler watching the
build: the tree reached ~32 GiB of artifacts with 266 GiB still free. No
source change was involved, and the failure was in the build, not in the
code.

**7.4.2 The secured fixture variant was missing (Low).** After the clean,
the workspace test failed with
`i2pr-app-fixture-secured was not found at target/debug/…`. The daemon's
in-crate qualification test resolves that static sibling binary and fails
closed when it is absent — the intended behaviour. The AGENTS.md floor's
sibling-build line produces only the *ordinary* fixture, so the floor script
now also performs the AGENTS.md-documented Linux procedure: build the static
fixture, copy it to `target/debug/i2pr-app-fixture-secured`, then rebuild the
ordinary fixture binary. The three steps are in the floor table below.

### 7.5 The false-pass audit

The floor's documentation step was executed in the `VAR=value command` shell
form, **not** in the `env VAR=value command` form. On the authoring host
`~/.local/bin/env` is a PATH shim that ignores its arguments and never
execs the command, so the `env` form exits 0 having built nothing — a
silent false pass. This was found and corrected during the Plan 487
close-out and is recorded there as a host-environment finding.

The floor wrapper for this closure runs the step as
`bash -c 'RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps'`,
which is the form `AGENTS.md` itself uses, and the documentation step's
output was audited for emptiness. (An intermediate wrapper attempt that
passed `RUSTDOCFLAGS="-D warnings"` as a bare word to the wrapper's `"$@"`
failed with `command not found`; that is a wrapper defect, it was caught by
the floor's own exit status, and it is the reason the step is now run through
`bash -c`.)

---

## 8. Migration and compatibility

- **No migration.** The peer profile store is new state. The Plan 103
  `RouterInfoStore` is unchanged, and its record and byte ceilings are
  unchanged.
- **No compatibility surface.** Nothing on the wire changed. No I2NP
  message, no RouterInfo option, no transport address, and no SAM/I2CP/
  I2PControl behaviour changed. The daemon's existing tunnel building is
  untouched: the selection policy is not wired in.
- **Persisted state.** The projection is a **new** format with its own
  magic line (`i2pr-peer-profile/1`). It is not read by any existing
  component and no existing file is migrated or rewritten. A projection that
  is corrupt, truncated, or oversized is discarded with a counted receipt;
  the router continues with a freshly classified store. There is no failure
  mode in which a bad projection stops the router from starting.
- **Configuration.** No new configuration key was added. The store's
  ceilings are code-level constants with the Plan 103 defaults; a future
  phase that needs operator control can add it under its own plan.
- **Dependency change (not a Plan 488 change).** The floor's advisory step
  forced a patch-level bump of two already-locked crates in the daemon's TLS
  path (`webpki-roots` 1.0.9 → 1.0.10, `rustls-webpki` 0.103.15 → 0.103.16;
  commit `8f6df1bf`). The bump removes one distrusted root certificate and
  makes the verifier reject the corresponding intermediate paths. No API
  changed, no dependency was added, and no feature or license changed. The
  workspace version requirement stays centralized: `webpki-roots` moved in
  the workspace manifest, `rustls-webpki` is transitive.

---

## 9. Security and contention

- **Self-asserted data is treated as self-asserted.** The capacity tier is
  read out of a peer's signed `caps` option. It influences *ordering*, never
  *admission* and never *trust*: a peer that lies about its class is
  mis-ordered, not admitted and not trusted. The tier is not used as an
  authorization or rate-limiting input anywhere in this plan.
- **A peer's `Untracked` state is not a penalty.** An unknown class is not
  a known-bad class, so an untracked peer is never demoted below a declared
  peer *on the strength of being untracked*; the bounded tier bonus only
  affects the ordering, and the responsive score dominates it.
- **Misbehaviour is penalised harder than unreachability** (`−300` against
  `−75`): a peer that replies with data that does not match the requested key
  is worse than a peer that is simply slow.
- **A misbehaving peer is never silently forgotten.** It is demoted, then
  dropped, and both transitions are counted.
- **No secret crosses a receipt.** A receipt carries counts, labels, and a
  digest. The digest is a SHA-256 over the counts, so it binds the receipt
  to its content without disclosing anything about a peer.
- **No key material is persisted.** The projection carries the RouterHash
  (already a digest), the tier, the state, bounded counters, and timestamps.
  A test asserts the fixture's public key bytes do not appear in it.
- **Contention.** The driver's shared handle holds a `std::sync::Mutex` only
  for the duration of a short, non-awaiting critical section; the closure
  passed to `PeerParticipationHandle::with` must not await, and the periodic
  owner drops the guard before it suspends. The store itself is
  single-owner and single-threaded: `ProfileStore` is not `Sync`-shared
  state, and no lock is held across an `.await`. The selection policy is a
  pure function and holds no lock at all.
- **The crate keeps `#![forbid(unsafe_code)]`**; the new modules introduce
  no `unsafe`.

---

## 10. Documentation and operational changes

| File | Change |
|---|---|
| `docs/architecture/i2pr-netdb.md` | New module-layout rows for `profile`, `profile_store`, `selection` (and the previously missing `els2_transcript` row, so the module count is accurate); the new public-surface re-export blocks; a new key-contract section "Peer profile store, tiers, score, and the selection policy"; the new bound constants in the `MAX_*` table; the test inventory updated to 345 tests across 7 integration files. |
| `docs/architecture/i2pr-daemon.md` | New module-layout row for `src/netdb_participation.rs`; the `pub mod` list; a new key-contract section "Peer participation driver" that states the explicit-clock convention, the owned periodic future, the receipt contract, and that the selection policy is **not** wired into tunnel building. |
| `specs/CONFORMANCE.md` | New section "NetDb peer profile store (Plan 488)" recording what is and is not claimed, the internal/non-advertised status, the failure-model and score rules, the receipt and evidence rules, the projection-discard rule, and the explicit statement that the selection policy is not wired into tunnel building. |
| `specs/support.toml` | `plan_488_plan_of_record`, `plan_488_closure`, `plan_488_status = "passed-peer-profile-store"`; one new surface entry `netdb.peer-profile-store` with `status = "experimental"`, `advertised = false`, a `notes` field stating plainly that Plan 488 adds no participation capability, and the evidence list. The inventory moves from 89 to 90 surface entries; **no existing entry was edited and no advertisement flag changed.** |

**No operational change.** No new configuration key, no new listener, no new
port, no new log line, no new startup task, and no change to what the router
advertises.

---

## 11. Known limitations

1. **The tier is self-asserted.** A peer that advertises a class it does not
   have is mis-ordered. The plan accepts this: bandwidth metering is
   unresolved in this router and in both reference routers, and an honest
   `Untracked` is preferred to a guessed class.
2. **There is no traffic-based score.** The score reflects *reachability
   and responsiveness* only. It does not reflect throughput, latency, or
   usefulness, and it must not be described as a bandwidth measure.
3. **The selection policy has no consumer.** Until a later phase wires it,
   the router's tunnel building uses the pre-existing distance-ordered path.
   The policy is exercised by its own lane, not by live traffic.
4. **The /16 diversity rule is coarse.** The prefix is the leading two bytes
   of the RouterHash. It is a documented definition, not a claim about the
   real network topology of a peer.
5. **The participation ledger is per-driver.** It is not shared across
   drivers, and it does not persist across a restart; the persisted
   projection carries the profile state, not the participation counters.
6. **The projection is not yet persisted by the daemon.** The daemon driver
   can encode and load the projection, and the round trip is proved, but no
   startup path writes it to disk yet. Writing it is a later phase's change.
7. **The runtime-neutrality guard is crate-local.** The floor's
   `check-runtime-boundaries.sh` still has no `i2pr-netdb` section.

---

## 12. Findings by severity

### Medium — found and fixed in this closure

- **F6 — two rustls advisories in the locked dependency graph.** The floor's
  `cargo deny check advisories bans sources` step failed on
  RUSTSEC-2026-0382 (`rustls-webpki` 0.103.15) and RUSTSEC-2026-0381
  (`webpki-roots` 1.0.9), both in the daemon's TLS path. **Neither was
  introduced by Plan 488** — the plan added no dependency and touched no TLS
  code — but a milestone must not close over a known advisory in the locked
  graph. **Fixed** in `8f6df1bf` by a patch-level bump of the two
  already-locked crates, and the floor was re-run in full afterwards. See
  §7.3.

### Medium — open

- **F1 — `scripts/check-runtime-boundaries.sh` has no `i2pr-netdb` section.**
  The Plan 488 invariant "runtime neutrality" is therefore not enforced by
  the routine floor. Plan 488 closes the gap for its own crate with
  `crates/i2pr-netdb/tests/profile_boundary.rs`, which is a real executable
  guard with its own inverted probe — but the durable fix is a script
  section, and the AGENTS.md rule stands: *fix the boundary, never weaken a
  script*. Adding a section changes a published CI surface, so it needs its
  own plan-of-record. **Not fixed in this plan.** Note that the AGENTS.md
  known-gaps list already records the same class of gap for the transport
  crates, so this is the same family of coverage hole and should be closed
  together with a transport-scoped pass.

### Low

- **F2 — The capacity tier is self-asserted and unmeasured.** Recorded as a
  limitation (§11.1) rather than a defect: an honest `Untracked` is the
  intended behaviour, and no claim of measurement is made anywhere.
- **F3 — The daemon's periodic maintenance owner has no consumer yet.** The
  driver exists and is correct, but no service-graph wiring starts
  `run_profile_maintenance` in production. This is deliberate: wiring it
  belongs to the consumer phase. The consequence is that no profile state is
  maintained in a live router yet, which is exactly why the plan claims no
  capability.
- **F4 — The projection is not written to disk.** See §11.6. Same reasoning:
  the format and the round trip are proved; the persistence wiring is a
  consumer's change.

### Informational

- **F5 — The crate-local boundary guard duplicates a rule that belongs in the
  shared script.** This is a deliberate duplication for this plan: the plan
  needed an executable inverted probe *now*, and adding a script section
  would have changed a published CI surface inside a feature plan. The
  duplication is expected to be removed when F1 is closed.

### No findings

- No high-severity finding was recorded. The one Medium security finding
  (F6) was **fixed** before this closure, not deferred.
- **Two host-environment failures** (§7.4) cost one floor re-run each. Both
  are recorded; neither is a repository defect, and neither changed a
  source file.
- No secret, key material, address, or payload reaches a receipt, a log
  line, or the evidence artifact.
- No advertisement was made or implied.
- No existing behaviour changed: the Plan 103 store, the lookup engine, the
  daemon's tunnel building, and every wire format are untouched.

---

## 13. Roadmap disposition

| Roadmap item | Disposition |
|---|---|
| Phase 1 — peer profile store | **Closed.** `passed-peer-profile-store`. |
| Phase 2 — floodfill-indexed routing table and exploration (Plan 489) | **Unblocked.** Plan 489's hard dependency was Plan 488's stable profile store and its selection policy; both now exist with a written contract (this record plus the public API in `i2pr-netdb`). Plan 489 moves to `ready`. |
| Phase 3 — SSU2 introducer relay role (Plan 490) | Unchanged (`ready`); no dependency on Plan 488. |
| Phase 4 — API-version floors (Plan 491) | Unchanged (`ready`); no dependency on Plan 488. |
| Phase 5 — transit participation posture (Plan 492) | Unchanged (`ready`); no dependency on Plan 488. |
| Phase 6 — two-family qualification (Plan 493) | Unchanged (`blocked`); blocked on the authorized non-loopback two-host topology and Plan 435. |
| Phase 7 — floodfill role and `caps=f` (Plan 494) | Unchanged (`blocked`); blocked on Plan 493's two-family evidence. `caps=f` stays refused. |
| The roadmap's "substrate first" ordering | **Upheld.** The tiering, the failure model, the score, and the selection inputs all landed before any role, before any advertisement, and before any wire change. |

---

## 14. Unblock audit

The registry's blocked work and the roadmap's dependency graph were audited
against this closure.

| Registered plan | Lists Plan 488 as a dependency? | Other hard dependencies | Interface dependencies stable? | Outcome |
|---|---|---|---|---|
| 489 floodfill-indexed routing table and exploration | **Yes** — hard | None | Yes: the profile store's public API (`ProfileStore`, `Profile`, `ProfileTier`, `ProfileState`, `ProfileScore`) and the selection policy (`select_candidates`, `SelectionConstraints`) are written down in this record and in the crate's own documentation | **Moved to `ready`** |
| 490 SSU2 introducer relay role | No | None | — | Unchanged (`ready`) |
| 491 API-version floors | No | None | — | Unchanged (`ready`) |
| 492 transit participation posture | No | None | — | Unchanged (`ready`) |
| 493 two-family qualification | No | Authorized non-loopback two-host topology (Plan 431 stopped at this boundary); Plan 435's transport activation | — | Unchanged (`blocked`) |
| 494 floodfill role and `caps=f` | No | Plan 493's two-family evidence | — | Unchanged (`blocked`) |

**Audit result.** Exactly one plan listed Plan 488 as a dependency, and all
of its other hard dependencies are closed and its interface dependencies have
stable written contracts. Plan 489 is therefore moved to `ready` in the same
commit as this record. No plan was silently unblocked, and no corrective
plan is required: the three findings that need a plan-of-record (F1, and the
later wiring in F3/F4) are recorded in §12 and are **not** prerequisites of
any registered plan.

---

## 15. Open items

| Item | Owner | Status |
|---|---|---|
| F1 — add an `i2pr-netdb` section to `scripts/check-runtime-boundaries.sh` | next boundary-script plan-of-record | open, recorded |
| F3 — wire `run_profile_maintenance` into the daemon's service graph | the first consumer phase (Plan 489 or later) | open, recorded |
| F4 — persist the profile projection at startup | the first consumer phase | open, recorded |
| F6 — the two rustls advisories | closed in this closure | **fixed** (`8f6df1bf`), floor re-run green |
| Plan 489 — floodfill-indexed routing table and exploration | network participation | **ready** |

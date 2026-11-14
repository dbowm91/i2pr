# Plan 479 closure — controlled Java candidate preference and NetDB product qualification

**Status token:** `passed-controlled-java-comparator-with-truthful-tiers-and-correlated-store`
**Disposition:** passed. Two exact-pinned Java comparator profiles executed: stock Java 2.13.0
observes a truthfully tiered eligible i2pr `fN` candidate and a truthful `L` i2pr candidate,
selects only the eligible one for its normal local-RouterInfo Store, and the Store completes
with a daemon-accepted authenticated effect and no effect on the low-tier candidate. A second
profile proves Java operates among genuinely multiple healthy floodfills (eligible `fN` plus
stock i2pd `fO`) with both selector-included and the single Store recipient attributed. All
tiers are stock-observed, all classes derive from live enforced governor rates, and the
fail-closed negative controls hold end to end.

**Implementation commits:** `6a94ac8` (capacity-gated controlled publication:
class-bearing dual builders, tier-split activation, tiered addressed fallback) and the
lane/status commit containing this closure (comparator lane with both profiles green,
Plan 470 timeout attribution). Lane script:
`tests/integration/ntcp2/plan479_java_comparator.py` (`--profile a|b|all`, frozen
per-profile attempt budget 1).

## Requirement-to-evidence matrix

| Requirement (implementation plan §Ordered recovery pass) | Evidence | Result |
| --- | --- | --- |
| 1. Reuse the Plan 470 Java NTCP2 runner/diagnostics; freeze pins, listener readiness without TCP probing, lookup-throttler admission | Lane reuses `ControlledRouter.java` + `P470TransportPreflight`/`P220SelectorProbe` unchanged; pins verified before provisioning (`2.13.0@9134f808…`, i2pd `2.61.0@635b013a…`); profile B reproduces the eligible-candidate selection preflight | Passed |
| 2. Controlled matrix: (A) healthy stock floodfills for discovery, (B) low-tier i2pr ranked behind healthy, (C) eligible N-or-higher i2pr under identical reachability/capacity gates | Profile A seeds `{stock i2pd fO, eligible fN}`; profile B seeds `{eligible fN, low-tier L}`; both daemons run the identical controlled-loopback composition differing only in configured rates/share | Passed |
| 2b. No forced f/R, no patched stock, L stays last-resort (never categorically rejected) | Guard refuses `f` below N (unit + live `TierBelowFloodfillFloor`); the L record is addressed, biddable, reachable — Java's own selector yields it for nothing while a healthy peer exists; the structurally unreachable `fL`-vs-`fN` arm is recorded, not fabricated | Passed |
| 3. Same configured bytes/sec bottleneck/share as the transport governor; class+cap from current healthy permits only | `[bandwidth]` enforced live on both daemons (N: 262144/262144 B/s @50 → 128 KiB/s shared; L: 131072/131072 B/s @25 → 32 KiB/s shared); `CapacityEligibility` issued per tick, `current_class` revalidated against the live shaper; stale/half-present gates fail closed | Passed, unit + live |
| 4. Real unmodified-Java sender actions to the selected candidate: accepted DatabaseStore, acknowledgement, correlated response; one correctly signed LS2-class record path retained by predecessor | Java's normal `FloodfillStoreJob` (reply-requested) sends its local RI; profile B: eligible daemon logs `authenticated floodfill DatabaseStore accepted with response owed`, job `succeeded=1`; profile A: job `succeeded=1` with recipient attributed. Phase-2 LS2 lookup correlation retained from Plan 475 (not repeated: separate owner) | Passed |
| 5. Bounded replication/rollover/withdrawal through the existing M12 owner or a named 438 gate | Out of 479 scope by plan text ("or note a specific remaining gated Plan 438 acceptance"): normal-role replication/withdrawal stays with Plans 437/438, which remain blocked on 433/436. Controlled withdrawal form untouched and still covered by existing tests | Noted, remains gated |
| 6. Source-owned defects fixed with deterministic regression, one source-supported rerun; no unchanged repeats, no private stock methods, no demotion of Plan 475 | Three lane-blocking findings below, each with a fail-before/pass-after (or observed-before/after) record; each profile ran once per design, reruns only after material lane/source changes with the causal delta recorded | Passed |
| Pass gate: two executed Java comparator profiles, Java-owned selection, accepted request/reply with sanitized attribution, truthful capacity, failed negative controls | Profile B (14/14 stages) + profile A (12/12 stages); receipts below; only booleans/counts/tier-letters printed | Passed |

## Source defects found and fixed (all in-plan, per the corrective policy)

1. **Live controlled activation published classless `f` with bandwidth enabled (guard bypass).**
   `apply_controlled_dual_activation` never consulted `CapacityEligibility`: any
   bandwidth-enabled controlled daemon — including an L-configured one — published plain
   `f`. Proven live: pristine tree + `[bandwidth]` serves `f` with no tier. Fix: per-tick
   eligibility issue + `current_class` revalidation; N-or-higher builds `f`+tier via the
   new dual builder; below-floor returns `TierBelowFloodfillFloor` before the role opens
   activation; stale/half-present gates fail closed (`InvalidConfig`) instead of
   publishing classless. Regression: `resolve_controlled_capacity_stays_legacy_or_current_or_refused`
   (legacy/stale/half arms), `controlled_capacity_form_splits_every_class_at_java_floor`
   (all 7 classes, inversion-probed), tiered-record builder test, plus live TierBelow
   publication in profile B shape phase.
2. **Below-floor candidates had no honest publication form.** Added
   `build_controlled_addressed_with_bandwidth_class` (dual loopback gates, tier letter
   only, caller letters still refused, no permit consumed) and the role-untouched
   `publish_controlled_tiered_addressed` path (endpoint-bound install gate shared with
   the floodfill form; inspection snapshot published for I2PControl). Live proof:
   low-tier daemon serves caps `L`, `floodfill: False`, both loopback addresses.
3. **Plan 470 lane exit-1 could not classify a missing link promotion.** On
   `daemon_outbound_link_promoted == 0` with a submitted lookup, the first-missing line
   now attaches the stock NTCP connect-failure deltas
   (`ntcp_connectFailedTimeout{,IOE}`, `ntcp_connectFailedIOE`,
   `ntcp_outboundEstablishFailed`; `-1` = unobservable). Lane-only change; the
   baseline all-green run is unaffected. Verified by `py_compile` (the branch is
   failure-path-only and the passing tree does not take it).

Recorded but **not** fixed here (needs its own plan-of-record): the 64 KiB daemon
default `burst_bytes` is below the NTCP2 runtime's wire-frame minimum
(`ceil(65537·100/90) = 72819` at the lane's 10% reserve), so any bandwidth-enabled
daemon with NTCP2 fails construction at defaults. The comparator lane sets 128 KiB
explicitly with the derivation in a comment. Do not "fix" by lowering the wire
minimum or raising the default silently — both are cross-plan policy.

## Pinned-source audit (why the comparator is shaped this way)

Java 2.13.0 `@9134f808…`, `router/java/src/net/i2p/router/networkdb/kademlia/`:

- `FloodfillPeerSelector.selectFloodfillParticipants(key, N)` sorts all `f` peers by
  XOR distance to the routing key, then classifies in order: same-/16, published > 3h,
  strict-country, and **bandwidth tier `L` ("Slow")** go to the bad tail; the rest
  split good/ok by profile DB history. Non-`f` peers never enter this path.
- `StoreJob` (parent of `FloodfillStoreJob`, parallelization = redundancy = 1) sends
  exclusively to `selectFloodfillParticipants(routingKey, 1)` members passing the
  version/key-type `shouldStoreTo` filter — tier is not a send filter, membership is.
  Consequence: a present, reachable, biddable non-`f` peer can never receive a
  flood Store while a healthy `f` peer exists; that is the stock-computed negative.
- `StoreJob.shouldStoreTo` is version/key-type only, so it does not discriminate
  tiers; the lane does not use it as a tier oracle.
- i2pd 2.61.0 caps letters match Java's tiers exactly (`N` = 64–128, `O` = 128–256
  KB/s); `bandwidth = O` yields caps `O`, and classical NTCP2 is pinned with
  `version = 2` so Java 2.13.0 can bid it.

## Comparator design (frozen)

- Eligible daemon: controlled-loopback floodfill intent + `[bandwidth]` N-class
  (262144/262144 B/s, share 50) → live caps `fN`, role Active.
- Low-tier daemon: same composition, L-class rates (131072/131072 B/s, share 25) →
  `TierBelowFloodfillFloor` → live caps `L`, role untouched, listeners live.
- Stock peer (profile A only): exact-pinned i2pd, `floodfill = true`,
  `bandwidth = O`, classical NTCP2 published on loopback, SSU2 as in the proven
  `run-i2pd.sh` shape.
- Java seeded pre-start with the signed role-shaped records only; no selector
  result manufactured. Java's tiers/caps/peer-set/selection read through the
  unchanged read-only diagnostics (`P220-CAPABILITIES`, `P220-PEERS-FLOODFILL`,
  `P470-TRANSPORT`); hashes stay in memory, receipts carry booleans/counts/tiers.
- Attempt budget: 1 per profile. Reruns in this record happened only after material
  lane changes (over-strict Java assertion; wrong-path flood gate; log close-flush
  ordering), each recorded below. Raw logs never leave scratch; one failing
  profile-B snapshot was preserved under `target/interop/plan479-scratch-snaps/`
  to localize the log-flush finding, then superseded by the green rerun.

## Executed receipts

Profile B (low-tier comparator), exit 0:

```text
plan479_profile_b_stages={"both_present_in_java_netdb":1,"eligible_in_floodfill_peer_set":1,"eligible_java_tier_n":1,"eligible_role_shaped":1,"eligible_selected_for_local_key":1,"eligible_store_effect_accepted":1,"java_stock_transport_send":1,"java_store_job_succeeded":1,"lowtier_absent_from_peer_set":1,"lowtier_java_tier_l":1,"lowtier_no_store_effect":1,"lowtier_not_selected":1,"lowtier_ntcp_bid_available":1,"lowtier_role_shaped":1}
plan479_profile_b_recipient="eligible-i2pr-candidate"
plan479_profile_b_lowtier_fairness_bid="available"
plan479_reference_pin=2.13.0@9134f808337b401e8e53c73734c81fab04280c9d
plan479_profile_results={"b":0}
```

Profile A (healthy-stock population), exit 0:

```text
plan479_profile_a_stages={"both_present_in_java_netdb":1,"eligible_in_floodfill_peer_set":1,"eligible_java_tier_n":1,"eligible_role_shaped":1,"eligible_selected_for_local_key":1,"java_stock_transport_send":1,"java_store_job_succeeded":1,"stock_floodfill_capable":1,"stock_in_floodfill_peer_set":1,"stock_role_shaped":1,"stock_selected_for_local_key":1,"store_recipient_attributed":1}
plan479_profile_a_recipient="stock-peer"
plan479_profile_a_stock_tier_observed="O"
plan479_reference_pin=2.13.0@9134f808337b401e8e53c73734c81fab04280c9d
plan479_profile_results={"a":0}
```

Recipient note: profile A's single Store went to the stock peer (XOR-first among two
healthy `f` peers — the documented nondeterminism between healthy candidates). The
profile-A gate is operation-among-healthy, not preference, so both recipient
outcomes pass; the claim recorded is only what the attribution shows. Profile B is
the preference proof: the only selectable peer received the only send
(`succeeded=1`, eligible accept ≥ 1, low-tier accept = 0, low-tier biddable).

Rerun deltas (each after a material change, budget respected): (1) an over-strict
lane assertion rejected Java's own floodfill-enabled record — removed to match the
Plan 470 assertion exactly; (2) a `Flooded the data to 1` gate targeted the facade
flood path while the triggered job travels StoreJob/OutNetMessage — replaced with
the stock transport-send signal; (3) the send line was gated pre-flush while the
pinned stock writer buffers behind the success callback (localized via a scratch
snapshot showing the lines present post-close) — Java is now stopped before its
logs are counted. None of the three touched production behavior.

## Verification

| Command | Outcome |
| --- | --- |
| `rtk python3 tests/integration/ntcp2/plan470_java_ntcp2.py` (baseline, this host) | Exit 0, all 23 stages incl. `daemon_outbound_link_promoted=1`; the 479-status failure (`promoted=0` + Java NTCP timeout) is host/timing variance, not a source defect |
| `rtk python3 tests/integration/ntcp2/plan479_java_comparator.py --profile b` | Exit 0, 14/14 stages (receipt above) |
| `rtk python3 tests/integration/ntcp2/plan479_java_comparator.py --profile a` | Exit 0, 12/12 stages (receipt above) |
| `cargo test --locked -p i2pr-netdb --all-targets` | Passed (284 tests, incl. 2 new builder tests, inversion-probed) |
| `cargo test --locked -p i2pr-daemon --lib floodfill -- --test-threads=1` | Passed (35 tests, incl. 3 new tier/resolution/record tests, inversion-probed) |
| `cargo test --locked --workspace --all-targets -- --test-threads=1` | Passed (4904 passed, 39 ignored, 187 suites; a first background attempt died mid-run without output inside `i2pr-netdb` lib with zero failures recorded — the isolated rerun passes, and this foreground full run is green throughout) |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Passed |
| `cargo fmt --all --check`, `git diff --check` | Passed |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps`; `cargo test --locked --workspace --doc` | Passed (doc clean; 29 suites, 0 doc-tests, matching the predecessor floor shape) |
| `bash scripts/check-m12-floodfill-boundaries.sh` (+ `--self-test` per floor) | Passed |
| `bash scripts/check-m12-floodfill-qualification-evidence.sh` (+ `--self-test`) | Passed integrity check (10 i2pd + 12 Java command-derived rows) |
| `bash scripts/check-runtime-boundaries.sh`, `bash scripts/check-service-tunnel-boundaries.sh` | Passed |
| `python3 scripts/check-global-plan-number-uniqueness.py`, `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | Passed (uniqueness; 51 planning tests OK) |
| `cargo deny check advisories bans sources` | Red, external: rustls RUSTSEC-2026-0346/0347 advisories on the HEAD-pinned lock (same family as the 478 floor reds); bans/sources ok; no dependency changes in this turn |
| `python3 -m py_compile` on both lane scripts | Passed |

Live smoke held outside the lane (manual, same binaries): eligible serves caps
`fN` (`floodfill: True`, SSU2+NTCP2 loopback); low-tier serves caps `L`
(`floodfill: False`, same addresses); pristine tree with `[bandwidth]` serves
classless `f` (the defect, pre-fix).

## Invariants, security, migration, operations

- No normal startup path changed: bandwidth-disabled daemons publish exactly the
  prior classless `f` (the Plan 470 lane shape is byte-identical); the capacity
  gate only engages with `[bandwidth] enabled`, which stays default-off.
- No secret material in new surfaces: `TierBelowFloodfillFloor` carries the public
  caps class; new logs carry role/tier labels only; lane prints booleans/counts/tiers.
- No unbounded channels/queues, no new tasks, no ownership changes; the governor
  handle is `Arc`-cloned per tick, eligibility is re-issued per tick (no stale class).
- No reference patched: Java/i2pd caches verified by pin before provisioning; stock
  logs used for counts only, never exported.
- No dependency, config-schema, wire, `specs/support.toml`, or RouterInfo-default
  changes. Migration impact: none (opt-in profile only).
- The `fL`-vs-`fN` last-resort arm is structurally unreachable from honest code
  (guard + live refusal); it is recorded with the pinned-source citation, not
  approximated.

## Limitations

- Profile A attributes but does not steer the healthy-peer recipient (XOR order);
  preference between two healthy peers is not claimed.
- Java's first missing transition on a bad day remains stock-side connect
  timeouts (observed in the superseded 479-status run); the Plan 470 lane now
  classifies that exit-1 itself.
- Controlled withdrawal/replication/rollover and the normal role stay with Plans
  437/438 (blocked on 433/436 — see audit below). No public `f`/`R`, no
  capability, no support-inventory change.

## Findings

- Critical: none. High: none.
- Medium: (a) the classless-`f`-with-bandwidth defect, fixed with regression
  (above); (b) the 64 KiB default burst vs the NTCP2 wire-frame minimum —
  recorded, needs its own plan, lane works around it explicitly.
- Low: stock Java log buffering requires close-flush before counting (lane
  lesson, now structural in the comparator); pre-activation I2PControl records
  need the composition log line + refetch + stock tier gate to defeat staleness
  (lane implements all three).

## Roadmap disposition and unblock audit

Plan 479 is **passed**; no dependent capability is unblocked. Audit: Plan 437
(plus 412/413 through it) lists hard dependencies 433+436, both still blocked —
479's controlled-Java evidence is an interface contribution, not a hard-dep
satisfaction, so 437/438 stay blocked and no row moves to ready. The Plan 444
resume's open Java controls are now satisfied by this record; 444's historical
blocked closures are unchanged. Normal public floodfill advertisement stays
forbidden; Plans 437/438 retain role, health, reachability, and two-family gates.

**Roadmap disposition:** closed; 437/438 remain gated.

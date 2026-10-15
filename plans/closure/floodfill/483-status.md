# Plan 483 closure — Plan 479 residual-risk hardening

**Status token:** `passed-479-residual-risks-executable-no-normal-role-change`
**Disposition:** passed. 479's three recorded limitations are now executable
evidence: (1) profile A's XOR-order recipient is proven starvation-free and
key-determined within each run (both healthy peers win their own near-key at
width 1; this run's live recipient was the eligible candidate, 479's was the
stock peer — the documented variance, now bounded); (2) the unreachable
`fL`-vs-`fN` arm is covered by a mechanical pinned-source pin plus an executed
degenerate L-only profile that fails closed end to end; (3) the tiered
publication forms are proven to flow through the existing M12
replication/withdrawal owners unchanged, with a written handoff 437/438 can
consume. No normal-role gate moved: 437/438 stay blocked on 433/436, no
support/capability/advertisement change.

**Implementation commits:** `deed140b` (width-1 probe +
diagnostic, lane profiles/gates/pin, three product regression tests) and
the lane/status commit containing this closure (receipts A/B/C, floor
outcomes, registry/roadmap). Lane script:
`tests/integration/ntcp2/plan479_java_comparator.py` (`--profile a|b|c|all`,
frozen per-profile attempt budget 1; `--profile all` runs C, B, A).

**Number note:** 483 is taken for this explicitly scoped purpose, not for a
stock-timeout retry — the registry's prohibition on a timeout-retry 483 is
purpose-specific and is not violated: no failed stock timeout was retried and
no 475/461 lower-tier proof repeated.

## Requirement-to-evidence matrix

| Requirement (implementation plan) | Evidence | Result |
| --- | --- | --- |
| WP1: width-1 multi-key no-starvation proof (R1) | `P220-SELECTOR-N1` (additive case + `selectForKeyWidth` overload, test-only, stock untouched); near-keys (last-byte flip, ≤0xFF) with fail-closed pairwise separability precheck incl. Java-self; A: each healthy peer wins its own near-key; B: eligible wins both near-keys | Passed |
| WP2: pinned-source pin + degenerate profile C (R2) | `assert_selector_pin` (L-demotion + empty-selection-fail branches, exit 2 on drift); profile C 10/10: L-only topology, peer-set absence, empty width-1, `submitted=1 succeeded=0 failed=1`, zero sends, zero effects | Passed |
| WP3: tiered-form replication/withdrawal regressions (R3) | 3 new unit tests (service candidacy, candidate-scan role gating, per-tick TierBelow determinism), each inversion-probed | Passed |
| WP4: closure, registry/roadmap, 437/438 handoff | This record; registry 483 row → passed; floodfill-roadmap §7 row + paragraph; handoff section below | Passed |
| Pass gate: C + one controlled A/B replay each; floor green; no support change | Receipts below; floor table below; `specs/support.toml` untouched | Passed |

## Pinned-source facts used (2.13.0 `@9134f808…`, kademlia/)

- `FloodfillPeerSelector`: good → ok → bad fallback, XOR-sorted within each
  class; same-/16, >3h-old, strict-country, and tier-`L` demote to the bad
  tail. Both loopback peers share 127.0.0.1, so profile A selects the
  XOR-nearest *bad* peer at width 1 — determinism + starvation-freedom, not
  health preference (never claimed for A).
- `StoreJob`: empty flood selection with nothing pending → `fail()`; the
  degenerate job ends `failed=1` without sending.
- Non-`f` peers never enter the selector path (capability-indexed input).
- Java runs `router.floodfillParticipant=true`, so it self-lists in its own
  `getPeersByCapability('f')` index — but self is not k-bucketed, so it never
  contends in width-1 verdicts. Profile C's gates are absence-based for this
  reason; asserting emptiness was a lane defect, corrected in-plan (below).

## Executed receipts

Profile C (degenerate last-resort arm), exit 0, first full pass after two
in-plan lane corrections:

```text
plan479_profile_c_stages={"java_no_stock_transport_send":1,"java_store_job_failed_bounded":1,"lowtier_absent_from_peer_set":1,"lowtier_java_tier_l":1,"lowtier_no_store_effect":1,"lowtier_not_selected_for_local_key":1,"lowtier_present_in_java_netdb":1,"lowtier_role_shaped":1,"lowtier_without_floodfill_capability":1,"near_key_selects_no_lowtier_n1":1}
plan479_profile_c_degenerate_outcome="fail-closed"
plan479_reference_pin=2.13.0@9134f808337b401e8e53c73734c81fab04280c9d
plan479_profile_results={"c":0}
```

Profile B (controlled replay — the N1 gates are a material lane change), exit 0:

```text
plan479_profile_b_stages={"both_present_in_java_netdb":1,"eligible_in_floodfill_peer_set":1,"eligible_java_tier_n":1,"eligible_role_shaped":1,"eligible_selected_for_local_key":1,"eligible_store_effect_accepted":1,"eligible_wins_lowtier_near_key_n1":1,"eligible_wins_own_near_key_n1":1,"java_stock_transport_send":1,"java_store_job_succeeded":1,"lowtier_absent_from_peer_set":1,"lowtier_java_tier_l":1,"lowtier_loses_lowtier_near_key_n1":1,"lowtier_no_store_effect":1,"lowtier_not_selected":1,"lowtier_ntcp_bid_available":1,"lowtier_role_shaped":1}
plan479_profile_b_recipient="eligible-i2pr-candidate"
plan479_profile_b_lowtier_fairness_bid="available"
plan479_reference_pin=2.13.0@9134f808337b401e8e53c73734c81fab04280c9d
plan479_profile_results={"b":0}
```

Profile A (controlled replay), exit 0:

```text
plan479_profile_a_stages={"both_present_in_java_netdb":1,"eligible_in_floodfill_peer_set":1,"eligible_java_tier_n":1,"eligible_role_shaped":1,"eligible_selected_for_local_key":1,"eligible_wins_own_near_key_n1":1,"java_stock_transport_send":1,"java_store_job_succeeded":1,"stock_floodfill_capable":1,"stock_in_floodfill_peer_set":1,"stock_role_shaped":1,"stock_selected_for_local_key":1,"stock_wins_own_near_key_n1":1,"store_recipient_attributed":1}
plan479_profile_a_recipient="eligible-i2pr-candidate"
plan479_profile_a_stock_tier_observed="O"
plan479_reference_pin=2.13.0@9134f808337b401e8e53c73734c81fab04280c9d
plan479_profile_results={"a":0}
```

Recipient note: 479's profile A went to the stock peer, this run to the
eligible candidate — the XOR-key variance 479 recorded, now with the proof
that both outcomes are starvation-free single-recipient selections rather
than an unobserved bias. The profile-A gate remains operation-among-healthy,
not preference.

Rerun deltas (each after a material lane change, budget respected): (C1) the
peer-set gate asserted emptiness but Java self-lists — replaced with an
absence gate plus a non-vacuity pin (index live via self); the N1 count
corrected 1→0 with the reason recorded (selector consumes k-bucket `f`
members; self-listing lives only in the capability index). This correction
also fixed B's design before it ran: the lowtier-adjacent winner is
deterministically eligible (sole `f` member), not a coin flip with self.
(C2) the failed-job status read reused the exhausted run deadline and saw
"unavailable" — fresh deadline. None of the three touched production
behavior. A/B each ran once with the new gates (one controlled replay,
causal delta = N1 gates + pin + self-listing model).

## Unit regressions (all inversion-probed)

1. `tiered_floodfill_record_is_validated_acknowledged_and_offered_for_replication_like_classless_f`
   (`floodfill_service.rs`): tiered-`fN` direct-publisher store →
   Inserted + ack + replication offer. Inversion (tunnel ingress):
   offer lost → FAILS as required, reverted.
2. `tiered_addressed_record_is_disseminated_but_never_a_floodfill_referral`
   (`floodfill_service.rs`): `L` direct-publisher store → Inserted + offer
   (dissemination is caps-agnostic); candidate scan admits `fN` only to the
   floodfill set and `L` only to the non-floodfill set (this scan also feeds
   DatabaseSearchReply referrals). Inversion (eligible caps `fN`→`M`):
   floodfill-set membership lost → FAILS as required, reverted.
   (Two vacuous mutant attempts recorded honestly: `fM` still contains `f`;
   candidacy is caps-agnostic by design, so only the role-gating mutant bites.)
3. `tier_below_floor_refuses_identically_every_tick` (daemon `floodfill.rs`):
   L-class inputs resolve to the addressed form on 3 consecutive ticks —
   no tick can drift into activation (the live shape phases prove the
   caller maps this to the tiered addressed publication). Inversion
   (expect `N`): FAILS as required, reverted.
4. A neighboring-test repair during probing: a broad revert script
   over-matched into `router_tunnel_ack_route_is_copied_but_never_replication_authority`;
   restored byte-identical (diff verified purely additive, suite green).

Production changes: none. All three tests run through existing owners;
no behavior, config, wire, or dependency change.

## Verification

| Command | Outcome |
| --- | --- |
| `rtk python3 tests/integration/ntcp2/plan479_java_comparator.py --profile c` | Exit 0, 10/10 stages (receipt above) |
| `... --profile b` | Exit 0, 17/17 stages (receipt above) |
| `... --profile a` | Exit 0, 14/14 stages (receipt above) |
| `cargo test --locked -p i2pr-netdb --all-targets` | Passed (286 tests, incl. 2 new tiered-form tests, inversion-probed) |
| `cargo test --locked -p i2pr-daemon --lib floodfill -- --test-threads=1` | Passed (36 tests, incl. 1 new per-tick refusal test, inversion-probed) |
| `cargo test --locked --workspace --all-targets -- --test-threads=1` | Passed (4907 passed, 39 ignored, 187 suites) |
| `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` | Passed (No issues found) |
| `cargo fmt --all --check`, `git diff --check` | Passed |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps`; `cargo test --locked --workspace --doc` | Passed (doc clean; 29 suites, 0 doc-tests, matching the predecessor floor shape) |
| `bash scripts/check-m12-floodfill-boundaries.sh` (+ `--self-test`) | Passed |
| `bash scripts/check-m12-floodfill-qualification-evidence.sh` (+ `--self-test`) | Passed integrity check (10 i2pd + 12 Java command-derived rows) |
| `bash scripts/check-m6-mixed-router-acceptance-evidence.sh` | Passed (Plan 220 §14 invariants hold with the additive N1 command) |
| `bash scripts/check-runtime-boundaries.sh`, `bash scripts/check-service-tunnel-boundaries.sh` | Passed |
| `python3 scripts/check-global-plan-number-uniqueness.py`, `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | Passed (uniqueness; 51 planning tests OK) |
| `cargo deny check advisories bans sources` | Red, external: rustls RUSTSEC-2026-0346/0347/0348/0349 on the HEAD-pinned lock (same family as the 479 floor reds — the advisory DB now lists two more of the same crate family); bans/sources ok; no dependency changes in this turn |
| `python3 -m py_compile` on the lane; out-of-tree `javac` of the helper | Passed |

## Invariants, security, migration, operations

- No production change: no normal startup path, no RouterInfo default, no
  wire, no config-schema, no dependency, no support-inventory change.
- `P220-SELECTOR-N1` is additive; existing `P220-SELECTOR` (width 3) and all
  M6 lanes that consume it are untouched (m6 evidence checker asserts
  presence, still green — see table).
- Lane prints booleans/counts/tiers only; peer hashes stay in memory;
  scratch handling unchanged (`I2PR_PLAN479_KEEP_SCRATCH` snapshots failures).
- No new tasks/channels/queues; unit tests deterministic, no sleeps.
- Rollover inputs (`RecordId` + day) contain no caps by signature — verified
  by inspection (`replication.rs`: `routing_key`, `should_cover_next_key`);
  no test can pin a non-existent coupling, recorded here instead of
  approximated.

## Limitations

- Profile A still attributes rather than steers the healthy-peer recipient
  (XOR order) — now proven starvation-free, still not steered.
- The `fL`-bearing-peer variant remains unexecuted by design: i2pr cannot
  publish it (guard), and a stock-`fL` topology would test Java+i2pd, not
  i2pr, on top of unproven i2pd L floodfill eligibility. The pin +
  degenerate arm is the honest coverage.
- Live two-router replication/rollover/withdrawal execution stays entirely
  with 437/438. What 483 delivers is planning-level proof the tiered forms
  do not diverge through existing owners.
- Burst_bytes default policy (479 finding) still needs its own plan.

## Findings

- Critical: none. High: none.
- Medium: none in production. Two lane-design corrections (self-listing
  model, exhausted-deadline status read) — both fixed in-plan with
  regressions (the corrected gates themselves).
- Low: a broad revert script over-matched a neighboring test during
  inversion probing (caught by compile + suite, restored byte-identical);
  probe scripts must use uniquely-scoped replacements. The `fM` vacuous
  mutant (above) — a reminder that mutants must target the asserted
  coupling, not a bystander.

## Roadmap disposition and unblock audit

Plan 483 is **passed**; no dependent capability is unblocked. Audit: Plan 437
lists hard dependencies 433+436, both still blocked — 483's interface
contribution (deterministic selection proof, degenerate fail-closed arm,
tiered-form owner regressions) does not satisfy reachability/transit
prerequisites, so 437/438 stay blocked and no row moves to ready. Plan 412's
recorded hard dependency (437 selection evidence) is unchanged in kind —
483 narrows the future 437 proof obligation but satisfies none of it.

**Roadmap disposition:** closed; 437/438 remain gated.

## Handoff notes (consumable by 437/438, 412/413)

- 437: the controlled Java selection interface now includes a deterministic
  width-1 proof harness (`P220-SELECTOR-N1` + near-key methodology) reusable
  for the normal-role selection proof when 433/436 clear; the L-demotion pin
  guards the stock semantics 437's negative controls will rely on.
- 438: tiered `fN`/`L` records are proven dissemination-safe and
  role-gated through the current owners; the live replication/rollover/
  withdrawal execution itself is untouched and remains 438's gate.
- 412/413: Java selection evidence stays interface-only; the hard
  dependency on 437 is unchanged.

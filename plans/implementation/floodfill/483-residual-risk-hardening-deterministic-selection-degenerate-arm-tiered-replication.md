# Plan 483 — Plan 479 residual-risk hardening (deterministic selection, degenerate arm, tiered-form replication)

**Status:** registered / ready (2026-10-15). **Owner:** floodfill subsystem,
controlled-loopback Java comparator lane + existing M12 replication/withdrawal
owners. **Baseline:** work branch `work/plans-477-480-router-recovery` at the
Plan 479 lane commit `408c027c` on production commit `6a94ac81`.
**Authoritative predecessor:** `plans/closure/floodfill/479-status.md` (passed;
§Limitations records exactly the three risks closed here). This is a NEW
bounded scope over 479's recorded limitations — not a rerun of Plan 475/461
lower-tier proofs, and not a retry of any failed stock timeout. (Registry
prohibits creating Plan 483 *for the next failed stock timeout*; this plan's
number is taken for a different, explicitly scoped purpose, and no stock-timeout
retry is authorized by it.)

## Objective (one bounded outcome)

Convert 479's three recorded residual risks from prose limitations into
executable, pinned evidence, without touching normal-role gates:

1. Profile-A recipient nondeterminism → a deterministic within-run proof that
   the single-recipient outcome is XOR-key-determined and starvation-free
   across keys (membership attribution stays; steering is still not claimed).
2. Structurally unreachable `fL`-vs-`fN` arm → a mechanical pinned-source pin
   plus an executed degenerate arm (L-only, non-`f`) proving fail-closed
   behavior instead of a citation alone.
3. Normal-role replication staying with 437/438 → deterministic regressions
   proving 479's new tiered publication forms flow through the EXISTING M12
   replication/withdrawal owners unchanged, plus a written handoff 437/438 can
   consume. No 437/438 unblock is claimed (433/436 still blocked).

## Why ready

Hard deps closed: 479 passed (comparator lane, tiered builders, capacity gate);
454 passed (governor/permit); 475 passed (Java two-role lane + helper
diagnostics); 303 passed (one-family matrix); 350/351 passed (type-5
serve/consume — untouched). Interface deps stable: `P220-SELECTOR
<target-hex> <b-hex>` already probes arbitrary keys read-only at width 3;
pinned Java source checkout verifies by pin in the lane before provisioning;
`ReplicationPlanner::plan` and controlled withdrawal are existing tested
owners. No ADR gap: ADR 0026/0027/0030 govern; no new router role or
advertisement is proposed.

## Current implementation evidence

- `tests/integration/ntcp2/plan479_java_comparator.py` (1002 lines): profiles
  A (12/12) and B (14/14) green; recipient attribution is membership-based;
  both-selected-at-width-3 gates pass because `PROBE_FANOUT = 3` covers two
  peers; the live Store travels width-1 (`redundancy = 1`) so the recipient is
  XOR-first for the run's fresh local-RI routing key.
- `P220SelectorProbe.selectForKey` calls stock
  `selectFloodfillParticipants(key, 3, empty, kbuckets)` read-only; the helper
  is ours, stock is never patched.
- Pinned source facts (2.13.0 `@9134f808…`,
  `router/java/src/net/i2p/router/networkdb/kademlia/`): `FloodfillPeerSelector`
  demotes tier-`L` to the bad tail (same branch as same-/16, >3h-old,
  strict-country); `StoreJob` fails the job on empty selection (line ~186);
  non-`f` peers never enter the selector path.
- `ReplicationPlanner::plan` filters targets on
  `verified && floodfill && !hidden` plus provenance/age/type — caps letters
  are not inputs. `ReplicationCandidate` is `(RecordId, source_peer)`.
  Controlled withdrawal (`withdraw_controlled`, `prepare_normal_withdrawal`)
  already removes `f` and keeps the address (tested classless).

## Invariants that must not regress

- Exact pins frozen (Java 2.13.0 `@9134f808…`, i2pd 2.61.0 `@635b013a…`);
  loopback-only; no patched stock; no forced `f`/`R`; `L` stays last-resort,
  never categorically rejected; no public `f`/`R` advertisement;
  `specs/support.toml` and `specs/CONFORMANCE.md` unchanged; 437/438 stay
  blocked on 433/436; frozen per-profile attempt budget 1; sanitized receipts
  only (booleans/counts/tier-letters); no unbounded channels/queues/tasks.

## Scope

In: (WP1) width-1 multi-key selector probe + lane gates; (WP2) lane-side
selector-pin assertion + profile C degenerate run; (WP3) tiered-form
replication/withdrawal unit regressions through existing owners; (WP4) closure,
registry/roadmap updates, 437/438 handoff note. Test-only helper + lane +
unit-test changes; production changes expected: none.

Explicitly out: any stock-i2pd `fL` topology (would test Java+i2pd, not i2pr,
and i2pd's own L floodfill eligibility is unproven pinned behavior —
manufacturing it risks a fabricated-topology pass); any normal-role
replication/rollover/withdrawal execution (437/438 scope); any live two-router
replication (438 scope); burst_bytes default policy (recorded 479 finding,
needs its own plan); 478 STREAM/recovery work.

## Ordered work packages

### WP1 — Deterministic single-recipient proof (R1)

**Mechanism.** XOR distance is deterministic for a fixed (key, peer-set).
The lane crafts two keys per profile run: `key_near_X = H_X` with the last
byte flipped (distance ≤ 0xFF from `H_X`), after a fail-closed precheck that
`dist(H_A, H_B) > 0xFF` as integers (violates with probability ~2⁻²⁴⁸;
failure reports `degenerate-peer-hashes`, never silently passes). A new
read-only diagnostic `P220-SELECTOR-N1 <target-hex> <b-hex>` (additive case in
`ControlledRouter.java` + width-parameter overload in `P220SelectorProbe`,
test-only, no stock touched) returns the width-1 selection for the key.

**Gates.** Profile A: width-1 for `key_near_eligible` is exactly `{eligible}`;
for `key_near_stock` exactly `{stock}` (no starvation either direction; every
winner is a member of the healthy-`f` set). Profile B: width-1 for
`key_near_eligible` AND `key_near_lowtier` is exactly `{eligible}` (cross-key
preference proof: the reachable, biddable non-`f` peer never wins). Keep the
existing width-3 membership gates and the live-Store membership attribution
unchanged; document the XOR mechanism in the lane header (why both width-3
membership AND width-1 single-recipient can hold simultaneously).

### WP2 — Degenerate last-resort arm (R2)

**Pin.** `check_environment` gains `assert_selector_pin(java_source)`:
`FloodfillPeerSelector.java` contains the `getBandwidthTier().equals("L")`
→ bad-tail branch; `StoreJob.java` contains the empty-selection → `fail()`
branch. Mismatch fails hard (exit 2), same class as pin checks. The lane
refuses to run against a stock source whose demotion semantics drifted.

**Profile C (L-only, budget 1).** Seed Java with the truthful-L non-`f`
record only (same low-tier daemon shape as profile B, no eligible daemon, no
stock peer). Gates: present in NetDB; tier `L` as observed by stock Java; no
`f` capability; floodfill peer set EMPTY; width-1 selection for the local key
and the near-lowtier key both empty; triggered normal Store ends
`submitted=1 succeeded=0 failed=1` within the run timeout (bounded,
single submission — the helper's `STORE_SUBMITTED` guard already prevents
resubmission); zero NTCP send lines; zero daemon flood effects; Java stays
diagnosable throughout (no crash/hang). This executes the nearest reachable
arm to `fL`-vs-`fN` with honest code: the guard's refusal cannot conjure a
fake selection, and stock fails closed instead of storing to non-`f`.

### WP3 — Tiered-form replication/withdrawal regressions (R3)

Through existing owners, no behavior change (if a divergence surfaces and is
source-owned, fix with regression in-plan; else stop with a finding):

1. `floodfill_service.rs` tests: a controlled tiered-`fN` RouterInfo stored
   via direct-publisher ingress is validated, acknowledged, and offered for
   replication exactly like classless `f` (model on
   `direct_publisher_store_is_validated_acknowledged_and_offered_for_replication`);
   inversion: strip the `f` flag (tiered-addressed form) and assert NO
   replication offer from the service path.
2. `replication.rs` planner: a peer view derived from the truthful-L non-`f`
   record (`verified=true`, `floodfill=false`, `hidden=false`) is excluded
   from replication targets even beside eligible `f` peers; an `fN` peer view
   is selected by XOR order identically to classless `f` (caps-agnostic
   ordering pin).
3. Daemon `floodfill.rs` tests: withdrawal preparation on a tiered-`fN`
   record removes `f` and keeps the address (tiered variant of
   `normal_withdrawal_prepare_removes_f_and_keeps_address`); the TierBelow
   path never opens activation across repeated ticks (pin the 479 invariant
   against future edits).
4. Rollover: assert the next-key coverage decision is unchanged for tiered
   records (keys derive from record key + day, never caps) — planner-level,
   via `should_cover_next_key`/routing-key behavior already owned by
   `replication.rs`.

### WP4 — Closure and handoff

`plans/closure/floodfill/483-status.md` (matrix, receipts A/B/C, unit +
inversion outcomes, limitations, findings, unblock audit confirming 437/438
stay blocked); registry row + floodfill-roadmap §7 row + short paragraph;
explicit 437/438 handoff section naming the consumable interface
(capacity-gated tiered publication, tiered builders, comparator lane with
deterministic selection proof, degenerate fail-closed arm, tiered-form
replication/withdrawal regressions). No 437/438 plan-text rewrites (correct
forward, never rewrite history).

## Failure / cancellation / restart / contention semantics

Lane: per-profile budget 1; A/B rerun at most once total (the N1 gates are a
material lane change — one controlled replay with the causal delta recorded);
C runs once. Any second failure stops the profile with first-missing-stage.
Java/daemons are stopped in `finally` (existing structure); close-flush
before log counts (479 lesson). Unit tests: deterministic, caller-supplied
time where time is involved; no wall-clock sleeps. No new tasks/channels.

## Compatibility and migration

None: no config-schema, wire, RouterInfo-default, dependency, or support-
inventory change. Lane/test-only. The `P220-SELECTOR-N1` command is additive;
existing `P220-SELECTOR` behavior untouched.

## Required tests

- New unit tests in `floodfill_service.rs`, `replication.rs`, daemon
  `floodfill.rs` (WP3), each inversion-probed (break the assertion target,
  observe the exact failure).
- Lane: A (12 existing + N1 stages), B (14 existing + N1 stages), C (new
  stage set, all green) — each exit 0 once.
- `python3 -m py_compile` on the lane; `javac` of the helper via the lane's
  existing build step (it compiles the probe sources out-of-tree).

## Exact verification commands

```text
cargo fmt --all --check
cargo test --locked -p i2pr-netdb --all-targets
cargo test --locked -p i2pr-daemon --lib floodfill -- --test-threads=1
rtk python3 tests/integration/ntcp2/plan479_java_comparator.py --profile c
rtk python3 tests/integration/ntcp2/plan479_java_comparator.py --profile b
rtk python3 tests/integration/ntcp2/plan479_java_comparator.py --profile a
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-m12-floodfill-boundaries.sh
bash scripts/check-m12-floodfill-boundaries.sh --self-test
bash scripts/check-m12-floodfill-qualification-evidence.sh
bash scripts/check-m12-floodfill-qualification-evidence.sh --self-test
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

Order note: run C first (cheapest failure mode — if the degenerate arm
misbehaves, stop before spending A/B reruns), then B, then A.

## Documentation updates

Closure record; registry row; floodfill-roadmap §7 row + paragraph; lane
header documenting the XOR mechanism; no architecture-deep-dive or skill
changes (plan state is never mirrored outside `plans/`).

## Acceptance criteria

- Profile C exit 0 with all new stages 1 (fail-closed arm executed, not cited).
- Profiles A/B each exit 0 once with existing + N1 stages all 1 (one controlled
  replay each, causal delta recorded).
- Pin assertion rejects a tampered/empty demotion branch (negative-tested).
- All new unit tests pass + inversion-probed; full floor green (external reds,
  if any, recorded as external with no dep changes).
- Closure record complete per `plans/closure/README.md`; unblock audit shows
  437/438 still blocked on 433/436; no support/capability/advertisement change.

## Stop conditions

- N1 probe shows a healthy peer never winning (starvation): stop, record —
  contradicts the XOR model; fix in-plan only if a source-owned fixture defect
  is localized, else blocked finding. No selector/peer-set production change
  to force a winner.
- Profile C sends to non-`f` or succeeds: stop, blocked finding — contradicts
  pinned source; no patch to stock or guard to force the expected outcome.
- Tiered replication/withdrawal diverges from classless: fix if source-owned
  with regression, else stop with finding. Never weaken a guard or checker to
  pass.
- Second consecutive same-cause lane failure without new information: stop per
  finite-budget policy.

## Closure evidence required

Implementation commits; requirement-to-evidence matrix (R1/R2/R3 → gates);
all receipts (A/B rerun + C, sanitized); unit + inversion outcomes; floor
command table; limitations; findings by severity; roadmap disposition
(closed, no dependent row moves); 437/438 handoff.

## Handoff notes

Consumable by 437 (controlled Java selection interface + deterministic proof),
438 (tiered-form replication/withdrawal regressions through existing owners),
412/413 (Java selection evidence remains interface-only, hard dep on 437
unchanged). Normal-role replication/rollover/withdrawal execution stays
entirely with 437/438; this plan asserts nothing about live two-router
replication.

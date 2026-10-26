## Status

`passed-transit-creator-correlation-retention`

Plan 508 is closed. The transit owner's creator-correlation facts are
**bounded over the router's lifetime** instead of growing with every build
the router performs, and the bound is **the coordinator's own**.

**The gap was a recorded finding the repo already owned.** Plan 502 gave
the transit owner the creator-correlation facts it needs to keep the
router's own coordinator traffic out of the transit path, and recorded as
Finding M1 that those facts **never expire**:

> The owner's `creator_builds`, `creator_data` and `creator_gateways` are
> `BTreeSet`s with only `insert`; the production wiring calls
> `install_transit_creator_correlation` on **every** pump tick, so each
> distinct build the router performs leaves one entry behind forever.

Plan 502 closed with:

> The fix is a retention window, not a cap: a fact should be dropped only
> after the coordinator could no longer accept a reply for it, which means
> deriving the window from the coordinator's attempt timeout. That is a
> protocol decision, not a mechanical one, and it is not made here.

**This plan makes that decision, explicitly and on the record.** A fact is
installed while the coordinator's build is **pending**; the coordinator's
own memory of that build's identifiers starts when the build is
**retired**. The window is therefore the build deadline plus the
coordinator's own longest identifier retention —
`BUILD_DEADLINE_MS + BUILD_TUNNEL_ID_RETENTION_MS`, 46 minutes — derived
through a constant from bounds the tree already enforces.

**The plan's own file list was corrected against what landed.** The plan of
record's §6 production-change table did not list the **counts accessor**
(`TransitLiveOwner::creator_correlation_counts`), which the live lane
needs as its observability seam. The correction lives in this record and
in the registry row; the plan of record is not rewritten, and no closed
record is rewritten.

**The work packages ran out of order, and that is recorded.** The plan put
the feasibility artefact first, on the grounds that it *may stop the
plan*. The implementation landed first, so the artefact functions as the
**evidence** rather than as the **gate**. It still carries two sensitivity
controls of its own, and it caught a real fragility (§ *Three weaknesses
found while building the controls*).

**Nothing is promoted.** `router.version` stays `0.9.62`; `caps` and the
published RouterInfo are unchanged; the support inventory's advertised
count stays **zero**; `Cargo.toml` and `Cargo.lock` are byte-identical.

## Commits

| Commit | Subject |
| --- | --- |
| `aaee346c` | docs(plans): register Plan 508, the transit creator-correlation retention window |
| `e36e3e79` | feat(daemon): bound the transit creator-correlation facts by a window |
| `9fbbebb9` | test(daemon): the Plan 508 feasibility artefact, guard and live lane |
| `<closure>` | docs(plans): close Plan 508 and advance the network-participation roadmap |

## What changed

### The gap

`crates/i2pr-daemon/src/transit_owner.rs` held the three facts as
insert-only `BTreeSet`s, and the production wiring
(`crates/i2pr-daemon/src/lib.rs`) installed them on **every** pump tick.
The function's own doc comment admitted the shape:

> The sets only grow: a fact that was true stays true for the router's
> lifetime, and the number of facts is bounded by the coordinator's
> pending-build and tunnel-retention ceilings times the number of builds
> the router performs.

That is a bound **per build**, not over time.

### The window is the coordinator's own bound

`exploratory_build::CREATOR_CORRELATION_RETENTION_MS` is derived from
`BUILD_DEADLINE_MS` and `BUILD_TUNNEL_ID_RETENTION_MS`, the two bounds
the tree already enforces:

- `BUILD_DEADLINE_MS` — how long a build attempt can stay **pending**, so
  the fact is installed at the earliest moment it can exist and must
  survive that long before the coordinator's own clock has even started;
- `BUILD_TUNNEL_ID_RETENTION_MS` — how long the coordinator still
  remembers a **retired** build's identifiers.

**The coordinator's own memory of a build's identifiers is the definition
of "the coordinator could still accept a reply for it".** The transit
owner's fact must live at least as long as that memory, and because the
fact is installed while the build is pending rather than when it is
retired, the pending deadline must be added to it.

There is **no new configuration key**: `config.rs` is untouched, and the
static guard refuses a `retention` or `correlation` key there.

### The expiry is checked at use time, and swept on the tick

Two different things are needed, and they are different:

- **Correctness** is decided at **use time**: a fact is honoured only if
  it has not expired. That is the safe direction — an expired fact is
  declined, which is the benign error, because the coordinator has already
  forgotten the build and could not have matched the reply either.
- **Memory** is reclaimed by a **sweep** that drops the expired entries.
  The sweep rides the pump tick that already installs the facts, so the
  owner gains no second timer and no second task.

### Every comparison rounds in the safe direction

Two of the four use sites (`dispatch_router_garlic` and
`dispatch_build_body`) carry `now_seconds` and two
(`handle_tunnel_data_inner` and `handle_gateway_inner`) carry `now_ms`.
The window is stored in **seconds**:

- at install time the expiry is computed with a **ceiling**, so the stored
  second is never earlier than the true millisecond expiry;
- at a milliseconds site the clock is read with a **floor**, so the fact is
  honoured for at least as long as its expiry allows.

### Three things the change deliberately does not do

- It does not touch the **coordinator's own** fact set, which was already
  bounded and already expired. Only the owner's **copy** was unbounded.
- It does not change the **transit participation posture**, the relay
  accounting or the OBEP path. Plan 492 owns them.
- It does not add a **cap**. A count cap would drop the *oldest* fact,
  which is exactly the fact whose reply is most likely still to arrive.
  Plan 502 said the fix is a retention window, not a cap, and that is
  what landed.

## Requirement-to-evidence matrix

| Requirement | Evidence |
| --- | --- |
| The window is the coordinator's own bound | `crates/i2pr-daemon/src/exploratory_build.rs`; guard rule `the_window_must_be_the_trees_own_bound` |
| Every installed fact carries an expiry | `crates/i2pr-daemon/src/transit_owner.rs`; guard rule `every_installed_fact_must_carry_an_expiry` |
| The expiry is computed in the safe direction | the same file; guard rule `the_expiry_must_be_computed_in_the_safe_direction` |
| Every use site consults the expiry | the same file; guard rule `every_use_site_must_consult_the_expiry` |
| The facts are maps with an expiry, not insert-only sets | the same file; guard rule `the_facts_must_be_maps_with_an_expiry` |
| The sweep drops the expired facts | the same file; guard rule `the_sweep_must_drop_the_expired_facts` |
| The production wiring installs and sweeps on the tick | `crates/i2pr-daemon/src/lib.rs`; guard rule `the_wiring_must_install_and_sweep` |
| No new configuration key | `crates/i2pr-daemon/src/config.rs` is untouched; guard rule `the_retention_policy_must_add_no_configuration_key` |
| The owner gains no task and no timer | `crates/i2pr-daemon/src/transit_owner.rs`; guard rule `the_owner_must_gain_no_task_or_timer` |
| The counts are the observability seam | the same file; guard rule `the_counts_must_be_the_observability_seam` |
| An absent function fails closed | guard rule `an_absent_function_fails_closed` |
| The window is derivable and the facts are bounded | `crates/i2pr-daemon/tests/transit_creator_retention_feasibility.rs` — 7 tests |
| A live fact still bypasses transit | `crates/i2pr-daemon/tests/transit_creator_retention_live.rs` — `a_fact_inside_the_window_still_bypasses_transit` |
| A fact is never declined early | the same file — `a_fact_at_the_last_live_instant_still_bypasses_transit` |
| An expired fact is declined | the same file — `an_expired_fact_no_longer_bypasses_transit` |
| The facts are bounded over many builds | the same file — `the_sweep_bounds_the_facts_over_many_builds` |
| The counts are readable | the same file — `the_counts_are_the_observability_seam` |
| The M11 transit lane is unchanged | `cargo test --locked -p i2pr-daemon --test m11_transit_live_owner` — 41 passed |
| The controls are not vacuous | the four negative controls, verified on the real tree |
| Nothing is advertised | `specs/support.toml` — 100 surfaces, 0 advertised |
| No new dependency | `git diff --exit-code <base> HEAD -- Cargo.toml Cargo.lock` |

## Commands run

### The routine floor

<!-- FLOOR-TABLE -->

### The focused verification

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | PASS |
| `cargo check --locked -p i2pr-daemon --all-targets` | PASS |
| `cargo clippy --locked -p i2pr-daemon --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --locked -p i2pr-daemon --test transit_creator_retention_feasibility --test transit_creator_retention_boundary --test transit_creator_retention_live -- --test-threads=1` | PASS — 7 + 24 + 6 |
| `cargo test --locked -p i2pr-daemon --test m11_transit_live_owner -- --test-threads=1` | PASS — 41 (unchanged) |
| `cargo test --locked -p i2pr-daemon --test peer_reconcile_boundary --test netdb_retention_boundary --test netdb_retention_live -- --test-threads=1` | PASS — 28 + 26 + 8 (unchanged) |
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS |
| `python3 scripts/check-tooling-inventory.py` | PASS |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 51 tests |

### The four negative controls, verified on the real tree

Each control was applied to the **real** production source, run against the
**real** lanes, and reverted; the files were restored **byte-for-byte**
after each, and the restoration was asserted.

| Control | Mutation | Failing tests | Named failures |
| --- | --- | --- | --- |
| A | a use site stops consulting the expiry | 6 | `an_expired_fact_no_longer_bypasses_transit`, `every_use_site_must_consult_the_expiry`, `the_use_site_rule_fires_when_the_expiry_is_not_consulted`, … |
| B | the sweep compares against nothing and retains everything | 11 | `an_expired_fact_is_declined`, `the_sweep_must_drop_the_expired_facts`, `the_facts_are_bounded_over_many_builds`, `the_sweep_bounds_the_facts_over_many_builds`, … |
| C | the retention window collapses to zero | 11 | `the_window_must_be_the_trees_own_bound`, `the_window_covers_the_build_deadline_and_the_replay_window`, `a_fact_inside_the_window_still_bypasses_transit`, … |
| D | one install drops the expiry | 8 | `every_installed_fact_must_carry_an_expiry`, `a_fact_inside_the_window_still_bypasses_transit`, `a_live_fact_is_never_declined_early`, … |

### Four weaknesses found while building the controls

They are recorded because each one **would have made a control or a probe
pass vacuously**.

1. **The control detector ignored build failures.** The first run of the
   controls reported two of the four as *vacuous* — no test failed. Both
   had in fact broken the tree, but by **failing the build**, and the
   detector only counted `test … FAILED` lines. A control that breaks the
   *build* proves less than a control that breaks the *assertion*, so the
   detector now counts a build failure as a failure and the artefact was
   made robust: the simulated-build step is floored at one millisecond, so
   a collapsed window breaks the assertion rather than the compile.
2. **Control B was a compile failure, not a semantic one.** The first
   mutation renamed `retain` to a method that does not exist, so the tree
   did not build and no assertion was reached. It was rewritten to a
   mutation that compiles and is semantically wrong — `*expiry > 0`, which
   retains every fact — and it now fails eleven tests, the guard included.
3. **The sweep probe replaced only one of the three retains.** The rule
   asserts that the sweep body contains `retain(`, so a sweep that
   retained two of the three facts still satisfied it. The probe now
   replaces all three, **and rule 6 was strengthened** to require the
   clock comparison `> now_seconds` rather than merely the local's name.
4. **The install probe's anchor spanned a line rustfmt had wrapped.** The
   mutation silently did not apply. It failed loudly — the probe asserts
   the mutation applied — and was rewritten against a short, stable
   anchor.

## Invariant review

| Invariant | How it is held |
| --- | --- |
| The router's own traffic is never claimed as transit work | the expiry is checked at use time and the window is the coordinator's own identifier-retention bound; a fact is honoured for at least as long as the coordinator could recognise a reply |
| The error direction is safe | an expired fact is **declined** (benign); a missing fact would make the owner claim the router's own traffic (dangerous), and the window prevents that |
| No unbounded collection | the three sets are `BTreeMap`s and the sweep drops the expired entries; the bound is measured over 256 and 512 simulated builds |
| No new task, timer, listener or socket | the sweep rides the pump tick that already installs the facts; asserted by the guard |
| No lock is held across an await | the change adds no await and no lock |
| The coordinator's own fact set is untouched | the change is confined to the transit owner's copy |
| Plan 492's transit posture and relay accounting are unchanged | untouched |
| No dependency direction change | `scripts/check-dependency-direction.sh` passes; no new dependency |
| No capability advertisement | `specs/support.toml` — 100 surfaces, **0 advertised** |
| No production wire change | none — the facts never cross the wire |
| `i2pr-transport`, `i2pr-tunnel` and `i2pr-proto` are unchanged | the change is confined to the daemon |

## Failure review

| Failure | Behaviour |
| --- | --- |
| The pump tick does not run | no facts are installed and none are swept; the sets keep what they held. The window is not enforced without a tick, and the sets stay bounded by the previous window's worth of facts. |
| The sweep runs | expired facts are removed; live facts keep their expiry |
| Two ticks race | the owner is reached through a single `&mut` handle, so the install and the sweep are serialised by the borrow; no lock is added |
| The process restarts | the sets are empty again; nothing is persisted |
| A fact is installed twice | the second install **refreshes** the expiry; the fact's life is bounded by the window from the latest install |
| The clock jumps backwards | `saturating_add` is used everywhere; an expiry is never in the past because of an overflow |
| The clock jumps forwards | facts expire early, which is the safe direction, and the live count shrinks |
| The coordinator reports no facts | nothing is installed; the sweep still runs, so the sets still shrink |

**Nothing fails open.** The bypass is a *decline*, so a lost fact can only
ever make the router claim its own traffic — which the window prevents —
and never the other way round.

## Migration review

There is **no migration**:

- no configuration key changes;
- no on-disk format changes;
- the sets are in-memory only and start empty;
- the transit owner's behaviour is unchanged for every fact that is still
  inside the window, which is every fact the coordinator could still
  recognise;
- the three install methods gain a clock parameter. The three call sites
  in `crates/i2pr-daemon/tests/m11_transit_live_owner.rs` are updated.
  This is a **source-level** signature change inside the workspace, not a
  wire change.

## Security review

- **No new secret, key or token** is introduced, derived or persisted. The
  facts are the router's own build identifiers, which the router already
  holds.
- **No new identifier crosses a boundary.** The change is entirely
  internal.
- **The counts carry no peer, identifier or payload.** The counts accessor
  returns three lengths and nothing else.
- **The window is a shortening, not an extension.** A fact is honoured for
  no longer than the coordinator could recognise a reply. Nothing is
  retained longer than the protocol allows.
- **The sweep writes nothing to the network.** No I2NP message, no relay,
  no publication.
- **No new configuration key** means no new operator-controlled surface.
- **Nothing is advertised.** `router.version` stays `0.9.62`; `caps` and
  the published RouterInfo are unchanged; the support inventory's
  advertised count stays **zero**.
- **The decline direction is the safe one.** The change can only ever make
  the owner *decline* traffic it used to bypass, never the reverse.

## Documentation and operations

- `specs/support.toml` — one `experimental` surface,
  `transit.creator-correlation-retention`, with `advertised = false`. The
  inventory moves from **99 surfaces (93 experimental, 5 deferred, 1
  ready)** to **100 surfaces (94 experimental, 5 deferred, 1 ready)**;
  **0 advertised**.
- `specs/CONFORMANCE.md` — the Plan 508 section, with the explicit
  statement that the window is a **protocol decision made on the record**
  and that no stock-peer evidence exists.
- `docs/architecture/i2pr-daemon.md` — the `src/transit_owner.rs`,
  `src/exploratory_build.rs` and `src/lib.rs` module rows, the Plan 508
  section, and the test-file counts (108 → 112 files; 69 → 72 named).
- `plans/registry.md` and `plans/subsystems/network-participation-roadmap.md`
  — the Plan 508 rows, updated to `closed`.

**Operations.** The counts are readable through
`TransitLiveOwner::creator_correlation_counts()`. An operator watching the
transit participation metrics sees the facts bounded: the count rises
while builds are in flight and falls on the sweep, and it no longer grows
for ever.

## Limitations

- **The window is the coordinator's own bound, not a measured one.** It is
  derived from two constants the tree enforces, which is a *protocol*
  argument and not an empirical one. Whether an ordinary public router's
  own build replies still arrive inside a 46-minute window is
  **unproven**.
- **The sweep rides the pump tick.** If the tick stops, the sweep stops
  and the facts stop being reclaimed. The sets remain bounded by the
  previous window's worth of facts, so the growth is bounded even then,
  but it is not reclaimed.
- **The pending deadline is a coordinator-side bound.** A build attempt
  that stays pending longer than `BUILD_DEADLINE_MS` would make the fact
  expire before the coordinator gave up on it. The coordinator enforces
  the deadline, so this is bounded by the tree's own invariant.
- **The peer component of a build fact is not re-derived.** The expiry is
  per-fact, and a fact is identified by the peer hash and the message id
  exactly as before.
- **The counters are not persisted.** A restart empties the sets, which
  is the safe direction: the coordinator's own facts are rebuilt and the
  router simply declines for the first window.
- **Plan 487 F3's destination-inbound half** (which needs tunnel
  rekeying), Plan 497 limitation 5's **read** half, Plan 503's
  **lookup-target API-version gate**, Plan 491 **F2** and **F4**, Plan 490
  Medium #3 and every externally-blocked plan stay open.

## Findings by severity

### Medium

- **The daemon deep-dive's module-table line counts are stale for 33 of
  the 63 rows that carry one** (measured by comparing every row against
  the file). Plan 508 measured **36**, updated the **three** it changed,
  and left the rest: the drift is a separate concern with its own
  plan-of-record, and correcting thirty-three unrelated rows here would
  bury the change. **Not fixed.**

### Low

- **The control harness's detector ignored build failures**, so two of
  the four controls were first reported as vacuous when they had in fact
  broken the tree. The detector now counts a build failure as a failure.
  Recorded above.
- **Two of the guard's probes were initially wrong** in a way that would
  have made them pass vacuously — the sweep probe replaced only one of
  the three retains, and the install probe's anchor spanned a line rustfmt
  had wrapped. Both are recorded above and fixed, and rule 6 was
  strengthened because of the first.
- **The work packages ran out of order.** The feasibility artefact was
  written after the implementation, so it is the evidence rather than the
  gate the plan intended. The artefact still carries two sensitivity
  controls of its own, and it caught a real fragility.

### Recorded, not defects

- **The plan of record's production-change table did not list the counts
  accessor.** The correction lives in this record and in the registry
  row; the plan of record is not rewritten, and no closed record is
  rewritten.
- **The window is a protocol decision**, and Plan 502 deferred it. It is
  made here, on the record, with its reasoning.

## Roadmap disposition

`plans/subsystems/network-participation-roadmap.md` — the Plan 508 row
moves from `registered` to `closed`, with the closure record and the
commits, and §12 records the disposition.

Plan 508 discharges **Plan 502's Finding M1**, the last recorded finding
on the transit-owner's own structures. The network-participation line's
remaining items are the **externally-blocked** ones — Plan 493's
authorized two-host topology, Plan 494's floodfill advertisement, Plan
433's coordinator — and the **product decisions** that have no protocol
basis: Plan 503's lookup-target gate and Plan 487 F3's destination-inbound
half. None of them is reachable from the source side alone, and none is
attempted here.

## Unblock audit

Plan 508 unblocks **nothing that was blocked**, and it is honest about
that. It discharges one recorded finding of a closed plan (Plan 502's
M1) and closes a gap no plan had registered.

Every item it leaves open is listed in the table above with the reason it
stays open, and each of those reasons is **external or a separate
plan-of-record** — not a piece of this plan that was quietly skipped:

- the external-environment blocks (Plans 433, 435, 436, 437, 438, 439, 493,
  494) need an authorized two-host topology this repository does not have;
- the stock-peer questions need that same topology;
- Plan 503's lookup-target gate is a **product decision** with no protocol
  basis, and inventing a floor would shrink the router's reach without
  cause;
- Plan 487 F3's destination-inbound half needs tunnel rekeying, a new
  I2NP mechanism the tree does not implement at all.

The plan's own stop conditions were all held: no wire-format change, no
new dependency, no change to `i2pr-transport`, `i2pr-tunnel` or
`i2pr-proto`, no change to the coordinator's own fact set, no new
configuration key, no second task or timer, no new lock, and no change to
Plan 492's transit posture.

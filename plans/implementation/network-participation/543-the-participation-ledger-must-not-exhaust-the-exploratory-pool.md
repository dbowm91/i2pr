# Plan 543 — the participation ledger must not exhaust the exploratory pool

**Status:** registered. **Subsystem:** network-participation.
**Registered:** 2026-11-11. **Successor to:** Plan 542.

## 1. Objective

The router's exploratory build driver must keep having peers to build
**through**, once it has built through them.

`PartipationLedger` (sic — `ParticipationLedger`,
`crates/i2pr-netdb/src/selection.rs:172`) is the router's record of how often
each peer has been selected, and the selection **skips** any peer whose counter
has reached the cap:

```text
// crates/i2pr-netdb/src/selection.rs:292
if constraints.max_participation > 0
    && constraints.participation.get(key).copied().unwrap_or(0)
        >= constraints.max_participation
{
    continue;
}
```

The counter **saturates** at the cap
(`crates/i2pr-netdb/src/selection.rs:219`), and the ledger's only release is
eviction of the **oldest** entry when it is **full**
(`crates/i2pr-netdb/src/selection.rs:209`). The ledger is constructed with
`MAX_PARTICIPATION_TRACKED_PEERS = 4_096`
(`crates/i2pr-netdb/src/selection.rs:37`) and
`DEFAULT_MAX_PARTICIPATION = 8` (`crates/i2pr-netdb/src/selection.rs:40`).

The exploratory build driver's only live peer source is the peer-profile
store's version-gated selection (`crates/i2pr-daemon/src/exploratory_driver.rs:796`),
and the profile store is fed the **bounded 256-record prefix**
(`crates/i2pr-daemon/src/peer_reconcile.rs:528`, from
`crates/i2pr-daemon/src/peer_reconcile.rs:467`, bounded by
`crates/i2pr-daemon/src/bootstrap.rs:53`).

So the population is at most a few hundred peers and the ledger tracks four
thousand: **eviction never fires**. Every peer therefore reaches the cap after
eight selections, and the cap is **never** released. After at most
256 × 8 = 2 048 selections the selection is permanently empty, and the router
stops building exploratory tunnels for as long as it runs.

The plan gives the ledger a **bounded, time-based release**, so a peer's
participation is a fact about a **window** rather than a fact about all time.
It adds no configuration key, no task, no timer and no listener.

## 2. Why ready

**The tree already has the pattern.** Plan 508 solved exactly this shape for a
different set: the transit owner's creator-correlation sets were insert-only and
grew for ever, and Plan 508 made the facts expire on a window **derived from the
tree's own constants**, checked **at use time**, and reclaimed by a sweep on a
tick that already existed. That plan's closure record is the precedent: a
participation fact is a fact about a window, the window is derived rather than
configured, and the expiry is a `const fn` so it cannot drift.

**The ledger is already bounded.** It evicts the oldest entry when full
(`crates/i2pr-netdb/src/selection.rs:209`), so the only missing property is that
a **saturated** entry is released when it is old. The plan does not add a bound;
it makes the existing bound able to fire.

**The release is a check, not a sweep.** The selection already reads the
ledger's counts and compares each against the cap
(`crates/i2pr-netdb/src/selection.rs:292`). If the counts carry a timestamp,
the same comparison can treat an **old** entry as zero — no new pass, no new
tick, no new task.

**No predecessor's guard needs an extension.** Plan 530's, Plan 531's, Plan
532's, Plan 533's, Plan 534's, Plan 535's, Plan 538's, Plan 539's, Plan 540's,
Plan 541's and Plan 542's guards pin their own surfaces; the ledger's entry
shape is not among them. Plan 505's guard pins the exploratory driver's
selection **call site** (`profiles.select_tunnel_build_peers(1, seed)`), which
this plan does not change.

## 3. Current implementation evidence

### 3.1 The ledger saturates and never releases

```text
// crates/i2pr-netdb/src/selection.rs:204
pub fn record(&mut self, key: &RouterHash) {
    if self.max_tracked_peers == 0 {
        return;
    }
    if !self.counts.contains_key(key) {
        if self.order.len() >= self.max_tracked_peers
            && let Some(oldest) = self.order.first().copied()
        {
            self.order.remove(0);
            self.counts.remove(&oldest);
        }
        self.order.push(*key);
        self.counts.insert(*key, 0);
    }
    let entry = self.counts.entry(*key).or_insert(0);
    *entry = entry.saturating_add(1).min(self.max_participation);
}
```

`*entry` is `.min(self.max_participation)`, so a counter can never rise above
the cap — and nothing ever lowers it. `counts` is a `BTreeMap<RouterHash, u32>`
with **no timestamp**, so the ledger holds no information from which an old
entry could be recognised.

`order` is the eviction order, and `record` only removes from it at the front,
when `order.len() >= max_tracked_peers`. With four thousand tracked peers and a
population of a few hundred, that comparison is never true.

### 3.2 The selection treats the cap as a permanent exclusion

```text
// crates/i2pr-netdb/src/selection.rs:291
if constraints.max_participation > 0
    && constraints.participation.get(key).copied().unwrap_or(0)
        >= constraints.max_participation
{
    continue;
}
```

The skip is unconditional: there is no "unless the peer has become eligible
again" arm, and `constraints.participation` is the caller's snapshot of the
counts.

### 3.3 The oracle is the only live peer source for the exploratory driver

```text
// crates/i2pr-daemon/src/exploratory_driver.rs:794
let Some(selection) = self.profiles.with(|profiles| {
    profiles.advance_time(now_ms);
    profiles.select_tunnel_build_peers(1, seed)
}) else {
```

`select_tunnel_build_peers` → `select_version_gated` →
`select_candidates_with_report` — the same skip rule. There is no fallback
population and no second selection path, so an empty selection is an empty
driver.

### 3.4 The population is bounded and does not churn

The profile store is fed by `learn_all(records.iter().cloned())`
(`crates/i2pr-daemon/src/peer_reconcile.rs:528`) from the reconcile's
`fn snapshot` (`crates/i2pr-daemon/src/peer_reconcile.rs:467`), which is the
group provider's bounded prefix — `MAX_GROUP_PROVIDER_ROUTER_INFOS = 256`
(`crates/i2pr-daemon/src/bootstrap.rs:53`). The prefix is stable for a stable
store, so the population neither grows nor turns over, and no peer is ever
dropped from the ledger.

### 3.5 What the plan does not claim

- It does **not** claim the profile store's feed should become the advertisers.
  That is Plan 541's class applied to a different consumer, and it has its own
  owner. This plan's release works whatever the feed is.
- It does **not** claim the exploratory posture is usable today. The dial scope
  is loopback and RFC1918, and a public deployment's peers are not dialable.
  That is the transport line's and Plan 433's business.
- It does **not** claim a stock-peer measurement.

## 4. Invariants

1. **A saturated count is released by time, not by luck.** A peer whose last
   selection is older than the window counts as zero for the selection's skip
   rule.
2. **The window is derived, not configured.** It comes from the tree's own
   constants through a `const fn`, so no configuration key is added and no
   second source of truth exists.
3. **The release is a check at use time.** The selection's comparison consults
   the window; no new pass, tick, task or timer is added.
4. **The ledger stays bounded.** Its entry count and its counter values keep
   their existing bounds.
5. **The skip rule keeps its meaning.** A peer selected recently is still
   skipped; the plan changes *when* an entry is stale, never whether a fresh
   entry excludes a peer.
6. **The selection's order and its other gates are unchanged.**
7. **No peer is blamed.** The ledger is the router's own record; nothing is
   written into a peer's profile.
8. **No advertisement.** `router.version` stays `0.9.62` and the inventory's
   advertised count stays **zero**.
9. **Plan 505's, Plan 530's, Plan 531's, Plan 532's, Plan 533's, Plan 534's,
   Plan 535's, Plan 538's, Plan 539's, Plan 540's, Plan 541's and Plan 542's
   lanes and guards stay byte-identical.**

## 5. Scope

### 5.1 In scope

- `crates/i2pr-netdb/src/selection.rs` — the ledger's entry shape and the
  selection's skip rule.
- `crates/i2pr-daemon/src/netdb_participation.rs` — the driver's clock handoff
  to the ledger, if the ledger needs it at a different point.

### 5.2 Out of scope

- The profile store's feed and its bounded prefix.
- The exploratory driver's posture, its dial scope and its outcome map.
- The version gate and the destination coordinator's ledger.
- The ledger's size bound and the counter cap.

### 5.3 Out of scope, with the owner named

- **The peer-profile store's feed is still the bounded prefix.**
  **Owner:** the successor that gives the router a NetDB owner of its own.
- **The exploratory posture is not dialable against a public deployment.**
  **Owner:** the transport line and Plan 433's blocked composition.
- **The router's validated store is still the bootstrap's store.** Plan 536's
  recorded **Low** finding 1. **Owner:** the same successor.
- **The production SSU2 peer-test driver** — Plan 530's Low finding 2.
- **The `B` advertisement and `caps=f`** — their own owners.
- **The stock-peer half** — Plan 493's authorized two-host topology.

## 6. Required production changes

### 6.1 `crates/i2pr-netdb/src/selection.rs`

1. The ledger's entry becomes a `(u32 count, u64 last_ms)` pair, so an entry
   carries the instant of its last selection. The map stays bounded and the
   counter keeps saturating.
2. `record` takes the caller's instant and stamps the entry.
3. A new reading, `count_at(key, now_ms)`, returns the counter **only** when the
   entry is inside the window, and `0` otherwise. `is_capped` and the
   selection's skip rule both read through it.
4. The window is a `const` derived from the tree's own constants through a
   `const fn`, so it cannot be configured and cannot drift.

### 6.2 The selection's skip rule

5. `SelectionConstraints::participation` becomes a reading rather than a borrow
   of the raw map, so the skip rule cannot accidentally consult a stale entry.
   The existing `counts()` accessor keeps its signature for the callers that
   only need the map's shape.

## 7. Design decisions

### 7.1 Release by time, not by population

The population does not churn, so eviction cannot be the release. A window is
the only release that works whatever the feed is, and it is the property the
question actually needs: *has this peer been used a lot recently?* not *has this
peer ever been used a lot?*

### 7.2 The window is derived through a `const fn`

Plan 507 and Plan 508 both establish the rule: a window that a second caller
could restate is a second source of truth. The window is a `const` derived from
constants the tree already has, so it cannot be configured and a change to its
inputs is a compile-time change.

### 7.3 The release is checked at use time

The selection already walks the counts to apply the skip rule. A check there
costs one comparison per candidate and needs no new pass. Plan 508's precedent
*also* reclaims memory with a sweep; this plan does not, because the ledger's
size bound already reclaims memory, and adding a sweep would need a tick that
does not exist for the ledger.

### 7.4 The counter cap is unchanged

The cap is the router's participation policy — it says *do not over-use one
peer* — and this plan does not touch it. It changes only how long an over-used
peer stays over-used.

### 7.5 The lane proves the release; the guard proves the shape

The lane drives the **real** selection with a ledger whose entries are old and
shows that the selection is non-empty, and with the same entries fresh and
shows that it is empty. The guard proves that the ledger carries a timestamp,
that the skip rule reads through the windowed reading, and that the window is
derived rather than restated.

## 8. Work packages

1. **WP1 — the entry.** The ledger's timestamped entry and its saturation.
2. **WP2 — the reading.** `count_at`, and the skip rule reading through it.
3. **WP3 — the window.** The derived `const fn`.
4. **WP4 — the lane.** The release observed through the real selection.
5. **WP5 — the guard.** The shape, with an inverted probe per rule.
6. **WP6 — the negative controls.**
7. **WP7 — the documentation and the inventory.**
8. **WP8 — the floor and the closure record.**

## 9. Failure, cancellation, restart and contention semantics

**Failure.** The change adds no failure mode. A ledger with no entry for a peer
reads zero, exactly as it does today.

**Cancellation.** Nothing async changes. The ledger is synchronous and
`i2pr-netdb` stays runtime-neutral.

**Restart.** The ledger is in memory and is rebuilt with the driver, so a
restart clears every count — which is the behaviour the plan wants.

**Contention.** None: the ledger is owned by the driver behind the existing
handle, and it takes no lock this plan adds.

## 10. Compatibility and migration

**No wire change.**

**No configuration migration.** **No key is added.**

**No persisted state.** The ledger is not persisted.

**No change to the netdb crate's public surface** beyond the ledger's entry
shape, which is private to the ledger; `counts()` keeps its signature.

**No change to the manifest or the lock.**

## 11. Required tests

### 11.1 The lane

`crates/i2pr-daemon/tests/participation_release.rs` — the **real** selection
over a **real** profile store and a **real** ledger. The lane must prove:

1. a peer whose count is at the cap and whose last selection is **inside** the
   window is **skipped**;
2. the same peer, with the same count and a last selection **outside** the
   window, **is selected**;
3. the pool is **not** permanently exhausted: after every peer reaches the cap,
   advancing the clock past the window restores a non-empty selection;
4. a peer selected again after its entry went stale is recorded with the
   stamped instant, so the window restarts;
5. the counter still saturates at the cap;
6. the ledger's entry count still respects its bound.

The lane fixes its clock explicitly and never reads the wall clock.

### 11.2 The guard

`crates/i2pr-daemon/tests/participation_release_boundary.rs` — a static guard
that is the authority for the **shape**. Rules, each with an inverted probe:

1. the ledger's entry carries an instant, and `record` stamps it;
2. the skip rule reads through the **windowed** reading, not the raw map;
3. the window is a `const` derived through a `const fn`, and it is **not**
   restated anywhere;
4. the counter's saturation and the entry bound are unchanged;
5. the selection's order, its version gate and its tier gate are unchanged;
6. **no** configuration key, task, timer, listener or socket is added, and
   nothing is advertised;
7. Plan 505's, Plan 530's, Plan 531's, Plan 532's, Plan 533's, Plan 534's,
   Plan 535's, Plan 538's, Plan 539's, Plan 540's, Plan 541's and Plan 542's
   lanes and guards stay **byte-identical**.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-netdb --all-targets
cargo test --locked -p i2pr-daemon --test participation_release -- --test-threads=1
cargo test --locked -p i2pr-daemon --test participation_release_boundary -- --test-threads=1
python3 scripts/check-tooling-inventory.py
python3 scripts/check-global-plan-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
```

plus the routine floor in `AGENTS.md`, with this plan's two steps added.

## 13. Documentation updates

- `specs/support.toml` — a new experimental, non-advertised surface
  `netdb.participation-release`.
- `specs/CONFORMANCE.md` — the count paragraph and a section.
- `specs/QUALIFICATION-GATES.md` — the gates this plan's lane and guard
  discharge.
- `docs/architecture/i2pr-netdb.md` — the ledger's windowed reading.
- `docs/architecture/i2pr-daemon.md` — what the driver's selection now yields.
- The plan's own amendment log.

## 14. Acceptance criteria and stop conditions

The plan is **accepted** when:

1. a peer whose entry is outside the window is selectable again, and one inside
   it is still skipped;
2. a pool in which every peer is at the cap becomes non-empty again once the
   clock passes the window;
3. the counter's saturation and the ledger's entry bound are unchanged;
4. the window has one definition and no configuration key is added;
5. no task, timer, listener or socket is added, and `Cargo.toml` and
   `Cargo.lock` stay byte-identical;
6. `router.version` stays `0.9.62`, and the inventory's advertised count stays
   **zero**;
7. the full floor is green, with the closure record's table complete;
8. the guard's inverted probes each fail as intended and the tree is restored
   byte-identically after each.

**Stop conditions.** The plan stops and reports if:

- the change would make a peer selectable **while** it is inside the window —
  that would defeat the participation policy, which is not this plan's;
- the fix would need a configuration key — the window is derived;
- the fix would need a new tick or task — the release is a check at use time;
- any predecessor's guard would have to change;
- the floor's `cargo deny check advisories bans sources` fails;
- a stock peer would be required.

## 15. Closure evidence required

The implementation commit, the lane-and-guard commit, the documentation commit
and the closure commit; the complete floor table; the lane's rows and the
guard's rows with their results; the negative controls with the SHA-256 proof
that the tree was restored byte-identically; the requirement-to-evidence
matrix; the invariant, failure, migration and security reviews; the
limitations, the findings by severity, the roadmap disposition and the unblock
audit.

## 16. Handoff notes

- The plan is **experimental** and default-off. Do not describe the change as
  released, supported or advertised.
- The delivery is confined to the controlled loopback profile.
- The defect is **permanent exhaustion**, not slowness: do not "fix" it by
  raising the cap or by shrinking the ledger. A higher cap only moves the
  exhaustion later, and a smaller ledger evicts peers the policy never intended
  to release.
- Do not feed the profile store from the advertiser set to fix this. That is a
  different plan with a different owner, and this release works whatever the
  feed is.
- The predecessor closure records are **not** rewritten.
- The plan **extends** no other plan's guard.
- A successor must **not** restate the window, and must **not** make the skip
  rule read the raw map again: a stale entry that is never released is a
  permanent pool exhaustion, and the windowed reading is what prevents it.

## 17. Amendment log

### 17.1 — 2026-11-11, registration

Plan 543 is registered as the successor to Plan 542 in the router's NetDB line:
the participation ledger saturates at its cap and is never released, the profile
store's population is a bounded prefix that does not churn, and the ledger's
only release — eviction of the oldest entry when it is full — therefore never
fires. After at most `MAX_GROUP_PROVIDER_ROUTER_INFOS × DEFAULT_MAX_PARTICIPATION`
selections the exploratory build driver's selection is permanently empty.

The scope is the **release**, not the cap and not the feed. The plan gives the
ledger's entry an instant, makes the skip rule read through a windowed reading,
and derives the window through a `const fn` so it cannot be configured or
restated.

The plan **extends** no other plan's guard: Plan 505's, Plan 530's, Plan 531's,
Plan 532's, Plan 533's, Plan 534's, Plan 535's, Plan 538's, Plan 539's, Plan
540's, Plan 541's and Plan 542's lanes and guards stay byte-identical.

The plan adds no configuration key, no task, no timer, no listener and no
socket, it adds no dependency, it leaves `Cargo.toml` and `Cargo.lock`
byte-identical, and it advertises nothing.

No closed closure record is rewritten by this plan.

### 17.2 — 2026-11-11, the implementation's own findings

| # | What the implementation found | What this plan does about it |
| --- | --- | --- |
| 17.2.1 | §6.2 says the selection's skip rule should read through a **windowed reading**, which would have changed `SelectionConstraints`' field type and rippled through every caller. | The release is instead a **reclaim on the clock the driver already advances**, so the skip rule and the public `counts()` view are byte-identical and no caller moves. This is Plan 508's shape for a fact set that must not grow for ever. §6 is not rewritten; this row is the correction. |
| 17.2.2 | `ParticipationLedger::record` had no clock, so the stamp needed a parameter and three call sites moved: two in the driver (which owns `clock_ms`) and one in the destination coordinator (which owns `now_ms`). | All three pass their own clock, and the guard requires **every** recording site to stamp the driver's clock, so a site that stops stamping is a guard failure rather than a silent stale entry. |
| 17.2.3 | The lane's first draft asserted that **one** selection after the release re-caps a peer. It does not: the counter restarts at one, so the pool stays selectable. | The row now drives the pool to the cap **again** at the new instant, asserts the cap re-armed, and then asserts the window restarts from that instant. |
| 17.2.4 | The guard's first draft forbade `tokio::time::interval` in the driver, but the driver **legitimately** owns cadence loops. | The rule forbids a **new** ownerless task and the socket constructs instead, and its probe mutates `reclaim` to add a `tokio::spawn`. |
| 17.2.5 | A control that removed the ledger's `stamps` field did not compile, so it proved nothing about the rule it named. | The control breaks the rule's **eviction clause** instead — eviction stops releasing the stamp with the counter — which compiles and fires the rule. A control that breaks the build proves nothing about the row it names. |
| 17.2.6 | A control that removed `self.stamps.remove(key)` from `reclaim` did **not** fire the "window restarts" row, because `record` overwrites the stamp on re-selection. | The control targets the **exactness** row, which asserts `last_recorded_at` is `None` for a released peer — the one observable consequence of the leftover stamp. |
